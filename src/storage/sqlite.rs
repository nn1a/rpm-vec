use crate::error::Result;
use crate::normalize::package::{Dependency, Package};
use crate::storage::schema::Schema;
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

pub struct PackageStore {
    conn: Connection,
}

impl PackageStore {
    /// Create a new package store
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        Schema::initialize(&conn)?;
        Ok(Self { conn })
    }

    /// Insert a package and return its pkg_id
    pub fn insert_package(&mut self, package: &Package) -> Result<i64> {
        let tx = self.conn.transaction()?;

        // Insert package
        tx.execute(
            "INSERT INTO packages (name, epoch, version, release, arch, summary, description, repo)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                package.name,
                package.epoch,
                package.version,
                package.release,
                package.arch,
                package.summary,
                package.description,
                package.repo,
            ],
        )?;

        let pkg_id = tx.last_insert_rowid();

        // Insert requires
        for req in &package.requires {
            tx.execute(
                "INSERT INTO requires (pkg_id, name, flags, version) VALUES (?, ?, ?, ?)",
                params![pkg_id, req.name, req.flags, req.version],
            )?;
        }

        // Insert provides
        for prov in &package.provides {
            tx.execute(
                "INSERT INTO provides (pkg_id, name, flags, version) VALUES (?, ?, ?, ?)",
                params![pkg_id, prov.name, prov.flags, prov.version],
            )?;
        }

        tx.commit()?;
        Ok(pkg_id)
    }

    /// Get a package by pkg_id
    pub fn get_package(&self, pkg_id: i64) -> Result<Option<Package>> {
        let mut stmt = self.conn.prepare(
            "SELECT pkg_id, name, epoch, version, release, arch, summary, description, repo
             FROM packages WHERE pkg_id = ?",
        )?;

        let package = stmt
            .query_row([pkg_id], |row| {
                Ok(Package {
                    pkg_id: Some(row.get(0)?),
                    name: row.get(1)?,
                    epoch: row.get(2)?,
                    version: row.get(3)?,
                    release: row.get(4)?,
                    arch: row.get(5)?,
                    summary: row.get(6)?,
                    description: row.get(7)?,
                    repo: row.get(8)?,
                    requires: Vec::new(),
                    provides: Vec::new(),
                })
            })
            .optional()?;

        if let Some(mut pkg) = package {
            // Load requires
            let mut req_stmt = self
                .conn
                .prepare("SELECT name, flags, version FROM requires WHERE pkg_id = ?")?;
            let requires = req_stmt
                .query_map([pkg_id], |row| {
                    Ok(Dependency {
                        name: row.get(0)?,
                        flags: row.get(1)?,
                        version: row.get(2)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            pkg.requires = requires;

            // Load provides
            let mut prov_stmt = self
                .conn
                .prepare("SELECT name, flags, version FROM provides WHERE pkg_id = ?")?;
            let provides = prov_stmt
                .query_map([pkg_id], |row| {
                    Ok(Dependency {
                        name: row.get(0)?,
                        flags: row.get(1)?,
                        version: row.get(2)?,
                    })
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            pkg.provides = provides;

            Ok(Some(pkg))
        } else {
            Ok(None)
        }
    }

    /// Search packages by name
    pub fn search_by_name(&self, name: &str) -> Result<Vec<Package>> {
        // First try exact match
        let mut stmt = self
            .conn
            .prepare("SELECT pkg_id FROM packages WHERE name = ?")?;

        let mut pkg_ids: Vec<i64> = stmt
            .query_map([name], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // If no exact match, try partial match (contains the search term)
        if pkg_ids.is_empty() {
            let mut stmt = self.conn.prepare(
                "SELECT pkg_id FROM packages WHERE name LIKE ? ORDER BY name ASC LIMIT 100",
            )?;

            pkg_ids = stmt
                .query_map([format!("%{}%", name)], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
        }

        let mut packages = Vec::new();
        for pkg_id in pkg_ids {
            if let Some(pkg) = self.get_package(pkg_id)? {
                packages.push(pkg);
            }
        }

        Ok(packages)
    }

    /// Search packages by name with relevance scoring
    /// Returns (pkg_id, score) pairs ordered by relevance
    pub fn search_by_name_ranked(&self, query: &str) -> Result<Vec<(i64, f32)>> {
        let lower_query = query.to_lowercase();
        let mut results: Vec<(i64, f32)> = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        // 1. Exact name match (highest score: 1.0)
        {
            let mut stmt = self
                .conn
                .prepare("SELECT pkg_id FROM packages WHERE LOWER(name) = LOWER(?)")?;
            let ids: Vec<i64> = stmt
                .query_map([&lower_query], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for id in ids {
                if seen_ids.insert(id) {
                    results.push((id, 1.0));
                }
            }
        }

        // 2. Prefix match (score: 0.85)
        {
            let mut stmt = self
                .conn
                .prepare("SELECT pkg_id FROM packages WHERE LOWER(name) LIKE ? LIMIT 50")?;
            let pattern = format!("{}%", lower_query);
            let ids: Vec<i64> = stmt
                .query_map([&pattern], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for id in ids {
                if seen_ids.insert(id) {
                    results.push((id, 0.85));
                }
            }
        }

        // 3. Contains match on name (score: 0.7)
        {
            let mut stmt = self
                .conn
                .prepare("SELECT pkg_id FROM packages WHERE LOWER(name) LIKE ? LIMIT 50")?;
            let pattern = format!("%{}%", lower_query);
            let ids: Vec<i64> = stmt
                .query_map([&pattern], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for id in ids {
                if seen_ids.insert(id) {
                    results.push((id, 0.7));
                }
            }
        }

        // 4. Summary keyword match (score: 0.5)
        {
            let mut stmt = self
                .conn
                .prepare("SELECT pkg_id FROM packages WHERE LOWER(summary) LIKE ? LIMIT 100")?;
            let pattern = format!("%{}%", lower_query);
            let ids: Vec<i64> = stmt
                .query_map([&pattern], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for id in ids {
                if seen_ids.insert(id) {
                    results.push((id, 0.5));
                }
            }
        }

        // 5. Description keyword match (score: 0.35)
        {
            let mut stmt = self
                .conn
                .prepare("SELECT pkg_id FROM packages WHERE LOWER(description) LIKE ? LIMIT 100")?;
            let pattern = format!("%{}%", lower_query);
            let ids: Vec<i64> = stmt
                .query_map([&pattern], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for id in ids {
                if seen_ids.insert(id) {
                    results.push((id, 0.35));
                }
            }
        }

        // 6. Provides capability match (score: 0.45)
        {
            let mut stmt = self.conn.prepare(
                "SELECT DISTINCT pkg_id FROM provides WHERE LOWER(name) LIKE ? LIMIT 50",
            )?;
            let pattern = format!("%{}%", lower_query);
            let ids: Vec<i64> = stmt
                .query_map([&pattern], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for id in ids {
                if seen_ids.insert(id) {
                    results.push((id, 0.45));
                }
            }
        }

        // Sort by score descending
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        Ok(results)
    }

    /// Get all package IDs
    pub fn get_all_pkg_ids(&self) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare("SELECT pkg_id FROM packages")?;
        let pkg_ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(pkg_ids)
    }

    /// Get package count
    pub fn count_packages(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM packages", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Get package IDs filtered by arch and/or repo (for pre-filtering vector search)
    pub fn get_filtered_pkg_ids(&self, arch: Option<&str>, repo: Option<&str>) -> Result<Vec<i64>> {
        let query = match (arch, repo) {
            (Some(_), Some(_)) => "SELECT pkg_id FROM packages WHERE arch = ? AND repo = ?",
            (Some(_), None) => "SELECT pkg_id FROM packages WHERE arch = ?",
            (None, Some(_)) => "SELECT pkg_id FROM packages WHERE repo = ?",
            (None, None) => "SELECT pkg_id FROM packages",
        };

        let mut stmt = self.conn.prepare(query)?;

        let pkg_ids: Vec<i64> = match (arch, repo) {
            (Some(a), Some(r)) => stmt
                .query_map([a, r], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
            (Some(a), None) => stmt
                .query_map([a], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
            (None, Some(r)) => stmt
                .query_map([r], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
            (None, None) => stmt
                .query_map([], |row| row.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?,
        };

        Ok(pkg_ids)
    }

    /// List all repositories with package counts
    pub fn list_repositories(&self) -> Result<Vec<(String, usize)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT repo, COUNT(*) FROM packages GROUP BY repo ORDER BY repo")?;

        let repos: Vec<(String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(repos
            .into_iter()
            .map(|(repo, count)| (repo, count as usize))
            .collect())
    }

    /// Get package count for a specific repository
    pub fn count_packages_by_repo(&self, repo: &str) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM packages WHERE repo = ?",
            [repo],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    /// Find a package by name, arch, and repo (for incremental updates)
    pub fn find_package(&self, name: &str, arch: &str, repo: &str) -> Result<Option<Package>> {
        let mut stmt = self
            .conn
            .prepare("SELECT pkg_id FROM packages WHERE name = ? AND arch = ? AND repo = ?")?;

        let pkg_id: Option<i64> = stmt
            .query_row([name, arch, repo], |row| row.get(0))
            .optional()?;

        if let Some(id) = pkg_id {
            self.get_package(id)
        } else {
            Ok(None)
        }
    }

    /// Update an existing package (delete old, insert new)
    pub fn update_package(&mut self, old_pkg_id: i64, new_package: &Package) -> Result<i64> {
        let tx = self.conn.transaction()?;

        // Delete old package data
        tx.execute("DELETE FROM requires WHERE pkg_id = ?", [old_pkg_id])?;
        tx.execute("DELETE FROM provides WHERE pkg_id = ?", [old_pkg_id])?;
        // Ignore error if embeddings table doesn't exist
        let _ = tx.execute("DELETE FROM embeddings WHERE pkg_id = ?", [old_pkg_id]);
        tx.execute("DELETE FROM packages WHERE pkg_id = ?", [old_pkg_id])?;

        // Insert new package
        tx.execute(
            "INSERT INTO packages (name, epoch, version, release, arch, summary, description, repo)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                new_package.name,
                new_package.epoch,
                new_package.version,
                new_package.release,
                new_package.arch,
                new_package.summary,
                new_package.description,
                new_package.repo,
            ],
        )?;

        let new_pkg_id = tx.last_insert_rowid();

        // Insert requires
        for req in &new_package.requires {
            tx.execute(
                "INSERT INTO requires (pkg_id, name, flags, version) VALUES (?, ?, ?, ?)",
                params![new_pkg_id, req.name, req.flags, req.version],
            )?;
        }

        // Insert provides
        for prov in &new_package.provides {
            tx.execute(
                "INSERT INTO provides (pkg_id, name, flags, version) VALUES (?, ?, ?, ?)",
                params![new_pkg_id, prov.name, prov.flags, prov.version],
            )?;
        }

        tx.commit()?;
        Ok(new_pkg_id)
    }

    /// Get all packages in a repository (name, arch, version)
    #[allow(clippy::type_complexity)]
    pub fn get_packages_in_repo(
        &self,
        repo: &str,
    ) -> Result<Vec<(String, String, String, String, String)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name, arch, epoch, version, release FROM packages WHERE repo = ?")?;

        let packages = stmt
            .query_map([repo], |row| {
                let epoch: Option<i64> = row.get(2)?;
                Ok((
                    row.get::<_, String>(0)?,       // name
                    row.get::<_, String>(1)?,       // arch
                    epoch.unwrap_or(0).to_string(), // epoch (NULL -> 0)
                    row.get::<_, String>(3)?,       // version
                    row.get::<_, String>(4)?,       // release
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(packages)
    }

    /// Delete a specific package by name, arch, and repo
    pub fn delete_package(&mut self, name: &str, arch: &str, repo: &str) -> Result<bool> {
        if let Some(pkg) = self.find_package(name, arch, repo)? {
            let pkg_id = pkg.pkg_id.unwrap();

            let tx = self.conn.transaction()?;
            tx.execute("DELETE FROM requires WHERE pkg_id = ?", [pkg_id])?;
            tx.execute("DELETE FROM provides WHERE pkg_id = ?", [pkg_id])?;
            // Ignore error if embeddings table doesn't exist
            let _ = tx.execute("DELETE FROM embeddings WHERE pkg_id = ?", [pkg_id]);
            tx.execute("DELETE FROM packages WHERE pkg_id = ?", [pkg_id])?;
            tx.commit()?;

            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Delete all packages from a repository
    pub fn delete_repository(&mut self, repo: &str) -> Result<usize> {
        let tx = self.conn.transaction()?;

        // Get pkg_ids for this repo
        let mut stmt = tx.prepare("SELECT pkg_id FROM packages WHERE repo = ?")?;
        let pkg_ids: Vec<i64> = stmt
            .query_map([repo], |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Drop statement before using tx again
        drop(stmt);

        // Delete related data
        for pkg_id in &pkg_ids {
            tx.execute("DELETE FROM requires WHERE pkg_id = ?", [pkg_id])?;
            tx.execute("DELETE FROM provides WHERE pkg_id = ?", [pkg_id])?;
            // Ignore error if embeddings table doesn't exist
            let _ = tx.execute("DELETE FROM embeddings WHERE pkg_id = ?", [pkg_id]);
        }

        // Delete packages
        let deleted = tx.execute("DELETE FROM packages WHERE repo = ?", [repo])?;

        tx.commit()?;
        Ok(deleted)
    }
}
