//! Unified llama.cpp FFI wrapper for desktop (Linux/macOS/Windows) and Android.
//!
//! This module provides a safe, async-friendly interface to llama.cpp inference,
//! with identical functionality on desktop and Android. Platform differences
//! (bindings source, model library paths) are handled via conditional compilation.
//!
//! See ARCHITECTURE.md for how the inference engine fits into the message-native UI.

/// Platform-specific bindings to llama.cpp.
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

/// Android uses pre-generated bindings because bindgen doesn't work in cross-compile.
#[cfg(target_os = "android")]
pub mod bindings {
    include!("bindings.rs");
}

use self::bindings::*;
use std::ffi::CString;
use std::path::Path;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};

static LOG_SUPPRESSED: AtomicBool = AtomicBool::new(false);
static ABORT: AtomicBool = AtomicBool::new(false);

pub fn request_stop() {
    ABORT.store(true, Ordering::SeqCst);
}

pub fn is_stop_requested() -> bool {
    ABORT.load(Ordering::SeqCst)
}

fn clear_stop() {
    ABORT.store(false, Ordering::SeqCst);
}

/// Silence llama.cpp logs (call once at startup).
pub fn suppress_llama_logging() {
    if LOG_SUPPRESSED.swap(true, Ordering::SeqCst) {
        return;
    }
    unsafe {
        llama_log_set(Some(silent_log_callback), std::ptr::null_mut());
    }
}

unsafe extern "C" fn silent_log_callback(
    _level: ggml_log_level,
    _text: *const std::os::raw::c_char,
    _user_data: *mut std::os::raw::c_void,
) {
}

pub const GGML_TYPE_TQ1_0: u32 = 34;
pub const GGML_TYPE_TQ2_0: u32 = 35;
pub const GGML_TYPE_Q1_0: u32 = 41;
pub const GGML_TYPE_Q2_0: u32 = 42;
pub const GGML_TYPE_Q4_0: u32 = 2;
pub const GGML_TYPE_Q8_0: u32 = 8;

#[derive(Debug, Clone)]
pub struct Config {
    pub n_ctx: u32,
    pub n_gpu_layers: u32,
    pub n_threads: i32,
    pub n_batch: u32,
    pub use_mmap: bool,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: i32,
    pub min_p: f32,
    pub repeat_penalty: f32,
    pub type_k: u32,
    pub type_v: u32,
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    ThinkingStart,
    ThinkingEnd,
    ToolCallBegin,
    ToolCallEnd,
    Done(String),
    Error(String),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            n_ctx: 2048,
            n_gpu_layers: 99,
            n_threads: 8,
            n_batch: 512,
            use_mmap: true,
            temperature: 0.7,
            top_p: 0.9,
            top_k: 40,
            min_p: 0.05,
            repeat_penalty: 1.5,
            type_k: GGML_TYPE_Q8_0,
            type_v: GGML_TYPE_Q8_0,
        }
    }
}

unsafe impl Send for Engine {}

pub struct Engine {
    model: *mut llama_model,
    ctx: *mut llama_context,
    sampler: *mut llama_sampler,
    config: Config,
    n_tokens: i32,
    cached_prompt_tokens: Vec<llama_token>,
    cached_segments: Vec<String>,
}

