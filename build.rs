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
    // Embed rpath so the binary finds .so files in ../lib relative to the executable
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib");

    // For desktop Dioxus builds (non-Android, non-WASM), automatically copy native libraries into the bundle's lib directory
    if !is_android && !is_wasm {
        use std::ffi::OsStr;

        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let target_triple = std::env::var("TARGET").unwrap_or_default();

        // Determine platform directory name used by Dioxus (e.g., "linux", "macos", "windows")
        let platform_dir = if target_triple.contains("linux") {
            "linux"
        } else if target_triple.contains("darwin") {
            "macos"
        } else if target_triple.contains("windows") {
            "windows"
        } else {
            // Unknown platform; skip bundling
            ""
        };

         if !platform_dir.is_empty() {
             let dx_lib_dir = std::path::Path::new(&manifest_dir)
                 .join("target")
                 .join("dx")
                 .join("thoth")
                 .join(&profile)
                 .join(platform_dir)
                 .join("lib");
             let src_lib_dir = std::path::Path::new(&manifest_dir).join("lib");

             // Ensure the destination lib directory exists
             std::fs::create_dir_all(&dx_lib_dir).expect("Failed to create dx lib directory");

             if src_lib_dir.exists() {
                 // Determine expected library extension for this platform
                 let lib_ext = if target_triple.contains("linux") {
                     "so"
                 } else if target_triple.contains("darwin") {
                     "dylib"
                 } else if target_triple.contains("windows") {
                     "dll"
                 } else {
                     "so"
                 };

                 for entry in std::fs::read_dir(&src_lib_dir).expect("Failed to read lib directory") {
                         let entry = entry.expect("Invalid lib entry");
                         let path = entry.path();
                         let file_type = entry.file_type().expect("Failed to get file type");
  
                         // Skip directories (including android subdir)
                         if file_type.is_dir() {
                             continue;
                         }
  
                         // Track changes to these files to rerun build script when they change
                         println!("cargo:rerun-if-changed={}", path.display());
  
                         // Copy file or symlink (fs::copy follows symlinks)
                         let dest = dx_lib_dir.join(path.file_name().unwrap());
                         std::fs::copy(&path, &dest)
                             .expect(&format!("Failed to copy {} to dx lib", path.display()));
                 }
             }

             // Copy model files from assets/models to the bundle's app/assets/models
             let src_assets_models = std::path::Path::new(&manifest_dir).join("assets/models");
             let dx_app_dir = std::path::Path::new(&manifest_dir)
                 .join("target")
                 .join("dx")
                 .join("thoth")
                 .join(&profile)
                 .join(platform_dir)
                 .join("app");
             let dst_assets_models = dx_app_dir.join("assets/models");

             if src_assets_models.exists() {
                 // Ensure destination exists
                 std::fs::create_dir_all(&dst_assets_models).expect("Failed to create dst assets/models dir");

                 for entry in std::fs::read_dir(&src_assets_models).expect("Failed to read assets/models") {
                     let entry = entry.expect("Invalid entry");
                     let path = entry.path();
                     if path.is_file() {
                         let dest = dst_assets_models.join(path.file_name().unwrap());
                         std::fs::copy(&path, &dest)
                             .expect(&format!("Failed to copy {} to bundle assets", path.display()));
                     }
                 }
             }
         }
     }

    // For Android Dioxus builds, copy native libraries into jniLibs/<abi> so they are packaged into the APK
    if is_android && !is_wasm {
        eprintln!("[build.rs] Android copy block triggered: target={}, is_wasm={}", target, is_wasm);
        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let target_triple = std::env::var("TARGET").unwrap_or_default();
        eprintln!("[build.rs] profile={}, target_triple={}", profile, target_triple);

        // Determine ABI directory name (e.g., "arm64-v8a", "armeabi-v7a")
        let abi = if target_triple.contains("aarch64") {
            "arm64-v8a"
        } else if target_triple.contains("armv7") {
            "armeabi-v7a"
        } else if target_triple.contains("i686") {
            "i686"
        } else if target_triple.contains("x86_64") {
            "x86_64"
        } else {
            // Unknown ABI; skip bundling
            ""
        };

        if !abi.is_empty() {
            let dx_android_project = std::path::Path::new(&manifest_dir)
                .join("target")
                .join("dx")
                .join("thoth")
                .join(&profile)
                .join("android")
                .join("app")
                .join("app");
            let jni_libs_dir = dx_android_project.join("src/main/jniLibs").join(abi);
            let src_lib_android = std::path::Path::new(&manifest_dir).join("lib/android").join(abi);

            // Ensure destination exists
            std::fs::create_dir_all(&jni_libs_dir).expect("Failed to create jniLibs directory");

            if src_lib_android.exists() {
                for entry in std::fs::read_dir(&src_lib_android).expect("Failed to read lib/android/abi") {
                    let entry = entry.expect("Invalid lib entry");
                    let path = entry.path();
                    let file_type = entry.file_type().expect("Failed to get file type");

                    if file_type.is_file() {
                        // Track changes
                        println!("cargo:rerun-if-changed={}", path.display());

                        // Copy native library (e.g., libthoth-*.so)
                        let dest = jni_libs_dir.join(path.file_name().unwrap());
                        std::fs::copy(&path, &dest)
                            .expect(&format!("Failed to copy {} to jniLibs", path.display()));
                    }
                }
            }
        }
    }

    // For Android Dioxus builds, copy native libraries into jniLibs/<abi> so they are packaged into the APK
    #[cfg(all(target_os = "android", not(target_arch = "wasm32")))]
    {
        use std::ffi::OsStr;

        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        let target_triple = std::env::var("TARGET").unwrap_or_default();

        // Determine ABI directory name (e.g., "arm64-v8a", "armeabi-v7a")
        let abi = if target_triple.contains("aarch64") {
            "arm64-v8a"
        } else if target_triple.contains("armv7") {
            "armeabi-v7a"
        } else if target_triple.contains("i686") {
            "i686"
        } else if target_triple.contains("x86_64") {
            "x86_64"
        } else {
            // Unknown ABI; skip bundling
            ""
        };

        if !abi.is_empty() {
            let dx_android_project = std::path::Path::new(&manifest_dir)
                .join("target")
                .join("dx")
                .join("thoth")
                .join(&profile)
                .join("android")
                .join("app")
                .join("app");
            let jni_libs_dir = dx_android_project.join("src/main/jniLibs").join(abi);
            let src_lib_android = std::path::Path::new(&manifest_dir).join("lib/android").join(abi);

            // Ensure destination exists
            std::fs::create_dir_all(&jni_libs_dir).expect("Failed to create jniLibs directory");

            if src_lib_android.exists() {
                for entry in std::fs::read_dir(&src_lib_android).expect("Failed to read lib/android/abi") {
                    let entry = entry.expect("Invalid lib entry");
                    let path = entry.path();
                    let file_type = entry.file_type().expect("Failed to get file type");

                    if file_type.is_file() {
                        // Track changes
                        println!("cargo:rerun-if-changed={}", path.display());

                        // Copy native library (e.g., libthoth-*.so)
                        let dest = jni_libs_dir.join(path.file_name().unwrap());
                        std::fs::copy(&path, &dest)
                            .expect(&format!("Failed to copy {} to jniLibs", path.display()));
                    }
                }
            }
        }
    }
 }