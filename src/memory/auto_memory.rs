//! Curated AutoMemory: topic files + capped `MEMORY.md` index under `~/.miroclaw/memory/<bucket>/`.

use super::layered_paths::{auto_memory_index_path, auto_memory_topics_dir};
use std::path::Path;

/// Append a topic file and a short index line. Prunes the index body to `max_lines` non-empty lines.
pub async fn append_high_confidence_fact(
    workspace_dir: &Path,
    title: &str,
    fact_body: &str,
    confidence: f32,
    index_max_lines: usize,
) -> anyhow::Result<()> {
    let topics = auto_memory_topics_dir(workspace_dir);
    tokio::fs::create_dir_all(&topics).await?;
    let slug: String = title
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else if c.is_whitespace() {
                '-'
            } else {
                '_'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').trim_matches('_');
    let head: String = if slug.is_empty() {
        "topic".into()
    } else {
        slug.chars().take(48).collect()
    };
    let id = uuid::Uuid::new_v4();
    let filename = format!("{head}-{id}.md");
    let path = topics.join(&filename);
    let now = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let safe_title = title.trim();
    let safe_body = fact_body.trim();
    let doc = format!(
        "---\ntitle: \"{}\"\nlast_updated: {}\ntags: [consolidated]\nconfidence: {}\n---\n\n{}\n",
        escape_yaml_string(safe_title),
        now,
        confidence,
        safe_body
    );
    tokio::fs::write(&path, doc).await?;

    let rel = format!("topics/{filename}");
    let index_line = format!(
        "- [{}]({}) | conf {:.2} | updated {}\n",
        safe_title, rel, confidence, now
    );
    append_index_line(workspace_dir, &index_line, index_max_lines).await?;
    Ok(())
}

fn escape_yaml_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

async fn append_index_line(
    workspace_dir: &Path,
    line: &str,
    index_max_lines: usize,
) -> anyhow::Result<()> {
    let idx = auto_memory_index_path(workspace_dir);
    if let Some(parent) = idx.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let mut body = if idx.exists() {
        tokio::fs::read_to_string(&idx).await?
    } else {
        "# Auto memory index\n\nCurated long-lived facts (layered memory). Full bodies live under `topics/`.\n\n".to_string()
    };
    body.push_str(line);
    prune_index_entries(&mut body, index_max_lines);
    tokio::fs::write(&idx, body).await?;
    Ok(())
}

/// Keep at most `max_entries` markdown list lines that start with `- [` (index rows).
fn prune_index_entries(body: &mut String, max_entries: usize) {
    if max_entries == 0 {
        return;
    }
    let lines: Vec<String> = body.lines().map(|l| l.to_string()).collect();
    let mut preamble: Vec<String> = Vec::new();
    let mut entries: Vec<String> = Vec::new();
    for line in lines {
        if line.trim_start().starts_with("- [") {
            entries.push(line);
        } else {
            preamble.push(line);
        }
    }
    while entries.len() > max_entries {
        entries.remove(0);
    }
    let mut all = preamble;
    all.append(&mut entries);
    *body = all.join("\n");
    if !body.ends_with('\n') {
        body.push('\n');
    }
}

#[cfg(test)]
#[test]
fn prune_keeps_only_newest_index_entries() {
    let mut body = "# Index\n\n- [a](t/a.md)\n- [b](t/b.md)\n- [c](t/c.md)\n".to_string();
    prune_index_entries(&mut body, 2);
    assert_eq!(body.matches("- [").count(), 2);
    assert!(!body.contains("[a]"));
}
