use dioxus::prelude::*;
use crate::shared::{Message, MessageRole, MessageKind, Theme};
use crate::ui::markdown::Markdown;

fn sender_label(role: MessageRole) -> &'static str {
    match role {
        MessageRole::System => "SYSTEM",
        MessageRole::Assistant => "Tot",
        MessageRole::User => "You",
    }
}

#[component]
pub fn MessageBubble(msg: Message, theme: Theme, show_thinking: bool) -> Element {
    let role = msg.role;
    let time_str = msg.timestamp_str();
    let label = sender_label(role);
    let muted_class = theme.muted();

    rsx! {
        div {
            class: match role {
                MessageRole::System => "p-3 rounded-lg break-words self-start",
                _ => "p-3 rounded-lg max-w-[80%] break-words self-start",
            },
            p { class: "m-0 text-xs font-thin mb-1 {muted_class}", {time_str}{" "}{label} }
            if let MessageKind::ToolCall { tool_name } = &msg.kind {
                p { class: "m-0 italic {muted_class}",
                    {"calling "}{tool_name.clone()}{"..."}
                }
            } else if let MessageKind::Text = msg.kind {
                Markdown { content: msg.content.clone() }
            } else {
                p { class: "m-0 whitespace-pre-wrap font-inherit", {msg.content.clone()} }
            }
        }
    }
}
