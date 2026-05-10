use dioxus::prelude::*;
use crate::shared::{Message, MessageRole, MessageKind, Theme};
use crate::ui::markdown::Markdown;

#[component]
pub fn MessageBubble(msg: Message, theme: Theme, show_thinking: bool) -> Element {
    let role = msg.role;
    let muted = theme.muted();
    let time_str = msg.timestamp_str();

    rsx! {
        div {
            class: "p-3 rounded-lg max-w-[80%] break-words self-start",
            style: "background: transparent",
            p { class: "m-0 text-xs font-thin mb-1", style: format!("color: {}", muted), {time_str} }
            if let MessageKind::Text = msg.kind {
                if role == MessageRole::System || role == MessageRole::Assistant {
                    Markdown { content: msg.content.clone() }
                } else {
                    p { class: "m-0 whitespace-pre-wrap font-inherit", {msg.content.clone()} }
                }
            } else {
                p { class: "m-0 whitespace-pre-wrap font-inherit", {msg.content.clone()} }
            }
        }
    }
}
