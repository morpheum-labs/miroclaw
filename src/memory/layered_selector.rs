//! Pre-LLM selective recall: keyword (+ optional embedding) over AutoMemory topics + latest SessionMemory.

use super::layered_paths::{auto_memory_index_path, auto_memory_topics_dir};
use super::session_memory;
use crate::config::{EmbeddingRouteConfig, LayeredMemoryConfig, MemoryConfig};
use std::fmt::Write;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryLayer {
    Auto,
    Session,
}

#[derive(Debug, Clone, Default)]
pub struct LayeredMemorySelectionResult {
    pub text: String,
    pub topics_picked: usize,
    pub session_injected: bool,
    pub staleness_warnings: usize,
}

fn tokenize_query(q: &str) -> Vec<String> {
    q.to_ascii_lowercase()
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|t| !t.is_empty() && t.len() > 1)
        .map(str::to_string)
        .collect()
}

fn keyword_score(text: &str, tokens: &[String]) -> f32 {
    let lower = text.to_ascii_lowercase();
    let mut s = 0f32;
    for t in tokens {
        if lower.contains(t.as_str()) {
            s += 1.0;
        }
    }
    s
}

struct TopicCandidate {
    path: std::path::PathBuf,
    title: String,
    tags: String,
    confidence: f32,
    last_updated: Option<chrono::NaiveDate>,
    body_snip: String,
}

fn parse_frontmatter_block(raw: &str) -> (String, String) {
    let t = raw.trim();
    if let Some(rest) = t.strip_prefix("---") {
        if let Some(end) = rest.find("\n---") {
            let fm = &rest[..end];
            let body = rest[end + 4..].trim();
            return (fm.to_string(), body.to_string());
        }
    }
    (String::new(), t.to_string())
}

