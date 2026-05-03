//! Deferred MCP tool loading — stubs and activated-tool tracking.
//!
//! When `mcp.deferred_loading` is enabled, MCP tool schemas are NOT eagerly
//! included in the LLM context window. Instead, only lightweight stubs (name +
//! description) are exposed in the system prompt. The LLM must call the built-in
//! `tool_search` tool to fetch full schemas, which moves them into the
//! [`ActivatedToolSet`] for the current conversation.

use std::collections::HashMap;
use std::sync::Arc;

use crate::tools::mcp_client::McpRegistry;
use crate::tools::mcp_protocol::McpToolDef;
use crate::tools::mcp_tool::McpToolWrapper;
use crate::tools::traits::{Tool, ToolSpec};

// ── DeferredMcpToolStub ──────────────────────────────────────────────────

/// A lightweight stub representing a known-but-not-yet-loaded MCP tool.
/// Full parameter schemas live only in [`McpRegistry`] until activation.
#[derive(Debug, Clone)]
pub struct DeferredMcpToolStub {
    /// Prefixed name: `<server_name>__<tool_name>`.
    pub prefixed_name: String,
    /// Human-readable description (extracted once from the MCP `tools/list` payload).
    pub description: String,
}

impl DeferredMcpToolStub {
    pub fn new(prefixed_name: String, description: String) -> Self {
        Self {
            prefixed_name,
            description,
        }
    }
}

// ── DeferredMcpToolSet ───────────────────────────────────────────────────

/// Collection of all deferred MCP tool stubs discovered at startup.
/// Provides keyword search for `tool_search`.
#[derive(Clone)]
pub struct DeferredMcpToolSet {
    /// All stubs — exposed for test construction.
    pub stubs: Vec<DeferredMcpToolStub>,
    /// Shared registry — exposed for test construction.
    pub registry: Arc<McpRegistry>,
    /// MCP server names from workspace skills — boosts [`Self::search`] ranking.
    pub mcp_server_hints: Vec<String>,
}

impl DeferredMcpToolSet {
    /// Build the set from a connected [`McpRegistry`] (no skill-based ranking hints).
    pub async fn from_registry(registry: Arc<McpRegistry>) -> Self {
        Self::from_registry_with_hints(registry, &[]).await
    }

    /// Build stubs and attach optional skill-declared MCP server hints for `tool_search` ranking.
    pub async fn from_registry_with_hints(registry: Arc<McpRegistry>, hints: &[String]) -> Self {
        let names = registry.tool_names();
        let mut stubs = Vec::with_capacity(names.len());
        for name in names {
            if let Some(def) = registry.get_tool_def(&name).await {
                let description = def
                    .description
                    .clone()
                    .unwrap_or_else(|| "MCP tool".to_string());
                stubs.push(DeferredMcpToolStub::new(name, description));
            }
        }
        Self {
            stubs,
            registry,
            mcp_server_hints: hints.to_vec(),
        }
    }

    /// All stub names (for rendering in the system prompt).
    pub fn stub_names(&self) -> Vec<&str> {
        self.stubs
            .iter()
            .map(|s| s.prefixed_name.as_str())
            .collect()
    }

    /// Number of deferred stubs.
    pub fn len(&self) -> usize {
        self.stubs.len()
    }

    /// Whether the set is empty.
    pub fn is_empty(&self) -> bool {
        self.stubs.is_empty()
    }

    /// Look up stubs by exact name. Used for `select:name1,name2` queries.
    pub fn get_by_name(&self, name: &str) -> Option<&DeferredMcpToolStub> {
        self.stubs.iter().find(|s| s.prefixed_name == name)
    }

