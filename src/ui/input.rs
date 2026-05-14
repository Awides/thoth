use dioxus::prelude::*;
use crate::shared::{LoadingState, Theme};

#[component]
pub fn InputArea(
    input: Signal<String>,
    on_submit: EventHandler<FormEvent>,
    loading_state: Signal<LoadingState, SyncStorage>,
    theme: Theme,
    is_inferencing: bool,
    on_stop: EventHandler<()>,
) -> Element {
    rsx! {
        div {
            class: "w-full shrink-0 p-3",
            form {
                onsubmit: on_submit,
                div { class: "flex gap-2 max-w-[896px] mx-auto",
                    input {
                        r#type: "text",
                        autofocus: true,
                        placeholder: match *loading_state.read() {
                            LoadingState::Loading => "Loading…",
                            LoadingState::Ready => "Prompt…",
                            LoadingState::Error(_) => "Error - try again",
                        },
                        disabled: matches!(*loading_state.read(), LoadingState::Loading),
                        value: input.read().clone(),
                        oninput: move |e| {
                            *input.write() = e.data.value();
                        },
                class: "flex-1 px-3 py-2 border rounded focus:outline-none focus:border-gray-500 bg-transparent",
                style: format!("border-color: {}; color: {}", theme.border(), theme.fg()),
                        onmounted: move |event| {
                            spawn(async move {
                                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                                let _ = event.set_focus(true).await;
                            });
                        },
                    }
                    if is_inferencing {
                        button {
                            r#type: "button",
                            onclick: move |_| on_stop.call(()),
                            class: "w-10 h-[42px] grid place-items-center rounded border text-lg",
            style: format!("border-color: {}; color: {}", theme.border(), theme.fg()),
                    span { class: "mt-0.5", "■" }
                        }
                    } else {
                        button {
                            r#type: "submit",
                            disabled: matches!(*loading_state.read(), LoadingState::Loading) || input.read().trim().is_empty(),
                            class: "w-10 h-[42px] grid place-items-center rounded border disabled:opacity-50 disabled:cursor-not-allowed text-lg",
            style: format!("border-color: {}; color: {}", theme.border(), theme.fg()),
                    "▲"
                        }
                    }
                }
            }
        }
    }
}
