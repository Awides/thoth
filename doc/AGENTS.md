# AGENTS.md: Guide for Working in the Thoth Codebase

## Project Overview

**Thoth** is a Rust desktop application (using Dioxus) that integrates llama.cpp for on-device LLM inference. The project uses **PrismML's fork of llama.cpp** which adds support for ternary Bonsai models.

- **Language**: Rust (edition 2021)
- **Framework**: Dioxus (desktop, main branch) – GUI with chat interface
- **Runtime**: llama.cpp (PrismML fork) via FFI
- **Model format**: GGUF (quantized, including ternary Bonsai)
- **Platform**: Linux (x86_64), targeting WASM/web worker for web

---

## Directory Structure

```
thoth/
├── Cargo.toml           # Manifest (includes tokio, hf-hub, directories, toml, dioxus git main)
├── Dioxus.toml          # Dioxus desktop configuration
├── build.rs             # Generates bindings; expects headers at /tmp/llama.cpp-build/
├── lib/                 # Prebuilt shared libraries from PrismML/llama.cpp
│   ├── libllama.so
│   ├── libggml.so
│   ├── libggml-cpu.so
│   ├── libggml-base.so
│   └── libllama-common.so
├── models/              # Place GGUF model files here (gitignored)
│   └── Bonsai-1.7B-Q1_0.gguf
├── assets/
│   └── tailwind.css     # Tailwind CSS v4 entry point
└── src/
    ├── main.rs          # Dioxus app launcher + module declarations
    ├── app.rs           # Dioxus GUI: chat interface, llama.cpp integration, Tailwind CSS
    ├── engine.rs        # Engine abstraction (currently just Config + handle factory)
    ├── llama.rs        # Unified native inference wrapper (desktop + Android)
    └── test_standalone.rs  # Legacy (duplicate FFI code – not used)
```

---

## Setup: Build llama.cpp (PrismML Fork)

**Critical**: The headers and shared libraries **must come from the same build** of the **PrismML/llama.cpp** fork. Mixing with upstream llama.cpp will cause crashes (assertions in sampler).

### One-time Setup

```bash
# 1. Clone the correct fork
git clone https://github.com/PrismML/llama.cpp ~/src/llama.cpp
cd ~/src/llama.cpp

# 2. Build with CPU only (adjust CMake flags if you want CUDA/OpenCL)
mkdir -p build && cd build
cmake .. -DLLAMA_CUDA=off -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=install
cmake --build . -j$(nproc)

# 3. Install to a prefix (headers to install/include/, libs to install/lib/)
cmake --install install

# 4. Prepare include tree that build.rs expects:
#    A directory containing:
#      include/llama.h
#      ggml/include/
#    You can either copy as before, or just set LLAMA_HOME to the install prefix
#    and rely on the build script to locate the headers.
#
#    Option A (staging): Create $LLAMA_HOME as a unified tree:
export LLAMA_HOME=$HOME/.llama.cpp
mkdir -p $LLAMA_HOME/include
cp -r install/include/* $LLAMA_HOME/include/
cp -r ../ggml/include $LLAMA_HOME/ggml/include/
#
#    Option B (symlinks): Point LLAMA_HOME to install and ensure ggml is sibling:
#      LLAMA_HOME=$HOME/src/llama.cpp/build/install
#      And have a symlink $LLAMA_HOME/../ggml/include pointing to ../ggml/include
#      (The build script expects $LLAMA_HOME/ggml/include to exist)

# 5. Copy shared libraries into the project's lib/ directory:
cp install/lib/libllama.so* install/lib/libggml*.so install/lib/libllama-common.so /home/awides/dev/thoth/lib/
#    For Android, additionally copy to lib/android/arm64-v8a/ (see below).
```

**Notes**:
- The build script uses `LLAMA_HOME` (or `LLAMA_CPP_BUILD`) environment variable to locate headers. If unset, it defaults to `/tmp/llama.cpp-build` for backward compatibility.
- For **Android**: you need a separate set of `.so` files for `arm64-v8a` placed in `lib/android/arm64-v8a/`. These should be built with the NDK toolchain. The Android build uses pre-generated Rust bindings (`src/llama/bindings.rs`); `bindgen` is only used for desktop builds.
- You need `libclang` installed for `bindgen` on desktop (`apt install libclang-dev` on Ubuntu/Debian).

