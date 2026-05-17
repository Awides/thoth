use dioxus::prelude::*;
use crate::shared::{Message, MessageKind, MessageRole, Theme};
use crate::ui::markdown::Markdown;

fn sender_label(role: MessageRole, sender: &str) -> String {
    match role {
        MessageRole::System => "SYSTEM".to_string(),
        MessageRole::Assistant => "Tot".to_string(),
        MessageRole::User => "You".to_string(),
        MessageRole::Peer => {
            if sender.is_empty() { "Peer".to_string() }
            else if sender.len() > 12 { format!("{}...", &sender[..12]) }
            else { sender.to_string() }
        }
    }
}

#[component]
pub fn MessageBubble(msg: Message, theme: Theme, show_thinking: bool, on_color_pick: EventHandler<(usize, String, String)>) -> Element {
    let role = msg.role;
    let time_str = msg.timestamp_str();
    let label = sender_label(role, &msg.sender);
    let muted_class = theme.muted();

    let color_req = match &msg.kind {
        MessageKind::ColorRequest { color_index, tag, initial_hex } => Some((*color_index, tag.clone(), initial_hex.clone())),
        _ => None,
    };

    rsx! {
        div {
            class: match role {
                MessageRole::System => "p-3 rounded-lg break-words self-start",
                MessageRole::Peer => "p-3 rounded-lg max-w-[80%] break-words self-start border border-neutral-700",
                _ => "p-3 rounded-lg max-w-[80%] break-words self-start",
            },
            p { class: "m-0 text-xs font-extralight mb-1 {muted_class}", {time_str}{" "}{label} }
        if let MessageKind::ToolCall { tool_name } = &msg.kind {
            p { class: "m-0 italic {muted_class}",
                {"calling "}{tool_name.clone()}{"..."}
            }
        } else if let MessageKind::NostrDm { .. } = &msg.kind {
            Markdown { content: msg.content.clone() }
        } else if let Some((ci, tag, hex)) = color_req {
                div { class: "flex items-center gap-3 flex-wrap",
                    input {
                        r#type: "color",
                        value: hex,
                        class: "w-10 h-10 border-0 cursor-pointer bg-transparent p-0",
                        onchange: move |e| {
                            on_color_pick.call((ci, e.data.value().clone(), tag.clone()));
                        },
                    }
                    span { class: "text-sm", {msg.content.clone()} }
                }
            } else if let MessageKind::Text = msg.kind {
                Markdown { content: msg.content.clone() }
            } else {
                p { class: "m-0 whitespace-pre-wrap font-inherit", {msg.content.clone()} }
            }
        }
    }
}
