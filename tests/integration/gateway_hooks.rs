//! Exercises the same post-turn hook surface used by the gateway WebSocket path
//! ([`zeroclaw::agent::Agent::turn_streamed`] + [`HookRunner`]), without standing up HTTP.

use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::support::helpers::{make_memory, make_observer, text_response};
use crate::support::{EchoTool, MockProvider};
use zeroclaw::agent::dispatcher::NativeToolDispatcher;
use zeroclaw::agent::Agent;
use zeroclaw::agent::TurnEventSink;
use zeroclaw::hooks::{HookHandler, HookResult, HookRunner};

struct GatewayTurnRecorder {
    after_turn_void: Arc<AtomicUsize>,
    after_turn_blocking: Arc<AtomicUsize>,
}

#[async_trait]
impl HookHandler for GatewayTurnRecorder {
    fn name(&self) -> &str {
        "gateway-turn-recorder"
    }

    async fn on_after_turn_completed(
        &self,
        channel: &str,
        user_message: &str,
        _assistant_summary: &str,
    ) {
        if channel == "gateway" && user_message.len() >= 20 {
            self.after_turn_void.fetch_add(1, Ordering::SeqCst);
        }
    }

    async fn after_turn_completed_blocking(
        &self,
        channel: &str,
        user_message: &str,
        _assistant_summary: &str,
    ) -> HookResult<()> {
        if channel == "gateway" && user_message.len() >= 20 {
            self.after_turn_blocking.fetch_add(1, Ordering::SeqCst);
        }
        HookResult::Continue(())
    }
}

#[tokio::test]
async fn streamed_agent_turn_runs_post_turn_hooks_like_gateway_ws() {
    let void_count = Arc::new(AtomicUsize::new(0));
    let blocking_count = Arc::new(AtomicUsize::new(0));

    let mut runner = HookRunner::new();
    runner.register(Box::new(GatewayTurnRecorder {
        after_turn_void: Arc::clone(&void_count),
        after_turn_blocking: Arc::clone(&blocking_count),
    }));
    let hooks = Arc::new(runner);

    let provider = Box::new(MockProvider::new(vec![text_response(
        "Consolidation-style assistant summary that is comfortably over twenty characters.",
    )]));

    let mut agent = Agent::builder()
        .provider(provider)
        .tools(vec![Box::new(EchoTool)])
        .memory(make_memory())
        .observer(make_observer())
        .tool_dispatcher(Box::new(NativeToolDispatcher))
        .workspace_dir(std::env::temp_dir())
        .hooks(Some(hooks))
        .build()
        .expect("agent should build");

    let (tx, mut rx) = tokio::sync::mpsc::channel::<TurnEventSink>(16);
    let user = "user message with enough characters for consolidation threshold";
    let out = agent
        .turn_streamed(user, tx)
        .await
        .expect("turn_streamed should succeed");
    assert!(
        !out.answer.is_empty(),
        "turn_streamed should return assistant answer"
    );

    while rx.recv().await.is_some() {}

    assert!(
        void_count.load(Ordering::SeqCst) >= 1,
        "void after-turn hook should run for gateway channel"
    );
    assert!(
        blocking_count.load(Ordering::SeqCst) >= 1,
        "blocking after-turn hook chain should run for gateway channel"
    );
}
