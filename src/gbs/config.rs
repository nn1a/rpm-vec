//! GBS Configuration Parser
//!
//! Parses GBS configuration files (~/.gbs.conf or custom paths)
//! to extract profile and repository URL information.
//!
//! ## gbs.conf format
//!
//! ```ini
//! [general]
//! profile = profile.tizen
//!
//! [profile.tizen]
//! repos = repo.tizen_base, repo.tizen_unified
//!
//! [repo.tizen_base]
//! url = http://download.tizen.org/.../packages/
//!
//! [repo.tizen_unified]
//! url = http://download.tizen.org/.../packages/
//! ```
//!
//! Parsing logic follows GBS Python implementation (gbs/gitbuildsys/conf.py).

use crate::error::{Result, RpmSearchError};
use crate::sync::config::{RepoSyncConfig, SyncConfig};
use ini::Ini;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Parsed GBS configuration
#[derive(Debug, Clone)]
pub struct GbsConfig {
    /// Path to the config file
    #[allow(dead_code)]
    pub config_path: PathBuf,
    /// Default profile name from [general] section
    pub default_profile: Option<String>,
    /// Profile-specific configurations
    pub profiles: HashMap<String, ProfileConfig>,
    /// Repository configurations ([repo.*] sections)
    pub repos: HashMap<String, RepoConfig>,
}

/// Profile-specific configuration
#[derive(Debug, Clone)]
pub struct ProfileConfig {
    /// Profile name (e.g., "tizen" from "profile.tizen")
    #[allow(dead_code)]
    pub name: String,
    /// Repository references (e.g., ["repo.tizen_base", "repo.tizen_unified"])
    pub repos: Vec<String>,
}

/// Repository configuration from [repo.*] section
#[derive(Debug, Clone)]
pub struct RepoConfig {
    /// Repository name (e.g., "tizen_base" from "repo.tizen_base")
    pub name: String,
    /// Repository URL
    pub url: String,
}

impl GbsConfig {
    /// Parse GBS config from default location (~/.gbs.conf)
    #[allow(dead_code)]
    pub fn from_default() -> Result<Self> {
        let home = dirs::home_dir()
            .ok_or_else(|| RpmSearchError::Config("Cannot determine home directory".to_string()))?;
        let config_path = home.join(".gbs.conf");
        Self::from_path(&config_path)
    }

    /// Parse GBS config from specified path
    pub fn from_path(path: &Path) -> Result<Self> {
        let ini = Ini::load_from_file(path).map_err(|e| {
            RpmSearchError::Config(format!(
                "Failed to read GBS config from {}: {}",
                path.display(),
                e
            ))
        })?;

        Self::parse(&ini, path.to_path_buf())
    }

    /// Parse GBS config from INI structure
    fn parse(ini: &Ini, config_path: PathBuf) -> Result<Self> {
        let mut default_profile = None;
        let mut profiles = HashMap::new();
        let mut repos = HashMap::new();

        // Parse [general] section
        if let Some(general) = ini.section(Some("general")) {
            if let Some(profile_val) = general.get("profile") {
                if let Some(name) = profile_val.strip_prefix("profile.") {
                    default_profile = Some(name.to_string());
                } else {
                    default_profile = Some(profile_val.to_string());
                }
            }
        }

        // Parse [profile.*] and [repo.*] sections
        for (section_name, section_data) in ini.iter() {
            if let Some(section_name) = section_name {
                if let Some(profile_name) = section_name.strip_prefix("profile.") {
                    // Parse [profile.*] section
                    let repo_refs = section_data
                        .get("repos")
                        .map(|r| r.split(',').map(|s| s.trim().to_string()).collect())
                        .unwrap_or_default();

                    profiles.insert(
                        profile_name.to_string(),
                        ProfileConfig {
                            name: profile_name.to_string(),
                            repos: repo_refs,
                        },
                    );
                } else if let Some(repo_name) = section_name.strip_prefix("repo.") {
                    // Parse [repo.*] section
                    if let Some(url) = section_data.get("url") {
                        repos.insert(
                            repo_name.to_string(),
                            RepoConfig {
                                name: repo_name.to_string(),
                                url: url.to_string(),
                            },
                        );
                    }
                }
            }
        }

        Ok(GbsConfig {
            config_path,
            default_profile,
            profiles,
            repos,
        })
    }

