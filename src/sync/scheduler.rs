use crate::config::Config;
use crate::error::Result;
use crate::sync::config::SyncConfig;
use crate::sync::state::SyncStateStore;
use crate::sync::syncer::RepoSyncer;
use rusqlite::Connection;
use std::collections::HashMap;
use std::time::Duration;
use tokio::time;
use tracing::{error, info, warn};

pub struct SyncScheduler {
    sync_config: SyncConfig,
    db_config: Config,
    embedding_enabled: bool,
}

impl SyncScheduler {
    pub fn new(sync_config: SyncConfig, db_config: Config) -> Self {
        Self {
            sync_config,
            db_config,
            embedding_enabled: true,
        }
    }

    /// Enable or disable automatic embedding generation after sync
    pub fn set_embedding_enabled(&mut self, enabled: bool) {
        self.embedding_enabled = enabled;
    }

    /// Run scheduler in daemon mode
    pub async fn run_daemon(&self) -> Result<()> {
        info!("Starting sync scheduler daemon");

        // Create interval tasks for each repository
        let mut tasks = Vec::new();

        let embedding_enabled = self.embedding_enabled;

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
                    if let Err(e) =
                        Self::perform_sync(&repo_config, &db_config, &work_dir, embedding_enabled)
                            .await
                    {
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

            let result = Self::perform_sync(
                repo_config,
                &self.db_config,
                &self.sync_config.work_dir,
                false, // sync_once: embedding is handled by the caller (main.rs)
            )
            .await;

            let repo_name = repo_config.name.clone();
            results.insert(repo_name, result);
        }

        Ok(results)
    }

    async fn perform_sync(
        repo_config: &crate::sync::config::RepoSyncConfig,
        db_config: &Config,
        work_dir: &std::path::Path,
        embedding_enabled: bool,
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
            let result = syncer.sync_repository(&repo_config)?;

            // Build embeddings incrementally for new packages
            if embedding_enabled && result.changed && result.packages_synced > 0 {
                info!(
                    repo = %repo_config.name,
                    packages_synced = result.packages_synced,
                    "Building embeddings for new packages"
                );
                let api = crate::api::RpmSearchApi::new(db_config.clone())?;
                let embedder = crate::embedding::Embedder::new(
                    &db_config.model_path,
                    &db_config.tokenizer_path,
                    db_config.model_type.clone(),
                )?;
                let count = api.build_embeddings(&embedder, false, false)?;
                info!(
                    repo = %repo_config.name,
                    new_embeddings = count,
                    "Incremental embedding build completed"
                );
            }

            Ok(())
        })
        .await
        .map_err(|e| crate::error::RpmSearchError::Config(format!("Task join error: {}", e)))?
    }
}
