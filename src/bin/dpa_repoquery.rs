use rpm_repo_search::api;
use rpm_repo_search::config::Config;
use rpm_repo_search::error::{Result, RpmSearchError};
use rpm_repo_search::gbs;
use rpm_repo_search::normalize::Package;
use rpm_repo_search::storage::FindFilter;
use rpm_repo_search::sync;

use clap::Parser;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::info;

const CACHE_DIR: &str = ".cache/dpa";
const DB_FILENAME: &str = "packages.db";

#[derive(Parser)]
#[command(name = "dpa_repoquery")]
#[command(about = "Query packages from GBS-configured RPM repositories")]
struct Cli {
    /// Package name or glob pattern (e.g., "bash", "lib*ssl*")
    package: Option<String>,

    // -- GBS config --
    /// GBS configuration file path (default: ~/.gbs.conf)
    #[arg(long, value_name = "PATH")]
    gbs_conf: Option<PathBuf>,

    /// GBS profile to use (default: from gbs.conf [general] section)
    #[arg(long)]
    gbs_profile: Option<String>,

    // -- Query mode --
    /// Find packages that provide a capability (e.g., "libssl.so*")
    #[arg(long)]
    whatprovides: Option<String>,

    /// Find packages that require a capability
    #[arg(long)]
    whatrequires: Option<String>,

    /// Find packages that own a specific file (e.g., "/usr/bin/python*", "*.so")
    #[arg(long)]
    file: Option<String>,

    // -- Additional filters --
    /// Summary keyword pattern
    #[arg(short, long)]
    summary: Option<String>,

    /// Description keyword pattern
    #[arg(long)]
    description: Option<String>,

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
    /// %{summary}, %{description}, %{license}, %{repo}, %{vcs}, %{nevra},
    /// %{location}, %{download_url})
    #[arg(long)]
    queryformat: Option<String>,

    // -- Filters --
    /// Filter by architecture
    #[arg(short, long)]
    arch: Option<String>,

    /// Filter by repository (can be specified multiple times)
    #[arg(long)]
    repo: Vec<String>,

    /// Show only the latest version per package name+arch
    #[arg(long)]
    latest: bool,

    /// Maximum results
    #[arg(long, default_value = "200")]
    limit: usize,

    /// Skip repository sync (use cached database only)
    #[arg(long)]
    no_sync: bool,
}

/// Get the default GBS config path (~/.gbs.conf)
fn default_gbs_conf_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| RpmSearchError::Config("Cannot determine home directory".to_string()))?;
    Ok(home.join(".gbs.conf"))
}

/// Get DB path at ~/.cache/dpa/packages.db
fn get_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| RpmSearchError::Config("Cannot determine home directory".to_string()))?;
    let cache_dir = home.join(CACHE_DIR);
    std::fs::create_dir_all(&cache_dir).map_err(RpmSearchError::Io)?;
    Ok(cache_dir.join(DB_FILENAME))
}

/// Sync repositories from GBS config into the database
fn sync_repos(gbs_config: &gbs::GbsConfig, profile: Option<&str>, config: &Config) -> Result<()> {
    let mut sync_config = gbs_config.to_sync_config(profile)?;

    // dpa_repoquery always fetches filelists for --file / --list queries
    for repo in &mut sync_config.repositories {
        repo.sync_filelists = true;
    }

    // Use ~/.cache/dpa/ as work directory for temporary downloads
    let work_dir = config
        .db_path
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join(".sync-work");

    for repo_config in &sync_config.repositories {
        if !repo_config.enabled {
            continue;
        }

        let api = api::RpmSearchApi::new(config.clone())?;
        let state_conn = rusqlite::Connection::open(&config.db_path)?;
        let state_store = sync::SyncStateStore::new(state_conn)?;

        let mut syncer = sync::syncer::RepoSyncer::new(api, state_store, work_dir.clone())?;

        match syncer.sync_repository(repo_config) {
            Ok(result) => {
                if result.changed {
                    info!(
                        repo = %repo_config.name,
                        packages = result.packages_synced,
                        "Repository updated"
                    );
                    eprintln!(
                        "Synced: {} ({} packages)",
                        repo_config.name, result.packages_synced
                    );
                }
            }
            Err(e) => {
                eprintln!("Warning: failed to sync '{}': {}", repo_config.name, e);
            }
        }
    }

    Ok(())
}

// ── Repoquery helpers ────────────────────────────────────────────────

fn format_querystring(fmt: &str, pkg: &Package, download_url: Option<&str>) -> String {
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
        .replace("%{location}", pkg.location_href.as_deref().unwrap_or(""))
        .replace("%{download_url}", download_url.unwrap_or(""))
        .replace(
            "%{nevra}",
            &format!("{}-{}.{}", pkg.name, pkg.full_version(), pkg.arch),
        )
        .replace("\\n", "\n")
        .replace("\\t", "\t")
}

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