impl Engine {
    pub fn load<P: AsRef<Path>>(path: P, config: Config) -> Result<Self> {
        let path = path.as_ref();
        let path_cstr = CString::new(path.to_string_lossy().as_bytes())?;
        suppress_llama_logging();
        unsafe { llama_backend_init() }
        let mut params = unsafe { llama_model_default_params() };
        params.n_gpu_layers = config.n_gpu_layers as i32;
        params.use_mmap = config.use_mmap;
        let model = unsafe { llama_model_load_from_file(path_cstr.as_ptr(), params) };
        if model.is_null() {
            anyhow::bail!("Failed to load model: {}", path.display());
        }
        let mut ctx_params = unsafe { llama_context_default_params() };
        ctx_params.n_ctx = config.n_ctx;
        ctx_params.n_batch = config.n_batch;
        ctx_params.n_ubatch = config.n_batch.min(512);
        ctx_params.n_threads = config.n_threads;
        ctx_params.n_threads_batch = config.n_threads;
        ctx_params.type_k = config.type_k;
        ctx_params.type_v = config.type_v;
        ctx_params.flash_attn_type = if config.type_k != 1 || config.type_v != 1 { 1 } else { 0 };
        let mut ctx = unsafe { llama_init_from_model(model, ctx_params) };
        if ctx.is_null() {
            eprintln!(
                "KV type k={} v={} with flash_attn failed, trying without flash_attn + F16",
                config.type_k, config.type_v
            );
            ctx_params.type_k = 1;
            ctx_params.type_v = 1;
            ctx_params.flash_attn_type = 0;
            ctx = unsafe { llama_init_from_model(model, ctx_params) };
        }
        let ctx = ctx;
        if ctx.is_null() {
            unsafe { llama_model_free(model) };
            anyhow::bail!("Failed to create context");
        }
        let sampler = unsafe { llama_sampler_chain_init(llama_sampler_chain_default_params()) };
        if sampler.is_null() {
            unsafe {
                llama_free(ctx);
                llama_model_free(model);
            }
            anyhow::bail!("Failed to create sampler chain");
        }
        let rp = unsafe {
            llama_sampler_init_penalties(
                -1,
                config.repeat_penalty,
                0.5,
                1.5,
            )
        };
        if !rp.is_null() {
            unsafe { llama_sampler_chain_add(sampler, rp); }
        }
        let vocab = unsafe { llama_model_get_vocab(model) };
        let dry = unsafe {
            llama_sampler_init_dry(
                vocab,
                2048,
                1.0,
                1.75,
                4,
                -1,
                std::ptr::null_mut(),
                0,
            )
        };
        if !dry.is_null() {
            unsafe { llama_sampler_chain_add(sampler, dry); }
        }
        let t = unsafe { llama_sampler_init_temp(config.temperature) };
        if !t.is_null() {
            unsafe { llama_sampler_chain_add(sampler, t); }
        }
        let t = unsafe { llama_sampler_init_top_k(config.top_k) };
        if !t.is_null() {
            unsafe { llama_sampler_chain_add(sampler, t); }
        }
        let mp = unsafe { llama_sampler_init_min_p(config.min_p, 1) };
        if !mp.is_null() {
            unsafe { llama_sampler_chain_add(sampler, mp); }
        }
        let t = unsafe { llama_sampler_init_top_p(config.top_p, 1) };
        if !t.is_null() {
            unsafe { llama_sampler_chain_add(sampler, t); }
        }
        let dist = unsafe { llama_sampler_init_dist(42) };
        if !dist.is_null() {
            unsafe { llama_sampler_chain_add(sampler, dist); }
        }
        Ok(Self {
            model,
            ctx,
            sampler,
            config,
            n_tokens: 0,
            cached_prompt_tokens: Vec::new(),
            cached_segments: Vec::new(),
        })
    }

    pub fn reset(&mut self) -> Result<()> {
        unsafe {
            let mem = llama_get_memory(self.ctx);
            llama_memory_seq_rm(mem, 0, 0, -1);
            llama_sampler_reset(self.sampler);
        }
        self.n_tokens = 0;
        self.cached_prompt_tokens.clear();
        self.cached_segments.clear();
        Ok(())
    }

    pub fn clear_kv(&mut self) -> Result<()> {
        self.reset()
    }

    fn n_ctx_used(&self) -> u32 {
        unsafe { llama_n_ctx(self.ctx) }
    }

    fn tokenize(&self, text: &str) -> Result<Vec<llama_token>> {
        let cstr = CString::new(text)?;
        let mut tokens = vec![0i32; text.len() * 2 + 64];
        let vocab = unsafe { llama_model_get_vocab(self.model) };
        let n = unsafe {
            llama_tokenize(
                vocab,
                cstr.as_ptr(),
                text.len() as i32,
                tokens.as_mut_ptr(),
                tokens.len() as i32,
                false,
                true,
            )
        };
        if n < 0 {
            anyhow::bail!("Tokenization failed");
        }
        tokens.truncate(n as usize);
        Ok(tokens)
    }

    fn decode_batch(&mut self, tokens: &[llama_token]) -> Result<()> {
        if tokens.is_empty() {
            return Ok(());
        }
        let batch_size = self.config.n_batch as usize;
        for chunk in tokens.chunks(batch_size) {
            let mut chunk_mut = chunk.to_vec();
            let batch = unsafe { llama_batch_get_one(chunk_mut.as_mut_ptr(), chunk_mut.len() as i32) };
            if unsafe { llama_decode(self.ctx, batch) } != 0 {
                anyhow::bail!("Decode failed (context overflow?)");
            }
            self.n_tokens += chunk.len() as i32;
        }
        Ok(())
    }

