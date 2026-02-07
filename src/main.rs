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

        /// Force full rebuild (drop all embeddings and regenerate)
        #[arg(long)]
        rebuild: bool,
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
    SyncInit {
        /// Output file path
        #[arg(short, long, default_value = "sync-config.toml")]
        output: PathBuf,
    },

    /// Perform one-time sync of all repositories
    SyncOnce {
        /// Sync configuration file
        #[arg(short, long, default_value = "sync-config.toml")]
        config: PathBuf,
    },

    /// Run sync daemon (continuous background syncing)
    SyncDaemon {
        /// Sync configuration file
        #[arg(short, long, default_value = "sync-config.toml")]
        config: PathBuf,
    },

    /// Show sync status for all repositories
    SyncStatus,

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

    // Try to load CUDA runtime library
    let cuda_available = unsafe {
        !libc::dlopen(
            c"libcuda.so.1".as_ptr(),
            libc::RTLD_LAZY | libc::RTLD_GLOBAL,
        )
        .is_null()
            || !libc::dlopen(c"libcuda.so".as_ptr(), libc::RTLD_LAZY | libc::RTLD_GLOBAL).is_null()
    };

    if cuda_available {
        // Get the path to the current executable
        let exe_path = std::env::current_exe()?;

        // Look for _cuda variant in the same directory
        let cuda_binary = exe_path.parent().map(|p| p.join("rpm_repo_search_cuda"));

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
    let is_mcp_mode = {
        #[cfg(feature = "mcp")]
        {
            std::env::args().any(|a| a == "mcp-server")
        }
        #[cfg(not(feature = "mcp"))]
        {
            false
        }
    };

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
        tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::filter::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("info")),
            )
            .with_target(false)
            .init();
    }

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
            rebuild,
        } => {
            let _span = tracing::info_span!("build_embeddings",
                model = %model.display(),
                tokenizer = %tokenizer.display(),
                verbose,
                rebuild
            )
            .entered();
            info!("Building embeddings");
            let mut config = config;
            config.model_path = model;
            config.tokenizer_path = tokenizer;

            let api = api::RpmSearchApi::new(config.clone())?;
            let embedder = embedding::Embedder::new(&config.model_path, &config.tokenizer_path)?;
            let count = api.build_embeddings(&embedder, verbose, rebuild)?;
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

        Commands::SyncOnce {
            config: sync_config_path,
        } => {
            let _span =
                tracing::info_span!("sync_once", config = %sync_config_path.display()).entered();
            info!("Performing one-time sync");

            let sync_config = sync::SyncConfig::from_file(&sync_config_path)?;
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
            let api = api::RpmSearchApi::new(config.clone())?;
            let embedder = embedding::Embedder::new(&config.model_path, &config.tokenizer_path)?;
            let count = api.build_embeddings(&embedder, false, false)?;
            if count > 0 {
                println!("✅ Built embeddings for {} new packages", count);
            } else {
                println!("✅ All embeddings up to date");
            }
        }

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

        Commands::SyncStatus => {
            let _span = tracing::info_span!("sync_status").entered();
            info!("Retrieving sync status");

            let conn = rusqlite::Connection::open(&config.db_path)?;
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

        Commands::DebugSearch { query, pkg_ids } => {
            let mut config = config;
            config.top_k = 10;

            let embedder = embedding::Embedder::new(&config.model_path, &config.tokenizer_path)?;

            // Embed the query
            let query_embedding = embedder.embed(&query)?;
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
            let ref_embeddings = embedder.embed_batch(&ref_texts)?;

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
