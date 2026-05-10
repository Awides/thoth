#![allow(warnings)]
//! Thoth: A message-native application.

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

// Unified UI module
mod app;
mod shared;
mod ui;

#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
fn main() {
    use dioxus::desktop::{Config, WindowBuilder, LogicalSize};
    use dioxus::desktop::tao::window::Theme as WindowTheme;

    dioxus::LaunchBuilder::new()
        .with_cfg(
            Config::default()
                .with_menu(None)
                .with_window(
                    WindowBuilder::new()
                        .with_title("THOTH ▷")
                        .with_resizable(true)
                        .with_inner_size(LogicalSize::new(800.0, 600.0))
                        .with_background_color((3, 3, 3, 255))
                        .with_theme(Some(WindowTheme::Dark)),
                ),
        )
        .launch(app::App);
}

#[cfg(target_os = "android")]
fn main() {
    dioxus::LaunchBuilder::new()
        .launch(app::App);
}

#[cfg(target_arch = "wasm32")]
fn main() {
    dioxus::LaunchBuilder::new()
        .launch(app::App);
}

