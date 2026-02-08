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
    pub license: Option<String>,
    pub vcs: Option<String>,
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

/// File type from filelists.xml
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RpmFileType {
    File,
    Dir,
    Ghost,
}

impl RpmFileType {
    /// Convert to integer for storage (0=file, 1=dir, 2=ghost)
    pub fn as_i32(&self) -> i32 {
        match self {
            RpmFileType::File => 0,
            RpmFileType::Dir => 1,
            RpmFileType::Ghost => 2,
        }
    }

    /// Convert from integer
    #[allow(dead_code)]
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => RpmFileType::Dir,
            2 => RpmFileType::Ghost,
            _ => RpmFileType::File,
        }
    }
}

/// File entry with type information from filelists.xml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpmFileEntry {
    pub path: String,
    pub file_type: RpmFileType,
}

/// Package file list from filelists.xml (matched to existing packages by NEVRA)
#[derive(Debug, Clone)]
pub struct FilelistsPackage {
    pub name: String,
    pub arch: String,
    pub epoch: Option<i64>,
    pub version: String,
    pub release: String,
    pub files: Vec<RpmFileEntry>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RepoMetadata {
    pub location: String,
    pub checksum: String,
    pub timestamp: i64,
}
