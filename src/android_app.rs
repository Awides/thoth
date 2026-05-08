use dioxus::prelude::*;
use crate::llama;
use std::io::{Read, Write};
use std::fs;
use std::path::Path;

static TAILWIND: Asset = asset!("/assets/tailwind.css");

// Bundled model data - included at compile time  
static MODEL_DATA: &[u8] = include_bytes!("android/assets/models/Bonsai-1.7B-Q1_0.gguf");

/// Extract bundled model to writable location on first launch
fn ensure_model_extracted() -> Result<String, String> {
    let model_path = "/data/data/com.thoth.app/files/models/Bonsai-1.7B-Q1_0.gguf";
    
    // Check if already extracted
    if Path::new(model_path).exists() {
        return Ok(model_path.to_string());
    }
    
    // Create directory
    let model_dir = Path::new(model_path).parent().ok_or("Invalid model path")?;
    fs::create_dir_all(model_dir).map_err(|e| format!("Failed to create model dir: {}", e))?;
    
    // Extract model
    let mut file = fs::File::create(model_path).map_err(|e| format!("Failed to create model file: {}", e))?;
    file.write_all(MODEL_DATA).map_err(|e| format!("Failed to write model: {}", e))?;
    
    eprintln!("Model extracted to: {}", model_path);
    Ok(model_path.to_string())
}

#[derive(Clone, PartialEq, Copy)]
pub enum MessageRole { User, Assistant, System }

#[derive(Clone, PartialEq)]
struct Message {
    id: u64,
    role: MessageRole,
    content: String,
    thinking: String,
}

#[derive(Clone, PartialEq, Debug)]
enum LoadingState { Loading, Ready, Error(String) }

#[derive(Clone, PartialEq, Debug)]
enum Theme { Light, Dark }

impl Theme {
    fn toggle(&self) -> Self {
        match self { Theme::Light => Theme::Dark, Theme::Dark => Theme::Light }
    }
    fn bg(&self) -> &'static str { match self { Theme::Light => "#fafafa", Theme::Dark => "#0d0d0d" } }
    fn fg(&self) -> &'static str { match self { Theme::Light => "#171717", Theme::Dark => "#ededed" } }
    fn panel(&self) -> &'static str { match self { Theme::Light => "#f0f0f0", Theme::Dark => "#1a1a1a" } }
    fn border(&self) -> &'static str { match self { Theme::Light => "#e5e5e5", Theme::Dark => "#262626" } }
}