---

## Building Thoth

```bash
cd /home/awides/dev/bn/thoth
cargo build                    # debug build (Dioxus desktop)
cargo build --release          # optimized build
```

If the build fails with "Unable to generate bindings", check that the headers exist in `/tmp/llama.cpp-build/`.

---

## Running

Set the library path so the dynamic linker can find the `.so` files:

```bash
export LD_LIBRARY_PATH=$PWD/lib
```

Then run:

```bash
# Launch the Dioxus desktop application
cargo run

# Release build
cargo run --release
```

The default model path is `./models/Bonsai-1.7B-Q1_0.gguf` (configured in `src/app.rs`). Use the file picker in the app to load a different GGUF model.

---

## Architecture: How It Works

### Inference Engine and Threading

- The **inference engine** lives in `src/llama/mod.rs`. It provides an `Engine` struct that wraps the llama.cpp FFI.
- The `spawn_inference_thread()` function creates a dedicated thread that owns the `Engine` and processes commands sequentially over a synchronous MPSC channel.
- The Dioxus UI interacts with the inference thread by sending `Command` messages (Load, Infer, Unload) and blocking on response oneshot channels.
- Inference is **streaming** — `infer_stream` returns a `tokio::sync::mpsc::UnboundedReceiver<StreamEvent>` that the UI consumes in an async task.

### Engine Lifecycle

1. **Load**: `Engine::load(path, config)` initializes llama backend, loads the GGUF model, creates context and sampler.
2. **Inference**: `infer_stream(prompt, callback)` tokenizes, decodes the prompt, then loops sampling tokens up to `max_new = min(256, n_ctx - n_tokens)`. Each token is detokenized and sent via callback. `Engine::reset()` clears the KV cache by freeing/recreating the context.
3. **Unload**: Dropping the engine frees all native resources (sampler, context, model).

### Thread Safety Pattern

- `spawn_inference_thread()` returns `Arc<Mutex<Option<std::sync::mpsc::Sender<Command>>>>` for thread-safe command sending.
- The inference thread uses `try_recv()` in a busy loop with 100µs sleep (not blocking on recv).
- **CRITICAL**: The Arc handle MUST be stored in a Dioxus signal (`use_signal_sync`) to prevent it from being dropped on re-render. If the last Arc is dropped, the MPSC sender is dropped, the channel disconnects, the inference thread exits, and the engine is destroyed.

### Dioxus UI

- The main UI is in `src/app.rs` (`App` component).
- `use_signal_sync` for all state (theme, messages, ids, loading).
- Inference thread handle stored in `use_signal_sync` to survive re-renders.
- `use_future` for model loading on startup.
- Thinking tokens handled via `in_thinking` boolean flag in inference task.
- Slash commands: `/light`, `/dark`, `/theme` toggle theme state.

### Tailwind CSS

- `tailwind.css` uses `@import "tailwindcss"` with `@source` directives.
- Processed by `dx` CLI during build.
- Dioxus `document::Stylesheet { href: TAILWIND }` loads the compiled CSS.

---

## Known Issues

### Autofocus on Desktop
- `autofocus: true` HTML attribute and `onmounted` + `spawn(async move { sleep().await; event.set_focus(true) })` BOTH fail to focus the input field on Dioxus desktop (WebView renderer).
- Root cause unclear — may be a Dioxus WebView timing issue or missing `autofocus` support in the evaluator.
- Workaround: None yet. User manually clicks the input field.

### Scroll on New Message
- Auto-scroll works via `flex-col-reverse` CSS trick + `onmounted` anchor div at the bottom of the message list.
- `spawn` + 150ms delay required before `event.scroll_to()` call.

### Tailwind CSS Processing
- Tailwind classes are detected at build time via `@source` directives scanning `src/`.
- Must use `class:` attributes (not `style:`) for Tailwind classes to be included.

