//! Live grok-browser provider test using claw-bun-mcp example prompt.
//!
//! Requires bun-browser daemon + logged-in grok.com session.
//! Run: `cargo test --test live grok_browser_live_test1 -- --ignored --nocapture`

use std::sync::Arc;
use std::time::Duration;
use zeroclaw::config::GrokBrowserConfig;
use zeroclaw::providers::grok_browser::scheduler::JobStatusKind;
use zeroclaw::providers::grok_browser::GrokBrowserProvider;
use zeroclaw::providers::traits::{ChatMessage, ChatRequest, Provider};
use zeroclaw::providers::ProviderRuntimeOptions;

const TEST1_PATH: &str = "/Users/hesdx/Documents/clawhost/claw-bun-mcp/grok/example/test1.txt";

fn bun_browser_ready() -> bool {
    std::env::var("BUN_BROWSER_TOKEN")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .is_some()
        || directories::UserDirs::new()
            .map(|d| d.home_dir().join(".bun-browser/daemon.json"))
            .is_some_and(|p| p.is_file())
}

#[tokio::test]
#[ignore = "requires live bun-browser daemon and grok.com login"]
async fn grok_browser_live_test1() {
    if !bun_browser_ready() {
        eprintln!("skip: no BUN_BROWSER_TOKEN and no ~/.bun-browser/daemon.json");
        return;
    }

    let prompt =
        std::fs::read_to_string(TEST1_PATH).unwrap_or_else(|e| panic!("read {TEST1_PATH}: {e}"));
    assert!(
        prompt.contains("financial news temporal analyst"),
        "unexpected test1.txt content"
    );

    let options = ProviderRuntimeOptions {
        grok_browser: GrokBrowserConfig {
            disable_search: false,
            model: Some("fast".into()),
            max_parallel_tabs: 2,
            ..GrokBrowserConfig::default()
        },
        ..ProviderRuntimeOptions::default()
    };

    let provider = GrokBrowserProvider::new(&options).expect("create grok browser provider");
    provider.warmup().await.expect("warmup grok/modes + agents");

    let session_key = "live:test1";
    let registry = provider.job_registry();

    let poll_handle = {
        let registry = Arc::clone(&registry);
        tokio::spawn(async move {
            let mut last_status = String::new();
            for _ in 0..120 {
                tokio::time::sleep(Duration::from_secs(5)).await;
                // Poll latest jobs is not indexed; this test mainly verifies registry exists.
                let _ = registry;
                if last_status.is_empty() {
                    eprintln!("[poll] registry ready");
                    last_status = "ready".into();
                }
            }
        })
    };

    let messages = vec![ChatMessage::user(&prompt)];
    let request = ChatRequest {
        messages: &messages,
        tools: None,
        session_key: Some(session_key),
    };

    eprintln!("[test] submitting test1.txt via grok-browser provider (model=fast)...");
    let started = std::time::Instant::now();
    let response = provider
        .chat(request, "fast", 0.7)
        .await
        .expect("provider chat with test1.txt");
    eprintln!(
        "[test] first turn completed in {:.1}s",
        started.elapsed().as_secs_f64()
    );

    let text = response.text.unwrap_or_default();
    assert!(!text.trim().is_empty(), "empty response from grok browser");
    eprintln!(
        "[test] response preview: {}...",
        text.chars().take(240).collect::<String>()
    );

    let session = provider
        .sessions()
        .get(session_key)
        .await
        .expect("follow-mode session sidecar should exist after first turn");
    assert!(
        !session.conversation_id.is_empty(),
        "missing conversation_id in session sidecar"
    );
    eprintln!(
        "[test] session conversation_id={} tab_id={:?}",
        session.conversation_id, session.tab_id
    );

    // Second turn: short follow-up on same session/tab.
    let follow_messages = vec![
        ChatMessage::user(&prompt),
        ChatMessage::assistant(&text),
        ChatMessage::user(
            "Reply with ONLY the novelty_type field value from your JSON, nothing else.",
        ),
    ];
    let follow_request = ChatRequest {
        messages: &follow_messages,
        tools: None,
        session_key: Some(session_key),
    };

    eprintln!("[test] follow-up on pinned session/tab...");
    let follow = provider
        .chat(follow_request, "fast", 0.7)
        .await
        .expect("follow-up chat");
    let follow_text = follow.text.unwrap_or_default();
    assert!(!follow_text.trim().is_empty(), "empty follow-up response");
    eprintln!("[test] follow-up response: {}", follow_text.trim());

    let session_after = provider.sessions().get(session_key).await.unwrap();
    if let Some(tab_before) = session.tab_id.clone() {
        assert_eq!(
            session_after.tab_id.as_deref(),
            Some(tab_before.as_str()),
            "follow mode should reuse the same grok.com tab"
        );
    }

    poll_handle.abort();

    // Sanity: registry should have at least one completed job snapshot if polled by id
    // (request_id is internal to submit; we only verify registry is wired).
    let _ = JobStatusKind::Completed;
}
