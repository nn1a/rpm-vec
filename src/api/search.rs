use crate::config::Config;
use crate::embedding::Embedder;
use crate::error::Result;
use crate::normalize::Package;
use crate::repomd::fetch::RepoFetcher;
use crate::repomd::filelists_parser::FilelistsXmlParser;
use crate::repomd::model::RpmPackage;
use crate::repomd::parser::PrimaryXmlParser;
use crate::search::{
    QueryPlanner, SearchFilters, SearchQuery, SearchResult, SemanticSearch, StructuredSearch,
};
use crate::storage::{FindFilter, PackageStore, VectorStore};
use rusqlite::Connection;
use std::path::Path;
use tracing::{debug, info, instrument, warn};

pub struct RpmSearchApi {
    config: Config,
    package_store: PackageStore,
}

impl RpmSearchApi {
    /// Create a new API instance
    pub fn new(config: Config) -> Result<Self> {
        let package_store = PackageStore::new(&config.db_path)?;
        Ok(Self {
            config,
            package_store,
        })
    }

    /// Index a repository from primary.xml file
    #[instrument(skip(self, primary_xml_path), fields(path = %primary_xml_path.as_ref().display(), repo = %repo_name, update))]
    pub fn index_repository<P: AsRef<Path>>(
        &mut self,
        primary_xml_path: P,
        repo_name: &str,
        update: bool,
    ) -> Result<usize> {
        debug!("Fetching local file");
        // Fetch and parse primary.xml
        let data = RepoFetcher::fetch_local(&primary_xml_path)?;

        debug!("Decompressing data");
        // Auto-detect and decompress (supports .gz, .zst)
        let xml_data = RepoFetcher::auto_decompress(&primary_xml_path, &data)?;

        debug!("Parsing XML");
        let rpm_packages = PrimaryXmlParser::parse(&xml_data[..])?;

        info!(
            package_count = rpm_packages.len(),
            update, "Parsed RPM packages"
        );

        if update {
            self.update_repository_packages(rpm_packages, repo_name)
        } else {
            // Convert all packages first, then batch insert
            let packages: Vec<Package> = rpm_packages
                .into_iter()
                .map(|rpm_pkg| Package::from_rpm_package(rpm_pkg, repo_name.to_string()))
                .collect();

            let count = packages.len();
            self.package_store.insert_packages_batch(&packages)?;

            debug!(inserted = count, "Stored packages in database");

            Ok(count)
        }
    }

    /// Update repository with incremental changes (single transaction)
    #[instrument(skip(self, rpm_packages), fields(repo = %repo_name, package_count = rpm_packages.len()))]
    fn update_repository_packages(
        &mut self,
        rpm_packages: Vec<RpmPackage>,
        repo_name: &str,
    ) -> Result<usize> {
        use std::collections::{HashMap, HashSet};

        info!("Starting incremental update");

        // Get existing packages in the repository
        let existing = self.package_store.get_packages_in_repo(repo_name)?;
        let mut existing_map: HashMap<(String, String), (String, String, String)> = HashMap::new();

        for (name, arch, epoch, version, release) in existing {
            existing_map.insert((name.clone(), arch.clone()), (epoch, version, release));
        }

        debug!(
            existing_count = existing_map.len(),
            "Loaded existing packages"
        );

        let mut new_package_set = HashSet::new();
        let mut inserts: Vec<Package> = Vec::new();
        let mut updates: Vec<(i64, Package)> = Vec::new();

        // Classify packages: insert vs update vs skip
        for rpm_pkg in rpm_packages {
            let package = Package::from_rpm_package(rpm_pkg.clone(), repo_name.to_string());
            let key = (package.name.clone(), package.arch.clone());

            new_package_set.insert(key.clone());

            if let Some((old_epoch, old_version, old_release)) = existing_map.get(&key) {
                let old_epoch_num: i64 = old_epoch.parse().unwrap_or(0);
                let new_epoch_num = rpm_pkg.epoch.unwrap_or(0);

                if old_epoch_num != new_epoch_num
                    || old_version != &package.version
                    || old_release != &package.release
                {
                    if let Some(old_pkg) =
                        self.package_store.find_package(&key.0, &key.1, repo_name)?
                    {
                        debug!(
                            package = %key.0,
                            arch = %key.1,
                            old_version = format!("{}:{}-{}", old_epoch, old_version, old_release),
                            new_version = %package.full_version(),
                            "Updating package"
                        );
                        updates.push((old_pkg.pkg_id.unwrap(), package));
                    }
                }
            } else {
                debug!(package = %package.name, arch = %package.arch, "Adding new package");
                inserts.push(package);
            }
        }

        // Collect packages to delete
        let deletes: Vec<(String, String, String)> = existing_map
            .iter()
            .filter(|(key, _)| !new_package_set.contains(key))
            .map(|(key, _)| {
                debug!(package = %key.0, arch = %key.1, "Removing deleted package");
                (key.0.clone(), key.1.clone(), repo_name.to_string())
            })
            .collect();

        let added = inserts.len();
        let updated = updates.len();
        let removed = deletes.len();

        // Execute all changes in a single transaction
        self.package_store
            .batch_incremental_update(&inserts, &updates, &deletes)?;

        info!(
            added,
            updated,
            removed,
            total = added + updated,
            "Incremental update completed"
        );

        Ok(added + updated)
    }

