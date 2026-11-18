//! Write audit trail: who changed what, and when.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::types::AdminError;

/// One recorded write action.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEntry {
    pub timestamp: String,
    pub user: String,
    pub action: String,
    pub resource: String,
    pub record_id: String,
}

/// Destination for the audit trail.
#[async_trait]
pub trait AuditStore: Send + Sync + 'static {
    async fn record(&self, entry: AuditEntry) -> Result<(), AdminError>;
    /// Most recent entries first.
    async fn recent(&self, limit: usize) -> Result<Vec<AuditEntry>, AdminError>;
}

/// Drops every entry. Used when the site has no audit store configured.
pub struct NullAudit;

#[async_trait]
impl AuditStore for NullAudit {
    async fn record(&self, _entry: AuditEntry) -> Result<(), AdminError> {
        Ok(())
    }

    async fn recent(&self, _limit: usize) -> Result<Vec<AuditEntry>, AdminError> {
        Ok(Vec::new())
    }
}

/// Appends entries to a JSON file, mirroring [`JsonUserStore`](crate::auth::JsonUserStore).
pub struct JsonAuditLog {
    path: PathBuf,
    entries: Mutex<Vec<AuditEntry>>,
}

impl JsonAuditLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let entries = if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|data| serde_json::from_str(&data).ok())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        Self {
            path,
            entries: Mutex::new(entries),
        }
    }

    fn save(&self, entries: &[AuditEntry]) -> Result<(), AdminError> {
        let data = serde_json::to_string_pretty(entries)
            .map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        std::fs::write(&self.path, data).map_err(|e| AdminError::DatabaseError(e.to_string()))
    }
}

#[async_trait]
impl AuditStore for JsonAuditLog {
    async fn record(&self, entry: AuditEntry) -> Result<(), AdminError> {
        let snapshot = {
            let mut entries = self
                .entries
                .lock()
                .map_err(|e| AdminError::DatabaseError(e.to_string()))?;
            entries.push(entry);
            entries.clone()
        };
        self.save(&snapshot)
    }

    async fn recent(&self, limit: usize) -> Result<Vec<AuditEntry>, AdminError> {
        let entries = self
            .entries
            .lock()
            .map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        Ok(entries.iter().rev().take(limit).cloned().collect())
    }
}

/// Shared handle registered as request data.
pub type SharedAudit = Arc<dyn AuditStore>;

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(user: &str, action: &str) -> AuditEntry {
        AuditEntry {
            timestamp: "2026-01-01T00:00:00Z".into(),
            user: user.into(),
            action: action.into(),
            resource: "products".into(),
            record_id: "7".into(),
        }
    }

    #[tokio::test]
    async fn records_and_lists_newest_first() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_audit_log.json");
        let _ = std::fs::remove_file(&path);

        let log = JsonAuditLog::new(&path);
        log.record(entry("ana", "create")).await.unwrap();
        log.record(entry("ana", "delete")).await.unwrap();

        let recent = log.recent(10).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].action, "delete");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn entries_survive_a_restart() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_audit_persist.json");
        let _ = std::fs::remove_file(&path);

        {
            let log = JsonAuditLog::new(&path);
            log.record(entry("root", "update")).await.unwrap();
        }
        let reopened = JsonAuditLog::new(&path);
        assert_eq!(reopened.recent(1).await.unwrap()[0].user, "root");

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn limit_is_respected() {
        let dir = std::env::temp_dir();
        let path = dir.join("test_audit_limit.json");
        let _ = std::fs::remove_file(&path);

        let log = JsonAuditLog::new(&path);
        for _ in 0..5 {
            log.record(entry("ana", "create")).await.unwrap();
        }
        assert_eq!(log.recent(2).await.unwrap().len(), 2);

        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn null_audit_stores_nothing() {
        let audit = NullAudit;
        audit.record(entry("x", "create")).await.unwrap();
        assert!(audit.recent(10).await.unwrap().is_empty());
    }
}
