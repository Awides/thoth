# Android Build Setup for Thoth

This guide covers building Thoth for Android with native llama.cpp inference.

## Prerequisites

### 1. Install Android SDK Command-Line Tools

```bash
# Create Android SDK directory
export ANDROID_HOME=$HOME/android-sdk
mkdir -p $ANDROID_HOME

# Download and extract command-line tools
cd /tmp
wget https://dl.google.com/android/repository/commandlinetools-linux-11076708_linux.zip
unzip commandlinetools-linux-11076708_linux.zip
mv cmdline-tools $ANDROID_HOME/
```

### 2. Install NDK and Platform Tools

```bash
export PATH=$PATH:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools

# Accept licenses
yes | sdkmanager --licenses

# Install required components
sdkmanager "platform-tools"
sdkmanager "platforms;android-34"
sdkmanager "ndk;26.1.10909125"
```

### 3. Set Up Build Environment

```bash
export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/26.1.10909125
export PATH=$PATH:$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin

# Verify installation
aarch64-linux-android21-clang --version
```

## Building llama.cpp for Android

### Option A: Build from Source (Recommended)

```bash
# Clone PrismML fork
cd ~/src
git clone https://github.com/PrismML/llama.cpp
cd llama.cpp

# Set up Android toolchain
export NDK=$ANDROID_NDK_ROOT
export TOOLCHAIN=$NDK/toolchains/llvm/prebuilt/linux-x86_64
export AR=llvm-ar
export CC=aarch64-linux-android21-clang

# Build for ARM64
mkdir -p build-android && cd build-android
cmake .. \
  -DCMAKE_TOOLCHAIN_FILE=$NDK/build/cmake/android.toolchain.cmake \
  -DANDROID_ABI=arm64-v8a \
  -DANDROID_PLATFORM=android-21 \
  -DCMAKE_BUILD_TYPE=Release \
  -DLLAMA_CUDA=off \
  -DLLAMA_OPENMP=off

cmake --build . -j$(nproc)

# Install to prefix
cmake --install . --prefix install
```

### Option B: Pre-built Libraries

Copy the built libraries to your project:

```bash
# From llama.cpp build
cp install/lib/libllama.so /path/to/thoth/lib/android/arm64-v8a/
cp install/lib/libggml*.so /path/to/thoth/lib/android/arm64-v8a/
cp install/lib/libllama-common.so /path/to/thoth/lib/android/arm64-v8a/
```

## Building the GGUF Model Bundle

### Compress and Bundle Model

```bash
cd /path/to/thoth

# Create assets directory for Android
mkdir -p src/android/assets/models

# Compress model (optional but recommended)
# GGUF files are already fairly compact, but gzip can help
gzip -c models/Bonsai-1.7B-Q1_0.gguf > src/android/assets/models/Bonsai-1.7B-Q1_0.gguf.gz

# The model will be extracted at runtime to the app's data directory
```

### Alternative: Download at First Launch

Instead of bundling, download the model on first run:

```rust
// In android_app.rs, add model download logic
async fn download_model_if_needed() -> Result<String> {
    let model_path = "/data/data/com.thoth.app/files/models/Bonsai-1.7B-Q1_0.gguf";
    
    if !std::path::Path::new(model_path).exists() {
        // Download from HuggingFace or other source
        // Example: hf-hub download
    }
    
    Ok(model_path.to_string())
}
```

## Build Thoth for Android

### 1. Configure Cargo

Add Android target:
```bash
rustup target add aarch64-linux-android
```

### 2. Build APK or Library

For a standalone Rust binary (requires Android wrapper):
```bash
export CC_aarch64_linux_android=aarch64-linux-android21-clang
export CFLAGS_aarch64_linux_android="--sysroot=$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/sysroot"

cargo build --target aarch64-linux-android --release
```

### 3. Create Android Project Structure

```
thoth-android/
├── app/
│   ├── src/
│   │   ├── main/
│   │   │   ├── AndroidManifest.xml
│   │   │   ├── java/com/thoth/app/
│   │   │   │   └── MainActivity.java
│   │   │   └── rust/
│   │   │       ├── Cargo.toml
│   │   │       └── src/
│   │   │       │   └── lib.rs
│   │   └── AndroidManifest.xml
│   └── build.gradle
└── build.gradle
```

### 4. Use cargo-apk or cargo-mobile2

```bash
# Install cargo-apk
cargo install cargo-apk

# Or use cargo-mobile2 for better integration
cargo install cargo-mobile2

# Build and deploy
cargo apk build
cargo apk run
```

## Runtime Model Extraction

The bundled model needs to be extracted to a writable location:

```rust
// In android_app.rs initialization
fn extract_bundled_model() -> Result<String> {
    let model_path = "/data/data/com.thoth.app/files/models/Bonsai-1.7B-Q1_0.gguf";
    
    // Check if already extracted
    if std::path::Path::new(model_path).exists() {
        return Ok(model_path.to_string());
    }
    
    // Extract from assets
    let model_data = include_bytes!("../assets/models/Bonsai-1.7B-Q1_0.gguf.gz");
    
    // Create directory
    std::fs::create_dir_all("/data/data/com.thoth.app/files/models/")?;
    
    // Decompress and write
    let mut decoder = flate2::read::GzDecoder::new(model_data);
    let mut file = std::fs::File::create(model_path)?;
    std::io::copy(&mut decoder, &mut file)?;
    
    Ok(model_path.to_string())
}
```

## Testing on Device/Emulator

### 1. Set Up Emulator

```bash
# Create AVD
avdmanager create avd -n thoth_test -k "system-images;android-34;default;arm64-v8a"

# Start emulator
emulator -avd thoth_test
```

### 2. Deploy and Test

```bash
# Install APK
adb install -r target/aarch64-linux-android/release/thoth.apk

# Run on device
adb shell am start -n com.thoth.app/.MainActivity

# View logs
adb logcat | grep thoth
```

## Troubleshooting

### Build Errors

**Error: `aarch64-linux-android-clang` not found**
- Ensure NDK is installed and PATH is set correctly
- Check `$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin`

**Error: `unresolved external symbol`**
- Verify llama.cpp libraries are built for Android
- Check library architecture matches target (arm64-v8a)

### Runtime Errors

**Model not found**
- Verify model is bundled in assets or downloaded
- Check file permissions on Android

**Inference crashes**
- Ensure llama.cpp libraries are built with same headers
- Check memory limits on Android device

## Next Steps

1. ✅ Set up Android NDK environment
2. ✅ Build llama.cpp for ARM64
3. ✅ Bundle or download GGUF model
4. ⏳ Create Android wrapper app (Java/Kotlin)
5. ⏳ Integrate with Dioxus mobile backend
6. ⏳ Test inference on device

## References

- [Dioxus Mobile](https://dioxuslabs.com/learn/0.4/guides/mobile.html)
- [Android NDK](https://developer.android.com/ndk)
- [llama.cpp Android Build](https://github.com/ggerganov/llama.cpp/blob/master/docs/build.md#android)
- [cargo-apk](https://github.com/rust-android/cargo-apk)