    /// Build embeddings for packages
    ///
    /// - `rebuild = false` (default): incremental — only builds for packages missing embeddings
    /// - `rebuild = true`: full rebuild — drops all embeddings and regenerates from scratch
    ///
    /// Model mismatch protection: if the DB was built with a different model type,
    /// incremental builds are rejected (must use `--rebuild`).
    #[instrument(skip(self, embedder), fields(verbose, rebuild))]
    pub fn build_embeddings(
        &self,
        embedder: &Embedder,
        verbose: bool,
        rebuild: bool,
    ) -> Result<usize> {
        use std::collections::HashSet;

        let conn = Connection::open(&self.config.db_path)?;
        let vector_store = VectorStore::new(conn)?;

        // Check model mismatch (only for incremental builds)
        let requested_type = embedder.model_type();
        if !rebuild {
            if let Some(db_type_str) = vector_store.get_embedding_model_type()? {
                if db_type_str != requested_type.as_db_str() {
                    return Err(crate::error::RpmSearchError::Embedding(format!(
                        "Model mismatch: existing embeddings were built with '{}', \
                         but '{}' was requested.\n\
                         Use --rebuild to drop existing embeddings and regenerate with the new model.",
                        db_type_str,
                        requested_type.as_db_str()
                    )));
                }
            }
        }

        let (pkg_ids, label) = if rebuild {
            // Full rebuild: drop + recreate
            if verbose {
                println!("✓ Full rebuild mode — dropping existing embeddings");
            }
            vector_store.reinitialize(self.config.embedding_dim)?;

            let ids = self.package_store.get_all_pkg_ids()?;
            let total = ids.len();
            info!(total, "Starting full embedding rebuild");
            if verbose {
                println!("Total packages to process: {}", total);
            }
            (ids, "packages")
        } else {
            // Incremental: only missing
            vector_store.ensure_table(self.config.embedding_dim)?;

            let all_ids: HashSet<i64> = self.package_store.get_all_pkg_ids()?.into_iter().collect();
            let existing_ids: HashSet<i64> = vector_store
                .get_embedded_pkg_ids()
                .unwrap_or_default()
                .into_iter()
                .collect();

            let missing: Vec<i64> = all_ids.difference(&existing_ids).copied().collect();

            if missing.is_empty() {
                info!("All packages already have embeddings, nothing to do");
                if verbose {
                    println!("✓ All packages already have embeddings");
                }
                return Ok(0);
            }

            let total = missing.len();
            info!(
                total_missing = total,
                total_existing = existing_ids.len(),
                "Starting incremental embedding generation"
            );
            if verbose {
                println!(
                    "Packages needing embeddings: {} (existing: {})",
                    total,
                    existing_ids.len()
                );
            }
            (missing, "new packages")
        };

        let total = pkg_ids.len();
        let batch_size = self.config.batch_size;
        let mut count = 0;
        let total_batches = total.div_ceil(batch_size);

        if verbose {
            println!("Batch size: {}", batch_size);
            println!();
        }

        for (batch_idx, chunk) in pkg_ids.chunks(batch_size).enumerate() {
            let mut texts = Vec::new();
            let mut ids = Vec::new();

            for &pkg_id in chunk {
                if let Some(pkg) = self.package_store.get_package(pkg_id)? {
                    texts.push(pkg.build_embedding_text());
                    ids.push(pkg_id);
                }
            }

            debug!(
                batch = batch_idx + 1,
                packages = texts.len(),
                "Generating embeddings for batch"
            );
            let embeddings = embedder.embed_passages(&texts)?;

            // Batch insert embeddings in a single transaction
            let batch_items: Vec<(i64, Vec<f32>)> = ids
                .iter()
                .zip(embeddings.iter())
                .map(|(&id, emb)| (id, emb.clone()))
                .collect();
            vector_store.insert_embeddings_batch(&batch_items)?;
            count += batch_items.len();

            debug!(batch = batch_idx + 1, total = count, "Stored embeddings");

            if verbose {
                println!(
                    "Batch {}/{}: Processed {} packages → Total: {}/{} ({:.1}%)",
                    batch_idx + 1,
                    total_batches,
                    texts.len(),
                    count,
                    total,
                    (count as f64 / total as f64) * 100.0
                );
            } else {
                print!(
                    "\rProcessing: {}/{} {} ({:.1}%)...",
                    count,
                    total,
                    label,
                    (count as f64 / total as f64) * 100.0
                );
                std::io::Write::flush(&mut std::io::stdout()).ok();
            }
        }

        if !verbose {
            println!();
        }

        info!(total_embeddings = count, "Completed embedding generation");

        // Record model info in DB metadata
        vector_store.set_embedding_model_info(requested_type)?;
        info!(model = %requested_type, "Saved embedding model info to DB");

        Ok(count)
    }