    pub fn infer_stream_segments<F>(&mut self, segments: &[String], mut f: F) -> Result<()>
    where
        F: FnMut(StreamEvent) -> Result<(), ()>,
    {
        clear_stop();

        let n_ctx = self.n_ctx_used() as i32;

        // Find the first segment that differs from the cached segments.
        let first_diff = segments.iter()
            .zip(self.cached_segments.iter())
            .position(|(a, b)| a != b)
            .unwrap_or(self.cached_segments.len().min(segments.len()));

        // Tokenize the full prompt as one string (correct BPE boundaries).
        let full_text: String = segments.concat();
        let full_tokens = self.tokenize(&full_text)?;

        // If the first N segments are unchanged, tokenize just those N segments
        // to find how many tokens they produce. We can reuse those cached tokens.
        let can_incremental = if first_diff > 0 && !self.cached_segments.is_empty() && first_diff <= self.cached_segments.len() {
            // Tokenize the unchanged prefix
            let prefix_text: String = segments[..first_diff].concat();
            let prefix_tokens = self.tokenize(&prefix_text)?;
            // Verify the prefix tokens match what we have cached
            if prefix_tokens.len() <= self.cached_prompt_tokens.len() {
                let matches = prefix_tokens.iter()
                    .zip(self.cached_prompt_tokens.iter())
                    .all(|(a, b)| a == b);
                if matches {
                    // eprintln!("DEBUG incremental: first_diff={}, prefix_tokens={} match cached", first_diff, prefix_tokens.len());
                    Some(prefix_tokens.len())
                } else {
                    // eprintln!("DEBUG incremental: first_diff={}, prefix_tokens={} DO NOT match cached", first_diff, prefix_tokens.len());
                    None
                }
            } else {
                // eprintln!("DEBUG incremental: prefix_tokens={} > cached_tokens={}", prefix_tokens.len(), self.cached_prompt_tokens.len());
                None
            }
        } else {
            None
        };

        // eprintln!("DEBUG incremental: segments={} cached_segs={} first_diff={} reusable={:?} cached_tokens={}",
        //     segments.len(), self.cached_segments.len(), first_diff, can_incremental, self.cached_prompt_tokens.len());

        if let Some(reuse_count) = can_incremental {
            // We can reuse `reuse_count` tokens from the KV cache.
            // First, evict any generated tokens + everything after the reuse point.
            if self.n_tokens > reuse_count as i32 {
                unsafe {
                    let mem = llama_get_memory(self.ctx);
                    llama_memory_seq_rm(mem, 0, reuse_count as i32, -1);
                }
                self.n_tokens = reuse_count as i32;
            }
            // Decode only the suffix tokens (from reuse_count onward in the full token list)
            let suffix = &full_tokens[reuse_count..];
            let available = n_ctx - self.n_tokens;
            if suffix.len() <= available as usize {
                // eprintln!("DEBUG incremental: decoding {} suffix tokens (reuse {} cached)", suffix.len(), reuse_count);
                self.decode_batch(suffix)?;
            } else {
                self.reset()?;
                let max_prompt_tokens = (n_ctx - 1) as usize;
                let tokens = if full_tokens.len() > max_prompt_tokens {
                    let skip = full_tokens.len() - max_prompt_tokens;
                    eprintln!("WARNING: prompt truncated {} -> {} tokens", full_tokens.len(), max_prompt_tokens);
                    &full_tokens[skip..]
                } else {
                    &full_tokens[..]
                };
                self.decode_batch(tokens)?;
            }
        } else {
            self.reset()?;
            let max_prompt_tokens = (n_ctx - 1) as usize;
            let tokens = if full_tokens.len() > max_prompt_tokens {
                let skip = full_tokens.len() - max_prompt_tokens;
                eprintln!("WARNING: prompt truncated {} -> {} tokens", full_tokens.len(), max_prompt_tokens);
                &full_tokens[skip..]
            } else {
                &full_tokens[..]
            };
            self.decode_batch(tokens)?;
        }

        self.cached_prompt_tokens = full_tokens.clone();
        self.cached_segments = segments.to_vec();

        unsafe { llama_sampler_reset(self.sampler); }

        let vocab = unsafe { llama_model_get_vocab(self.model) };
        let eos = unsafe { llama_token_eos(vocab) };
        let mut output = String::new();
        let mut in_thinking = false;
        let mut in_tool_call = false;
        let think_open_id: i32 = 151667;
        let think_close_id: i32 = 151668;
        let tool_call_begin_id: i32 = 151657;
        let tool_call_end_id: i32 = 151658;
        let max_new = (n_ctx - self.n_tokens).min(2048).max(1);

        for _ in 0..max_new {
            if is_stop_requested() { break; }
            let token = unsafe { llama_sampler_sample(self.sampler, self.ctx, -1) };
            if token == eos { break; }
            if token == tool_call_begin_id {
                if in_thinking {
                    in_thinking = false;
                    let _ = f(StreamEvent::ThinkingEnd);
                }
                unsafe { llama_sampler_accept(self.sampler, token); }
                let mut t = token;
                let batch = unsafe { llama_batch_get_one(&mut t, 1) };
                if unsafe { llama_decode(self.ctx, batch) } != 0 { break; }
                self.n_tokens += 1;
                if !in_tool_call {
                    in_tool_call = true;
                    output.push_str("<|tool_call_begin|>\n");
                    let _ = f(StreamEvent::ToolCallBegin);
                }
                continue;
            }
            if token == tool_call_end_id {
                if in_thinking {
                    in_thinking = false;
                    let _ = f(StreamEvent::ThinkingEnd);
                }
                unsafe { llama_sampler_accept(self.sampler, token); }
                let mut t = token;
                let batch = unsafe { llama_batch_get_one(&mut t, 1) };
                if unsafe { llama_decode(self.ctx, batch) } != 0 { break; }
                self.n_tokens += 1;
                if in_tool_call {
                    output.push_str("\n<|tool_call_end|>");
                    let _ = f(StreamEvent::ToolCallEnd);
                }
                in_tool_call = false;
                break;
            }
            if token == think_open_id {
                in_thinking = true;
                if f(StreamEvent::ThinkingStart).is_err() { break; }
                unsafe { llama_sampler_accept(self.sampler, token); }
                let mut t = token;
                let batch = unsafe { llama_batch_get_one(&mut t, 1) };
                if unsafe { llama_decode(self.ctx, batch) } != 0 { break; }
                self.n_tokens += 1;
                continue;
            }
            if token == think_close_id {
                in_thinking = false;
                if f(StreamEvent::ThinkingEnd).is_err() { break; }
                unsafe { llama_sampler_accept(self.sampler, token); }
                let mut t = token;
                let batch = unsafe { llama_batch_get_one(&mut t, 1) };
                if unsafe { llama_decode(self.ctx, batch) } != 0 { break; }
                self.n_tokens += 1;
                continue;
            }
            let mut buf = [0u8; 32];
            let n = unsafe {
                llama_detokenize(
                    vocab,
                    &token,
                    1,
                    buf.as_mut_ptr() as *mut _,
                    buf.len() as i32,
                    false,
                    true,
                )
            };
            if n > 0 {
                if let Ok(s) = std::str::from_utf8(&buf[..n as usize]) {
                    if in_thinking {
                        if f(StreamEvent::Token(s.to_string())).is_err() { break; }
                    } else {
                        output.push_str(s);
                        if f(StreamEvent::Token(s.to_string())).is_err() { break; }
                    }
                }
            }
            unsafe { llama_sampler_accept(self.sampler, token); }
            let mut t = token;
            let batch = unsafe { llama_batch_get_one(&mut t, 1) };
            if unsafe { llama_decode(self.ctx, batch) } != 0 { break; }
            self.n_tokens += 1;
        }
        if in_thinking {
            let _ = f(StreamEvent::ThinkingEnd);
        }

        // After generation, evict generated tokens from KV cache so the next
        // turn can do prefix matching against the prompt-only token sequence.
        let prompt_len = self.cached_prompt_tokens.len() as i32;
        if self.n_tokens > prompt_len {
            unsafe {
                let mem = llama_get_memory(self.ctx);
                llama_memory_seq_rm(mem, 0, prompt_len, -1);
            }
            self.n_tokens = prompt_len;
        }

        let _ = f(StreamEvent::Done(output));
        Ok(())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            if !self.sampler.is_null() {
                llama_sampler_free(self.sampler);
            }
            if !self.ctx.is_null() {
                llama_free(self.ctx);
            }
            if !self.model.is_null() {
                llama_model_free(self.model);
            }
        }
    }
}

