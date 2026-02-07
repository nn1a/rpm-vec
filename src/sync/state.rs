use crate::error::Result;
use crate::sync::config::{RepoSyncState, SyncStatus};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use tracing::{debug, info};

/// Manages repository sync state in the database
pub struct SyncStateStore {
    conn: Connection,
}

impl SyncStateStore {
    pub fn new(conn: Connection) -> Result<Self> {
        let store = Self { conn };
        store.create_schema()?;
        Ok(store)
    }

    /// Create sync_state table if it doesn't exist
    fn create_schema(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS repo_sync_state (
                repo_name TEXT PRIMARY KEY,
                last_sync TEXT,
                last_checksum TEXT,
                last_status TEXT NOT NULL,
                last_error TEXT
            )",
            [],
        )?;

        debug!("Sync state schema created or verified");
        Ok(())
    }

    /// Get sync state for a repository
    pub fn get_state(&self, repo_name: &str) -> Result<Option<RepoSyncState>> {
        let mut stmt = self.conn.prepare(
            "SELECT repo_name, last_sync, last_checksum, last_status, last_error 
             FROM repo_sync_state 
             WHERE repo_name = ?",
        )?;

        let result = stmt.query_row([repo_name], |row| {
            let last_sync_str: Option<String> = row.get(1)?;
            let last_sync = last_sync_str
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|dt| dt.with_timezone(&Utc));

            let last_status_str: String = row.get(3)?;
            let last_status = match last_status_str.as_str() {
                "never" => SyncStatus::Never,
                "success" => SyncStatus::Success,
                "failed" => SyncStatus::Failed,
                "in-progress" => SyncStatus::InProgress,
                _ => SyncStatus::Never,
            };

            Ok(RepoSyncState {
                repo_name: row.get(0)?,
                last_sync,
                last_checksum: row.get(2)?,
                last_status,
                last_error: row.get(4)?,
            })
        });

        match result {
            Ok(state) => Ok(Some(state)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Update or insert sync state
    pub fn update_state(&self, state: &RepoSyncState) -> Result<()> {
        let last_sync_str = state.last_sync.map(|dt| dt.to_rfc3339());

        self.conn.execute(
            "INSERT OR REPLACE INTO repo_sync_state 
             (repo_name, last_sync, last_checksum, last_status, last_error) 
             VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![
                &state.repo_name,
                last_sync_str,
                &state.last_checksum,
                state.last_status.to_string(),
                &state.last_error,
            ],
        )?;

        info!(
            repo = %state.repo_name,
            status = %state.last_status,
            "Updated sync state"
        );
        Ok(())
    }

    /// List all repository sync states
    pub fn list_states(&self) -> Result<Vec<RepoSyncState>> {
        let mut stmt = self.conn.prepare(
            "SELECT repo_name, last_sync, last_checksum, last_status, last_error 
             FROM repo_sync_state 
             ORDER BY repo_name",
        )?;

        let states = stmt
            .query_map([], |row| {
                let last_sync_str: Option<String> = row.get(1)?;
                let last_sync = last_sync_str
                    .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                    .map(|dt| dt.with_timezone(&Utc));

                let last_status_str: String = row.get(3)?;
                let last_status = match last_status_str.as_str() {
                    "never" => SyncStatus::Never,
                    "success" => SyncStatus::Success,
                    "failed" => SyncStatus::Failed,
                    "in-progress" => SyncStatus::InProgress,
                    _ => SyncStatus::Never,
                };

                Ok(RepoSyncState {
                    repo_name: row.get(0)?,
                    last_sync,
                    last_checksum: row.get(2)?,
                    last_status,
                    last_error: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(states)
    }

    /// Delete sync state for a repository
    /// Delete sync state for a repository (not currently used)
    #[allow(dead_code)]
    pub fn delete_state(&self, repo_name: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM repo_sync_state WHERE repo_name = ?",
            [repo_name],
        )?;

        info!(repo = %repo_name, "Deleted sync state");
        Ok(())
    }
}