    /// Keyword search — returns stubs whose name or description contains any
    /// of the query terms (case-insensitive). Results are ranked by number of
    /// matching terms (descending).
    pub fn search(&self, query: &str, max_results: usize) -> Vec<&DeferredMcpToolStub> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|t| t.to_ascii_lowercase())
            .collect();
        if terms.is_empty() {
            return self.stubs.iter().take(max_results).collect();
        }

        let mut scored: Vec<(&DeferredMcpToolStub, usize)> = self
            .stubs
            .iter()
            .filter_map(|stub| {
                let haystack = format!(
                    "{} {}",
                    stub.prefixed_name.to_ascii_lowercase(),
                    stub.description.to_ascii_lowercase()
                );
                let hits = terms
                    .iter()
                    .filter(|t| haystack.contains(t.as_str()))
                    .count();
                if hits > 0 {
                    let hint_boost = stub
                        .prefixed_name
                        .split_once("__")
                        .map(|(srv, _)| {
                            if self.mcp_server_hints.iter().any(|hint| hint == srv) {
                                1_000usize
                            } else {
                                0usize
                            }
                        })
                        .unwrap_or(0);
                    Some((stub, hits.saturating_add(hint_boost)))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored
            .into_iter()
            .take(max_results)
            .map(|(s, _)| s)
            .collect()
    }

    /// Activate a stub by name, returning a boxed [`Tool`].
    pub async fn activate_deferred_tool(&self, name: &str) -> Option<Box<dyn Tool>> {
        let stub = self.get_by_name(name)?;
        let def = if let Some(d) = self.registry.get_tool_def(name).await {
            d
        } else {
            // Unit tests may use stubs without a live MCP server; synthesize a minimal def.
            let orig = name
                .split_once("__")
                .map(|(_, n)| n.to_string())
                .unwrap_or_else(|| name.to_string());
            McpToolDef {
                name: orig,
                description: Some(stub.description.clone()),
                input_schema: serde_json::json!({"type": "object", "properties": {}}),
            }
        };
        let wrapper = McpToolWrapper::new(name.to_string(), def, Arc::clone(&self.registry));
        Some(Box::new(wrapper) as Box<dyn Tool>)
    }

    /// Return the full [`ToolSpec`] for a stub (for inclusion in `tool_search` results).
    pub async fn deferred_tool_spec(&self, name: &str) -> Option<ToolSpec> {
        let stub = self.get_by_name(name)?;
        let def = if let Some(d) = self.registry.get_tool_def(name).await {
            d
        } else {
            let orig = name
                .split_once("__")
                .map(|(_, n)| n.to_string())
                .unwrap_or_else(|| name.to_string());
            McpToolDef {
                name: orig,
                description: Some(stub.description.clone()),
                input_schema: serde_json::json!({"type": "object", "properties": {}}),
            }
        };
        let wrapper = McpToolWrapper::new(name.to_string(), def, Arc::clone(&self.registry));
        Some(wrapper.spec())
    }
}

// ── ActivatedToolSet ─────────────────────────────────────────────────────

/// Per-conversation mutable state tracking which deferred tools have been
/// activated (i.e. their full schemas have been fetched via `tool_search`).
/// The agent loop consults this each iteration to decide which tool_specs
/// to include in the LLM request.
pub struct ActivatedToolSet {
    tools: HashMap<String, Arc<dyn Tool>>,
}

impl ActivatedToolSet {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    pub fn activate(&mut self, name: String, tool: Arc<dyn Tool>) {
        self.tools.insert(name, tool);
    }

    pub fn is_activated(&self, name: &str) -> bool {
        self.tools.contains_key(name)
    }

    /// Clone the Arc so the caller can drop the mutex guard before awaiting.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    /// Resolve an activated tool by exact name first, then by unique MCP suffix.
    ///
    /// Some providers occasionally strip the `<server>__` prefix when calling a
    /// deferred MCP tool after `tool_search` activation. When the suffix maps to
    /// exactly one activated tool, allow that call to proceed.
    pub fn get_resolved(&self, name: &str) -> Option<Arc<dyn Tool>> {
        if let Some(tool) = self.get(name) {
            return Some(tool);
        }
        if name.contains("__") {
            return None;
        }

        let mut resolved = None;
        for (tool_name, tool) in &self.tools {
            let Some((_, suffix)) = tool_name.split_once("__") else {
                continue;
            };
            if suffix != name {
                continue;
            }
            if resolved.is_some() {
                return None;
            }
            resolved = Some(Arc::clone(tool));
        }

        resolved
    }

    pub fn tool_specs(&self) -> Vec<ToolSpec> {
        self.tools.values().map(|t| t.spec()).collect()
    }

    pub fn tool_names(&self) -> Vec<&str> {
        self.tools.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for ActivatedToolSet {
    fn default() -> Self {
        Self::new()
    }
}

// ── System prompt helper ─────────────────────────────────────────────────

/// Build the `<available-deferred-tools>` section for the system prompt.
/// Lists only tool names so the LLM knows what is available without
/// consuming context window on full schemas. Includes an instruction
/// block that tells the LLM to call `tool_search` to activate them.
pub fn build_deferred_tools_section(deferred: &DeferredMcpToolSet) -> String {
    if deferred.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    out.push_str("## Deferred Tools\n\n");
    out.push_str(
        "The tools listed below are available but NOT yet loaded. \
         To use any of them you MUST first call the `tool_search` tool \
         to fetch their full schemas. Use `\"select:name1,name2\"` for \
         exact tools or keywords to search. Once activated, the tools \
         become callable for the rest of the conversation.\n\n",
    );
    out.push_str("<available-deferred-tools>\n");
    for stub in &deferred.stubs {
        out.push_str(&stub.prefixed_name);
        out.push_str(" - ");
        out.push_str(&stub.description);
        out.push('\n');
    }
    out.push_str("</available-deferred-tools>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_stub(name: &str, desc: &str) -> DeferredMcpToolStub {
        DeferredMcpToolStub::new(name.to_string(), desc.to_string())
    }

    #[test]
    fn stub_stores_description() {
        let stub = make_stub("fs__read", "Read a file");
        assert_eq!(stub.description, "Read a file");
    }

    #[test]
    fn activated_set_tracks_activation() {
        use crate::tools::traits::ToolResult;
        use async_trait::async_trait;

        struct FakeTool;
        #[async_trait]
        impl Tool for FakeTool {
            fn name(&self) -> &str {
                "fake"
            }
            fn description(&self) -> &str {
                "fake tool"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn execute(&self, _: serde_json::Value) -> anyhow::Result<ToolResult> {
                Ok(ToolResult {
                    success: true,
                    output: String::new(),
                    error: None,
                })
            }
        }

        let mut set = ActivatedToolSet::new();
        assert!(!set.is_activated("fake"));
        set.activate("fake".into(), Arc::new(FakeTool));
        assert!(set.is_activated("fake"));
        assert!(set.get("fake").is_some());
        assert_eq!(set.tool_specs().len(), 1);
    }

    #[test]
    fn activated_set_resolves_unique_suffix() {
        use crate::tools::traits::ToolResult;
        use async_trait::async_trait;

        struct FakeTool;
        #[async_trait]
        impl Tool for FakeTool {
            fn name(&self) -> &str {
                "docker-mcp__extract_text"
            }
            fn description(&self) -> &str {
                "fake tool"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn execute(&self, _: serde_json::Value) -> anyhow::Result<ToolResult> {
                Ok(ToolResult {
                    success: true,
                    output: String::new(),
                    error: None,
                })
            }
        }

        let mut set = ActivatedToolSet::new();
        set.activate("docker-mcp__extract_text".into(), Arc::new(FakeTool));
        assert!(set.get_resolved("extract_text").is_some());
    }

    #[test]
    fn activated_set_rejects_ambiguous_suffix() {
        use crate::tools::traits::ToolResult;
        use async_trait::async_trait;

        struct FakeTool(&'static str);
        #[async_trait]
        impl Tool for FakeTool {
            fn name(&self) -> &str {
                self.0
            }
            fn description(&self) -> &str {
                "fake tool"
            }
            fn parameters_schema(&self) -> serde_json::Value {
                serde_json::json!({})
            }
            async fn execute(&self, _: serde_json::Value) -> anyhow::Result<ToolResult> {
                Ok(ToolResult {
                    success: true,
                    output: String::new(),
                    error: None,
                })
            }
        }

        let mut set = ActivatedToolSet::new();
        set.activate(
            "docker-mcp__extract_text".into(),
            Arc::new(FakeTool("docker-mcp__extract_text")),
        );
        set.activate(
            "ocr-mcp__extract_text".into(),
            Arc::new(FakeTool("ocr-mcp__extract_text")),
        );
        assert!(set.get_resolved("extract_text").is_none());
    }

    #[test]
    fn build_deferred_section_empty_when_no_stubs() {
        let set = DeferredMcpToolSet {
            stubs: vec![],
            registry: std::sync::Arc::new(
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(McpRegistry::connect_all(&[]))
                    .unwrap(),
            ),
            mcp_server_hints: vec![],
        };
        assert!(build_deferred_tools_section(&set).is_empty());
    }

    #[test]
    fn build_deferred_section_lists_names() {
        let stubs = vec![
            make_stub("fs__read_file", "Read a file"),
            make_stub("git__status", "Git status"),
        ];
        let set = DeferredMcpToolSet {
            stubs,
            registry: std::sync::Arc::new(
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(McpRegistry::connect_all(&[]))
                    .unwrap(),
            ),
            mcp_server_hints: vec![],
        };
        let section = build_deferred_tools_section(&set);
        assert!(section.contains("<available-deferred-tools>"));
        assert!(section.contains("fs__read_file - Read a file"));
        assert!(section.contains("git__status - Git status"));
        assert!(section.contains("</available-deferred-tools>"));
    }

    #[test]
    fn build_deferred_section_includes_tool_search_instruction() {
        let stubs = vec![make_stub("fs__read_file", "Read a file")];
        let set = DeferredMcpToolSet {
            stubs,
            registry: std::sync::Arc::new(
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(McpRegistry::connect_all(&[]))
                    .unwrap(),
            ),
            mcp_server_hints: vec![],
        };
        let section = build_deferred_tools_section(&set);
        assert!(
            section.contains("tool_search"),
            "deferred section must instruct the LLM to use tool_search"
        );
        assert!(
            section.contains("## Deferred Tools"),
            "deferred section must include a heading"
        );
    }

    #[test]
    fn build_deferred_section_multiple_servers() {
        let stubs = vec![
            make_stub("server_a__list", "List items"),
            make_stub("server_a__create", "Create item"),
            make_stub("server_b__query", "Query records"),
        ];
        let set = DeferredMcpToolSet {
            stubs,
            registry: std::sync::Arc::new(
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(McpRegistry::connect_all(&[]))
                    .unwrap(),
            ),
            mcp_server_hints: vec![],
        };
        let section = build_deferred_tools_section(&set);
        assert!(section.contains("server_a__list"));
        assert!(section.contains("server_a__create"));
        assert!(section.contains("server_b__query"));
        assert!(
            section.contains("tool_search"),
            "section must mention tool_search for multi-server setups"
        );
    }

    #[test]
    fn keyword_search_ranks_by_hits() {
        let stubs = vec![
            make_stub("fs__read_file", "Read a file from disk"),
            make_stub("fs__write_file", "Write a file to disk"),
            make_stub("git__log", "Show git log"),
        ];
        let set = DeferredMcpToolSet {
            stubs,
            registry: std::sync::Arc::new(
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(McpRegistry::connect_all(&[]))
                    .unwrap(),
            ),
            mcp_server_hints: vec![],
        };

        // "file read" should rank fs__read_file highest (2 hits vs 1)
        let results = set.search("file read", 5);
        assert!(!results.is_empty());
        assert_eq!(results[0].prefixed_name, "fs__read_file");
    }

    #[test]
    fn get_by_name_returns_correct_stub() {
        let stubs = vec![
            make_stub("a__one", "Tool one"),
            make_stub("b__two", "Tool two"),
        ];
        let set = DeferredMcpToolSet {
            stubs,
            registry: std::sync::Arc::new(
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(McpRegistry::connect_all(&[]))
                    .unwrap(),
            ),
            mcp_server_hints: vec![],
        };
        assert!(set.get_by_name("a__one").is_some());
        assert!(set.get_by_name("nonexistent").is_none());
    }

    #[test]
    fn search_across_multiple_servers() {
        let stubs = vec![
            make_stub("server_a__read_file", "Read a file from disk"),
            make_stub("server_b__read_config", "Read configuration from database"),
        ];
        let set = DeferredMcpToolSet {
            stubs,
            registry: std::sync::Arc::new(
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(McpRegistry::connect_all(&[]))
                    .unwrap(),
            ),
            mcp_server_hints: vec![],
        };

        // "read" should match stubs from both servers
        let results = set.search("read", 10);
        assert_eq!(results.len(), 2);

        // "file" should match only server_a
        let results = set.search("file", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].prefixed_name, "server_a__read_file");

        // "config database" should rank server_b highest (2 hits)
        let results = set.search("config database", 10);
        assert!(!results.is_empty());
        assert_eq!(results[0].prefixed_name, "server_b__read_config");
    }

    #[test]
    fn search_prioritizes_skill_mcp_server_hints() {
        let stubs = vec![
            make_stub("other__tool", "Generic helper"),
            make_stub("hinted__tool", "Hinted server capability"),
        ];
        let set = DeferredMcpToolSet {
            stubs,
            registry: std::sync::Arc::new(
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(McpRegistry::connect_all(&[]))
                    .unwrap(),
            ),
            mcp_server_hints: vec!["hinted".to_string()],
        };
        // Both match "tool"; hinted server should rank first due to boost.
        let results = set.search("tool", 5);
        assert_eq!(results[0].prefixed_name, "hinted__tool");
    }
}
