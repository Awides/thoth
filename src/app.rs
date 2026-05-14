use dioxus::prelude::*;
use crate::mem::{self, ChatMessage, ConversationSnapshot, MemoryFact};
#[cfg(any(
all(not(target_arch = "wasm32"), not(target_os = "android")),
target_os = "android"
))]
use crate::llama;
use crate::shared::{Message, MessageRole, MessageKind, LoadingState, Theme, now_secs, next_msg_id};
use crate::ui::{self, MessageList, InputArea};
use crate::system::model;
use crate::system::config::{self, AppConfig};
#[cfg(any(
all(not(target_arch = "wasm32"), not(target_os = "android")),
target_os = "android"
))]
use crate::tools::{ToolEngine, parse_tool_calls};
use bip39::Mnemonic;
use hostname;
use nostr_sdk::{Keys, ToBech32};

const TAILWIND_CSS: &str = include_str!("../assets/tailwind.css");
const FONTS_CSS: &str = include_str!("../assets/fonts.css");
const APP_CSS: &str = include_str!("../assets/app.css");

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

fn detect_language() -> String {
    #[cfg(target_os = "android")]
    {
        let locale = std::env::var("LANG")
            .or_else(|_| std::env::var("LC_ALL"))
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .unwrap_or_else(|_| "en_US.UTF-8".to_string());
        locale.split('.').next().unwrap_or("en_US").to_string()
    }
    #[cfg(not(target_os = "android"))]
    {
        std::env::var("LC_ALL")
            .or_else(|_| std::env::var("LC_MESSAGES"))
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_else(|_| "en_US.UTF-8".to_string())
            .split('.')
            .next()
            .unwrap_or("en_US")
            .to_string()
    }
}

fn default_system_prompt(lang: &str) -> String {
    let greet_instruction = if lang.starts_with("en") {
        "Greet the user in English."
    } else if lang.starts_with("es") {
        "Greet the user in Spanish."
    } else if lang.starts_with("fr") {
        "Greet the user in French."
    } else if lang.starts_with("de") {
        "Greet the user in German."
    } else if lang.starts_with("pt") {
        "Greet the user in Portuguese."
    } else if lang.starts_with("ja") {
        "Greet the user in Japanese."
    } else if lang.starts_with("zh") {
        "Greet the user in Chinese."
    } else if lang.starts_with("ko") {
        "Greet the user in Korean."
    } else if lang.starts_with("ru") {
        "Greet the user in Russian."
    } else if lang.starts_with("ar") {
        "Greet the user in Arabic."
    } else if lang.starts_with("hi") {
        "Greet the user in Hindi."
    } else if lang.starts_with("it") {
        "Greet the user in Italian."
    } else if lang.starts_with("nl") {
        "Greet the user in Dutch."
    } else if lang.starts_with("pl") {
        "Greet the user in Polish."
    } else if lang.starts_with("tr") {
        "Greet the user in Turkish."
    } else if lang.starts_with("vi") {
        "Greet the user in Vietnamese."
    } else if lang.starts_with("th") {
        "Greet the user in Thai."
    } else {
        "Greet the user in their language."
    };
    format!("You are Tot, a helpful and concise AI assistant. You respond in the user's language. {greet_instruction} Be friendly but brief. You can use the provided tools when helpful.")
}

fn splash_content(is_new_user: bool) -> String {
    if is_new_user {
        "# *THOTH▷*\n\nWelcome! Type to chat with **Tot**, or use:\n\n- `/login <phrase>` — restore your identity from a backup phrase\n- `/backup` — view your backup phrase\n- `/system` — view or set the system prompt\n- `/theme` — toggle dark/light mode".to_string()
    } else {
        "# *THOTH▷*".to_string()
    }
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
    let facts = use_signal_sync(|| Vec::<MemoryFact>::new());
    let mut system_prompt = use_signal_sync(|| {
        let lang = detect_language();
        default_system_prompt(&lang)
    });

    #[cfg(any(
all(not(target_arch = "wasm32"), not(target_os = "android")),
target_os = "android"
    ))]
