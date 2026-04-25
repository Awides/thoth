use dioxus::prelude::*;
use std::sync::{Arc, Mutex};
use std::fs;
use crate::llama;

#[derive(Clone, PartialEq, Copy)]
pub enum MessageRole { User, Assistant, System }

#[derive(Clone, PartialEq)]
struct Message { id: u64, role: MessageRole, content: String, thinking: String }

#[derive(Clone, PartialEq, Debug)]
enum LoadingState { Loading, Ready, Error(String) }

pub fn App() -> Element {
    let mut messages = use_signal_sync(|| Vec::<Message>::new());
    let mut next_id = use_signal_sync(|| 0u64);
    let mut is_loading = use_signal_sync(|| false);
    let mut loading_state = use_signal_sync(|| LoadingState::Loading);
    let mut engine_ready = use_signal_sync(|| false);

    // Persistent shared engine command sender (initially None)
    let engine_tx = use_signal(|| Arc::new(Mutex::new(None)));

    // Config and model path
    let config = llama::Config {
        n_ctx: 512, n_gpu_layers: 99, n_threads: 8, n_batch: 512,
        use_mmap: true, temperature: 0.7, top_p: 0.9, top_k: 40,
    };
    let model_path = "/home/awides/dev/bn/thoth/models/Bonsai-1.7B-Q1_0.gguf".to_string();

    // Spawn inference thread and load model at startup
    let engine_tx_state = engine_tx.clone();
    let engine_ready_c = engine_ready.clone();
    let loading_state_c = loading_state.clone();
    let is_loading_c = is_loading.clone();
    let config_c = config.clone();
    let model_path_c = model_path.clone();
    use_future(move || {
        let engine_tx_arc = (engine_tx_state)().clone();
        let mut engine_ready = engine_ready_c.clone();
        let mut loading_state = loading_state_c.clone();
        let mut is_loading = is_loading_c.clone();
        let config = config_c.clone();
        let model_path = model_path_c.clone();
        async move {
            is_loading.set(true);
            loading_state.set(LoadingState::Loading);
            let handle = llama::spawn_inference_thread();
            let sender = {
                let mut guard = handle.lock().unwrap();
                guard.take().expect("Sender missing")
            };
            *engine_tx_arc.lock().unwrap() = Some(sender);
            engine_ready.set(true);
            match llama::load_model(&engine_tx_arc, model_path, config).await {
                Ok(_) => loading_state.set(LoadingState::Ready),
                Err(e) => loading_state.set(LoadingState::Error(format!("Load error: {}", e))),
            }
            is_loading.set(false);
        }
    });

    // System message dispatcher
    let system_msg: Arc<Mutex<Box<dyn FnMut(String) + Send>>> = Arc::new(Mutex::new(Box::new({
        let mut messages = messages.clone();
        let mut next_id = next_id.clone();
        move |content: String| {
            let id = next_id();
            next_id.set(id + 1);
            messages.with_mut(|v| v.push(Message { id, role: MessageRole::System, content, thinking: String::new() }));
        }
    })));

    // Input processor
    let process_input: Arc<Mutex<Box<dyn FnMut(String) + Send>>> = Arc::new(Mutex::new(Box::new({
        let mut messages = messages.clone();
        let mut next_id = next_id.clone();
        let mut is_loading = is_loading.clone();
        let mut loading_state = loading_state.clone();
        let engine_tx = (engine_tx)().clone();
        let mut system_msg = system_msg.clone();

        move |input_val: String| {
            if input_val.is_empty() { return; }

            // Slash commands
            if input_val.starts_with('/') {
                let parts: Vec<&str> = input_val.split_whitespace().collect();
                match parts.get(0).copied() {
                    Some("/help") => {
                        (system_msg.lock().unwrap())(format!(
                            "Commands:\n/clear - clear chat\n/list - list models\n/load <path> - load model\n/unload - unload model\n/switch <path> - switch model"
                        ));
                    }
                    Some("/clear") => { messages.with_mut(|v| v.clear()); }
                    Some("/list") => {
                        match fs::read_dir("models") {
                            Ok(entries) => {
                                let mut list = String::new();
                                for entry in entries.flatten() {
                                    if let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) {
                                        if name.ends_with(".gguf") { list.push_str(name); list.push('\n'); }
                                    }
                                }
                                if list.is_empty() { list = "No GGUF models found".to_string(); }
                                (system_msg.lock().unwrap())(format!("Available models:\n{}", list));
                            }
                            Err(e) => (system_msg.lock().unwrap())(format!("Error: {}", e)),
                        }
                    }
                    Some("/unload") => {
                        if is_loading() { return; }
                        is_loading.set(true);
                        loading_state.set(LoadingState::Loading);
                        let mut sm = system_msg.clone();
                        let mut ls = loading_state.clone();
                        let mut il = is_loading.clone();
                        let engine_tx_c = engine_tx.clone();
                        tokio::spawn(async move {
                            match llama::unload(&engine_tx_c).await {
                                Ok(_) => (sm.lock().unwrap())("Model unloaded".into()),
                                Err(e) => (sm.lock().unwrap())(format!("Error: {}", e)),
                            }
                            il.set(false);
                            ls.set(LoadingState::Ready);
                        });
                    }
                    Some("/load") | Some("/switch") => {
                        if parts.len() < 2 {
                            (system_msg.lock().unwrap())("Usage: /load <path> or /switch <path>".into());
                            return;
                        }
                        if is_loading() { return; }
                        let path = parts[1].to_string();
                        let cmd = parts[0].to_string();
                        is_loading.set(true);
                        loading_state.set(LoadingState::Loading);
                        let mut sm = system_msg.clone();
                        let mut ls = loading_state.clone();
                        let mut il = is_loading.clone();
                        let config = llama::Config {
                            n_ctx: 512, n_gpu_layers: 99, n_threads: 8, n_batch: 512,
                            use_mmap: true, temperature: 0.7, top_p: 0.9, top_k: 40,
                        };
                        let engine_tx_c = engine_tx.clone();
                        tokio::spawn(async move {
                            if cmd == "/switch" { let _ = llama::unload(&engine_tx_c).await; }
                            match llama::load_model(&engine_tx_c, path.clone(), config).await {
                                Ok(_) => (sm.lock().unwrap())(format!("Model {}: {}", cmd.trim_start_matches('/'), path)),
                                Err(e) => (sm.lock().unwrap())(format!("Load error: {}", e)),
                            }
                            il.set(false);
                            ls.set(LoadingState::Ready);
                        });
                    }
                    _ => { (system_msg.lock().unwrap())(format!("Unknown command: {}", input_val)); }
                }
                return;
            }

            // Inference (streaming)
            if is_loading() { return; }
            let user_id = next_id();
            next_id.set(user_id + 1);
            messages.with_mut(|v| v.push(Message { id: user_id, role: MessageRole::User, content: input_val.clone(), thinking: String::new() }));
            let assistant_id = next_id();
            next_id.set(assistant_id + 1);
            messages.with_mut(|v| v.push(Message { id: assistant_id, role: MessageRole::Assistant, content: String::new(), thinking: String::new() }));
            is_loading.set(true);

            let mut msgs_c = messages.clone();
            let mut il_c = is_loading.clone();
            let mut a_id = assistant_id;
            let engine_tx_c = engine_tx.clone();

            tokio::spawn(async move {
// Build a chat prompt using Qwen3 chat template format
            // Format: <|im_start|>role\ncontent<|im_end|>
            let prompt = format!(
                "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                input_val
            );
match llama::infer_stream(&engine_tx_c, prompt) {
            Ok((mut rx, _)) => {
                let mut in_thinking = false;
                while let Some(event) = rx.recv().await {
                    match event {
                        llama::StreamEvent::ThinkingStart => { in_thinking = true; }
                        llama::StreamEvent::ThinkingEnd => { in_thinking = false; }
                        llama::StreamEvent::Token(token) => {
                            msgs_c.with_mut(|v| {
                                if let Some(msg) = v.iter_mut().find(|m| m.id == a_id) {
                                    if in_thinking {
                                        msg.thinking.push_str(&token);
                                    } else {
                                        msg.content.push_str(&token);
                                    }
                                }
                            });
                        }
                        llama::StreamEvent::Done(_) => {}
                        llama::StreamEvent::Error(e) => {
                            msgs_c.with_mut(|v| {
                                if let Some(msg) = v.iter_mut().find(|m| m.id == a_id) {
                                    msg.content = format!("Error: {}", e);
                                }
                            });
                            break;
                        }
                    }
                }
            }
                    Err(e) => {
                        msgs_c.with_mut(|v| {
                            if let Some(msg) = v.iter_mut().find(|m| m.id == a_id) {
                                msg.content = format!("Error: {}", e);
                            }
                        });
                    }
                }
                il_c.set(false);
            });
        }
    })));

    let mut input = use_signal(|| String::new());

    let on_keydown = {
        let process_input = process_input.clone();
        let mut is_loading = is_loading.clone();
        let mut loading_state = loading_state.clone();
        move |e: KeyboardEvent| {
            if e.key() == Key::Enter && !e.modifiers().shift() {
                e.prevent_default();
                let val = input.read().trim().to_string();
                if !val.is_empty() && !is_loading() && !matches!(loading_state(), LoadingState::Loading) {
                    (process_input.lock().unwrap())(val);
                }
            }
        }
    };

    let on_submit = {
        let process_input = process_input.clone();
        let mut is_loading = is_loading.clone();
        let mut loading_state = loading_state.clone();
        move |_| {
            let val = input.read().trim().to_string();
            if !val.is_empty() && !is_loading() && !matches!(loading_state(), LoadingState::Loading) {
                (process_input.lock().unwrap())(val);
            }
        }
    };

    let msgs = messages();
    rsx! {
        div {
            style: "display: flex; flex-direction: column; height: 100vh; background: #1e1e1e; color: #e0e0e0; font-family: system-ui, -apple-system, sans-serif;",
            div {
                style: "padding: 1rem; border-bottom: 1px solid #333; background: #252526;",
                h1 { style: "margin: 0; font-size: 1.2rem; color: #fff;", "Thoth – Streaming Inference" },
                p {
                    style: "margin: 0.5rem 0 0; font-size: 0.85rem; color: #888;",
                    match loading_state() {
                        LoadingState::Loading => "Loading model…",
                        LoadingState::Ready => "Inference engine ready",
                        LoadingState::Error(ref e) => e,
                    }
                },
            },
            div {
                style: "flex: 1; overflow-y: auto; padding: 1rem; display: flex; flex-direction: column; gap: 0.75rem;",
                for msg in msgs.iter() {
                    div {
                        key: "{msg.id}",
                        style: format!(
                            "padding: 0.75rem 1rem; border-radius: 8px; max-width: 80%; align-self: {}; background: {}; word-wrap: break-word;",
                            match msg.role { MessageRole::User => "flex-end", _ => "flex-start" },
                            match msg.role { MessageRole::User => "#0d6efd", MessageRole::Assistant => "#2d2d2d", MessageRole::System => "#5c2d2d" }
                        ),
if !msg.thinking.is_empty() {
                        pre { style: "margin: 0 0 0.25rem 0; white-space: pre-wrap; font-family: inherit; font-weight: 300; font-style: italic; opacity: 0.8;", "{msg.thinking}" }
                    }
                    pre { style: "margin: 0; white-space: pre-wrap; font-family: inherit;", "{msg.content}" }
                }
                },
                
            },
            div {
                style: "padding: 1rem; border-top: 1px solid #333; background: #252526;",
                form {
                    onsubmit: on_submit,
                    div { style: "display: flex; gap: 0.5rem;",
                        input {
                            r#type: "text",
                            placeholder: match loading_state() {
                                LoadingState::Loading => "Loading model…",
                                _ => "Enter prompt or /command…",
                            },
                            disabled: matches!(loading_state(), LoadingState::Loading),
                            value: "{input.read()}",
                            oninput: move |e| { *input.write() = e.data.value(); },
                            onkeydown: on_keydown,
                            style: "flex: 1; padding: 0.75rem; border: 1px solid #444; border-radius: 6px; background: #1e1e1e; color: #e0e0e0; font-size: 1rem; outline: none;",
                        },
                        button {
                            r#type: "submit",
                            disabled: is_loading() || matches!(loading_state(), LoadingState::Loading) || input.read().trim().is_empty(),
                            style: if is_loading() || matches!(loading_state(), LoadingState::Loading) {
                                "padding: 0.75rem 1.5rem; border: none; border-radius: 6px; background: #444; color: #fff; font-size: 1rem; cursor: not-allowed; opacity: 0.6;"
                            } else {
                                "padding: 0.75rem 1.5rem; border: none; border-radius: 6px; background: #0d6efd; color: #fff; font-size: 1rem; cursor: pointer;"
                            },
                            "Send"
                        }
                    }
                }
            }
        }
    }
}