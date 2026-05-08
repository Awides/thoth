
# Mercury Interface Language

## 100% Pure Message-based UI.


### Thinking:

I want to discuss a concept, and produce a summary that my coding agents can follow. The basic idea is that all UI is pure messaging. There is no UI outside of messages. There is no concept of View in the application. Applications are always "actor model" and receive and send messages only. *Request Messages:* to capture input from another actor, which may be human or LLM agent, or a program, we send messages that are requests for typed, tagged data. These messages can then be rendered as UI elements within the rendered messages. It might be any type of data that can be captured with standard UI elements, or more complex elements where interactions require client-side code or even LLM assistance, use validation and debouncing, etc, but the result is a _reply message_ containing that data and whatever tags were sent with the request. Interesting to note: reply thread semantics allow for message ordering without a general purpose event sourcing architecture, but we still want that anyway, to allow for less-explicitly structured interaction. Messages are powerful. They can be fullscreen splash pages. They can do permissioned effects on the receiving end, like play a sound or launch a fullscreen video, for example. They have a syntax which starts with Markdown, enhanced with a Rhai DSL that provides a model of the system: agents, peers, devices, roles, etc. (We may use special sigils to denote these things.) We populate template syntax in the context of the domain specified. For example, we may populate some values pre-send, and others post-send. Trigger effects on this machine before send, and effects on the other machine after send (governed by the permission system). This type of message-based UI allows us (humans and LLM agents) to operate the UI 'by hand', facilitating Wizard of Oz and lazy, fallback-driven, story-based development. The corpus of stories collected, which may start with solitaire, or a small team, expanding to a larger user base as the domain is mapped out and automated more and more, forms _test cases_. When code is added to an application, we test against the stories that we have. When a story breaks, we surface that as a choice: which is correct, the new story or the old one. (Maybe the old story path is deprecated.)


You're describing a **pure message-driven UI architecture**, where the *only* interface is a stream of structured messages—no traditional views, components, or DOM. This aligns with **agentic AI systems**, **actor models**, and **conversational UIs**, but pushes further by eliminating any notion of a persistent view layer.  Below is a concise, structured summary your coding agents can follow.

---

## 🧩 Core Concept: UI as Pure Messaging

All user interaction occurs through **messages**.  There is no "UI" outside of message send/receive cycles. The system behaves as a network of **actors** (humans, LLMs, services) exchanging typed, semantic messages. 

- **No Views**: No templates, no components, no DOM. Only message rendering. 
- **No Stateful UI**: State lives in agents, not in UI layers. 
- **UI = Message Interpretation**: Each message defines how it should be rendered and what input it expects.



---

### 1. 📨 Message Types

| Type | Purpose | Example |
|------|--------|--------|
| **Request Message** | Asks for data from another actor | "Enter customer ID" → rendered as input field |
| **Reply Message** | Contains requested data + tags | `{ value: "CUST-123", tag: "customer-id" }` |
| **Event Message** | Notifies of a state change | "Order confirmed" |
| **Effect Message** | Triggers side effects (sound, video, etc.) | `{{ play(sound="alert") }}` |
| **System Message** | Controls flow, permissions, routing | `{{ grant(role="admin") }}` |

---

### 2. 🧱 Message Syntax & Structure

Messages are **Markdown-enhanced** with embedded **Rhai DSL** for logic and system modeling. 

```markdown
# Welcome, {{ user.name }}

Please confirm the transaction:

- **Amount**: {{ transaction.amount }}
- **Recipient**: {{ transaction.recipient }}

{{ request("confirm", type="boolean", tag="tx-approval") }}

{{ effect(post_send="notify_slack", permission="send:slack") }}
```

- **Pre-send interpolation**: `{{ user.name }}` → resolved before sending.
- **Post-send interpolation**: `{{ transaction.amount }}` → resolved on recipient side if permitted.
- **Rhai DSL blocks**: `{{ ... }}` for logic, effects, requests.

---

### 3. 🔄 Interaction Flow

1. **Actor A** sends a *Request Message*.
2. **Actor B** renders it (e.g., as form, prompt, or fullscreen overlay).
3. **Actor B** provides input → generates a *Reply Message*.
4. **Actor A** receives reply, processes, sends next message. 

> This enables **Wizard of Oz prototyping**: humans simulate agents by hand, replying to messages.

---

### 4. 🔐 Security & Permissions

Messages may trigger effects, but only if permitted.

- **Permission model**: Based on roles, devices, peers.
- **Sigils for system entities**:
  - `@user`, `@agent`, `@device:mobile`, `@role:admin`
- **Declarative permissions**:
  ```rhai
  {{ effect("play_sound", permission="device:speaker") }}
  ```

Validation, debouncing, and type checking are enforced on reply messages.

