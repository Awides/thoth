use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use js_sys::{Object, Reflect, Date};
use tokio::sync::{mpsc as tokio_mpsc, oneshot};
use anyhow::Result;
use std::cell::RefCell;

#[derive(Debug, Clone)]
pub struct Config {
pub n_ctx: u32, pub n_gpu_layers: u32, pub n_threads: i32, pub n_batch: u32,
pub use_mmap: bool, pub temperature: f32, pub top_p: f32, pub top_k: i32,
}

impl Default for Config {
fn default() -> Self {
Self { n_ctx: 2048, n_gpu_layers: 0, n_threads: 4, n_batch: 512, use_mmap: false, temperature: 0.8, top_p: 0.9, top_k: 40 }
}
}

#[derive(Debug, Clone)]
pub enum StreamEvent {
Token(String), ThinkingStart, ThinkingEnd, Done(String), Error(String),
}

thread_local! {
static WORKER: RefCell<Option<web_sys::Worker>> = const { RefCell::new(None) };
static LOAD_TX: RefCell<Option<oneshot::Sender<Result<()>>>> = const { RefCell::new(None) };
static INFER_STATE: RefCell<Option<(tokio_mpsc::UnboundedSender<StreamEvent>, oneshot::Sender<Result<()>>)>> = const { RefCell::new(None) };
}

fn setup_worker() -> web_sys::Worker {
WORKER.with(|w| {
if let Some(ref worker) = *w.borrow() {
return worker.clone();
}

web_sys::console::log_1(&"Creating Web Worker from public/worker.js...".into());

// Use external worker file with cache-busting
let worker_url = format!("worker.js?v={}", Date::now() as u64);
let worker = web_sys::Worker::new(&worker_url).unwrap_or_else(|e| {
web_sys::console::error_1(&format!("Worker creation failed: {:?}", e).into());
panic!("Failed to create worker");
});

let handler = Closure::wrap(Box::new(move |e: web_sys::MessageEvent| {
let data = e.data();
let type_val = Reflect::get(&data, &"type".into()).ok().and_then(|v| v.as_string());

match type_val.as_deref() {
Some("worker_loaded") => {
web_sys::console::log_1(&"Worker script loaded successfully".into());
}
Some("ready") => {
web_sys::console::log_1(&"Worker ready!".into());
LOAD_TX.with(|tx| {
if let Some(sender) = tx.borrow_mut().take() {
let _ = sender.send(Ok(()));
}
});
}
Some("token") => {
if let Some(token) = Reflect::get(&data, &"token".into()).ok().and_then(|v| v.as_string()) {
INFER_STATE.with(|state| {
if let Some((tx, _)) = state.borrow().as_ref() {
let _ = tx.send(StreamEvent::Token(token));
}
});
}
}
Some("done") => {
INFER_STATE.with(|state| {
if let Some((tx, resp)) = state.borrow_mut().take() {
let _ = tx.send(StreamEvent::Done(String::new()));
let _ = resp.send(Ok(()));
}
});
}
Some("error") => {
let msg = Reflect::get(&data, &"message".into()).ok().and_then(|v| v.as_string()).unwrap_or_default();
web_sys::console::error_1(&format!("Worker error: {}", msg).into());
INFER_STATE.with(|state| {
if let Some((tx, resp)) = state.borrow_mut().take() {
let _ = tx.send(StreamEvent::Error(msg.clone()));
let _ = resp.send(Err(anyhow::anyhow!("{}", msg)));
}
});
}
Some("progress") => {
let msg = Reflect::get(&data, &"message".into()).ok().and_then(|v| v.as_string()).unwrap_or_default();
web_sys::console::log_1(&format!("Load progress: {}", msg).into());
}
_ => {}
}
}) as Box<dyn FnMut(_)>);

worker.set_onmessage(Some(handler.as_ref().unchecked_ref()));
handler.forget();

*w.borrow_mut() = Some(worker.clone());
worker
})
}

pub fn spawn_inference_thread() -> bool {
let _ = setup_worker();
true
}

pub async fn load_model(_handle: &bool, path: String, _config: Config) -> Result<()> {
    let worker = setup_worker();
    let (tx, rx) = oneshot::channel();
    
    LOAD_TX.with(|ltx| {
        *ltx.borrow_mut() = Some(tx);
    });
    
    web_sys::console::log_1(&format!("Sending load command for: {}", path).into());
    let msg = Object::new();
    Reflect::set(&msg, &"type".into(), &"load".into()).unwrap();
    Reflect::set(&msg, &"path".into(), &path.into()).unwrap();
    worker.post_message(&msg).unwrap();
    
    match rx.await {
        Ok(result) => result,
        Err(_) => Err(anyhow::anyhow!("Load channel closed")),
    }
}

pub fn infer_stream(
    _handle: &bool, prompt: String,
) -> Result<(tokio_mpsc::UnboundedReceiver<StreamEvent>, oneshot::Receiver<Result<()>>)> {
    let worker = setup_worker();
    let (event_tx, event_rx) = tokio_mpsc::unbounded_channel();
    let (resp, result_rx) = oneshot::channel();
    
    INFER_STATE.with(|state| {
        *state.borrow_mut() = Some((event_tx, resp));
    });
    
    let msg = Object::new();
    Reflect::set(&msg, &"type".into(), &"infer".into()).unwrap();
    Reflect::set(&msg, &"prompt".into(), &prompt.into()).unwrap();
    Reflect::set(&msg, &"id".into(), &1.0.into()).unwrap();
    worker.post_message(&msg).unwrap();
    
    Ok((event_rx, result_rx))
}

pub async fn unload(_handle: &bool) -> Result<()> {
    Ok(())
}