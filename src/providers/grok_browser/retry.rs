use super::models::{
    bool_str_arg, disable_search_arg, extract_conversation_id, extract_grok_chat_answer,
    extract_tab_id, grok_chat_retry_delay_secs, grok_http_timeout_secs,
    is_grok_adapter_retryable_error, is_incomplete_grok_answer, map_grok_model,
    GROK_CHAT_MAX_ATTEMPTS,
};
use crate::providers::bun_browser::{BunBrowserClient, RunSiteOptions};
use serde_json::{Map, Value};
use std::time::Duration;
use tokio::time::sleep;

const GROK_CHAT: &str = "grok/chat";
const GROK_AGENT_CHAT: &str = "grok/agent-chat";
const GROK_CHATFOLLOW: &str = "grok/chatfollow";

#[derive(Debug, Clone)]
pub enum GrokSiteOp {
    Chat {
        query: String,
        model: String,
        new_chat: bool,
    },
    AgentChat {
        agent_id: String,
        query: String,
        model: String,
    },
    ChatFollow {
        conversation_id: String,
        query: String,
        model: String,
    },
    Simple {
        adapter: String,
        args: Map<String, Value>,
    },
}

impl GrokSiteOp {
    pub fn model(&self) -> &str {
        match self {
            Self::Chat { model, .. }
            | Self::AgentChat { model, .. }
            | Self::ChatFollow { model, .. } => model,
            Self::Simple { .. } => "fast",
        }
    }

    fn adapter(&self) -> &str {
        match self {
            Self::Chat { .. } => GROK_CHAT,
            Self::AgentChat { .. } => GROK_AGENT_CHAT,
            Self::ChatFollow { .. } => GROK_CHATFOLLOW,
            Self::Simple { adapter, .. } => adapter,
        }
    }

    fn build_args(&self, awaiting_reply: bool, disable_search: bool) -> Map<String, Value> {
        match self {
            Self::Simple { args, .. } => args.clone(),
            Self::Chat {
                query,
                model,
                new_chat,
            } => {
                let mut args = Map::new();
                args.insert("query".into(), Value::String(query.clone()));
                args.insert("model".into(), Value::String(map_grok_model(model)));
                args.insert("disableSearch".into(), disable_search_arg(disable_search));
                if awaiting_reply {
                    args.insert("waitOnly".into(), bool_str_arg(true));
                } else {
                    args.insert("newChat".into(), bool_str_arg(*new_chat));
                }
                args
            }
            Self::AgentChat {
                agent_id,
                query,
                model,
            } => {
                let mut args = Map::new();
                args.insert("agent".into(), Value::String(agent_id.clone()));
                args.insert("query".into(), Value::String(query.clone()));
                args.insert("model".into(), Value::String(map_grok_model(model)));
                args.insert("disableSearch".into(), disable_search_arg(disable_search));
                if awaiting_reply {
                    args.insert("waitOnly".into(), bool_str_arg(true));
                } else {
                    args.insert("newChat".into(), bool_str_arg(false));
                }
                args
            }
            Self::ChatFollow {
                conversation_id,
                query,
                model,
            } => {
                let mut args = Map::new();
                args.insert(
                    "conversation".into(),
                    Value::String(conversation_id.clone()),
                );
                args.insert("query".into(), Value::String(query.clone()));
                args.insert("model".into(), Value::String(map_grok_model(model)));
                args.insert("disableSearch".into(), disable_search_arg(disable_search));
                if awaiting_reply {
                    args.insert("waitOnly".into(), bool_str_arg(true));
                }
                args
            }
        }
    }
}

pub struct GrokRetryResult {
    pub answer: String,
    pub conversation_id: Option<String>,
    pub tab_id: Option<String>,
    pub data: Value,
}

