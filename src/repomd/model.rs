use serde::{Deserialize, Serialize};

/// Raw RPM package metadata from rpm-md XML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpmPackage {
    pub name: String,
    pub epoch: Option<i64>,
    pub version: String,
    pub release: String,
    pub arch: String,
    pub summary: String,
    pub description: String,
    pub packager: Option<String>,
    pub url: Option<String>,
    pub requires: Vec<RpmDependency>,
    pub provides: Vec<RpmDependency>,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpmDependency {
    pub name: String,
    pub flags: Option<String>, // "EQ", "LT", "GT", "LE", "GE"
    pub epoch: Option<String>,
    pub version: Option<String>,
    pub release: Option<String>,
}

impl RpmDependency {
    #[allow(dead_code)]
    pub fn new(name: String) -> Self {
        Self {
            name,
            flags: None,
            epoch: None,
            version: None,
            release: None,
        }
    }

    #[allow(dead_code)]
    pub fn with_version(mut self, flags: String, version: String) -> Self {
        self.flags = Some(flags);
        self.version = Some(version);
        self
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RepoMetadata {
    pub location: String,
    pub checksum: String,
    pub timestamp: i64,
}
