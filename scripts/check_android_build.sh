#!/bin/bash
# Check Android build prerequisites

echo "=== Thoth Android Build Check ==="
echo ""

# Check Rust target
echo -n "Rust Android target: "
if rustup target list --installed | grep -q "aarch64-linux-android"; then
    echo "✓ Installed"
else
    echo "✗ Missing - run: rustup target add aarch64-linux-android"
fi

# Check Android SDK
echo -n "Android SDK: "
if [ -n "$ANDROID_HOME" ] && [ -d "$ANDROID_HOME" ]; then
    echo "✓ $ANDROID_HOME"
else
    echo "✗ Not set - export ANDROID_HOME=\$HOME/android-sdk"
fi

# Check NDK
echo -n "Android NDK: "
if [ -n "$ANDROID_NDK_ROOT" ] && [ -d "$ANDROID_NDK_ROOT" ]; then
    echo "✓ $ANDROID_NDK_ROOT"
else
    echo "✗ Not set - export ANDROID_NDK_ROOT=\$ANDROID_HOME/ndk/26.1.10909125"
fi

# Check for Android compiler
echo -n "Android compiler: "
if command -v aarch64-linux-android21-clang &> /dev/null; then
    echo "✓ Found"
else
    echo "✗ Not in PATH - add NDK toolchain to PATH"
fi

# Check for llama.cpp Android libs
echo -n "llama.cpp Android libs: "
if [ -f "lib/android/arm64-v8a/libllama.so" ]; then
    echo "✓ Found"
else
    echo "✗ Missing - build llama.cpp for Android"
fi

# Check for model
echo -n "GGUF model: "
if [ -f "models/Bonsai-1.7B-Q1_0.gguf" ]; then
    echo "✓ Found"
elif [ -f "src/android/assets/models/Bonsai-1.7B-Q1_0.gguf" ]; then
    echo "✓ Bundled in assets"
else
    echo "✗ Missing - place model in models/ directory"
fi

echo ""
echo "To set up Android build environment:"
echo "  1. Install Android NDK: sdkmanager 'ndk;26.1.10909125'"
echo "  2. Build llama.cpp for Android (see ANDROID.md)"
echo "  3. Copy libraries to lib/android/arm64-v8a/"
echo "  4. Set environment variables"
echo "  5. Run: dx build --android --release"