let tool_engine = use_signal_sync(|| {
    let tool_dir = {
        let candidate = std::env::current_dir()
            .unwrap_or_default()
            .join("assets/tools");
        if candidate.exists() {
            candidate
        } else {
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
            let manifest_candidate = std::path::Path::new(&manifest_dir).join("assets/tools");
            if manifest_candidate.exists() {
                manifest_candidate
            } else {
                candidate
            }
        }
    };
    eprintln!("Loading tools from: {}", tool_dir.display());
    let mut te = ToolEngine::new(tool_dir);
    #[cfg(not(target_os = "android"))]
    if let Err(e) = te.load_scripts() {
        eprintln!("WARNING: tool script loading failed: {}", e);
    }
    #[cfg(target_os = "android")]
    te.load_embedded_scripts();
    eprintln!("Loaded {} tools", te.tool_defs().len());
    te
    });

    let mem_handle = use_signal_sync(|| {
    crate::mem::spawn_worker()
});

    #[cfg(any(
all(not(target_arch = "wasm32"), not(target_os = "android")),
target_os = "android"
))]
    let handle = use_signal_sync(|| llama::spawn_inference_thread());

    #[cfg(any(
all(not(target_arch = "wasm32"), not(target_os = "android")),
target_os = "android"
))]
    let _fut = {
        let handle_c = handle.clone();
        let llama_config = llama::Config {
            n_ctx: 4096,
            n_gpu_layers: if cfg!(target_os = "android") { 0 } else { 99 },
            n_threads: if cfg!(target_os = "android") { 4 } else { 8 },
            n_batch: 128,
            use_mmap: true,
            temperature: 0.5,
            top_p: 0.85,
            top_k: 20,
            min_p: 0.0,
            repeat_penalty: 1.5,
            type_k: llama::GGML_TYPE_Q4_0,
            type_v: llama::GGML_TYPE_Q4_0,
        };
        let ls_load = loading_state.clone();
        let il_load = is_loading.clone();
        let mut nid = next_id.clone();
        let mut msgs = messages.clone();
    let mut theme_detect = theme.clone();
    let memh = mem_handle.clone();
    let facts_load = facts.clone();
    use_future(move || {
        let h = handle_c.read().clone();
        let mut ls = ls_load.clone();
        let mut il = il_load.clone();
        let c = llama_config.clone();
        let mut td = theme_detect.clone();
        let memh = memh.read().clone();
        let mut facts_sig = facts_load.clone();
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
        let mh = memh.clone();
        if let Some(path) = mem::memvid_path() {
                match mh.open(path) {
            Ok(snap) => {
                if !snap.facts.is_empty() {
                    facts_sig.set(snap.facts.clone());
                }
                    if snap.messages.is_empty() {
                        let is_new = config::needs_onboarding();
                        let current_id = nid();
                        nid.set(current_id + 1);
                        msgs.with_mut(|v| v.push(Message {
                            id: current_id,
                            role: MessageRole::System,
                            content: splash_content(is_new),
                            thinking: String::new(),
                            kind: MessageKind::Text,
                            timestamp: now_secs(),
                        }));
                        } else {
                            for cm in &snap.messages {
                                msgs.with_mut(|v| v.push(cm.to_shared()));
                            }
                            if snap.next_id > nid() {
                                nid.set(snap.next_id);
                            }
                        }
                    }
                Err(_) => {
                    let is_new = config::needs_onboarding();
                    let current_id = nid();
                    nid.set(current_id + 1);
                    msgs.with_mut(|v| v.push(Message {
                        id: current_id,
                        role: MessageRole::System,
                        content: splash_content(is_new),
                        thinking: String::new(),
                        kind: MessageKind::Text,
                        timestamp: now_secs(),
                    }));
                }
            }
        } else {
            let is_new = config::needs_onboarding();
            let current_id = nid();
            nid.set(current_id + 1);
            msgs.with_mut(|v| v.push(Message {
                id: current_id,
                role: MessageRole::System,
                content: splash_content(is_new),
                thinking: String::new(),
                kind: MessageKind::Text,
                timestamp: now_secs(),
            }));
            }
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
target_os = "android"
))]
    let mut process_input = {
    let handle = handle.clone();
    let msgs = messages.clone();
    let nid = next_id.clone();
    let il = is_loading.clone();
        let t = theme.clone();
        let memh = mem_handle.clone();
        let facts_sig = facts.clone();
        let tool_engine_c = tool_engine.clone();
        let sp = system_prompt.clone();
        move |input: String| {
            let mut msgs = msgs;
            let mut nid = nid;
            let mut il = il;
            let mut t = t;
            let memh = memh;
            let mut facts_sig = facts_sig;
            let tool_engine = tool_engine_c;
            let mut system_prompt_sig = sp;
        let trimmed = input.trim().to_string();
            if trimmed.is_empty() { return; }

        if trimmed.starts_with("/theme") { t.set(t().toggle()); return; }
        if trimmed.starts_with("/light") { t.set(Theme::Light); return; }
        if trimmed.starts_with("/dark") { t.set(Theme::Dark); return; }
        if trimmed.starts_with("/system") {
            let rest = trimmed["/system".len()..].trim();
            if rest.is_empty() || rest == "show" {
                let current = system_prompt_sig.read().clone();
                let msg_id = nid();
                nid.set(msg_id + 1);
                let _ = msgs.with_mut(|v| v.push(Message {
                    id: msg_id, role: MessageRole::System,
                    content: format!("**System prompt:**\n```\n{}\n```", current),
                    thinking: String::new(), kind: MessageKind::Text, timestamp: now_secs(),
                }));
            } else if rest == "reset" {
                system_prompt_sig.set(default_system_prompt(&detect_language()));
                let msg_id = nid();
                nid.set(msg_id + 1);
                let _ = msgs.with_mut(|v| v.push(Message {
                    id: msg_id, role: MessageRole::System,
                    content: "System prompt reset to default.".to_string(),
                    thinking: String::new(), kind: MessageKind::Text, timestamp: now_secs(),
                }));
            } else {
                system_prompt.set(rest.to_string());
                let msg_id = nid();
                nid.set(msg_id + 1);
                let _ = msgs.with_mut(|v| v.push(Message {
                    id: msg_id, role: MessageRole::System,
                    content: format!("System prompt updated:\n```\n{}\n```", rest),
                    thinking: String::new(), kind: MessageKind::Text, timestamp: now_secs(),
                }));
            }
            return;
        }
    if trimmed == "/clear" {
        let h = handle.read().clone();
        let _ = llama::clear_kv(&h);
        msgs.set(Vec::new());
        let msg_id = nid();
        nid.set(msg_id + 1);
        let welcome = Message {
            id: msg_id,
            role: MessageRole::System,
            content: splash_content(false),
            thinking: String::new(),
            kind: MessageKind::Text,
            timestamp: now_secs(),
        };
        msgs.with_mut(|v| v.push(welcome.clone()));
        memh.read().clone().append_snapshot(ConversationSnapshot { next_id: msg_id + 1, messages: vec![ChatMessage::from_shared(&welcome)], facts: Vec::new() });
        facts_sig.set(Vec::new());
        return;
    }

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
    let user_msg = Message { id: uid, role: MessageRole::User, content: trimmed.clone(), thinking: String::new(), kind: MessageKind::Text, timestamp: now_secs() };
    msgs.with_mut(|v| v.push(user_msg.clone()));
    memh.read().clone().append_message(ChatMessage::from_shared(&user_msg));

    extract_facts(&trimmed, facts_sig.clone(), &memh);

        let aid = nid();
        nid.set(aid + 1);
        let asst_msg = Message { id: aid, role: MessageRole::Assistant, content: String::new(), thinking: String::new(), kind: MessageKind::Text, timestamp: now_secs() };
        msgs.with_mut(|v| v.push(asst_msg.clone()));

        il.set(true);
        let h = handle.read().clone();
        let msgs_snapshot: Vec<(MessageRole, String, String)> = msgs.read().iter()
            .filter(|m| (m.role == MessageRole::User || m.role == MessageRole::Assistant) && !m.content.is_empty())
            .map(|m| (m.role.clone(), m.content.clone(), m.thinking.clone()))
            .collect();
        let current_facts = facts_sig.read().clone();
        let assistant_prefix = "<|im_start|>assistant\n".to_string();
    let current_sp = system_prompt_sig.read().clone();
    let mut segments: Vec<String> = vec![
        format!("<|im_start|>system\n{}", current_sp),
    ];
    #[cfg(any(
        all(not(target_arch = "wasm32"), not(target_os = "android")),
        target_os = "android"
    ))]
    {
        let te = tool_engine.read();
        let tools_section = te.tools_prompt_section();
                    if !tools_section.is_empty() {
                        segments[0].push_str(&tools_section);
                    }
                }
    segments[0].push_str("<|im_end|>\n");
        if !current_facts.is_empty() {
            let mut facts_text = String::from("<|im_start|>system\nWhat you know about the user:\n");
            for f in &current_facts {
                facts_text.push_str(&format!("- {}: {}\n", f.key, f.value));
            }
            facts_text.push_str("<|im_end|>\n");
            segments.push(facts_text);
        }
        let mut prompt_chars = segments.iter().map(|s| s.len()).sum::<usize>() + assistant_prefix.len();
        let max_prompt_chars = 8192;
        let start_idx = {
            let mut idx = 0;
            let mut chars = prompt_chars;
            for (i, m) in msgs_snapshot.iter().enumerate() {
                let turn_len = match m.0 {
                    MessageRole::User => format!("<|im_start|>user\n{}<|im_end|>\n", m.1).len(),
                    MessageRole::Assistant => format!("<|im_start|>assistant\n{}<|im_end|>\n", m.1).len(),
                    _ => continue,
                };
                chars += turn_len;
                if chars > max_prompt_chars && i < msgs_snapshot.len() - 1 {
                    idx = i + 1;
                    break;
                }
            }
            idx
        };
        for i in start_idx..msgs_snapshot.len() {
            let (role, content, _) = &msgs_snapshot[i];
            let turn = match role {
                MessageRole::User => format!("<|im_start|>user\n{}<|im_end|>\n", content),
                MessageRole::Assistant => {
                    format!("<|im_start|>assistant\n{}<|im_end|>\n", content)
                },
                _ => continue,
            };
            segments.push(turn);
        }
            segments.push(assistant_prefix);
            let msgs = msgs.clone();
        let il = il.clone();
        let memh_inf = memh.read().clone();
        let tool_eng = tool_engine.read().clone();
        let h_tool = handle.read().clone();
        tokio::spawn(async move {
            let mut ms = msgs;
            let mut il = il;
            let tool_eng = tool_eng;
            let mut segs_for_reinfer = segments.clone();
            match llama::infer_stream(&h_tool, segments) {
                Ok((mut rx, _)) => {
                    let mut in_thinking = false;
                    let mut full_output = String::new();
                    while let Some(event) = rx.recv().await {
                        match event {
                            llama::StreamEvent::ThinkingStart => { in_thinking = true; }
                            llama::StreamEvent::ThinkingEnd => { in_thinking = false; }
                            llama::StreamEvent::ToolCallBegin | llama::StreamEvent::ToolCallEnd => {}
                            llama::StreamEvent::Token(token) => {
                                ms.with_mut(|v| {
                                    if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                                        if in_thinking { msg.thinking.push_str(&token); }
                                        else { msg.content.push_str(&token); }
                                    }
                                });
                            }
                            llama::StreamEvent::Done(output) => { full_output = output; }
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

            // Check for tool calls using the full output (contains special tokens not in msg.content)
            let parsed = parse_tool_calls(&full_output);

            if !parsed.tool_calls.is_empty() {
                // Strip tool call markup from the assistant message, keep only text
                ms.with_mut(|v| {
                    if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                        msg.content = if parsed.text.is_empty() { String::new() } else { parsed.text.clone() };
                    }
                });

                // Insert ephemeral tool-call messages
                let mut tool_call_ids: Vec<u64> = Vec::new();
                for tc in &parsed.tool_calls {
                    let tc_id = next_msg_id();
                    ms.with_mut(|v| v.push(Message {
                        id: tc_id,
                        role: MessageRole::Assistant,
                        content: String::new(),
                        thinking: String::new(),
                        kind: MessageKind::ToolCall { tool_name: tc.name.clone() },
                        timestamp: now_secs(),
                    }));
                    tool_call_ids.push(tc_id);
                }

                // Execute tool calls
                let mut tool_results = Vec::new();
                for tc in &parsed.tool_calls {
                    let result = tool_eng.execute(tc);
                    tool_results.push(result);
                }

            // Build new prompt with tool results using Qwen3's Hermes format
            segs_for_reinfer.pop(); // remove the assistant prefix
            // Add the assistant's tool calls as a completed turn using proper tokens
            let mut assistant_tool_turn = String::from("<|im_start|>assistant\n");
            for tc in &parsed.tool_calls {
                assistant_tool_turn.push_str("<|tool_call_begin|>\n");
                assistant_tool_turn.push_str(&serde_json::to_string(&serde_json::json!({
                    "name": tc.name,
                    "arguments": tc.arguments
                })).unwrap_or_default());
                assistant_tool_turn.push_str("\n<|tool_call_end|>\n");
            }
            assistant_tool_turn.push_str("<|im_end|>\n");
            segs_for_reinfer.push(assistant_tool_turn);
            // Add tool results in user turn using Qwen3's result tokens
            let mut tool_result_turn = String::from("<|im_start|>user\n");
            for tr in &tool_results {
                tool_result_turn.push_str("<|tool_call_result_begin|>\n");
                tool_result_turn.push_str(&tr.content);
                tool_result_turn.push_str("\n<|tool_call_result_end|>\n");
            }
            tool_result_turn.push_str("<|im_end|>\n");
            segs_for_reinfer.push(tool_result_turn);
            segs_for_reinfer.push("<|im_start|>assistant\n".to_string());

            // Create a new assistant message for the follow-up
                    let aid2 = next_msg_id();
                    ms.with_mut(|v| v.push(Message {
                        id: aid2,
                        role: MessageRole::Assistant,
                        content: String::new(),
                        thinking: String::new(),
                        kind: MessageKind::Text,
                        timestamp: now_secs(),
                    }));

                // Remove ephemeral tool-call messages and the original trigger message
                let ids_to_remove: Vec<u64> = {
                    let mut ids = tool_call_ids.clone();
                    ids.push(aid);
                    ids
                };
                ms.with_mut(|v| v.retain(|m| !ids_to_remove.contains(&m.id)));

                // Force clear KV cache before re-inference to avoid stale state
                // Use block_in_place since we're in a tokio task
                {
                    let h_ref = &h_tool;
                    tokio::task::block_in_place(|| {
                        let _ = llama::clear_kv(h_ref);
                    });
                }

        match llama::infer_stream(&h_tool, segs_for_reinfer) {
            Ok((mut rx2, _)) => {
                let mut in_thinking2 = false;
                let mut full_output2 = String::new();
                while let Some(event) = rx2.recv().await {
                    match event {
                        llama::StreamEvent::ThinkingStart => { in_thinking2 = true; }
                        llama::StreamEvent::ThinkingEnd => { in_thinking2 = false; }
                        llama::StreamEvent::ToolCallBegin | llama::StreamEvent::ToolCallEnd => {}
                        llama::StreamEvent::Token(token) => {
                            ms.with_mut(|v| {
                                if let Some(msg) = v.iter_mut().find(|m| m.id == aid2) {
                                    if in_thinking2 { msg.thinking.push_str(&token); }
                                    else { msg.content.push_str(&token); }
                                }
                            });
                        }
                        llama::StreamEvent::Done(output2) => { full_output2 = output2; }
                        llama::StreamEvent::Error(e) => {
                            ms.with_mut(|v| {
                                if let Some(msg) = v.iter_mut().find(|m| m.id == aid2) {
                                    msg.content = format!("Error: {}", e);
                                }
                            });
                            break;
                        }
                    }
                }
                let parsed2 = parse_tool_calls(&full_output2);
                if !parsed2.tool_calls.is_empty() {
                    ms.with_mut(|v| {
                        if let Some(msg) = v.iter_mut().find(|m| m.id == aid2) {
                            msg.content = if parsed2.text.is_empty() { String::new() } else { parsed2.text.clone() };
                        }
                    });
                }
            let reinf_content = ms.read().iter().find(|m| m.id == aid2).map(|m| m.content.clone()).unwrap_or_default();
            if reinf_content.trim().is_empty() {
                            let fallback = tool_results.iter().map(|r| format!("{}: {}", r.name, r.content)).collect::<Vec<_>>().join("\n");
                            ms.with_mut(|v| {
                                if let Some(msg) = v.iter_mut().find(|m| m.id == aid2) {
                                    msg.content = fallback;
                                }
                            });
                        }
                            let final_msg2 = ms.read().iter().find(|m| m.id == aid2).cloned();
                            if let Some(msg) = final_msg2 {
                                memh_inf.append_message(ChatMessage::from_shared(&msg));
                            }
                        }
                        Err(e) => {
                            ms.with_mut(|v| {
                                if let Some(msg) = v.iter_mut().find(|m| m.id == aid2) {
                                    msg.content = format!("Error in tool follow-up: {}", e);
                                }
                            });
                        }
                    }
                } else {
                    let final_msg = ms.read().iter().find(|m| m.id == aid).cloned();
                    if let Some(msg) = final_msg {
                        memh_inf.append_message(ChatMessage::from_shared(&msg));
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

    use_future(move || async move {
        dioxus::document::eval("document.fonts.ready.then(function(){var el=document.querySelector('.font-loading');if(el){el.classList.replace('font-loading','font-ready');}})").await;
    });

    rsx! {
        style { {TAILWIND_CSS} },
        style { {FONTS_CSS} },
        style { {APP_CSS} },
        style { "html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background: {current_theme.bg()}; color: {current_theme.fg()}; font-family: 'MsgSans', sans-serif; }}" },
        div {
            class: "font-loading flex flex-col fixed inset-0 overflow-hidden",
            style: format!("background: {}; color: {}", current_theme.bg(), current_theme.fg()),
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
target_os = "android"
))]
                    llama::request_stop();
                    il.set(false);
                }
            },
            }
        }
    }
}

