//! Cache-oriented system prompt assembly: stable prefix vs volatile tail, separated by
//! [`SYSTEM_PROMPT_DYNAMIC_BOUNDARY`] for Anthropic multi-block system prompts.

use crate::config::{
    AutonomyConfig, DynamicContextConfig, EmbeddingRouteConfig, IdentityConfig, MemoryConfig,
    SkillsPromptInjectionMode,
};
use crate::context::DynamicContextPaths;
use crate::identity;
use crate::providers::ChatMessage;
use crate::security::AutonomyLevel;
use crate::skills::Skill;
use anyhow::Result;
use std::collections::hash_map::DefaultHasher;
use std::fmt::Write;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::SystemTime;

pub const SYSTEM_PROMPT_DYNAMIC_BOUNDARY: &str = "__SYSTEM_PROMPT_DYNAMIC_BOUNDARY__";

const BOOTSTRAP_MAX_CHARS: usize = 20_000;

/// Assembled system prompt: provider-visible text is `full()` =
/// `static_prefix` + boundary + `dynamic_tail`.
#[derive(Debug, Clone, Default)]
pub struct SystemPrompt {
    pub static_prefix: String,
    pub dynamic_tail: String,
}

impl SystemPrompt {
    #[must_use]
    pub fn full(&self) -> String {
        if self.dynamic_tail.is_empty() {
            return self.static_prefix.clone();
        }
        format!(
            "{}{}{}",
            self.static_prefix, SYSTEM_PROMPT_DYNAMIC_BOUNDARY, self.dynamic_tail
        )
    }
}

/// Build worker chat history: frozen parent [`SystemPrompt::full`] as system, one user turn
/// with parent summary, worker goal, and explicit callable tool names (hand coordinator path).
#[must_use]
pub fn build_forked_history(
    parent: &SystemPrompt,
    parent_turn_summary: &str,
    worker_goal: &str,
    worker_tool_names: &[String],
) -> Vec<ChatMessage> {
    let mut user = String::new();
    user.push_str("## Parent context (read-only summary)\n\n");
    user.push_str(parent_turn_summary);
    user.push_str("\n\n## Your worker task\n\n");
    user.push_str(worker_goal);
    user.push_str("\n\n## Callable tools this turn\n\n");
    if worker_tool_names.is_empty() {
        user.push_str("(none — respond with text only.)");
    } else {
        user.push_str(&worker_tool_names.join(", "));
        user.push_str("\n\nOnly invoke tools from this list.");
    }
    vec![ChatMessage::system(parent.full()), ChatMessage::user(user)]
}

/// Per-turn inputs for system prompt assembly.
#[derive(Debug, Clone)]
pub struct PromptAssemblyContext<'a> {
    pub workspace_dir: &'a Path,
    pub model_name: &'a str,
    pub tools: &'a [(&'a str, &'a str)],
    pub skills: &'a [Skill],
    pub identity_config: Option<&'a IdentityConfig>,
    pub bootstrap_max_chars: Option<usize>,
    pub autonomy_config: Option<&'a AutonomyConfig>,
    pub native_tools: bool,
    pub skills_prompt_mode: SkillsPromptInjectionMode,
    pub compact_context: bool,
    pub max_system_prompt_chars: usize,
    pub dynamic_context: Option<&'a DynamicContextConfig>,
    pub dynamic_paths: DynamicContextPaths<'a>,
    /// Appended to the **static** segment (before the boundary), e.g. XML tool instructions +
    /// deferred MCP section. Not subject to `max_system_prompt_chars` truncation (matches
    /// `loop_.rs` behavior).
    pub static_suffix: &'a str,
    /// Extra tool dispatcher instructions (gateway / XML path), appended inside `## Tools`.
    pub dispatcher_instructions: Option<&'a str>,
    /// Optional concrete security policy summary (gateway).
    pub security_summary: Option<&'a str>,
    /// Include the channel media markers section (gateway).
    pub include_channel_media: bool,
    /// When set, replaces wholesale workspace `MEMORY.md` in the dynamic tail (layered memory).
    pub layered_memory_markdown: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoFingerprint {
    tools_hash: u64,
    skills_hash: u64,
    workspace: String,
    bootstrap_max: Option<usize>,
    autonomy: Option<AutonomyLevel>,
    compact: bool,
    native_tools: bool,
    skills_mode_tag: u8,
    openclaw_files_hash: u64,
    aieos_hash: u64,
    static_suffix_hash: u64,
    identity_tag: u64,
    dispatcher_hash: u64,
    security_hash: u64,
    include_channel_media: bool,
}

