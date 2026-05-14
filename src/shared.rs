use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::atomic::{AtomicU64, Ordering};

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
