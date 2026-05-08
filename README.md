# Thoth

**A message-native application framework where the message list *is* the UI.**

Thoth is a Dioxus-based runtime that treats all user interfaces as structured message streams. Requests, replies, and UI elements are first-class messages, enabling symmetric communication between humans, LLM agents, and Rhai scripts. This design makes the message log the authoritative source of truth for both state and presentation.

## Core Invariants

- **All UI is dialog** — Every interface is a request for typed, tagged data. There are no standalone widgets; everything lives in the message list.
- **All state is event-sourced** — The message log persists; UI can be recreated at any time by replaying messages.
- **All actors speak the same language** — Humans, LLMs, and Rhai scripts send and receive the same message schema, enabling composition and substitution.
- **UI is ephemeral** — The message log outlives any particular UI session; the client can close and reopen without losing context.

## Quick Start

### Prerequisites

- Rust + `cargo`
- Dioxus CLI (`cargo install dioxus-cli`)
- llama.cpp headers and libraries (see [BUILD.md](doc/BUILD.md))

### Build

```bash
# Desktop
dx build --release

# Android (ARM64)
dx build --release --target aarch64-linux-android

# Web (WASM)
dx build --web --release
```

See [doc/BUILD.md](doc/BUILD.md) for detailed platform setup and [doc/RUNNING.md](doc/RUNNING.md) for test instructions.

## Architecture

### Message Schema

Every message is a typed request or reply with tags:

```rust
pub struct Message {
    /// Kind of data requested or provided (text, number, select, etc.)
    pub input_type: InputType,
    /// Target: user, bot name, or script ID
    pub target: String,
    /// Optional tags for routing/filtering (e.g., #greeting)
    pub tags: Vec<Tag>,
    /// The content (prompt, reply, or input value)
    pub content: String,
    /// Sender identifier
    pub sender: String,
}
```

See `src/net/message.rs` for full definition and `InputType` variants.

### Rendering Pipeline

1. **Message creation** — Any actor sends a request message into the stream (`insert MessageKind::Request`).
2. **Delivery** — Messages are routed to the intended recipient(s) via Nostr relays or MLS groups.
3. **Rendering** — The frontend client reads the message log and renders each `Request` as an interactive element; `Reply` messages are displayed inline.
4. **Interaction** — User input produces a reply message with the filled value, which is then routed back to the requestor.
5. **Persistence** — All messages are stored locally (via Memvid) and optionally synchronized with remote relays.

No separate UI state; the message log is the source of truth.

### Extensibility

- **Custom input types** — Add new `InputType` variants and corresponding renderers in the frontend.
- **Filtering** — Use tags to create dynamic UI "channels" that only show relevant messages.
- **Plugins** — Rhai scripts can register handlers for specific message patterns, enabling bot-like behavior without recompiling.
- **LLM integration** — The built-in llama.cpp backend can process `Request(messages)` and emit `Reply(content)` as streamed tokens.

### Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| Desktop (Linux/Windows/macOS) | ✅ Full inference + networking |
| Android (ARM64) | ✅ Full inference + networking |
| Android (ARMv7) | ⚠️ UI only (no local inference) |
| Web (WASM) | ⚠️ Stub (remote inference only) |

## Project Layout

```
src/
├── net/                # Nostr client, MLS, runtime, Rhai
├── system/             # Dialogs, config, commands, onboarding
├── key_storage/        # Encrypted credentials
├── mem/                # Persistence layer (native/web)
├── llama/              # Unified inference (desktop + Android)
├── llama_web/          # Web inference stub (worker)
├── main.rs             # Platform dispatch
├── app.rs              # Desktop UI (should become message-renderer-only)
├── android_app.rs      # Android UI
└── web_app.rs          # Web UI
```

## Development

### Running Tests

```bash
cargo test --package thoth --lib net
cargo test --package thoth --lib mem
cargo test --package thoth --lib system
```

Integration tests require a model file; set `THOTH_TEST_MODEL` to a valid `.gguf`.

### Configuration

- Model path: `models/Bonsai-1.7B-Q1_0.gguf` (or override via `THOTH_MODEL_PATH`)
- Nostr relays: defaults in `src/net/nostr_client.rs`

## Design Principles (in depth)

See [ARCHITECTURE.md](ARCHITECTURE.md) for a deeper treatment of:
- Canonical message schema and cute syntax
- Element lifecycle and custom renderers
- Actor symmetry and message routing
- State management and replayability
- Security model (MLS encryption, capability-based APIs)

## Contributing

- Follow existing conventions: use `InputType` variants, tag messages appropriately.
- Keep UI logic out of business logic; message generation should be platform-agnostic.
- Write tests for new `InputType` renderers and serialization.
- Document message kinds in `doc/ELEMENTS.md` when adding new element types.

## License

Same as Thoth project.