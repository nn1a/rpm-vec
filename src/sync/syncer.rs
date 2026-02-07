#[cfg(feature = "sync")]
use crate::api::RpmSearchApi;
#[cfg(feature = "sync")]
use crate::error::{Result, RpmSearchError};
#[cfg(feature = "sync")]
use crate::sync::config::{RepoSyncConfig, RepoSyncState, SyncStatus};
#[cfg(feature = "sync")]
use crate::sync::state::SyncStateStore;
#[cfg(feature = "sync")]
use chrono::Utc;
#[cfg(feature = "sync")]
use std::fs;
#[cfg(feature = "sync")]
use std::io::Write;
#[cfg(feature = "sync")]
use std::path::PathBuf;
#[cfg(feature = "sync")]
use tracing::{debug, error, info, warn};

#[cfg(feature = "sync")]
pub struct RepoSyncer {
    api: RpmSearchApi,
    state_store: SyncStateStore,
    work_dir: PathBuf,
}

#[cfg(feature = "sync")]
impl RepoSyncer {
    pub fn new(api: RpmSearchApi, state_store: SyncStateStore, work_dir: PathBuf) -> Result<Self> {
        // Create work directory if it doesn't exist
        fs::create_dir_all(&work_dir).map_err(RpmSearchError::Io)?;

        Ok(Self {
            api,
            state_store,
            work_dir,
        })
    }

    /// Sync a single repository
    pub fn sync_repository(&mut self, config: &RepoSyncConfig) -> Result<SyncResult> {
        info!(repo = %config.name, url = %config.base_url, "Starting repository sync");

        // Mark as in progress
        let mut state = self
            .state_store
            .get_state(&config.name)?
            .unwrap_or(RepoSyncState {
                repo_name: config.name.clone(),
                last_sync: None,
                last_checksum: None,
                last_status: SyncStatus::Never,
                last_error: None,
            });

        state.last_status = SyncStatus::InProgress;
        self.state_store.update_state(&state)?;

        // Perform sync
        let result = self.do_sync(config, &state);

        // Update state based on result
        match &result {
            Ok(sync_result) => {
                state.last_sync = Some(Utc::now());
                state.last_checksum = Some(sync_result.checksum.clone());
                state.last_status = SyncStatus::Success;
                state.last_error = None;

                info!(
                    repo = %config.name,
                    changed = sync_result.changed,
                    packages = sync_result.packages_synced,
                    "Sync completed successfully"
                );
            }
            Err(e) => {
                state.last_status = SyncStatus::Failed;
                state.last_error = Some(e.to_string());

                error!(
                    repo = %config.name,
                    error = %e,
                    "Sync failed"
                );
            }
        }

        self.state_store.update_state(&state)?;
        result
    }

    fn do_sync(
        &mut self,
        config: &RepoSyncConfig,
        current_state: &RepoSyncState,
    ) -> Result<SyncResult> {
        // Download repomd.xml
        let repomd_url = format!(
            "{}/repodata/repomd.xml",
            config.base_url.trim_end_matches('/')
        );
        debug!(url = %repomd_url, "Downloading repomd.xml");

        let repomd_content = self.download_file(&repomd_url)?;

        // Parse repomd.xml to find primary.xml location
        let primary_info = self.parse_repomd(&repomd_content)?;

        // Check if changed (compare checksum)
        let changed = match &current_state.last_checksum {
            Some(last) => last != &primary_info.checksum,
            None => true,
        };

        if !changed {
            info!(repo = %config.name, "No changes detected, skipping update");
            return Ok(SyncResult {
                changed: false,
                checksum: primary_info.checksum,
                packages_synced: 0,
            });
        }

        // Download primary.xml
        let primary_url = format!(
            "{}/{}",
            config.base_url.trim_end_matches('/'),
            primary_info.location.trim_start_matches('/')
        );
        debug!(url = %primary_url, "Downloading primary.xml");

        let primary_file = self.download_to_file(&primary_url, &config.name)?;

        // Perform incremental update
        info!(repo = %config.name, file = %primary_file.display(), "Performing incremental update");
        let packages_synced = self
            .api
            .index_repository(&primary_file, &config.name, true)?;

        // Clean up downloaded file
        if let Err(e) = fs::remove_file(&primary_file) {
            warn!(file = %primary_file.display(), error = %e, "Failed to clean up downloaded file");
        }

        Ok(SyncResult {
            changed: true,
            checksum: primary_info.checksum,
            packages_synced,
        })
    }

    fn download_file(&self, url: &str) -> Result<String> {
        let response = reqwest::blocking::get(url)
            .map_err(|e| RpmSearchError::Fetch(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(RpmSearchError::Fetch(format!(
                "HTTP {} for {}",
                response.status(),
                url
            )));
        }

        response
            .text()
            .map_err(|e| RpmSearchError::Fetch(format!("Failed to read response: {}", e)))
    }

    fn download_to_file(&self, url: &str, repo_name: &str) -> Result<PathBuf> {
        let filename = url
            .split('/')
            .next_back()
            .ok_or_else(|| RpmSearchError::Fetch("Invalid URL".to_string()))?;

        let dest_path = self.work_dir.join(format!("{}_{}", repo_name, filename));

        let response = reqwest::blocking::get(url)
            .map_err(|e| RpmSearchError::Fetch(format!("HTTP request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(RpmSearchError::Fetch(format!(
                "HTTP {} for {}",
                response.status(),
                url
            )));
        }

        let bytes = response
            .bytes()
            .map_err(|e| RpmSearchError::Fetch(format!("Failed to read response: {}", e)))?;

        let mut file = fs::File::create(&dest_path).map_err(RpmSearchError::Io)?;

        file.write_all(&bytes).map_err(RpmSearchError::Io)?;

        Ok(dest_path)
    }

    fn parse_repomd(&self, xml: &str) -> Result<PrimaryFileInfo> {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);
        // trim_text removed in quick-xml 0.39

        let mut in_primary = false;
        let mut location = None;
        let mut checksum = None;

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                    match e.name().as_ref() {
                        b"data" => {
                            // Check if this is primary data
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"type" && &attr.value[..] == b"primary"
                                {
                                    in_primary = true;
                                }
                            }
                        }
                        b"location" if in_primary => {
                            for attr in e.attributes().flatten() {
                                if attr.key.as_ref() == b"href" {
                                    location =
                                        Some(String::from_utf8_lossy(&attr.value).to_string());
                                }
                            }
                        }
                        b"checksum" if in_primary => {
                            // Read checksum text
                            if let Ok(Event::Text(e)) = reader.read_event_into(&mut buf) {
                                checksum = Some(
                                    reader
                                        .decoder()
                                        .decode(e.as_ref())
                                        .unwrap_or_default()
                                        .to_string(),
                                );
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::End(ref e)) => {
                    if e.name().as_ref() == b"data" {
                        in_primary = false;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(RpmSearchError::Parse(format!("XML parse error: {}", e))),
                _ => {}
            }
            buf.clear();
        }

        match (location, checksum) {
            (Some(loc), Some(sum)) => Ok(PrimaryFileInfo {
                location: loc,
                checksum: sum,
            }),
            _ => Err(RpmSearchError::Parse(
                "Could not find primary.xml location or checksum in repomd.xml".to_string(),
            )),
        }
    }
}

#[cfg(feature = "sync")]
#[derive(Debug)]
struct PrimaryFileInfo {
    location: String,
    checksum: String,
}

#[cfg(feature = "sync")]
#[derive(Debug)]
pub struct SyncResult {
    pub changed: bool,
    pub checksum: String,
    pub packages_synced: usize,
}