    /// Search packages
    #[instrument(skip(self, query, filters), fields(query = %query, top_k = self.config.top_k))]
    pub fn search(&self, query: &str, filters: SearchFilters) -> Result<Vec<Package>> {
        let result = self.search_with_scores(query, filters)?;
        Ok(result.packages)
    }

    /// Search packages with scores
    ///
    /// Auto-detects the embedding model type from DB metadata if available,
    /// falling back to the config default.
    #[instrument(skip(self, query, filters), fields(query = %query, top_k = self.config.top_k))]
    pub fn search_with_scores(&self, query: &str, filters: SearchFilters) -> Result<SearchResult> {
        debug!("Creating embedder and vector store");

        let conn = Connection::open(&self.config.db_path)?;
        let vector_store = VectorStore::new(conn)?;

        // Auto-detect model type from DB metadata
        let model_type = if let Some(db_type_str) = vector_store.get_embedding_model_type()? {
            crate::config::ModelType::from_db_str(&db_type_str)
                .unwrap_or_else(|| self.config.model_type.clone())
        } else {
            self.config.model_type.clone()
        };

        let model_path = model_type.default_model_path();
        let tokenizer_path = model_type.default_tokenizer_path();

        // Create embedder with the detected model type
        let embedder = Embedder::new(&model_path, &tokenizer_path, model_type)?;

        debug!("Initializing search components");
        let semantic_search = SemanticSearch::new(vector_store, embedder);
        let structured_search = StructuredSearch::new(&self.package_store);
        let planner = QueryPlanner::new(semantic_search, structured_search, self.config.top_k);

        let search_query = SearchQuery {
            query_text: query.to_string(),
            filters,
            top_k: Some(self.config.top_k),
        };

        debug!("Executing hybrid search");
        let result = planner.search(search_query)?;

        info!(results = result.packages.len(), "Search completed");

        Ok(result)
    }

    /// Get package count
    pub fn package_count(&self) -> Result<usize> {
        self.package_store.count_packages()
    }

    /// List all repositories
    pub fn list_repositories(&self) -> Result<Vec<(String, usize)>> {
        self.package_store.list_repositories()
    }

    /// Get package count for a specific repository
    pub fn repo_package_count(&self, repo: &str) -> Result<usize> {
        self.package_store.count_packages_by_repo(repo)
    }

    /// Delete a repository and all its packages
    pub fn delete_repository(&mut self, repo: &str) -> Result<usize> {
        self.package_store.delete_repository(repo)
    }

    /// Search by name only
    #[allow(dead_code)]
    pub fn search_by_name(&self, name: &str) -> Result<Vec<Package>> {
        self.package_store.search_by_name(name)
    }

    // ── Filelists methods ───────────────────────────────────────────────

