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
            class: match role {
                MessageRole::System => "p-3 rounded-lg break-words self-start",
                _ => "p-3 rounded-lg max-w-[80%] break-words self-start",
            },
            style: match role {
                MessageRole::System => "background: transparent; padding: 0.75rem; border-radius: 0.5rem; overflow-wrap: break-word; align-self: flex-start;".to_string(),
                _ => "background: transparent; padding: 0.75rem; border-radius: 0.5rem; max-width: 80%; overflow-wrap: break-word; align-self: flex-start;".to_string(),
            },
            p { class: "m-0 text-xs font-thin mb-1", style: format!("margin: 0; font-size: 0.75rem; font-weight: 100; margin-bottom: 0.25rem; color: {}", muted), {time_str} }
            if let MessageKind::ToolCall { tool_name } = &msg.kind {
                p { class: "m-0 italic", style: format!("margin: 0; font-style: italic; color: {}", muted), 
                    {"calling "}{tool_name.clone()}{"..."}
                }
            } else if let MessageKind::Text = msg.kind {
                Markdown { content: msg.content.clone() }
            } else {
                p { class: "m-0 whitespace-pre-wrap font-inherit", style: "margin: 0; white-space: pre-wrap; font-family: inherit;", {msg.content.clone()} }
            }
        }
    }
}