// ─── Command protocol ─────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum Command {
    Load(String, Config, tokio::sync::oneshot::Sender<Result<()>>),
    InferStream(
        Vec<String>,
        tokio::sync::mpsc::UnboundedSender<StreamEvent>,
        tokio::sync::oneshot::Sender<Result<()>>,
    ),
    Unload(tokio::sync::oneshot::Sender<Result<()>>),
    ClearKv(tokio::sync::oneshot::Sender<Result<()>>),
}

/// Spawn a persistent std::thread that owns the engine.
/// Uses std::sync::mpsc channel (Send + Sync via tokio oneshot for responses).
/// Returns Arc<Mutex<Option<std::mpsc::Sender<Command>>>> so any thread can send commands.
pub fn spawn_inference_thread(
) -> std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let shared_tx: std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));

    std::thread::spawn(move || {
        let mut engine_ptr: *mut Option<Engine> = Box::into_raw(Box::new(None));
        loop {
            match rx.try_recv() {
                Ok(Command::Load(path, config, resp)) => {
                    unsafe { *engine_ptr = None; }
                    match Engine::load(&path, config) {
                        Ok(e) => {
                            unsafe { *engine_ptr = Some(e); }
                            let _ = resp.send(Ok(()));
                        }
                        Err(e) => {
                            let _ = resp.send(Err(e));
                        }
                    }
                }
                Ok(Command::InferStream(segments, event_tx, resp)) => {
                    if let Some(ref mut e) = unsafe { (*engine_ptr).as_mut() } {
                        let tx = event_tx.clone();
                        let cb = |event: StreamEvent| {
                            if tx.send(event).is_err() {
                                return Err(());
                            }
                            Ok(())
                        };
                        match e.infer_stream_segments(&segments, cb) {
                            Ok(()) => {
                                let _ = resp.send(Ok(()));
                            }
                            Err(err) => {
                                let _ = event_tx.send(StreamEvent::Error(format!(
                                    "Inference error: {}", err
                                )));
                                let _ = resp.send(Err(err));
                            }
                        }
                    } else {
                        let _ = event_tx.send(StreamEvent::Error("No model loaded".into()));
                        let _ = resp.send(Err(anyhow::anyhow!("No model loaded")));
                    }
                }
                Ok(Command::Unload(resp)) => {
                    unsafe { *engine_ptr = None; }
                    let _ = resp.send(Ok(()));
                }
                Ok(Command::ClearKv(resp)) => {
                    if let Some(ref mut e) = unsafe { (*engine_ptr).as_mut() } {
                        let _ = resp.send(e.clear_kv());
                    } else {
                        let _ = resp.send(Ok(()));
                    }
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
            }
        }
        unsafe { *engine_ptr = None; Box::from_raw(engine_ptr); }
    });

    shared_tx
}

