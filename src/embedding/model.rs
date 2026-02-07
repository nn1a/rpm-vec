#[cfg(feature = "embedding")]
use crate::error::{Result, RpmSearchError};
#[cfg(feature = "embedding")]
use candle_core::{Device, Tensor};
#[cfg(feature = "embedding")]
use candle_nn::VarBuilder;
#[cfg(feature = "embedding")]
use candle_transformers::models::bert::{BertModel, Config};
#[cfg(feature = "embedding")]
use std::path::Path;

#[cfg(feature = "embedding")]
pub struct EmbeddingModel {
    model: BertModel,
    device: Device,
}

#[cfg(feature = "embedding")]
impl EmbeddingModel {
    /// Select the best available device (GPU -> CPU fallback)
    fn select_device() -> Device {
        // Try CUDA (NVIDIA) first if available
        #[cfg(feature = "cuda")]
        {
            match Device::new_cuda(0) {
                Ok(device) => {
                    tracing::info!("🚀 Using CUDA GPU for embeddings");
                    return device;
                }
                Err(e) => {
                    tracing::warn!("CUDA GPU unavailable ({}), trying other options", e);
                }
            }
        }

        // Fallback to CPU (with accelerate if available)
        #[cfg(feature = "accelerate")]
        {
            tracing::info!("💻 Using CPU with Apple Accelerate framework");
        }
        #[cfg(not(feature = "accelerate"))]
        {
            tracing::info!("💻 Using CPU for embeddings");
        }
        Device::Cpu
    }

    /// Load the MiniLM model from local files
    pub fn load<P: AsRef<Path>>(model_path: P) -> Result<Self> {
        let device = Self::select_device();

        // Load model config
        let config_path = model_path.as_ref().join("config.json");
        let config_str = std::fs::read_to_string(&config_path).map_err(|e| {
            RpmSearchError::ModelLoad(format!(
                "Failed to read config from {}: {}\n\n\
                Please download the all-MiniLM-L6-v2 model:\n\
                1. Visit: https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2\n\
                2. Download: config.json, model.safetensors, tokenizer.json\n\
                3. Place in: {}",
                config_path.display(),
                e,
                model_path.as_ref().display()
            ))
        })?;
        let config: Config = serde_json::from_str(&config_str)
            .map_err(|e| RpmSearchError::ModelLoad(format!("Failed to parse config: {}", e)))?;

        // Load model weights
        let weights_path = model_path.as_ref().join("model.safetensors");
        if !weights_path.exists() {
            return Err(RpmSearchError::ModelLoad(format!(
                "Model weights not found: {}\n\n\
                Please download the all-MiniLM-L6-v2 model:\n\
                1. Visit: https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2\n\
                2. Download: config.json, model.safetensors, tokenizer.json\n\
                3. Place in: {}",
                weights_path.display(),
                model_path.as_ref().display()
            )));
        }
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(&[weights_path], candle_core::DType::F32, &device)
                .map_err(|e| RpmSearchError::ModelLoad(format!("Failed to load weights: {}", e)))?
        };

        let model = BertModel::load(vb, &config)
            .map_err(|e| RpmSearchError::ModelLoad(format!("Failed to load model: {}", e)))?;

        Ok(Self { model, device })
    }

    /// Generate embeddings for a batch of texts
    pub fn embed_batch(&self, token_ids: &[Vec<u32>]) -> Result<Vec<Vec<f32>>> {
        let batch_size = token_ids.len();
        if batch_size == 0 {
            return Ok(Vec::new());
        }

        // Find max length
        let max_len = token_ids.iter().map(|ids| ids.len()).max().unwrap_or(0);

        // Pad sequences
        let mut padded_ids = Vec::with_capacity(batch_size);
        for ids in token_ids {
            let mut padded = ids.clone();
            padded.resize(max_len, 0); // 0 is typically the PAD token
            padded_ids.push(padded);
        }

        // Convert to tensor
        let ids_flat: Vec<u32> = padded_ids.iter().flatten().copied().collect();
        let ids_tensor = Tensor::from_vec(ids_flat, (batch_size, max_len), &self.device)
            .map_err(|e| RpmSearchError::Embedding(format!("Failed to create tensor: {}", e)))?;

        // Create token_type_ids (all zeros for single sequence)
        let token_type_ids =
            Tensor::zeros((batch_size, max_len), candle_core::DType::U32, &self.device).map_err(
                |e| RpmSearchError::Embedding(format!("Failed to create token_type_ids: {}", e)),
            )?;

        // Run model (Candle 0.9 requires token_type_ids and attention_mask)
        let embeddings = self
            .model
            .forward(&ids_tensor, &token_type_ids, None)
            .map_err(|e| RpmSearchError::Embedding(format!("Model forward failed: {}", e)))?;

        // Mean pooling over sequence dimension
        let pooled = embeddings
            .mean(1)
            .map_err(|e| RpmSearchError::Embedding(format!("Pooling failed: {}", e)))?;

        // Convert to Vec<Vec<f32>>
        let pooled_data = pooled
            .to_vec2::<f32>()
            .map_err(|e| RpmSearchError::Embedding(format!("Conversion failed: {}", e)))?;

        Ok(pooled_data)
    }
}

#[cfg(not(feature = "embedding"))]
pub struct EmbeddingModel;

#[cfg(not(feature = "embedding"))]
impl EmbeddingModel {
    #[allow(dead_code)]
    pub fn load<P: AsRef<std::path::Path>>(_model_path: P) -> crate::error::Result<Self> {
        Err(crate::error::RpmSearchError::ModelLoad(
            "Embedding feature disabled. Rebuild with default features enabled".to_string(),
        ))
    }

    #[allow(dead_code)]
    pub fn embed_batch(&self, _token_ids: &[Vec<u32>]) -> crate::error::Result<Vec<Vec<f32>>> {
        Err(crate::error::RpmSearchError::Embedding(
            "Embedding feature not enabled".to_string(),
        ))
    }
}