---

## Configuration

The `Config` struct in `src/llama/mod.rs` exposes:

- `n_ctx`: context length (tokens)
- `n_gpu_layers`: how many layers to offload to GPU (99 = all, if available)
- `n_threads`: CPU threads for inference
- `n_batch`: batch size for prompt processing
- `use_mmap`: memory-map the model file
- `temperature`, `top_p`, `top_k`: sampling parameters

Default values are defined in `Config::default()`.

---

## Troubleshooting

| Symptom | Likely Cause | Fix |
|---------|--------------|-----|
| `bindgen` error | Missing headers (set `LLAMA_HOME`) | Ensure `LLAMA_HOME` points to directory containing `include/llama.h` and `ggml/include/` |
| crash on inference | Header/lib version mismatch | Rebuild PrismML/llama.cpp and copy both headers and libs from that build |
| `libllama.so.0: cannot open shared object file` | `LD_LIBRARY_PATH` not set | `export LD_LIBRARY_PATH=$PWD/lib` |
| Model not found | Wrong path in `app.rs` or selected file doesn't exist | Edit `model_path` in `app.rs` or use file picker |
| "No model loaded" after re-render | Handle Arc dropped on re-render | Store handle in `use_signal_sync` (not a local variable) |
| `set`/`with_mut` borrow errors on Signal | Dioxus 0.7 main branch requires `mut` for these methods | Use `let mut signal = signal;` to shadow with mutable binding |

---

## Next Steps: Web (WASM) Build

### Architecture for Web
- Replace llama.cpp FFI with a Web Worker that loads the GGUF model via llama.cpp compiled to WASM.
- `spawn_inference_thread()` needs a platform-specific implementation:
  - **Desktop (current)**: Uses `std::thread` + llama.cpp FFI via `llama/`
  - **Web (current stub)**: Uses placeholder that echoes prompt back; needs Web Worker IPC
- Platform dispatch via `main.rs`: `#[path = "llama/mod.rs"]` for desktop, separate `web_app` module for web.
- `build.rs` skips bindgen/linking for wasm target.

### Current State
- **Desktop**: Fully working - llama.cpp native, Tailwind CSS, autoscroll, theme toggle
- **Web**: Compiles to `wasm32-unknown-unknown`, renders placeholder page
- **Next**: Create a Web Worker that loads `llama.wasm` and communicates via `postMessage`. The Rust web backend (`llama_web/`) would use `wasm-bindgen` to spawn the worker and bridge events.

### Key Changes for WASM (remaining)
1. Compile llama.cpp to WASM with Emscripten (produces `llama.wasm` and worker JS glue).
2. Create a Web Worker JS (`public/worker.js`) that imports the Emscripten module and handles commands.
3. Implement `spawn_inference_thread()` equivalent using Web Worker API via `wasm-bindgen`.
4. Bridge `StreamEvent` between worker `postMessage` and Rust `mpsc` channels.
5. Add the same `app.rs` UI on web (reuse or platform-gate llama imports).

### Implementation Plan
1. Compile PrismML/llama.cpp to WASM with Emscripten (`emcmake cmake .. -DLLAMA_CUDA=off`)
2. Create `public/worker.js` that loads the wasm and processes inference commands
3. Replace `src/llama_web/` stub with real worker IPC
4. Wire up Dioxus web app to use the web backend
5. Test with `dx serve --platform web`

---

## Code Patterns for Agents

- **Desktop llama**: Modify `src/llama/mod.rs`. Keep FFI `unsafe` confined to `Engine`.
- **Web llama**: Modify `src/llama_web/mod.rs`. Uses `wasm-bindgen` to talk to Web Worker.
- **Extend Config** if you need more llama.cpp parameters.
- **Async/Streaming**: Use `tokio::sync::mpsc::UnboundedChannel` for token streaming between threads.
- **State management**: Use `use_signal_sync` for cross-render persistence, `use_signal` for local state.
- **Error handling**: Use `anyhow::Result` for convenience.