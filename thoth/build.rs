fn main() {
    // Generate bindings using bindgen
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
    
    // Link to llama.cpp
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let lib_path = std::path::Path::new(&manifest_dir).join("lib");
    if lib_path.exists() {
        println!("cargo:rustc-link-search=native={}", lib_path.display());
    }
    println!("cargo:rustc-link-lib=dylib=llama");
    println!("cargo:rustc-link-lib=dylib=ggml");
    println!("cargo:rustc-link-lib=dylib=ggml-cpu");
    println!("cargo:rustc-link-lib=dylib=ggml-base");
    println!("cargo:rustc-link-lib=dylib=llama-common");
}