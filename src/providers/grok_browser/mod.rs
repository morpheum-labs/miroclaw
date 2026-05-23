pub mod models;
pub mod session;

use crate::config::schema::{GrokBrowserConfig, GrokBrowserSessionMode};
use crate::providers::bun_browser::BunBrowserClient;
use crate::providers::traits::{
    ChatMessage, ChatRequest, ChatResponse, Provider, ProviderCapabilities,
};
use crate::providers::ProviderRuntimeOptions;
use async_trait::async_trait;
use models::{
    bool_str_arg, disable_search_arg, extract_answer, extract_conversation_id,
    is_conversation_not_found, map_grok_model,
};
use serde_json::{Map, Value};
use session::{GrokBrowserSession, GrokSessionStore};
use std::sync::{Arc, Mutex as StdMutex};
use tokio::sync::Mutex;

const GROK_CHAT: &str = "grok/chat";
const GROK_AGENT_CHAT: &str = "grok/agent-chat";
const GROK_CHATFOLLOW: &str = "grok/chatfollow";
const GROK_MODES: &str = "grok/modes";
const GROK_AGENTS: &str = "grok/agents";

const DEFAULT_MODEL_MARKER: &str = "default";
const GROK_BROWSER_SUPPORTED_TEMPERATURES: [f64; 2] = [0.7, 1.0];
const TEMP_EPSILON: f64 = 1e-9;

pub struct GrokBrowserProvider {
    client: Mutex<BunBrowserClient>,
    config: GrokBrowserConfig,
    resolved_agent_id: StdMutex<Option<String>>,
    sessions: Arc<GrokSessionStore>,
    call_lock: Mutex<()>,
}

impl GrokBrowserProvider {
    pub fn new(options: &ProviderRuntimeOptions) -> anyhow::Result<Self> {
        let config = options.grok_browser.clone();
        let timeout_secs = if config.request_timeout_secs > 0 {
            Some(config.request_timeout_secs)
        } else {
            None
        };
        let client = BunBrowserClient::new_deferred(config.host.clone(), timeout_secs)?;
        Ok(Self {
            client: Mutex::new(client),
            config,
            resolved_agent_id: StdMutex::new(None),
            sessions: Arc::new(GrokSessionStore::new()),
            call_lock: Mutex::new(()),
        })
    }

    fn effective_agent_id(&self) -> Option<String> {
        if let Ok(guard) = self.resolved_agent_id.lock() {
            if let Some(id) = guard.as_ref().filter(|s| !s.trim().is_empty()) {
                return Some(id.clone());
            }
        }
        self.config
            .agent_id
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }

