# Android Architecture Support

## Supported Architectures

### ✅ ARM64 (aarch64-linux-android)
- **Full local inference** with llama.cpp
- Bundled Bonsai 1.7B model
- All features working
- **Minimum requirement:** Android 5.0+ (API 21+) on 64-bit ARM device

### ❌ ARMv7 (armeabi-v7a) - NOT SUPPORTED
- Dioxus/Manganis only supports 64-bit Android
- No workaround available
- **Solution:** Use ARM64 device (most phones from 2015+)

### ⚠️ Web WASM (wasm32-unknown-unknown)
- UI only, no local inference
- Can connect to remote inference (Nostr, API, etc.)
- Works in modern browsers

## How to Check Your Device

```bash
# Check if your device is 64-bit
adb shell getprop ro.product.cpu.abi

# ARM64 devices show: arm64-v8a
# ARMv7 devices show: armeabi-v7a (NOT SUPPORTED)
```

## Building

### For ARM64 (Recommended)
```bash
cd /home/awides/dev/bn/thoth
dx build --android --target aarch64-linux-android
# APK: target/dx/thoth/release/android/app/app/build/outputs/apk/release/app-release.apk
```

### For Web WASM
```bash
dx build --web
# Output: target/dx/thoth/release/web/
```

## Device Compatibility

**Works on:**
- ✅ Most Android phones from 2016+
- ✅ All Android tablets from 2018+
- ✅ ARM64 emulators

**Does NOT work on:**
- ❌ ARMv7-only devices (old 32-bit phones)
- ❌ x86 Android (without ARM translation)

## Why No ARMv7?

1. **Dioxus Limitation**: The Manganis asset system (used by Dioxus) only supports 64-bit Android
2. **Performance**: llama.cpp optimizations require ARM64 NEON/FP16
3. **Industry Standard**: Google Play requires 64-bit support since 2019

## Solutions for ARMv7 Devices

If you have an ARMv7-only device:

1. **Use a newer device** - Most phones from 2016+ are ARM64
2. **Use the web version** - Run in browser with remote inference
3. **Use an emulator** - Create ARM64 emulator for testing

## Current Status

| Architecture | Status | Inference | Notes |
|-------------|--------|-----------|-------|
| ARM64 | ✅ Full Support | Local (llama.cpp) | Recommended |
| ARMv7 | ❌ Not Supported | N/A | Dioxus limitation |
| WASM | ⚠️ UI Only | Remote only | Web browsers |
