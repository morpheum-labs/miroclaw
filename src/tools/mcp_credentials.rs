//! Clawgotcha-backed MCP credential overlays (memory-only cache).

use std::collections::HashMap;
use std::sync::Arc;

use clawgotcha::client::ClawgotchaHttpAdapter;
use parking_lot::RwLock;
use serde_json::Value;

#[derive(Clone, Debug, Default)]
pub struct ResolvedMcpAuth {
    pub headers: HashMap<String, String>,
    pub env: HashMap<String, String>,
}

struct Cached {
    #[allow(dead_code)]
    revision: u64,
    by_server: HashMap<String, ResolvedMcpAuth>,
}

/// Fetches and caches decrypted MCP bindings per delegate agent (invalidated on sync).
pub struct McpCredentialResolver {
    client: Arc<ClawgotchaHttpAdapter>,
    cache: RwLock<HashMap<String, Cached>>,
}

impl McpCredentialResolver {
    pub fn new(client: Arc<ClawgotchaHttpAdapter>) -> Self {
        Self {
            client,
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn invalidate_all(&self) {
        self.cache.write().clear();
    }

    /// Resolve vault auth overlay for `server_name` when running as delegate `agent_name`.
    pub async fn resolve_for_server(
        &self,
        agent_name: Option<&str>,
        server_name: &str,
    ) -> Option<ResolvedMcpAuth> {
        let agent = agent_name?.trim();
        if agent.is_empty() {
            return None;
        }
        let sn = server_name.trim();
        if sn.is_empty() {
            return None;
        }

        {
            let guard = self.cache.read();
            if let Some(c) = guard.get(agent) {
                if let Some(auth) = c.by_server.get(sn) {
                    return Some(auth.clone());
                }
            }
        }

        let resp = self
            .client
            .fetch_mcp_credentials_by_agent_name(agent)
            .await
            .map_err(|e| {
                tracing::debug!(error = %e, %agent, "mcp credentials fetch failed");
                e
            })
            .ok()?;

        let mut by_server = HashMap::new();
        for b in resp.mcp_bindings {
            if let Some(auth) = map_binding(&b.material_kind, &b.payload) {
                by_server.insert(b.mcp_server_name.clone(), auth);
            }
        }

        self.cache.write().insert(
            agent.to_string(),
            Cached {
                revision: resp.revision,
                by_server: by_server.clone(),
            },
        );

        by_server.get(sn).cloned()
    }
}

fn json_string_field(payload: &Value, keys: &[&str]) -> Option<String> {
    for k in keys {
        if let Some(s) = payload.get(*k).and_then(|v| v.as_str()) {
            let t = s.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
    }
    None
}

fn map_binding(kind: &str, payload: &Value) -> Option<ResolvedMcpAuth> {
    match kind.trim() {
        "bearer_token" => {
            let token = json_string_field(payload, &["token", "access_token", "bearer"])?;
            let mut headers = HashMap::new();
            headers.insert("Authorization".to_string(), format!("Bearer {token}"));
            Some(ResolvedMcpAuth {
                headers,
                env: HashMap::new(),
            })
        }
        "api_key" => {
            let key = json_string_field(payload, &["key", "api_key"])?;
            let mut headers = HashMap::new();
            headers.insert("X-API-Key".to_string(), key.clone());
            let mut env = HashMap::new();
            env.insert("API_KEY".to_string(), key);
            Some(ResolvedMcpAuth { headers, env })
        }
        "github_pat" => {
            let pat = json_string_field(payload, &["pat", "token"])?;
            let mut headers = HashMap::new();
            headers.insert("Authorization".to_string(), format!("token {pat}"));
            let mut env = HashMap::new();
            env.insert("GITHUB_TOKEN".to_string(), pat);
            Some(ResolvedMcpAuth { headers, env })
        }
        "oauth_tokens" => {
            let token = json_string_field(payload, &["access_token", "token"])?;
            let mut headers = HashMap::new();
            headers.insert("Authorization".to_string(), format!("Bearer {token}"));
            Some(ResolvedMcpAuth {
                headers,
                env: HashMap::new(),
            })
        }
        _ => {
            tracing::debug!(%kind, "mcp credential material_kind not mapped for MCP overlay");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_github_pat_to_headers_and_env() {
        let auth = map_binding("github_pat", &json!({ "pat": "ghp_test_example_not_real" }))
            .expect("maps");
        assert!(auth.headers["Authorization"].contains("ghp_test"));
        assert_eq!(
            auth.env.get("GITHUB_TOKEN").map(String::as_str),
            Some("ghp_test_example_not_real")
        );
    }

    #[test]
    fn maps_bearer_token() {
        let auth = map_binding("bearer_token", &json!({ "token": "tok_x" })).expect("maps");
        assert_eq!(
            auth.headers.get("Authorization").map(String::as_str),
            Some("Bearer tok_x")
        );
    }
}