#[derive(Default)]
pub struct AssemblyMemo {
    last_fp: Option<MemoFingerprint>,
    cached_static: String,
}

fn hash_str(s: &str) -> u64 {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn file_meta_hash(path: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    match std::fs::metadata(path) {
        Ok(m) => {
            if let Ok(ft) = m.modified() {
                if let Ok(d) = ft.duration_since(SystemTime::UNIX_EPOCH) {
                    d.as_secs().hash(&mut h);
                    d.subsec_nanos().hash(&mut h);
                }
            }
            m.len().hash(&mut h);
        }
        Err(_) => {
            path.hash(&mut h);
            0u8.hash(&mut h);
        }
    }
    h.finish()
}

fn openclaw_bootstrap_files_fp(workspace_dir: &Path) -> u64 {
    let mut h = DefaultHasher::new();
    for f in [
        "AGENTS.md",
        "SOUL.md",
        "TOOLS.md",
        "IDENTITY.md",
        "USER.md",
        "BOOTSTRAP.md",
    ] {
        file_meta_hash(&workspace_dir.join(f)).hash(&mut h);
    }
    h.finish()
}

fn aieos_fp(config: Option<&IdentityConfig>, workspace_dir: &Path) -> u64 {
    let Some(config) = config else {
        return 0;
    };
    if !identity::is_aieos_configured(config) {
        return 0;
    }
    let mut h = DefaultHasher::new();
    match identity::load_aieos_identity(config, workspace_dir) {
        Ok(Some(id)) => {
            identity::aieos_to_system_prompt(&id).hash(&mut h);
        }
        Ok(None) => {
            1u8.hash(&mut h);
        }
        Err(e) => {
            2u8.hash(&mut h);
            e.to_string().hash(&mut h);
        }
    }
    h.finish()
}

fn tools_fp(tools: &[(&str, &str)]) -> u64 {
    let mut h = DefaultHasher::new();
    for (n, d) in tools {
        n.hash(&mut h);
        d.hash(&mut h);
    }
    h.finish()
}

fn skills_fp(skills: &[Skill], mode: SkillsPromptInjectionMode) -> u64 {
    let mut h = DefaultHasher::new();
    let mode_tag = match mode {
        SkillsPromptInjectionMode::Full => 0u8,
        SkillsPromptInjectionMode::Compact => 1u8,
    };
    mode_tag.hash(&mut h);
    for s in skills {
        s.name.hash(&mut h);
        s.version.hash(&mut h);
    }
    h.finish()
}

fn identity_cfg_tag(config: Option<&IdentityConfig>) -> u64 {
    let mut h = DefaultHasher::new();
    if let Some(c) = config {
        c.format.hash(&mut h);
        if let Some(ref p) = c.aieos_path {
            p.hash(&mut h);
        }
        if let Some(ref inline) = c.aieos_inline {
            inline.hash(&mut h);
        }
    }
    h.finish()
}

fn fingerprint(ctx: &PromptAssemblyContext<'_>) -> MemoFingerprint {
    MemoFingerprint {
        tools_hash: tools_fp(ctx.tools),
        skills_hash: skills_fp(ctx.skills, ctx.skills_prompt_mode),
        workspace: ctx.workspace_dir.display().to_string(),
        bootstrap_max: ctx.bootstrap_max_chars,
        autonomy: ctx.autonomy_config.map(|a| a.level),
        compact: ctx.compact_context,
        native_tools: ctx.native_tools,
        skills_mode_tag: match ctx.skills_prompt_mode {
            SkillsPromptInjectionMode::Full => 0,
            SkillsPromptInjectionMode::Compact => 1,
        },
        openclaw_files_hash: openclaw_bootstrap_files_fp(ctx.workspace_dir),
        aieos_hash: aieos_fp(ctx.identity_config, ctx.workspace_dir),
        static_suffix_hash: hash_str(ctx.static_suffix),
        identity_tag: identity_cfg_tag(ctx.identity_config),
        dispatcher_hash: ctx.dispatcher_instructions.map(hash_str).unwrap_or(0),
        security_hash: ctx.security_summary.map(hash_str).unwrap_or(0),
        include_channel_media: ctx.include_channel_media,
    }
}

/// Build static + dynamic halves (no boundary inside parts), apply truncation to the
/// joined core (static + boundary + dynamic) per legacy `max_system_prompt_chars`, then
/// append `static_suffix` unchanged.
///
/// The returned `bool` is `true` when the memoized static body was reused unchanged.
pub fn assemble_with_memo(
    memo: &mut AssemblyMemo,
    ctx: &PromptAssemblyContext<'_>,
) -> Result<(SystemPrompt, bool)> {
    let fp = fingerprint(ctx);
    let static_reused = memo.last_fp.as_ref() == Some(&fp) && !memo.cached_static.is_empty();
    let static_body = if static_reused {
        memo.cached_static.clone()
    } else {
        let s = render_static_body(ctx)?;
        memo.cached_static = s.clone();
        memo.last_fp = Some(fp);
        s
    };

    let dynamic_tail = render_dynamic_tail(ctx)?;
    let mut static_prefix = static_body;
    if !ctx.static_suffix.is_empty() {
        static_prefix.push_str(ctx.static_suffix);
    }

    let mut core = String::with_capacity(
        static_prefix.len() + SYSTEM_PROMPT_DYNAMIC_BOUNDARY.len() + dynamic_tail.len() + 8,
    );
    core.push_str(&static_prefix);
    core.push_str(SYSTEM_PROMPT_DYNAMIC_BOUNDARY);
    core.push_str(&dynamic_tail);

    if ctx.max_system_prompt_chars > 0 && core.len() > ctx.max_system_prompt_chars {
        let mut end = ctx.max_system_prompt_chars;
        while !core.is_char_boundary(end) && end > 0 {
            end -= 1;
        }
        core.truncate(end);
        core.push_str("\n\n[System prompt truncated to fit context budget]\n");
        // After truncation the boundary may be lost; split conservatively.
        if let Some(idx) = core.find(SYSTEM_PROMPT_DYNAMIC_BOUNDARY) {
            let (s, rest) = core.split_at(idx);
            let d = &rest[SYSTEM_PROMPT_DYNAMIC_BOUNDARY.len()..];
            return Ok((
                SystemPrompt {
                    static_prefix: s.to_string(),
                    dynamic_tail: d.to_string(),
                },
                static_reused,
            ));
        }
        return Ok((
            SystemPrompt {
                static_prefix: core,
                dynamic_tail: String::new(),
            },
            static_reused,
        ));
    }

    Ok((
        SystemPrompt {
            static_prefix,
            dynamic_tail,
        },
        static_reused,
    ))
}

/// One-shot assembly without cross-turn memoization.
pub fn assemble_once(ctx: &PromptAssemblyContext<'_>) -> Result<SystemPrompt> {
    let mut m = AssemblyMemo::default();
    assemble_with_memo(&mut m, ctx).map(|(s, _)| s)
}

/// Full pipeline matching `channels::build_system_prompt_with_mode_and_autonomy` output shape.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_system_prompt_with_mode_and_autonomy(
    workspace_dir: &Path,
    model_name: &str,
    tools: &[(&str, &str)],
    skills: &[Skill],
    identity_config: Option<&IdentityConfig>,
    bootstrap_max_chars: Option<usize>,
    autonomy_config: Option<&AutonomyConfig>,
    native_tools: bool,
    skills_prompt_mode: SkillsPromptInjectionMode,
    compact_context: bool,
    max_system_prompt_chars: usize,
    dynamic_context: Option<&DynamicContextConfig>,
    dynamic_paths: DynamicContextPaths<'_>,
    static_suffix: &str,
) -> String {
    let ctx = PromptAssemblyContext {
        workspace_dir,
        model_name,
        tools,
        skills,
        identity_config,
        bootstrap_max_chars,
        autonomy_config,
        native_tools,
        skills_prompt_mode,
        compact_context,
        max_system_prompt_chars,
        dynamic_context,
        dynamic_paths,
        static_suffix,
        dispatcher_instructions: None,
        security_summary: None,
        include_channel_media: false,
        layered_memory_markdown: None,
    };
    match assemble_once(&ctx) {
        Ok(sp) => {
            let out = sp.full();
            if out.trim().is_empty() {
                "You are ZeroClaw, a fast and efficient AI assistant built in Rust. Be helpful, concise, and direct."
                    .to_string()
            } else {
                out
            }
        }
        Err(_) => "You are ZeroClaw, a fast and efficient AI assistant built in Rust. Be helpful, concise, and direct."
            .to_string(),
    }
}