---

### 5. 🧪 Testing & Evolution via Stories

- **Stories = Message Logs**: Sequences of real or simulated interactions.
- **Test cases**: Replayed against new code.
- **Conflict resolution**: When a story breaks, choose:
  - Keep old behavior (deprecate new code), or
  - Accept new behavior (deprecate old story).

> This creates a **living test suite** based on actual usage patterns.

---

### 6. 🚀 Advantages

- **Uniform interface** across humans, LLMs, and services. 
- **Easily mockable**: Any agent can be replaced by a human.
- **Portable**: Messages can be rendered on any device. 
- **Versionable & auditable**: Full message history = event log.
- **Lazy automation**: Start with manual stories, automate incrementally.

---

### 7. ⚠️ Challenges

- **No visual layout control**: Layout is message-defined, not CSS-driven.
- **Client complexity**: Rendering logic must interpret message semantics. 
- **Validation overhead**: All input must be validated on receipt.
- **Performance**: High message volume may require streaming.

---

## ✅ Summary for Coding Agents

> **UI is a message stream.  Every interaction is a request-reply cycle. Render messages, not views. Use Rhai for logic, Markdown for display, and tags for routing. Trust no input—validate all replies. Test against real stories. Evolve by conflict resolution.** 

This model supports **agentic workflows**, **LLM collaboration**, and **progressive automation**—ideal for AI-native applications.


### Another take from the same prompt, different model:

This is a fascinating architectural concept the user wants me to help them understand and summarize. Let me break down what they're describing:

1. **Core Principle**: All UI is pure messaging - no separate View layer
2. **Actor Model**: Applications communicate through messages (request/reply)
3. **Request Messages**: Messages that request typed, tagged data from another actor (human, LLM, or program)
4. **UI Rendering**: Request messages can be rendered as UI elements within the message itself
5. **Reply Messages**: Contain the captured data with associated tags
6. **Threading**: Message reply semantics provide ordering without explicit event sourcing (though they want it anyway)
7. **Message Capabilities**: Can be fullscreen pages, trigger effects, have permissions
8. **Syntax**: Markdown enhanced with Rhai DSL for domain modeling
9. **Template Population**: Some values pre-send, some post-send; effects on sending/receiving machines
10. **Wizard of Oz Development**: Humans can operate UI by hand to drive development
11. **Story-Based Testing**: Accumulated interaction patterns become test cases

This is a really innovative approach to application architecture. It's essentially:
- Message-oriented UI (not component-based)
- Conversational/narrative interface
- Test cases emerge from usage patterns
- Hybrid human-AI development workflow
- Declarative, effect-driven architecture

Let me structure this clearly for their coding agents.# Message-Oriented Actor Architecture: UI as Pure Messaging

This is a sophisticated and elegant architectural pattern. Let me synthesize the core concept and structure it for agent implementation:

## The Core Principle

**All user interaction is mediated through structured messages in an actor model—there is no separate UI layer.** The application is a system of autonomous agents that communicate exclusively through request/reply messaging. UI emerges from the message protocol itself, not from a distinct rendering system.

---

## Message Types and Flow

| Message Type | Direction | Purpose | Payload | Response |
|---|---|---|---|---|
| **Request Message** | Sender → Receiver | Ask another actor for typed, tagged data | Schema + validation + rendering hints | Reply Message with data |
| **Reply Message** | Receiver → Sender | Deliver captured/processed data | Data + original tags + metadata | None (terminates exchange) |
| **Effect Message** | Any | Trigger side effects (sound, video, fullscreen modal) | Permission scope + effect descriptor | Implicit ACK |

---

## Request Message Semantics

A request message is **fundamentally a question with metadata**. When one actor needs input from another (human, LLM, or program), it sends a request containing:

- **Type specification**: What data structure is expected (string, number, object, custom)
- **Tags**: Labels or identifiers to correlate request → reply
- **Rendering hints**: Directives on how to present the input UI (text field, dropdown, slider, camera input, etc.)
- **Validation rules**: Optional schema or constraints applied client-side or server-side
- **Permissions**: What effects the response may trigger on the receiving end

The receiver renders this message as interactive UI, captures input (with debouncing, validation, LLM assistance if needed), and replies with a **reply message** containing the data and echoing the original tags for ordering and correlation.

---

## Message Syntax: Markdown + Rhai DSL

Messages are written in **Markdown with embedded Rhai scripting**. This hybrid syntax serves two purposes:

### Markdown Layer
Provides narrative, layout, and human-readable structure. The message *itself* is the UI—it can be a paragraph, a form description, a fullscreen splash, or a complex modal.

### Rhai DSL Layer
Provides **domain-aware templating and behavior**. Rhai expressions reference the system model:

