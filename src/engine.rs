use tokio::sync::mpsc;
use crate::llama;

#[derive(Clone, Debug)]
pub struct Config {
    pub n_ctx: u32,
    pub n_gpu_layers: u32,
    pub n_threads: u32,
    pub n_batch: u32,
    pub use_mmap: bool,
    pub temperature: f32,
    pub top_p: f32,
    pub top_k: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            n_ctx: 512,
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

pub fn new_handle() -> std::sync::Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<llama::Command>>>> {
    llama::spawn_inference_thread()
}