/// Wiring for layered memory selector + dynamic tail injection.
#[derive(Debug, Clone, Copy)]
pub struct LayeredMemoryAssembly<'a> {
    pub memory: &'a MemoryConfig,
    pub session_key: &'a str,
    pub zeroclaw_dir: Option<&'a Path>,
    pub embedding_api_key: Option<&'a str>,
    pub embedding_routes: &'a [EmbeddingRouteConfig],
}

/// Inputs needed to rebuild the system message inside the tool loop (after compaction).
pub struct SystemPromptAssemblyRefs<'a> {
    pub workspace_dir: &'a Path,
    /// Owned so callers can switch models without holding a borrow across `await`.
    pub model_name: String,
    pub tool_descs: &'a [(&'a str, &'a str)],
    pub skills: &'a [Skill],
    pub identity_config: Option<&'a IdentityConfig>,
    pub bootstrap_max_chars: Option<usize>,
    pub autonomy_config: Option<&'a AutonomyConfig>,
    pub skills_prompt_mode: SkillsPromptInjectionMode,
    pub compact_context: bool,
    pub max_system_prompt_chars: usize,
    pub dynamic_context: Option<&'a DynamicContextConfig>,
    pub dynamic_paths: DynamicContextPaths<'a>,
    pub i18n_descs: &'a crate::i18n::ToolDescriptions,
    pub deferred_section: &'a str,
    pub thinking_prefix: Option<&'a str>,
    /// When present, the tool loop may compute a layered-memory markdown block each iteration.
    pub layered: Option<LayeredMemoryAssembly<'a>>,
}

