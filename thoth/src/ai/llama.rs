//! llama.cpp backend with full FFI implementation

use anyhow::{Result, anyhow};
use std::ffi::{CStr, CString};
use std::ptr;
use std::sync::Arc;

// FFI Types from llama.h
#[repr(C)]
pub struct llama_context { _unused: [u8; 0] }
#[repr(C)]
pub struct llama_model { _unused: [u8; 0] }
#[repr(C)] 
pub struct llama_sampler { _unused: [u8; 0] }

#[repr(C)]
pub struct llama_batch {
    pub n_tokens: i32,
    pub tokens: *mut i32,
    pub embd: *mut f32,
    pub n_seq_id: *mut i32,
    pub seq_id: *mut *mut i32,
    pub logits: *mut f32,
    pub all_pos_0: i32,
    pub all_pos_1: i32,
}

#[repr(C)]
pub struct llama_model_params {
    pub n_gpu_layers: i32,
    pub split_mode: i32,
    pub main_gpu: i32,
    pub tensor_split: *mut f32,
    pub rpc_servers: *const i8,
    pub progress_callback: Option<unsafe extern "C" fn(f32, *mut std::ffi::c_void) -> bool>,
    pub progress_callback_user_data: *mut std::ffi::c_void,
    pub vocab_only: bool,
    pub use_mmap: bool,
    pub use_mlock: bool,
    pub check_tensors: bool,
}

#[repr(C)]
pub struct llama_context_params {
    pub seed: u64,
    pub n_ctx: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub n_seq_max: u32,
    pub n_threads: u32,
    pub n_threads_batch: u32,
    pub rope_scaling_type: i32,
    pub pooling_type: i32,
    pub rope_freq_base: f32,
    pub rope_freq_scale: f32,
    pub yarn_ext_factor: f32,
    pub yarn_attn_factor: f32,
    pub yarn_beta_fast: f32,
    pub yarn_beta_slow: f32,
    pub yarn_orig_ctx: u32,
    pub defrag_thold: f32,
    pub cb_eval: Option<unsafe extern "C" fn(i32, i32, *mut std::ffi::c_void) -> bool>,
    pub cb_eval_user_data: *mut std::ffi::c_void,
    pub type_k: u32,
    pub type_v: u32,
    pub logits_all: bool,
    pub embeddings: bool,
    pub offload_kqv: bool,
    pub flash_attn: bool,
}

// FFI Functions
extern "C" {
    fn llama_backend_init(numa: bool);
    fn llama_model_default_params() -> llama_model_params;
    fn llama_context_default_params() -> llama_context_params;
    fn llama_load_model_from_file(path: *const i8, params: *const llama_model_params) -> *mut llama_model;
    fn llama_free_model(model: *mut llama_model);
    fn llama_new_context_with_model(model: *mut llama_model, params: llama_context_params) -> *mut llama_context;
    fn llama_free(ctx: *mut llama_context);
    fn llama_tokenize(model: *const llama_model, text: *const i8, text_len: i32, tokens: *mut i32, n_max_tokens: i32, add_bos: bool, special: bool) -> i32;
    fn llama_decode(ctx: *mut llama_context, batch: llama_batch) -> i32;
    fn llama_sampler_sample(sampler: *mut llama_sampler, ctx: *mut llama_context, idx: i32) -> i32;
    fn llama_token_to_piece(model: *const llama_model, token: i32, buf: *mut i8, length: i32, lstrip: i32, special: bool) -> i32;
    fn llama_token_eos(model: *const llama_model) -> i32;
    fn llama_get_sampler(ctx: *mut llama_context, i: i32) -> *mut llama_sampler;
}

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
    pub type_k: u32,
    pub type_v: u32,
    pub flash_attn: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            n_ctx: 2048,
            n_gpu_layers: 0,
            n_threads: 4,
            n_batch: 512,
            use_mmap: true,
            temperature: 0.8,
            top_p: 0.9,
            top_k: 40,
            type_k: 0,
            type_v: 0,
            flash_attn: true,
        }
    }
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
    Token(String),
    ThinkingStart,
    ThinkingEnd,
    Done(String),
    Error(String),
}

