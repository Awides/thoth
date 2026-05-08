# Architecture: Message-Native UI

## The Central Idea

> The message list *is* the UI.

Instead of a conventional widget-based GUI, Thoth treats all user interactions as structured messages passed between actors. Every human query, LLM reply, and script action is a first-class message. Interface elements (inputs, displays, controls) are themselves messages that appear in the log. This makes the message log the single source of truth for both state and presentation.

## Key Invariants

1. **All UI is dialog** — No separate layout engine; all interface elements are request messages that expect a reply.
2. **All state is event-sourced** — The message log is persisted; any UI can be reconstructed by replaying messages.
3. **All actors speak the same language** — Humans, LLMs, and Rhai scripts use the same message schema symmetrically (both send and receive).
4. **UI is ephemeral** — Renderers come and go; the message log outlives them.

## Canonical Message Schema

```rust
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    /// Unique ID (SHA256 of serialized form or Nostr event ID)
    pub id: Option<String>,
    /// Timestamp when created
    pub timestamp: u64,
    /// Human-readable type of data requested/provided (text, number, select, etc.)
    pub input_type: InputType,
    /// Target recipient (user, bot name, or script ID)
    pub target: String,
    /// Optional tags for routing/filtering (e.g., "#greeting", "#todo")
    pub tags: Vec<Tag>,
    /// Content: prompt, reply, or input value
    pub content: String,
    /// Sender identifier
    pub sender: String,
    /// Optional reply-to ID (threading)
    pub reply_to: Option<String>,
}
```

All messages are serializable and signed (when sent over Nostr). The `input_type` determines how the message is rendered and parsed.

### InputType Variants (canonical UI elements)

- `text`, `textarea` — free-form text
- `number`, `range` — numbers with optional min/max
- `boolean` — yes/no toggle
- `select` — pick from options (single or multi)
- `date`, `time`, `datetime` — temporal pickers
- `file` — file upload
- `location` — geolocation
- `contact` — vCard selection
- `transform2d`, `transform3d` — vector manipulations
- `rich-text` — HTML content editor
- `json` — structured data editor

Extensible: agents can define custom types and register renderers capability.

## Rendering Pipeline

### Element Lifecycle

1. **display** — An agent emits a `Request` message with `input_type` and `content`. The renderer creates an interactive element in the message list.
2. **change** (optional) — As the user interacts, the renderer emits `Change` messages (with `debounce_ms` support) carrying partial updates.
3. **commit** — When the user finalizes (presses Enter, clicks Submit), a `Reply` message with the filled value is sent to the `target`.
4. **rejection** — If validation fails or user cancels, a `Rejected` message is emitted.

All these are ordinary messages in the log. They can be replayed; re-rendering an old `display` message rehydrates the UI exactly as it was.

### Renderer Responsibilities

- Render `Request` messages as interactive widgets appropriate to `input_type`.
- Emit structured `Change`/`Commit` messages on user actions.
- Handle sticky elements (`persist_after_send`) by keeping them visible.
- Update its own capabilities via `capability` messages (e.g., "renders type `transform2d`").

### Custom Renderers

Agents can register custom renderers for element types they define. Mercury (the privileged system agent) may provide fallback renderers (e.g., JSON editor) for unknown types. This enables extension without modifying core.

## Routing & Recipients

Messages carry:
- `target` — a string identifying the recipient (user, LLM agent, or Rhai script)
- `tags` — arbitrary strings for filtering/subscribing (e.g., `#prompts`, `#system`)

Agents subscribe to messages by:
- `target == self.id`
- Presence in `tags` (wildcard subscriptions)

Replies set `reply_to` to the original message `id`, forming threads. Nostr relays or MLS groups provide network transport.

## State Management & Replayability

Because the UI is purely a function of the message log, any client can:
- Load the complete log
- Filter to relevant subset (by `target`, `tags`, time)
- Render each message according to its `input_type`
- Replay user input values from `Reply` messages

Local storage (Memvid) holds the durable event log. On startup, the client syncs from relays (`since` timestamp) and applies new messages incrementally. ML

S groups enable decentralized state sharing with end-to-end encryption.

## Security & Sandboxing

- **Agents run in WASM** (Wasmtime) with capability-based API: they can only access storage, messaging, crypto, etc., if explicitly granted by Hades.
- **Fuel metering + epochs** — Prevent DoS by limiting CPU cycles per agent.
- **Encryption at rest** — Local storage encrypted; MLS provides E2E for group messages.
- **Supply chain** — Agents are signed; updates verified before installation.

## Actor Symmetry

> Humans, LLMs, and scripts are peers in the message stream.

- A human sends a `Request` (e.g., "What's the weather?") and receives a `Reply`.
- An LLM agent subscribes to requests targeting it and replies with `Reply` (optionally streamed tokens as multiple `Reply` messages).
- A Rhai script registers a handler for a pattern (e.g., `tag contains "log"`) and emits new messages as side effects.

No special-cased APIs: all act as senders and receivers. This enables composition: an LLM can ask a human for clarification by sending a `Request`, or a script can route messages to an LLM transparently.

## Extensibility Paths

1. **New input type** — Add variant to `InputType`, implement renderer in frontend; optionally write agent that produces it.
2. **New agent** — Write WASM module (Roc, Rhai compiled to WASM, or any language). Just subscribe to and emit messages.
3. **Custom renderer** — Register via `capability` message with renderer component (Dioxus component ID).
4. **Transport** — Add new Nostr relay or MLS group management as needed.

## Cross-Platform Consistency

- Web (WASM): uses OPFS for persistence, WebSocket + WebRTC for networking.
- Desktop: uses filesystem + QUIC or WebSocket.
- Android: uses Android APIs via JNI; model inference via native llama.cpp.

The message schema and core logic (`src/net/`) are identical across platforms; only `mem/` and `llama_*` backends differ.

## Where Things Are

- **Message schema**: `src/net/message.rs`
- **Rhai integration**: `src/net/rhai_integration.rs`
- **Runtime & dispatch**: `src/net/runtime.rs`
- **Nostr client**: `src/net/nostr_client.rs`
- **MLS groups**: `src/net/mls_group.rs`
- **Persistence**: `src/mem/`
- **LLM backends**: `src/llama/` (unified native), `src/llama_web/` (web stub)
- **UI renderers** (to become message-driven): `src/app.rs`, `src/android_app.rs`, `src/web_app.rs`
- **Plugins & config**: `src/system/`

## Next Steps

1. Refactor frontends to render *only* from the message log (remove direct `send_command` calls).
2. Introduce a `MessageLog` abstraction that owns the event stream and provides filtered views.
3. Ensure every actor (human UI, LLM, Rhai) goes through the same message insertion points.
4. Document element types and tagging conventions in `doc/ELEMENTS.md`.
5. Add end-to-end test: simulate a `Request → Reply` flow through the message log.

## References

- Original Quicksilver concept (Roc/WASM): `../thoth-roc-wasm/CONCEPT.md`, `ARCHITECTURE.md`, `ELEMENTS.md`
- Current Nostr/MLS implementation: `src/net/`
- Build instructions: `doc/BUILD.md`
- Testing: `doc/RUNNING.md`