impl SystemPromptAssemblyRefs<'_> {
    pub fn prompt_context<'b>(
        &'b self,
        native_tools: bool,
        static_suffix: &'b str,
        layered_memory_markdown: Option<&'b str>,
    ) -> PromptAssemblyContext<'b> {
        PromptAssemblyContext {
            workspace_dir: self.workspace_dir,
            model_name: self.model_name.as_str(),
            tools: self.tool_descs,
            skills: self.skills,
            identity_config: self.identity_config,
            bootstrap_max_chars: self.bootstrap_max_chars,
            autonomy_config: self.autonomy_config,
            native_tools,
            skills_prompt_mode: self.skills_prompt_mode,
            compact_context: self.compact_context,
            max_system_prompt_chars: self.max_system_prompt_chars,
            dynamic_context: self.dynamic_context,
            dynamic_paths: self.dynamic_paths,
            static_suffix,
            dispatcher_instructions: None,
            security_summary: None,
            include_channel_media: false,
            layered_memory_markdown,
        }
    }
}

/// Rebuild system prompt from refs + per-iteration `static_suffix`, patch `history[0]`.
pub fn patch_history_system_prompt(
    memo: &mut AssemblyMemo,
    refs: &SystemPromptAssemblyRefs<'_>,
    native_tools: bool,
    static_suffix: &str,
    layered_memory_markdown: Option<&str>,
    history: &mut [ChatMessage],
) -> Result<()> {
    let ctx = refs.prompt_context(native_tools, static_suffix, layered_memory_markdown);
    let (sp, static_reused) = assemble_with_memo(memo, &ctx)?;
    let body = sp.full();
    let content = match refs.thinking_prefix {
        Some(p) if !p.trim().is_empty() => format!("{p}\n\n{body}"),
        _ => body,
    };
    if let Some(m) = history.first_mut() {
        if m.role == "system" {
            m.content = content;
        }
    }
    let st = sp.static_prefix.chars().count() / 4;
    let dt = sp.dynamic_tail.chars().count() / 4;
    let st_u32 = u32::try_from(st).unwrap_or(u32::MAX);
    let dt_u32 = u32::try_from(dt).unwrap_or(u32::MAX);
    crate::agent::query_engine::record_system_prompt_assembly(st_u32, dt_u32, static_reused);
    Ok(())
}