pub struct Engine {
    ctx: *mut llama_context,
    model: *mut llama_model,
    config: Config,
}

unsafe impl Send for Engine {}
unsafe impl Sync for Engine {}

impl Engine {
    pub fn new(path: String, config: Config) -> Result<Self> {
        unsafe {
            llama_backend_init(false);
            
            let path_c = CString::new(path.as_str()).map_err(|_| anyhow!("Invalid path"))?;
            let mut model_params = llama_model_default_params();
            model_params.n_gpu_layers = config.n_gpu_layers as i32;
            model_params.use_mmap = config.use_mmap;
            
            eprintln!("[llama.cpp] Loading model: {}", path);
            let model = llama_load_model_from_file(path_c.as_ptr(), &model_params);
            if model.is_null() {
                return Err(anyhow!("Failed to load model"));
            }
            
            let mut ctx_params = llama_context_default_params();
            ctx_params.n_ctx = config.n_ctx;
            ctx_params.n_batch = config.n_batch;
            ctx_params.n_threads = config.n_threads as u32;
            ctx_params.n_threads_batch = config.n_threads as u32;
            ctx_params.flash_attn = config.flash_attn;
            
            eprintln!("[llama.cpp] Creating context (n_ctx={}, batch={})", config.n_ctx, config.n_batch);
            let ctx = llama_new_context_with_model(model, ctx_params);
            if ctx.is_null() {
                llama_free_model(model);
                return Err(anyhow!("Failed to create context"));
            }
            
            eprintln!("[llama.cpp] Model loaded successfully!");
            Ok(Self { ctx, model, config })
        }
    }

    pub fn infer_stream<F>(&self, prompt: &str, mut callback: F) -> Result<()>
    where
        F: FnMut(&str) -> Result<(), ()>,
    {
        unsafe {
            let prompt_c = CString::new(prompt).map_err(|_| anyhow::anyhow!("Error"))?;
            
            // Tokenize
            let mut tokens: Vec<i32> = vec![0; prompt.len() * 2];
            let n_tokens = llama_tokenize(
                self.model,
                prompt_c.as_ptr(),
                prompt.len() as i32,
                tokens.as_mut_ptr(),
                tokens.len() as i32,
                true,
                false,
            );
            
            if n_tokens < 0 {
                return Err(anyhow::anyhow!("Error"));
            }
            tokens.truncate(n_tokens as usize);
            
            // Decode prompt
            let batch = llama_batch {
                n_tokens,
                tokens: tokens.as_mut_ptr(),
                embd: ptr::null_mut(),
                n_seq_id: ptr::null_mut(),
                seq_id: ptr::null_mut(),
                logits: ptr::null_mut(),
                all_pos_0: 0,
                all_pos_1: 0,
            };
            
            if llama_decode(self.ctx, batch) != 0 {
                return Err(anyhow::anyhow!("Error"));
            }
            
            // Generate
            let max_tokens = self.config.n_ctx.saturating_sub(n_tokens as u32) as i32;
            for _ in 0..max_tokens {
                let sampler = llama_get_sampler(self.ctx, 0);
                let mut token = llama_sampler_sample(sampler, self.ctx, -1);
                
                if token == llama_token_eos(self.model) {
                    break;
                }
                
                // Convert token to text
                let mut buf = vec![0i8; 32];
                let len = llama_token_to_piece(self.model, token, buf.as_mut_ptr(), 32, 0, false);
                if len > 0 {
                    buf.truncate(len as usize);
                    if let Ok(piece) = CStr::from_ptr(buf.as_ptr()).to_str() {
                        if callback(piece).is_err() { return Err(anyhow::anyhow!("Callback error")); }
                    }
                }
                
                // Decode next token
                let batch = llama_batch {
                    n_tokens: 1,
                    tokens: &mut token,
                    embd: ptr::null_mut(),
                    n_seq_id: ptr::null_mut(),
                    seq_id: ptr::null_mut(),
                    logits: ptr::null_mut(),
                    all_pos_0: 0,
                    all_pos_1: 0,
                };
                llama_decode(self.ctx, batch);
            }
        }
        Ok(())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        unsafe {
            if !self.ctx.is_null() { llama_free(self.ctx); }
            if !self.model.is_null() { llama_free_model(self.model); }
        }
    }
}

