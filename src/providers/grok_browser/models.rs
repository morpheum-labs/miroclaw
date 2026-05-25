use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

const KNOWN_GROK_MODES: &[&str] = &[
    "fast",
    "auto",
    "expert",
    "heavy",
    "beta",
    "grok-420-computer-use-sa",
];

static GROK_EVALUATING_TIMER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)Evaluating .+ • \d+s").expect("valid regex"));
static GROK_TIMER_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"•\s*\d+s\s*$").expect("valid regex"));
static GROK_STRUCTURING_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(Structuring|Compiling|Drafting|Formulating|Preparing|Organizing)\b")
        .expect("valid regex")
});

pub const GROK_CHAT_MAX_ATTEMPTS: u32 = 3;
pub const GROK_CHAT_RETRY_DELAY_SECS: u64 = 5;
pub const GROK_CHAT_POLL_RETRY_DELAY_SECS: u64 = 15;
pub const GROK_HTTP_TIMEOUT_BUFFER_SECS: u64 = 300;

pub fn map_grok_model(model: &str) -> String {
    let normalized = model.trim().to_lowercase();
    match normalized.as_str() {
        "fast" | "grok-3" => "fast".into(),
        "expert" | "grok-4" => "expert".into(),
        "heavy" | "grok-4-heavy" | "team-of-experts" => "heavy".into(),
        "auto" => "auto".into(),
        "beta" => "beta".into(),
        "grok-420-computer-use-sa" => "grok-420-computer-use-sa".into(),
        other if KNOWN_GROK_MODES.contains(&other) => other.to_string(),
        other if !other.is_empty() => other.to_string(),
        _ => "fast".into(),
    }
}

pub fn grok_mode_wait_secs(model: &str) -> u64 {
    match map_grok_model(model).as_str() {
        "expert" => 25 * 60,
        "heavy" => 40 * 60,
        "beta" | "grok-420-computer-use-sa" => 15 * 60,
        _ => 15 * 60,
    }
}

pub fn grok_http_timeout_secs(model: &str) -> u64 {
    grok_mode_wait_secs(model) + GROK_HTTP_TIMEOUT_BUFFER_SECS
}

pub fn disable_search_arg(disable_search: bool) -> Value {
    Value::String(if disable_search { "true" } else { "false" }.to_string())
}

pub fn bool_str_arg(value: bool) -> Value {
    Value::String(if value { "true" } else { "false" }.to_string())
}

pub fn extract_grok_chat_answer(data: &Value) -> Option<String> {
    if let Some(answer_json) = data.get("answerJson") {
        if answer_json.is_object() || answer_json.is_array() {
            return serde_json::to_string(answer_json).ok();
        }
    }
    data.get("answer")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn extract_answer(data: &Value) -> Option<String> {
    extract_grok_chat_answer(data)
}

pub fn extract_conversation_id(data: &Value) -> Option<String> {
    data.get("conversationId")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

pub fn extract_tab_id(data: &Value) -> Option<String> {
    data.get("tab")
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                Some(s.to_string())
            } else if let Some(n) = v.as_u64() {
                Some(n.to_string())
            } else {
                None
            }
        })
        .filter(|s| !s.is_empty())
}

pub fn is_grok_adapter_retryable_error(err: &str) -> bool {
    err.contains("Mode change requires page reload")
        || err.contains("Empty response")
        || err.contains("Still generating")
}

pub fn is_conversation_not_found(err: &anyhow::Error) -> bool {
    err.to_string()
        .to_lowercase()
        .contains("conversation not found")
}

fn grok_progress_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return false;
    }
    if t.starts_with("Preview:") || t.starts_with("Searched web") || t.starts_with("Searched 𝕏")
    {
        return true;
    }
    if GROK_EVALUATING_TIMER.is_match(t) {
        return true;
    }
    if t.ends_with(" results") || t.ends_with(" posts") {
        return true;
    }
    if t.starts_with("Searching")
        || t.starts_with("Reading")
        || t.starts_with("Browsing")
        || t.starts_with("Fetching")
        || t.starts_with("Running tool")
        || t.starts_with("Identifying")
        || t.starts_with("Analyzing")
        || t.starts_with("Thinking")
    {
        return true;
    }
    if GROK_TIMER_SUFFIX.is_match(t) {
        return true;
    }
    if t.starts_with("Thought for ") && t.ends_with('s') {
        return true;
    }
    if t == "Agents thinking" {
        return true;
    }
    if t.starts_with("Agent ") && t.len() < 12 {
        return true;
    }
    if GROK_STRUCTURING_PREFIX.is_match(t) {
        return true;
    }
    if t.len() < 80
        && (t.ends_with(" thinking") || t.ends_with(" response") || t.ends_with(" JSON response"))
    {
        return true;
    }
    false
}

