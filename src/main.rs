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

// Core libraries
mod key_storage;
mod mem;
mod net;
mod system;

// Platform-specific UI modules
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
mod app; // Desktop UI

#[cfg(target_os = "android")]
mod android_app; // Android UI

#[cfg(target_arch = "wasm32")]
mod web_app; // Web UI

fn main() {
// Desktop
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
dioxus::launch(app::App);

// Android (ARM64 only; ARMv7 is UI-only and not compiled)
#[cfg(target_os = "android")]
dioxus::launch(android_app::App);

// Web WASM
#[cfg(target_arch = "wasm32")]
dioxus::launch(web_app::App);
}