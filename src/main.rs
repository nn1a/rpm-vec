mod api;
mod config;
mod embedding;
mod error;
mod gbs;
mod mcp;
mod normalize;
mod repomd;
mod search;
mod storage;
mod sync;

use clap::{Parser, Subcommand};
use config::{Config, ModelType};
use error::Result;
use normalize::Package;
use search::SearchFilters;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use storage::FindFilter;
use tracing::info;

#[derive(Parser)]
#[command(name = "rpm-search")]
#[command(about = "RPM Repository Vector Search Tool", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Database file path
    #[arg(short, long, default_value = "rpm_search.db")]
    db: PathBuf,
}

#[derive(Subcommand)]
enum SyncCommands {
    /// Generate example sync configuration file
    Init {
        /// Output file path
        #[arg(short, long, default_value = "sync-config.toml")]
        output: PathBuf,

        /// Generate config from GBS configuration file instead of example
        #[arg(long, value_name = "PATH")]
        from_gbs: Option<PathBuf>,

        /// GBS profile to use (default: from gbs.conf [general] section)
        #[arg(long, requires = "from_gbs")]
        gbs_profile: Option<String>,
    },

    /// Perform one-time sync of all repositories
    Once {
        /// Sync configuration file (TOML format)
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Use GBS configuration file instead of sync config
        #[arg(long, value_name = "PATH", conflicts_with = "config")]
        gbs_conf: Option<PathBuf>,

        /// GBS profile to use (default: from gbs.conf [general] section)
        #[arg(long, requires = "gbs_conf")]
        gbs_profile: Option<String>,
    },

    /// Run sync daemon (continuous background syncing)
    Daemon {
        /// Sync configuration file (TOML format)
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Use GBS configuration file instead of sync config
        #[arg(long, value_name = "PATH", conflicts_with = "config")]
        gbs_conf: Option<PathBuf>,

        /// GBS profile to use (default: from gbs.conf [general] section)
        #[arg(long, requires = "gbs_conf")]
        gbs_profile: Option<String>,
    },

    /// Show sync status for all repositories
    Status,
}

#[derive(Subcommand)]
enum RepoCommands {
    /// List all indexed repositories
    List,

    /// Show repository statistics
    Stats {
        /// Repository name
        repo: String,
    },