    /// Get the effective profile name
    ///
    /// Priority: explicit argument > default_profile from config > first available profile
    pub fn resolve_profile(&self, profile: Option<&str>) -> Result<String> {
        if let Some(name) = profile {
            if self.profiles.contains_key(name) {
                return Ok(name.to_string());
            }
            return Err(RpmSearchError::Config(format!(
                "GBS profile '{}' not found. Available profiles: {}",
                name,
                self.get_profile_names().join(", ")
            )));
        }

        if let Some(ref name) = self.default_profile {
            if self.profiles.contains_key(name) {
                return Ok(name.clone());
            }
        }

        self.profiles
            .keys()
            .next()
            .cloned()
            .ok_or_else(|| RpmSearchError::Config("No profiles found in GBS config".to_string()))
    }

    /// Get repository URLs for a profile
    ///
    /// Returns a list of (repo_name, url) pairs.
    pub fn get_repo_urls(&self, profile: Option<&str>) -> Result<Vec<(String, String)>> {
        let profile_name = self.resolve_profile(profile)?;

        let profile_config = self.profiles.get(&profile_name).ok_or_else(|| {
            RpmSearchError::Config(format!("GBS profile '{}' not found", profile_name))
        })?;

        let mut result = Vec::new();

        for repo_ref in &profile_config.repos {
            // Strip "repo." prefix if present (e.g., "repo.tizen_base" → "tizen_base")
            let repo_key = repo_ref.strip_prefix("repo.").unwrap_or(repo_ref);

            let repo_config = self.repos.get(repo_key).ok_or_else(|| {
                RpmSearchError::Config(format!(
                    "GBS config: repository section [repo.{}] not found (referenced by profile '{}')",
                    repo_key, profile_name
                ))
            })?;

            result.push((repo_config.name.clone(), repo_config.url.clone()));
        }

        Ok(result)
    }

    /// Convert GBS config to SyncConfig for use with the sync infrastructure
    pub fn to_sync_config(&self, profile: Option<&str>) -> Result<SyncConfig> {
        let repo_urls = self.get_repo_urls(profile)?;

        let repositories = repo_urls
            .into_iter()
            .map(|(name, url)| RepoSyncConfig {
                name,
                base_url: url.trim_end_matches('/').to_string(),
                interval_seconds: 3600,
                enabled: true,
                sync_filelists: false,
            })
            .collect();

        Ok(SyncConfig {
            repositories,
            work_dir: PathBuf::from(".rpm-sync"),
        })
    }

    /// Get all available profile names
    pub fn get_profile_names(&self) -> Vec<String> {
        self.profiles.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_temp_config(content: &str) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut tmpfile = tempfile::NamedTempFile::new().unwrap();
        tmpfile.write_all(content.as_bytes()).unwrap();
        tmpfile.flush().unwrap();
        tmpfile
    }

    #[test]
    fn test_parse_with_repos() {
        let config = r#"
[general]
profile = profile.tizen

[profile.tizen]
repos = repo.base, repo.unified

[repo.base]
url = http://download.tizen.org/base/packages/

[repo.unified]
url = http://download.tizen.org/unified/packages/
"#;
        let tmpfile = write_temp_config(config);
        let parsed = GbsConfig::from_path(tmpfile.path()).unwrap();

        assert_eq!(parsed.default_profile, Some("tizen".to_string()));
        assert_eq!(parsed.profiles.len(), 1);
        assert_eq!(parsed.repos.len(), 2);

        let base = parsed.repos.get("base").unwrap();
        assert_eq!(base.url, "http://download.tizen.org/base/packages/");

        let unified = parsed.repos.get("unified").unwrap();
        assert_eq!(unified.url, "http://download.tizen.org/unified/packages/");
    }

