//! Optional offline snapshot persistence.

use std::path::PathBuf;

use async_trait::async_trait;

use crate::error::ClawgotchaError;
use crate::models::domain::OfflineSnapshot;
use crate::traits::OfflineCache;

pub struct FileOfflineCache {
    path: PathBuf,
}

impl FileOfflineCache {
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
}

#[async_trait]
impl OfflineCache for FileOfflineCache {
    async fn load(&self) -> Result<Option<OfflineSnapshot>, ClawgotchaError> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = tokio::fs::read(&self.path).await?;
        let snap: OfflineSnapshot = serde_json::from_slice(&bytes)
            .map_err(|e| ClawgotchaError::Validation(format!("offline snapshot: {e}")))?;
        Ok(Some(snap))
    }

    async fn save(&self, snapshot: &OfflineSnapshot) -> Result<(), ClawgotchaError> {
        if let Some(dir) = self.path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let data = serde_json::to_vec_pretty(snapshot)?;
        tokio::fs::write(&self.path, data).await?;
        Ok(())
    }
}