    fn effective_model(&self, model: &str) -> String {
        let trimmed = model.trim();
        if trimmed.is_empty() || trimmed == DEFAULT_MODEL_MARKER {
            self.config
                .model
                .as_deref()
                .map(str::trim)
                .filter(|m| !m.is_empty())
                .unwrap_or("auto")
                .to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn validate_temperature(temperature: f64) -> anyhow::Result<()> {
        if !temperature.is_finite() {
            anyhow::bail!("Grok browser provider received non-finite temperature value");
        }
        if !GROK_BROWSER_SUPPORTED_TEMPERATURES
            .iter()
            .any(|v| (temperature - v).abs() < TEMP_EPSILON)
        {
            tracing::debug!(
                requested = temperature,
                "Grok browser ignores temperature; model mode controls sampling"
            );
        }
        Ok(())
    }

    fn merge_system_user(system: Option<&str>, user: &str) -> String {
        match system.map(str::trim).filter(|s| !s.is_empty()) {
            Some(system) => format!("{system}\n\n{}", user.trim()),
            None => user.trim().to_string(),
        }
    }

    fn last_user_message(messages: &[ChatMessage]) -> anyhow::Result<&str> {
        messages
            .iter()
            .rev()
            .find(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .filter(|s| !s.trim().is_empty())
            .ok_or_else(|| {
                anyhow::anyhow!("Grok browser provider requires a non-empty user message")
            })
    }

    fn system_message(messages: &[ChatMessage]) -> Option<&str> {
        messages
            .iter()
            .find(|m| m.role == "system")
            .map(|m| m.content.as_str())
            .filter(|s| !s.trim().is_empty())
    }

    async fn run_site_locked(&self, name: &str, args: Map<String, Value>) -> anyhow::Result<Value> {
        let _guard = self.call_lock.lock().await;
        let mut client = self.client.lock().await;
        client
            .run_site_mut(name, args)
            .await
            .map_err(|err| anyhow::anyhow!(err))
    }

    async fn chat_stateless(
        &self,
        query: &str,
        model: &str,
    ) -> anyhow::Result<(String, Option<String>)> {
        let mut args = Map::new();
        args.insert("query".into(), Value::String(query.to_string()));
        args.insert(
            "model".into(),
            Value::String(map_grok_model(model).to_string()),
        );
        args.insert(
            "disableSearch".into(),
            disable_search_arg(self.config.disable_search),
        );
        args.insert("newChat".into(), bool_str_arg(true));

        let data = self.run_site_locked(GROK_CHAT, args).await?;
        let answer = extract_answer(&data)
            .ok_or_else(|| anyhow::anyhow!("Empty response from grok/chat"))?;
        Ok((answer, extract_conversation_id(&data)))
    }

    async fn agent_chat(
        &self,
        agent_id: &str,
        query: &str,
        model: &str,
    ) -> anyhow::Result<(String, Option<String>)> {
        let mut args = Map::new();
        args.insert("agent".into(), Value::String(agent_id.to_string()));
        args.insert("query".into(), Value::String(query.to_string()));
        args.insert(
            "model".into(),
            Value::String(map_grok_model(model).to_string()),
        );
        args.insert(
            "disableSearch".into(),
            disable_search_arg(self.config.disable_search),
        );
        args.insert("newChat".into(), bool_str_arg(false));

        let data = self.run_site_locked(GROK_AGENT_CHAT, args).await?;
        let answer = extract_answer(&data)
            .ok_or_else(|| anyhow::anyhow!("Empty response from grok/agent-chat"))?;
        Ok((answer, extract_conversation_id(&data)))
    }

    async fn chat_follow(
        &self,
        conversation_id: &str,
        query: &str,
        model: &str,
    ) -> anyhow::Result<(String, Option<String>)> {
        let mut args = Map::new();
        args.insert(
            "conversation".into(),
            Value::String(conversation_id.to_string()),
        );
        args.insert("query".into(), Value::String(query.to_string()));
        args.insert(
            "model".into(),
            Value::String(map_grok_model(model).to_string()),
        );
        args.insert(
            "disableSearch".into(),
            disable_search_arg(self.config.disable_search),
        );

        let data = self.run_site_locked(GROK_CHATFOLLOW, args).await?;
        let answer = extract_answer(&data)
            .ok_or_else(|| anyhow::anyhow!("Empty response from grok/chatfollow"))?;
        Ok((answer, extract_conversation_id(&data)))
    }

    async fn resolve_agent_name(&self) -> anyhow::Result<()> {
        if self.effective_agent_id().is_some() {
            return Ok(());
        }
        let Some(name) = self
            .config
            .agent_name
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            return Ok(());
        };

        let data = self.run_site_locked(GROK_AGENTS, Map::new()).await?;
        let agents = data
            .get("agents")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("grok/agents returned no agents list"))?;

        for agent in agents {
            if agent.get("name").and_then(|v| v.as_str()) == Some(name) {
                if let Some(id) = agent.get("id").and_then(|v| v.as_str()) {
                    if let Ok(mut guard) = self.resolved_agent_id.lock() {
                        *guard = Some(id.to_string());
                    }
                    return Ok(());
                }
            }
        }

        anyhow::bail!(
            "Grok Project Agent '{name}' not found. Run bun-browser site grok/agents to list available agents."
        )
    }

    async fn chat_follow_mode(
        &self,
        messages: &[ChatMessage],
        model: &str,
        session_key: Option<&str>,
    ) -> anyhow::Result<String> {
        let user = Self::last_user_message(messages)?;
        let effective_model = self.effective_model(model);
        let key = session_key.map(str::trim).filter(|s| !s.is_empty());

        if let Some(key) = key {
            if let Some(existing) = self.sessions.get(key).await {
                match self
                    .chat_follow(&existing.conversation_id, user, effective_model.as_str())
                    .await
                {
                    Ok((answer, conv)) => {
                        let conversation_id = conv.unwrap_or(existing.conversation_id);
                        self.sessions
                            .set(
                                key,
                                GrokBrowserSession {
                                    conversation_id,
                                    agent_id: self.effective_agent_id(),
                                    model: map_grok_model(&effective_model).to_string(),
                                },
                            )
                            .await;
                        return Ok(answer);
                    }
                    Err(err) if is_conversation_not_found(&err) => {
                        self.sessions.clear(key).await;
                    }
                    Err(err) => return Err(err),
                }
            }

            let query = if self.effective_agent_id().is_some() {
                user.to_string()
            } else {
                Self::merge_system_user(Self::system_message(messages), user)
            };

            let (answer, conversation_id) = if let Some(agent_id) = self.effective_agent_id() {
                self.agent_chat(&agent_id, &query, effective_model.as_str())
                    .await?
            } else {
                self.chat_stateless(&query, effective_model.as_str())
                    .await?
            };

            if let Some(conversation_id) = conversation_id {
                self.sessions
                    .set(
                        key,
                        GrokBrowserSession {
                            conversation_id,
                            agent_id: self.effective_agent_id(),
                            model: map_grok_model(&effective_model).to_string(),
                        },
                    )
                    .await;
            }

            return Ok(answer);
        }

        let query = Self::merge_system_user(Self::system_message(messages), user);
        let (answer, _) = self
            .chat_stateless(&query, effective_model.as_str())
            .await?;
        Ok(answer)
    }

    pub fn sessions(&self) -> Arc<GrokSessionStore> {
        Arc::clone(&self.sessions)
    }
}

#[async_trait]
impl Provider for GrokBrowserProvider {
    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            native_tool_calling: false,
            vision: false,
            prompt_caching: false,
        }
    }

    async fn warmup(&self) -> anyhow::Result<()> {
        self.resolve_agent_name().await?;
        let _ = self.run_site_locked(GROK_MODES, Map::new()).await?;
        Ok(())
    }

    async fn chat_with_system(
        &self,
        system_prompt: Option<&str>,
        message: &str,
        model: &str,
        temperature: f64,
    ) -> anyhow::Result<String> {
        Self::validate_temperature(temperature)?;
        let query = Self::merge_system_user(system_prompt, message);
        let (answer, _) = self
            .chat_stateless(&query, self.effective_model(model).as_str())
            .await?;
        Ok(answer)
    }

    async fn chat_with_history(
        &self,
        messages: &[ChatMessage],
        model: &str,
        temperature: f64,
    ) -> anyhow::Result<String> {
        Self::validate_temperature(temperature)?;
        if self.config.session_mode == GrokBrowserSessionMode::Stateless {
            let system = Self::system_message(messages);
            let user = Self::last_user_message(messages)?;
            let query = Self::merge_system_user(system, user);
            let (answer, _) = self
                .chat_stateless(&query, self.effective_model(model).as_str())
                .await?;
            return Ok(answer);
        }
        self.chat_follow_mode(messages, model, None).await
    }

    async fn chat(
        &self,
        request: ChatRequest<'_>,
        model: &str,
        temperature: f64,
    ) -> anyhow::Result<ChatResponse> {
        if let Some(tools) = request.tools {
            if !tools.is_empty() {
                anyhow::bail!(
                    "Grok browser provider does not support native tool calling. \
                     Use an API-based provider for agentic tool loops."
                );
            }
        }

        let text = if self.config.session_mode == GrokBrowserSessionMode::Stateless {
            self.chat_with_history(request.messages, model, temperature)
                .await?
        } else {
            self.chat_follow_mode(request.messages, model, request.session_key)
                .await?
        };

        Ok(ChatResponse {
            text: Some(text),
            tool_calls: Vec::new(),
            usage: None,
            reasoning_content: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_options(config: GrokBrowserConfig) -> ProviderRuntimeOptions {
        ProviderRuntimeOptions {
            grok_browser: config,
            ..ProviderRuntimeOptions::default()
        }
    }

    #[test]
    fn merge_system_user_formats_prompt() {
        assert_eq!(
            GrokBrowserProvider::merge_system_user(Some("sys"), "user"),
            "sys\n\nuser"
        );
        assert_eq!(GrokBrowserProvider::merge_system_user(None, "user"), "user");
    }

    #[test]
    fn effective_model_uses_config_default() {
        let provider = GrokBrowserProvider {
            client: Mutex::new(
                BunBrowserClient::new_deferred(Some("http://127.0.0.1:1".into()), Some(1)).unwrap(),
            ),
            config: GrokBrowserConfig {
                model: Some("fast".into()),
                ..GrokBrowserConfig::default()
            },
            resolved_agent_id: StdMutex::new(None),
            sessions: Arc::new(GrokSessionStore::new()),
            call_lock: Mutex::new(()),
        };
        assert_eq!(provider.effective_model("default"), "fast");
        assert_eq!(provider.effective_model("expert"), "expert");
    }
}
