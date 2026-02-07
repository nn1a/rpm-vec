mod api;
mod config;
mod embedding;
mod error;
#[cfg(feature = "mcp")]
mod mcp;
mod normalize;
mod repomd;
mod search;
mod storage;
#[cfg(feature = "sync")]
mod sync;

use clap::{Parser, Subcommand};
use config::Config;
use error::Result;
use search::SearchFilters;
use std::path::PathBuf;
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
enum Commands {
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
    },

    /// Build embeddings for indexed packages
    BuildEmbeddings {
        /// Model directory path
        #[arg(short, long, default_value = "models/all-MiniLM-L6-v2")]
        model: PathBuf,

        /// Tokenizer file path
        #[arg(short, long, default_value = "models/all-MiniLM-L6-v2/tokenizer.json")]
        tokenizer: PathBuf,

        /// Show progress information
        #[arg(short, long)]
        verbose: bool,
    },

    /// Search packages
    Search {
        /// Search query
        query: String,

        /// Filter by architecture
        #[arg(short, long)]
        arch: Option<String>,

        /// Filter by repository
        #[arg(short, long)]
        repo: Option<String>,

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

    /// Show database statistics
    Stats,

    /// List all indexed repositories
    ListRepos,

    /// Show repository statistics
    RepoStats {
        /// Repository name
        repo: String,
    },

    /// Delete a repository and all its packages
    DeleteRepo {
        /// Repository name
        repo: String,

        /// Confirm deletion
        #[arg(short, long)]
        yes: bool,
    },

    /// Run MCP (Model Context Protocol) server
    #[cfg(feature = "mcp")]
    McpServer,

    /// Generate example sync configuration file
    #[cfg(feature = "sync")]
    SyncInit {
        /// Output file path
        #[arg(short, long, default_value = "sync-config.toml")]
        output: PathBuf,
    },

    /// Perform one-time sync of all repositories
    #[cfg(feature = "sync")]
    SyncOnce {
        /// Sync configuration file
        #[arg(short, long, default_value = "sync-config.toml")]
        config: PathBuf,
    },

    /// Run sync daemon (continuous background syncing)
    #[cfg(feature = "sync")]
    SyncDaemon {
        /// Sync configuration file
        #[arg(short, long, default_value = "sync-config.toml")]
        config: PathBuf,
    },

    /// Show sync status for all repositories
    #[cfg(feature = "sync")]
    SyncStatus,
}

fn main() -> Result<()> {
    // Register sqlite-vec extension for all connections (when feature enabled)
    #[cfg(feature = "sqlite-vec")]
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
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::filter::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let config = Config::new(cli.db);

    match cli.command {
        Commands::Index { file, repo, update } => {
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
        }

        Commands::BuildEmbeddings {
            model,
            tokenizer,
            verbose,
        } => {
            let _span = tracing::info_span!("build_embeddings",
                model = %model.display(),
                tokenizer = %tokenizer.display(),
                verbose
            )
            .entered();
            info!("Building embeddings");
            let mut config = config;
            config.model_path = model;
            config.tokenizer_path = tokenizer;

            let api = api::RpmSearchApi::new(config.clone())?;
            let embedder = embedding::Embedder::new(&config.model_path, &config.tokenizer_path)?;
            let count = api.build_embeddings(&embedder, verbose)?;
            info!(count, "Successfully built embeddings");
        }

        Commands::Search {
            query,
            arch,
            repo,
            not_requiring,
            providing,
            top_k,
        } => {
            let _span = tracing::info_span!("search",
                query = %query,
                ?arch,
                ?repo,
                top_k
            )
            .entered();

            let mut config = config;
            config.top_k = top_k;

            let api = api::RpmSearchApi::new(config)?;
            let filters = SearchFilters {
                name: None,
                arch,
                repo,
                not_requiring,
                providing,
            };

            let packages = api.search(&query, filters)?;

            info!(count = packages.len(), "Search completed");

            // Output results to stdout (not logged)
            println!("\nFound {} packages:\n", packages.len());
            for pkg in packages {
                println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
                println!("📦 {} - {}", pkg.name, pkg.full_version());
                println!("   Architecture: {}", pkg.arch);
                println!("   Repository: {}", pkg.repo);
                println!("   Summary: {}", pkg.summary);
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

        Commands::Stats => {
            let _span = tracing::info_span!("stats").entered();
            let api = api::RpmSearchApi::new(config)?;
            let count = api.package_count()?;
            info!(count, "Retrieved statistics");
            println!("Database Statistics:");
            println!("  Total packages: {}", count);
        }

        Commands::ListRepos => {
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

        Commands::RepoStats { repo } => {
            let _span = tracing::info_span!("repo_stats", repo = %repo).entered();
            let api = api::RpmSearchApi::new(config)?;
            let count = api.repo_package_count(&repo)?;

            info!(count, "Retrieved repository statistics");

            println!("\nRepository: {}", repo);
            println!("  Packages: {}", count);
        }

        Commands::DeleteRepo { repo, yes } => {
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

        #[cfg(feature = "mcp")]
        Commands::McpServer => {
            let _span = tracing::info_span!("mcp_server").entered();
            info!("Starting MCP server");
            let server = mcp::McpServer::new(config)?;
            server.run()?;
        }

        #[cfg(feature = "sync")]
        Commands::SyncInit { output } => {
            let _span = tracing::info_span!("sync_init", output = %output.display()).entered();
            info!("Generating example sync configuration");

            let example_config = sync::SyncConfig::example();
            example_config.to_file(&output)?;

            println!("✓ Created example sync configuration: {}", output.display());
            println!("\nEdit this file to configure your repositories, then run:");
            println!("  rpm_repo_search sync-once --config {}", output.display());
            println!(
                "  rpm_repo_search sync-daemon --config {}",
                output.display()
            );
        }

        #[cfg(feature = "sync")]
        Commands::SyncOnce {
            config: sync_config_path,
        } => {
            let _span =
                tracing::info_span!("sync_once", config = %sync_config_path.display()).entered();
            info!("Performing one-time sync");

            let sync_config = sync::SyncConfig::from_file(&sync_config_path)?;
            let scheduler = sync::SyncScheduler::new(sync_config, config);

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
        }

        #[cfg(feature = "sync")]
        Commands::SyncDaemon {
            config: sync_config_path,
        } => {
            let _span =
                tracing::info_span!("sync_daemon", config = %sync_config_path.display()).entered();
            info!("Starting sync daemon");

            let sync_config = sync::SyncConfig::from_file(&sync_config_path)?;
            let scheduler = sync::SyncScheduler::new(sync_config, config);

            println!("Starting sync daemon...");
            println!("Press Ctrl+C to stop");

            let runtime = tokio::runtime::Runtime::new().map_err(|e| {
                error::RpmSearchError::Config(format!("Failed to create runtime: {}", e))
            })?;

            runtime.block_on(scheduler.run_daemon())?;
        }

        #[cfg(feature = "sync")]
        Commands::SyncStatus => {
            let _span = tracing::info_span!("sync_status").entered();
            info!("Retrieving sync status");

            let conn = rusqlite::Connection::open(&db_path)?;
            let state_store = sync::SyncStateStore::new(conn)?;
            let states = state_store.list_states()?;

            if states.is_empty() {
                println!("No sync state found. Run 'sync-once' or 'sync-daemon' first.");
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
    }

    Ok(())
}