#[cfg(any(
all(not(target_arch = "wasm32"), not(target_os = "android")),
target_os = "android"
))]
fn extract_facts(text: &str, mut facts_sig: dioxus::prelude::Signal<Vec<MemoryFact>, dioxus::prelude::SyncStorage>, memh: &dioxus::prelude::Signal<crate::mem::MemvidHandle, dioxus::prelude::SyncStorage>) {
    let lower = text.to_lowercase();
    let patterns: &[(&str, &str, fn(&str)->Option<String>)] = &[
        ("my name is", "name", |v| Some(v.trim().to_string())),
        ("i'm ", "name", |v| {
            let v = v.trim();
            if v.len() < 20 && !v.contains('.') && v.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) { Some(v.to_string()) } else { None }
        }),
        ("i am ", "name", |v| {
            let v = v.trim();
            if v.len() < 20 && !v.contains('.') && v.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) { Some(v.to_string()) } else { None }
        }),
        ("call me ", "name", |v| Some(v.trim().to_string())),
        ("i live in", "location", |v| Some(v.trim().to_string())),
        ("i'm from", "location", |v| Some(v.trim().to_string())),
        ("i work", "occupation", |v| Some(v.trim().to_string())),
        ("my job is", "occupation", |v| Some(v.trim().to_string())),
        ("i'm a ", "occupation", |v| Some(v.trim().to_string())),
        ("my favorite", "preference", |v| Some(v.trim().to_string())),
        ("i prefer", "preference", |v| Some(v.trim().to_string())),
        ("i like", "preference", |v| Some(v.trim().to_string())),
    ];
    let mut new_facts: Vec<MemoryFact> = Vec::new();
    for (trigger, key, extract) in patterns {
        if let Some(idx) = lower.find(trigger) {
            let after = &text[idx + trigger.len()..];
            let val = after.split('.').next().unwrap_or(after);
            if let Some(v) = extract(val) {
                if v.is_empty() { continue; }
                let fact = MemoryFact { key: key.to_string(), value: v };
                facts_sig.with_mut(|facts| {
                    facts.retain(|f| f.key != fact.key);
                    facts.push(fact.clone());
                });
                new_facts.push(fact);
                break;
            }
        }
    }
    if !new_facts.is_empty() {
        let facts = facts_sig.read().clone();
        let snap = ConversationSnapshot {
            next_id: 0,
            messages: Vec::new(),
            facts,
        };
        memh.read().clone().append_snapshot(snap);
    }
}
