//! Built-in `tool_search` tool for on-demand MCP tool schema loading.
//!
//! When `mcp.deferred_loading` is enabled, this tool lets the LLM discover and
//! activate deferred MCP tools. Supports two query modes:
//! - `select:name1,name2` — fetch exact tools by prefixed name.
//! - Free-text keyword search — returns the best-matching stubs.

use std::fmt::Write;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

use crate::tools::mcp_deferred::{ActivatedToolSet, DeferredMcpToolSet};
use crate::tools::traits::{Tool, ToolResult};

/// Default maximum number of search results.
const DEFAULT_MAX_RESULTS: usize = 5;

/// Built-in tool that fetches full schemas for deferred MCP tools.
pub struct ToolSearchTool {
    deferred: DeferredMcpToolSet,
    activated: Arc<Mutex<ActivatedToolSet>>,
}

impl ToolSearchTool {
    pub fn new(deferred: DeferredMcpToolSet, activated: Arc<Mutex<ActivatedToolSet>>) -> Self {
        Self {
            deferred,
            activated,
        }
    }
}

#[async_trait]
impl Tool for ToolSearchTool {
    fn name(&self) -> &str {
        "tool_search"
    }

    fn description(&self) -> &str {
        "Discover deferred MCP tools and optionally load full schemas. \
         Use \"select:name1,name2\" to load schemas for exact tools. \
         For keyword search, set \"activate\": false to list names and descriptions only \
         (no schema load); omit \"activate\" or true to load full schemas for matches."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "description": "Query to find deferred tools. Use \"select:<tool_name>\" for direct selection, or keywords to search.",
                    "type": "string"
                },
                "max_results": {
                    "description": "Maximum number of results to return (default: 5)",
                    "type": "number",
                    "default": DEFAULT_MAX_RESULTS
                },
                "activate": {
                    "description": "Keyword search only: when false, return a short preview list without loading schemas or activating tools (default: true).",
                    "type": "boolean",
                    "default": true
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim();

        let max_results = args
            .get("max_results")
            .and_then(|v| v.as_u64())
            .map(|v| usize::try_from(v).unwrap_or(DEFAULT_MAX_RESULTS))
            .unwrap_or(DEFAULT_MAX_RESULTS);

        let activate = args
            .get("activate")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        if query.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("query parameter is required".into()),
            });
        }

        // Parse query mode
        if let Some(names_str) = query.strip_prefix("select:") {
            // Exact selection mode — always loads full schemas
            let names: Vec<&str> = names_str.split(',').map(str::trim).collect();
            return self.select_tools(&names).await;
        }

        // Keyword search mode
        let results = self.deferred.search(query, max_results);
        if results.is_empty() {
            return Ok(ToolResult {
                success: true,
                output: "No matching deferred tools found.".into(),
                error: None,
            });
        }

        if !activate {
            let mut lines: Vec<String> = results
                .iter()
                .map(|stub| format!("{} — {}", stub.prefixed_name, stub.description))
                .collect();
            lines.push(String::from(
                "\n(Set \"activate\": true or call tool_search again to load full schemas.)",
            ));
            return Ok(ToolResult {
                success: true,
                output: lines.join("\n"),
                error: None,
            });
        }

        // Activate and return full specs
        let mut output = String::from("<functions>\n");
        let mut activated_count = 0;

        for stub in &results {
            let Some(spec) = self.deferred.deferred_tool_spec(&stub.prefixed_name).await else {
                continue;
            };
            let need_activate = !self
                .activated
                .lock()
                .unwrap()
                .is_activated(&stub.prefixed_name);
            let maybe_tool = if need_activate {
                self.deferred
                    .activate_deferred_tool(&stub.prefixed_name)
                    .await
            } else {
                None
            };
            if let Some(tool) = maybe_tool {
                self.activated
                    .lock()
                    .unwrap()
                    .activate(stub.prefixed_name.clone(), Arc::from(tool));
                activated_count += 1;
            }
            let _ = writeln!(
                output,
                "<function>{{\"name\": \"{}\", \"description\": \"{}\", \"parameters\": {}}}</function>",
                spec.name,
                spec.description.replace('"', "\\\""),
                spec.parameters
            );
        }

        output.push_str("</functions>\n");

        tracing::debug!(
            "tool_search: query={query:?}, matched={}, activated={activated_count}",
            results.len()
        );

        Ok(ToolResult {
            success: true,
            output,
            error: None,
        })
    }
}

