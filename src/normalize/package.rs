use super::version::RpmVersion;
use crate::repomd::model::{RpmDependency, RpmPackage};
use serde::{Deserialize, Serialize};

/// Normalized package model for internal use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub pkg_id: Option<i64>,
    pub name: String,
    pub epoch: Option<i64>,
    pub version: String,
    pub release: String,
    pub arch: String,
    pub summary: String,
    pub description: String,
    pub license: Option<String>,
    pub vcs: Option<String>,
    pub repo: String,
    pub requires: Vec<Dependency>,
    pub provides: Vec<Dependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dependency {
    pub name: String,
    pub flags: Option<String>,
    pub version: Option<String>,
}

impl From<RpmDependency> for Dependency {
    fn from(rpm_dep: RpmDependency) -> Self {
        // Combine epoch:version-release into a single version string
        let version = if let Some(ver) = rpm_dep.version {
            let mut full_version = String::new();
            if let Some(epoch) = rpm_dep.epoch {
                full_version.push_str(&epoch);
                full_version.push(':');
            }
            full_version.push_str(&ver);
            if let Some(rel) = rpm_dep.release {
                full_version.push('-');
                full_version.push_str(&rel);
            }
            Some(full_version)
        } else {
            None
        };

        Self {
            name: rpm_dep.name,
            flags: rpm_dep.flags,
            version,
        }
    }
}

impl Package {
    /// Convert raw RPM package to normalized Package
    pub fn from_rpm_package(rpm_pkg: RpmPackage, repo: String) -> Self {
        Self {
            pkg_id: None,
            name: rpm_pkg.name,
            epoch: rpm_pkg.epoch,
            version: rpm_pkg.version,
            release: rpm_pkg.release,
            arch: rpm_pkg.arch,
            summary: rpm_pkg.summary,
            description: rpm_pkg.description,
            license: rpm_pkg.license,
            vcs: rpm_pkg.vcs,
            repo,
            requires: rpm_pkg.requires.into_iter().map(Dependency::from).collect(),
            provides: rpm_pkg.provides.into_iter().map(Dependency::from).collect(),
        }
    }

    /// Convert package to RpmVersion for version comparison
    pub fn to_rpm_version(&self) -> RpmVersion {
        RpmVersion::new(self.epoch, self.version.clone(), self.release.clone())
    }

    /// Maximum description length in chars for embedding text.
    /// Keeps total token count well within the 512-token context window.
    const MAX_DESCRIPTION_CHARS: usize = 400;

    /// Maximum number of provides/requires entries in embedding text.
    const MAX_DEPS_COUNT: usize = 20;

    /// Build text for embedding
    ///
    /// Package name is repeated twice to increase its weight in the embedding vector,
    /// ensuring name-based semantic matches rank higher.
    ///
    /// Description is truncated to [`MAX_DESCRIPTION_CHARS`] and provides/requires are
    /// limited to [`MAX_DEPS_COUNT`] entries to stay within the 512-token context window
    /// (important for both MiniLM and E5 models).
    pub fn build_embedding_text(&self) -> String {
        let mut text = String::new();

        // Name appears twice for higher weight in embedding
        text.push_str("Package: ");
        text.push_str(&self.name);
        text.push('\n');

        text.push_str("Name: ");
        text.push_str(&self.name);
        text.push('\n');

        text.push_str("Architecture: ");
        text.push_str(&self.arch);
        text.push('\n');

        text.push_str("Summary: ");
        text.push_str(&self.summary);
        text.push('\n');

        text.push_str("Description:\n");
        if self.description.len() > Self::MAX_DESCRIPTION_CHARS {
            // Truncate at char boundary
            let truncated = &self.description[..self
                .description
                .char_indices()
                .take_while(|(i, _)| *i < Self::MAX_DESCRIPTION_CHARS)
                .last()
                .map(|(i, c)| i + c.len_utf8())
                .unwrap_or(0)];
            text.push_str(truncated);
        } else {
            text.push_str(&self.description);
        }
        text.push('\n');

        if !self.provides.is_empty() {
            text.push_str("Provides: ");
            let provides_str: Vec<String> = self
                .provides
                .iter()
                .take(Self::MAX_DEPS_COUNT)
                .map(|p| p.name.clone())
                .collect();
            text.push_str(&provides_str.join(", "));
            text.push('\n');
        }

        if !self.requires.is_empty() {
            text.push_str("Requires: ");
            let requires_str: Vec<String> = self
                .requires
                .iter()
                .take(Self::MAX_DEPS_COUNT)
                .map(|r| r.name.clone())
                .collect();
            text.push_str(&requires_str.join(", "));
            text.push('\n');
        }

        text
    }

