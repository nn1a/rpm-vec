#[cfg(feature = "sync")]
use crate::config::Config;
#[cfg(feature = "sync")]
use crate::error::Result;
#[cfg(feature = "sync")]
use crate::sync::config::SyncConfig;
#[cfg(feature = "sync")]
use crate::sync::state::SyncStateStore;
#[cfg(feature = "sync")]
use crate::sync::syncer::RepoSyncer;
#[cfg(feature = "sync")]
use rusqlite::Connection;
#[cfg(feature = "sync")]
use std::collections::HashMap;
#[cfg(feature = "sync")]
use std::time::Duration;
#[cfg(feature = "sync")]
use tokio::time;
#[cfg(feature = "sync")]
use tracing::{error, info, warn};

#[cfg(feature = "sync")]
pub struct SyncScheduler {
    sync_config: SyncConfig,
    db_config: Config,
}

#[cfg(feature = "sync")]
impl SyncScheduler {
    pub fn new(sync_config: SyncConfig, db_config: Config) -> Self {
        Self {
            sync_config,
            db_config,
        }
    }

    /// Run scheduler in daemon mode
    pub async fn run_daemon(&self) -> Result<()> {
        info!("Starting sync scheduler daemon");

        // Create interval tasks for each repository
        let mut tasks = Vec::new();

        for repo_config in &self.sync_config.repositories {
            if !repo_config.enabled {
                info!(repo = %repo_config.name, "Repository disabled, skipping");
                continue;
            }

            let repo_config = repo_config.clone();
            let db_config = self.db_config.clone();
            let work_dir = self.sync_config.work_dir.clone();

            let task = tokio::spawn(async move {
                let mut interval =
                    time::interval(Duration::from_secs(repo_config.interval_seconds));
                interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

                info!(
                    repo = %repo_config.name,
                    interval_seconds = repo_config.interval_seconds,
                    "Starting sync task"
                );

                loop {
                    interval.tick().await;

                    info!(repo = %repo_config.name, "Sync tick triggered");

                    // Perform sync
                    if let Err(e) = Self::perform_sync(&repo_config, &db_config, &work_dir).await {
                        error!(repo = %repo_config.name, error = %e, "Sync failed");
                    }
                }
            });

            tasks.push(task);
        }

        if tasks.is_empty() {
            warn!("No repositories enabled for syncing");
            return Ok(());
        }

        // Wait for all tasks (they run indefinitely)
        for task in tasks {
            if let Err(e) = task.await {
                error!(error = %e, "Sync task panicked");
            }
        }

        Ok(())
    }

    /// Perform a one-time sync of all enabled repositories
    pub async fn sync_once(&self) -> Result<HashMap<String, Result<()>>> {
        info!("Performing one-time sync of all repositories");

        let mut results = HashMap::new();

        for repo_config in &self.sync_config.repositories {
            if !repo_config.enabled {
                info!(repo = %repo_config.name, "Repository disabled, skipping");
                continue;
            }

            let result =
                Self::perform_sync(repo_config, &self.db_config, &self.sync_config.work_dir).await;

            let repo_name = repo_config.name.clone();
            results.insert(repo_name, result);
        }

        Ok(results)
    }

    async fn perform_sync(
        repo_config: &crate::sync::config::RepoSyncConfig,
        db_config: &Config,
        work_dir: &std::path::Path,
    ) -> Result<()> {
        // Run sync in blocking context (since RpmSearchApi is synchronous)
        let repo_config = repo_config.clone();
        let db_config = db_config.clone();
        let work_dir = work_dir.to_path_buf();

        tokio::task::spawn_blocking(move || {
            // Create API and state store
            let api = crate::api::RpmSearchApi::new(db_config.clone())?;

            let state_conn = Connection::open(&db_config.db_path)?;
            let state_store = SyncStateStore::new(state_conn)?;

            let mut syncer = RepoSyncer::new(api, state_store, work_dir)?;

            // Perform sync
            syncer.sync_repository(&repo_config)?;

            Ok(())
        })
        .await
        .map_err(|e| crate::error::RpmSearchError::Config(format!("Task join error: {}", e)))?
    }
}
