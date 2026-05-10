use dioxus::prelude::*;
use comrak;

#[component]
pub fn Markdown(content: String) -> Element {
    let html = comrak::markdown_to_html(&content, &comrak::ComrakOptions::default());
    rsx! {
         div { class: "markdown-content", dangerous_inner_html: html }
    }
}
