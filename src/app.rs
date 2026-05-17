use dioxus::prelude::*;
use std::sync::Arc;
use crate::mem::{self, ChatMessage, ConversationSnapshot, MemoryFact};
#[cfg(any(
all(not(target_arch = "wasm32"), not(target_os = "android")),
target_os = "android"
))]
use crate::llama;
use crate::shared::{Message, MessageRole, MessageKind, LoadingState, Theme, CommandResult, now_secs, next_msg_id, hex_to_rgb, rgb_to_hex, push_system_msg};
use crate::net::{NetRuntime, NetEvent};
use crate::net::relay_inference::{InferenceRequest, InferenceResponse, DeviceCaps};
#[cfg(target_arch = "wasm32")]
use nostr_sdk::ToBech32;
use crate::ui::{self, MessageList, InputArea};
#[cfg(any(
    all(not(target_arch = "wasm32"), not(target_os = "android")),
    target_os = "android"
))]
use crate::system::model;
use crate::system::config::{self, AppConfig};
use crate::system::agent::AgentManager;
use crate::system::app_shell::ShellManager;
#[cfg(any(
all(not(target_arch = "wasm32"), not(target_os = "android")),
target_os = "android"
))]
use crate::tools::{ToolEngine, parse_tool_calls};
#[cfg(not(target_arch = "wasm32"))]
use bip39::Mnemonic;
#[cfg(not(target_arch = "wasm32"))]
use nostr_sdk::{Keys, ToBech32};

const TAILWIND_CSS: &str = include_str!("../assets/tailwind.css");
const FONTS_CSS: &str = include_str!("../assets/fonts.css");
const APP_CSS: &str = include_str!("../assets/app.css");
const PLASMA_JS: &str = include_str!("../assets/plasma.js");

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
    #[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
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
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::eval("navigator.language || 'en-US'")
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_else(|| "en_US".to_string())
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
        "# *THOTH▷*\n\nWelcome! Type to chat with **Tot**, or use:\n\n- `/login <phrase>` — restore your identity from a backup phrase\n- `/backup` — view your backup phrase\n- `/agent` — view or switch agent personality\n- `/shell` — view or switch app context\n- `/groups` — list MLS groups\n- `/invite <pubkey>` — invite to active group\n- `/join <group_id>` — accept a pending invite\n- `/members` — list group members\n- `/system` — view or set the system prompt\n- `/theme` — toggle dark/light mode\n- `/plasma` — configure background shader\n- `/plasma color` — pick shader colors\n- `/blend` — set text blend mode over shader\n- `/fullscreen` — toggle fullscreen (or F11)".to_string()
    } else {
        "# *THOTH▷*".to_string()
    }
}

fn save_plasma_config(sig: &dioxus::prelude::Signal<config::PlasmaConfig, dioxus::prelude::SyncStorage>) {
    let pc = sig.read().clone();
    let path = config::get_config_path();
    let mut cfg = config::AppConfig::load(&path).unwrap_or_default();
    cfg.plasma = pc;
    let _ = cfg.save(&path);
}

