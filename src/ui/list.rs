use dioxus::prelude::*;
use dioxus::html::ScrollData;
use crate::shared::{Message, MessageRole, Theme};
use crate::ui::message::MessageBubble;

#[component]
pub fn MessageList(
    messages: Signal<Vec<Message>, SyncStorage>,
    current_theme: Theme,
    at_bottom: Signal<bool>,
    has_new: Signal<bool>,
    on_color_pick: EventHandler<(usize, String, String)>,
) -> Element {
    let mut frozen_items = use_signal::<Vec<Message>>(|| Vec::new());

    let on_scroll = move |e: Event<ScrollData>| {
        let top = e.data().scroll_top();
        let near = top > -50.0;
        if near {
            at_bottom.set(true);
            if !frozen_items.peek().is_empty() {
                frozen_items.set(Vec::new());
            }
            has_new.set(false);
        } else {
            at_bottom.set(false);
        }
    };

    let total = messages.read().len();
    let last_role = messages.read().last().map(|m| m.role);
    let at_bottom_val = *at_bottom.peek();
    let is_frozen = !at_bottom_val && !frozen_items.peek().is_empty();
    let frozen_len = frozen_items.peek().len();

    if !at_bottom_val && !is_frozen {
        let mut snapshot: Vec<Message> = Vec::new();
        for msg in messages.iter() {
            snapshot.push(msg.clone());
        }
        frozen_items.set(snapshot);
    } else if is_frozen && total > frozen_len {
        if last_role == Some(MessageRole::User) {
            has_new.set(false);
            frozen_items.set(Vec::new());
        } else {
            has_new.set(true);
        }
    }

    let items = if !is_frozen {
        let mut v: Vec<Message> = Vec::new();
        for msg in messages.iter() {
            v.push(msg.clone());
        }
        v
    } else {
        frozen_items.peek().clone()
    };

    rsx! {
        div {
            id: "message-list",
            class: "flex-1 overflow-y-auto p-6 pt-8 w-full flex flex-col-reverse min-h-0",
            onscroll: on_scroll,
            div {
                key: "inner",
                class: "flex flex-col space-y-3 w-full max-w-[896px] mx-auto my-auto",
                for msg in items {
                    MessageBubble {
                        key: "{msg.id}",
                        msg: msg.clone(),
                        theme: current_theme.clone(),
                        show_thinking: true,
                        on_color_pick: on_color_pick,
                    }
                }
            },
        }
    }
}