impl ToolSearchTool {
    async fn select_tools(&self, names: &[&str]) -> anyhow::Result<ToolResult> {
        let mut output = String::from("<functions>\n");
        let mut not_found = Vec::new();
        let mut activated_count = 0;

        for name in names {
            if name.is_empty() {
                continue;
            }
            match self.deferred.deferred_tool_spec(name).await {
                Some(spec) => {
                    let need_activate = !self.activated.lock().unwrap().is_activated(name);
                    let maybe_tool = if need_activate {
                        self.deferred.activate_deferred_tool(name).await
                    } else {
                        None
                    };
                    if let Some(tool) = maybe_tool {
                        self.activated
                            .lock()
                            .unwrap()
                            .activate(name.to_string(), Arc::from(tool));
                        activated_count += 1;
                    }
                    let _ = writeln!(
                        output,
                        "<function>{{\"name\": \"{}\", \"description\": \"{}\", \"parameters\": {}}}</function>",
                        spec.name,
                        spec.description.replace('"', "\\\""),
                        spec.parameters
                    );
                }
                None => {
                    not_found.push(*name);
                }
            }
        }

        output.push_str("</functions>\n");

        if !not_found.is_empty() {
            let _ = write!(output, "\nNot found: {}", not_found.join(", "));
        }

        tracing::debug!(
            "tool_search select: requested={}, activated={activated_count}, not_found={}",
            names.len(),
            not_found.len()
        );

        Ok(ToolResult {
            success: true,
            output,
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::mcp_client::McpRegistry;
    use crate::tools::mcp_deferred::DeferredMcpToolStub;

    async fn make_deferred_set(stubs: Vec<DeferredMcpToolStub>) -> DeferredMcpToolSet {
        let registry = Arc::new(McpRegistry::connect_all(&[]).await.unwrap());
        DeferredMcpToolSet {
            stubs,
            registry,
            mcp_server_hints: vec![],
        }
    }

    fn make_stub(name: &str, desc: &str) -> DeferredMcpToolStub {
        DeferredMcpToolStub::new(name.to_string(), desc.to_string())
    }

    #[tokio::test]
    async fn tool_metadata() {
        let tool = ToolSearchTool::new(
            make_deferred_set(vec![]).await,
            Arc::new(Mutex::new(ActivatedToolSet::new())),
        );
        assert_eq!(tool.name(), "tool_search");
        assert!(!tool.description().is_empty());
        assert!(tool.parameters_schema()["properties"]["query"].is_object());
    }

    #[tokio::test]
    async fn empty_query_returns_error() {
        let tool = ToolSearchTool::new(
            make_deferred_set(vec![]).await,
            Arc::new(Mutex::new(ActivatedToolSet::new())),
        );
        let result = tool
            .execute(serde_json::json!({"query": ""}))
            .await
            .unwrap();
        assert!(!result.success);
    }

    #[tokio::test]
    async fn select_nonexistent_tool_reports_not_found() {
        let tool = ToolSearchTool::new(
            make_deferred_set(vec![]).await,
            Arc::new(Mutex::new(ActivatedToolSet::new())),
        );
        let result = tool
            .execute(serde_json::json!({"query": "select:nonexistent"}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("Not found"));
    }

    #[tokio::test]
    async fn keyword_search_no_matches() {
        let tool = ToolSearchTool::new(
            make_deferred_set(vec![make_stub("fs__read", "Read file")]).await,
            Arc::new(Mutex::new(ActivatedToolSet::new())),
        );
        let result = tool
            .execute(serde_json::json!({"query": "zzzzz_nonexistent"}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("No matching"));
    }

    #[tokio::test]
    async fn keyword_search_finds_match() {
        let activated = Arc::new(Mutex::new(ActivatedToolSet::new()));
        let tool = ToolSearchTool::new(
            make_deferred_set(vec![make_stub("fs__read", "Read a file from disk")]).await,
            Arc::clone(&activated),
        );
        let result = tool
            .execute(serde_json::json!({"query": "read file"}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("<function>"));
        assert!(result.output.contains("fs__read"));
        // Tool should now be activated
        assert!(activated.lock().unwrap().is_activated("fs__read"));
    }

    #[tokio::test]
    async fn keyword_search_preview_does_not_activate() {
        let activated = Arc::new(Mutex::new(ActivatedToolSet::new()));
        let tool = ToolSearchTool::new(
            make_deferred_set(vec![make_stub("fs__read", "Read a file from disk")]).await,
            Arc::clone(&activated),
        );
        let result = tool
            .execute(serde_json::json!({
                "query": "read file",
                "activate": false
            }))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("fs__read"));
        assert!(!result.output.contains("<function>"));
        assert!(!activated.lock().unwrap().is_activated("fs__read"));
    }

    /// Verify tool_search works with stubs from multiple MCP servers,
    /// simulating a daemon-mode setup where several servers are deferred.
    #[tokio::test]
    async fn multiple_servers_stubs_all_searchable() {
        let activated = Arc::new(Mutex::new(ActivatedToolSet::new()));
        let stubs = vec![
            make_stub("server_a__list_files", "List files on server A"),
            make_stub("server_a__read_file", "Read file on server A"),
            make_stub("server_b__query_db", "Query database on server B"),
            make_stub("server_b__insert_row", "Insert row on server B"),
        ];
        let tool = ToolSearchTool::new(make_deferred_set(stubs).await, Arc::clone(&activated));

        // Search should find tools across both servers
        let result = tool
            .execute(serde_json::json!({"query": "file"}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("server_a__list_files"));
        assert!(result.output.contains("server_a__read_file"));

        // Server B tools should also be searchable
        let result = tool
            .execute(serde_json::json!({"query": "database query"}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(result.output.contains("server_b__query_db"));
    }

    /// Verify select mode activates tools and they stay activated across calls,
    /// matching the daemon-mode pattern where a single ActivatedToolSet persists.
    #[tokio::test]
    async fn select_activates_and_persists_across_calls() {
        let activated = Arc::new(Mutex::new(ActivatedToolSet::new()));
        let stubs = vec![
            make_stub("srv__tool_a", "Tool A"),
            make_stub("srv__tool_b", "Tool B"),
        ];
        let tool = ToolSearchTool::new(make_deferred_set(stubs).await, Arc::clone(&activated));

        // Activate tool_a
        let result = tool
            .execute(serde_json::json!({"query": "select:srv__tool_a"}))
            .await
            .unwrap();
        assert!(result.success);
        assert!(activated.lock().unwrap().is_activated("srv__tool_a"));
        assert!(!activated.lock().unwrap().is_activated("srv__tool_b"));

        // Activate tool_b in a separate call
        let result = tool
            .execute(serde_json::json!({"query": "select:srv__tool_b"}))
            .await
            .unwrap();
        assert!(result.success);

        // Both should remain activated
        let guard = activated.lock().unwrap();
        assert!(guard.is_activated("srv__tool_a"));
        assert!(guard.is_activated("srv__tool_b"));
        assert_eq!(guard.tool_specs().len(), 2);
    }

    /// Verify re-activating an already-activated tool does not duplicate it.
    #[tokio::test]
    async fn reactivation_is_idempotent() {
        let activated = Arc::new(Mutex::new(ActivatedToolSet::new()));
        let tool = ToolSearchTool::new(
            make_deferred_set(vec![make_stub("srv__tool", "A tool")]).await,
            Arc::clone(&activated),
        );

        tool.execute(serde_json::json!({"query": "select:srv__tool"}))
            .await
            .unwrap();
        tool.execute(serde_json::json!({"query": "select:srv__tool"}))
            .await
            .unwrap();

        assert_eq!(activated.lock().unwrap().tool_specs().len(), 1);
    }
}
