use dioxus::prelude::*;
use crate::llama;
use comrak;

static TAILWIND: Asset = asset!("/assets/tailwind.css");

const markdown_css: &str = r#"
.markdown-content { font-size: 1rem; line-height: 1.75; }
.markdown-content strong { font-weight: 600; }
.markdown-content code { background: rgba(100,100,100,0.2); padding: 0.125rem 0.375rem; border-radius: 0.25rem; font-family: monospace; font-size: 0.875em; }
.markdown-content pre { background: rgba(100,100,100,0.15); padding: 0.75rem 1rem; border-radius: 0.375rem; overflow-x: auto; margin: 0.5rem 0; }
.markdown-content pre code { background: transparent; padding: 0; }
"#;

#[derive(Clone, PartialEq, Copy)]
pub enum MessageRole { User, Assistant, System }

#[derive(Clone, PartialEq)]
enum MessageKind {
    /// Regular text — rendered as markdown
    Text,
    /// Typed, tagged request for structured user input
    Request { request_type: String, tag: String },
}

#[derive(Clone, PartialEq)]
struct Message {
    id: u64,
    role: MessageRole,
    content: String,
    thinking: String,
    kind: MessageKind,
}

#[component]
fn Markdown(content: String) -> Element {
    let html = comrak::markdown_to_html(&content, &comrak::ComrakOptions::default());
    rsx! {
        div { class: "markdown-content", dangerous_inner_html: "{html}" }
    }
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

    // Spawn inference thread - store handle persistently in a signal
    let handle = use_signal_sync(|| llama::spawn_inference_thread());

    // Load model at startup
    let model_path = "/home/awides/dev/bn/models/Bonsai-1.7B-Q1_0.gguf".to_string();
    let config = llama::Config {
        n_ctx: 512, n_gpu_layers: 99, n_threads: 8, n_batch: 512,
        use_mmap: true, temperature: 0.7, top_p: 0.9, top_k: 40,
    };

let handle_c = handle.clone();
let ls_load = loading_state.clone();
let il_load = is_loading.clone();
let msgs = messages.clone();
let nid = next_id.clone();
let _fut = use_future(move || {
        let h = handle_c.read().clone();
        let mut ls = ls_load.clone();
        let mut il = il_load.clone();
        let c = config.clone();
        let p = model_path.clone();
        async move {
            il.set(true);
            ls.set(LoadingState::Loading);
            eprintln!("DEBUG: starting model load...");
            match llama::load_model(&h, p, c).await {
                Ok(_) => {
                    eprintln!("DEBUG: model loaded successfully");
ls.set(LoadingState::Ready);
                    // Show welcome on first launch
                    let msgs_w = msgs.clone();
                    let nid_w = nid.clone();
                    let need_onboard = crate::system::config::needs_onboarding();
                    tokio::spawn(async move {
                        let mut ms = msgs_w;
                        let mut n = nid_w;
                        if need_onboard {
                            let prompts: Vec<(&str, u64)> = vec![
                                ("👋 Welcome to Thoth!", 600),
                                ("I'm your decentralized AI assistant.", 600),
                                ("Let's get you set up…", 400),
                            ];
                            for (text, delay) in &prompts {
                                let id = n();
                                n.set(id + 1);
                                ms.with_mut(|v| {
                                    v.push(Message {
                                        id,
                                        role: MessageRole::System,
                                        content: String::new(),
                                        thinking: String::new(),
                                        kind: MessageKind::Text,
                                    });
                                });
                                for ch in text.chars() {
                                    tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
                                    ms.with_mut(|v| {
                                        if let Some(msg) = v.iter_mut().find(|m| m.id == id) {
                                            msg.content.push(ch);
                                        }
                                    });
                                }
                                tokio::time::sleep(tokio::time::Duration::from_millis(*delay)).await;
                            }
// Final prompt + request (typed)
let pid = n();
n.set(pid + 1);
let prompt_text = "# How would you like to proceed?";
ms.with_mut(|v| {
    v.push(Message {
        id: pid,
        role: MessageRole::System,
        content: String::new(),
        thinking: String::new(),
        kind: MessageKind::Text,
    });
});
for ch in prompt_text.chars() {
    tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
    ms.with_mut(|v| {
        if let Some(msg) = v.iter_mut().find(|m| m.id == pid) {
            msg.content.push(ch);
        }
    });
}
// Small delay, then add the Request
tokio::time::sleep(tokio::time::Duration::from_millis(600)).await;
                            let rid = n();
                            n.set(rid + 1);
                            ms.with_mut(|v| {
                                v.push(Message {
                                    id: rid,
                                    role: MessageRole::System,
                                    content: String::new(),
                                    thinking: String::new(),
kind: MessageKind::Request {
    request_type: "choice".to_string(),
    tag: "onboard-start".to_string(),
},
                                });
                            });
                        }
                    });
                }
                Err(e) => {
                    eprintln!("DEBUG: model load error: {}", e);
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

// Handle slash commands locally — don't send to assistant
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

let id = nid();
nid.set(id + 1);
msgs.with_mut(|v| {
    v.push(Message { id, role: MessageRole::User, content: trimmed.clone(), thinking: String::new(), kind: MessageKind::Text })
});

il.set(true);
let aid = nid();
nid.set(aid + 1);
msgs.with_mut(|v| {
    v.push(Message { id: aid, role: MessageRole::Assistant, content: String::new(), thinking: String::new(), kind: MessageKind::Text })
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
if val.is_empty() {
    return;
}
// Allow slash commands even while model is loading
if val.starts_with('/') {
    *input.write() = String::new();
    process_input(val);
    return;
}
// Real prompts need the model ready
if *loading_state.read() == LoadingState::Ready {
    *input.write() = String::new();
    process_input(val);
}
};

    let current_theme = theme();
    let msgs = messages();

    rsx! {
        document::Stylesheet { href: TAILWIND },
        div {
    style: format!("background: {}; color: {}; display: flex; flex-direction: column; overflow: hidden;", current_theme.bg(), current_theme.fg()),
    class: "h-screen flex flex-col",
    // Message list area
    div {
        style: "flex: 1; overflow-y: auto; min-height: 0;",
        class: "flex-1 overflow-y-auto p-6 pt-8 space-y-3 scroll-smooth w-full max-w-[896px] mx-auto",
                for msg in msgs.iter() {
div {
    key: "{msg.id}",
    class: match msg.kind {
        MessageKind::Request { .. } => "p-3 w-full self-start".to_string(),
        _ => format!("p-3 rounded-lg max-w-[80%] break-words {}",
            match msg.role { MessageRole::User => "self-end", _ => "self-start" }
        ),
    },
    style: match &msg.kind {
        MessageKind::Request { .. } => "".to_string(),
        _ => format!("background: {}", match msg.role {
            MessageRole::User => "#3b82f6",
            MessageRole::Assistant => current_theme.panel(),
            MessageRole::System => "transparent",
        }),
    },
    if !msg.thinking.is_empty() {
        pre { class: "text-sm italic opacity-80 mb-1 whitespace-pre-wrap font-inherit font-light", "{msg.thinking}" }
    }
    if let MessageKind::Request { request_type, tag } = &msg.kind {
div { class: "border rounded-lg p-4 max-w-[80%]",
    style: format!("border-color: {}; background: {}", current_theme.border(), current_theme.panel()),
    p { class: "text-xs opacity-50 mb-1", "#{tag}" },
    p { class: "text-sm", "[{request_type}]" },
}
    } else {
        if msg.role == MessageRole::System {
            Markdown { content: msg.content.clone() }
        } else {
            pre { class: "m-0 whitespace-pre-wrap font-inherit", "{msg.content}" }
        }
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
    class: "w-full max-w-[896px] shrink-0 p-3 border-t mx-auto",
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
}