/// When AIEOS identity loads successfully, legacy prompts omit `MEMORY.md`.
fn should_emit_memory_md(ctx: &PromptAssemblyContext<'_>) -> bool {
    match ctx.identity_config {
        None => true,
        Some(config) => {
            if !identity::is_aieos_configured(config) {
                return true;
            }
            !matches!(
                identity::load_aieos_identity(config, ctx.workspace_dir),
                Ok(Some(_))
            )
        }
    }
}

fn render_static_body(ctx: &PromptAssemblyContext<'_>) -> Result<String> {
    let mut prompt = String::with_capacity(8192);

    prompt.push_str(
        "## CRITICAL: No Tool Narration\n\n\
         NEVER narrate, announce, describe, or explain your tool usage to the user. \
         Do NOT say things like 'Let me check...', 'I will use http_request to...', \
         'I'll fetch that for you', 'Searching now...', or 'Using the web_search tool'. \
         The user must ONLY see the final answer. Tool calls are invisible infrastructure — \
         never reference them. If you catch yourself starting a sentence about what tool \
         you are about to use or just used, DELETE it and give the answer directly.\n\n",
    );

    prompt.push_str(
        "## CRITICAL: Tool Honesty\n\n\
         - NEVER fabricate, invent, or guess tool results. If a tool returns empty results, say \"No results found.\"\n\
         - If a tool call fails, report the error — never make up data to fill the gap.\n\
         - When unsure whether a tool call succeeded, ask the user rather than guessing.\n\n",
    );

    if ctx.tools.iter().any(|(name, _)| *name == "file_read") {
        prompt.push_str(
            "## Workspace file reads (freshness)\n\n\
             Workspace files can change on disk between turns (editors, sync, git, Docker bind mounts). \
             When the user asks to read, show, open, print, or display a workspace file, **call `file_read` for that path on this turn**. \
             Do not treat earlier `file_read` output or chat history as the authoritative current file contents after the user may have edited the file.\n\n",
        );
    }

    if !ctx.tools.is_empty() {
        prompt.push_str("## Tools\n\n");
        if ctx.compact_context {
            prompt.push_str("Available tools: ");
            let names: Vec<&str> = ctx.tools.iter().map(|(name, _)| *name).collect();
            prompt.push_str(&names.join(", "));
            prompt.push_str("\n\n");
        } else {
            prompt.push_str("You have access to the following tools:\n\n");
            for (name, desc) in ctx.tools {
                let _ = writeln!(prompt, "- **{name}**: {desc}");
            }
            prompt.push('\n');
        }
        if let Some(instr) = ctx.dispatcher_instructions {
            if !instr.trim().is_empty() {
                prompt.push_str(instr);
                prompt.push('\n');
            }
        }
    }

    let has_hardware = ctx.tools.iter().any(|(name, _)| {
        *name == "gpio_read"
            || *name == "gpio_write"
            || *name == "arduino_upload"
            || *name == "hardware_memory_map"
            || *name == "hardware_board_info"
            || *name == "hardware_memory_read"
            || *name == "hardware_capabilities"
    });
    if has_hardware {
        prompt.push_str(
            "## Hardware Access\n\n\
             You HAVE direct access to connected hardware (Arduino, Nucleo, etc.). The user owns this system and has configured it.\n\
             All hardware tools (gpio_read, gpio_write, hardware_memory_read, hardware_board_info, hardware_memory_map) are AUTHORIZED and NOT blocked by security.\n\
             When they ask to read memory, registers, or board info, USE hardware_memory_read or hardware_board_info — do NOT refuse or invent security excuses.\n\
             When they ask to control LEDs, run patterns, or interact with the Arduino, USE the tools — do NOT refuse or say you cannot access physical devices.\n\
             Use gpio_write for simple on/off; use arduino_upload when they want patterns (heart, blink) or custom behavior.\n\n",
        );
    }

    if ctx.native_tools {
        prompt.push_str(
            "## Your Task\n\n\
             When the user sends a message, respond naturally. Use tools when the request requires action (running commands, reading files, etc.).\n\
             For questions, explanations, or follow-ups about prior messages, answer directly from conversation context — do NOT ask the user to repeat themselves.\n\
             Do NOT: summarize this configuration, describe your capabilities, or output step-by-step meta-commentary.\n\n",
        );
    } else {
        prompt.push_str(
            "## Your Task\n\n\
             When the user sends a message, ACT on it. Use the tools to fulfill their request.\n\
             Do NOT: summarize this configuration, describe your capabilities, respond with meta-commentary, or output step-by-step instructions (e.g. \"1. First... 2. Next...\").\n\
             Instead: emit actual <tool_call> tags when you need to act. Just do what they ask.\n\n",
        );
    }

    prompt.push_str("## Safety\n\n");
    prompt.push_str("- Do not exfiltrate private data.\n");
    if ctx.autonomy_config.map(|cfg| cfg.level) != Some(AutonomyLevel::Full) {
        prompt.push_str(
            "- Do not run destructive commands without asking.\n\
             - Do not bypass oversight or approval mechanisms.\n",
        );
    }
    prompt.push_str("- Prefer `trash` over `rm` (recoverable beats gone forever).\n");
    prompt.push_str(match ctx.autonomy_config.map(|cfg| cfg.level) {
        Some(AutonomyLevel::Full) => {
            "- Respect the runtime autonomy policy: if a tool or action is allowed, execute it directly instead of asking the user for extra approval.\n\
             - If a tool or action is blocked by policy or unavailable, explain that concrete restriction instead of simulating an approval dialog.\n"
        }
        Some(AutonomyLevel::ReadOnly) => {
            "- Respect the runtime autonomy policy: this runtime is read-only for side effects unless a tool explicitly reports otherwise.\n\
             - If a requested action is blocked by policy, explain the restriction directly instead of simulating an approval dialog.\n"
        }
        _ => {
            "- When in doubt, ask before acting externally.\n\
             - Respect the runtime autonomy policy: ask for approval only when the current runtime policy actually requires it.\n\
             - If a tool or action is blocked by policy or unavailable, explain that concrete restriction instead of simulating an approval dialog.\n"
        }
    });
    prompt.push('\n');

    if let Some(s) = ctx.security_summary {
        if !s.trim().is_empty() {
            prompt.push_str("\n\n### Active Security Policy\n\n");
            prompt.push_str(s);
            prompt.push('\n');
        }
    }

    if !ctx.skills.is_empty() {
        prompt.push_str(&crate::skills::skills_to_prompt_with_mode(
            ctx.skills,
            ctx.workspace_dir,
            ctx.skills_prompt_mode,
        ));
        prompt.push_str("\n\n");
    }

    let _ = writeln!(
        prompt,
        "## Workspace\n\nWorking directory: `{}`\n",
        ctx.workspace_dir.display()
    );

    prompt.push_str("## Project Context\n\n");

    if let Some(config) = ctx.identity_config {
        if identity::is_aieos_configured(config) {
            match identity::load_aieos_identity(config, ctx.workspace_dir) {
                Ok(Some(aieos_identity)) => {
                    let aieos_prompt = identity::aieos_to_system_prompt(&aieos_identity);
                    if !aieos_prompt.is_empty() {
                        prompt.push_str(&aieos_prompt);
                        prompt.push_str("\n\n");
                    }
                }
                Ok(None) => {
                    let max_chars = ctx.bootstrap_max_chars.unwrap_or(BOOTSTRAP_MAX_CHARS);
                    load_openclaw_bootstrap_files_static(&mut prompt, ctx.workspace_dir, max_chars);
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load AIEOS identity: {e}. Using OpenClaw format."
                    );
                    let max_chars = ctx.bootstrap_max_chars.unwrap_or(BOOTSTRAP_MAX_CHARS);
                    load_openclaw_bootstrap_files_static(&mut prompt, ctx.workspace_dir, max_chars);
                }
            }
        } else {
            let max_chars = ctx.bootstrap_max_chars.unwrap_or(BOOTSTRAP_MAX_CHARS);
            load_openclaw_bootstrap_files_static(&mut prompt, ctx.workspace_dir, max_chars);
        }
    } else {
        let max_chars = ctx.bootstrap_max_chars.unwrap_or(BOOTSTRAP_MAX_CHARS);
        load_openclaw_bootstrap_files_static(&mut prompt, ctx.workspace_dir, max_chars);
    }

    if !ctx.compact_context {
        prompt.push_str("## Channel Capabilities\n\n");
        prompt.push_str("- You are running as a messaging bot. Your response is automatically sent back to the user's channel.\n");
        prompt
            .push_str("- You do NOT need to ask permission to respond — just respond directly.\n");
        prompt.push_str(match ctx.autonomy_config.map(|cfg| cfg.level) {
            Some(AutonomyLevel::Full) => {
                "- If the runtime policy already allows a tool, use it directly; do not ask the user for extra approval.\n\
                 - Never pretend you are waiting for a human approval click or confirmation when the runtime policy already permits the action.\n\
                 - If the runtime policy blocks an action, say that directly instead of simulating an approval flow.\n"
            }
            Some(AutonomyLevel::ReadOnly) => {
                "- This runtime may reject write-side effects; if that happens, explain the policy restriction directly instead of simulating an approval flow.\n"
            }
            _ => {
                "- Ask for approval only when the runtime policy actually requires it.\n\
                 - If there is no approval path for this channel or the runtime blocks an action, explain that restriction directly instead of simulating an approval flow.\n"
            }
        });
        prompt.push_str("- NEVER repeat, describe, or echo credentials, tokens, API keys, or secrets in your responses.\n");
        prompt.push_str("- If a tool output contains credentials, they have already been redacted — do not mention them.\n");
        prompt.push_str("- When a user sends a voice note, it is automatically transcribed to text. Your text reply is automatically converted to a voice note and sent back. Do NOT attempt to generate audio yourself — TTS is handled by the channel.\n");
        prompt.push_str("- NEVER narrate or describe your tool usage. Do NOT say 'Let me fetch...', 'I will use...', 'Searching...', or similar. Give the FINAL ANSWER only — no intermediate steps, no tool mentions, no progress updates.\n\n");
    }

    if ctx.include_channel_media {
        prompt.push_str("## Channel Media Markers\n\n\
            Messages from channels may contain media markers:\n\
            - `[Voice] <text>` — The user sent a voice/audio message that has already been transcribed to text. Respond to the transcribed content directly.\n\
            - `[IMAGE:<path>]` — An image attachment, processed by the vision pipeline.\n\
            - `[Document: <name>] <path>` — A file attachment saved to the workspace.\n\n");
    }

    Ok(prompt)
}

