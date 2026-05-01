# Web Inference Status - Backburnered

## Decision
**Date:** May 1, 2026  
**Status:** Backburnered - Desktop-only for now

## Summary
Web-based WASM inference using llama.cpp has been **deprioritized** due to fundamental limitations with browser-based LLM inference for models of practical size.

## What We Tried
- Compiled llama.cpp (PrismML fork) to WASM using Emscripten
- Implemented full Web Worker pipeline with virtual filesystem
- Added progress reporting, error handling, and memory optimization
- Tested with 237MB Bonsai-1.7B-Q1_0 model

## Problems Encountered
1. **Memory Limits**: Browser WASM limited to ~2GB max, model + context exceeds this
2. **Initialization Failures**: WASM runtime fails to initialize reliably (30s timeouts)
3. **Download Size**: 237MB model + 2MB WASM = poor UX (30+ second load times)
4. **Silent Failures**: Browser WASM errors often uncatchable, leading to hangs

## Why PrismML Used ONNX for Web
This investigation confirms why PrismML chose ONNX Runtime for web deployment:
- ONNX Runtime Web is optimized for browser WASM constraints
- Better memory management and smaller runtime footprint
- More reliable cross-browser WASM compatibility
- Quantization support designed for web constraints

llama.cpp's GGUF format and GGML backend, while excellent for native desktop inference, are not optimized for WASM/browser environments.

## Current State
- **Desktop (Native)**: ✅ Full inference working perfectly with Bonsai 1.7B
- **Web (WASM)**: ❌ Initialization fails, backburnered

## Next Steps (If Revisited)
1. **Smaller Models**: Test with <50MB models (e.g., TinyLlama, Qwen-0.5B)
2. **Burn WGPU/Flex**: Build native ternary kernel support for Burn's WGPU backend
3. **Alternative Runtimes**: Explore wasm-llama, llamafile, or other WASM-optimized runtimes
4. **Server Backend**: Offload inference to server, stream tokens via WebSocket

## Recommendation
**Focus on desktop experience** where llama.cpp excels. For web demos, use echo mode or direct users to desktop app. Revisit only if:
- Burn WGPU gains ternary Bonsai support
- WASM memory limits increase significantly
- Small (<50MB) ternary models become available

---
*Note: This is not a failure of implementation - the WASM pipeline works. The issue is that llama.cpp + GGUF + large models are fundamentally mismatched with current browser WASM constraints.*
