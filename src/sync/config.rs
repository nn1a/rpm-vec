use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Sync configuration for a repository
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSyncConfig {
    /// Repository name (used as identifier in database)
    pub name: String,

    /// Base URL of the repository (e.g., http://download.tizen.org/snapshots/TIZEN/Tizen/Tizen-Base/reference/repos/standard/packages/)
    pub base_url: String,

    /// Sync interval in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_interval")]
    pub interval_seconds: u64,

    /// Whether this repository is enabled for syncing
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_interval() -> u64 {
    3600 // 1 hour
}

fn default_enabled() -> bool {
    true
}

/// Global sync configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// List of repositories to sync
    pub repositories: Vec<RepoSyncConfig>,

    /// Working directory for downloaded metadata
    #[serde(default = "default_work_dir")]
    pub work_dir: PathBuf,
}

fn default_work_dir() -> PathBuf {
    PathBuf::from(".rpm-sync")
}

impl SyncConfig {
    /// Load sync configuration from TOML file
    pub fn from_file(path: &std::path::Path) -> crate::error::Result<Self> {
        let content = std::fs::read_to_string(path).map_err(crate::error::RpmSearchError::Io)?;

        let config: SyncConfig = toml::from_str(&content).map_err(|e| {
            crate::error::RpmSearchError::Config(format!("Invalid sync config: {}", e))
        })?;

        Ok(config)
    }

    /// Save sync configuration to TOML file
    pub fn to_file(&self, path: &std::path::Path) -> crate::error::Result<()> {
        let content = toml::to_string_pretty(self).map_err(|e| {
            crate::error::RpmSearchError::Config(format!("Failed to serialize config: {}", e))
        })?;

        std::fs::write(path, content).map_err(crate::error::RpmSearchError::Io)?;

        Ok(())
    }

    /// Generate example configuration
    pub fn example() -> Self {
        Self {
            repositories: vec![
                RepoSyncConfig {
                    name: "tizen-unified".to_string(),
                    base_url: "http://download.tizen.org/snapshots/TIZEN/Tizen/Tizen-Base/reference/repos/standard/packages/".to_string(),
                    interval_seconds: 3600,
                    enabled: true,
                },
                RepoSyncConfig {
                    name: "tizen-base".to_string(),
                    base_url: "http://download.tizen.org/snapshots/TIZEN/Tizen/Tizen-Base/reference/repos/standard/packages/"
                        .to_string(),
                    interval_seconds: 3600,
                    enabled: true,
                },
            ],
            work_dir: default_work_dir(),
        }
    }
}

/// Repository sync state (tracked in database)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSyncState {
    pub repo_name: String,
    pub last_sync: Option<DateTime<Utc>>,
    pub last_checksum: Option<String>,
    pub last_status: SyncStatus,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncStatus {
    Never,
    Success,
    Failed,
    InProgress,
}

impl std::fmt::Display for SyncStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncStatus::Never => write!(f, "never"),
            SyncStatus::Success => write!(f, "success"),
            SyncStatus::Failed => write!(f, "failed"),
            SyncStatus::InProgress => write!(f, "in-progress"),
        }
    }
}
