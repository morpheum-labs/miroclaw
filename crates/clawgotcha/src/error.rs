//! Unified errors for the Clawgotcha integration crate.

use crate::models::wire::WireParseError;

/// Top-level error for HTTP adapters, sync orchestration, and validation.
#[derive(Debug, thiserror::Error)]
pub enum ClawgotchaError {
    #[error("HTTP error: {0}")]
    Http(String),

    #[error("HTTP status {status}: {body}")]
    HttpStatus { status: u16, body: String },

    #[error("serialization: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("invalid response: {0}")]
    InvalidResponse(String),

    #[error("validation: {0}")]
    Validation(String),

    #[error("webhook signature verification failed")]
    WebhookSignature,

    #[error("not modified (304)")]
    NotModified,

    #[error("offline — using cached snapshot")]
    Offline,

    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}

impl From<reqwest::Error> for ClawgotchaError {
    fn from(value: reqwest::Error) -> Self {
        ClawgotchaError::Http(value.to_string())
    }
}

impl From<WireParseError> for ClawgotchaError {
    fn from(value: WireParseError) -> Self {
        ClawgotchaError::Validation(value.to_string())
    }
}
