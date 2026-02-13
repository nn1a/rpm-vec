use crate::config::ModelType;
use crate::error::{Result, RpmSearchError};
use hf_hub::api::tokio::{Api, ApiBuilder, ApiRepo};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Default HuggingFace Hub endpoint
const DEFAULT_HF_ENDPOINT: &str = "https://huggingface.co";

/// Paths to the required model files
pub struct ModelFiles {
    /// Path to config.json
    pub config: PathBuf,
    /// Path to model.safetensors
    pub weights: PathBuf,
    /// Path to tokenizer.json
    pub tokenizer: PathBuf,
}

/// HuggingFace Hub client for downloading embedding models
///
/// Supported environment variables:
/// - `HF_ENDPOINT`: Custom Hub endpoint URL (e.g., JFrog Artifactory mirror)
/// - `HF_HOME`: Custom cache directory (default: `~/.cache/huggingface/`)
/// - `HF_TOKEN`: Authentication token for private models
///
/// Note: `HF_HUB_ETAG_TIMEOUT` and `HF_HUB_DOWNLOAD_TIMEOUT` are NOT supported
/// by the hf-hub Rust crate (Python huggingface_hub only).
pub struct ModelHub {
    api: Api,
}

impl ModelHub {
    /// Create a new ModelHub, reading configuration from environment variables.
    ///
    /// If `HF_ENDPOINT` is not set, uses the default (`https://huggingface.co`).
    /// Set `HF_ENDPOINT` to use a mirror (e.g., JFrog Artifactory):
    /// ```sh
    /// export HF_ENDPOINT=https://myArtifactory.jfrog.io/artifactory/api/huggingfaceml/repo
    /// ```
    pub fn new() -> Result<Self> {
        let endpoint =
            std::env::var("HF_ENDPOINT").unwrap_or_else(|_| DEFAULT_HF_ENDPOINT.to_string());

        info!(endpoint = %endpoint, "Initializing HuggingFace Hub API");

        // from_env() reads HF_HOME and HF_ENDPOINT from environment
        let api = ApiBuilder::from_env()
            .with_endpoint(endpoint)
            .with_progress(true)
            .build()
            .map_err(|e| {
                RpmSearchError::ModelDownload(format!(
                    "Failed to initialize HuggingFace Hub API: {}",
                    e
                ))
            })?;
        Ok(Self { api })
    }

    /// Download (or retrieve from cache) all required model files
    pub fn get_model_files(&self, model_type: &ModelType) -> Result<ModelFiles> {
        let repo_id = model_type.hf_repo_id();
        let repo = self.api.model(repo_id.to_string());

        info!(
            model = %model_type.display_name(),
            repo = %repo_id,
            "Resolving model files from HuggingFace Hub"
        );

        let config = self.get_file(&repo, "config.json", model_type)?;
        let weights = self.get_file(&repo, "model.safetensors", model_type)?;
        let tokenizer = self.get_file(&repo, "tokenizer.json", model_type)?;

        info!(
            config = %config.display(),
            weights = %weights.display(),
            tokenizer = %tokenizer.display(),
            "Model files resolved"
        );

        Ok(ModelFiles {
            config,
            weights,
            tokenizer,
        })
    }

    /// Check if all required model files are already cached
    pub fn is_cached(model_type: &ModelType) -> bool {
        let repo_id = model_type.hf_repo_id();
        let cache = hf_hub::Cache::default();
        let cache_repo = cache.model(repo_id.to_string());
        ["config.json", "model.safetensors", "tokenizer.json"]
            .iter()
            .all(|f| cache_repo.get(f).is_some())
    }

    fn get_file(&self, repo: &ApiRepo, filename: &str, model_type: &ModelType) -> Result<PathBuf> {
        debug!(file = %filename, "Fetching model file");
        let fetch_result = if let Ok(handle) = tokio::runtime::Handle::try_current() {
            tokio::task::block_in_place(|| handle.block_on(repo.get(filename)))
        } else {
            let runtime = tokio::runtime::Runtime::new().map_err(|e| {
                RpmSearchError::ModelDownload(format!(
                    "Failed to create Tokio runtime for model download: {}",
                    e
                ))
            })?;
            runtime.block_on(repo.get(filename))
        };

        fetch_result.map_err(|e| {
            RpmSearchError::ModelDownload(format!(
                "Failed to download '{}' for {}: {}\n\
                 Model: {}\n\
                 Ensure you have internet access or the model is already cached.",
                filename,
                model_type.display_name(),
                e,
                model_type.huggingface_url(),
            ))
        })
    }
}

/// Resolve model files with fallback: custom paths > local directory > hf-hub download
///
/// Priority:
/// 1. Custom paths provided via CLI (`--model` / `--tokenizer`) - use directly
/// 2. Default local directory (`models/...`) with all files present - use it
/// 3. Download from HuggingFace Hub via hf-hub (cached in `~/.cache/huggingface/`)
pub fn resolve_model_files(
    model_type: &ModelType,
    custom_model_path: Option<&Path>,
    custom_tokenizer_path: Option<&Path>,
) -> Result<ModelFiles> {
    // Case 1: Both custom paths provided
    if let Some(model_dir) = custom_model_path {
        let tokenizer = match custom_tokenizer_path {
            Some(tp) => tp.to_path_buf(),
            None => model_dir.join("tokenizer.json"),
        };
        info!(path = %model_dir.display(), "Using custom model path");
        return Ok(ModelFiles {
            config: model_dir.join("config.json"),
            weights: model_dir.join("model.safetensors"),
            tokenizer,
        });
    }

    // Case 2: Check default local directory
    let default_path = model_type.default_model_path();
    let has_local = default_path.join("config.json").exists()
        && default_path.join("model.safetensors").exists()
        && default_path.join("tokenizer.json").exists();

    if has_local {
        info!(path = %default_path.display(), "Using local model files");
        return Ok(ModelFiles {
            config: default_path.join("config.json"),
            weights: default_path.join("model.safetensors"),
            tokenizer: default_path.join("tokenizer.json"),
        });
    }

    // Case 3: Download via hf-hub
    if ModelHub::is_cached(model_type) {
        info!("Model found in HuggingFace cache");
    } else {
        println!(
            "Model '{}' not found locally. Downloading from HuggingFace Hub...",
            model_type.display_name()
        );
    }

    let hub = ModelHub::new()?;
    hub.get_model_files(model_type)
}