    /// Delete a repository and all its packages
    Delete {
        /// Repository name
        repo: String,

        /// Confirm deletion
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum Commands {
    // ── Indexing ─────────────────────────────────────────────────────
    /// Index a repository from primary.xml file
    Index {
        /// Path to primary.xml, primary.xml.gz, or primary.xml.zst
        #[arg(short, long)]
        file: PathBuf,

        /// Repository name
        #[arg(short, long)]
        repo: String,

        /// Update existing repository (incremental update)
        #[arg(short, long)]
        update: bool,

        /// Path to filelists.xml (optional, will index file lists after primary.xml)
        #[arg(long)]
        filelists: Option<PathBuf>,
    },

    /// Index filelists from filelists.xml file (run after 'index')
    IndexFilelists {
        /// Path to filelists.xml, filelists.xml.gz, or filelists.xml.zst
        #[arg(short, long)]
        file: PathBuf,

        /// Repository name (must match the repo used in 'index')
        #[arg(short, long)]
        repo: String,
    },

    /// Build embeddings for indexed packages
    BuildEmbeddings {
        /// Embedding model type (minilm = English, e5-multilingual = 100 languages)
        #[arg(long, value_enum, default_value = "minilm")]
        model_type: ModelType,

        /// Model directory path (default: auto from model-type)
        #[arg(short, long)]
        model: Option<PathBuf>,

        /// Tokenizer file path (default: auto from model-type)
        #[arg(short, long)]
        tokenizer: Option<PathBuf>,

        /// Show progress information
        #[arg(short, long)]
        verbose: bool,

        /// Force full rebuild (drop all embeddings and regenerate)
        #[arg(long)]
        rebuild: bool,
    },

    // ── Search ───────────────────────────────────────────────────────
    /// Search packages using natural language (semantic vector search)
    Search {
        /// Natural language search query (e.g., 'compression library', 'image processing tool')
        query: String,

        /// Filter by architecture
        #[arg(short, long)]
        arch: Option<String>,

        /// Filter by repository (can be specified multiple times)
        #[arg(short, long)]
        repo: Vec<String>,

        /// Use GBS configuration file to resolve repos from profile
        #[arg(long, value_name = "PATH")]
        gbs_conf: Option<PathBuf>,

        /// GBS profile to use (default: from gbs.conf [general] section)
        #[arg(long, requires = "gbs_conf")]
        gbs_profile: Option<String>,

        /// Exclude packages requiring this dependency
        #[arg(long)]
        not_requiring: Option<String>,

        /// Include only packages providing this capability
        #[arg(long)]
        providing: Option<String>,

        /// Number of results to return
        #[arg(short = 'n', long, default_value = "10")]
        top_k: usize,
    },

    /// Find packages by structured filters with wildcard support (* and ?)
    Find {
        /// Package name pattern (e.g., "lib*ssl*", "python?")
        #[arg(short, long)]
        name: Option<String>,

        /// Summary keyword pattern
        #[arg(short, long)]
        summary: Option<String>,

        /// Description keyword pattern
        #[arg(long)]
        description: Option<String>,

        /// Provides capability pattern (e.g., "libssl.so*")
        #[arg(short, long)]
        provides: Option<String>,

        /// Requires dependency pattern (e.g., "libcrypto*")
        #[arg(long)]
        requires: Option<String>,

        /// File path pattern (e.g., "/usr/bin/python*", "*.so")
        #[arg(short, long)]
        file: Option<String>,

        /// Filter by architecture
        #[arg(short, long)]
        arch: Option<String>,

        /// Filter by repository (can be specified multiple times)
        #[arg(long)]
        repo: Vec<String>,

        /// Use GBS configuration file to resolve repos from profile
        #[arg(long, value_name = "PATH")]
        gbs_conf: Option<PathBuf>,

        /// GBS profile to use (default: from gbs.conf [general] section)
        #[arg(long, requires = "gbs_conf")]
        gbs_profile: Option<String>,

        /// Number of results to return
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Search for packages that contain a specific file
    SearchFile {
        /// File path to search (e.g., /usr/bin/python3 or just "python3")
        path: String,

        /// Number of results to return
        #[arg(short = 'n', long, default_value = "20")]
        limit: usize,
    },

    /// List files provided by a package
    ListFiles {
        /// Package name
        #[arg(short, long)]
        package: String,

        /// Filter by architecture
        #[arg(short, long)]
        arch: Option<String>,

        /// Filter by repository (can be specified multiple times)
        #[arg(short, long)]
        repo: Vec<String>,

        /// Use GBS configuration file to resolve repos from profile
        #[arg(long, value_name = "PATH")]
        gbs_conf: Option<PathBuf>,

        /// GBS profile to use (default: from gbs.conf [general] section)
        #[arg(long, requires = "gbs_conf")]
        gbs_profile: Option<String>,
    },

    // ── Repository management ────────────────────────────────────────
    /// Show database statistics
    Stats,

    /// Repository management commands
    Repo {
        #[command(subcommand)]
        command: RepoCommands,
    },

    // ── Sync ─────────────────────────────────────────────────────────
    /// Sync repository metadata
    Sync {
        #[command(subcommand)]
        command: SyncCommands,
    },

    // ── Model management ────────────────────────────────────────────
    /// Download embedding model from HuggingFace Hub
    DownloadModel {
        /// Model type to download
        #[arg(long, value_enum, default_value = "minilm")]
        model_type: ModelType,
    },

    // ── Repoquery ─────────────────────────────────────────────────────
    /// Query packages from indexed repositories (similar to dnf repoquery)
    Repoquery {
        /// Package name or glob pattern (e.g., "bash", "lib*ssl*")
        package: Option<String>,

        // -- Query mode --
        /// Find packages that provide a capability (e.g., "libssl.so*")
        #[arg(long)]
        whatprovides: Option<String>,

        /// Find packages that require a capability
        #[arg(long)]
        whatrequires: Option<String>,

        /// Find packages that own a specific file
        #[arg(long)]
        file: Option<String>,

        // -- Output mode --
        /// Show detailed package information
        #[arg(short, long)]
        info: bool,

        /// List files in matched packages
        #[arg(short, long)]
        list: bool,

        /// Show requires of matched packages
        #[arg(long)]
        requires: bool,

        /// Show provides of matched packages
        #[arg(long)]
        provides: bool,

        /// Custom output format (supports %{name}, %{version}, %{release}, %{epoch}, %{arch},
        /// %{summary}, %{description}, %{license}, %{repo}, %{vcs}, %{nevra})
        #[arg(long)]
        queryformat: Option<String>,

        // -- Filters --
        /// Filter by architecture
        #[arg(short, long)]
        arch: Option<String>,

        /// Filter by repository (can be specified multiple times)
        #[arg(long)]
        repo: Vec<String>,

        /// Use GBS configuration file to resolve repos from profile
        #[arg(long, value_name = "PATH")]
        gbs_conf: Option<PathBuf>,

        /// GBS profile to use (default: from gbs.conf [general] section)
        #[arg(long, requires = "gbs_conf")]
        gbs_profile: Option<String>,

        /// Show only the latest version per package name+arch
        #[arg(long)]
        latest: bool,

        /// Maximum results
        #[arg(long, default_value = "200")]
        limit: usize,
    },

    // ── Server & Debug ───────────────────────────────────────────────
    /// Run MCP (Model Context Protocol) server
    McpServer,

    /// Debug search - diagnose embedding quality
    DebugSearch {
        /// Search query
        query: String,

        /// Specific pkg_ids to check (comma-separated)
        #[arg(long)]
        pkg_ids: Option<String>,
    },
}

/// Check if CUDA is available and exec CUDA version of the binary if it is
#[cfg(not(feature = "cuda"))]
fn check_and_exec_cuda_version() -> Result<()> {
    use std::os::unix::process::CommandExt;
    use std::process::Command;

    // Probe for CUDA driver library (load then immediately close)
    let cuda_available = unsafe {
        let handle = libc::dlopen(c"libcuda.so.1".as_ptr(), libc::RTLD_LAZY);
        if !handle.is_null() {
            libc::dlclose(handle);
            true
        } else {
            let handle = libc::dlopen(c"libcuda.so".as_ptr(), libc::RTLD_LAZY);
            if !handle.is_null() {
                libc::dlclose(handle);
                true
            } else {
                false
            }
        }
    };

    if cuda_available {
        let exe_path = std::env::current_exe()?;

        // Derive CUDA binary name from current executable (e.g., foo -> foo_cuda)
        let cuda_binary = exe_path.parent().and_then(|dir| {
            exe_path
                .file_name()
                .map(|name| dir.join(format!("{}_cuda", name.to_string_lossy())))
        });

        if let Some(cuda_bin) = cuda_binary {
            if cuda_bin.exists() {
                tracing::debug!("CUDA detected, executing CUDA version");

                // Replace current process with CUDA version
                let err = Command::new(&cuda_bin)
                    .args(std::env::args_os().skip(1))
                    .exec();

                // exec() should never return on success; if it does, error
                return Err(error::RpmSearchError::Config(format!(
                    "Failed to exec CUDA binary: {}",
                    err
                )));
            }
        }
    }

    Ok(())
}

// ── Repoquery helpers ────────────────────────────────────────────────

/// Format a package using a custom query format string.
/// Supports tags: %{name}, %{version}, %{release}, %{epoch}, %{arch},
/// %{summary}, %{description}, %{license}, %{repo}, %{vcs}, %{nevra}.
/// Also handles \n and \t escape sequences.
fn format_querystring(fmt: &str, pkg: &Package) -> String {
    fmt.replace("%{name}", &pkg.name)
        .replace("%{version}", &pkg.version)
        .replace("%{release}", &pkg.release)
        .replace(
            "%{epoch}",
            &pkg.epoch.map(|e| e.to_string()).unwrap_or_default(),
        )
        .replace("%{arch}", &pkg.arch)
        .replace("%{summary}", &pkg.summary)
        .replace("%{description}", &pkg.description)
        .replace("%{license}", pkg.license.as_deref().unwrap_or(""))
        .replace("%{repo}", &pkg.repo)
        .replace("%{vcs}", pkg.vcs.as_deref().unwrap_or(""))
        .replace(
            "%{nevra}",
            &format!("{}-{}.{}", pkg.name, pkg.full_version(), pkg.arch),
        )
        .replace("\\n", "\n")
        .replace("\\t", "\t")
}

/// Filter packages to keep only the latest version per (name, arch) pair.
/// Uses RPM version comparison via Package::Ord.
fn filter_latest(packages: Vec<Package>) -> Vec<Package> {
    let mut latest_map: HashMap<(String, String), Package> = HashMap::new();
    for pkg in packages {
        let key = (pkg.name.clone(), pkg.arch.clone());
        match latest_map.entry(key) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(pkg);
            }
            std::collections::hash_map::Entry::Occupied(mut e) => {
                if pkg > *e.get() {
                    e.insert(pkg);
                }
            }
        }
    }
    let mut result: Vec<Package> = latest_map.into_values().collect();
    result.sort();
    result
}

/// Resolve repository filter from --repo flags and --gbs-conf/--gbs-profile options.
/// If both --repo and --gbs-conf are provided, the repos are merged.
fn resolve_repos(
    repo: Vec<String>,
    gbs_conf: Option<&Path>,
    gbs_profile: Option<&str>,
) -> Result<Vec<String>> {
    let mut repos = repo;
    if let Some(gbs_path) = gbs_conf {
        let gbs = gbs::GbsConfig::from_path(gbs_path)?;
        let gbs_repos = gbs.get_repo_urls(gbs_profile)?;
        for (name, _url) in gbs_repos {
            if !repos.contains(&name) {
                repos.push(name);
            }
        }
    }
    Ok(repos)
}

fn main() -> Result<()> {
    // Check if CUDA is available and exec CUDA version if it is
    #[cfg(not(feature = "cuda"))]
    check_and_exec_cuda_version()?;

    // Register sqlite-vec extension for all connections (when feature enabled)
    unsafe {
        use rusqlite::ffi::sqlite3_auto_extension;
        sqlite3_auto_extension(Some(std::mem::transmute::<
            *const (),
            unsafe extern "C" fn(
                *mut rusqlite::ffi::sqlite3,
                *mut *mut std::os::raw::c_char,
                *const rusqlite::ffi::sqlite3_api_routines,
            ) -> std::os::raw::c_int,
        >(sqlite_vec::sqlite3_vec_init as *const ())));
    }

    // Initialize logging with environment variable support (RUST_LOG)
    // MCP server mode: logs MUST go to stderr since stdout is the JSON-RPC transport
    let is_mcp_mode = std::env::args().any(|a| a == "mcp-server");

    if is_mcp_mode {
        // MCP mode: write logs to stderr to avoid polluting the JSON-RPC channel on stdout
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::filter::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("warn")),
            )
            .with_target(false)
            .with_writer(std::io::stderr)
            .init();
    } else {
        // CLI mode: default to warn to keep output clean (use RUST_LOG=info for verbose)
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::filter::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("warn")),
            )
            .with_target(false)
            .with_writer(std::io::stderr)
            .init();
    }

    let cli = Cli::parse();
    let config = Config::new(cli.db);

    match cli.command {
        Commands::Index {
            file,
            repo,
            update,
            filelists,
        } => {
            let _span = tracing::info_span!("index", repo = %repo, file = %file.display(), update)
                .entered();
            if update {
                info!("Updating repository (incremental)");
            } else {
                info!("Indexing repository");
            }
            let mut api = api::RpmSearchApi::new(config)?;
            let count = api.index_repository(&file, &repo, update)?;
            if update {
                info!(count, "Successfully updated packages");
            } else {
                info!(count, "Successfully indexed packages");
            }

            // If filelists provided, index them too
            if let Some(filelists_path) = filelists {
                info!("Indexing filelists");
                let fl_count = api.index_filelists(&filelists_path, &repo)?;
                info!(fl_count, "Successfully indexed file entries");
            }
        }

        Commands::BuildEmbeddings {
            model_type,
            model,
            tokenizer,
            verbose,
            rebuild,
        } => {
            // Resolve model files: custom paths > local dir > hf-hub download
            let model_files = embedding::hub::resolve_model_files(
                &model_type,
                model.as_deref(),
                tokenizer.as_deref(),
            )?;

            let _span = tracing::info_span!("build_embeddings",
                model_type = %model_type,
                config = %model_files.config.display(),
                weights = %model_files.weights.display(),
                tokenizer = %model_files.tokenizer.display(),
                verbose,
                rebuild
            )
            .entered();
            info!("Building embeddings");
            let mut config = config;
            config.model_type = model_type;
            config.model_path = model_files
                .weights
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
            config.tokenizer_path = model_files.tokenizer.clone();

            let api = api::RpmSearchApi::new(config.clone())?;
            let embedder =
                embedding::Embedder::from_model_files(&model_files, config.model_type.clone())?;
            let count = api.build_embeddings(&embedder, verbose, rebuild)?;
            info!(count, "Successfully built embeddings");
        }

        Commands::Search {
            query,
            arch,
            repo,
            gbs_conf,
            gbs_profile,
            not_requiring,
            providing,
            top_k,
        } => {
            let repos = resolve_repos(repo, gbs_conf.as_deref(), gbs_profile.as_deref())?;

            let _span = tracing::info_span!("search",
                query = %query,
                ?arch,
                ?repos,
                top_k
            )
            .entered();

            let mut config = config;
            config.top_k = top_k;

            let api = api::RpmSearchApi::new(config)?;
            let filters = SearchFilters {
                name: None,
                arch,
                repos,
                not_requiring,
                providing,
            };

            let result = api.search_with_scores(&query, filters)?;

            info!(count = result.packages.len(), "Search completed");

            // Output results to stdout (not logged)
            println!("\nFound {} packages:\n", result.packages.len());
            for (i, pkg) in result.packages.iter().enumerate() {
                let score = result.scores.get(i).copied().unwrap_or(0.0);
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                println!(
                    "📦 {} - {}  (score: {:.3})",
                    pkg.name,
                    pkg.full_version(),
                    score
                );
                println!("   Architecture: {}", pkg.arch);
                println!("   Repository: {}", pkg.repo);
                println!("   Summary: {}", pkg.summary);
                if let Some(ref license) = pkg.license {
                    println!("   License: {}", license);
                }
                if let Some(ref vcs) = pkg.vcs {
                    println!("   VCS: {}", vcs);
                }
                if !pkg.description.is_empty() {
                    let desc = if pkg.description.len() > 200 {
                        format!("{}...", &pkg.description[..200])
                    } else {
                        pkg.description.clone()
                    };
                    println!("   Description: {}", desc);
                }
                println!();
            }
        }

        Commands::IndexFilelists { file, repo } => {
            let _span =
                tracing::info_span!("index_filelists", repo = %repo, file = %file.display())
                    .entered();
            info!("Indexing filelists");
            let mut api = api::RpmSearchApi::new(config)?;
            let count = api.index_filelists(&file, &repo)?;
            info!(count, "Successfully indexed file entries");
        }

        Commands::SearchFile { path, limit } => {
            let _span = tracing::info_span!("search_file", path = %path).entered();
            let api = api::RpmSearchApi::new(config)?;
            let results = api.search_file(&path)?;

            if results.is_empty() {
                println!("No packages found providing '{}'", path);
            } else {
                println!("\nPackages providing '{}':\n", path);
                for (i, (pkg, full_path, file_type)) in results.iter().enumerate().take(limit) {
                    println!(
                        "  {}. {}-{}.{} ({}) [{}] — {}",
                        i + 1,
                        pkg.name,
                        pkg.full_version(),
                        pkg.arch,
                        pkg.repo,
                        file_type,
                        full_path,
                    );
                }
                let total = results.len();
                if total > limit {
                    println!("\n  ... and {} more results", total - limit);
                }
            }
        }

        Commands::ListFiles {
            package,
            arch,
            repo,
            gbs_conf,
            gbs_profile,
        } => {
            let repos = resolve_repos(repo, gbs_conf.as_deref(), gbs_profile.as_deref())?;

            let _span = tracing::info_span!("list_files", package = %package).entered();
            let api = api::RpmSearchApi::new(config)?;
            let results = api.list_package_files(&package, arch.as_deref(), &repos)?;

            if results.is_empty() {
                println!("No packages found matching '{}'", package);
                println!("(Make sure filelists have been indexed with 'index-filelists')");
            } else {
                for (pkg, files) in &results {
                    println!(
                        "\n{}-{}.{} ({})",
                        pkg.name,
                        pkg.full_version(),
                        pkg.arch,
                        pkg.repo
                    );
                    if files.is_empty() {
                        println!("  (no filelists indexed)");
                    } else {
                        for (path, ft) in files {
                            let marker = match ft.as_str() {
                                "dir" => "d",
                                "ghost" => "g",
                                _ => " ",
                            };
                            println!("  [{}] {}", marker, path);
                        }
                        println!("  Total: {} file(s)", files.len());
                    }
                }
            }
        }

        Commands::Find {
            name,
            summary,
            description,
            provides,
            requires,
            file,
            arch,
            repo,
            gbs_conf,
            gbs_profile,
            limit,
        } => {
            let repos = resolve_repos(repo, gbs_conf.as_deref(), gbs_profile.as_deref())?;

            let _span = tracing::info_span!("find").entered();
            let api = api::RpmSearchApi::new(config)?;

            let filter = FindFilter {
                name,
                summary,
                description,
                provides,
                requires,
                file,
                arch,
                repos,
                limit,
            };

            let results = api.find(&filter)?;

            if results.is_empty() {
                println!("No packages found matching the given criteria.");
            } else {
                println!("\nFound {} package(s):\n", results.len());
                for (i, pkg) in results.iter().enumerate() {
                    println!(
                        "  {}. {}-{}.{} ({})",
                        i + 1,
                        pkg.name,
                        pkg.full_version(),
                        pkg.arch,
                        pkg.repo,
                    );
                    if !pkg.summary.is_empty() {
                        println!("     {}", pkg.summary);
                    }
                }
            }
        }

        Commands::Stats => {
            let _span = tracing::info_span!("stats").entered();
            let api = api::RpmSearchApi::new(config)?;
            let count = api.package_count()?;
            let file_count = api.file_count()?;
            let dir_count = api.directory_count()?;
            info!(count, "Retrieved statistics");
            println!("Database Statistics:");
            println!("  Total packages:    {}", count);
            println!("  Total files:       {}", file_count);
            println!("  Total directories: {}", dir_count);
        }

        Commands::Repo { command } => match command {
            RepoCommands::List => {
                let _span = tracing::info_span!("list_repos").entered();
                let api = api::RpmSearchApi::new(config)?;
                let repos = api.list_repositories()?;

                info!(repo_count = repos.len(), "Retrieved repository list");

                if repos.is_empty() {
                    println!("No repositories indexed yet.");
                } else {
                    println!("\nIndexed Repositories:\n");
                    println!("{:<30} {:>10}", "Repository", "Packages");
                    println!("{}", "─".repeat(42));
                    for (repo_name, count) in repos {
                        println!("{:<30} {:>10}", repo_name, count);
                    }
                }
            }

            RepoCommands::Stats { repo } => {
                let _span = tracing::info_span!("repo_stats", repo = %repo).entered();
                let api = api::RpmSearchApi::new(config)?;
                let count = api.repo_package_count(&repo)?;

                info!(count, "Retrieved repository statistics");

                println!("\nRepository: {}", repo);
                println!("  Packages: {}", count);
            }

            RepoCommands::Delete { repo, yes } => {
                let _span = tracing::info_span!("delete_repo", repo = %repo).entered();

                if !yes {
                    println!(
                        "⚠️  This will permanently delete repository '{}' and all its packages.",
                        repo
                    );
                    println!("   Use --yes to confirm deletion.");
                    return Ok(());
                }

                let mut api = api::RpmSearchApi::new(config)?;
                let deleted = api.delete_repository(&repo)?;

                info!(deleted, "Deleted repository");

                if deleted == 0 {
                    println!("Repository '{}' not found or already empty.", repo);
                } else {
                    println!(
                        "✓ Deleted repository '{}' ({} packages removed)",
                        repo, deleted
                    );
                }
            }
        },

        Commands::DownloadModel { model_type } => {
            let _span = tracing::info_span!("download_model", model_type = %model_type).entered();
            info!("Downloading model");

            println!(
                "Downloading {} model from HuggingFace Hub...",
                model_type.display_name()
            );
            println!("Repository: {}", model_type.huggingface_url());
            println!();

            let hub = embedding::ModelHub::new()?;
            let files = hub.get_model_files(&model_type)?;

            println!("Model files downloaded successfully:");
            println!("  Config:    {}", files.config.display());
            println!("  Weights:   {}", files.weights.display());
            println!("  Tokenizer: {}", files.tokenizer.display());
        }

        Commands::McpServer => {
            let _span = tracing::info_span!("mcp_server").entered();
            info!("Starting MCP server");
            let server = mcp::McpServer::new(config)?;
            server.run()?;
        }

        Commands::Sync { command } => match command {
            SyncCommands::Init {
                output,
                from_gbs,
                gbs_profile,
            } => {
                let _span = tracing::info_span!("sync_init", output = %output.display()).entered();

                let sync_config = if let Some(gbs_path) = from_gbs {
                    info!(gbs_conf = %gbs_path.display(), "Generating sync config from GBS config");
                    gbs::GbsConfig::from_path(&gbs_path)?.to_sync_config(gbs_profile.as_deref())?
                } else {
                    info!("Generating example sync configuration");
                    sync::SyncConfig::example()
                };

                sync_config.to_file(&output)?;

                println!("✓ Created sync configuration: {}", output.display());
                println!("\nEdit this file to configure your repositories, then run:");
                println!("  rpm_repo_search sync once --config {}", output.display());
                println!(
                    "  rpm_repo_search sync daemon --config {}",
                    output.display()
                );
            }

            SyncCommands::Once {
                config: sync_config_path,
                gbs_conf,
                gbs_profile,
            } => {
                let sync_config = if let Some(gbs_path) = gbs_conf {
                    let _span =
                        tracing::info_span!("sync_once", gbs_conf = %gbs_path.display()).entered();
                    info!("Performing one-time sync from GBS config");
                    gbs::GbsConfig::from_path(&gbs_path)?.to_sync_config(gbs_profile.as_deref())?
                } else {
                    let config_path =
                        sync_config_path.unwrap_or_else(|| PathBuf::from("sync-config.toml"));
                    let _span =
                        tracing::info_span!("sync_once", config = %config_path.display()).entered();
                    info!("Performing one-time sync");
                    sync::SyncConfig::from_file(&config_path)?
                };
                let scheduler = sync::SyncScheduler::new(sync_config, config.clone());

                let runtime = tokio::runtime::Runtime::new().map_err(|e| {
                    error::RpmSearchError::Config(format!("Failed to create runtime: {}", e))
                })?;

                let results = runtime.block_on(scheduler.sync_once())?;

                println!("\nSync Results:");
                println!("{:<30} {:<15}", "Repository", "Status");
                println!("{}", "─".repeat(47));

                for (repo, result) in results {
                    let status = match result {
                        Ok(_) => "✓ Success",
                        Err(ref e) => {
                            eprintln!("Error for {}: {}", repo, e);
                            "✗ Failed"
                        }
                    };
                    println!("{:<30} {:<15}", repo, status);
                }

                // Automatically build embeddings incrementally after sync
                println!("\n🔨 Building embeddings for new packages...");
                let model_files =
                    embedding::hub::resolve_model_files(&config.model_type, None, None)?;
                let api = api::RpmSearchApi::new(config.clone())?;
                let embedder =
                    embedding::Embedder::from_model_files(&model_files, config.model_type.clone())?;
                let count = api.build_embeddings(&embedder, false, false)?;
                if count > 0 {
                    println!("✅ Built embeddings for {} new packages", count);
                } else {
                    println!("✅ All embeddings up to date");
                }
            }

            SyncCommands::Daemon {
                config: sync_config_path,
                gbs_conf,
                gbs_profile,
            } => {
                let sync_config = if let Some(gbs_path) = gbs_conf {
                    let _span = tracing::info_span!("sync_daemon", gbs_conf = %gbs_path.display())
                        .entered();
                    info!("Starting sync daemon from GBS config");
                    gbs::GbsConfig::from_path(&gbs_path)?.to_sync_config(gbs_profile.as_deref())?
                } else {
                    let config_path =
                        sync_config_path.unwrap_or_else(|| PathBuf::from("sync-config.toml"));
                    let _span = tracing::info_span!("sync_daemon", config = %config_path.display())
                        .entered();
                    info!("Starting sync daemon");
                    sync::SyncConfig::from_file(&config_path)?
                };
                let scheduler = sync::SyncScheduler::new(sync_config, config);

                println!("Starting sync daemon...");
                println!("Press Ctrl+C to stop");

                let runtime = tokio::runtime::Runtime::new().map_err(|e| {
                    error::RpmSearchError::Config(format!("Failed to create runtime: {}", e))
                })?;

                runtime.block_on(scheduler.run_daemon())?;
            }

            SyncCommands::Status => {
                let _span = tracing::info_span!("sync_status").entered();
                info!("Retrieving sync status");

                let conn = rusqlite::Connection::open(&config.db_path)?;
                let state_store = sync::SyncStateStore::new(conn)?;
                let states = state_store.list_states()?;

                if states.is_empty() {
                    println!("No sync state found. Run 'sync once' or 'sync daemon' first.");
                } else {
                    println!("\nSync Status:");
                    println!(
                        "{:<25} {:<15} {:<25} {:<15}",
                        "Repository", "Status", "Last Sync", "Checksum"
                    );
                    println!("{}", "─".repeat(82));

                    for state in states {
                        let last_sync_str = state
                            .last_sync
                            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                            .unwrap_or_else(|| "Never".to_string());

                        let checksum_short = state
                            .last_checksum
                            .map(|s| {
                                if s.len() > 12 {
                                    format!("{}...", &s[..12])
                                } else {
                                    s
                                }
                            })
                            .unwrap_or_else(|| "N/A".to_string());

                        println!(
                            "{:<25} {:<15} {:<25} {:<15}",
                            state.repo_name,
                            state.last_status.to_string(),
                            last_sync_str,
                            checksum_short
                        );

                        if let Some(error) = state.last_error {
                            println!("  Error: {}", error);
                        }
                    }
                }
            }
        },

        Commands::Repoquery {
            package,
            whatprovides,
            whatrequires,
            file,
            info,
            list,
            requires,
            provides,
            queryformat,
            arch,
            repo,
            gbs_conf,
            gbs_profile,
            latest,
            limit,
        } => {
            let repos = resolve_repos(repo, gbs_conf.as_deref(), gbs_profile.as_deref())?;

            let _span = tracing::info_span!("repoquery").entered();
            let api = api::RpmSearchApi::new(config)?;

            // 1. Query phase: select packages
            let mut packages = if let Some(ref file_path) = file {
                // --file: find packages owning a file
                let file_results = api.search_file(file_path)?;
                file_results
                    .into_iter()
                    .map(|(pkg, _, _)| pkg)
                    .filter(|pkg| arch.as_ref().is_none_or(|a| pkg.arch == *a))
                    .filter(|pkg| repos.is_empty() || repos.contains(&pkg.repo))
                    .collect::<Vec<_>>()
            } else {
                // Build FindFilter for name / whatprovides / whatrequires queries
                let filter = FindFilter {
                    name: package.clone(),
                    provides: whatprovides.clone(),
                    requires: whatrequires.clone(),
                    arch: arch.clone(),
                    repos: repos.clone(),
                    limit,
                    ..Default::default()
                };

                // If no query criteria given at all, list all packages (with arch/repo filter)
                if filter.name.is_none()
                    && filter.provides.is_none()
                    && filter.requires.is_none()
                    && filter.arch.is_none()
                    && filter.repos.is_empty()
                {
                    // general_search returns empty when no conditions, so use name wildcard
                    let all_filter = FindFilter {
                        name: Some("*".to_string()),
                        arch: arch.clone(),
                        repos: repos.clone(),
                        limit,
                        ..Default::default()
                    };
                    api.find(&all_filter)?
                } else {
                    api.find(&filter)?
                }
            };

            // 2. Filter phase: --latest
            if latest {
                packages = filter_latest(packages);
            }

            if packages.is_empty() {
                // Describe what was searched
                if let Some(ref p) = package {
                    println!("No packages found matching '{}'", p);
                } else if let Some(ref cap) = whatprovides {
                    println!("No packages found providing '{}'", cap);
                } else if let Some(ref cap) = whatrequires {
                    println!("No packages found requiring '{}'", cap);
                } else if let Some(ref f) = file {
                    println!("No packages found owning '{}'", f);
                } else {
                    println!("No packages found.");
                }
                return Ok(());
            }

            // 3. Output phase
            if info {
                // --info: detailed package information
                for pkg in &packages {
                    println!("Name        : {}", pkg.name);
                    println!(
                        "Epoch       : {}",
                        pkg.epoch
                            .map(|e| e.to_string())
                            .unwrap_or_else(|| "0".to_string())
                    );
                    println!("Version     : {}", pkg.version);
                    println!("Release     : {}", pkg.release);
                    println!("Arch        : {}", pkg.arch);
                    println!("Summary     : {}", pkg.summary);
                    println!(
                        "License     : {}",
                        pkg.license.as_deref().unwrap_or("(none)")
                    );
                    println!("Repo        : {}", pkg.repo);
                    if let Some(ref vcs) = pkg.vcs {
                        println!("VCS         : {}", vcs);
                    }
                    println!("Description : {}", pkg.description);
                    println!();
                }
            } else if requires {
                // --requires: show requires for each package
                for pkg in &packages {
                    if packages.len() > 1 {
                        println!("# {}-{}.{}", pkg.name, pkg.full_version(), pkg.arch);
                    }
                    for dep in &pkg.requires {
                        if let (Some(flags), Some(ver)) = (&dep.flags, &dep.version) {
                            println!("{} {} {}", dep.name, flags, ver);
                        } else {
                            println!("{}", dep.name);
                        }
                    }
                }
            } else if provides {
                // --provides: show provides for each package
                for pkg in &packages {
                    if packages.len() > 1 {
                        println!("# {}-{}.{}", pkg.name, pkg.full_version(), pkg.arch);
                    }
                    for dep in &pkg.provides {
                        if let (Some(flags), Some(ver)) = (&dep.flags, &dep.version) {
                            println!("{} {} {}", dep.name, flags, ver);
                        } else {
                            println!("{}", dep.name);
                        }
                    }
                }
            } else if list {
                // --list: list files for each package
                for pkg in &packages {
                    if packages.len() > 1 {
                        println!("# {}-{}.{}", pkg.name, pkg.full_version(), pkg.arch);
                    }
                    if pkg.pkg_id.is_some() {
                        let pkg_repo = vec![pkg.repo.clone()];
                        let files =
                            api.list_package_files(&pkg.name, Some(&pkg.arch), &pkg_repo)?;
                        let mut found = false;
                        for (_, file_list) in &files {
                            for (path, _) in file_list {
                                println!("{}", path);
                                found = true;
                            }
                        }
                        if !found {
                            println!("  (no filelists indexed — run 'index-filelists' first)");
                        }
                    }
                }
            } else if let Some(ref fmt) = queryformat {
                // --queryformat: custom format
                for pkg in &packages {
                    print!("{}", format_querystring(fmt, pkg));
                }
            } else {
                // Default: NEVRA output
                for pkg in &packages {
                    println!("{}-{}.{}", pkg.name, pkg.full_version(), pkg.arch);
                }
            }
        }

        Commands::DebugSearch { query, pkg_ids } => {
            let mut config = config;
            config.top_k = 10;

            // Auto-detect model type from DB metadata
            let db_model_type = {
                let conn = rusqlite::Connection::open(&config.db_path)?;
                let vector_store = storage::VectorStore::new(conn)?;
                vector_store.get_embedding_model_type()?
            };
            if let Some(ref db_type_str) = db_model_type {
                if let Some(detected) = ModelType::from_db_str(db_type_str) {
                    info!(model = %detected, "Auto-detected embedding model from DB");
                    config.model_type = detected;
                }
            }

            let model_files = embedding::hub::resolve_model_files(&config.model_type, None, None)?;
            let embedder =
                embedding::Embedder::from_model_files(&model_files, config.model_type.clone())?;

            // Embed the query (auto-adds prefix for E5 models)
            let query_embedding = embedder.embed_query(&query)?;
            let norm: f32 = query_embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            println!("Query: \"{}\"", query);
            println!("Query embedding norm: {:.6} (should be ~1.0)", norm);
            println!(
                "Query embedding first 10 dims: {:?}",
                &query_embedding[..10]
            );
            println!();

            // Open vector store and get raw distances
            let conn = rusqlite::Connection::open(&config.db_path)?;
            let pkg_conn = rusqlite::Connection::open(&config.db_path)?;

            // Get top 20 nearest by L2 distance
            let mut stmt = conn.prepare(
                "SELECT pkg_id, distance FROM embeddings WHERE embedding MATCH ? ORDER BY distance LIMIT 20"
            )?;
            let embedding_json = serde_json::to_string(&query_embedding).unwrap();
            let results: Vec<(i64, f64)> = stmt
                .query_map(rusqlite::params![embedding_json], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?
                .collect::<std::result::Result<_, _>>()?;

            println!("Top 20 nearest by raw L2 distance:");
            println!(
                "{:<8} {:<12} {:<12} {:<40} {:<20}",
                "pkg_id", "L2_dist", "cos_sim", "name", "summary"
            );
            println!("{}", "─".repeat(92));
            for (pkg_id, dist) in &results {
                let cos_sim = 1.0 - dist * dist / 2.0;
                let cos_sim = cos_sim.clamp(0.0, 1.0);

                let name_summary: (String, String) = pkg_conn
                    .query_row(
                        "SELECT name, summary FROM packages WHERE pkg_id = ?",
                        [pkg_id],
                        |row| Ok((row.get(0)?, row.get(1)?)),
                    )
                    .unwrap_or(("?".to_string(), "?".to_string()));

                println!(
                    "{:<8} {:<12.6} {:<12.6} {:<40} {:<20}",
                    pkg_id,
                    dist,
                    cos_sim,
                    if name_summary.0.len() > 38 {
                        format!("{}...", &name_summary.0[..35])
                    } else {
                        name_summary.0.clone()
                    },
                    if name_summary.1.len() > 18 {
                        format!("{}...", &name_summary.1[..15])
                    } else {
                        name_summary.1.clone()
                    }
                );
            }

            // Also check specific packages if requested
            if let Some(ids_str) = pkg_ids {
                println!("\nSpecific package checks:");
                for id_str in ids_str.split(',') {
                    if let Ok(pkg_id) = id_str.trim().parse::<i64>() {
                        // Read stored embedding
                        let emb_row: Option<Vec<u8>> = conn
                            .query_row(
                                "SELECT embedding FROM embeddings WHERE pkg_id = ?",
                                [pkg_id],
                                |row| row.get(0),
                            )
                            .ok();

                        if let Some(blob) = emb_row {
                            let stored: Vec<f32> = blob
                                .chunks_exact(4)
                                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                                .collect();
                            let stored_norm: f32 = stored.iter().map(|x| x * x).sum::<f32>().sqrt();
                            let dot: f32 = query_embedding
                                .iter()
                                .zip(stored.iter())
                                .map(|(a, b)| a * b)
                                .sum();
                            let cos_sim = dot / (norm * stored_norm);

                            let name: String = pkg_conn
                                .query_row(
                                    "SELECT name FROM packages WHERE pkg_id = ?",
                                    [pkg_id],
                                    |row| row.get(0),
                                )
                                .unwrap_or("?".to_string());

                            println!("  pkg_id={}: name={}, stored_norm={:.6}, dot={:.6}, cos_sim={:.6}, first_10={:?}",
                                pkg_id, name, stored_norm, dot, cos_sim, &stored[..10.min(stored.len())]);
                        } else {
                            println!("  pkg_id={}: no embedding found", pkg_id);
                        }
                    }
                }
            }

            // Embed a few reference texts and compare
            println!("\nReference embedding similarities:");
            let ref_texts = vec![
                query.clone(),
                "Package: libopenssl11\nName: libopenssl11\nSummary: Secure Sockets Layer and cryptography libraries".to_string(),
                "Package: doc\nName: doc\nSummary: Document\nDescription: Example files".to_string(),
                "Package: gcc-contrib\nName: gcc-contrib\nSummary: GCC related scripts".to_string(),
            ];
            let ref_labels = [
                "query (self)",
                "libopenssl11-like",
                "doc-like",
                "gcc-contrib-like",
            ];
            let ref_embeddings = embedder.embed_passages(&ref_texts)?;

            for (label, emb) in ref_labels.iter().zip(ref_embeddings.iter()) {
                let n: f32 = emb.iter().map(|x| x * x).sum::<f32>().sqrt();
                let dot: f32 = query_embedding
                    .iter()
                    .zip(emb.iter())
                    .map(|(a, b)| a * b)
                    .sum();
                let sim = dot / (norm * n);
                println!("  {} → norm={:.6}, cos_sim_with_query={:.6}", label, n, sim);
            }

            // CRITICAL TEST: Compare single vs batch embedding for same text
            println!("\n=== Single vs Batch Embedding Comparison ===");
            let test_text = "Package: libopenssl11\nName: libopenssl11\nArchitecture: armv7l\nSummary: Secure Sockets Layer and cryptography libraries\nDescription: This package contains the OpenSSL shared libraries.";
            let single_emb = embedder.embed(test_text)?;
            let batch_emb = embedder.embed_batch(&[test_text.to_string()])?;
            let batch_emb1 = &batch_emb[0];

            let single_norm: f32 = single_emb.iter().map(|x| x * x).sum::<f32>().sqrt();
            let batch_norm: f32 = batch_emb1.iter().map(|x| x * x).sum::<f32>().sqrt();
            let cross_dot: f32 = single_emb
                .iter()
                .zip(batch_emb1.iter())
                .map(|(a, b)| a * b)
                .sum();
            let cross_sim = cross_dot / (single_norm * batch_norm);

            println!("  Single embed norm: {:.6}", single_norm);
            println!("  Batch(1) embed norm: {:.6}", batch_norm);
            println!(
                "  Cos sim (single vs batch): {:.6} (should be ~1.0)",
                cross_sim
            );
            println!("  Single first 5: {:?}", &single_emb[..5]);
            println!("  Batch  first 5: {:?}", &batch_emb1[..5]);

            // Test with actual batch of 4 texts
            let batch4 = vec![
                test_text.to_string(),
                "Package: doc\nName: doc\nSummary: Document\nDescription: Example files"
                    .to_string(),
                "Package: gcc-contrib\nName: gcc-contrib\nSummary: GCC related scripts".to_string(),
                "Package: zlib\nName: zlib\nSummary: Compression library".to_string(),
            ];
            let batch4_emb = embedder.embed_batch(&batch4)?;
            let batch4_first = &batch4_emb[0]; // Should be same text as single_emb

            let cross_dot2: f32 = single_emb
                .iter()
                .zip(batch4_first.iter())
                .map(|(a, b)| a * b)
                .sum();
            let b4_norm: f32 = batch4_first.iter().map(|x| x * x).sum::<f32>().sqrt();
            let cross_sim2 = cross_dot2 / (single_norm * b4_norm);

            println!("\n  Batch(4)[0] norm: {:.6}", b4_norm);
            println!(
                "  Cos sim (single vs batch4[0]): {:.6} (should be ~1.0)",
                cross_sim2
            );
            println!("  Batch4[0] first 5: {:?}", &batch4_first[..5]);
        }
    }

    Ok(())
}
