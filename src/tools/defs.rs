use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParam {
    pub name: String,
    #[serde(rename = "type")]
    pub param_type: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub enum_values: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParam>,
}

impl ToolDef {
    pub fn to_qwen3_json(&self) -> String {
        let mut props = serde_json::Map::new();
        let mut required = Vec::new();
        for p in &self.parameters {
            let mut prop = serde_json::Map::new();
            prop.insert("type".into(), serde_json::Value::String(p.param_type.clone()));
            if !p.description.is_empty() {
                prop.insert("description".into(), serde_json::Value::String(p.description.clone()));
            }
            if !p.enum_values.is_empty() {
                let vals: Vec<serde_json::Value> = p.enum_values.iter()
                    .map(|v| serde_json::Value::String(v.clone()))
                    .collect();
                prop.insert("enum".into(), serde_json::Value::Array(vals));
            }
            props.insert(p.name.clone(), serde_json::Value::Object(prop));
            if p.required {
                required.push(serde_json::Value::String(p.name.clone()));
            }
        }
        let mut param_obj = serde_json::Map::new();
        param_obj.insert("type".into(), serde_json::Value::String("object".into()));
        param_obj.insert("properties".into(), serde_json::Value::Object(props));
        if !required.is_empty() {
            param_obj.insert("required".into(), serde_json::Value::Array(required));
        }
        let mut func = serde_json::Map::new();
        func.insert("name".into(), serde_json::Value::String(self.name.clone()));
        func.insert("description".into(), serde_json::Value::String(self.description.clone()));
        func.insert("parameters".into(), serde_json::Value::Object(param_obj));
        let mut obj = serde_json::Map::new();
        obj.insert("type".into(), serde_json::Value::String("function".into()));
        obj.insert("function".into(), serde_json::Value::Object(func));
        serde_json::Value::Object(obj).to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub name: String,
    pub content: String,
}

impl ToolResult {
    pub fn ok(name: impl Into<String>, content: impl Into<String>) -> Self {
        Self { name: name.into(), content: content.into() }
    }
    pub fn err(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self { name: name.into(), content: format!("Error: {}", msg.into()) }
    }
}
