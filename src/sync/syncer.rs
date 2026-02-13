use crate::api::RpmSearchApi;
use crate::error::{Result, RpmSearchError};
use crate::sync::config::{RepoSyncConfig, RepoSyncState, SyncStatus};
use crate::sync::state::SyncStateStore;
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

pub struct RepoSyncer {
    api: RpmSearchApi,
    state_store: SyncStateStore,
    work_dir: PathBuf,
    http: reqwest::blocking::Client,
}

impl RepoSyncer {
    pub fn new(api: RpmSearchApi, state_store: SyncStateStore, work_dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&work_dir).map_err(RpmSearchError::Io)?;

        let http = reqwest::blocking::Client::builder()
            .build()
            .map_err(|e| RpmSearchError::Fetch(format!("Failed to build HTTP client: {}", e)))?;

        Ok(Self {
            api,
            state_store,
            work_dir,
            http,
        })
    }

    /// Sync a single repository
    pub fn sync_repository(&mut self, config: &RepoSyncConfig) -> Result<SyncResult> {
        info!(repo = %config.name, url = %config.base_url, "Starting repository sync");

        let mut state = self
            .state_store
            .get_state(&config.name)?
            .unwrap_or(RepoSyncState {
                repo_name: config.name.clone(),
                last_sync: None,
                last_checksum: None,
                last_status: SyncStatus::Never,
                last_error: None,
                base_url: None,
            });

        state.base_url = Some(config.base_url.clone());
        state.last_status = SyncStatus::InProgress;
        self.state_store.update_state(&state)?;

        let result = self.do_sync(config, &state);

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

    fn do_sync(&mut self, config: &RepoSyncConfig, current_state: &RepoSyncState) -> Result<SyncResult> {
        let repomd_url = format!(
            "{}/repodata/repomd.xml",
            config.base_url.trim_end_matches('/')
        );
        debug!(url = %repomd_url, "Downloading repomd.xml");

        let repomd_content = self.download_file(&repomd_url)?;
        let repodata_info = self.parse_repomd(&repomd_content)?;

        let changed = match &current_state.last_checksum {
            Some(last) => last != &repodata_info.primary_checksum,
            None => true,
        };

        if !changed {
            info!(repo = %config.name, "No changes detected, skipping update");
            return Ok(SyncResult {
                changed: false,
                checksum: repodata_info.primary_checksum,
                packages_synced: 0,
            });
        }

        let primary_url = format!(
            "{}/{}",
            config.base_url.trim_end_matches('/'),
            repodata_info.primary_location.trim_start_matches('/')
        );
        debug!(url = %primary_url, "Downloading primary.xml");

        let primary_file = self.download_to_file(&primary_url, &config.name)?;

        info!(repo = %config.name, file = %primary_file.display(), "Performing incremental update");
        let packages_synced = self
            .api
            .index_repository(&primary_file, &config.name, true)?;

        if let Err(e) = fs::remove_file(&primary_file) {
            warn!(file = %primary_file.display(), error = %e, "Failed to clean up downloaded file");
        }

        if config.sync_filelists {
            if let Some(ref fl_location) = repodata_info.filelists_location {
                let fl_url = format!(
                    "{}/{}",
                    config.base_url.trim_end_matches('/'),
                    fl_location.trim_start_matches('/')
                );
                debug!(url = %fl_url, "Downloading filelists.xml");

                match self.download_to_file(&fl_url, &config.name) {
                    Ok(fl_file) => {
                        match self.api.index_filelists(&fl_file, &config.name) {
                            Ok(count) => {
                                info!(files_indexed = count, "Filelists indexed successfully");
                            }
                            Err(e) => {
                                warn!(error = %e, "Failed to index filelists (non-fatal)");
                            }
                        }
                        if let Err(e) = fs::remove_file(&fl_file) {
                            warn!(file = %fl_file.display(), error = %e, "Failed to clean up filelists file");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to download filelists.xml (non-fatal)");
                    }
                }
            }
        }

        Ok(SyncResult {
            changed: true,
            checksum: repodata_info.primary_checksum,
            packages_synced,
        })
    }

    fn download_file(&self, url: &str) -> Result<String> {
        let body = self
            .http
            .get(url)
            .send()
            .map_err(|e| RpmSearchError::Fetch(format!("HTTP request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| RpmSearchError::Fetch(format!("HTTP status error: {}", e)))?
            .text()
            .map_err(|e| RpmSearchError::Fetch(format!("Failed to read response: {}", e)))?;

        Ok(body)
    }

    fn download_to_file(&self, url: &str, repo_name: &str) -> Result<PathBuf> {
        let filename = url
            .split('/')
            .next_back()
            .ok_or_else(|| RpmSearchError::Fetch("Invalid URL".to_string()))?;

        let dest_path = self.work_dir.join(format!("{}_{}", repo_name, filename));

        let mut response = self
            .http
            .get(url)
            .send()
            .map_err(|e| RpmSearchError::Fetch(format!("HTTP request failed: {}", e)))?
            .error_for_status()
            .map_err(|e| RpmSearchError::Fetch(format!("HTTP status error: {}", e)))?;

        let mut file = fs::File::create(&dest_path).map_err(RpmSearchError::Io)?;
        std::io::copy(&mut response, &mut file)
            .map_err(|e| RpmSearchError::Fetch(format!("Failed to write downloaded file: {}", e)))?;

        Ok(dest_path)
    }

    fn parse_repomd(&self, xml: &str) -> Result<RepoDataInfo> {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);

        #[derive(PartialEq)]
        enum Section {
            None,
            Primary,
            Filelists,
        }

        let mut section = Section::None;
        let mut primary_location = None;
        let mut primary_checksum = None;
        let mut filelists_location = None;

        let mut buf = Vec::new();
        loop {
            match reader.read_event_into(&mut buf) {
                Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => match e.name().as_ref() {
                    b"data" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"type" {
                                match &attr.value[..] {
                                    b"primary" => section = Section::Primary,
                                    b"filelists" => section = Section::Filelists,
                                    _ => {}
                                }
                            }
                        }
                    }
                    b"location" if section != Section::None => {
                        for attr in e.attributes().flatten() {
                            if attr.key.as_ref() == b"href" {
                                let href = String::from_utf8_lossy(&attr.value).to_string();
                                match section {
                                    Section::Primary => primary_location = Some(href),
                                    Section::Filelists => filelists_location = Some(href),
                                    Section::None => {}
                                }
                            }
                        }
                    }
                    b"checksum" if section == Section::Primary => {
                        if let Ok(Event::Text(e)) = reader.read_event_into(&mut buf) {
                            primary_checksum = Some(
                                reader
                                    .decoder()
                                    .decode(e.as_ref())
                                    .unwrap_or_default()
                                    .to_string(),
                            );
                        }
                    }
                    _ => {}
                },
                Ok(Event::End(ref e)) => {
                    if e.name().as_ref() == b"data" {
                        section = Section::None;
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(RpmSearchError::Parse(format!("XML parse error: {}", e))),
                _ => {}
            }
            buf.clear();
        }

        match (primary_location, primary_checksum) {
            (Some(loc), Some(sum)) => Ok(RepoDataInfo {
                primary_location: loc,
                primary_checksum: sum,
                filelists_location,
            }),
            _ => Err(RpmSearchError::Parse(
                "Could not find primary.xml location or checksum in repomd.xml".to_string(),
            )),
        }
    }
}

#[derive(Debug)]
struct RepoDataInfo {
    primary_location: String,
    primary_checksum: String,
    filelists_location: Option<String>,
}

#[derive(Debug)]
pub struct SyncResult {
    pub changed: bool,
    pub checksum: String,
    pub packages_synced: usize,
}