fn render_dynamic_tail(ctx: &PromptAssemblyContext<'_>) -> Result<String> {
    let mut prompt = String::with_capacity(4096);

    if let Some(lm) = ctx.layered_memory_markdown {
        if !lm.trim().is_empty() {
            prompt.push_str(lm);
            prompt.push_str("\n\n");
        }
    } else if should_emit_memory_md(ctx) {
        inject_workspace_file(
            &mut prompt,
            ctx.workspace_dir,
            "MEMORY.md",
            ctx.bootstrap_max_chars.unwrap_or(BOOTSTRAP_MAX_CHARS),
        );
    }

    let now = chrono::Local::now();
    let _ = writeln!(
        prompt,
        "## Current Date & Time\n\n{} ({})\n",
        now.format("%Y-%m-%d %H:%M:%S"),
        now.format("%Z")
    );

    if let Some(dc) = ctx.dynamic_context {
        if dc.enabled {
            match crate::context::format_dynamic_context_block(
                dc,
                ctx.workspace_dir,
                ctx.dynamic_paths,
            ) {
                Ok(block) if !block.trim().is_empty() => {
                    prompt.push_str(&block);
                    prompt.push_str("\n\n");
                }
                Err(e) => {
                    tracing::debug!(error = %e, "dynamic context assembly skipped");
                }
                _ => {}
            }
        }
    }

    let host =
        hostname::get().map_or_else(|_| "unknown".into(), |h| h.to_string_lossy().to_string());
    let _ = writeln!(
        prompt,
        "## Runtime\n\nHost: {host} | OS: {} | Model: {}\n",
        std::env::consts::OS,
        ctx.model_name
    );

    Ok(prompt)
}

