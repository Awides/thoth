**`hermes-rs` can be used as a library** to build our agent harness in Rust.  It exposes modular components like `HermesAgent`, `AgentConfig`, `ToolRegistry`, and `OpenAIClient`, making it ideal for integration into a **Dioxus app**. 

We can embed it as a dependency and:
- Disable filesystem tools or replace them with our system-specific ones.
- Use agent state (built from the log) for configuration.
- Leverage its **structured logging**, **error recovery**, and **streaming parser**. 
- Adopt its **ReAct orchestration loop**: *Think → Plan → Execute → Observe → Respond*. 

Use it with our local Bitnet models, to create local agents which outsource privacy-guarded, pure coding tasks to stronger models on remote providers (or the users own llama server or whatever).

For our use case, define a custom `ToolRegistry` that only exposes safe, application-layer actions, and run the agent within a controlled context.

https://github.com/eikarna/hermes-rs


