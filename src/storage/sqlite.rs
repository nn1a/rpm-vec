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
        Schema::migrate(&conn)?;
        Schema::initialize(&conn)?;
        Ok(Self { conn })
    }

    /// Insert a package and return its pkg_id
    #[allow(dead_code)]
    pub fn insert_package(&mut self, package: &Package) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let pkg_id = Self::insert_package_in_tx(&tx, package)?;
        tx.commit()?;
        Ok(pkg_id)
    }

    /// Insert a single package within an existing transaction
    fn insert_package_in_tx(tx: &rusqlite::Transaction, package: &Package) -> Result<i64> {
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

        for req in &package.requires {
            tx.execute(
                "INSERT INTO requires (pkg_id, name, flags, version) VALUES (?, ?, ?, ?)",
                params![pkg_id, req.name, req.flags, req.version],
            )?;
        }

        for prov in &package.provides {
            tx.execute(
                "INSERT INTO provides (pkg_id, name, flags, version) VALUES (?, ?, ?, ?)",
                params![pkg_id, prov.name, prov.flags, prov.version],
            )?;
        }

        Ok(pkg_id)
    }

    /// Batch insert packages in a single transaction with prepared statements
    pub fn insert_packages_batch(&mut self, packages: &[Package]) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let mut pkg_ids = Vec::with_capacity(packages.len());

        {
            let mut pkg_stmt = tx.prepare_cached(
                "INSERT INTO packages (name, epoch, version, release, arch, summary, description, repo)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )?;
            let mut req_stmt = tx.prepare_cached(
                "INSERT INTO requires (pkg_id, name, flags, version) VALUES (?, ?, ?, ?)",
            )?;
            let mut prov_stmt = tx.prepare_cached(
                "INSERT INTO provides (pkg_id, name, flags, version) VALUES (?, ?, ?, ?)",
            )?;

            for package in packages {
                pkg_stmt.execute(params![
                    package.name,
                    package.epoch,
                    package.version,
                    package.release,
                    package.arch,
                    package.summary,
                    package.description,
                    package.repo,
                ])?;

                let pkg_id = tx.last_insert_rowid();

                for req in &package.requires {
                    req_stmt.execute(params![pkg_id, req.name, req.flags, req.version])?;
                }

                for prov in &package.provides {
                    prov_stmt.execute(params![pkg_id, prov.name, prov.flags, prov.version])?;
                }

                pkg_ids.push(pkg_id);
            }
        }

        tx.commit()?;
        Ok(pkg_ids)
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
    #[allow(dead_code)]
    pub fn update_package(&mut self, old_pkg_id: i64, new_package: &Package) -> Result<i64> {
        let tx = self.conn.transaction()?;
        let new_pkg_id = Self::update_package_in_tx(&tx, old_pkg_id, new_package)?;
        tx.commit()?;
        Ok(new_pkg_id)
    }

    /// Update a package within an existing transaction
    fn update_package_in_tx(
        tx: &rusqlite::Transaction,
        old_pkg_id: i64,
        new_package: &Package,
    ) -> Result<i64> {
        tx.execute("DELETE FROM requires WHERE pkg_id = ?", [old_pkg_id])?;
        tx.execute("DELETE FROM provides WHERE pkg_id = ?", [old_pkg_id])?;
        tx.execute("DELETE FROM files WHERE pkg_id = ?", [old_pkg_id])?;
        let _ = tx.execute("DELETE FROM embeddings WHERE pkg_id = ?", [old_pkg_id]);
        tx.execute("DELETE FROM packages WHERE pkg_id = ?", [old_pkg_id])?;

        let pkg_id = Self::insert_package_in_tx(tx, new_package)?;
        Ok(pkg_id)
    }

    /// Batch incremental update: inserts, updates, deletes in a single transaction
    pub fn batch_incremental_update(
        &mut self,
        inserts: &[Package],
        updates: &[(i64, Package)],
        deletes: &[(String, String, String)],
    ) -> Result<(usize, usize, usize)> {
        let tx = self.conn.transaction()?;

        // Batch inserts
        for package in inserts {
            Self::insert_package_in_tx(&tx, package)?;
        }

        // Batch updates
        for (old_pkg_id, new_package) in updates {
            Self::update_package_in_tx(&tx, *old_pkg_id, new_package)?;
        }

        // Batch deletes
        for (name, arch, repo) in deletes {
            let pkg_id: Option<i64> = tx
                .query_row(
                    "SELECT pkg_id FROM packages WHERE name = ? AND arch = ? AND repo = ?",
                    params![name, arch, repo],
                    |row| row.get(0),
                )
                .optional()?;

            if let Some(id) = pkg_id {
                tx.execute("DELETE FROM requires WHERE pkg_id = ?", [id])?;
                tx.execute("DELETE FROM provides WHERE pkg_id = ?", [id])?;
                tx.execute("DELETE FROM files WHERE pkg_id = ?", [id])?;
                let _ = tx.execute("DELETE FROM embeddings WHERE pkg_id = ?", [id]);
                tx.execute("DELETE FROM packages WHERE pkg_id = ?", [id])?;
            }
        }

        tx.commit()?;
        Ok((inserts.len(), updates.len(), deletes.len()))
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
    #[allow(dead_code)]
    pub fn delete_package(&mut self, name: &str, arch: &str, repo: &str) -> Result<bool> {
        if let Some(pkg) = self.find_package(name, arch, repo)? {
            let pkg_id = pkg.pkg_id.unwrap();

            let tx = self.conn.transaction()?;
            tx.execute("DELETE FROM requires WHERE pkg_id = ?", [pkg_id])?;
            tx.execute("DELETE FROM provides WHERE pkg_id = ?", [pkg_id])?;
            tx.execute("DELETE FROM files WHERE pkg_id = ?", [pkg_id])?;
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
            tx.execute("DELETE FROM files WHERE pkg_id = ?", [pkg_id])?;
            let _ = tx.execute("DELETE FROM embeddings WHERE pkg_id = ?", [pkg_id]);
        }

        // Delete packages
        let deleted = tx.execute("DELETE FROM packages WHERE repo = ?", [repo])?;

        tx.commit()?;
        Ok(deleted)
    }

    // ── Filelists methods ───────────────────────────────────────────────

    /// Find a package by NEVRA (name, epoch, version, release, arch) + repo.
    /// Used for matching filelists.xml entries to existing packages.
    pub fn find_package_by_nevra(
        &self,
        name: &str,
        arch: &str,
        epoch: Option<i64>,
        version: &str,
        release: &str,
        repo: &str,
    ) -> Result<Option<i64>> {
        let epoch_val = epoch.unwrap_or(0);
        let mut stmt = self.conn.prepare_cached(
            "SELECT pkg_id FROM packages
             WHERE name = ? AND arch = ? AND version = ? AND release = ? AND repo = ?
             AND COALESCE(epoch, 0) = ?",
        )?;

        let pkg_id: Option<i64> = stmt
            .query_row(
                params![name, arch, version, release, repo, epoch_val],
                |row| row.get(0),
            )
            .optional()?;

        Ok(pkg_id)
    }

    /// Batch insert file lists for multiple packages.
    /// `entries`: Vec of (pkg_id, Vec<(path, file_type_int)>).
    pub fn insert_filelists_batch(
        &mut self,
        entries: &[(i64, Vec<(String, i32)>)],
    ) -> Result<usize> {
        use std::collections::HashMap;

        let tx = self.conn.transaction()?;
        let mut count = 0;

        {
            let mut dir_cache: HashMap<String, i64> = HashMap::new();

            // Pre-load existing directories into cache
            {
                let mut dir_stmt = tx.prepare("SELECT dir_id, path FROM directories")?;
                let rows = dir_stmt.query_map([], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                })?;
                for row in rows {
                    let (dir_id, path) = row?;
                    dir_cache.insert(path, dir_id);
                }
            }

            let mut dir_insert_stmt =
                tx.prepare_cached("INSERT OR IGNORE INTO directories (path) VALUES (?)")?;
            let mut dir_lookup_stmt =
                tx.prepare_cached("SELECT dir_id FROM directories WHERE path = ?")?;
            let mut file_stmt = tx.prepare_cached(
                "INSERT INTO files (pkg_id, dir_id, name, file_type) VALUES (?, ?, ?, ?)",
            )?;

            for (pkg_id, files) in entries {
                for (path, file_type) in files {
                    let is_dir = *file_type == 1; // RpmFileType::Dir
                    let (dir_path, file_name) = split_path(path, is_dir);

                    let dir_id = if let Some(&cached_id) = dir_cache.get(dir_path) {
                        cached_id
                    } else {
                        dir_insert_stmt.execute(params![dir_path])?;
                        let id: i64 =
                            dir_lookup_stmt.query_row(params![dir_path], |row| row.get(0))?;
                        dir_cache.insert(dir_path.to_string(), id);
                        id
                    };

                    file_stmt.execute(params![pkg_id, dir_id, file_name, file_type])?;
                    count += 1;
                }
            }
        }

        tx.commit()?;
        Ok(count)
    }

    /// Search for packages that provide a specific file path.
    /// If `path` contains '/', splits into dir+name for exact lookup.
    /// Otherwise searches by filename only.
    pub fn search_by_file_path(&self, path: &str) -> Result<Vec<(i64, String, i32)>> {
        if path.contains('/') {
            let (dir_path, file_name) = if path.ends_with('/') {
                // Directory query
                (path.trim_end_matches('/'), "")
            } else {
                split_path(path, false)
            };

            let mut stmt = self.conn.prepare(
                "SELECT f.pkg_id, d.path, f.name, f.file_type
                 FROM files f
                 JOIN directories d ON f.dir_id = d.dir_id
                 WHERE d.path = ? AND f.name = ?
                 ORDER BY f.pkg_id",
            )?;

            let results = stmt
                .query_map(params![dir_path, file_name], |row| {
                    let dir: String = row.get(1)?;
                    let name: String = row.get(2)?;
                    let full = if name.is_empty() {
                        dir
                    } else {
                        format!("{}/{}", dir, name)
                    };
                    Ok((row.get(0)?, full, row.get(3)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(results)
        } else {
            // Filename-only search
            let mut stmt = self.conn.prepare(
                "SELECT f.pkg_id, d.path, f.name, f.file_type
                 FROM files f
                 JOIN directories d ON f.dir_id = d.dir_id
                 WHERE f.name = ?
                 ORDER BY f.pkg_id
                 LIMIT 200",
            )?;

            let results = stmt
                .query_map(params![path], |row| {
                    let dir: String = row.get(1)?;
                    let name: String = row.get(2)?;
                    let full = format!("{}/{}", dir, name);
                    Ok((row.get(0)?, full, row.get(3)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            Ok(results)
        }
    }

    /// List all files belonging to a package
    pub fn get_files_for_package(&self, pkg_id: i64) -> Result<Vec<(String, i32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT d.path, f.name, f.file_type
             FROM files f
             JOIN directories d ON f.dir_id = d.dir_id
             WHERE f.pkg_id = ?
             ORDER BY d.path, f.name",
        )?;

        let results = stmt
            .query_map(params![pkg_id], |row| {
                let dir: String = row.get(0)?;
                let name: String = row.get(1)?;
                let full = if name.is_empty() {
                    dir
                } else {
                    format!("{}/{}", dir, name)
                };
                Ok((full, row.get(2)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(results)
    }

    /// Check if filelists have been indexed for a given repository
    #[allow(dead_code)]
    pub fn has_filelists(&self, repo: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM files f
             JOIN packages p ON f.pkg_id = p.pkg_id
             WHERE p.repo = ?
             LIMIT 1",
            [repo],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    /// Get total file count
    pub fn count_files(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    /// Get total directory count
    pub fn count_directories(&self) -> Result<usize> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM directories", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    // ── General search ──────────────────────────────────────────────────

    /// General-purpose search with multiple optional filters.
    /// All provided filters are ANDed together.
    /// Wildcards: `*` → `%`, `?` → `_`. No wildcards → contains match.
    pub fn general_search(&self, filter: &FindFilter) -> Result<Vec<i64>> {
        let mut conditions = Vec::new();
        let mut bind_values: Vec<String> = Vec::new();

        // Core filters on packages table
        if let Some(ref name) = filter.name {
            conditions.push("p.name LIKE ?".to_string());
            bind_values.push(wildcard_to_like(name));
        }
        if let Some(ref summary) = filter.summary {
            conditions.push("p.summary LIKE ?".to_string());
            bind_values.push(wildcard_to_like(summary));
        }
        if let Some(ref description) = filter.description {
            conditions.push("p.description LIKE ?".to_string());
            bind_values.push(wildcard_to_like(description));
        }
        if let Some(ref arch) = filter.arch {
            conditions.push("p.arch = ?".to_string());
            bind_values.push(arch.clone());
        }
        if let Some(ref repo) = filter.repo {
            conditions.push("p.repo = ?".to_string());
            bind_values.push(repo.clone());
        }

        // Subquery filters
        if let Some(ref provides) = filter.provides {
            conditions.push(
                "EXISTS (SELECT 1 FROM provides pv WHERE pv.pkg_id = p.pkg_id AND pv.name LIKE ?)"
                    .to_string(),
            );
            bind_values.push(wildcard_to_like(provides));
        }
        if let Some(ref requires) = filter.requires {
            conditions.push(
                "EXISTS (SELECT 1 FROM requires rq WHERE rq.pkg_id = p.pkg_id AND rq.name LIKE ?)"
                    .to_string(),
            );
            bind_values.push(wildcard_to_like(requires));
        }
        if let Some(ref file) = filter.file {
            let like_pattern = wildcard_to_like(file);
            // Use subquery with directory+filename join
            conditions.push(
                "EXISTS (SELECT 1 FROM files f JOIN directories d ON f.dir_id = d.dir_id \
                 WHERE f.pkg_id = p.pkg_id AND (d.path || '/' || f.name) LIKE ?)"
                    .to_string(),
            );
            bind_values.push(like_pattern);
        }

        if conditions.is_empty() {
            return Ok(Vec::new());
        }

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            "SELECT DISTINCT p.pkg_id FROM packages p WHERE {} ORDER BY p.name LIMIT ?",
            where_clause
        );
        bind_values.push(filter.limit.to_string());

        let mut stmt = self.conn.prepare(&sql)?;

        // Bind all parameters dynamically
        let params: Vec<&dyn rusqlite::types::ToSql> = bind_values
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();

        let pkg_ids: Vec<i64> = stmt
            .query_map(params.as_slice(), |row| row.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(pkg_ids)
    }
}

/// Search filter for general-purpose package search.
/// All provided fields are ANDed together.
#[derive(Debug)]
pub struct FindFilter {
    /// Package name pattern (supports `*` and `?` wildcards)
    pub name: Option<String>,
    /// Summary text pattern
    pub summary: Option<String>,
    /// Description text pattern
    pub description: Option<String>,
    /// Provides capability pattern
    pub provides: Option<String>,
    /// Requires dependency pattern
    pub requires: Option<String>,
    /// File path pattern (searches in filelists)
    pub file: Option<String>,
    /// Exact architecture match
    pub arch: Option<String>,
    /// Exact repository match
    pub repo: Option<String>,
    /// Maximum results (default 50)
    pub limit: usize,
}

impl Default for FindFilter {
    fn default() -> Self {
        Self {
            name: None,
            summary: None,
            description: None,
            provides: None,
            requires: None,
            file: None,
            arch: None,
            repo: None,
            limit: 50,
        }
    }
}

/// Convert user wildcard pattern to SQL LIKE pattern.
/// `*` → `%`, `?` → `_`.
/// If no wildcards present, wraps with `%` for contains match.
fn wildcard_to_like(pattern: &str) -> String {
    // First escape any literal SQL LIKE special chars in the input
    let escaped = pattern.replace('%', "\\%").replace('_', "\\_");

    if pattern.contains('*') || pattern.contains('?') {
        // Convert user wildcards to SQL LIKE wildcards
        escaped.replace('*', "%").replace('?', "_")
    } else {
        // No wildcards: default to contains match
        format!("%{}%", escaped)
    }
}

/// Split a file path into (directory, filename).
/// `/usr/bin/bash` -> (`/usr/bin`, `bash`)
/// `/etc/nginx` with is_dir=true -> (`/etc/nginx`, ``)
fn split_path(path: &str, is_dir: bool) -> (&str, &str) {
    if is_dir || path == "/" {
        (path, "")
    } else {
        match path.rfind('/') {
            Some(0) => ("/", &path[1..]),
            Some(pos) => (&path[..pos], &path[pos + 1..]),
            None => ("/", path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_path_regular() {
        assert_eq!(split_path("/usr/bin/bash", false), ("/usr/bin", "bash"));
        assert_eq!(
            split_path("/usr/lib64/libssl.so.3", false),
            ("/usr/lib64", "libssl.so.3")
        );
    }

    #[test]
    fn test_split_path_root() {
        assert_eq!(split_path("/bash", false), ("/", "bash"));
        assert_eq!(split_path("/", false), ("/", ""));
    }

    #[test]
    fn test_split_path_dir() {
        assert_eq!(split_path("/etc/nginx", true), ("/etc/nginx", ""));
        assert_eq!(
            split_path("/usr/share/locale", true),
            ("/usr/share/locale", "")
        );
    }

    #[test]
    fn test_wildcard_to_like() {
        // No wildcards → contains match
        assert_eq!(wildcard_to_like("ssl"), "%ssl%");
        assert_eq!(wildcard_to_like("python3"), "%python3%");

        // Wildcards
        assert_eq!(wildcard_to_like("lib*ssl*"), "lib%ssl%");
        assert_eq!(wildcard_to_like("python?.?"), "python_._");
        assert_eq!(wildcard_to_like("*"), "%");

        // SQL special chars escaped
        assert_eq!(wildcard_to_like("100%"), "%100\\%%");
        assert_eq!(wildcard_to_like("a_b"), "%a\\_b%");
    }
}
