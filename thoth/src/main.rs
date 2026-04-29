pub mod llama;
pub mod engine;
pub mod app;

fn main() {
    dioxus::launch(app::App);
}
