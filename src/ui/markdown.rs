use dioxus::prelude::*;
use comrak;

#[component]
pub fn Markdown(content: String) -> Element {
    let html = comrak::markdown_to_html(&content, &comrak::ComrakOptions::default());
    rsx! {
        div {
            class: "markdown-content",
            dangerous_inner_html: html,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_splash_rendering() {
        let md = "# *THOTH▷*";
        let html = comrak::markdown_to_html(md, &comrak::ComrakOptions::default());
        eprintln!("SPLASH HTML: {}", html);
        assert!(html.contains("<h1>"));
    }
}