pub async fn run_grok_site_with_retry(
    client: &mut BunBrowserClient,
    op: &GrokSiteOp,
    disable_search: bool,
    tab_id: Option<String>,
) -> anyhow::Result<GrokRetryResult> {
    let model = op.model();
    let adapter = op.adapter();
    let timeout = Duration::from_secs(grok_http_timeout_secs(model));
    let mut awaiting_reply = false;
    let mut last_answer = String::new();
    let mut last_tab = tab_id.clone();

    for attempt in 1..=GROK_CHAT_MAX_ATTEMPTS {
        let args = op.build_args(awaiting_reply, disable_search);
        let run_result = client
            .run_site_with_options(
                adapter,
                args,
                RunSiteOptions {
                    tab_id: last_tab.clone(),
                    timeout: Some(timeout),
                },
            )
            .await;

        match run_result {
            Err(err) => {
                let message = err.to_string();
                if is_grok_adapter_retryable_error(&message) {
                    if message.contains("Still generating") || message.contains("Empty response") {
                        awaiting_reply = true;
                    }
                    if attempt < GROK_CHAT_MAX_ATTEMPTS {
                        tracing::warn!(
                            adapter,
                            attempt,
                            max = GROK_CHAT_MAX_ATTEMPTS,
                            "grok adapter retryable error: {message}"
                        );
                        sleep(Duration::from_secs(grok_chat_retry_delay_secs(
                            &message,
                            awaiting_reply,
                        )))
                        .await;
                        continue;
                    }
                }
                return Err(err);
            }
            Ok(data) => {
                if let Some(tab) = extract_tab_id(&data) {
                    last_tab = Some(tab);
                }
                awaiting_reply = true;

                let answer = extract_grok_chat_answer(&data).unwrap_or_default();
                if answer.is_empty() {
                    if matches!(op, GrokSiteOp::Simple { .. }) {
                        return Ok(GrokRetryResult {
                            answer: String::new(),
                            conversation_id: extract_conversation_id(&data),
                            tab_id: last_tab,
                            data,
                        });
                    }
                    if attempt < GROK_CHAT_MAX_ATTEMPTS {
                        tracing::warn!(
                            adapter,
                            attempt,
                            max = GROK_CHAT_MAX_ATTEMPTS,
                            "grok adapter returned empty answer; polling in-flight reply"
                        );
                        sleep(Duration::from_secs(grok_chat_retry_delay_secs(
                            "Empty response",
                            true,
                        )))
                        .await;
                        continue;
                    }
                    anyhow::bail!("Empty response from {adapter}");
                }

                if !is_incomplete_grok_answer(&answer) {
                    return Ok(GrokRetryResult {
                        answer,
                        conversation_id: extract_conversation_id(&data),
                        tab_id: last_tab,
                        data,
                    });
                }

                last_answer = answer;
                if attempt < GROK_CHAT_MAX_ATTEMPTS {
                    tracing::warn!(
                        adapter,
                        attempt,
                        max = GROK_CHAT_MAX_ATTEMPTS,
                        preview = %last_answer.chars().take(120).collect::<String>(),
                        "grok adapter returned progress text; polling in-flight reply"
                    );
                    sleep(Duration::from_secs(grok_chat_retry_delay_secs(
                        "Still generating",
                        true,
                    )))
                    .await;
                }
            }
        }
    }

    if !last_answer.is_empty() {
        anyhow::bail!(
            "grok adapter returned progress text instead of a finished reply after {GROK_CHAT_MAX_ATTEMPTS} attempts: {}",
            last_answer.chars().take(200).collect::<String>()
        );
    }
    anyhow::bail!("Empty response from {adapter} after {GROK_CHAT_MAX_ATTEMPTS} attempts")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_op_builds_wait_only_on_retry() {
        let op = GrokSiteOp::Chat {
            query: "hello".into(),
            model: "fast".into(),
            new_chat: true,
        };
        let initial = op.build_args(false, true);
        assert_eq!(
            initial.get("newChat").and_then(|v| v.as_str()),
            Some("true")
        );
        assert!(initial.get("waitOnly").is_none());

        let retry = op.build_args(true, true);
        assert_eq!(retry.get("waitOnly").and_then(|v| v.as_str()), Some("true"));
        assert!(retry.get("newChat").is_none());
    }

    #[test]
    fn chatfollow_op_builds_wait_only() {
        let op = GrokSiteOp::ChatFollow {
            conversation_id: "abc".into(),
            query: "follow".into(),
            model: "expert".into(),
        };
        let retry = op.build_args(true, false);
        assert_eq!(retry.get("waitOnly").and_then(|v| v.as_str()), Some("true"));
    }
}
