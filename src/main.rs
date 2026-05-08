//! Thoth: A message-native application.
//!
//! The entire UI is driven by a message log. See ARCHITECTURE.md for design.
//!
//! Platform selection:
//! - Desktop (not wasm, not android) → llama (unified)
//! - Android (ARM64) → llama (unified)
//! - Web (wasm) → llama_web

// Native inference (desktop or Android ARM64) — unified llama module
#[cfg(any(
    all(not(target_arch = "wasm32"), not(target_os = "android")),
    all(target_os = "android", target_arch = "aarch64")
))]
mod llama;

// Web WASM - stub inference
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