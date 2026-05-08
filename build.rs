fn main() {
let target = std::env::var("TARGET").unwrap_or_default();
let is_android = target.contains("android");
let is_armv7 = target.contains("armv7");

// ARMv7 Android doesn't use llama.cpp (no native inference)
// WASM and ARMv7 skip bindgen
if target.contains("wasm32") || is_armv7 {
// For ARMv7, still set up basic linking but no llama
if is_armv7 {
let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
let lib_path = std::path::Path::new(&manifest_dir).join("lib");
if lib_path.exists() {
println!("cargo:rustc-link-search=native={}", lib_path.display());
}
println!("cargo:rustc-link-lib=log");
}
return;
}

    let bindings = bindgen::Builder::default()
        .header("/tmp/llama.cpp-build/include/llama.h")
        .clang_arg("-I/tmp/llama.cpp-build/ggml/include")
        .clang_arg("-I/tmp/llama.cpp-build/include")
        .clang_arg("-isystem")
        .clang_arg("/usr/lib/gcc/x86_64-linux-gnu/15/include")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("bindings.rs");
    bindings
        .write_to_file(&out_path)
        .expect("Couldn't write bindings");

let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
let lib_path = std::path::Path::new(&manifest_dir).join("lib");

// For Android, use android/<arch> directory first
if is_android {
let android_lib_path = lib_path.join("android").join("arm64-v8a");
if android_lib_path.exists() {
println!("cargo:rustc-link-search=native={}", android_lib_path.display());
}
// Android needs to link log library for __android_log_write
println!("cargo:rustc-link-lib=log");
} else if lib_path.exists() {
println!("cargo:rustc-link-search=native={}", lib_path.display());
}

println!("cargo:rustc-link-lib=dylib=llama");
println!("cargo:rustc-link-lib=dylib=ggml");
println!("cargo:rustc-link-lib=dylib=ggml-cpu");
println!("cargo:rustc-link-lib=dylib=ggml-base");
println!("cargo:rustc-link-lib=dylib=llama-common");
}