use crate::config::Config;
use crate::embedding::Embedder;
use crate::error::Result;
use crate::normalize::Package;
use crate::repomd::fetch::RepoFetcher;
use crate::repomd::model::RpmPackage;
use crate::repomd::parser::PrimaryXmlParser;
use crate::search::{
    QueryPlanner, SearchFilters, SearchQuery, SearchResult, SemanticSearch, StructuredSearch,
};
use crate::storage::{PackageStore, VectorStore};
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
            // Convert and store packages
            let mut count = 0;
            for rpm_pkg in rpm_packages {
                let package = Package::from_rpm_package(rpm_pkg, repo_name.to_string());
                self.package_store.insert_package(&package)?;
                count += 1;
            }

            debug!(inserted = count, "Stored packages in database");

            Ok(count)
        }
    }

    /// Update repository with incremental changes
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

        let mut new_packages = HashSet::new();
        let mut added = 0;
        let mut updated = 0;

        // Process new/updated packages
        for rpm_pkg in rpm_packages {
            let package = Package::from_rpm_package(rpm_pkg.clone(), repo_name.to_string());
            let key = (package.name.clone(), package.arch.clone());

            new_packages.insert(key.clone());

            if let Some((old_epoch, old_version, old_release)) = existing_map.get(&key) {
                // Package exists, check if version changed
                let old_epoch_num: i64 = old_epoch.parse().unwrap_or(0);
                let new_epoch_num = rpm_pkg.epoch.unwrap_or(0);

                if old_epoch_num != new_epoch_num
                    || old_version != &package.version
                    || old_release != &package.release
                {
                    // Version changed, update package
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
                        self.package_store
                            .update_package(old_pkg.pkg_id.unwrap(), &package)?;
                        updated += 1;
                    }
                }
                // else: same version, skip
            } else {
                // New package
                debug!(package = %package.name, arch = %package.arch, "Adding new package");
                self.package_store.insert_package(&package)?;
                added += 1;
            }
        }

        // Find and remove packages that no longer exist
        let mut removed = 0;
        for (key, _) in existing_map.iter() {
            if !new_packages.contains(key) {
                debug!(package = %key.0, arch = %key.1, "Removing deleted package");
                if self
                    .package_store
                    .delete_package(&key.0, &key.1, repo_name)?
                {
                    removed += 1;
                }
            }
        }

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
            let embeddings = embedder.embed_batch(&texts)?;

            for (pkg_id, embedding) in ids.iter().zip(embeddings.iter()) {
                vector_store.insert_embedding(*pkg_id, embedding)?;
                count += 1;
            }

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

        Ok(count)
    }

    /// Search packages
    #[instrument(skip(self, query, filters), fields(query = %query, top_k = self.config.top_k))]
    pub fn search(&self, query: &str, filters: SearchFilters) -> Result<Vec<Package>> {
        let result = self.search_with_scores(query, filters)?;
        Ok(result.packages)
    }

    /// Search packages with scores
    #[instrument(skip(self, query, filters), fields(query = %query, top_k = self.config.top_k))]
    pub fn search_with_scores(&self, query: &str, filters: SearchFilters) -> Result<SearchResult> {
        debug!("Creating embedder and vector store");
        // Create embedder and vector store
        let embedder = Embedder::new(&self.config.model_path, &self.config.tokenizer_path)?;
        let conn = Connection::open(&self.config.db_path)?;
        let vector_store = VectorStore::new(conn)?;

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
}
