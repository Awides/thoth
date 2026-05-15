use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicU64, Ordering};
use dioxus::prelude::{WritableExt, Signal, SyncStorage};

static NEXT_MSG_ID: AtomicU64 = AtomicU64::new(100_000);

pub fn next_msg_id() -> u64 {
    NEXT_MSG_ID.fetch_add(1, Ordering::Relaxed)
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, PartialEq, Copy)]
pub enum MessageRole { User, Assistant, System }

#[derive(Clone, PartialEq)]
pub enum MessageKind {
    Text,
    Request { request_type: String, tag: String },
    ColorRequest { color_index: usize, tag: String, initial_hex: String },
    ToolCall { tool_name: String },
}

#[derive(Clone, PartialEq)]
pub struct Message {
    pub id: u64,
    pub role: MessageRole,
    pub content: String,
    pub thinking: String,
    pub kind: MessageKind,
    pub timestamp: u64,
}

impl Message {
    pub fn timestamp_str(&self) -> String {
        let secs = self.timestamp;
        let utc = chrono::DateTime::from_timestamp(secs as i64, 0)
            .unwrap_or_else(|| chrono::Utc::now());
        let local: chrono::DateTime<chrono::Local> = utc.into();
        local.format("%H:%M").to_string()
    }
}

pub fn hex_to_rgb(hex: &str) -> Option<[f32; 3]> {
    let hex = hex.trim().trim_start_matches('#');
    if hex.len() != 6 { return None; }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
}

pub fn rgb_to_hex(r: f32, g: f32, b: f32) -> String {
    format!("#{:02x}{:02x}{:02x}",
        (r.clamp(0.0, 1.0) * 255.0).round() as u8,
        (g.clamp(0.0, 1.0) * 255.0).round() as u8,
        (b.clamp(0.0, 1.0) * 255.0).round() as u8,
    )
}

pub fn push_system_msg(
    msgs: &mut dioxus::prelude::Signal<Vec<Message>, dioxus::prelude::SyncStorage>,
    nid: &mut dioxus::prelude::Signal<u64, dioxus::prelude::SyncStorage>,
    content: String,
    kind: MessageKind,
) {
    let id = nid();
    nid.set(id + 1);
    let _ = msgs.with_mut(|v| v.push(Message {
        id,
        role: MessageRole::System,
        content,
        thinking: String::new(),
        kind,
        timestamp: now_secs(),
    }));
}

#[derive(Clone, PartialEq, Debug)]
pub enum LoadingState { Loading, Ready, Error(String) }

#[derive(Clone, PartialEq, Debug)]
pub enum Theme { Light, Dark }

impl Theme {
    pub fn toggle(&self) -> Self {
        match self { Theme::Light => Theme::Dark, Theme::Dark => Theme::Light }
    }
    pub fn bg(&self) -> &'static str { match self { Theme::Light => "#fafafa", Theme::Dark => "#0d0d0d" } }
    pub fn fg(&self) -> &'static str { match self { Theme::Light => "#171717", Theme::Dark => "#ededed" } }
    pub fn panel(&self) -> &'static str { match self { Theme::Light => "#f0f0f0", Theme::Dark => "#1a1a1a" } }
    pub fn border(&self) -> &'static str { match self { Theme::Light => "#e5e5e5", Theme::Dark => "#262626" } }
    pub fn muted(&self) -> &'static str { match self { Theme::Light => "neutral-900", Theme::Dark => "neutral-300" } }
}
