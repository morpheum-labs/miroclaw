//! Persisted revision watermark storage.

use std::path::{Path, PathBuf};

use async_trait::async_trait;

use crate::error::ClawgotchaError;
use crate::models::domain::RevisionSummary;
use crate::traits::RevisionStore;

/// JSON file under the workspace: `.miroclaw/clawgotcha/revisions.json` (path supplied by host).
pub struct FileRevisionStore {
    path: PathBuf,
}

impl FileRevisionStore {
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn parent(&self) -> Option<&Path> {
        self.path.parent()
    }
}

#[async_trait]
impl RevisionStore for FileRevisionStore {
    async fn load(&self) -> Result<RevisionSummary, ClawgotchaError> {
        if !self.path.exists() {
            return Ok(RevisionSummary::default());
        }
        let bytes = tokio::fs::read(&self.path).await?;
        let v: RevisionSummary = serde_json::from_slice(&bytes)
            .map_err(|e| ClawgotchaError::Validation(e.to_string()))?;
        Ok(v)
    }

    async fn save(&self, summary: &RevisionSummary) -> Result<(), ClawgotchaError> {
        if let Some(dir) = self.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let data = serde_json::to_vec_pretty(summary)?;
        tokio::fs::write(&self.path, data).await?;
        Ok(())
    }
}
