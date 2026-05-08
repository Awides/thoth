fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    let is_android = target.contains("android");
    let is_armv7 = target.contains("armv7");
    let is_wasm = target.contains("wasm32");

    // WASM and ARMv7 skip bindgen and llama linking
    if is_wasm || is_armv7 {
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

    // Determine llama.cpp install prefix.
    // Priority:
    // 1. LLAMA_HOME or LLAMA_CPP_BUILD env vars
    // 2. Auto-detect: $CARGO_MANIFEST_DIR/../llama.cpp (sibling checkout)
    // 3. Fallback: /tmp/llama.cpp-build
    let llama_home = std::env::var("LLAMA_HOME")
        .or_else(|_| std::env::var("LLAMA_CPP_BUILD"))
        .or_else(|_| {
            // Auto-detect sibling llama.cpp checkout
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
            let candidate = std::path::Path::new(&manifest_dir).join("../llama.cpp");
            if candidate.exists() && candidate.join("include/llama.h").exists() {
                Ok(candidate.to_string_lossy().into_owned())
            } else {
                Err(())
            }
        })
        .unwrap_or_else(|_| "/tmp/llama.cpp-build".to_string());

    let header_path = std::path::Path::new(&llama_home).join("include/llama.h");
    let ggml_include = std::path::Path::new(&llama_home).join("ggml/include");

    // For desktop (non-Android), generate bindings and require headers.
    if !is_android {
        if !header_path.exists() {
            eprintln!("\nERROR: llama.h not found at {}.", header_path.display());
            eprintln!("Please set LLAMA_HOME (or LLAMA_CPP_BUILD) to the root of your llama.cpp installation.");
            eprintln!("Expected layout under LLAMA_HOME:");
            eprintln!("  include/llama.h");
            eprintln!("  ggml/include/");
            eprintln!("Example: export LLAMA_HOME=$HOME/.llama.cpp\n");
            std::process::exit(1);
        }

        let bindings = bindgen::Builder::default()
            .header(header_path.to_str().unwrap())
            .clang_arg(&format!("-I{}", ggml_include.display()))
            .clang_arg(&format!("-I{}", header_path.parent().unwrap().display()))
            // Try a common GCC include path; harmless if missing
            .clang_arg("-isystem")
            .clang_arg("/usr/lib/gcc/x86_64-linux-gnu/15/include")
            .generate()
            .expect("Unable to generate bindings");

        let out_path = std::path::Path::new(&std::env::var("OUT_DIR").unwrap()).join("bindings.rs");
        bindings
            .write_to_file(&out_path)
            .expect("Couldn't write bindings");
    }

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