- **Agents**: `@agent_name`, `@self`, `@peer`
- **Devices**: `@device.location`, `@device.capability`
- **Roles**: `@role.permissions`, `@role.active_tasks`
- **Peers**: Network relationships, presence
- **Environment state**: Application context, user context

Example syntax (pseudo-code):
```
# Request data input

Please confirm your name:

{{ request(type: "string", tag: "user_name", validation: /^[a-zA-Z\s]+$/) }}

Send this message to @supervisor with role check: @self.has_role("operator")
If approved, play_sound("success.mp3") on this device.
```

---

## Pre-Send vs. Post-Send Population

Messages can reference values at different lifecycle stages:

- **Pre-send (template time)**: Populate from local state before transmission. Example: `Current user: {{ @self.user_id }}`
- **Post-send (receiver time)**: Populate based on receiver's context after delivery. Example: `Your available devices: {{ @receiver.devices.available() }}`

This split allows **context-aware but decoupled** messaging.

---

## Effects and Permissions

Messages can declare **side effects** that execute in specific scopes:

- **Pre-send effects**: Trigger on the sender's machine before message transmission (e.g., record video locally)
- **Post-send effects**: Trigger on the receiver's machine after receipt (e.g., play alert, show fullscreen modal, vibrate)

All effects are **governed by a permission system**. The receiver's role and relationship to the sender determine what effects are authorized.

---

## Message Ordering via Reply Threading

Messages form **reply chains** without requiring a full event sourcing system (though you can add one for audit trails). Each reply references its request via thread ID + tags, providing **natural causal ordering** in concurrent environments.

This allows the system to:
- Process replies out-of-order yet reconstruct causality
- Avoid complex distributed consensus
- Map interaction patterns explicitly (who asked whom, in what sequence)

---

## Wizard of Oz Development: Manual Message Choreography

During early development, **humans literally "operate" the UI by hand**—they send and reply to messages manually, as if they were agents themselves. This allows:

- **Rapid prototyping** without implementing agent logic
- **Exploring interaction patterns** before automating them
- **Discovering edge cases** through naturalistic conversation
- **Iterating design** in real-time without code changes

The human acts as a "fallback agent," filling in any role (validation, approval, decision-making) until that role is automated.

---

## Story-Based Testing: Interaction Patterns as Test Cases

As humans and agents interact via messages, **interaction sequences become recorded stories**:

- **Solitaire phase**: Single user, simple workflows
- **Team phase**: Multiple agents, peer interactions, approvals
- **Scaling phase**: Larger user base, discovered patterns formalized

Each story is a **complete trace**:
```
1. Agent A sends request (type, tags, validation)
2. Agent B replies with data
3. Downstream effects execute
4. System reaches new state
```

### Story as Test Case

When new code is added, **stories are replayed** against it. If a story fails:

- **Regression**: New code broke old behavior → revert or fix
- **Intentional change**: Old story is now incorrect → decide:
  - Mark old story as deprecated
  - Author new story reflecting new behavior
  - Track deprecation path

This creates a **living test suite** that emerges organically from usage, not written in advance. Tests are **human-readable narratives**, not assertion syntax.

---

## Benefits of This Architecture

| Aspect | Advantage |
|---|---|
| **No View Layer** | UI logic collapses into message serialization; no separation of concerns tax |
| **Actor Model** | Natural concurrency, resilience, distribution; agents are unit-testable in isolation |
| **Message Threading** | Causality without explicit event sourcing; lazy consistency acceptable |
| **Hybrid Development** | Humans and LLMs collaborate on the same protocol; no "AI mode" vs. "user mode" |
| **Test Emergence** | Tests are interaction transcripts, not brittle assertion code; they capture domain intent |
| **Declarative Effects** | Side effects are explicit and permissioned; auditable and deterministic |
| **Markdown + DSL** | Domain-aware templating; agents can read and generate their own messages |
| **Evolutionary Design** | Start with stories, add automation incrementally; patterns guide architecture |

---

## Guidance for Coding Agents

When implementing or modifying behavior:

1. **Think in messages, not functions.** Every capability is a request/reply exchange.
2. **Write stories first.** Sketch the interaction as a message dialogue before implementing.
3. **Respect reply threading.** Never break the causal chain; preserve tag correlation.
4. **Declare effects explicitly.** If behavior should trigger on receipt, mark it as post-send effect with permission scope.
5. **Use templates carefully.** Pre-send values are for sender context; post-send values are for receiver context. Don't mix.
6. **Replay stories constantly.** If you change message structure or agent behavior, re-run the story suite and decide: old behavior or new?
7. **Defer automation.** If unsure how an agent should respond, make it a fallback: let a human handle it via message until the pattern is clear.

---


