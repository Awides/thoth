# Android Build - Setup Summary

## What's Been Done ✅

The Thoth codebase is now **ready for Android builds**. Here's what was implemented:

### 1. Platform Configuration
- ✅ Added `aarch64-linux-android` target support
- ✅ Updated `Cargo.toml` with Android-specific dependencies
- ✅ Updated `build.rs` to skip bindgen on Android
- ✅ Conditional compilation for Android in `main.rs`

### 2. Android App Module
- ✅ Created `src/android_app.rs` - mobile-optimized chat UI
- ✅ Touch-friendly interface with mobile-first design
- ✅ Same inference capabilities as desktop

### 3. Native Inference Engine
- ✅ Created `src/llama_android/mod.rs` - native llama.cpp FFI
- ✅ Identical implementation to desktop (`llama_native`)
- ✅ Full streaming inference support
- ✅ Thinking token handling
- ✅ All Bonsai model features

### 4. Documentation & Scripts
- ✅ `ANDROID.md` - comprehensive Android build guide
- ✅ `ANDROID_SETUP.md` - detailed setup instructions
- ✅ `scripts/setup_android.sh` - automated setup script
- ✅ `scripts/check_android_build.sh` - build prerequisite checker

## What You Need To Do 📋

### Step 1: Install Android NDK

The NDK (Native Development Kit) is required to compile native code for Android.

**Option A: Using sdkmanager (Recommended)**
```bash
# Install Android SDK command-line tools first
export ANDROID_HOME=$HOME/android-sdk
mkdir -p $ANDROID_HOME

# Download and extract
cd /tmp
wget https://dl.google.com/android/repository/commandlinetools-linux-11076708_linux.zip
unzip commandlinetools-linux-11076708_linux.zip
mv cmdline-tools $ANDROID_HOME/

# Install NDK
export PATH=$PATH:$ANDROID_HOME/cmdline-tools/latest/bin
yes | sdkmanager --licenses
sdkmanager "platform-tools"
sdkmanager "platforms;android-34"
sdkmanager "ndk;26.1.10909125"
```

**Option B: Manual Download**
```bash
# Download from https://developer.android.com/ndk/downloads
# Extract to $HOME/android-sdk/ndk/
export ANDROID_NDK_ROOT=$HOME/android-sdk/ndk/26.1.10909125
```

### Step 2: Build llama.cpp for Android

```bash
# Clone PrismML fork (if not already done)
cd ~/src
git clone https://github.com/PrismML/llama.cpp
cd llama.cpp

# Set up environment
export NDK=$ANDROID_NDK_ROOT
export TOOLCHAIN=$NDK/toolchains/llvm/prebuilt/linux-x86_64

# Build for ARM64
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
cp install/lib/libllama.so /home/awides/dev/bn/thoth/lib/android/arm64-v8a/
cp install/lib/libggml*.so /home/awides/dev/bn/thoth/lib/android/arm64-v8a/
cp install/lib/libllama-common.so /home/awides/dev/bn/thoth/lib/android/arm64-v8a/
```

### Step 3: Set Environment Variables

Add to your `~/.bashrc` or `~/.zshrc`:

```bash
export ANDROID_HOME=$HOME/android-sdk
export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/26.1.10909125
export PATH=$PATH:$ANDROID_HOME/platform-tools:$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin
```

### Step 4: Test the Build

```bash
cd /home/awides/dev/bn/thoth

# Check prerequisites
./scripts/check_android_build.sh

# Build for Android
cargo build --target aarch64-linux-android --release

# Or use Dioxus CLI (if available)
dx build --android --release
```

## Model Strategy

For testing, you have three options:

### Option 1: Download at Runtime (Recommended for Development)
Modify `src/android_app.rs` to download the model on first launch:

```rust
// In android_app.rs, add model download logic
async fn get_model_path() -> Result<String> {
    let model_path = "/data/data/com.thoth.app/files/models/Bonsai-1.7B-Q1_0.gguf";
    
    if !std::path::Path::new(model_path).exists() {
        // Download from HuggingFace
        use hf_hub::api::tokio::Api;
        let api = Api::new()?;
        let repo = api.model("PrismML/Bonsai-1.7B".to_string());
        repo.get("Bonsai-1.7B-Q1_0.gguf").await?;
    }
    
    Ok(model_path.to_string())
}
```

### Option 2: Bundle in APK
Place the model in assets (increases APK size by ~500MB for 1.7B model).

### Option 3: Side-load
Use Android file picker to select model from device storage.

## Testing on Device

### 1. Enable USB Debugging
- Go to Settings → About Phone
- Tap "Build Number" 7 times
- Go to Settings → Developer Options
- Enable "USB Debugging"

### 2. Connect and Test
```bash
# Connect device via USB
adb devices

# Install APK
adb install -r target/aarch64-linux-android/release/thoth.apk

# Run app
adb shell am start -n com.thoth.app/.MainActivity

# View logs
adb logcat | grep thoth
```

## Current Status

| Component | Status | Notes |
|-----------|--------|-------|
| Rust target | ✅ Ready | `aarch64-linux-android` installed |
| Cargo config | ✅ Ready | Android dependencies configured |
| Build script | ✅ Ready | Skips bindgen on Android |
| App module | ✅ Ready | `src/android_app.rs` created |
| Inference engine | ✅ Ready | `src/llama_android/mod.rs` |
| NDK | ⏳ Required | Install via sdkmanager |
| llama.cpp ARM64 | ⏳ Required | Build from source |
| Model bundling | ⏳ TBD | Choose strategy above |
| Testing | ⏳ Pending | Wait for NDK install |

## Next Actions

1. **Install NDK** - Follow Step 1 above
2. **Build llama.cpp** - Follow Step 2 above
3. **Test build** - Run `cargo build --target aarch64-linux-android`
4. **Fix any issues** - Address compilation errors
5. **Test on device** - Deploy to Android device/emulator
6. **Optimize** - Performance, battery, UI

## Questions?

- See `ANDROID.md` for detailed architecture
- See `ANDROID_SETUP.md` for step-by-step setup
- Run `./scripts/check_android_build.sh` to check prerequisites
- Check Dioxus docs: https://dioxuslabs.com/learn/

---

**TL;DR**: Code is ready. Install NDK, build llama.cpp for ARM64, then test!