fn extract_grok_json_block(text: &str) -> Option<String> {
    let t = text.trim();
    let start = t.find('{')?;
    let slice = &t[start..];
    if serde_json::from_str::<Value>(slice).is_ok() {
        return Some(slice.to_string());
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, ch) in slice.chars().enumerate() {
        if in_string {
            if escape {
                escape = false;
            } else if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let candidate = &slice[..=i];
                    if serde_json::from_str::<Value>(candidate).is_ok() {
                        return Some(candidate.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn is_grok_progress_text(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if t.starts_with("Preview:") {
        return true;
    }
    if t.contains("Searched web") && GROK_EVALUATING_TIMER.is_match(t) {
        return true;
    }
    if t.contains("Searched web")
        && t.contains("results")
        && !t.contains('.')
        && !t.contains('!')
        && !t.contains('?')
    {
        return true;
    }
    if t.contains("Searched 𝕏")
        && t.contains("posts")
        && !t.contains('.')
        && !t.contains('!')
        && !t.contains('?')
    {
        return true;
    }
    if t.starts_with("Searched web") && t.len() < 500 {
        return true;
    }
    if t.contains("Agents thinking") && extract_grok_json_block(t).is_none() {
        return true;
    }
    if GROK_STRUCTURING_PREFIX.is_match(t) && t.len() < 120 {
        return true;
    }
    if GROK_TIMER_SUFFIX.is_match(t) && extract_grok_json_block(t).is_none() {
        return true;
    }
    let lines: Vec<&str> = t.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    if lines.is_empty() {
        return false;
    }
    lines.iter().all(|line| grok_progress_line(line))
}

pub fn looks_like_grok_final_answer(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() || is_grok_progress_text(t) {
        return false;
    }
    if let Some(json_block) = extract_grok_json_block(t) {
        if serde_json::from_str::<Value>(&json_block).is_ok() {
            return true;
        }
    }
    if t.contains("Searched web") && t.contains("results") {
        return false;
    }
    if t.starts_with("Searched web") {
        return false;
    }
    if t.len() < 12 {
        return false;
    }
    if t.contains('.') || t.contains('!') || t.contains('?') {
        if t.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3 {
            return true;
        }
    }
    t.len() >= 20
        && !t.contains("Searched 𝕏")
        && !t.contains("Searched web")
        && !t.contains("results")
        && !t.contains("posts")
}

pub fn is_incomplete_grok_answer(text: &str) -> bool {
    !looks_like_grok_final_answer(text.trim())
}

pub fn grok_chat_retry_delay_secs(err: &str, awaiting_reply: bool) -> u64 {
    if awaiting_reply || err.contains("Still generating") {
        GROK_CHAT_POLL_RETRY_DELAY_SECS
    } else {
        GROK_CHAT_RETRY_DELAY_SECS
    }
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
        assert_eq!(map_grok_model("heavy"), "heavy");
        assert_eq!(
            map_grok_model("grok-420-computer-use-sa"),
            "grok-420-computer-use-sa"
        );
        assert_eq!(map_grok_model("custom-mode-id"), "custom-mode-id");
    }

    #[test]
    fn grok_http_timeout_by_mode() {
        assert_eq!(grok_http_timeout_secs("fast"), 15 * 60 + 300);
        assert_eq!(grok_http_timeout_secs("expert"), 25 * 60 + 300);
        assert_eq!(grok_http_timeout_secs("heavy"), 40 * 60 + 300);
    }

    #[test]
    fn extract_answer_prefers_answer_json() {
        let data = json!({
            "answerFormat": "json",
            "answer": "{\n  \"category\": \"Other\"\n}",
            "answerJson": {"category": "Other"},
        });
        assert_eq!(
            extract_grok_chat_answer(&data).as_deref(),
            Some("{\"category\":\"Other\"}")
        );
    }

    #[test]
    fn is_incomplete_grok_answer_detects_progress() {
        assert!(is_incomplete_grok_answer(
            "Identifying the primary market event • 21s"
        ));
        assert!(!is_incomplete_grok_answer(
            r#"{"primary_event": "Tesla recall"}"#
        ));
        assert!(is_incomplete_grok_answer("Agents thinking"));
    }

    #[test]
    fn is_grok_adapter_retryable_error_matches() {
        assert!(is_grok_adapter_retryable_error(
            "Still generating: waitOnly"
        ));
        assert!(is_grok_adapter_retryable_error(
            "Empty response: Grok 未返回内容"
        ));
        assert!(!is_grok_adapter_retryable_error("Not logged in"));
    }

    #[test]
    fn extract_answer_and_conversation_id() {
        let data = json!({
            "answer": " hello ",
            "conversationId": "abc-123",
            "tab": "c416"
        });
        assert_eq!(extract_answer(&data).as_deref(), Some("hello"));
        assert_eq!(extract_conversation_id(&data).as_deref(), Some("abc-123"));
        assert_eq!(extract_tab_id(&data).as_deref(), Some("c416"));
    }
}
