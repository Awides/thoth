# Thoth Network Runtime

Distributed runtime for message-native applications with Nostr, MLS encryption, and Rhai scripting.

## Architecture

```
Message List (Nostr Event Log)
│
├── Request Messages (typed, tagged data)
├── Reply Messages (threaded responses)
├── Prompt Registrations (LLM handlers)
└── Script Registrations (Rhai handlers)
```

### Key Invariants

- **All UI is dialog** - No layout/navigation concerns; everything is a message
- **All state is event-sourced** - Message log is the source of truth
- **All actors speak the same language** - Humans, LLMs, and scripts all send/receive messages
- **UI is ephemeral** - Message log persists; UI can close and reopen

## Components

### 1. Message Schema (`src/net/message.rs`)

Implements the "cute syntax" for typed, tagged messages:

```
?text #firstname @thoth remember her name
```

Where:
- `?text` - Input type (text, number, date, boolean, select, etc.)
- `#tag` - Metadata tags for routing/filtering
- `@target` - Message recipient/handler
- `remember her name` - Content/prompt

Message types:
- **Request** - Input request from human/LLM/script
- **Reply** - Response to a request (threaded)
- **System** - Notifications and system messages
- **RegisterPrompt** - Attach handler to future reply

### 2. Nostr Client (`src/net/nostr_client.rs`)

Nostr relay integration for decentralized messaging:

- **Relay connections** - Pub/sub to Nostr relays
- **Key management** - nsec stored in file or environment
- **Event publishing** - Send signed messages
- **Subscription filters** - Listen for specific messages

Default relays:
- `wss://relay.nostr.io`
- `wss://nos.lol`
- `wss://relay.damus.io`

### 3. MLS Groups (`src/net/mls_group.rs`)

Message Layer Security for end-to-end encryption:

- **Device groups** - User's own devices (auto-created)
- **User groups** - Chat groups with other users
- **Member management** - Add/remove participants
- **Encryption** - MLS-encrypted messages (stub implementation)

### 4. Rhai Integration (`src/net/rhai_integration.rs`)

Scripting engine for message handlers:

- **Prebaked scripts** - Built-in handlers (echo, log, etc.)
- **User scripts** - Load custom Rhai scripts
- **REPL in messaging** - Evaluate expressions inline
- **Handler registration** - Attach scripts to message types

Example Rhai handler:
```rhai
fn handle_reply(msg) {
    send_message($"Reply received: {msg}", "sender")
}
```

### 5. Background Runtime (`src/net/runtime.rs`)

Tokio-based background tasks:

- **Nostr listener** - Polls relays for new messages
- **Message routing** - Delivers messages to UI/handlers
- **Group management** - MLS group lifecycle
- **Survives UI close** - Runtime continues in background

## Usage

### Initialize Runtime

```rust
use thoth::net::{NetRuntime, create_runtime};
use tokio::sync::mpsc;

let (tx, mut rx) = mpsc::unbounded_channel();
let runtime = create_runtime(tx);
runtime.start().await?;
```

### Send Message

```rust
use thoth::net::{Message, InputType, Tag};

let msg = Message::new_request(
    "Hello, world!".to_string(),
    Some(InputType::Text { default: None }),
    Some("user".to_string()),
    vec![Tag { name: "greeting".to_string(), value: "true".to_string() }],
    "sender_pubkey".to_string(),
);

runtime.send_message(msg).await?;
```

### Parse Cute Syntax

```rust
let msg = Message::parse_cute_syntax(
    "?text #greeting @user hello there",
    "sender_pubkey".to_string(),
)?;
```

### Register Handler

```rust
runtime.register_handler(
    "reply".to_string(),
    "pattern".to_string(),
    "fn handler(msg) { send_message(msg, \"sender\") }".to_string(),
).await?;
```

## Message Flow

1. **User sends message** → Parsed to `Message` struct
2. **Message signed** → Nostr event created
3. **Published to relay** → Encrypted with MLS (if group)
4. **Recipients receive** → Decrypt and validate
5. **Handler registered** → Rhai function attached
6. **Reply received** → Handler called automatically
7. **UI updates** → Message list re-renders

## Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| Desktop (Linux/Windows/macOS) | ✅ Working | Full inference + networking |
| Android (ARM64) | ✅ Working | Full inference + networking |
| Android (ARMv7) | ⚠️ UI only | No local inference |
| Web (WASM) | ⚠️ Stub | Remote inference only |

## Next Steps

### Phase 1: Basic Chat (Current)
- [x] Message schema with cute syntax
- [x] Nostr client (stub)
- [x] MLS group management (stub)
- [x] Rhai integration
- [ ] Actual Nostr publishing/subscribing
- [ ] Real MLS encryption
- [ ] Message threading UI

### Phase 2: Distributed Runtime
- [ ] Prompt registration system
- [ ] Device group auto-discovery
- [ ] Cross-network chat (desktop ↔ web)
- [ ] Push notifications

### Phase 3: Advanced Features
- [ ] QUIC direct connections (bypass relay)
- [ ] WASM plugin support
- [ ] Process model (background workers)
- [ ] Notification routing

## Configuration

### Environment Variables

- `NOSTR_NSEC` - Nostr secret key (optional, auto-generated if missing)

### Files

- `nsec.key` - Nostr key storage (auto-created)
- `models/*.gguf` - LLM models (gitignored)

## Testing

```bash
cargo test --package thoth --lib net
```

Run specific tests:
```bash
cargo test --package thoth --lib net::message::tests
cargo test --package thoth --lib net::nostr_client::tests
```

## License

Same as Thoth project.
