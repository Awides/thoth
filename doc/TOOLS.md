# Thoth Tool System Architecture

## Overview

Thoth's tool system is designed in two distinct layers:

1. **System Layer** — protected, privileged operations (file system outside sandbox, process management, network I/O, memory/thread management). LLM tools **MUST NOT** directly invoke this layer.
2. **Runtime Layer** — isolated domains containing tools that can be safely extended with Rhai scripts at runtime.

The LLM (Bonsai) interacts only with the Runtime Layer. All system-level operations are mediated through well-defined interfaces.

---

## System Layer (Protected)

The system layer has direct access to OS resources. It is **not exposed** to the LLM or Rhai scripts without explicit permission granting.

### Components

| Component | Description | Access |
|-----------|-------------|--------|
| **Engine** | llama.cpp context, model loading/unloading, inference control | Internal only |
| **FS Manager** | File read/write outside sandboxed paths | Restricted |
| **Process Manager** | Spawn/kill processes, thread pools | Internal only |
| **Memory Manager** | KV cache control, context window management | Internal only |
| **Scheduler** | Token scheduling, batch management | Internal only |

### Protected Operations

- Loading/unloading GGUF models (via Engine API)
- KV cache reset/eviction
- Thread affinity management
- Direct file access outside `$APP_DIR/models/`
- Network I/O (future: downloads via hf-hub)
- Dynamic library loading

**Principle:** If an operation could compromise the process, corrupt model state, or escape the sandbox, it belongs in the System Layer and is never directly callable from tools.

---

## Runtime Layer (Isolated)

The runtime layer is organized into **Domains**, each containing **Shells** and **Agents**.

### Domains

A domain groups related functionality with a security boundary.

```
Runtime
├── ui          # UI state, rendering, user interaction
├── models      # Model listing, loading, switching
├── memory      # Conversation history, context management
├── scripts     # Rhai scripting engine, user-defined extensions
└── system      # Non-protected system queries (version, uptime)
```

### Shells

A shell is an **isolated execution context** within a domain. Shells prevent cross-domain interference and limit blast radius of failures.

```
ui.shell.1  ──►  owns: messages signal, loading state
models.shell.1  ──►  owns: engine_tx channel
scripts.shell.1  ──►  owns: Rhai engine instance
```

Each shell has:
- **Owned state** — signals, channels, memory exclusive to it
- **Allowed tools** — subset of domain tools it may invoke
- **Quota** — token budget, execution time limits, call frequency

### Agents

An agent is an **LLM instance** bound to a shell. The agent can call tools from its shell's allowed set.

```
user-session-agent  ──►  ui.shell.1  ──►  allowed: [ui.add_message, models.list, memory.query]
```

Agents are lightweight — multiple agents can coexist, each with different permissions and context windows.

---

## Tool Definition

### Tool Schema

```rust
struct Tool {
    name: String,                    // e.g. "list_models"
    description: String,             // Human-readable description for LLM
    domain: Domain,                  // Which domain this tool belongs to
    parameters: ParameterSchema,     // JSON Schema for arguments
    handler: ToolHandler,            // Async function to execute
    permitted: PermissionScope,      // Required permissions
}

struct ParameterSchema {
    type: "object",
    properties: HashMap<String, Property>,
    required: Vec<String>,
}
```

### Built-in Tools (Runtime Layer)

#### Domain: `models`

| Tool | Description | Parameters |
|------|-------------|------------|
| `list_models` | List available GGUF models | none |
| `get_model_info` | Get metadata for a model | `{ path: string }` |
| `load_model` | Load a model into the engine | `{ path: string }` |
| `unload_model` | Unload current model | none |

#### Domain: `memory`

| Tool | Description | Parameters |
|------|-------------|------------|
| `get_history` | Get conversation history | `{ limit: number }` |
| `clear_history` | Clear conversation history | none |
| `search_history` | Full-text search history | `{ query: string }` |

#### Domain: `ui`

| Tool | Description | Parameters |
|------|-------------|------------|
| `add_message` | Append a message to the chat | `{ role, content, thinking }` |
| `set_status` | Update UI status indicator | `{ status: string }` |
| `show_notification` | Display a notification | `{ message: string }` |

#### Domain: `system`

| Tool | Description | Parameters |
|------|-------------|------------|
| `ping` | Health check | none |
| `get_capabilities` | List available tools | none |
| `get_stats` | Runtime statistics | none |

---

## Rhai Extension System

### Overview

The `scripts` domain hosts a Rhai engine that users can program against. Scripts can:

- Define new tools (registered at runtime)
- Composing existing tools into higher-level actions
- Implement event-driven workflows
- Create custom prompts or context injection

### Script Location

```
thoth/
├── scripts/           # User Rhai scripts
│   ├── tools/         # Tool definitions (*.rhai)
│   ├── workflows/     # Multi-step automations
│   └── prompts/       # Custom prompt templates
```

### Defining a Tool in Rhai

