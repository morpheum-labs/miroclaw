#[allow(clippy::module_inception)]
pub mod agent;
pub mod classifier;
pub mod compaction_pipeline;
pub mod context_analyzer;
pub mod dispatcher;
pub mod eval;
pub mod history_pruner;
pub mod loop_;
pub mod loop_detector;
pub mod memory_loader;
pub mod prompt;
pub mod query_engine;
pub mod session_record;
pub mod session_transcript;
pub mod state;
pub mod stop_hooks;
pub mod system_prompt;
pub mod thinking;
pub mod tool_result_offload;
pub mod tool_router;

#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use agent::{Agent, AgentBuilder, TurnEvent, TurnEventSink, TurnStreamedOutput};
#[allow(unused_imports)]
pub use loop_::{process_message, run};
