use crate::error::Result;
use crate::normalize::Package;
use crate::storage::PackageStore;

pub struct StructuredSearch<'a> {
    store: &'a PackageStore,
}

impl<'a> StructuredSearch<'a> {
    pub fn new(store: &'a PackageStore) -> Self {
        Self { store }
    }

    /// Search packages by name
    pub fn search_by_name(&self, name: &str) -> Result<Vec<Package>> {
        self.store.search_by_name(name)
    }

    /// Get packages by IDs
    pub fn get_packages(&self, pkg_ids: &[i64]) -> Result<Vec<Package>> {
        let mut packages = Vec::new();
        for &pkg_id in pkg_ids {
            if let Some(pkg) = self.store.get_package(pkg_id)? {
                packages.push(pkg);
            }
        }
        Ok(packages)
    }

    /// Filter packages by architecture
    pub fn filter_by_arch(&self, packages: Vec<Package>, arch: &str) -> Vec<Package> {
        packages.into_iter().filter(|p| p.arch == arch).collect()
    }

    /// Filter packages that do NOT require a specific dependency
    pub fn filter_not_requiring(&self, packages: Vec<Package>, dep_name: &str) -> Vec<Package> {
        packages
            .into_iter()
            .filter(|p| !p.requires.iter().any(|r| r.name == dep_name))
            .collect()
    }

    /// Filter packages that provide a specific capability
    pub fn filter_providing(&self, packages: Vec<Package>, capability: &str) -> Vec<Package> {
        packages
            .into_iter()
            .filter(|p| p.provides.iter().any(|prov| prov.name == capability))
            .collect()
    }
    /// Get filtered package IDs for pre-filtering vector search
    pub fn get_filtered_candidates(
        &self,
        arch: Option<&str>,
        repo: Option<&str>,
    ) -> Result<Vec<i64>> {
        self.store.get_filtered_pkg_ids(arch, repo)
    }
}
