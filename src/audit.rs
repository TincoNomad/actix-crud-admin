//! Write audit trail: who changed what, and when.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

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

/// Appends one JSON object per line (NDJSON).
///
/// Appending keeps writes O(1): the file is never rewritten. It grows forever,
/// so rotate it with `logrotate` or similar. `recent` reads the whole file, which
/// is fine for a report page but not for millions of lines.
pub struct JsonAuditLog {
    path: PathBuf,
}

impl JsonAuditLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Old versions stored a single JSON array. Rewrite it as NDJSON once,
    /// before the first append, so the file stays parseable.
    fn migrate_legacy(&self) {
        let Ok(mut file) = std::fs::File::open(&self.path) else {
            return;
        };
        let mut prefix = [0u8; 64];
        let Ok(read) = file.read(&mut prefix) else {
            return;
        };
        let starts_with_array = prefix[..read]
            .iter()
            .find(|byte| !byte.is_ascii_whitespace())
            == Some(&b'[');
        if !starts_with_array {
            return;
        }

        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return;
        };
        let entries: Vec<AuditEntry> = serde_json::from_str(&text).unwrap_or_default();
        let lines: String = entries
            .iter()
            .filter_map(|entry| serde_json::to_string(entry).ok())
            .map(|line| line + "\n")
            .collect();
        let _ = std::fs::write(&self.path, lines);
    }
}

#[async_trait]
impl AuditStore for JsonAuditLog {
    async fn record(&self, entry: AuditEntry) -> Result<(), AdminError> {
        self.migrate_legacy();
        let mut line =
            serde_json::to_string(&entry).map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        line.push('\n');
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        file.write_all(line.as_bytes())
            .map_err(|e| AdminError::DatabaseError(e.to_string()))?;
        Ok(())
    }

    async fn recent(&self, limit: usize) -> Result<Vec<AuditEntry>, AdminError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(AdminError::DatabaseError(e.to_string())),
        };

        // Tolerate a legacy array that still has not been migrated.
        if text.trim_start().starts_with('[') {
            let entries: Vec<AuditEntry> = serde_json::from_str(&text).unwrap_or_default();
            return Ok(entries.into_iter().rev().take(limit).collect());
        }

        let mut entries: Vec<AuditEntry> = text
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        entries.reverse();
        entries.truncate(limit);
        Ok(entries)
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

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(name);
        let _ = std::fs::remove_file(&path);
        path
    }

    #[tokio::test]
    async fn records_and_lists_newest_first() {
        let path = temp("test_audit_log.ndjson");
        let log = JsonAuditLog::new(&path);
        log.record(entry("ana", "create")).await.unwrap();
        log.record(entry("ana", "delete")).await.unwrap();

        let recent = log.recent(10).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].action, "delete");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn appends_one_line_per_entry() {
        let path = temp("test_audit_append.ndjson");
        let log = JsonAuditLog::new(&path);
        for _ in 0..3 {
            log.record(entry("ana", "create")).await.unwrap();
        }
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 3, "one line per entry");
        assert!(!text.trim_start().starts_with('['), "not a JSON array");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn entries_survive_a_restart() {
        let path = temp("test_audit_persist.ndjson");
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
        let path = temp("test_audit_limit.ndjson");
        let log = JsonAuditLog::new(&path);
        for _ in 0..5 {
            log.record(entry("ana", "create")).await.unwrap();
        }
        assert_eq!(log.recent(2).await.unwrap().len(), 2);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn missing_file_is_empty() {
        let path = temp("test_audit_missing.ndjson");
        let log = JsonAuditLog::new(&path);
        assert!(log.recent(10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn legacy_array_is_migrated_on_write() {
        let path = temp("test_audit_legacy.json");
        let legacy = vec![entry("old", "create"), entry("old", "update")];
        std::fs::write(&path, serde_json::to_string_pretty(&legacy).unwrap()).unwrap();

        let log = JsonAuditLog::new(&path);
        assert_eq!(
            log.recent(10).await.unwrap().len(),
            2,
            "reads the legacy array"
        );

        log.record(entry("new", "delete")).await.unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.trim_start().starts_with('['), "migrated to NDJSON");

        let recent = log.recent(10).await.unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].user, "new");
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn malformed_lines_are_skipped() {
        let path = temp("test_audit_malformed.ndjson");
        let mut text = serde_json::to_string(&entry("ana", "create")).unwrap();
        text.push_str("\n{\"broken\": \n");
        std::fs::write(&path, text).unwrap();

        let log = JsonAuditLog::new(&path);
        assert_eq!(log.recent(10).await.unwrap().len(), 1);
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn null_audit_stores_nothing() {
        let audit = NullAudit;
        audit.record(entry("x", "create")).await.unwrap();
        assert!(audit.recent(10).await.unwrap().is_empty());
    }
}
