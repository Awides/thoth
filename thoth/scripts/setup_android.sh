#!/bin/bash
# Android Build Setup Script for Thoth
# This script sets up the Android NDK environment needed to build Thoth for Android

set -e

echo "=== Thoth Android Setup ==="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
ANDROID_HOME="${ANDROID_HOME:-$HOME/android-sdk}"
NDK_VERSION="26.1.10909125"
ANDROID_API="34"

echo -e "${YELLOW}Step 1: Checking for existing Android SDK...${NC}"
if [ -d "$ANDROID_HOME" ]; then
    echo -e "${GREEN}✓ Android SDK found at: $ANDROID_HOME${NC}"
else
    echo "Android SDK not found. Installing..."
    mkdir -p "$ANDROID_HOME"
    
    # Download command-line tools
    echo "Downloading Android command-line tools..."
    cd /tmp
    wget -q https://dl.google.com/android/repository/commandlinetools-linux-11076708_linux.zip
    unzip -q commandlinetools-linux-11076708_linux.zip
    mv cmdline-tools "$ANDROID_HOME/"
    
    echo -e "${GREEN}✓ Command-line tools installed${NC}"
fi

# Set up PATH
export PATH=$PATH:$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools

echo ""
echo -e "${YELLOW}Step 2: Installing NDK and platform tools...${NC}"

# Accept licenses
echo "Accepting licenses..."
yes | sdkmanager --licenses > /dev/null 2>&1 || true

# Install components
echo "Installing platform-tools..."
sdkmanager "platform-tools" > /dev/null 2>&1 || true

echo "Installing platform android-$ANDROID_API..."
sdkmanager "platforms;android-$ANDROID_API" > /dev/null 2>&1 || true

echo "Installing NDK version $NDK_VERSION..."
sdkmanager "ndk;$NDK_VERSION" > /dev/null 2>&1 || true

export ANDROID_NDK_ROOT=$ANDROID_HOME/ndk/$NDK_VERSION

echo -e "${GREEN}✓ NDK installed at: $ANDROID_NDK_ROOT${NC}"

echo ""
echo -e "${YELLOW}Step 3: Setting up build environment...${NC}"

# Create toolchain directory structure
TOOLCHAIN_DIR="$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64"

if [ -d "$TOOLCHAIN_DIR" ]; then
    echo -e "${GREEN}✓ Toolchain found at: $TOOLCHAIN_DIR${NC}"
    
    # Set environment variables
    export CC_aarch64_linux_android=aarch64-linux-android21-clang
    export CFLAGS_aarch64_linux_android="--sysroot=$TOOLCHAIN_DIR/sysroot"
    
    echo -e "${GREEN}✓ Environment variables set${NC}"
else
    echo -e "${RED}✗ Toolchain not found. NDK installation may be incomplete.${NC}"
    exit 1
fi

echo ""
echo -e "${YELLOW}Step 4: Testing Android build tools...${NC}"

# Test if we can find the compiler
if command -v aarch64-linux-android21-clang &> /dev/null; then
    echo -e "${GREEN}✓ Android compiler found${NC}"
else
    echo -e "${YELLOW}⚠ Android compiler not in PATH. Adding to PATH...${NC}"
    export PATH=$PATH:$TOOLCHAIN_DIR/bin
    
    if command -v aarch64-linux-android21-clang &> /dev/null; then
        echo -e "${GREEN}✓ Android compiler now available${NC}"
    else
        echo -e "${RED}✗ Could not find Android compiler${NC}"
        exit 1
    fi
fi

echo ""
echo -e "${YELLOW}Step 5: Installing Rust target...${NC}"
rustup target add aarch64-linux-android
echo -e "${GREEN}✓ Rust Android target installed${NC}"

echo ""
echo -e "${YELLOW}Step 6: Building llama.cpp for Android...${NC}"
echo "This step requires the llama.cpp source code."
echo "Please ensure you have cloned: https://github.com/PrismML/llama.cpp"
echo ""

LLAMA_DIR="${LLAMA_DIR:-$HOME/src/llama.cpp}"

if [ -d "$LLAMA_DIR" ]; then
    echo -e "${GREEN}✓ llama.cpp found at: $LLAMA_DIR${NC}"
    echo ""
    echo "Building llama.cpp for Android ARM64..."
    
    cd "$LLAMA_DIR"
    
    export NDK=$ANDROID_NDK_ROOT
    export TOOLCHAIN=$NDK/toolchains/llvm/prebuilt/linux-x86_64
    export AR=llvm-ar
    export CC=aarch64-linux-android21-clang
    
    mkdir -p build-android
    cd build-android
    
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
    
    echo -e "${GREEN}✓ llama.cpp built successfully${NC}"
    echo ""
    echo "Copying libraries to Thoth..."
    
    THOTH_LIB_DIR="$(dirname $(dirname $(dirname $(readlink -f $0))))/lib/android/arm64-v8a"
    mkdir -p "$THOTH_LIB_DIR"
    
    cp install/lib/libllama.so "$THOTH_LIB_DIR/"
    cp install/lib/libggml*.so "$THOTH_LIB_DIR/"
    cp install/lib/libllama-common.so "$THOTH_LIB_DIR/"
    
    echo -e "${GREEN}✓ Libraries copied to: $THOTH_LIB_DIR${NC}"
    
else
    echo -e "${YELLOW}⚠ llama.cpp not found at $LLAMA_DIR${NC}"
    echo "To build llama.cpp for Android:"
    echo "  1. Clone: git clone https://github.com/PrismML/llama.cpp ~/src/llama.cpp"
    echo "  2. Run: export LLAMA_DIR=~/src/llama.cpp && $0"
fi

echo ""
echo -e "${GREEN}=== Setup Complete ===${NC}"
echo ""
echo "Environment variables to set in your shell:"
echo "  export ANDROID_HOME=$ANDROID_HOME"
echo "  export ANDROID_NDK_ROOT=$ANDROID_NDK_ROOT"
echo "  export PATH=\$PATH:\$ANDROID_HOME/platform-tools:\$ANDROID_NDK_ROOT/toolchains/llvm/prebuilt/linux-x86_64/bin"
echo ""
echo "To build Thoth for Android:"
echo "  cd /path/to/thoth"
echo "  dx build --android --release"
echo ""
echo "Or to test the build:"
echo "  cargo build --target aarch64-linux-android"
echo ""
