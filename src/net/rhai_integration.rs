//! Rhai scripting integration
//! 
//! Handles:
//! - Loading and executing Rhai scripts
//! - Registering message handlers
//! - REPL in messaging
//! - Prebaked and user-defined scripts

use anyhow::{Result, Context};
use rhai::{Engine, Dynamic, EvalAltResult};
use std::sync::Arc;
use tracing::{info, warn};

/// Rhai scripting engine wrapper
pub struct RhaiEngine {
    engine: Engine,
    scripts: Vec<String>,
}

impl RhaiEngine {
    pub fn new() -> Self {
        let mut engine = Engine::new();

        // Register custom functions for message handling
        engine.register_fn("send_message", |content: &str, target: &str| {
            // TODO: Wire up to actual message sending
            info!("Sending message to {}: {}", target, content);
            Ok::<_, Box<EvalAltResult>>(())
        });

        engine.register_fn("register_handler", |trigger: &str, script: &str| {
            // TODO: Register handler for message type
            info!("Registered handler for trigger: {}", trigger);
            Ok::<_, Box<EvalAltResult>>(())
        });

        Self {
            engine,
            scripts: Vec::new(),
        }
    }

    /// Load a script from string
    pub fn load_script(&mut self, name: String, script: String) -> Result<()> {
        info!("Loading script: {}", name);
        
        // Compile and store the script
        let _compiled = self.engine.compile(&script)?;
        self.scripts.push(name);
        
        Ok(())
    }

    /// Execute a script
    pub fn execute(&self, script_name: &str) -> Result<Dynamic> {
        info!("Executing script: {}", script_name);
        
        // TODO: Find and execute script by name
        Ok(Dynamic::from(()))
    }

    /// Evaluate expression (REPL style)
    pub fn eval(&self, expression: &str) -> Result<Dynamic> {
        let result = self.engine.eval_expression::<Dynamic>(expression)?;
        Ok(result)
    }

    /// Register a message handler (for reply prompts)
    pub fn register_message_handler(
        &mut self,
        trigger_type: String,
        trigger_pattern: String,
        handler_script: String,
    ) -> Result<()> {
        info!(
            "Registering message handler: {} -> {}",
            trigger_type, trigger_pattern
        );

        // TODO: Store handler registration
        // When a message matching trigger arrives, execute handler_script

        Ok(())
    }
}

/// Prebaked scripts for common operations
const PREBAKED_SCRIPTS: &[(&str, &str)] = &[
    ("echo", r#"fn echo_handler(msg) { send_message(msg, "sender") }"#),
    ("log", r#"fn log_handler(msg) { println!("Message: {}", msg) }"#),
];

/// Load prebaked scripts into engine
pub fn load_prebaked_scripts(engine: &mut RhaiEngine) -> Result<()> {
    for (name, script) in PREBAKED_SCRIPTS {
        engine.load_script(name.to_string(), script.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rhai_eval() {
        let engine = RhaiEngine::new();
        let result = engine.eval("2 + 2").unwrap();
        assert_eq!(result.as_int().unwrap(), 4);
    }

    #[test]
    fn test_load_script() {
        let mut engine = RhaiEngine::new();
        engine.load_script("test".to_string(), "let x = 42".to_string()).unwrap();
        assert_eq!(engine.scripts.len(), 1);
    }
}