pub fn App() -> Element {
    let mut theme = use_signal_sync(|| Theme::Dark);
    let mut messages = use_signal_sync(|| Vec::<Message>::new());
    let mut next_id = use_signal_sync(|| 0u64);
    let mut is_loading = use_signal_sync(|| false);
    let mut loading_state = use_signal_sync(|| LoadingState::Loading);
    let mut input = use_signal(|| String::new());

    // Spawn inference thread - persistent across renders
    let handle = use_signal_sync(|| llama::spawn_inference_thread());

    let config = llama::Config {
        n_ctx: 512,
        n_gpu_layers: 99,
        n_threads: 8,
        n_batch: 512,
        use_mmap: true,
        temperature: 0.7,
        top_p: 0.9,
        top_k: 40,
    };

    let handle_c = handle.clone();
    let ls_load = loading_state.clone();
    let il_load = is_loading.clone();

    // Extract bundled model and load it
    let _fut = use_future(move || {
        let h = handle_c.read().clone();
        let mut ls = ls_load.clone();
        let mut il = il_load.clone();
        let c = config.clone();
        
        async move {
            il.set(true);
            ls.set(LoadingState::Loading);
            
            // Extract bundled model to writable location
            let model_path = match ensure_model_extracted() {
                Ok(path) => {
                    eprintln!("Model ready at: {}", path);
                    path
                }
                Err(e) => {
                    eprintln!("Failed to extract model: {}", e);
                    ls.set(LoadingState::Error(format!("Model extraction failed: {}", e)));
                    il.set(false);
                    return;
                }
            };
            
            // Load the model
            eprintln!("Loading model from: {}", model_path);
            match llama::load_model(&h, model_path, c).await {
                Ok(_) => {
                    eprintln!("Model loaded successfully");
                    ls.set(LoadingState::Ready);
                }
                Err(e) => {
                    eprintln!("Model load error: {}", e);
                    ls.set(LoadingState::Error(format!("Load error: {}", e)));
                }
            }
            il.set(false);
        }
    });

    // Process input
    let process_input = {
        let handle = handle.clone();
        let msgs = messages.clone();
        let nid = next_id.clone();
        let il = is_loading.clone();
        let t = theme.clone();

        move |input: String| {
            let mut msgs = msgs;
            let mut nid = nid;
            let mut il = il;
            let mut t = t;
            let handle = handle;
            let trimmed = input.trim().to_string();

            let id = nid();
            nid.set(id + 1);
            msgs.with_mut(|v| {
                v.push(Message { id, role: MessageRole::User, content: trimmed.clone(), thinking: String::new() })
            });

            if trimmed.starts_with("/theme") {
                t.set(t().toggle());
                return;
            }
            if trimmed.starts_with("/light") {
                t.set(Theme::Light);
                return;
            }
            if trimmed.starts_with("/dark") {
                t.set(Theme::Dark);
                return;
            }

            il.set(true);
            let aid = nid();
            nid.set(aid + 1);
            msgs.with_mut(|v| {
                v.push(Message { id: aid, role: MessageRole::Assistant, content: String::new(), thinking: String::new() })
            });

            let ms = msgs.clone();
            let il2 = il.clone();
            let h2 = handle.read().clone();
            let prompt = format!(
                "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n",
                trimmed
            );

            tokio::spawn(async move {
                let mut ms = ms;
                let mut il2 = il2;
                eprintln!("DEBUG: starting infer_stream...");
                match llama::infer_stream(&h2, prompt) {
                    Ok((mut rx, _)) => {
                        let mut in_thinking = false;
                        while let Some(event) = rx.recv().await {
                            match event {
                                llama::StreamEvent::ThinkingStart => { in_thinking = true; }
                                llama::StreamEvent::ThinkingEnd => { in_thinking = false; }
                                llama::StreamEvent::Token(token) => {
                                    ms.with_mut(|v| {
                                        if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                                            if in_thinking { msg.thinking.push_str(&token); }
                                            else { msg.content.push_str(&token); }
                                        }
                                    });
                                }
                                llama::StreamEvent::Done(_) => {}
                                llama::StreamEvent::Error(e) => {
                                    ms.with_mut(|v| {
                                        if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                                            msg.content = format!("Error: {}", e);
                                        }
                                    });
                                    break;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        ms.with_mut(|v| {
                            if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                                msg.content = format!("Error: {}", e);
                            }
                        });
                    }
                }
                il2.set(false);
            });
        }
    };

    let on_submit = move |e: FormEvent| {
        e.prevent_default();
        let val = input.read().trim().to_string();
        if !val.is_empty() && !is_loading() && !matches!(loading_state(), LoadingState::Loading) {
            process_input(val);
        }
    };

    let current_theme = theme();
    let msgs = messages();

    rsx! {
        document::Stylesheet { href: TAILWIND }
        div {
            class: "h-screen flex flex-col",
            style: format!("background: {}; color: {}", current_theme.bg(), current_theme.fg()),
            div {
                class: "flex-1 overflow-y-auto p-4 space-y-3 min-h-0 scroll-smooth flex flex-col-reverse",
                for msg in msgs.iter().rev() {
                    div {
                        key: "{msg.id}",
                        class: format!("p-3 rounded-lg max-w-[80%] break-words {}", 
                            match msg.role { MessageRole::User => "self-end", _ => "self-start" }
                        ),
                        style: format!("background: {}", match msg.role {
                            MessageRole::User => "#3b82f6",
                            MessageRole::Assistant => current_theme.panel(),
                            MessageRole::System => "#5c2d2d",
                        }),
                        if !msg.thinking.is_empty() {
                            pre { class: "text-sm italic opacity-80 mb-1 whitespace-pre-wrap font-inherit font-light", "{msg.thinking}" }
                        }
                        pre { class: "m-0 whitespace-pre-wrap font-inherit", "{msg.content}" }
                    }
                }
                div {
                    class: "h-px w-full",
                    onmounted: move |event| {
                        spawn(async move {
                            tokio::time::sleep(tokio::time::Duration::from_millis(150)).await;
                            let _ = event.scroll_to(dioxus::html::ScrollBehavior::Smooth).await;
                        });
                    },
                }
            }
            div {
                class: "p-3 border-t",
                style: format!("border-color: {}; background: {}", current_theme.border(), current_theme.panel()),
                form {
                    onsubmit: on_submit,
                    div { class: "flex gap-2",
                        input {
                            r#type: "text",
                            autofocus: true,
                            placeholder: match loading_state() { LoadingState::Loading => "Loading…", _ => "Prompt…" },
                            disabled: matches!(loading_state(), LoadingState::Loading),
                            value: "{input.read()}",
                            oninput: move |e| *input.write() = e.data.value(),
                            class: "flex-1 px-3 py-2 border rounded focus:outline-none focus:ring-2 focus:ring-blue-500",
                            style: format!("background: {}; color: {}; border-color: {}", current_theme.bg(), current_theme.fg(), current_theme.border()),
                            onmounted: move |event| {
                                spawn(async move {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                    let _ = event.set_focus(true).await;
                                });
                            },
                        }
                        button {
                            r#type: "submit",
                            disabled: is_loading() || matches!(loading_state(), LoadingState::Loading) || input.read().trim().is_empty(),
                            class: "px-4 py-2 rounded text-white disabled:opacity-50 disabled:cursor-not-allowed",
                            style: if is_loading() || matches!(loading_state(), LoadingState::Loading) { "background: #444;" } else { "background: #3b82f6;" },
                            "Send"
                        }
                    }
                }
            }
        }
    }
}