    #[test]
    fn test_get_repo_urls() {
        let config = r#"
[general]
profile = profile.tizen

[profile.tizen]
repos = repo.base, repo.unified

[repo.base]
url = http://example.com/base/

[repo.unified]
url = http://example.com/unified/
"#;
        let tmpfile = write_temp_config(config);
        let parsed = GbsConfig::from_path(tmpfile.path()).unwrap();

        let urls = parsed.get_repo_urls(None).unwrap();
        assert_eq!(urls.len(), 2);
        assert!(urls.iter().any(|(n, _)| n == "base"));
        assert!(urls.iter().any(|(n, _)| n == "unified"));
    }

    #[test]
    fn test_to_sync_config() {
        let config = r#"
[general]
profile = profile.tizen

[profile.tizen]
repos = repo.base, repo.unified

[repo.base]
url = http://example.com/base/

[repo.unified]
url = http://example.com/unified/
"#;
        let tmpfile = write_temp_config(config);
        let parsed = GbsConfig::from_path(tmpfile.path()).unwrap();

        let sync_config = parsed.to_sync_config(None).unwrap();
        assert_eq!(sync_config.repositories.len(), 2);

        let base_repo = sync_config
            .repositories
            .iter()
            .find(|r| r.name == "base")
            .unwrap();
        assert_eq!(base_repo.base_url, "http://example.com/base");
        assert!(base_repo.enabled);
    }

    #[test]
    fn test_explicit_profile_selection() {
        let config = r#"
[general]
profile = profile.default

[profile.default]
repos = repo.a

[profile.custom]
repos = repo.b

[repo.a]
url = http://example.com/a/

[repo.b]
url = http://example.com/b/
"#;
        let tmpfile = write_temp_config(config);
        let parsed = GbsConfig::from_path(tmpfile.path()).unwrap();

        // Default profile
        let urls = parsed.get_repo_urls(None).unwrap();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].0, "a");

        // Explicit profile
        let urls = parsed.get_repo_urls(Some("custom")).unwrap();
        assert_eq!(urls.len(), 1);
        assert_eq!(urls[0].0, "b");
    }

    #[test]
    fn test_missing_repo_section() {
        let config = r#"
[general]
profile = profile.tizen

[profile.tizen]
repos = repo.base, repo.missing

[repo.base]
url = http://example.com/base/
"#;
        let tmpfile = write_temp_config(config);
        let parsed = GbsConfig::from_path(tmpfile.path()).unwrap();

        let result = parsed.get_repo_urls(None);
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("missing"));
    }

    #[test]
    fn test_nonexistent_profile() {
        let config = r#"
[general]
profile = profile.tizen

[profile.tizen]
repos = repo.base

[repo.base]
url = http://example.com/base/
"#;
        let tmpfile = write_temp_config(config);
        let parsed = GbsConfig::from_path(tmpfile.path()).unwrap();

        let result = parsed.get_repo_urls(Some("nonexistent"));
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("nonexistent"));
    }

    #[test]
    fn test_default_profile_selection() {
        let config = r#"
[profile.only_one]
repos = repo.a

[repo.a]
url = http://example.com/a/
"#;
        let tmpfile = write_temp_config(config);
        let parsed = GbsConfig::from_path(tmpfile.path()).unwrap();

        // No default_profile set, should pick first available
        assert_eq!(parsed.default_profile, None);
        let urls = parsed.get_repo_urls(None).unwrap();
        assert_eq!(urls.len(), 1);
    }

    #[test]
    fn test_trailing_slash_normalization() {
        let config = r#"
[general]
profile = profile.test

[profile.test]
repos = repo.a

[repo.a]
url = http://example.com/packages/
"#;
        let tmpfile = write_temp_config(config);
        let parsed = GbsConfig::from_path(tmpfile.path()).unwrap();

        let sync_config = parsed.to_sync_config(None).unwrap();
        // Trailing slash should be stripped in sync config
        assert_eq!(
            sync_config.repositories[0].base_url,
            "http://example.com/packages"
        );
    }
}
