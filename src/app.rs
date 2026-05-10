use dioxus::prelude::*;
#[cfg(any(
    all(not(target_arch = "wasm32"), not(target_os = "android")),
    all(target_os = "android", target_arch = "aarch64")
))]
use crate::llama;
use crate::shared::{Message, MessageRole, MessageKind, LoadingState, Theme, now_secs};
use crate::ui::{self, MessageList, InputArea};
use crate::system::model;
use crate::system::config::{self, AppConfig};
use bip39::Mnemonic;
use hostname;
use nostr_sdk::{Keys, ToBech32};

static TAILWIND: Asset = asset!("/assets/tailwind.css");

const markdown_css: &str = r#"
.markdown-content { font-size: 1rem; line-height: 1.75; }
.markdown-content h1 { font-size: 3.5rem; line-height: 1.1; }
.markdown-content strong { font-weight: 600; }
.markdown-content code { background: rgba(100,100,100,0.2); padding: 0.125rem 0.375rem; border-radius: 0.25rem; font-family: monospace; font-size: 0.875em; }
.markdown-content pre { background: rgba(100,100,100,0.15); padding: 0.75rem 1rem; border-radius: 0.375rem; overflow-x: auto; margin: 0.5rem 0; }
.markdown-content pre code { background: transparent; padding: 0; }
"#;

#[cfg(target_os = "android")]
const ANDROID_MODEL_DATA: &[u8] = include_bytes!("android/assets/models/Bonsai-1.7B-Q1_0.gguf");

#[cfg(target_os = "android")]
fn ensure_android_model_extracted() -> Option<String> {
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::Path;

    let model_path = "/data/data/com.example.Thoth/files/models/Bonsai-1.7B-Q1_0.gguf";
    if Path::new(model_path).exists() {
        return Some(model_path.to_string());
    }
    let model_dir = Path::new(model_path).parent()?;
    fs::create_dir_all(model_dir).ok()?;
    let mut file = File::create(model_path).ok()?;
    file.write_all(ANDROID_MODEL_DATA).ok()?;
    eprintln!("Model extracted to: {}", model_path);
    Some(model_path.to_string())
}

#[cfg(not(target_os = "android"))]
fn ensure_android_model_extracted() -> Option<String> {
    None
}

