//! Shared models: domain entities and wire DTOs.

pub mod domain;
pub mod wire;

pub use domain::{
    AgentDefinition, ClawgotchaInstance, CronJobDefinition, OfflineSnapshot, RevisionSummary,
    SwarmDefaults, ToolMetadata,
};