```rhai
// scripts/tools/my_tools.rhai

// Tool definition with parameters
fn list_directory(path: string) -> string {
    // Use the fs API exposed to scripts domain
    let entries = fs::list_dir(path)?;
    if entries.len() == 0 {
        return "Empty directory";
    }
    let mut result = "";
    for entry in entries {
        result += entry + "\n";
    }
    result
}

// Composing existing tools
fn load_and_list(path: string) -> string {
    // Call built-in tools via tool:: namespace
    let model_info = tool::get_model_info(path)?;
    let list = tool::list_models()?;
    `Model: ${model_info.name}\n\nAvailable: ${list}`
}

// Register tool with the runtime
register_tool("list_directory", list_directory);
```

### Rhai API Surface

Scripts are given a limited API:

| Module | Functions | Notes |
|--------|-----------|-------|
| `tool::` | `call(name, args)`, `list()` | Invoke other tools |
| `fs::` | `list_dir`, `read_text`, `exists` | Sandbox-limited file access |
| `log::` | `info`, `warn`, `error` | Logging to console |
| `ctx::` | `get_history`, `set_var`, `get_var` | Conversation context |

**NOT available** in Rhai scripts:
- Direct engine manipulation
- Thread/process spawning
- Network I/O
- File write (read-only sandbox)

### Tool Registration Flow

```
1. User drops .rhai file into scripts/tools/
2. Thoth detects new file (watcher or restart)
3. Rhai engine loads and evaluates script
4. Script calls register_tool(name, function)
5. Tool appears in list_models output with description
6. LLM can now call the tool
```

---

## Permission Model

### Permission Scopes

| Scope | Description |
|-------|-------------|
| `none` | No permissions required |
| `read` | Read-only access |
| `write` | Read and write |
| `admin` | Full access (system layer only) |

### Tool Permissions

Built-in tools declare required permissions:

```rust
list_models    -> none
load_model     -> write (modifies engine state)
clear_history  -> write (modifies memory)
```

### Shell Constraints

Each shell has a maximum permission scope. The user-session-agent might have `read` scope, meaning it can call `list_models` but not `load_model`.

---

## LLM Interaction Protocol

### Tool Calling Flow

```
1. User sends message
2. LLM generates response
   - If no tool call: return text
   - If tool call(s): set finish_reason = "tool_calls"
3. For each tool call:
   a. Validate tool exists and shell has permission
   b. Execute tool handler (may cross domain)
   c. Collect result
4. Append tool results as system messages
5. LLM generates final response
```

### Message Format (OpenAI-compatible)

```json
{
  "messages": [
    {"role": "user", "content": "List the models"},
    {"role": "assistant", "content": null, "tool_calls": [
      {"id": "call_abc", "type": "function",
       "function": {"name": "list_models", "arguments": "{}"}}
    ]},
    {"role": "tool", "tool_call_id": "call_abc",
     "content": "models/Bonsai-1.7B-Q1_0.gguf\nmodels/Ternary-Bonsai-8B-Q2_0.gguf"}
  ]
}
```

---

## System vs Runtime Boundaries

```
┌─────────────────────────────────────────────────────────────┐
│                      System Layer                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌─────────────┐ │
│  │ Engine   │  │ FS Mgr   │  │ Process  │  │ Scheduler   │ │
│  │ (llama)  │  │ (strict) │  │ (threads)│  │ (tokens)    │ │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └──────┬──────┘ │
│       │             │             │               │         │
├───────┴─────────────┴─────────────┴───────────────┴─────────┤
│                    Runtime Layer                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │                    Domains                            │  │
│  │  ┌─────────┐  ┌─────────┐  ┌─────────┐  ┌─────────┐  │  │
│  │  │  ui     │  │ models  │  │ memory  │  │ scripts │  │  │
│  │  └────┬────┘  └────┬────┘  └────┬────┘  └────┬────┘  │  │
│  │       │            │            │            │        │  │
│  │  ┌────┴────────────┴────────────┴────────────┴────┐  │  │
│  │  │                  Tools                          │  │  │
│  │  │  list_models, load_model, add_message, ...      │  │  │
│  │  └─────────────────────────────────────────────────┘  │  │
│  └──────────────────────────────────────────────────────┘  │
│                     Agents (LLM sessions)                    │
└─────────────────────────────────────────────────────────────┘
```

**Rule:** Tools in the Runtime Layer call into the System Layer only through narrow, synchronous interfaces. Never expose raw pointers, handles, or internal state.

---

## Next Steps

- [ ] Implement `Tool` struct and `ToolRegistry`
- [ ] Add built-in tools: list_models, get_model_info, load_model, unload_model
- [ ] Create domain isolation with shell per domain
- [ ] Add Rhai engine integration with sandboxed API surface
- [ ] Implement tool call parsing from model output
- [ ] Add tool result injection back to LLM
- [ ] File watcher for scripts/tools/*.rhai
- [ ] Permission system for shell scoping

---

## Reference

- [OpenAI Function Calling](https://platform.openai.com/docs/guides/function-calling)
- [Rhai Scripting Engine](https://rhai.rs/)
- [Bonsai Tool Calling](https://github.com/kyr0/bonsai-garden-1bit-turboquant-mlx-server)