pub fn App() -> Element {
    let theme = use_signal_sync(|| Theme::Dark);
    let messages = use_signal_sync(|| Vec::<Message>::new());
    let next_id = use_signal_sync(|| 0u64);
    let mut is_loading = use_signal_sync(|| false);
    let loading_state = use_signal_sync(|| LoadingState::Loading);
    let input = use_signal(|| String::new());
    let mut at_bottom = use_signal(|| true);
    let mut has_new = use_signal(|| false);

    #[cfg(any(
    all(not(target_arch = "wasm32"), not(target_os = "android")),
    all(target_os = "android", target_arch = "aarch64")
))]
    let handle = use_signal_sync(|| llama::spawn_inference_thread());

    #[cfg(any(
    all(not(target_arch = "wasm32"), not(target_os = "android")),
    all(target_os = "android", target_arch = "aarch64")
))]
    let _fut = {
        let handle_c = handle.clone();
    let llama_config = llama::Config {
        n_ctx: 2048,
        n_gpu_layers: if cfg!(target_os = "android") { 0 } else { 99 },
        n_threads: if cfg!(target_os = "android") { 4 } else { 8 },
        n_batch: 512,
        use_mmap: true,
        temperature: 0.5,
        top_p: 0.85,
        top_k: 20,
        repeat_penalty: 1.5,
    };
        let ls_load = loading_state.clone();
        let il_load = is_loading.clone();
        let mut nid = next_id.clone();
        let mut msgs = messages.clone();
        let mut theme_detect = theme.clone();
        use_future(move || {
            let h = handle_c.read().clone();
            let mut ls = ls_load.clone();
            let mut il = il_load.clone();
            let c = llama_config.clone();
            let mut td = theme_detect.clone();
        async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
            let is_dark = {
                let scheme = std::process::Command::new("gsettings")
                    .args(["get", "org.gnome.desktop.interface", "color-scheme"])
                    .output()
                    .ok()
                    .and_then(|o| String::from_utf8(o.stdout).ok())
                    .unwrap_or_default();
                if scheme.contains("prefer-dark") {
                    true
                } else if scheme.contains("prefer-light") {
                    false
                } else {
                    std::process::Command::new("gsettings")
                        .args(["get", "org.gnome.desktop.interface", "gtk-theme"])
                        .output()
                        .ok()
                        .and_then(|o| String::from_utf8(o.stdout).ok())
                        .map(|s| s.to_lowercase().contains("dark"))
                        .unwrap_or(true)
                }
            };
            #[cfg(any(target_os = "android", target_arch = "wasm32"))]
            let is_dark = true;
            if !is_dark {
                td.set(Theme::Light);
            }
            il.set(true);
                ls.set(LoadingState::Loading);

                let p = if cfg!(target_os = "android") {
                    match ensure_android_model_extracted() {
                        Some(path) => path,
                        None => {
                            eprintln!("ERROR: Android model extraction failed");
                            ls.set(LoadingState::Error("Model extraction failed".to_string()));
                            il.set(false);
                            return;
                        }
                    }
                } else {
                    match crate::system::model::get_model_path() {
                        Some(path) => path.to_string_lossy().into_owned(),
                        None => {
                            eprintln!("ERROR: Model not found. Please ensure model is bundled or placed in ~/.local/share/thoth/models/Bonsai-1.7B-Q1_0.gguf");
                            ls.set(LoadingState::Error("Model not available".to_string()));
                            il.set(false);
                            return;
                        }
                    }
                };

            match llama::load_model(&h, p, c).await {
                Ok(_) => {
                    ls.set(LoadingState::Ready);
                        // Send welcome message
                        let current_id = nid();
                        nid.set(current_id + 1);
                        msgs.with_mut(|v| v.push(Message {
                            id: current_id,
                            role: MessageRole::System,
                            content: "# *THOTH ▷*".to_string(),
                            thinking: String::new(),
                            kind: MessageKind::Text,
                timestamp: now_secs(),
                        }));
                    }
                Err(e) => {
                    ls.set(LoadingState::Error(format!("Load error: {}", e)));
                    }
                }
                il.set(false);
            }
        })
    };

    #[cfg(any(
    all(not(target_arch = "wasm32"), not(target_os = "android")),
    all(target_os = "android", target_arch = "aarch64")
))]
    let mut process_input = {
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
            let trimmed = input.trim().to_string();
            if trimmed.is_empty() { return; }

            if trimmed.starts_with("/theme") { t.set(t().toggle()); return; }
            if trimmed.starts_with("/light") { t.set(Theme::Light); return; }
            if trimmed.starts_with("/dark") { t.set(Theme::Dark); return; }

            // Backup and login commands
            if trimmed == "/backup" {
                let path = config::get_config_path();
                match AppConfig::load(&path) {
                    Ok(cfg) => {
                        if let Some(mnemonic) = cfg.mnemonic_encrypted {
                            let msg_id = nid();
                            nid.set(msg_id + 1);
                            let _ = msgs.with_mut(|v| v.push(Message {
                                id: msg_id,
                                role: MessageRole::System,
                                content: format!("**Your backup phrase:**\n\n`{}`\n\nWrite this down and store it safely. Anyone with this phrase can access your identity.", mnemonic),
                                thinking: String::new(),
                                kind: MessageKind::Text,
                timestamp: now_secs(),
                            }));
                        } else {
                            let msg_id = nid();
                            nid.set(msg_id + 1);
                            let _ = msgs.with_mut(|v| v.push(Message {
                                id: msg_id,
                                role: MessageRole::System,
                                content: "No backup phrase found. Use `/login <mnemonic>` to set up your identity with a backup phrase, or start chatting to generate a new identity.".to_string(),
                                thinking: String::new(),
                                kind: MessageKind::Text,
                timestamp: now_secs(),
                            }));
                        }
                    }
                    Err(e) => {
                        let msg_id = nid();
                        nid.set(msg_id + 1);
                        let _ = msgs.with_mut(|v| v.push(Message {
                            id: msg_id,
                            role: MessageRole::System,
                            content: format!("Error loading config: {}", e),
                            thinking: String::new(),
                            kind: MessageKind::Text,
                timestamp: now_secs(),
                        }));
                    }
                }
                return;
            }
            if trimmed.starts_with("/login ") {
                let mnemonic_str = trimmed["/login ".len()..].trim();
                if mnemonic_str.is_empty() {
                    let msg_id = nid();
                    nid.set(msg_id + 1);
                    let _ = msgs.with_mut(|v| v.push(Message {
                        id: msg_id,
                        role: MessageRole::System,
                        content: "Usage: `/login <12-word backup phrase>`".to_string(),
                        thinking: String::new(),
                        kind: MessageKind::Text,
                timestamp: now_secs(),
                    }));
                    return;
                }
                match Mnemonic::parse(mnemonic_str) {
                    Ok(mnemonic) => {
                        let keys = Keys::generate();
                        let secret_hex = keys.secret_key().to_secret_hex();
                        match keys.public_key().to_bech32() {
                            Ok(public_bech32) => {
                                let device_name = hostname::get()
                                    .unwrap_or_else(|_| "unknown".into())
                                    .to_string_lossy()
                                    .into_owned();
                                let path = config::get_config_path();
                                let mut cfg = AppConfig::load(&path).unwrap_or_default();
                                cfg.mnemonic_encrypted = Some(mnemonic.to_string());
                                cfg.nostr_secret_key_hex = Some(secret_hex);
                                let pubkey_for_display = public_bech32.clone();
                                cfg.nostr_public_key = Some(public_bech32);
                                cfg.device_name = Some(device_name);
                                cfg.onboarding_completed = true;
                                match cfg.save(&path) {
                                    Ok(()) => {
                                        let msg_id = nid();
                                        nid.set(msg_id + 1);
                                        let _ = msgs.with_mut(|v| v.push(Message {
                                            id: msg_id,
                                            role: MessageRole::System,
                                             content: format!("✅ **Identity restored!**\n\nYour public key: `{}`\n\nBackup phrase saved. You can now use Thoth with your identity.", pubkey_for_display),
                                            thinking: String::new(),
                                            kind: MessageKind::Text,
                timestamp: now_secs(),
                                        }));
                                    }
                                    Err(e) => {
                                        let msg_id = nid();
                                        nid.set(msg_id + 1);
                                        let _ = msgs.with_mut(|v| v.push(Message {
                                            id: msg_id,
                                            role: MessageRole::System,
                                            content: format!("Error saving config: {}", e),
                                            thinking: String::new(),
                                            kind: MessageKind::Text,
                timestamp: now_secs(),
                                        }));
                                    }
                                }
                            }
                            Err(e) => {
                                let msg_id = nid();
                                nid.set(msg_id + 1);
                                let _ = msgs.with_mut(|v| v.push(Message {
                                    id: msg_id,
                                    role: MessageRole::System,
                                    content: format!("Error deriving public key: {}", e),
                                    thinking: String::new(),
                                    kind: MessageKind::Text,
                timestamp: now_secs(),
                                }));
                            }
                        }
                    }
                    Err(_) => {
                        let msg_id = nid();
                        nid.set(msg_id + 1);
                        let _ = msgs.with_mut(|v| v.push(Message {
                            id: msg_id,
                            role: MessageRole::System,
                            content: "Invalid backup phrase. Please check the 12 words and try again.".to_string(),
                            thinking: String::new(),
                            kind: MessageKind::Text,
                timestamp: now_secs(),
                        }));
                    }
                }
                return;
            }

            let uid = nid();
            nid.set(uid + 1);
            msgs.with_mut(|v| v.push(Message {
                id: uid, role: MessageRole::User, content: trimmed.clone(),
                thinking: String::new(), kind: MessageKind::Text,
                timestamp: now_secs(),
            }));

            let aid = nid();
            nid.set(aid + 1);
            msgs.with_mut(|v| v.push(Message {
                id: aid, role: MessageRole::Assistant, content: String::new(),
                thinking: String::new(), kind: MessageKind::Text,
                timestamp: now_secs(),
            }));

            il.set(true);
        let h = handle.read().clone();
        let prompt = format!("<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\n{}<|im_end|>\n<|im_start|>assistant\n", trimmed);
        let msgs = msgs.clone();
        let il = il.clone();
        tokio::spawn(async move {
            let mut ms = msgs;
            let mut il = il;
            match llama::infer_stream(&h, prompt) {
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
        il.set(false);
    });
        }
    };

    #[cfg(target_arch = "wasm32")]
    {
        loading_state.set(LoadingState::Ready);

        let msgs = messages.clone();
        let nid = next_id.clone();
        let id = nid();
        nid.set(id + 1);
        msgs.with_mut(|v| v.push(Message {
            id, role: MessageRole::System,
            content: "Web UI: local inference not available. Please use desktop or Android app.".to_string(),
            thinking: String::new(), kind: MessageKind::Text,
                timestamp: now_secs(),
        }));
    }

    #[cfg(target_arch = "wasm32")]
    let mut process_input = {
        let msgs = messages.clone();
        let nid = next_id.clone();
        let il = is_loading.clone();
        move |input: String| {
            let mut msgs = msgs;
            let mut nid = nid;
            let mut il = il;
            il.set(true);
            let trimmed = input.trim().to_string();
            if trimmed.is_empty() { il.set(false); return; }

            let uid = nid();
            nid.set(uid + 1);
            msgs.with_mut(|v| v.push(Message {
                id: uid, role: MessageRole::User, content: trimmed.clone(),
                thinking: String::new(), kind: MessageKind::Text,
                timestamp: now_secs(),
            }));

            let aid = nid();
            nid.set(aid + 1);
            msgs.with_mut(|v| v.push(Message {
                id: aid, role: MessageRole::Assistant, content: String::new(),
                thinking: String::new(), kind: MessageKind::Text,
                timestamp: now_secs(),
            }));

            msgs.with_mut(|v| {
                if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                    msg.content = "Web: remote inference not implemented yet.".to_string();
                }
            });

            il.set(false);
        }
    };

    let mut input_for_submit = input.clone();
    let mut scroll_to_bottom = move || {
        spawn(async move {
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            let js = r#"var el = document.getElementById('message-list'); if (el) { el.scrollTop = 0; }"#;
            let _ = dioxus::document::eval(js).await;
        });
        has_new.set(false);
        at_bottom.set(true);
    };

    let on_submit = EventHandler::new(move |e: FormEvent| {
        e.prevent_default();
        let mut process_input = process_input;
        let val = input_for_submit.read().trim().to_string();
        if val.is_empty() { return; }
        if val.starts_with('/') {
            input_for_submit.set(String::new());
            process_input(val);
            return;
        }
        if *loading_state.read() == LoadingState::Ready {
            input_for_submit.set(String::new());
            process_input(val);
            if !*at_bottom.peek() {
                scroll_to_bottom();
            }
        }
    });

    let current_theme = theme();
    let show_new_btn = *has_new.read();

    rsx! {
        document::Stylesheet { href: TAILWIND },
        style { {markdown_css} },
        div {
            style: format!("background: {}; color: {}; display: flex; flex-direction: column; overflow: hidden;", current_theme.bg(), current_theme.fg()),
            class: "h-screen flex flex-col",
            MessageList {
                messages: messages.clone(),
                current_theme: current_theme.clone(),
                at_bottom: at_bottom,
                has_new: has_new,
            },
            if show_new_btn {
                div {
                    class: "w-full max-w-[896px] mx-auto px-3 flex justify-center",
                    button {
                        key: "scroll-down-btn",
                        onclick: move |_| scroll_to_bottom(),
                        class: "px-4 py-2 rounded-full bg-blue-600 text-white text-sm font-medium shadow-lg hover:bg-blue-500 transition-colors -mt-2 mb-1",
                        "↓ New messages"
                    }
                }
            }
            InputArea {
                input: input,
                on_submit: on_submit,
                loading_state: loading_state,
                theme: current_theme.clone(),
                is_inferencing: *is_loading.read(),
            on_stop: {
                let mut il = is_loading.clone();
                move |_| {
                    #[cfg(any(
    all(not(target_arch = "wasm32"), not(target_os = "android")),
    all(target_os = "android", target_arch = "aarch64")
))]
                    llama::request_stop();
                    il.set(false);
                }
            },
            }
        }
    }
}
