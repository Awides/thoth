#[cfg(not(target_arch = "wasm32"))]
#[path = "llama_native/mod.rs"]
pub mod llama;

#[cfg(target_arch = "wasm32")]
#[path = "llama_web/mod.rs"]
pub mod llama;

#[cfg(not(target_arch = "wasm32"))]
pub mod engine;

#[cfg(not(target_arch = "wasm32"))]
mod app;

#[cfg(target_arch = "wasm32")]
mod web_app;

fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    dioxus::launch(app::App);

    #[cfg(target_arch = "wasm32")]
    dioxus::launch(web_app::App);
}