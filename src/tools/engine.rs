use anyhow::{Result, Context};
use rhai::{Engine, Scope};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use chrono::{TimeZone, Local};

use super::defs::{ToolDef, ToolCall, ToolResult, ToolParam};

pub struct ToolEngineInner {
    rhai: Engine,
    tools: HashMap<String, ToolEntry>,
    tool_dir: PathBuf,
}

#[derive(Clone)]
struct ToolEntry {
    def: ToolDef,
    script: String,
}

#[derive(Clone)]
pub struct ToolEngine {
    inner: Arc<std::sync::Mutex<ToolEngineInner>>,
    tool_dir: PathBuf,
    defs_cache: Vec<ToolDef>,
    prompt_cache: String,
}

impl ToolEngine {
    pub fn new(tool_dir: impl Into<PathBuf>) -> Self {
        let tool_dir = tool_dir.into();
        let mut inner = ToolEngineInner::new(&tool_dir);
        let defs_cache = inner.tool_defs_owned();
        let prompt_cache = inner.tools_prompt_section_inner();
        Self {
            inner: Arc::new(std::sync::Mutex::new(inner)),
            tool_dir,
            defs_cache,
            prompt_cache,
        }
    }

    pub fn load_scripts(&mut self) -> Result<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.load_scripts_inner()?;
        self.defs_cache = inner.tool_defs_owned();
        self.prompt_cache = inner.tools_prompt_section_inner();
        Ok(())
    }

    pub fn add_embedded_script(&mut self, name: &str, script: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Err(e) = inner.load_embedded_script(name, script) {
            eprintln!("WARNING: failed to load embedded tool '{}': {}", name, e);
        }
        self.defs_cache = inner.tool_defs_owned();
        self.prompt_cache = inner.tools_prompt_section_inner();
    }

    #[cfg(target_os = "android")]
    pub fn load_embedded_scripts(&mut self) {
        self.add_embedded_script("calculator", include_str!("../../assets/tools/calculator.rhai"));
        self.add_embedded_script("current_time", include_str!("../../assets/tools/current_time.rhai"));
    }

    pub fn tool_defs(&self) -> &[ToolDef] {
        &self.defs_cache
    }

    pub fn tools_prompt_section(&self) -> &str {
        &self.prompt_cache
    }

    pub fn execute(&self, call: &ToolCall) -> ToolResult {
        let mut inner = self.inner.lock().unwrap();
        inner.execute(call)
    }
}

unsafe impl Send for ToolEngine {}
unsafe impl Sync for ToolEngine {}

impl ToolEngineInner {
    fn new(tool_dir: &Path) -> Self {
        let tool_dir = tool_dir.to_path_buf();
        let mut rhai = Engine::new();
        rhai.set_allow_looping(true);
        rhai.set_fast_operators(true);

        rhai.register_fn("eval_math", |expr: &str| -> String {
            let expr = expr.replace(" ", "");
            let mut acc = 0.0_f64;
            let mut op = b'+';
            let mut buf = String::new();
            let mut first = true;

            let parse_num = |s: &str| s.parse::<f64>().ok();

            for ch in expr.chars() {
                match ch {
                    '+' | '-' | '*' | '/' => {
                        if let Some(n) = parse_num(&buf) {
                            if first { acc = n; first = false; } else {
                                match op {
                                    b'+' => acc += n,
                                    b'-' => acc -= n,
                                    b'*' => acc *= n,
                                    b'/' => { if n != 0.0 { acc /= n; } else { return "Error: division by zero".to_string(); } }
                                    _ => {}
}
                            }
                            op = ch as u8;
                            buf.clear();
                        } else {
                            return format!("Error: invalid number '{}'", buf);
                        }
                    }
                    _ => { buf.push(ch); }
                }
            }
            if let Some(n) = parse_num(&buf) {
                if first { acc = n; } else {
                    match op {
                        b'+' => acc += n,
                        b'-' => acc -= n,
                        b'*' => acc *= n,
                        b'/' => { if n != 0.0 { acc /= n; } else { return "Error: division by zero".to_string(); } }
                        _ => {}
                    }
                }
            }
            let s = format!("{}", acc);
            if s.ends_with(".0") { s[..s.len()-2].to_string() } else { s }
        });

        rhai.register_fn("system_time_str", || -> String {
            let secs = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let secs_i64 = secs as i64;
            match chrono::DateTime::from_timestamp(secs_i64, 0) {
                Some(utc) => {
                    let local: chrono::DateTime<chrono::Local> = utc.into();
                    local.format("%Y-%m-%d %H:%M:%S %Z").to_string()
                }
                None => format!("Unix timestamp: {}", secs),
            }
        });

        rhai.register_fn("system_time", || -> i64 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64
        });

        rhai.register_fn("format_timestamp", |ts: i64, fmt: &str| -> String {
            match chrono::DateTime::from_timestamp(ts, 0) {
                Some(utc) => {
                    let local: chrono::DateTime<chrono::Local> = utc.into();
                    local.format(fmt).to_string()
                }
                None => format!("Invalid timestamp: {}", ts),
            }
        });