// Command interface
#[derive(Debug)]
pub enum Command {
    Load(String, Config, tokio::sync::oneshot::Sender<Result<()>>),
    InferStream(String, tokio::sync::mpsc::UnboundedSender<StreamEvent>, tokio::sync::oneshot::Sender<Result<()>>),
    Unload(tokio::sync::oneshot::Sender<Result<()>>),
    Stop,
}

pub fn spawn_inference_thread() -> Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let shared_tx = Arc::new(std::sync::Mutex::new(Some(tx)));
    
    std::thread::spawn(move || {
        let mut engine: Option<Engine> = None;
        
        loop {
            match rx.try_recv() {
                Ok(Command::Load(path, config, resp)) => {
                    match Engine::new(path, config) {
                        Ok(e) => {
                            engine = Some(e);
                            let _ = resp.send(Ok(()));
                        }
                        Err(e) => { let _ = resp.send(Err(e)); }
                    }
                }
                Ok(Command::InferStream(prompt, event_tx, resp)) => {
                    if let Some(ref eng) = engine {
                        let tx = event_tx.clone();
                        match eng.infer_stream(&prompt, |token| {
                            tx.send(StreamEvent::Token(token.to_string())).map_err(|_| ())
                        }) {
                            Ok(_) => {
                                let _ = event_tx.send(StreamEvent::Done(String::new()));
                                let _ = resp.send(Ok(()));
                            }
                            Err(_) => {
                                let _ = resp.send(Err(anyhow::anyhow!("Generation failed")));
                            }
                        }
                    } else {
                        let _ = event_tx.send(StreamEvent::Error("No model".into()));
                        let _ = resp.send(Err(anyhow::anyhow!("No model")));
                    }
                }
                Ok(Command::Unload(resp)) => {
                    engine = None;
                    let _ = resp.send(Ok(()));
                }
                Ok(Command::Stop) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => break,
                Err(std::sync::mpsc::TryRecvError::Empty) => std::thread::sleep(std::time::Duration::from_micros(100)),
            }
        }
    });
    
    shared_tx
}

pub async fn load_model(handle: &Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>>, path: String, config: Config) -> Result<()> {
    let (resp, rx) = tokio::sync::oneshot::channel();
    {
        let guard = handle.lock().unwrap();
        let tx = guard.as_ref().ok_or_else(|| anyhow::anyhow!("Engine not started"))?;
        let _ = tx.send(Command::Load(path, config, resp));
    }
    rx.await.map_err(|e| anyhow::anyhow!("Channel: {}", e))??;
    Ok(())
}

pub fn infer_stream(handle: &Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>>, prompt: String) -> Result<(tokio::sync::mpsc::UnboundedReceiver<StreamEvent>, tokio::sync::oneshot::Receiver<Result<()>>)> {
    let guard = handle.lock().unwrap();
    let tx = guard.as_ref().ok_or_else(|| anyhow::anyhow!("Engine not started"))?;
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
    let (resp, result_rx) = tokio::sync::oneshot::channel();
    let _ = tx.send(Command::InferStream(prompt, event_tx, resp));
    Ok((event_rx, result_rx))
}

pub async fn unload(handle: &Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>>) -> Result<()> {
    let (resp, rx) = tokio::sync::oneshot::channel();
    {
        let guard = handle.lock().unwrap();
        let tx = guard.as_ref().ok_or_else(|| anyhow::anyhow!("Engine not started"))?;
        let _ = tx.send(Command::Unload(resp));
    }
    rx.await.map_err(|e| anyhow::anyhow!("Channel: {}", e))?
}

pub fn stop(handle: &Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<Command>>>>) {
    if let Ok(guard) = handle.lock() {
        if let Some(tx) = guard.as_ref() { let _ = tx.send(Command::Stop); }
    }
}