    /// Index filelists from filelists.xml file for an existing repository.
    /// Packages must already be indexed from primary.xml.
    #[instrument(skip(self, filelists_path), fields(path = %filelists_path.as_ref().display(), repo = %repo_name))]
    pub fn index_filelists<P: AsRef<Path>>(
        &mut self,
        filelists_path: P,
        repo_name: &str,
    ) -> Result<usize> {
        debug!("Fetching filelists file");
        let data = RepoFetcher::fetch_local(&filelists_path)?;

        debug!("Decompressing filelists data");
        let xml_data = RepoFetcher::auto_decompress(&filelists_path, &data)?;

        debug!("Parsing filelists XML");
        let fl_packages = FilelistsXmlParser::parse(&xml_data[..])?;

        info!(
            filelists_count = fl_packages.len(),
            "Parsed filelists packages"
        );

        // Match filelists packages to existing pkg_ids
        let mut entries: Vec<(i64, Vec<(String, i32)>)> = Vec::new();
        let mut matched = 0usize;
        let mut unmatched = 0usize;

        for fl_pkg in &fl_packages {
            let pkg_id = self.package_store.find_package_by_nevra(
                &fl_pkg.name,
                &fl_pkg.arch,
                fl_pkg.epoch,
                &fl_pkg.version,
                &fl_pkg.release,
                repo_name,
            )?;

            if let Some(id) = pkg_id {
                let files: Vec<(String, i32)> = fl_pkg
                    .files
                    .iter()
                    .map(|f| (f.path.clone(), f.file_type.as_i32()))
                    .collect();
                entries.push((id, files));
                matched += 1;
            } else {
                debug!(
                    name = %fl_pkg.name,
                    arch = %fl_pkg.arch,
                    version = %fl_pkg.version,
                    "Filelists package not found in indexed packages"
                );
                unmatched += 1;
            }
        }

        info!(matched, unmatched, "Filelists package matching completed");

        if entries.is_empty() {
            warn!("No filelists packages matched existing indexed packages");
            return Ok(0);
        }

        // Batch insert in chunks
        let batch_size = 500;
        let mut total_files = 0;

        for chunk in entries.chunks(batch_size) {
            let count = self.package_store.insert_filelists_batch(chunk)?;
            total_files += count;
        }

        info!(total_files, "Successfully indexed file entries");
        Ok(total_files)
    }

    /// Search for packages providing a specific file
    pub fn search_file(&self, path: &str) -> Result<Vec<(Package, String, String)>> {
        let results = self.package_store.search_by_file_path(path)?;

        let mut output = Vec::new();
        let mut seen_pkg_ids = std::collections::HashSet::new();

        for (pkg_id, full_path, file_type) in results {
            if !seen_pkg_ids.insert(pkg_id) {
                continue;
            }
            if let Some(pkg) = self.package_store.get_package(pkg_id)? {
                let type_str = match file_type {
                    1 => "dir".to_string(),
                    2 => "ghost".to_string(),
                    _ => "file".to_string(),
                };
                output.push((pkg, full_path, type_str));
            }
        }

        Ok(output)
    }

    /// List files for a specific package
    #[allow(clippy::type_complexity)]
    pub fn list_package_files(
        &self,
        name: &str,
        arch: Option<&str>,
        repo: Option<&str>,
    ) -> Result<Vec<(Package, Vec<(String, String)>)>> {
        let packages = self.package_store.search_by_name(name)?;

        let mut results = Vec::new();
        for pkg in packages {
            if let Some(a) = arch {
                if pkg.arch != a {
                    continue;
                }
            }
            if let Some(r) = repo {
                if pkg.repo != r {
                    continue;
                }
            }

            if let Some(id) = pkg.pkg_id {
                let files = self.package_store.get_files_for_package(id)?;
                let typed_files: Vec<(String, String)> = files
                    .into_iter()
                    .map(|(path, ft)| {
                        let type_str = match ft {
                            1 => "dir".to_string(),
                            2 => "ghost".to_string(),
                            _ => "file".to_string(),
                        };
                        (path, type_str)
                    })
                    .collect();
                results.push((pkg, typed_files));
            }
        }

        Ok(results)
    }

    /// Check if filelists have been indexed for a repository
    #[allow(dead_code)]
    pub fn has_filelists(&self, repo: &str) -> Result<bool> {
        self.package_store.has_filelists(repo)
    }

    /// Get file count
    pub fn file_count(&self) -> Result<usize> {
        self.package_store.count_files()
    }

    /// Get directory count
    pub fn directory_count(&self) -> Result<usize> {
        self.package_store.count_directories()
    }

    // ── General search ──────────────────────────────────────────────────

    /// General-purpose structured search with multiple filters and wildcard support.
    /// Returns matching packages ordered by name.
    pub fn find(&self, filter: &FindFilter) -> Result<Vec<Package>> {
        let pkg_ids = self.package_store.general_search(filter)?;

        let mut packages = Vec::new();
        for pkg_id in pkg_ids {
            if let Some(pkg) = self.package_store.get_package(pkg_id)? {
                packages.push(pkg);
            }
        }

        Ok(packages)
    }
}