pub fn App() -> Element {
    let theme = use_signal_sync(|| Theme::Dark);
    let messages = use_signal_sync(|| Vec::<Message>::new());
    let next_id = use_signal_sync(|| 0u64);
    let mut is_loading = use_signal_sync(|| false);
    let loading_state = use_signal_sync(|| LoadingState::Ready);
    let input = use_signal(|| String::new());
    let mut at_bottom = use_signal(|| true);
    let mut has_new = use_signal(|| false);
    let facts = use_signal_sync(|| Vec::<MemoryFact>::new());
    let mut system_prompt = use_signal_sync(|| {
        let lang = detect_language();
        default_system_prompt(&lang)
    });
    let mut plasma_config = use_signal_sync(|| {
        config::AppConfig::load(&config::get_config_path())
            .map(|c| c.plasma.clone())
            .unwrap_or_default()
    });

    let mut agent_manager = use_signal_sync(|| AgentManager::new());
    let mut shell_manager = use_signal_sync(|| ShellManager::new());

    let net_runtime = use_signal_sync(|| {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        Arc::new(NetRuntime::new(tx))
    });
    let mut net_connected = use_signal_sync(|| false);

    #[cfg(target_arch = "wasm32")]
    let _net_fut = {
        let mut net_conn = net_connected.clone();
        let mut msgs = messages.clone();
        let mut nid = next_id.clone();
        let net = net_runtime.clone();
        spawn(async move {
            let rt = net.read().clone();
    let own_pk: std::sync::Arc<std::sync::Mutex<Option<String>>> = std::sync::Arc::new(std::sync::Mutex::new(None));
    let own_pk_cb = own_pk.clone();
    let own_pk_fetch = own_pk.clone();
    rt.set_event_callback({
        let mut msgs = msgs.clone();
        let mut nid = nid.clone();
        move |ev: NetEvent| {
            match ev {
                NetEvent::NostrMessage { sender, content } => {
                    if let Ok(guard) = own_pk_cb.lock() {
                        if guard.as_ref() == Some(&sender) { return; }
                    }
                    let id = nid.peek().clone();
                    nid.set(id + 1);
                    msgs.with_mut(|v| v.push(Message {
                        id,
                        role: MessageRole::Peer,
                        content: content.clone(),
                        thinking: String::new(),
                        kind: MessageKind::NostrDm { sender_pubkey: sender.clone() },
                        sender: sender.chars().take(12).collect::<String>(),
                        timestamp: now_secs(),
                    }));
                }
                    NetEvent::GroupText { sender, content, .. } => {
                        let id = nid.peek().clone();
                        nid.set(id + 1);
                        msgs.with_mut(|v| v.push(Message {
                            id,
                            role: MessageRole::Peer,
                            content,
                            thinking: String::new(),
                            kind: MessageKind::NostrDm { sender_pubkey: sender.clone() },
                            sender: format!("peer:{}", sender.chars().take(8).collect::<String>()),
                            timestamp: now_secs(),
                        }));
                    }
                    NetEvent::DeviceDiscovered(caps) => {
                        push_system_msg(&mut msgs, &mut nid, format!("Device discovered: {} (score={:.0})", caps.device_name, caps.score()), MessageKind::Text);
                    }
                    NetEvent::MlsInvite { sender, group_id, .. } => {
                        push_system_msg(&mut msgs, &mut nid, format!("MLS invite from {} for group `{}`. Type `/join {}` to accept.", sender.chars().take(8).collect::<String>(), group_id, group_id), MessageKind::Text);
                    }
                    NetEvent::GroupJoined { group_id } => {
                        push_system_msg(&mut msgs, &mut nid, format!("Joined MLS group `{}`", group_id), MessageKind::Text);
                    }
                    _ => {}
            }
        }
    });
    match rt.start().await {
        Ok(()) => {
                net_conn.set(true);
                push_system_msg(&mut msgs, &mut nid, "Connected to Nostr network.".to_string(), MessageKind::Text);
                if let Some(pk) = rt.public_key_hex().await {
                    push_system_msg(&mut msgs, &mut nid, format!("Your public key: `{}`", pk), MessageKind::Text);
                    if let Ok(mut guard) = own_pk_fetch.lock() {
                        *guard = Some(pk.clone());
                    }
                    let caps = crate::net::relay_inference::local_device_caps(
                        &crate::shared::get_hostname(), &pk, 0, None,
                    );
                    let _ = rt.advertise_caps(caps).await;
                }
            }
        Err(e) => {
            push_system_msg(&mut msgs, &mut nid, format!("Failed to connect: {}", e), MessageKind::Text);
        }
    }
})
    };

    #[cfg(not(target_arch = "wasm32"))]
    let _net_cb = {
        let net = net_runtime.clone();
        let mut msgs = messages.clone();
        let mut nid = next_id.clone();
        let mut net_conn = net_connected.clone();
        let rt = net.read().clone();
        let own_pk: std::sync::Arc<std::sync::Mutex<Option<String>>> = std::sync::Arc::new(std::sync::Mutex::new(None));
        let own_pk_cb = own_pk.clone();
        let own_pk_fetch = own_pk.clone();
        rt.set_event_callback(move |ev: NetEvent| {
            match ev {
                NetEvent::NostrMessage { sender, content } => {
                    if let Ok(guard) = own_pk_cb.lock() {
                        if guard.as_ref() == Some(&sender) { return; }
                    }
                    let id = nid.peek().clone();
                    nid.set(id + 1);
                    msgs.with_mut(|v| v.push(Message {
                        id,
                        role: MessageRole::Peer,
                        content: content.clone(),
                        thinking: String::new(),
                        kind: MessageKind::NostrDm { sender_pubkey: sender.clone() },
                        sender: sender.chars().take(12).collect::<String>(),
                        timestamp: now_secs(),
                    }));
                }
            NetEvent::GroupText { sender, content, .. } => {
                let id = nid.peek().clone();
                nid.set(id + 1);
                msgs.with_mut(|v| v.push(Message {
                    id,
                    role: MessageRole::Peer,
                    content,
                    thinking: String::new(),
                    kind: MessageKind::NostrDm { sender_pubkey: sender.clone() },
                    sender: format!("peer:{}", sender.chars().take(8).collect::<String>()),
                    timestamp: now_secs(),
                }));
            }
            NetEvent::DeviceDiscovered(caps) => {
                push_system_msg(&mut msgs, &mut nid, format!("Device discovered: {} (score={:.0})", caps.device_name, caps.score()), MessageKind::Text);
            }
            NetEvent::MlsInvite { sender, group_id, .. } => {
                push_system_msg(&mut msgs, &mut nid, format!("MLS invite from {} for group `{}`. Type `/join {}` to accept.", sender.chars().take(8).collect::<String>(), group_id, group_id), MessageKind::Text);
            }
            NetEvent::GroupJoined { group_id } => {
                push_system_msg(&mut msgs, &mut nid, format!("Joined MLS group `{}`", group_id), MessageKind::Text);
            }
            _ => {}
            }
        });
        let rt_start = rt.clone();
        tokio::spawn(async move {
            match rt_start.start().await {
        Ok(()) => {
                net_conn.set(true);
                push_system_msg(&mut msgs, &mut nid, "Connected to Nostr network.".to_string(), MessageKind::Text);
                if let Some(pk) = rt_start.public_key_hex().await {
                    push_system_msg(&mut msgs, &mut nid, format!("Your public key: `{}`", pk), MessageKind::Text);
                    if let Ok(mut guard) = own_pk_fetch.lock() {
                        *guard = Some(pk.clone());
                    }
                    let gpu: u32 = {
                        #[cfg(not(target_arch = "wasm32"))]
                        { 99 }
                        #[cfg(target_arch = "wasm32")]
                        { 0 }
                    };
                    let model: Option<String> = None;
                    let caps = crate::net::relay_inference::local_device_caps(
                        &crate::shared::get_hostname(), &pk, gpu, model,
                    );
                    let _ = rt_start.advertise_caps(caps).await;
                }
                }
                Err(e) => {
                    push_system_msg(&mut msgs, &mut nid, format!("Failed to connect: {}", e), MessageKind::Text);
                }
            }
        });
    };

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
            crate::shared::sleep_ms(100).await;
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
                            sender: String::new(),
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
                        sender: String::new(),
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
                sender: String::new(),
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
    let pc = plasma_config.clone();
    let net = net_runtime.clone();
    let ls = loading_state.clone();
    let am = agent_manager.clone();
    let sm = shell_manager.clone();
    move |input: String| {
        let mut msgs = msgs;
        let mut nid = nid;
        let mut il = il;
        let mut t = t;
        let memh = memh;
        let mut facts_sig = facts_sig;
        let tool_engine = tool_engine_c;
        let mut plasma_cfg = pc;
        let mut system_prompt_sig = sp;
        let mut agent_mgr = am;
        let mut shell_mgr = sm;
        let mut plasma_cfg = plasma_cfg;
        let trimmed = input.trim().to_string();
            if trimmed.is_empty() { return; }

        if trimmed.starts_with("/theme") { t.set(t().toggle()); return; }
        if trimmed.starts_with("/light") { t.set(Theme::Light); return; }
        if trimmed.starts_with("/dark") { t.set(Theme::Dark); return; }
if trimmed.starts_with("/plasma") {
let rest = trimmed["/plasma".len()..].trim();
if rest.is_empty() || rest == "show" {
let pc = plasma_cfg.read().clone();
let dc = &pc.dark_colors;
let lc = &pc.light_colors;
let msg_id = nid();
nid.set(msg_id + 1);
let _ = msgs.with_mut(|v| v.push(Message {
id: msg_id, role: MessageRole::System,
content: format!(
"**Plasma shader:**\n- enabled: {}\n- speed: {}\n- dark colors: [{:.2},{:.2},{:.2}] [{:.2},{:.2},{:.2}] [{:.2},{:.2},{:.2}]\n- light colors: [{:.2},{:.2},{:.2}] [{:.2},{:.2},{:.2}] [{:.2},{:.2},{:.2}]\n\nUsage: `/plasma on|off` · `/plasma speed <0.1-5.0>` · `/plasma color [dark|light]` · `/plasma reset`",
pc.enabled, pc.speed,
dc[0],dc[1],dc[2], dc[3],dc[4],dc[5], dc[6],dc[7],dc[8],
lc[0],lc[1],lc[2], lc[3],lc[4],lc[5], lc[6],lc[7],lc[8],
),
thinking: String::new(), kind: MessageKind::Text, sender: String::new(),
            timestamp: now_secs(),
}));
} else if rest == "on" {
plasma_cfg.with_mut(|p| p.enabled = true);
save_plasma_config(&plasma_cfg);
push_system_msg(&mut msgs, &mut nid, "Plasma enabled.".to_string(), MessageKind::Text);
} else if rest == "off" {
plasma_cfg.with_mut(|p| p.enabled = false);
save_plasma_config(&plasma_cfg);
push_system_msg(&mut msgs, &mut nid, "Plasma disabled.".to_string(), MessageKind::Text);
} else if rest.starts_with("speed") {
let val: Result<f32, _> = rest["speed".len()..].trim().parse();
match val {
Ok(s) if s >= 0.1 && s <= 5.0 => {
plasma_cfg.with_mut(|p| p.speed = s);
save_plasma_config(&plasma_cfg);
push_system_msg(&mut msgs, &mut nid, format!("Plasma speed set to {:.1}.", s), MessageKind::Text);
}
_ => {
push_system_msg(&mut msgs, &mut nid, "Usage: `/plasma speed <0.1-5.0>`".to_string(), MessageKind::Text);
}
}
} else if rest.starts_with("color") {
let target = rest["color".len()..].trim();
let is_dark = t() == Theme::Dark;
let theme_label = if target.is_empty() {
if is_dark { "dark" } else { "light" }
} else if target == "dark" {
"dark"
} else if target == "light" {
"light"
} else {
push_system_msg(&mut msgs, &mut nid, "Usage: `/plasma color [dark|light]`".to_string(), MessageKind::Text);
return;
};
let color_idx = if theme_label == "dark" { 0usize } else { 1usize };
let pc = plasma_cfg.read().clone();
let colors = if theme_label == "dark" { &pc.dark_colors } else { &pc.light_colors };
let c1_hex = rgb_to_hex(colors[0], colors[1], colors[2]);
let c2_hex = rgb_to_hex(colors[3], colors[4], colors[5]);
let c3_hex = rgb_to_hex(colors[6], colors[7], colors[8]);
push_system_msg(&mut msgs, &mut nid, format!("**{theme_label}** c1 (color 1):"), MessageKind::ColorRequest { color_index: color_idx * 3 + 0, tag: theme_label.to_string(), initial_hex: c1_hex });
push_system_msg(&mut msgs, &mut nid, format!("**{theme_label}** c2 (color 2):"), MessageKind::ColorRequest { color_index: color_idx * 3 + 1, tag: theme_label.to_string(), initial_hex: c2_hex });
push_system_msg(&mut msgs, &mut nid, format!("**{theme_label}** c3 (color 3):"), MessageKind::ColorRequest { color_index: color_idx * 3 + 2, tag: theme_label.to_string(), initial_hex: c3_hex });
} else if rest == "reset" {
plasma_cfg.set(config::PlasmaConfig::default());
save_plasma_config(&plasma_cfg);
push_system_msg(&mut msgs, &mut nid, "Plasma config reset to defaults.".to_string(), MessageKind::Text);
}
return;
}
        if trimmed.starts_with("/blend") {
            let rest = trimmed["/blend".len()..].trim();
            let is_dark = t() == Theme::Dark;
            let theme_label = if is_dark { "dark" } else { "light" };
            if rest.is_empty() || rest == "show" {
                let pc = plasma_cfg.read().clone();
                let current = if is_dark { &pc.dark_blend } else { &pc.light_blend };
                let modes = config::BLEND_MODES.join(" · ");
                let msg_id = nid();
                nid.set(msg_id + 1);
                let _ = msgs.with_mut(|v| v.push(Message {
                    id: msg_id, role: MessageRole::System,
                    content: format!("**Blend mode ({theme_label}):** `{current}`\n**Dark:** `{}` · **Light:** `{}`\n\nAvailable:\n{modes}\n\nUsage: `/blend <mode>` — sets for current theme\n`/blend reset` — reset current theme to default", pc.dark_blend, pc.light_blend),
                    thinking: String::new(), kind: MessageKind::Text, sender: String::new(),
            timestamp: now_secs(),
                }));
            } else if rest == "reset" {
                let default = if is_dark { config::default_dark_blend() } else { config::default_light_blend() };
                plasma_cfg.with_mut(|p| {
                    if is_dark { p.dark_blend = default.clone(); } else { p.light_blend = default.clone(); }
                });
                save_plasma_config(&plasma_cfg);
                let msg_id = nid();
                nid.set(msg_id + 1);
                let _ = msgs.with_mut(|v| v.push(Message {
                    id: msg_id, role: MessageRole::System,
                    content: format!("Blend mode ({theme_label}) reset to `{default}`."),
                    thinking: String::new(), kind: MessageKind::Text, sender: String::new(),
            timestamp: now_secs(),
                }));
            } else if config::BLEND_MODES.contains(&rest) {
                plasma_cfg.with_mut(|p| {
                    if is_dark { p.dark_blend = rest.to_string(); } else { p.light_blend = rest.to_string(); }
                });
                save_plasma_config(&plasma_cfg);
                let msg_id = nid();
                nid.set(msg_id + 1);
                let _ = msgs.with_mut(|v| v.push(Message {
                    id: msg_id, role: MessageRole::System,
                    content: format!("Blend mode ({theme_label}) set to `{rest}`."),
                    thinking: String::new(), kind: MessageKind::Text, sender: String::new(),
            timestamp: now_secs(),
                }));
            } else {
                let msg_id = nid();
                nid.set(msg_id + 1);
                let _ = msgs.with_mut(|v| v.push(Message {
                    id: msg_id, role: MessageRole::System,
                    content: format!("Unknown blend mode `{rest}`. Type `/blend` to see available modes."),
                    thinking: String::new(), kind: MessageKind::Text, sender: String::new(),
            timestamp: now_secs(),
                }));
            }
        return;
    }
    if trimmed == "/fullscreen" {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        {
            let win = dioxus::desktop::window();
            let was_fullscreen = win.fullscreen().is_some();
            win.set_fullscreen(!was_fullscreen);
            push_system_msg(&mut msgs, &mut nid, if was_fullscreen { "Exited fullscreen.".to_string() } else { "Entered fullscreen. Press F11 or `/fullscreen` to exit.".to_string() }, MessageKind::Text);
        }
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        {
            push_system_msg(&mut msgs, &mut nid, "Fullscreen not available on this platform.".to_string(), MessageKind::Text);
        }
        return;
    }
    if trimmed.starts_with("/system") {
        let rest = trimmed["/system".len()..].trim();
            if rest.is_empty() || rest == "show" {
                let current = system_prompt_sig.read().clone();
                let msg_id = nid();
                nid.set(msg_id + 1);
                let _ = msgs.with_mut(|v| v.push(Message {
                    id: msg_id, role: MessageRole::System,
                    content: format!("**System prompt:**\n```\n{}\n```", current),
                    thinking: String::new(), kind: MessageKind::Text, sender: String::new(),
            timestamp: now_secs(),
                }));
            } else if rest == "reset" {
                system_prompt_sig.set(default_system_prompt(&detect_language()));
                let msg_id = nid();
                nid.set(msg_id + 1);
                let _ = msgs.with_mut(|v| v.push(Message {
                    id: msg_id, role: MessageRole::System,
                    content: "System prompt reset to default.".to_string(),
                    thinking: String::new(), kind: MessageKind::Text, sender: String::new(),
            timestamp: now_secs(),
                }));
            } else {
                system_prompt.set(rest.to_string());
                let msg_id = nid();
                nid.set(msg_id + 1);
                let _ = msgs.with_mut(|v| v.push(Message {
                    id: msg_id, role: MessageRole::System,
                    content: format!("System prompt updated:\n```\n{}\n```", rest),
                    thinking: String::new(), kind: MessageKind::Text, sender: String::new(),
            timestamp: now_secs(),
                }));
        }
        return;
    }

    if trimmed.starts_with("/agent") {
        let rest = trimmed["/agent".len()..].trim();
        if rest.is_empty() || rest == "show" {
            let entries = agent_mgr.read().list_display();
            let mut lines = vec!["**Agents:**\n".to_string()];
            for (name, desc, active) in &entries {
                let marker = if *active { " ◀" } else { "" };
                lines.push(format!("- **{name}** — {desc}{marker}"));
            }
            lines.push("\nUsage: `/agent <name>` to switch".to_string());
            push_system_msg(&mut msgs, &mut nid, lines.join("\n"), MessageKind::Text);
        } else if let Ok(agent) = agent_mgr.write().switch(rest) {
            system_prompt_sig.set(agent.system_prompt.clone());
            push_system_msg(&mut msgs, &mut nid, format!("Switched to agent **{}** ({})", agent.agent_name, agent.name), MessageKind::Text);
        } else {
            push_system_msg(&mut msgs, &mut nid, format!("Unknown agent: `{}`. Type `/agent` to see available.", rest), MessageKind::Text);
        }
        return;
    }

    if trimmed.starts_with("/shell") {
        let rest = trimmed["/shell".len()..].trim();
        if rest.is_empty() || rest == "show" {
            let entries = shell_mgr.read().list_display();
            let mut lines = vec!["**App Shells:**\n".to_string()];
            for (name, desc, icon, active) in &entries {
                let marker = if *active { " ◀" } else { "" };
                lines.push(format!("- {icon} **{name}** — {desc}{marker}"));
            }
            lines.push("\nUsage: `/shell <name>` to switch".to_string());
            push_system_msg(&mut msgs, &mut nid, lines.join("\n"), MessageKind::Text);
        } else if let Ok(shell) = shell_mgr.write().switch(rest) {
            let agent = agent_mgr.read().active().clone();
            if let Some(a) = agent_mgr.read().agents.get(&shell.agent) {
                system_prompt_sig.set(a.system_prompt.clone());
            }
            push_system_msg(&mut msgs, &mut nid, format!("Switched to {icon} **{name}** — {desc}", icon=shell.icon, name=shell.name, desc=shell.description), MessageKind::Text);
        } else {
            push_system_msg(&mut msgs, &mut nid, format!("Unknown shell: `{}`. Type `/shell` to see available.", rest), MessageKind::Text);
        }
        return;
    }

    if trimmed.starts_with("/invite") {
        let rest = trimmed["/invite".len()..].trim();
        if rest.is_empty() {
            push_system_msg(&mut msgs, &mut nid, "Usage: `/invite <pubkey_hex>` — invite user to the active shell's MLS group".to_string(), MessageKind::Text);
        } else {
            let active_shell = shell_mgr.read().active().clone();
            let group_id = active_shell.channels.first().cloned().unwrap_or_else(|| "devices".to_string());
            let net_c = net.clone();
            let gid = group_id.clone();
            let pk = rest.to_string();
            let mut ms_inv = msgs.clone();
            let mut nid_inv = nid.clone();
            spawn(async move {
                let rt = net_c.read().clone();
                if rt.is_running().await {
                    match rt.invite_to_group(&gid, &pk).await {
                        Ok(()) => {
                            push_system_msg(&mut ms_inv, &mut nid_inv, format!("Invited `{}` to group `{}`", pk.chars().take(12).collect::<String>(), gid), MessageKind::Text);
                        }
                        Err(e) => {
                            push_system_msg(&mut ms_inv, &mut nid_inv, format!("Invite failed: {}", e), MessageKind::Text);
                        }
                    }
                } else {
                    push_system_msg(&mut ms_inv, &mut nid_inv, "Not connected to Nostr.".to_string(), MessageKind::Text);
                }
            });
        }
        return;
    }

    if trimmed.starts_with("/join") {
        let rest = trimmed["/join".len()..].trim();
        if rest.is_empty() {
            let net_c = net.clone();
            let mut ms_j = msgs.clone();
            let mut nid_j = nid.clone();
            spawn(async move {
                let rt = net_c.read().clone();
                let invites = rt.pending_invites().await;
                if invites.is_empty() {
                    push_system_msg(&mut ms_j, &mut nid_j, "No pending MLS invites. Someone must `/invite` you first.".to_string(), MessageKind::Text);
                } else {
                    let mut lines = vec!["**Pending invites:**\n".to_string()];
                    for inv in &invites {
                        lines.push(format!("- Group `{}` from `{}` (received {})", inv.group_id, inv.sender.chars().take(8).collect::<String>(), inv.received_at));
                    }
                    lines.push("\nUsage: `/join <group_id>` to accept".to_string());
                    push_system_msg(&mut ms_j, &mut nid_j, lines.join("\n"), MessageKind::Text);
                }
            });
        } else {
            let net_c = net.clone();
            let gid = rest.to_string();
            let mut ms_j = msgs.clone();
            let mut nid_j = nid.clone();
            spawn(async move {
                let rt = net_c.read().clone();
                match rt.join_pending_invite(&gid).await {
                    Ok(joined_id) => {
                        push_system_msg(&mut ms_j, &mut nid_j, format!("Joined MLS group `{}`", joined_id), MessageKind::Text);
                    }
                    Err(e) => {
                        push_system_msg(&mut ms_j, &mut nid_j, format!("Join failed: {}", e), MessageKind::Text);
                    }
                }
            });
        }
        return;
    }

    if trimmed == "/members" {
        let active_shell = shell_mgr.read().active().clone();
        let group_id = active_shell.channels.first().cloned().unwrap_or_else(|| "devices".to_string());
        let net_c = net.clone();
        let gid = group_id.clone();
        let mut ms_m = msgs.clone();
        let mut nid_m = nid.clone();
        spawn(async move {
            let rt = net_c.read().clone();
            match rt.get_group_members(&gid).await {
                Ok(members) => {
                    if members.is_empty() {
                        push_system_msg(&mut ms_m, &mut nid_m, format!("No members found in group `{}`.", gid), MessageKind::Text);
                    } else {
                        let list: Vec<String> = members.iter().map(|m| format!("- `{}`", m.chars().take(12).collect::<String>())).collect();
                        push_system_msg(&mut ms_m, &mut nid_m, format!("**Members of `{}`:**\n{}", gid, list.join("\n")), MessageKind::Text);
                    }
                }
                Err(e) => {
                    push_system_msg(&mut ms_m, &mut nid_m, format!("Error: {}", e), MessageKind::Text);
                }
            }
        });
        return;
    }

    if trimmed == "/groups" {
        let net_c = net.clone();
        let mut ms_g = msgs.clone();
        let mut nid_g = nid.clone();
        spawn(async move {
            let rt = net_c.read().clone();
            let groups = rt.list_groups().await;
            if groups.is_empty() {
                push_system_msg(&mut ms_g, &mut nid_g, "No MLS groups yet. Use `/invite` to add members or switch to a shell with `/shell`.".to_string(), MessageKind::Text);
            } else {
                let mut lines = vec!["**MLS Groups:**\n".to_string()];
                for g in &groups {
                    let type_label = match &g.group_type {
                        crate::net::group_registry::GroupType::Device => "device",
                        crate::net::group_registry::GroupType::Chat => "chat",
                        crate::net::group_registry::GroupType::Shell { shell_name } => shell_name,
                    };
                    lines.push(format!("- `{}` [{}] {} members — {}", g.group_id, type_label, g.members.len(), g.name));
                }
                push_system_msg(&mut ms_g, &mut nid_g, lines.join("\n"), MessageKind::Text);
            }
        });
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
            sender: String::new(),
            timestamp: now_secs(),
        };
        msgs.with_mut(|v| v.push(welcome.clone()));
        memh.read().clone().append_snapshot(ConversationSnapshot { next_id: msg_id + 1, messages: vec![ChatMessage::from_shared(&welcome)], facts: Vec::new() });
        let _ = memh.read().clone().compact();
        facts_sig.set(Vec::new());
        return;
    }

        if trimmed == "/relays" {
            let net = net.clone();
            let mut msgs = msgs.clone();
            let mut nid = nid.clone();
            spawn(async move {
                let rt = net.read().clone();
                let statuses = rt.relay_statuses().await;
                let mut lines = vec!["**Nostr Relay Status:**\n".to_string()];
                for (url, status) in &statuses {
                    let icon = if status == "connected" { "\u{2705}" } else { "\u{274c}" };
                    lines.push(format!("{} `{}` — {}", icon, url, status));
                }
                if statuses.is_empty() {
                    lines.push("No relays configured.".to_string());
                }
                push_system_msg(&mut msgs, &mut nid, lines.join("\n"), MessageKind::Text);
            });
            return;
        }

        if trimmed.starts_with("/relay_add ") {
            let url = trimmed["/relay_add ".len()..].trim().to_string();
            if !url.is_empty() {
                let net = net.clone();
                let mut msgs = msgs.clone();
                let mut nid = nid.clone();
                spawn(async move {
                    let rt = net.read().clone();
                    match rt.add_relay(&url).await {
                        Ok(()) => { push_system_msg(&mut msgs, &mut nid, format!("Added relay: `{url}`"), MessageKind::Text); }
                        Err(e) => { push_system_msg(&mut msgs, &mut nid, format!("Failed to add relay: {e}"), MessageKind::Text); }
                    }
                });
            }
            return;
        }

        if trimmed.starts_with("/relay_rm ") {
            let url = trimmed["/relay_rm ".len()..].trim().to_string();
            if !url.is_empty() {
                let net = net.clone();
                let mut msgs = msgs.clone();
                let mut nid = nid.clone();
                spawn(async move {
                    let rt = net.read().clone();
                    match rt.remove_relay(&url).await {
                        Ok(()) => { push_system_msg(&mut msgs, &mut nid, format!("Removed relay: `{url}`"), MessageKind::Text); }
                        Err(e) => { push_system_msg(&mut msgs, &mut nid, format!("Failed to remove relay: {e}"), MessageKind::Text); }
                    }
                });
            }
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
                sender: String::new(),
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
                sender: String::new(),
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
                sender: String::new(),
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
                sender: String::new(),
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
                sender: String::new(),
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
                sender: String::new(),
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
                sender: String::new(),
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
                sender: String::new(),
            timestamp: now_secs(),
                        }));
                    }
                }
        return;
        }

    let uid = nid();
    nid.set(uid + 1);
    let user_msg = Message { id: uid, role: MessageRole::User, content: trimmed.clone(), thinking: String::new(), kind: MessageKind::Text, sender: "You".to_string(), timestamp: now_secs() };
    msgs.with_mut(|v| v.push(user_msg.clone()));
    memh.read().clone().append_message(ChatMessage::from_shared(&user_msg));

    extract_facts(&trimmed, facts_sig.clone(), &memh);

    let active_shell = shell_mgr.read().active().clone();

    match active_shell.publish_mode {
        crate::system::app_shell::PublishMode::Public => {
            let net_c = net.clone();
            let content = trimmed.clone();
            spawn(async move {
                let rt = net_c.read().clone();
                if rt.is_running().await {
                    if let Err(e) = rt.publish_text_note(content).await {
                        tracing::warn!("Failed to publish to Nostr: {}", e);
                    }
                }
            });
        }
        crate::system::app_shell::PublishMode::Private => {
            let net_c = net.clone();
            let content = trimmed.clone();
            let shell_name = shell_mgr.read().active().name.clone();
            spawn(async move {
                let rt = net_c.read().clone();
                if rt.is_running().await {
                    if let Some(pk) = rt.own_pubkey().await {
                        if let Err(e) = rt.send_shell_text(&pk, &shell_name, content).await {
                            tracing::warn!("Failed to send MLS group text: {}", e);
                        }
                    }
                }
            });
        }
        crate::system::app_shell::PublishMode::Local => {}
    }

    // @Tot prefix triggers local inference (private, never published)
        if trimmed.to_lowercase().starts_with("@tot ") {
            let query = trimmed[5..].trim().to_string();
            if query.is_empty() {
                push_system_msg(&mut msgs, &mut nid, "Usage: `@Tot <your question>`".to_string(), MessageKind::Text);
            } else if ls.read().clone() != LoadingState::Ready {
                push_system_msg(&mut msgs, &mut nid, "Model not loaded yet. Wait for it to finish loading, or check the model path.".to_string(), MessageKind::Text);
        } else {
                let aid = nid();
                nid.set(aid + 1);
                let asst_msg = Message { id: aid, role: MessageRole::Assistant, content: String::new(), thinking: String::new(), kind: MessageKind::Text, sender: "Tot".to_string(), timestamp: now_secs() };
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
        let net_resp = net.clone();
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
                        sender: String::new(),
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
                        sender: String::new(),
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
                                let rt = net_resp.read().clone();
                                if rt.is_running().await {
                                    let _ = rt.publish_text_note(msg.content.clone()).await;
                                }
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
                            let rt = net_resp.read().clone();
                            if rt.is_running().await {
                                let _ = rt.publish_text_note(msg.content.clone()).await;
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
            }
        }
    };

