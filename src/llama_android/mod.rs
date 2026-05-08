//! Android native llama.cpp inference engine
//! Uses the same FFI bindings as desktop, but with Android-specific paths

// Use pre-generated bindings for Android (bindgen doesn't work on cross-compile)
pub mod bindings {
    include!("../llama/bindings.rs");
}

use self::bindings::*;
use std::ffi::CString;
use std::path::Path;
use anyhow::Result;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

static LOG_SUPPRESSED: AtomicBool = AtomicBool::new(false);

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
) {}

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
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    ThinkingStart,
    ThinkingEnd,
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
        }
    }
}

unsafe impl Send for Engine {}

pub struct Engine {
    model: *mut llama_model,
    ctx: *mut llama_context,
    sampler: *mut llama_sampler,
    config: Config,
}

impl Engine {
    pub fn load<P: AsRef<Path>>(path: P, config: Config) -> Result<Self> {
        let path = path.as_ref();
        let path_cstr = CString::new(path.to_string_lossy().as_bytes())?;
        suppress_llama_logging();
        unsafe {
            llama_backend_init();
        }
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
        let ctx = unsafe { llama_init_from_model(model, ctx_params) };
        if ctx.is_null() {
            unsafe {
                llama_model_free(model);
            }
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
        let t = unsafe { llama_sampler_init_temp(config.temperature) };
        if !t.is_null() {
            unsafe {
                llama_sampler_chain_add(sampler, t);
            }
        }
        let t = unsafe { llama_sampler_init_top_k(config.top_k) };
        if !t.is_null() {
            unsafe {
                llama_sampler_chain_add(sampler, t);
            }
        }
        let t = unsafe { llama_sampler_init_top_p(config.top_p, 1) };
        if !t.is_null() {
            unsafe {
                llama_sampler_chain_add(sampler, t);
            }
        }
        let t = unsafe { llama_sampler_init_greedy() };
        if !t.is_null() {
            unsafe {
                llama_sampler_chain_add(sampler, t);
            }
        }
        Ok(Self {
            model,
            ctx,
            sampler,
            config,
        })
    }

    pub fn reset(&mut self) -> Result<()> {
        unsafe {
            llama_free(self.ctx);
            let mut ctx_params = llama_context_default_params();
            ctx_params.n_ctx = self.config.n_ctx;
            ctx_params.n_batch = self.config.n_batch;
            ctx_params.n_ubatch = self.config.n_batch.min(512);
            ctx_params.n_threads = self.config.n_threads;
            ctx_params.n_threads_batch = self.config.n_threads;
            self.ctx = llama_init_from_model(self.model, ctx_params);
            if self.ctx.is_null() {
                anyhow::bail!("Failed to reset context");
            }
            llama_sampler_reset(self.sampler);
        }
        Ok(())
    }

    pub fn infer_stream<F>(&mut self, prompt: &str, mut f: F) -> Result<()>
    where
        F: FnMut(StreamEvent) -> Result<(), ()>,
    {
        self.reset()?;
        let cstr = CString::new(prompt)?;
        let mut tokens = vec![0i32; prompt.len() * 2];
        let vocab = unsafe { llama_model_get_vocab(self.model) };
        let n_tokens = unsafe {
            llama_tokenize(
                vocab,
                cstr.as_ptr(),
                prompt.len() as i32,
                tokens.as_mut_ptr(),
                tokens.len() as i32,
                false,
                true,
            )
        };
        if n_tokens < 0 {
            anyhow::bail!("Tokenization failed");
        }
        tokens.truncate(n_tokens as usize);
        let batch = unsafe { llama_batch_get_one(tokens.as_mut_ptr(), n_tokens) };
        if unsafe { llama_decode(self.ctx, batch) } != 0 {
            anyhow::bail!("Prompt decode failed");
        }
        let eos = unsafe { llama_token_eos(vocab) };
        let mut output = String::new();
        let max_new = (self.config.n_ctx as i32 - n_tokens as i32)
            .min(256)
            .max(1);
        for _ in 0..max_new {
            let token = unsafe { llama_sampler_sample(self.sampler, self.ctx, -1) };
            if token == eos {
                break;
            }
            let mut buf = [0u8; 32];
            let n = unsafe {
                llama_detokenize(
                    vocab,
                    &token,
                    1,
                    buf.as_mut_ptr(),
                    buf.len() as i32,
                    false,
                    true,
                )
            };
            if n > 0 {
                if let Ok(s) = std::str::from_utf8(&buf[..n as usize]) {
                    let token_str = unsafe {
                        let ptr = llama_token_get_text(vocab, token);
                        if !ptr.is_null() {
                            std::ffi::CStr::from_ptr(ptr)
                                .to_string_lossy()
                                .into_owned()
                        } else {
                            s.to_string()
                        }
                    };
                    if token_str == "<think>" {
                        if f(StreamEvent::ThinkingStart).is_err() {
                            break;
                        }
                    } else if token_str == "</think>" {
                        if f(StreamEvent::ThinkingEnd).is_err() {
                            break;
                        }
                    } else {
                        output.push_str(s);
                        if f(StreamEvent::Token(s.to_string())).is_err() {
                            break;
                        }
                    }
                }
            }
            unsafe {
                llama_sampler_accept(self.sampler, token);
            }
            let mut t = token;
            let batch = unsafe { llama_batch_get_one(&mut t, 1) };
            if unsafe { llama_decode(self.ctx, batch) } != 0 {
                break;
            }
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

#[derive(Debug)]
pub enum Command {
    Load(String, Config, tokio::sync::oneshot::Sender<Result<()>>),
    InferStream(
        String,
        tokio::sync::mpsc::UnboundedSender<StreamEvent>,
        tokio::sync::oneshot::Sender<Result<()>>,
    ),
    Unload(tokio::sync::oneshot::Sender<Result<()>>),
}

pub fn spawn_inference_thread() -> std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>>
{
    let (tx, rx) = std::sync::mpsc::channel();
    let shared_tx: std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));

    std::thread::spawn(move || {
        let mut engine_ptr: *mut Option<Engine> = Box::into_raw(Box::new(None));
        loop {
            match rx.try_recv() {
                Ok(Command::Load(path, config, resp)) => {
                    unsafe {
                        *engine_ptr = None;
                    }
                    match Engine::load(&path, config) {
                        Ok(e) => {
                            unsafe {
                                *engine_ptr = Some(e);
                            }
                            let _ = resp.send(Ok(()));
                        }
                        Err(e) => {
                            let _ = resp.send(Err(e));
                        }
                    }
                }
                Ok(Command::InferStream(prompt, event_tx, resp)) => {
                    if let Some(ref mut e) = unsafe { (*engine_ptr).as_mut() } {
                        let tx = event_tx.clone();
                        let cb = |event: StreamEvent| {
                            if tx.send(event).is_err() {
                                return Err(());
                            }
                            Ok(())
                        };
                        match e.infer_stream(&prompt, cb) {
                            Ok(()) => {
                                let _ = resp.send(Ok(()));
                            }
                            Err(err) => {
                                let _ =
                                    event_tx.send(StreamEvent::Error(format!(
                                        "Inference error: {}",
                                        err
                                    )));
                                let _ = resp.send(Err(err));
                            }
                        }
                    } else {
                        let _ =
                            event_tx.send(StreamEvent::Error("No model loaded".into()));
                        let _ = resp.send(Err(anyhow::anyhow!("No model loaded")));
                    }
                }
                Ok(Command::Unload(resp)) => {
                    unsafe {
                        *engine_ptr = None;
                    }
                    let _ = resp.send(Ok(()));
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
            }
        }
        unsafe {
            *engine_ptr = None;
            Box::from_raw(engine_ptr);
        }
    });

    shared_tx
}

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
    rx.await
        .map_err(|e| anyhow::anyhow!("Channel closed: {}", e))?
}

pub fn infer_stream(
    handle: &std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>>,
    prompt: String,
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
    let _ = tx.send(Command::InferStream(prompt, event_tx, resp));
    Ok((event_rx, result_rx))
}

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
