use super::defs::ToolCall;

const TOOL_CALL_BEGIN: &str = "<|tool_call_begin|>";
const TOOL_CALL_END: &str = "<|tool_call_end|>";

pub struct ParsedOutput {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
}

fn find_end_marker(s: &str) -> Option<usize> {
    if let Some(i) = s.find(TOOL_CALL_END) {
        return Some(i);
    }
    // Handle model typos: </|tool_call_end|> or </|tool_call_end| or <|/tool_call_end|> etc.
    let patterns = ["</|tool_call_end|>", "</|tool_call_end|", "<|/tool_call_end|>"];
    for pat in patterns {
        if let Some(i) = s.find(pat) {
            return Some(i);
        }
    }
    None
}

fn end_marker_len(s: &str, start: usize) -> usize {
    let rest = &s[start..];
    if rest.starts_with(TOOL_CALL_END) { TOOL_CALL_END.len() }
    else if rest.starts_with("</|tool_call_end|>") { "</|tool_call_end|>".len() }
    else if rest.starts_with("</|tool_call_end|") { "</|tool_call_end|".len() }
    else if rest.starts_with("<|/tool_call_end|>") { "<|/tool_call_end|>".len() }
    else { TOOL_CALL_END.len() }
}

fn extract_json(text: &str) -> Option<ToolCall> {
    // Find the first '{' and last '}' to extract JSON
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    if start >= end {
        return None;
    }
    let json_str = &text[start..=end];
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(json_str) {
        let name = parsed.get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let arguments = parsed.get("arguments")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        if !name.is_empty() {
            return Some(ToolCall { name, arguments });
        }
    }
    None
}

pub fn parse_tool_calls(raw: &str) -> ParsedOutput {
    let mut text = String::new();
    let mut tool_calls = Vec::new();
    let mut remaining = raw;

    while let Some(start_idx) = remaining.find(TOOL_CALL_BEGIN) {
        text.push_str(&remaining[..start_idx]);

        let after_begin = &remaining[start_idx + TOOL_CALL_BEGIN.len()..];

        if let Some(end_idx) = find_end_marker(after_begin) {
            let call_content = after_begin[..end_idx].trim();
            if let Some(tc) = extract_json(call_content) {
                tool_calls.push(tc);
            }
            let consumed = end_idx + end_marker_len(after_begin, end_idx);
            remaining = &after_begin[consumed..];
        } else {
            // No end marker — try to extract JSON anyway (model may have forgotten it)
            if let Some(tc) = extract_json(after_begin) {
                tool_calls.push(tc);
                // Skip to end since we can't find the end marker
                remaining = "";
            } else {
                text.push_str(&remaining[start_idx..]);
                break;
            }
        }
    }

    text.push_str(remaining);
    ParsedOutput { text: text.trim().to_string(), tool_calls }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_tool_calls() {
        let parsed = parse_tool_calls("Hello, how can I help?");
        assert_eq!(parsed.text, "Hello, how can I help?");
        assert!(parsed.tool_calls.is_empty());
    }

    #[test]
    fn test_single_tool_call() {
        let raw = r#"<|tool_call_begin|>
{"name": "get_weather", "arguments": {"city": "Tokyo"}}
<|tool_call_end|>"#;
        let parsed = parse_tool_calls(raw);
        assert_eq!(parsed.text, "");
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "get_weather");
        assert_eq!(parsed.tool_calls[0].arguments["city"], "Tokyo");
    }

    #[test]
    fn test_mixed_text_and_tool_call() {
        let raw = r#"I'll check that for you.
<|tool_call_begin|>
{"name": "search", "arguments": {"query": "rust"}}
<|tool_call_end|>
Let me look this up."#;
        let parsed = parse_tool_calls(raw);
        assert!(parsed.text.contains("I'll check that for you."));
        assert!(parsed.text.contains("Let me look this up."));
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "search");
    }

    #[test]
    fn test_multiple_tool_calls() {
        let raw = r#"<|tool_call_begin|>
{"name": "get_weather", "arguments": {"city": "Tokyo"}}
<|tool_call_end|>
<|tool_call_begin|>
{"name": "get_weather", "arguments": {"city": "London"}}
<|tool_call_end|>"#;
        let parsed = parse_tool_calls(raw);
        assert_eq!(parsed.tool_calls.len(), 2);
        assert_eq!(parsed.tool_calls[0].name, "get_weather");
        assert_eq!(parsed.tool_calls[1].name, "get_weather");
    }

    #[test]
    fn test_typo_end_marker() {
        let raw = r#"<|tool_call_begin|>
{"name": "current_time", "arguments": {"format": "HH:mm:ss"}}
</|tool_call_end|"#;
        let parsed = parse_tool_calls(raw);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "current_time");
    }

    #[test]
    fn test_no_end_marker_with_json() {
        let raw = r#"<|tool_call_begin|>
{"name": "current_time", "arguments": {"format": "HH:mm:ss"}}"#;
        let parsed = parse_tool_calls(raw);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "current_time");
    }

    #[test]
    fn test_spelled_out_begin_marker() {
        // Model sometimes spells markers as text characters
        let raw = "<|tool_call_begin|>\n{\"name\": \"calc\", \"arguments\": {\"expr\": \"2+2\"}}\n<|tool_call_end|>";
        let parsed = parse_tool_calls(raw);
        assert_eq!(parsed.tool_calls.len(), 1);
        assert_eq!(parsed.tool_calls[0].name, "calc");
    }
}