#[cfg(target_arch = "wasm32")]
{
    let mut msgs = messages.clone();
    let mut nid = next_id.clone();
    if msgs.read().is_empty() {
        let id = nid();
        nid.set(id + 1);
        msgs.with_mut(|v| v.push(Message {
            id, role: MessageRole::System,
            content: splash_content(config::needs_onboarding()),
            thinking: String::new(), kind: MessageKind::Text,
            sender: String::new(),
            timestamp: now_secs(),
        }));
    }
}

#[cfg(target_arch = "wasm32")]
let mut process_input = {
    let msgs = messages.clone();
    let nid = next_id.clone();
    let il = is_loading.clone();
    let t = theme.clone();
    let sp = system_prompt.clone();
    let mut pc = plasma_config.clone();
    let net = net_runtime.clone();
    let net_conn = net_connected.clone();
    let am = agent_manager.clone();
    let sm = shell_manager.clone();
    move |input: String| {
        let mut msgs = msgs;
        let mut nid = nid;
        let mut il = il;
        let mut t = t;
        let mut system_prompt_sig = sp;
        let mut plasma_cfg = pc;
        let mut agent_mgr = am;
        let mut shell_mgr = sm;
        let trimmed = input.trim().to_string();
        if trimmed.is_empty() { return; }

        if trimmed.starts_with("/theme") { t.set(t().toggle()); return; }
        if trimmed.starts_with("/light") { t.set(Theme::Light); return; }
        if trimmed.starts_with("/dark") { t.set(Theme::Dark); return; }
        if trimmed.starts_with("/plasma") {
            let rest = trimmed["/plasma".len()..].trim();
            if rest.is_empty() || rest == "show" {
                let pc = plasma_cfg.read().clone();
                let dc = &pc.dark_colors;
                let lc = &pc.light_colors;
                push_system_msg(&mut msgs, &mut nid, format!(
                    "**Plasma shader:**\n- enabled: {}\n- speed: {}\n- dark colors: [{:.2},{:.2},{:.2}] [{:.2},{:.2},{:.2}] [{:.2},{:.2},{:.2}]\n- light colors: [{:.2},{:.2},{:.2}] [{:.2},{:.2},{:.2}] [{:.2},{:.2},{:.2}]\n\nUsage: `/plasma on|off` · `/plasma speed <0.1-5.0>` · `/plasma color [dark|light]` · `/plasma reset`",
                    pc.enabled, pc.speed,
                    dc[0],dc[1],dc[2], dc[3],dc[4],dc[5], dc[6],dc[7],dc[8],
                    lc[0],lc[1],lc[2], lc[3],lc[4],lc[5], lc[6],lc[7],lc[8],
                ), MessageKind::Text);
            } else if rest == "on" {
                plasma_cfg.with_mut(|p| p.enabled = true);
                save_plasma_config(&plasma_cfg);
                push_system_msg(&mut msgs, &mut nid, "Plasma enabled.".to_string(), MessageKind::Text);
            } else if rest == "off" {
                plasma_cfg.with_mut(|p| p.enabled = false);
                save_plasma_config(&plasma_cfg);
                push_system_msg(&mut msgs, &mut nid, "Plasma disabled.".to_string(), MessageKind::Text);
            } else if rest.starts_with("speed") {
                let val: Result<f32, _> = rest["speed".len()..].trim().parse();
                match val {
                    Ok(s) if s >= 0.1 && s <= 5.0 => {
                        plasma_cfg.with_mut(|p| p.speed = s);
                        save_plasma_config(&plasma_cfg);
                        push_system_msg(&mut msgs, &mut nid, format!("Plasma speed set to {:.1}.", s), MessageKind::Text);
                    }
                    _ => { push_system_msg(&mut msgs, &mut nid, "Usage: `/plasma speed <0.1-5.0>`".to_string(), MessageKind::Text); }
                }
            } else if rest.starts_with("color") {
                let target = rest["color".len()..].trim();
                let is_dark = t() == Theme::Dark;
                let theme_label = if target.is_empty() {
                    if is_dark { "dark" } else { "light" }
                } else if target == "dark" { "dark" }
                else if target == "light" { "light" }
                else { push_system_msg(&mut msgs, &mut nid, "Usage: `/plasma color [dark|light]`".to_string(), MessageKind::Text); return; };
                let color_idx = if theme_label == "dark" { 0usize } else { 1usize };
                let pc_r = plasma_cfg.read().clone();
                let colors = if theme_label == "dark" { &pc_r.dark_colors } else { &pc_r.light_colors };
                let c1_hex = rgb_to_hex(colors[0], colors[1], colors[2]);
                let c2_hex = rgb_to_hex(colors[3], colors[4], colors[5]);
                let c3_hex = rgb_to_hex(colors[6], colors[7], colors[8]);
                push_system_msg(&mut msgs, &mut nid, format!("**{theme_label}** c1 (color 1):"), MessageKind::ColorRequest { color_index: color_idx * 3 + 0, tag: theme_label.to_string(), initial_hex: c1_hex });
                push_system_msg(&mut msgs, &mut nid, format!("**{theme_label}** c2 (color 2):"), MessageKind::ColorRequest { color_index: color_idx * 3 + 1, tag: theme_label.to_string(), initial_hex: c2_hex });
                push_system_msg(&mut msgs, &mut nid, format!("**{theme_label}** c3 (color 3):"), MessageKind::ColorRequest { color_index: color_idx * 3 + 2, tag: theme_label.to_string(), initial_hex: c3_hex });
            } else if rest == "reset" {
                plasma_cfg.set(config::PlasmaConfig::default());
                save_plasma_config(&plasma_cfg);
                push_system_msg(&mut msgs, &mut nid, "Plasma config reset to defaults.".to_string(), MessageKind::Text);
            }
            return;
        }
        if trimmed.starts_with("/blend") {
            let rest = trimmed["/blend".len()..].trim();
            let is_dark = t() == Theme::Dark;
            let theme_label = if is_dark { "dark" } else { "light" };
            if rest.is_empty() || rest == "show" {
                let pc_r = plasma_cfg.read().clone();
                let current = if is_dark { &pc_r.dark_blend } else { &pc_r.light_blend };
                let modes = config::BLEND_MODES.join(" · ");
                push_system_msg(&mut msgs, &mut nid, format!("**Blend mode ({theme_label}):** `{current}`\n**Dark:** `{}` · **Light:** `{}`\n\nAvailable:\n{modes}\n\nUsage: `/blend <mode>` — sets for current theme\n`/blend reset` — reset current theme to default", pc_r.dark_blend, pc_r.light_blend), MessageKind::Text);
            } else if rest == "reset" {
                let default = if is_dark { config::default_dark_blend() } else { config::default_light_blend() };
                plasma_cfg.with_mut(|p| { if is_dark { p.dark_blend = default.clone(); } else { p.light_blend = default.clone(); } });
                save_plasma_config(&plasma_cfg);
                push_system_msg(&mut msgs, &mut nid, format!("Blend mode ({theme_label}) reset to `{default}`."), MessageKind::Text);
            } else if config::BLEND_MODES.contains(&rest) {
                plasma_cfg.with_mut(|p| { if is_dark { p.dark_blend = rest.to_string(); } else { p.light_blend = rest.to_string(); } });
                save_plasma_config(&plasma_cfg);
                push_system_msg(&mut msgs, &mut nid, format!("Blend mode ({theme_label}) set to `{rest}`."), MessageKind::Text);
            } else {
                push_system_msg(&mut msgs, &mut nid, format!("Unknown blend mode `{rest}`. Type `/blend` to see available modes."), MessageKind::Text);
            }
            return;
        }
        if trimmed == "/fullscreen" {
            push_system_msg(&mut msgs, &mut nid, "Fullscreen not available on this platform.".to_string(), MessageKind::Text);
            return;
        }
        if trimmed.starts_with("/system") {
            let rest = trimmed["/system".len()..].trim();
            if rest.is_empty() || rest == "show" {
                let current = system_prompt_sig.read().clone();
                push_system_msg(&mut msgs, &mut nid, format!("**System prompt:**\n```\n{}\n```", current), MessageKind::Text);
            } else if rest == "reset" {
                system_prompt_sig.set(default_system_prompt(&detect_language()));
                push_system_msg(&mut msgs, &mut nid, "System prompt reset to default.".to_string(), MessageKind::Text);
            } else {
                system_prompt_sig.set(rest.to_string());
                push_system_msg(&mut msgs, &mut nid, format!("System prompt updated:\n```\n{}\n```", rest), MessageKind::Text);
            }
        return;
    }

    if trimmed.starts_with("/agent") {
        let rest = trimmed["/agent".len()..].trim();
        if rest.is_empty() || rest == "show" {
            let entries = agent_mgr.read().list_display();
            let mut lines = vec!["**Agents:**\n".to_string()];
            for (name, desc, active) in &entries {
                let marker = if *active { " ◀" } else { "" };
                lines.push(format!("- **{name}** — {desc}{marker}"));
            }
            lines.push("\nUsage: `/agent <name>` to switch".to_string());
            push_system_msg(&mut msgs, &mut nid, lines.join("\n"), MessageKind::Text);
        } else if let Ok(agent) = agent_mgr.write().switch(rest) {
            system_prompt_sig.set(agent.system_prompt.clone());
            push_system_msg(&mut msgs, &mut nid, format!("Switched to agent **{}** ({})", agent.agent_name, agent.name), MessageKind::Text);
        } else {
            push_system_msg(&mut msgs, &mut nid, format!("Unknown agent: `{}`. Type `/agent` to see available.", rest), MessageKind::Text);
        }
        return;
    }

    if trimmed.starts_with("/shell") {
        let rest = trimmed["/shell".len()..].trim();
        if rest.is_empty() || rest == "show" {
            let entries = shell_mgr.read().list_display();
            let mut lines = vec!["**App Shells:**\n".to_string()];
            for (name, desc, icon, active) in &entries {
                let marker = if *active { " ◀" } else { "" };
                lines.push(format!("- {icon} **{name}** — {desc}{marker}"));
            }
            lines.push("\nUsage: `/shell <name>` to switch".to_string());
            push_system_msg(&mut msgs, &mut nid, lines.join("\n"), MessageKind::Text);
        } else if let Ok(shell) = shell_mgr.write().switch(rest) {
            if let Some(a) = agent_mgr.read().agents.get(&shell.agent) {
                system_prompt_sig.set(a.system_prompt.clone());
            }
        push_system_msg(&mut msgs, &mut nid, format!("Switched to {icon} **{name}** — {desc}", icon=shell.icon, name=shell.name, desc=shell.description), MessageKind::Text);
        } else {
            push_system_msg(&mut msgs, &mut nid, format!("Unknown shell: `{}`. Type `/shell` to see available.", rest), MessageKind::Text);
        }
        return;
    }

    if trimmed.starts_with("/invite") {
        let rest = trimmed["/invite".len()..].trim();
        if rest.is_empty() {
            push_system_msg(&mut msgs, &mut nid, "Usage: `/invite <pubkey_hex>` — invite user to the active shell's MLS group".to_string(), MessageKind::Text);
        } else {
            let active_shell = shell_mgr.read().active().clone();
            let group_id = active_shell.channels.first().cloned().unwrap_or_else(|| "devices".to_string());
            let rt = net.read().clone();
            let gid = group_id;
            let pk = rest.to_string();
            let mut ms_inv = msgs.clone();
            let mut nid_inv = nid.clone();
            let connected = net_conn.peek().clone();
            wasm_bindgen_futures::spawn_local(async move {
                if connected && rt.is_running().await {
                    match rt.invite_to_group(&gid, &pk).await {
                        Ok(()) => {
                            push_system_msg(&mut ms_inv, &mut nid_inv, format!("Invited `{}` to group `{}`", pk.chars().take(12).collect::<String>(), gid), MessageKind::Text);
                        }
                        Err(e) => {
                            push_system_msg(&mut ms_inv, &mut nid_inv, format!("Invite failed: {}", e), MessageKind::Text);
                        }
                    }
                } else {
                    push_system_msg(&mut ms_inv, &mut nid_inv, "Not connected to Nostr.".to_string(), MessageKind::Text);
                }
            });
        }
        return;
    }

    if trimmed.starts_with("/join") {
        let rest = trimmed["/join".len()..].trim();
        if rest.is_empty() {
            let rt = net.read().clone();
            let mut ms_j = msgs.clone();
            let mut nid_j = nid.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let invites = rt.pending_invites().await;
                if invites.is_empty() {
                    push_system_msg(&mut ms_j, &mut nid_j, "No pending MLS invites. Someone must `/invite` you first.".to_string(), MessageKind::Text);
                } else {
                    let mut lines = vec!["**Pending invites:**\n".to_string()];
                    for inv in &invites {
                        lines.push(format!("- Group `{}` from `{}` (received {})", inv.group_id, inv.sender.chars().take(8).collect::<String>(), inv.received_at));
                    }
                    lines.push("\nUsage: `/join <group_id>` to accept".to_string());
                    push_system_msg(&mut ms_j, &mut nid_j, lines.join("\n"), MessageKind::Text);
                }
            });
        } else {
            let rt = net.read().clone();
            let gid = rest.to_string();
            let mut ms_j = msgs.clone();
            let mut nid_j = nid.clone();
            wasm_bindgen_futures::spawn_local(async move {
                match rt.join_pending_invite(&gid).await {
                    Ok(joined_id) => {
                        push_system_msg(&mut ms_j, &mut nid_j, format!("Joined MLS group `{}`", joined_id), MessageKind::Text);
                    }
                    Err(e) => {
                        push_system_msg(&mut ms_j, &mut nid_j, format!("Join failed: {}", e), MessageKind::Text);
                    }
                }
            });
        }
        return;
    }

    if trimmed == "/members" {
        let active_shell = shell_mgr.read().active().clone();
        let group_id = active_shell.channels.first().cloned().unwrap_or_else(|| "devices".to_string());
        let rt = net.read().clone();
        let gid = group_id;
        let mut ms_m = msgs.clone();
        let mut nid_m = nid.clone();
        wasm_bindgen_futures::spawn_local(async move {
            match rt.get_group_members(&gid).await {
                Ok(members) => {
                    if members.is_empty() {
                        push_system_msg(&mut ms_m, &mut nid_m, format!("No members found in group `{}`.", gid), MessageKind::Text);
                    } else {
                        let list: Vec<String> = members.iter().map(|m| format!("- `{}`", m.chars().take(12).collect::<String>())).collect();
                        push_system_msg(&mut ms_m, &mut nid_m, format!("**Members of `{}`:**\n{}", gid, list.join("\n")), MessageKind::Text);
                    }
                }
                Err(e) => {
                    push_system_msg(&mut ms_m, &mut nid_m, format!("Error: {}", e), MessageKind::Text);
                }
            }
        });
        return;
    }

    if trimmed == "/groups" {
        let rt = net.read().clone();
        let mut ms_g = msgs.clone();
        let mut nid_g = nid.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let groups = rt.list_groups().await;
            if groups.is_empty() {
                push_system_msg(&mut ms_g, &mut nid_g, "No MLS groups yet. Use `/invite` to add members or switch to a shell with `/shell`.".to_string(), MessageKind::Text);
            } else {
                let mut lines = vec!["**MLS Groups:**\n".to_string()];
                for g in &groups {
                    let type_label = match &g.group_type {
                        crate::net::group_registry::GroupType::Device => "device",
                        crate::net::group_registry::GroupType::Chat => "chat",
                        crate::net::group_registry::GroupType::Shell { shell_name } => shell_name,
                    };
                    lines.push(format!("- `{}` [{}] {} members — {}", g.group_id, type_label, g.members.len(), g.name));
                }
                push_system_msg(&mut ms_g, &mut nid_g, lines.join("\n"), MessageKind::Text);
            }
        });
        return;
    }

    if trimmed == "/clear" {
        msgs.set(Vec::new());
        let msg_id = nid();
        nid.set(msg_id + 1);
            msgs.with_mut(|v| v.push(Message {
                id: msg_id, role: MessageRole::System,
                content: splash_content(false),
                thinking: String::new(), kind: MessageKind::Text,
                sender: String::new(),
            timestamp: now_secs(),
            }));
            return;
        }
        if trimmed == "/backup" {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(nsec)) = storage.get_item("thoth_nsec") {
                        push_system_msg(&mut msgs, &mut nid, format!("**Your backup key:**\n\n`{}`\n\nStore this safely. Anyone with this key can access your identity.", nsec), MessageKind::Text);
                    } else {
                        push_system_msg(&mut msgs, &mut nid, "No backup key found. Start chatting to generate an identity, or use `/login <nsec>` to restore one.".to_string(), MessageKind::Text);
                    }
                }
            }
            return;
        }
        if trimmed.starts_with("/login ") {
            let nsec_str = trimmed["/login ".len()..].trim();
            if nsec_str.is_empty() {
                push_system_msg(&mut msgs, &mut nid, "Usage: `/login <nsec1...>`".to_string(), MessageKind::Text);
                return;
            }
            if let Ok(keys) = nostr_sdk::Keys::parse(nsec_str) {
                if let Some(window) = web_sys::window() {
                    if let Ok(Some(storage)) = window.local_storage() {
                        let _ = storage.set_item("thoth_nsec", nsec_str);
                    }
                }
                let pk = keys.public_key().to_bech32().unwrap_or_else(|_| "unknown".to_string());
                push_system_msg(&mut msgs, &mut nid, format!("Identity restored! Public key: `{}`\n\nReconnecting to Nostr...", pk), MessageKind::Text);
                let rt = net.read().clone();
                let mut nc = net_conn.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match rt.start().await {
                        Ok(()) => { nc.set(true); }
                        Err(_) => {}
                    }
                });
            } else {
                push_system_msg(&mut msgs, &mut nid, "Invalid nsec key. Check the format and try again.".to_string(), MessageKind::Text);
            }
            return;
        }
            if trimmed == "/relays" {
            let rt = net.read().clone();
            let mut ms = msgs.clone();
            let mut nid_c = nid.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let statuses = rt.relay_statuses().await;
                let mut lines = vec!["**Nostr Relay Status:**\n".to_string()];
                for (url, status) in &statuses {
                    let icon = if status == "connected" { "\u{2705}" } else { "\u{274c}" };
                    lines.push(format!("{} `{}` — {}", icon, url, status));
                }
                if statuses.is_empty() {
                    lines.push("No relays configured.".to_string());
                }
                push_system_msg(&mut ms, &mut nid_c, lines.join("\n"), MessageKind::Text);
            });
            return;
        }

        if trimmed.starts_with("/relay_add ") {
            let url = trimmed["/relay_add ".len()..].trim().to_string();
            if !url.is_empty() {
                let rt = net.read().clone();
                let mut ms = msgs.clone();
                let mut nid_c = nid.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match rt.add_relay(&url).await {
                        Ok(()) => { push_system_msg(&mut ms, &mut nid_c, format!("Added relay: `{url}`"), MessageKind::Text); }
                        Err(e) => { push_system_msg(&mut ms, &mut nid_c, format!("Failed to add relay: {e}"), MessageKind::Text); }
                    }
                });
            }
            return;
        }

        if trimmed.starts_with("/relay_rm ") {
            let url = trimmed["/relay_rm ".len()..].trim().to_string();
            if !url.is_empty() {
                let rt = net.read().clone();
                let mut ms = msgs.clone();
                let mut nid_c = nid.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    match rt.remove_relay(&url).await {
                        Ok(()) => { push_system_msg(&mut ms, &mut nid_c, format!("Removed relay: `{url}`"), MessageKind::Text); }
                        Err(e) => { push_system_msg(&mut ms, &mut nid_c, format!("Failed to remove relay: {e}"), MessageKind::Text); }
                    }
                });
            }
            return;
        }

        if trimmed.starts_with("/dm ") {
                let rest = trimmed["/dm ".len()..].trim();
                let parts: Vec<&str> = rest.splitn(2, char::is_whitespace).collect();
                if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
                    push_system_msg(&mut msgs, &mut nid, "Usage: `/dm <npub1...> <message>`".to_string(), MessageKind::Text);
                    return;
                }
                let recipient = parts[0].to_string();
                let dm_content = parts[1].to_string();
                let rt = net.read().clone();
                let mut ms = msgs.clone();
                let mut nid_c = nid.clone();
                let recipient_display = recipient.chars().take(12).collect::<String>();
                let uid = nid();
                nid.set(uid + 1);
                msgs.with_mut(|v| v.push(Message {
                    id: uid, role: MessageRole::User, content: dm_content.clone(),
                    thinking: String::new(), kind: MessageKind::NostrDm { sender_pubkey: "You".to_string() },
                    sender: "You".to_string(), timestamp: now_secs(),
                }));
                wasm_bindgen_futures::spawn_local(async move {
                    match rt.send_nostr_message(dm_content).await {
                        Ok(()) => {
                            push_system_msg(&mut ms, &mut nid_c, format!("DM sent to `{recipient_display}...`"), MessageKind::Text);
                        }
                        Err(e) => {
                            push_system_msg(&mut ms, &mut nid_c, format!("Failed to send DM: {e}"), MessageKind::Text);
                        }
                    }
                });
                return;
            }
        if trimmed.starts_with('/') {
            push_system_msg(&mut msgs, &mut nid, format!("Unknown command: `{trimmed}`"), MessageKind::Text);
            return;
        }

        // @Tot prefix sends inference request to remote device
        if trimmed.to_lowercase().starts_with("@tot ") {
            let query = trimmed[5..].trim().to_string();
            if query.is_empty() {
                push_system_msg(&mut msgs, &mut nid, "Usage: `@Tot <your question>`".to_string(), MessageKind::Text);
                return;
            }
            il.set(true);
            let uid = nid();
            nid.set(uid + 1);
            msgs.with_mut(|v| v.push(Message {
                id: uid, role: MessageRole::User, content: query.clone(),
                thinking: String::new(), kind: MessageKind::Text,
                sender: "You".to_string(), timestamp: now_secs(),
            }));
            let aid = nid();
            nid.set(aid + 1);
            msgs.with_mut(|v| v.push(Message {
                id: aid, role: MessageRole::Assistant, content: String::new(),
                thinking: String::new(), kind: MessageKind::Text,
                sender: "Tot".to_string(), timestamp: now_secs(),
            }));
            let current_sp = system_prompt_sig.read().clone();
            let rt = net.read().clone();
            let connected = net_conn.peek().clone();
            wasm_bindgen_futures::spawn_local(async move {
                let mut ms = msgs;
                let mut il = il;
                if !connected {
                    ms.with_mut(|v| {
                        if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                            msg.content = "Not connected to Nostr yet. Waiting for connection...".to_string();
                        }
                    });
                    il.set(false);
                    return;
                }
                let req = InferenceRequest {
                    request_id: format!("web-{}", next_msg_id()),
                    prompt_segments: vec![query.clone()],
                    system_prompt: current_sp,
                    model_hint: None,
                    max_tokens: Some(512),
                    temperature: Some(0.5),
                };
                match rt.send_inference_request(req).await {
                    Ok(rx) => {
                        ms.with_mut(|v| {
                            if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                                msg.content = "_waiting for device..._".to_string();
                            }
                        });
                        match rx.await {
                            Ok(resp) => {
                                ms.with_mut(|v| {
                                    if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                                        if !resp.thinking.is_empty() {
                                            msg.thinking = resp.thinking;
                                        }
                                        msg.content = resp.content;
                                    }
                                });
                            }
                            Err(_) => {
                                ms.with_mut(|v| {
                                    if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                                        msg.content = "Inference request timed out.".to_string();
                                    }
                                });
                            }
                        }
                    }
                    Err(e) => {
                        ms.with_mut(|v| {
                            if let Some(msg) = v.iter_mut().find(|m| m.id == aid) {
                                msg.content = format!("No device available: {}", e);
                            }
                        });
                    }
                }
                il.set(false);
            });
            return;
        }

        // Regular message: dispatch by active shell publish_mode
        let uid = nid();
        nid.set(uid + 1);
        msgs.with_mut(|v| v.push(Message { id: uid, role: MessageRole::User, content: trimmed.clone(), thinking: String::new(), kind: MessageKind::Text, sender: "You".to_string(), timestamp: now_secs() }));

        let active_shell = shell_mgr.read().active().clone();
        match active_shell.publish_mode {
            crate::system::app_shell::PublishMode::Public => {
                let rt = net.read().clone();
                let connected = net_conn.peek().clone();
                let mut ms_pub = msgs.clone();
                let mut nid_pub = nid.clone();
                let content = trimmed.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    if connected {
                        let _ = rt.publish_text_note(content).await;
                    } else {
                        push_system_msg(&mut ms_pub, &mut nid_pub, "Not connected to Nostr. Message not sent.".to_string(), MessageKind::Text);
                    }
                });
            }
        crate::system::app_shell::PublishMode::Private => {
            let rt = net.read().clone();
            let content = trimmed.clone();
            let shell_name = shell_mgr.read().active().name.clone();
            let connected = net_conn.peek().clone();
            wasm_bindgen_futures::spawn_local(async move {
                if connected && rt.is_running().await {
                    if let Some(pk) = rt.own_pubkey().await {
                        if let Err(e) = rt.send_shell_text(&pk, &shell_name, content).await {
                            tracing::warn!("Failed to send MLS group text: {}", e);
                        }
                    }
                }
            });
        }
            crate::system::app_shell::PublishMode::Local => {}
        }
    }
};

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    let mut is_fullscreen = use_signal(|| false);

    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    let fullscreen_keydown = {
        let mut is_fs = is_fullscreen.clone();
        move |e: KeyboardEvent| {
            if e.key() == Key::F11 {
                e.prevent_default();
                let next = !is_fs();
                is_fs.set(next);
                let win = dioxus::desktop::window();
                win.set_fullscreen(next);
            }
        }
    };

    #[cfg(any(target_arch = "wasm32", target_os = "android"))]
    let fullscreen_keydown = move |_e: KeyboardEvent| {};

    let mut input_for_submit = input.clone();
    let mut scroll_to_bottom = move || {
spawn(async move {
        crate::shared::sleep_ms(10).await;
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

    use_future(move || async move {
        dioxus::document::eval(PLASMA_JS).await;
    });

    {
        let pc = plasma_config.read().clone();
        let is_dark = current_theme == Theme::Dark;
        let colors = if is_dark { &pc.dark_colors } else { &pc.light_colors };
        let js = format!(
            "if(window.__plasmaUpdate)window.__plasmaUpdate({{enabled:{},speed:{},c1:[{},{},{}],c2:[{},{},{}],c3:[{},{},{}]}});",
            pc.enabled, pc.speed,
            colors[0], colors[1], colors[2],
            colors[3], colors[4], colors[5],
            colors[6], colors[7], colors[8],
        );
        spawn(async move {
            dioxus::document::eval(&js).await;
        });
    }

        let on_color_pick = {
let mut plasma_cfg = plasma_config.clone();
let mut msgs = messages.clone();
let mut nid = next_id.clone();
let theme_sig = theme.clone();
move |(color_index, hex_value, tag): (usize, String, String)| {
let rgb = match hex_to_rgb(&hex_value) {
Some(rgb) => rgb,
None => return,
};
plasma_cfg.with_mut(|p| {
let colors = if tag == "dark" { &mut p.dark_colors } else { &mut p.light_colors };
let base = (color_index % 3) * 3;
colors[base] = rgb[0];
colors[base + 1] = rgb[1];
colors[base + 2] = rgb[2];
});
let ci = color_index % 3 + 1;
push_system_msg(&mut msgs, &mut nid, format!("**{tag}** c{ci} set to `{hex_value}`"), MessageKind::Text);
save_plasma_config(&plasma_cfg);
let pc = plasma_cfg.read().clone();
let is_dark = theme_sig() == Theme::Dark;
let colors = if is_dark { &pc.dark_colors } else { &pc.light_colors };
let js = format!(
"if(window.__plasmaUpdate)window.__plasmaUpdate({{enabled:{},speed:{},c1:[{},{},{}],c2:[{},{},{}],c3:[{},{},{}]}});",
pc.enabled, pc.speed,
colors[0], colors[1], colors[2],
colors[3], colors[4], colors[5],
colors[6], colors[7], colors[8],
);
spawn(async move {
dioxus::document::eval(&js).await;
});
}
};

rsx! {
        style { {TAILWIND_CSS} },
        style { {FONTS_CSS} },
        style { {APP_CSS} },
        style { "html, body {{ margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden; background: transparent; color: {current_theme.fg()}; font-family: 'MsgSans', sans-serif; }}" },
        div {
            class: "font-loading flex flex-col fixed inset-0 overflow-hidden",
            style: format!("color: {}", current_theme.fg()),
            onkeydown: fullscreen_keydown,
            canvas {
                id: "plasma-canvas",
                class: "absolute top-0 left-0 w-full h-full pointer-events-none",
                style: "z-index: 0;",
            },
            div {
                class: "relative flex flex-col flex-1 min-h-0",
                style: format!("z-index: 1; mix-blend-mode: {};", if current_theme == Theme::Dark { plasma_config.read().dark_blend.clone() } else { plasma_config.read().light_blend.clone() }),
                MessageList {
                    messages: messages.clone(),
                    current_theme: current_theme.clone(),
                    at_bottom: at_bottom,
                    has_new: has_new,
                    on_color_pick: on_color_pick,
                }
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