    /// Get version string with epoch
    pub fn full_version(&self) -> String {
        let mut version = String::new();
        if let Some(epoch) = self.epoch {
            version.push_str(&epoch.to_string());
            version.push(':');
        }
        version.push_str(&self.version);
        version.push('-');
        version.push_str(&self.release);
        version
    }
}

// Version comparison for Package
impl PartialEq for Package {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.epoch == other.epoch
            && self.version == other.version
            && self.release == other.release
            && self.arch == other.arch
    }
}

impl Eq for Package {}

impl PartialOrd for Package {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Package {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // First compare by name
        match self.name.cmp(&other.name) {
            std::cmp::Ordering::Equal => {}
            order => return order,
        }

        // Then compare by architecture
        match self.arch.cmp(&other.arch) {
            std::cmp::Ordering::Equal => {}
            order => return order,
        }

        // Finally compare by version using RPM algorithm
        self.to_rpm_version().cmp(&other.to_rpm_version())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_embedding_text() {
        let pkg = Package {
            pkg_id: None,
            name: "openssl".to_string(),
            epoch: Some(1),
            version: "3.0.0".to_string(),
            release: "1.el9".to_string(),
            arch: "x86_64".to_string(),
            summary: "Cryptography library".to_string(),
            description: "OpenSSL is a robust cryptography library".to_string(),
            license: None,
            vcs: None,
            repo: "baseos".to_string(),
            requires: vec![Dependency {
                name: "glibc".to_string(),
                flags: Some(">=".to_string()),
                version: Some("2.34".to_string()),
            }],
            provides: vec![Dependency {
                name: "libssl.so.3".to_string(),
                flags: None,
                version: None,
            }],
        };

        let text = pkg.build_embedding_text();
        assert!(text.contains("Package: openssl"));
        assert!(text.contains("Name: openssl"));
        assert!(text.contains("Architecture: x86_64"));
        assert!(text.contains("Summary: Cryptography library"));
        assert!(text.contains("Provides: libssl.so.3"));
        assert!(text.contains("Requires: glibc"));
    }

    #[test]
    fn test_full_version() {
        let pkg = Package {
            pkg_id: None,
            name: "test".to_string(),
            epoch: Some(2),
            version: "1.0.0".to_string(),
            release: "1.el9".to_string(),
            arch: "x86_64".to_string(),
            summary: "".to_string(),
            description: "".to_string(),
            license: None,
            vcs: None,
            repo: "".to_string(),
            requires: vec![],
            provides: vec![],
        };

        assert_eq!(pkg.full_version(), "2:1.0.0-1.el9");
    }

    #[test]
    fn test_version_comparison() {
        let pkg1 = Package {
            pkg_id: None,
            name: "kernel".to_string(),
            epoch: None,
            version: "5.14.0".to_string(),
            release: "279.el9".to_string(),
            arch: "x86_64".to_string(),
            summary: "".to_string(),
            description: "".to_string(),
            license: None,
            vcs: None,
            repo: "".to_string(),
            requires: vec![],
            provides: vec![],
        };

        let pkg2 = Package {
            pkg_id: None,
            name: "kernel".to_string(),
            epoch: None,
            version: "5.14.0".to_string(),
            release: "754.el9".to_string(),
            arch: "x86_64".to_string(),
            summary: "".to_string(),
            description: "".to_string(),
            license: None,
            vcs: None,
            repo: "".to_string(),
            requires: vec![],
            provides: vec![],
        };

        // pkg1 (279) < pkg2 (754)
        assert!(pkg1 < pkg2);
    }

    #[test]
    fn test_epoch_comparison() {
        let pkg1 = Package {
            pkg_id: None,
            name: "glibc".to_string(),
            epoch: Some(1),
            version: "2.34".to_string(),
            release: "1.el9".to_string(),
            arch: "x86_64".to_string(),
            summary: "".to_string(),
            description: "".to_string(),
            license: None,
            vcs: None,
            repo: "".to_string(),
            requires: vec![],
            provides: vec![],
        };

        let pkg2 = Package {
            pkg_id: None,
            name: "glibc".to_string(),
            epoch: None, // epoch 0
            version: "3.0".to_string(),
            release: "1.el9".to_string(),
            arch: "x86_64".to_string(),
            summary: "".to_string(),
            description: "".to_string(),
            license: None,
            vcs: None,
            repo: "".to_string(),
            requires: vec![],
            provides: vec![],
        };

        // epoch 1 > epoch 0, even though 2.34 < 3.0
        assert!(pkg1 > pkg2);
    }
}
