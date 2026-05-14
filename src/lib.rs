#![allow(warnings)]

#[cfg(any(
    all(not(target_arch = "wasm32"), not(target_os = "android")),
    target_os = "android"
))]
mod llama;

mod key_storage;
mod mem;
mod net;
mod system;
mod tools;

mod app;
mod shared;
mod ui;

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn main() {
    use dioxus::mobile::Config;

    dioxus::LaunchBuilder::new()
        .with_cfg(Config::default().with_background_color((3, 3, 3, 255)))
        .launch(app::App);
}