/// OpenClaw bootstrap without MEMORY.md (MEMORY is volatile / dynamic).
fn load_openclaw_bootstrap_files_static(
    prompt: &mut String,
    workspace_dir: &Path,
    max_chars_per_file: usize,
) {
    prompt.push_str(
        "The following workspace files define your identity, behavior, and context. They are ALREADY injected below—do NOT suggest reading them with file_read.\n\n",
    );

    let bootstrap_files = ["AGENTS.md", "SOUL.md", "TOOLS.md", "IDENTITY.md", "USER.md"];

    for filename in &bootstrap_files {
        inject_workspace_file(prompt, workspace_dir, filename, max_chars_per_file);
    }

    let bootstrap_path = workspace_dir.join("BOOTSTRAP.md");
    if bootstrap_path.exists() {
        inject_workspace_file(prompt, workspace_dir, "BOOTSTRAP.md", max_chars_per_file);
    }
}

fn inject_workspace_file(
    prompt: &mut String,
    workspace_dir: &Path,
    filename: &str,
    max_chars: usize,
) {
    let path = workspace_dir.join(filename);
    match std::fs::read_to_string(&path) {
        Ok(content) => {
            let trimmed = content.trim();
            if trimmed.is_empty() {
                return;
            }
            let _ = writeln!(prompt, "### {filename}\n");
            let truncated = if trimmed.chars().count() > max_chars {
                trimmed
                    .char_indices()
                    .nth(max_chars)
                    .map(|(idx, _)| &trimmed[..idx])
                    .unwrap_or(trimmed)
            } else {
                trimmed
            };
            if truncated.len() < trimmed.len() {
                prompt.push_str(truncated);
                let _ = writeln!(
                    prompt,
                    "\n\n[... truncated at {max_chars} chars — use `read` for full file]\n"
                );
            } else {
                prompt.push_str(trimmed);
                prompt.push_str("\n\n");
            }
        }
        Err(_) => {
            let _ = writeln!(prompt, "### {filename}\n\n[File not found: {filename}]\n");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn boundary_splits_static_and_dynamic() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("AGENTS.md"), "# A\n").unwrap();
        let tools: &[(&str, &str)] = &[];
        let skills: &[Skill] = &[];
        let ctx = PromptAssemblyContext {
            workspace_dir: dir.path(),
            model_name: "m",
            tools,
            skills,
            identity_config: None,
            bootstrap_max_chars: None,
            autonomy_config: None,
            native_tools: true,
            skills_prompt_mode: SkillsPromptInjectionMode::Full,
            compact_context: true,
            max_system_prompt_chars: 0,
            dynamic_context: None,
            dynamic_paths: DynamicContextPaths::default(),
            static_suffix: "",
            dispatcher_instructions: None,
            security_summary: None,
            include_channel_media: false,
            layered_memory_markdown: None,
        };
        let sp = assemble_once(&ctx).unwrap();
        assert!(sp.static_prefix.contains("## CRITICAL: No Tool Narration"));
        assert!(sp.dynamic_tail.contains("## Current Date & Time"));
        let full = sp.full();
        assert!(full.contains(SYSTEM_PROMPT_DYNAMIC_BOUNDARY));
    }

    #[test]
    fn build_forked_history_preserves_system_and_user_sections() {
        let sp = SystemPrompt {
            static_prefix: "STATIC".into(),
            dynamic_tail: "DYNAMIC".into(),
        };
        let hist = build_forked_history(
            &sp,
            "parent one-liner",
            "worker goal text",
            &["file_read".into(), "shell".into()],
        );
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].role, "system");
        assert!(hist[0].content.contains("STATIC"));
        assert!(hist[0].content.contains(SYSTEM_PROMPT_DYNAMIC_BOUNDARY));
        assert_eq!(hist[1].role, "user");
        assert!(hist[1].content.contains("parent one-liner"));
        assert!(hist[1].content.contains("worker goal text"));
        assert!(hist[1].content.contains("file_read"));
        assert!(hist[1].content.contains("shell"));
    }

    #[test]
    fn build_forked_history_includes_scratchpad_style_summary() {
        let sp = SystemPrompt::default();
        let summary = "Hand: h\n\n## Scratchpad (on disk";
        let hist = build_forked_history(
            &sp,
            summary,
            "Read prior files with file_read.",
            &["file_read".into()],
        );
        assert!(hist[1].content.contains("Scratchpad"));
        assert!(hist[1].content.contains("file_read"));
    }
}