// ─── Public async helpers ─────────────────────────────────────────────────────

/// Load a model into the inference thread.
pub async fn load_model(
    handle: &std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>>,
    path: String,
    config: Config,
) -> Result<()> {
    let (resp, rx) = tokio::sync::oneshot::channel();
    {
        let guard = handle.lock().unwrap();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Engine not started"))?;
        let _ = tx.send(Command::Load(path, config, resp));
    }
    rx.await.map_err(|e| anyhow::anyhow!("Channel closed: {}", e))?
}

    /// Start inference on prompt segments; returns receiver for stream events.
    /// Each segment is tokenized independently, guaranteeing the same segments
    /// produce the same tokens across turns (no BPE boundary mismatch).
    pub fn infer_stream(
        handle: &std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>>,
        segments: Vec<String>,
    ) -> Result<(
        tokio::sync::mpsc::UnboundedReceiver<StreamEvent>,
        tokio::sync::oneshot::Receiver<Result<()>>,
    )> {
        let guard = handle.lock().unwrap();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Engine not started"))?;
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (resp, result_rx) = tokio::sync::oneshot::channel();
        let _ = tx.send(Command::InferStream(segments, event_tx, resp));
        Ok((event_rx, result_rx))
    }

/// Unload the model and stop the inference thread (dropping the handle does this implicitly).
pub async fn unload(
    handle: &std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>>,
) -> Result<()> {
    let (resp, rx) = tokio::sync::oneshot::channel();
    {
        let guard = handle.lock().unwrap();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Engine not started"))?;
        let _ = tx.send(Command::Unload(resp));
    }
    rx.await
        .map_err(|e| anyhow::anyhow!("Channel closed: {}", e))?
}

/// Clear the KV cache (e.g., to start a fresh conversation).
pub fn clear_kv(
    handle: &std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>>,
) -> Result<()> {
    let (resp, rx) = tokio::sync::oneshot::channel();
    {
        let guard = handle.lock().unwrap();
        let tx = guard
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Engine not started"))?;
        let _ = tx.send(Command::ClearKv(resp));
        drop(guard);
    }
    tokio::task::block_in_place(|| {
        rx.blocking_recv().map_err(|e| anyhow::anyhow!("Channel closed: {}", e))?
    })
}
