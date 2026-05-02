// Desktop Linux/Windows/macOS - full native inference
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
#[path = "llama_native/mod.rs"]
pub mod llama;

// Android ARM64 - full native inference
#[cfg(all(target_os = "android", target_arch = "aarch64"))]
#[path = "llama_android/mod.rs"]
pub mod llama;

// Web WASM - placeholder/remote inference
#[cfg(target_arch = "wasm32")]
#[path = "llama_web/mod.rs"]
pub mod llama;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
pub mod engine;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
mod app;

#[cfg(any(target_os = "android", target_arch = "wasm32"))]
mod android_app;

#[cfg(target_arch = "wasm32")]
mod web_app;

fn main() {
// Desktop
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
dioxus::launch(app::App);

// Android (both ARM64 and ARMv7)
#[cfg(target_os = "android")]
dioxus::launch(android_app::App);

// Web WASM
#[cfg(target_arch = "wasm32")]
dioxus::launch(web_app::App);
}