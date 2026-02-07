#[cfg(feature = "sync")]
use chrono::{DateTime, Utc};
#[cfg(feature = "sync")]
use serde::{Deserialize, Serialize};
#[cfg(feature = "sync")]
use std::path::PathBuf;

/// Sync configuration for a repository
#[cfg(feature = "sync")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSyncConfig {
    /// Repository name (used as identifier in database)
    pub name: String,

    /// Base URL of the repository (e.g., https://dl.rockylinux.org/pub/rocky/9/BaseOS/x86_64/os)
    pub base_url: String,

    /// Sync interval in seconds (default: 3600 = 1 hour)
    #[serde(default = "default_interval")]
    pub interval_seconds: u64,

    /// Whether this repository is enabled for syncing
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    /// Architecture filter (optional)
    pub arch: Option<String>,
}

#[cfg(feature = "sync")]
fn default_interval() -> u64 {
    3600 // 1 hour
}

#[cfg(feature = "sync")]
fn default_enabled() -> bool {
    true
}

/// Global sync configuration
#[cfg(feature = "sync")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncConfig {
    /// List of repositories to sync
    pub repositories: Vec<RepoSyncConfig>,

    /// Working directory for downloaded metadata
    #[serde(default = "default_work_dir")]
    pub work_dir: PathBuf,
}

#[cfg(feature = "sync")]
fn default_work_dir() -> PathBuf {
    PathBuf::from(".rpm-sync")
}

#[cfg(feature = "sync")]
impl SyncConfig {
    /// Load sync configuration from TOML file
    pub fn from_file(path: &std::path::Path) -> crate::error::Result<Self> {
        let content =
            std::fs::read_to_string(path).map_err(|e| crate::error::RpmSearchError::Io(e))?;

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

        std::fs::write(path, content).map_err(|e| crate::error::RpmSearchError::Io(e))?;

        Ok(())
    }

    /// Generate example configuration
    pub fn example() -> Self {
        Self {
            repositories: vec![
                RepoSyncConfig {
                    name: "rocky9-baseos".to_string(),
                    base_url: "https://dl.rockylinux.org/pub/rocky/9/BaseOS/x86_64/os".to_string(),
                    interval_seconds: 3600,
                    enabled: true,
                    arch: Some("x86_64".to_string()),
                },
                RepoSyncConfig {
                    name: "rocky9-appstream".to_string(),
                    base_url: "https://dl.rockylinux.org/pub/rocky/9/AppStream/x86_64/os"
                        .to_string(),
                    interval_seconds: 3600,
                    enabled: true,
                    arch: Some("x86_64".to_string()),
                },
            ],
            work_dir: default_work_dir(),
        }
    }
}

/// Repository sync state (tracked in database)
#[cfg(feature = "sync")]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoSyncState {
    pub repo_name: String,
    pub last_sync: Option<DateTime<Utc>>,
    pub last_checksum: Option<String>,
    pub last_status: SyncStatus,
    pub last_error: Option<String>,
}

#[cfg(feature = "sync")]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SyncStatus {
    Never,
    Success,
    Failed,
    InProgress,
}

#[cfg(feature = "sync")]
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