fn yaml_get_str(fm: &str, key: &str) -> Option<String> {
    for line in fm.lines() {
        let line = line.trim();
        let prefix = format!("{key}:");
        if let Some(v) = line.strip_prefix(&prefix) {
            let v = v.trim().trim_matches('"').trim();
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn yaml_get_f32(fm: &str, key: &str) -> f32 {
    yaml_get_str(fm, key)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.5)
}

fn yaml_get_tags_line(fm: &str) -> String {
    yaml_get_str(fm, "tags").unwrap_or_default()
}

fn parse_date(s: &str) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok()
}

async fn load_topic_candidates(topics_dir: &Path) -> Vec<TopicCandidate> {
    let mut out = Vec::new();
    let mut rd = match tokio::fs::read_dir(topics_dir).await {
        Ok(r) => r,
        Err(_) => return out,
    };
    while let Ok(Some(e)) = rd.next_entry().await {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("md") {
            continue;
        }
        let raw = match tokio::fs::read_to_string(&p).await {
            Ok(s) => s,
            Err(_) => continue,
        };
        let (fm, body) = parse_frontmatter_block(&raw);
        let title = yaml_get_str(&fm, "title").unwrap_or_else(|| {
            p.file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("topic")
                .to_string()
        });
        let lu = yaml_get_str(&fm, "last_updated").and_then(|s| parse_date(&s));
        let conf = yaml_get_f32(&fm, "confidence");
        let tags = yaml_get_tags_line(&fm);
        let body_snip: String = body.chars().take(900).collect();
        out.push(TopicCandidate {
            path: p,
            title,
            tags,
            confidence: conf,
            last_updated: lu,
            body_snip,
        });
    }
    out
}

fn days_since_update(d: Option<chrono::NaiveDate>) -> i64 {
    let Some(d) = d else {
        return 9999;
    };
    let today = chrono::Utc::now().date_naive();
    today.signed_duration_since(d).num_days()
}

fn cosine_sim(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom == 0.0 {
        return 0.0;
    }
    dot / denom
}

/// Build the markdown block injected into the system prompt dynamic tail.
pub async fn select_relevant(
    query: &str,
    workspace_dir: &Path,
    session_key: &str,
    layered: &LayeredMemoryConfig,
    memory: &MemoryConfig,
    embedding_api_key: Option<&str>,
    embedding_routes: &[EmbeddingRouteConfig],
) -> LayeredMemorySelectionResult {
    let mut r = LayeredMemorySelectionResult::default();
    if !layered.enabled {
        return r;
    }
    let tokens = tokenize_query(query);
    let topics_dir = auto_memory_topics_dir(workspace_dir);
    let cands = load_topic_candidates(&topics_dir).await;

    let resolved = super::resolve_memory_embedding(memory, embedding_routes, embedding_api_key);
    let embedder: Arc<dyn crate::memory::embeddings::EmbeddingProvider> =
        Arc::from(crate::memory::embeddings::create_embedding_provider(
            resolved.provider.trim(),
            resolved.api_key.as_deref(),
            resolved.model.trim(),
            resolved.dimensions,
            resolved.embedding_api_url.as_deref(),
        ));

    let q_vec = if embedder.dimensions() > 0 {
        embedder.embed_one(query).await.ok()
    } else {
        None
    };

    let mut scored: Vec<(f32, TopicCandidate)> = Vec::new();
    for c in cands {
        let mut sc = keyword_score(&format!("{} {} {}", c.title, c.tags, c.body_snip), &tokens);
        let norm_kw = (sc / (tokens.len().max(1) as f32)).min(1.0);
        if let Some(ref qv) = q_vec {
            if let Ok(tv) = embedder.embed_one(&c.title).await {
                if tv.len() == qv.len() && !tv.is_empty() {
                    let sim = cosine_sim(qv, &tv).max(0.0) as f32;
                    #[allow(clippy::cast_possible_truncation)]
                    let vw = memory.vector_weight as f32;
                    #[allow(clippy::cast_possible_truncation)]
                    let kw = memory.keyword_weight as f32;
                    sc = norm_kw * kw + sim * vw;
                } else {
                    sc = norm_kw;
                }
            } else {
                sc = norm_kw;
            }
        } else {
            sc = norm_kw;
        }
        let age = days_since_update(c.last_updated).min(3650);
        let recency = (1.0 + (365.0 - age as f32).max(0.0) / 3650.0).min(1.1);
        sc = sc * recency + c.confidence * 0.15;
        scored.push((sc, c));
    }
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let max_t = layered.max_topics_in_prompt.clamp(1, 12);
    let staleness_days = layered.staleness_warn_days.max(1);
    let mut picked = 0usize;
    let mut warn = 0usize;
    let mut block = String::from("## Layered memory (Auto + Session)\n\n");

    let idx_path = auto_memory_index_path(workspace_dir);
    if tokio::fs::try_exists(&idx_path).await.unwrap_or(false) {
        if let Ok(ix) = tokio::fs::read_to_string(&idx_path).await {
            let excerpt: String = ix.lines().take(48).collect::<Vec<_>>().join("\n");
            if !excerpt.trim().is_empty() {
                let _ = writeln!(block, "### AutoMemory index (excerpt)\n\n{excerpt}\n");
            }
        }
    }

    for (_, c) in scored.iter().take(max_t) {
        let age_days = days_since_update(c.last_updated);
        let staleness_line = if age_days >= i64::from(staleness_days) {
            warn += 1;
            format!(
                "\n_Staleness: last_updated ~{age_days} days ago — verify against the workspace if critical._\n"
            )
        } else {
            String::new()
        };
        let rel = c
            .path
            .strip_prefix(auto_memory_bucket_dir(workspace_dir))
            .unwrap_or(c.path.as_path())
            .display();
        let _ = writeln!(
            block,
            "#### {} (`{}`)\n\n{}{}\n",
            c.title, rel, staleness_line, c.body_snip
        );
        picked += 1;
    }

    if let Some(sess) = session_memory::read_latest_summary(workspace_dir, session_key).await {
        if !sess.trim().is_empty() {
            let _ = writeln!(block, "### Latest session memory\n\n{}\n", sess.trim());
            r.session_injected = true;
        }
    }

    let max_chars = layered.max_chars_total.clamp(500, 50_000);
    if block.chars().count() > max_chars {
        let t: String = block.chars().take(max_chars).collect();
        block = format!("{t}\n\n_… [layered memory truncated]_\n");
    }

    r.text = block;
    r.topics_picked = picked;
    r.staleness_warnings = warn;
    tracing::info!(
        topics = picked,
        session = r.session_injected,
        staleness = warn,
        "MemorySelector layered recall"
    );
    r
}

fn auto_memory_bucket_dir(workspace_dir: &Path) -> std::path::PathBuf {
    super::layered_paths::auto_memory_bucket_dir(workspace_dir)
}
