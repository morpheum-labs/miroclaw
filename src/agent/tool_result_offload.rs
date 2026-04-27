//! Write oversized tool outputs to disk and return a preview + path for the LLM.

use crate::config::ToolResultOffloadConfig;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

fn offload_dir() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|b| {
        crate::context::preferred_user_data_dir_in_home(b.home_dir())
            .join("temp")
            .join("tool-results")
    })
}

fn safe_tool_slug(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let s = s.trim_matches('_');
    if s.is_empty() {
        "tool".to_string()
    } else {
        s.chars().take(64).collect()
    }
}

/// When enabled and `output` exceeds `threshold_chars` (Unicode scalar count), writes the full
/// output to `~/.miroclaw/temp/tool-results/` and returns a short preview plus the file path.
/// On I/O failure, falls back to an inline truncation so the agent always gets a response.
#[must_use]
pub fn maybe_offload_output(
    output: &str,
    tool_name: &str,
    cfg: &ToolResultOffloadConfig,
) -> String {
    if !cfg.enabled {
        return output.to_string();
    }
    let n = output.chars().count();
    if n <= cfg.threshold_chars {
        return output.to_string();
    }
    let Some(base) = offload_dir() else {
        tracing::warn!("tool result offload: no home directory; truncating inline");
        return truncate_inline(output, cfg.preview_chars);
    };
    if let Err(e) = fs::create_dir_all(&base) {
        tracing::warn!(error = %e, "tool result offload: mkdir failed; truncating inline");
        return truncate_inline(output, cfg.preview_chars);
    }
    let path = base.join(format!(
        "{}-{}.txt",
        safe_tool_slug(tool_name),
        Uuid::new_v4()
    ));
    match fs::File::create(&path).and_then(|mut f| {
        f.write_all(output.as_bytes())?;
        f.sync_all()?;
        Ok(())
    }) {
        Ok(()) => {
            tracing::debug!(
                path = %path.display(),
                chars = n,
                tool = %tool_name,
                "tool result offloaded to disk"
            );
            let preview: String = output.chars().take(cfg.preview_chars).collect();
            let note = if n > cfg.preview_chars {
                "\n… (preview truncated; full output is in the file above)\n"
            } else {
                "\n"
            };
            format!(
                "[Tool output offloaded: {n} Unicode characters → disk]\n\n\
                 Preview:\n```text\n{preview}\n```{note}\
                 Full path: `{path}`\n",
                preview = preview,
                note = note,
                path = path.display(),
            )
        }
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "tool result offload write failed; truncating inline");
            truncate_inline(output, cfg.preview_chars)
        }
    }
}

fn truncate_inline(s: &str, max_chars: usize) -> String {
    let n = s.chars().count();
    if n <= max_chars {
        return s.to_string();
    }
    let head: String = s.chars().take(max_chars).collect();
    format!("{head}\n… [truncated inline: {n} chars > {max_chars}]\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passthrough_when_disabled() {
        let cfg = ToolResultOffloadConfig {
            enabled: false,
            threshold_chars: 5,
            preview_chars: 3,
        };
        let s = "a".repeat(100);
        assert_eq!(maybe_offload_output(&s, "shell", &cfg), s);
    }

    #[test]
    fn passthrough_when_under_threshold() {
        let cfg = ToolResultOffloadConfig {
            enabled: true,
            threshold_chars: 100,
            preview_chars: 10,
        };
        let s = "hello";
        assert_eq!(maybe_offload_output(s, "shell", &cfg), s);
    }
}