fn build_download_url(state_store: &sync::SyncStateStore, pkg: &Package) -> Option<String> {
    let location = pkg.location_href.as_deref()?;
    let base_url = state_store.get_base_url(&pkg.repo).ok()??;
    Some(format!(
        "{}/{}",
        base_url.trim_end_matches('/'),
        location.trim_start_matches('/')
    ))
}

fn main() -> Result<()> {
    // Restore default SIGPIPE handling so piping to head/grep etc. exits cleanly
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::filter::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::filter::EnvFilter::new("warn")),
        )
        .with_target(false)
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();

    // 1. Resolve GBS config path
    let gbs_conf_path = match cli.gbs_conf {
        Some(ref p) => p.clone(),
        None => default_gbs_conf_path()?,
    };

    // 2. Parse GBS config
    let gbs_config = gbs::GbsConfig::from_path(&gbs_conf_path)?;

    // 3. Resolve DB path
    let db_path = get_db_path()?;
    let config = Config::new(db_path);

    // 4. Sync repositories (unless --no-sync)
    if !cli.no_sync {
        sync_repos(&gbs_config, cli.gbs_profile.as_deref(), &config)?;
    }

    // 5. Resolve repo names for filtering
    let mut repos = cli.repo.clone();
    let gbs_repos = gbs_config.get_repo_urls(cli.gbs_profile.as_deref())?;
    for (name, _url) in gbs_repos {
        if !repos.contains(&name) {
            repos.push(name);
        }
    }

    // 6. Execute repoquery
    let db_path = config.db_path.clone();
    let api = api::RpmSearchApi::new(config)?;

    let filter = FindFilter {
        name: cli.package.clone(),
        summary: cli.summary,
        description: cli.description,
        provides: cli.whatprovides.clone(),
        requires: cli.whatrequires.clone(),
        file: cli.file.clone(),
        arch: cli.arch.clone(),
        repos: repos.clone(),
        limit: cli.limit,
    };

    let has_any_condition = filter.name.is_some()
        || filter.summary.is_some()
        || filter.description.is_some()
        || filter.provides.is_some()
        || filter.requires.is_some()
        || filter.file.is_some()
        || filter.arch.is_some()
        || !filter.repos.is_empty();

    let mut packages = if has_any_condition {
        api.find(&filter)?
    } else {
        let all_filter = FindFilter {
            name: Some("*".to_string()),
            limit: cli.limit,
            ..Default::default()
        };
        api.find(&all_filter)?
    };

    // Filter: --latest
    if cli.latest {
        packages = filter_latest(packages);
    }

    if packages.is_empty() {
        if let Some(ref p) = cli.package {
            println!("No packages found matching '{}'", p);
        } else if let Some(ref cap) = cli.whatprovides {
            println!("No packages found providing '{}'", cap);
        } else if let Some(ref cap) = cli.whatrequires {
            println!("No packages found requiring '{}'", cap);
        } else if let Some(ref f) = cli.file {
            println!("No packages found owning '{}'", f);
        } else {
            println!("No packages found.");
        }
        return Ok(());
    }

    // 7. Output
    let state_store = {
        let conn = rusqlite::Connection::open(&db_path)?;
        sync::SyncStateStore::new(conn)?
    };

    if cli.info {
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
            if let Some(ref loc) = pkg.location_href {
                println!("Location    : {}", loc);
            }
            if let Some(url) = build_download_url(&state_store, pkg) {
                println!("URL         : {}", url);
            }
            println!("Description : {}", pkg.description);
            println!();
        }
    } else if cli.requires {
        for (i, pkg) in packages.iter().enumerate() {
            if packages.len() > 1 {
                if i > 0 {
                    println!();
                }
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
    } else if cli.provides {
        for (i, pkg) in packages.iter().enumerate() {
            if packages.len() > 1 {
                if i > 0 {
                    println!();
                }
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
    } else if cli.list {
        for (i, pkg) in packages.iter().enumerate() {
            if packages.len() > 1 {
                if i > 0 {
                    println!();
                }
                println!("# {}-{}.{}", pkg.name, pkg.full_version(), pkg.arch);
            }
            if pkg.pkg_id.is_some() {
                let pkg_repo = vec![pkg.repo.clone()];
                let files = api.list_package_files(&pkg.name, Some(&pkg.arch), &pkg_repo)?;
                let mut found = false;
                for (_, file_list) in &files {
                    for (path, _) in file_list {
                        println!("{}", path);
                        found = true;
                    }
                }
                if !found {
                    println!("  (no filelists indexed)");
                }
            }
        }
    } else if let Some(ref fmt) = cli.queryformat {
        for pkg in &packages {
            let url = build_download_url(&state_store, pkg);
            print!("{}", format_querystring(fmt, pkg, url.as_deref()));
        }
    } else {
        // Default: NEVRA output
        for pkg in &packages {
            println!("{}-{}.{}", pkg.name, pkg.full_version(), pkg.arch);
        }
    }

    Ok(())
}
