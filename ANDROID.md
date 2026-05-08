# Android Build Status for Thoth

## Current State

✅ **Completed:**
- Android target configuration (`aarch64-linux-android`)
- Android-specific app module (`src/android_app.rs`)
- Native llama.cpp FFI bindings for Android (`src/llama_android/`)
- Build configuration in `Cargo.toml` and `build.rs`
- Dioxus CLI integration (`dx build --android`)

⏳ **In Progress:**
- Android NDK setup and llama.cpp compilation
- Model bundling strategy
- Testing on device/emulator

## Quick Start (Once NDK is Installed)

```bash
# 1. Set up Android environment
export ANDROID_HOME=$HOME/android-sdk
export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/26.1.10909125
export PATH=$PATH:$ANDROID_HOME/platform-tools:$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin

# 2. Build for Android
cd /path/to/thoth
dx build --android --release

# Or test the build
cargo build --target aarch64-linux-android --release
```

## Architecture

### Platform Detection

Thoth uses conditional compilation to support multiple platforms:

```rust
// Desktop (Linux/Windows/macOS)
#[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
#[path = "llama_native/mod.rs"]
pub mod llama;

// Android
#[cfg(target_os = "android")]
#[path = "llama_android/mod.rs"]
pub mod llama;

// Web (WASM)
#[cfg(target_arch = "wasm32")]
#[path = "llama_web/mod.rs"]
pub mod llama;
```

### Inference Engine

Android uses the **same llama.cpp FFI** as desktop, compiled for ARM64:

```rust
// src/llama_android/mod.rs - identical to llama_native
pub fn spawn_inference_thread() -> Arc<Mutex<Option<Sender<Command>>>> {
    // Same implementation as desktop
    // Uses native llama.cpp via FFI
}
```

This means:
- ✅ Full local inference on device (no server needed)
- ✅ Same Bonsai model support
- ✅ Same streaming inference
- ✅ Same thinking token handling

### Model Bundling

The GGUF model can be:

1. **Bundled in APK** (larger APK, offline-ready)
   ```rust
   // Extract from assets at runtime
   let model_data = include_bytes!("../assets/models/Bonsai-1.7B-Q1_0.gguf");
   ```

2. **Downloaded at first launch** (smaller APK, requires network)
   ```rust
   // Download from HuggingFace
   use hf_hub::api::tokio::Api;
   let api = Api::new()?;
   let repo = api.model("PrismML/Bonsai-1.7B".to_string());
   let model_path = repo.get("Bonsai-1.7B-Q1_0.gguf").await?;
   ```

3. **Side-loaded** (user provides model)
   ```rust
   // Use Android file picker to select model
   // Store in app's data directory
   ```

**Recommendation:** Option 2 (download at first launch) for initial release.

## Build Requirements

### Android NDK

Minimum version: **r26** (26.1.10909125)

```bash
# Install via sdkmanager
sdkmanager "ndk;26.1.10909125"

# Or download from: https://developer.android.com/ndk/downloads
```

### llama.cpp for Android

Build llama.cpp for ARM64:

```bash
cd ~/src/llama.cpp
export NDK=$ANDROID_NDK_ROOT

mkdir build-android && cd build-android
cmake .. \
  -DCMAKE_TOOLCHAIN_FILE=$NDK/build/cmake/android.toolchain.cmake \
  -DANDROID_ABI=arm64-v8a \
  -DANDROID_PLATFORM=android-21 \
  -DCMAKE_BUILD_TYPE=Release \
  -DLLAMA_CUDA=off \
  -DLLAMA_OPENMP=off

cmake --build . -j$(nproc)
cmake --install . --prefix install

# Copy libraries to Thoth
cp install/lib/libllama.so /path/to/thoth/lib/android/arm64-v8a/
cp install/lib/libggml*.so /path/to/thoth/lib/android/arm64-v8a/
cp install/lib/libllama-common.so /path/to/thoth/lib/android/arm64-v8a/
```

### Rust Target

```bash
rustup target add aarch64-linux-android
```

## Testing

### On Emulator

```bash
# Create AVD (if not exists)
avdmanager create avd -n thoth_test -k "system-images;android-34;default;arm64-v8a"

# Start emulator
emulator -avd thoth_test

# Build and install
dx build --android --release
dx build --android --release --upload

# Or manually
adb install -r target/aarch64-linux-android/release/thoth.apk
```

### On Device

```bash
# Enable USB debugging on device
# Connect via USB

# Install
adb install -r target/aarch64-linux-android/release/thoth.apk

# Run
adb shell am start -n com.thoth.app/.MainActivity

# View logs
adb logcat | grep thoth
```

## Known Issues

### 1. Dioxus Mobile Support

Dioxus mobile support is still maturing. Current limitations:
- May need to use `dioxus-mobile` instead of `dioxus` crate
- Some desktop features may not work on mobile

### 2. Native Library Loading

Android requires specific library naming and location:
```:
lib/
  arm64-v8a/
    libllama.so
    libggml.so
    libggml-cpu.so
    libggml-base.so
    libllama-common.so
```

### 3. Model File Access

Android sandboxing means:
- Models must be in app's data directory
- Use Android's file picker for external models
- Consider download-on-first-launch approach

## Next Steps

1. **Install NDK** and build llama.cpp for Android
2. **Test basic build** with `cargo build --target aarch64-linux-android`
3. **Resolve Dioxus mobile issues** (if any)
4. **Implement model download** or bundling
5. **Test on device** with actual inference
6. **Optimize for mobile** (battery, performance, UI)

## Resources

- [Dioxus Android Examples](https://github.com/DioxusLabs/dioxus/tree/main/examples)
- [Android NDK Guide](https://developer.android.com/ndk/guides)
- [llama.cpp Android Build](https://github.com/ggerganov/llama.cpp/blob/master/docs/build.md#android)
- [Cargo APK](https://github.com/rust-android/cargo-apk)

## Troubleshooting

### Build fails with "aarch64-linux-android-clang not found"

Ensure NDK is installed and PATH is set:
```bash
export PATH=$PATH:$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin
```

### Runtime error: "library not found"

Verify native libraries are in the correct location:
```bash
ls -la lib/android/arm64-v8a/
# Should contain: libllama.so, libggml*.so, libllama-common.so
```

### App crashes on startup

Check logs:
```bash
adb logcat | grep thoth
```

Common issues:
- Missing native libraries
- Model file not found
- Permission issues

---

**Status:** 🚧 Setup phase - NDK installation and llama.cpp build required before testing.