        Self {
            rhai,
            tools: HashMap::new(),
            tool_dir,
        }
    }

    fn load_scripts_inner(&mut self) -> Result<()> {
        if !self.tool_dir.exists() {
            fs::create_dir_all(&self.tool_dir)?;
        }
        let entries = fs::read_dir(&self.tool_dir)?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map(|e| e == "rhai").unwrap_or(false) {
                if let Err(e) = self.load_script(&path) {
                    eprintln!("WARNING: failed to load tool script {}: {}", path.display(), e);
                }
            }
        }
        Ok(())
    }

    fn load_script(&mut self, path: &Path) -> Result<()> {
        let script = fs::read_to_string(path)?;
        let _ast = self.rhai.compile(&script)
            .map_err(|e| anyhow::anyhow!("compile error in {}: {}", path.display(), e))?;

        let name = path.file_stem()
            .context("no file stem")?
            .to_string_lossy()
            .into_owned();

        let tool_def = self.extract_tool_def(&name, &script)?;
        self.tools.insert(name, ToolEntry { def: tool_def, script });
        Ok(())
    }

    fn load_embedded_script(&mut self, name: &str, script: &str) -> Result<()> {
        let _ast = self.rhai.compile(script)
            .map_err(|e| anyhow::anyhow!("compile error in embedded tool '{}': {}", name, e))?;
        let tool_def = self.extract_tool_def(name, script)?;
        self.tools.insert(name.to_string(), ToolEntry { def: tool_def, script: script.to_string() });
        Ok(())
    }

    fn extract_tool_def(&self, name: &str, script: &str) -> Result<ToolDef> {
        let description = script.lines()
            .find(|l| l.trim().starts_with("// description:"))
            .map(|l| l.trim().trim_start_matches("// ").trim_start_matches("description:").trim())
            .unwrap_or(name)
            .to_string();

        let mut params = Vec::new();

        for line in script.lines() {
            let trimmed = line.trim();
            if let Some(param_str) = trimmed.strip_prefix("// param:") {
                let parts: Vec<&str> = param_str.trim().splitn(3, ' ').collect();
                if parts.len() >= 2 {
                    let param_name = parts[0].to_string();
                    let param_type = parts[1].to_string();
                    let param_desc = parts.get(2).unwrap_or(&"").to_string();
                    let required = !param_type.ends_with("?");
                    let param_type_clean = param_type.trim_end_matches('?');
                    let mut p = ToolParam {
                        name: param_name,
                        param_type: param_type_clean.to_string(),
                        description: param_desc,
                        required,
                        enum_values: Vec::new(),
                    };

                    for pl in script.lines() {
                        let tpl = pl.trim();
                        if tpl.starts_with(&format!("// enum[{}]:", p.name)) {
                            let vals_str = tpl.trim_start_matches(&format!("// enum[{}]:", p.name)).trim();
                            p.enum_values = vals_str.split(',').map(|v| v.trim().to_string()).filter(|v| !v.is_empty()).collect();
                        }
                    }

                    params.push(p);
                }
            }
        }

        Ok(ToolDef {
            name: name.to_string(),
            description,
            parameters: params,
        })
    }

    fn tool_defs_owned(&self) -> Vec<ToolDef> {
        self.tools.values().map(|e| e.def.clone()).collect()
    }

    fn tools_prompt_section_inner(&self) -> String {
        if self.tools.is_empty() {
            return String::new();
        }
        let mut s = String::from("\n\n# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided with function signatures within <tools></tools> XML tags:\n<tools>");
        for entry in self.tools.values() {
            s.push_str("\n");
            s.push_str(&entry.def.to_qwen3_json());
        }
        s.push_str("\n</tools>\n\nFor each function call, return a json object with function name and arguments within <|tool_call_begin|><|tool_call_end|> tags:\n<|tool_call_begin|>\n{\"name\": <function-name>, \"arguments\": <args-json-object>}\n<|tool_call_end|>");
        s
    }

    fn execute(&mut self, call: &ToolCall) -> ToolResult {
        let entry = match self.tools.get(&call.name) {
            Some(e) => e.clone(),
            None => return ToolResult::err(&call.name, format!("unknown tool: {}", call.name)),
        };

        let args_json = match serde_json::to_string(&call.arguments) {
            Ok(j) => j,
            Err(e) => return ToolResult::err(&call.name, format!("invalid arguments: {}", e)),
        };

        let mut scope = Scope::new();
        scope.push("args", args_json.clone());

        self.rhai.register_fn("get_arg", |args: &str, key: &str| -> String {
            match serde_json::from_str::<Value>(args) {
                Ok(v) => v.get(key)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                Err(_) => String::new(),
            }
        });

        self.rhai.register_fn("get_arg_int", |args: &str, key: &str| -> i64 {
            match serde_json::from_str::<Value>(args) {
                Ok(v) => v.get(key).and_then(|v| v.as_i64()).unwrap_or(0),
                Err(_) => 0,
            }
        });

        self.rhai.register_fn("get_arg_float", |args: &str, key: &str| -> f64 {
            match serde_json::from_str::<Value>(args) {
                Ok(v) => v.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0),
                Err(_) => 0.0,
            }
        });

        let script = entry.script;
        match self.rhai.eval_with_scope::<String>(&mut scope, &script) {
            Ok(result) => ToolResult::ok(&call.name, result),
            Err(e) => ToolResult::err(&call.name, format!("{}", e)),
        }
    }
}
