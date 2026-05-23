use serde_json::Value;

pub fn map_grok_model(model: &str) -> &'static str {
    match model.trim().to_lowercase().as_str() {
        "fast" | "grok-3" => "fast",
        "expert" | "grok-4" => "expert",
        "auto" => "auto",
        _ => "auto",
    }
}

pub fn disable_search_arg(disable_search: bool) -> Value {
    Value::String(if disable_search { "true" } else { "false" }.to_string())
}

pub fn bool_str_arg(value: bool) -> Value {
    Value::String(if value { "true" } else { "false" }.to_string())
}

pub fn extract_answer(data: &Value) -> Option<String> {
    data.get("answer")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn extract_conversation_id(data: &Value) -> Option<String> {
    data.get("conversationId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn is_conversation_not_found(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("conversation not found")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn map_grok_model_variants() {
        assert_eq!(map_grok_model("fast"), "fast");
        assert_eq!(map_grok_model("GROK-3"), "fast");
        assert_eq!(map_grok_model("expert"), "expert");
        assert_eq!(map_grok_model("grok-4"), "expert");
        assert_eq!(map_grok_model("unknown"), "auto");
    }

    #[test]
    fn extract_answer_and_conversation_id() {
        let data = json!({
            "answer": " hello ",
            "conversationId": "abc-123"
        });
        assert_eq!(extract_answer(&data).as_deref(), Some("hello"));
        assert_eq!(extract_conversation_id(&data).as_deref(), Some("abc-123"));
    }
}
