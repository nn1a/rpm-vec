use crate::error::{Result, RpmSearchError};
use candle_core::{Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config};
use std::path::Path;

pub struct EmbeddingModel {
    model: BertModel,
    device: Device,
}

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
    ///
    /// `token_ids`: tokenized input IDs for each text
    /// `attention_masks`: attention masks from the tokenizer (1 = real token, 0 = padding)
    pub fn embed_batch(
        &self,
        token_ids: &[Vec<u32>],
        attention_masks: &[Vec<u32>],
    ) -> Result<Vec<Vec<f32>>> {
        let batch_size = token_ids.len();
        if batch_size == 0 {
            return Ok(Vec::new());
        }

        // Find max length (from the raw token IDs, ignoring tokenizer padding)
        // Use attention mask to find actual token count per sequence
        let actual_lengths: Vec<usize> = attention_masks
            .iter()
            .map(|mask| mask.iter().filter(|&&x| x != 0).count())
            .collect();
        let max_len = actual_lengths.iter().copied().max().unwrap_or(0);

        // Pad sequences and build attention mask based on actual token length
        let mut padded_ids = Vec::with_capacity(batch_size * max_len);
        let mut attention_mask_data = Vec::with_capacity(batch_size * max_len);
        for (idx, ids) in token_ids.iter().enumerate() {
            let actual_len = actual_lengths[idx];
            for i in 0..max_len {
                if i < actual_len && i < ids.len() {
                    padded_ids.push(ids[i]);
                    attention_mask_data.push(1u32);
                } else {
                    padded_ids.push(0u32); // PAD token
                    attention_mask_data.push(0u32);
                }
            }
        }

        // Convert to tensors
        let ids_tensor = Tensor::from_vec(padded_ids, (batch_size, max_len), &self.device)
            .map_err(|e| RpmSearchError::Embedding(format!("Failed to create tensor: {}", e)))?;

        // Create token_type_ids (all zeros for single sequence)
        let token_type_ids =
            Tensor::zeros((batch_size, max_len), candle_core::DType::U32, &self.device).map_err(
                |e| RpmSearchError::Embedding(format!("Failed to create token_type_ids: {}", e)),
            )?;

        // Create attention mask tensor
        let attention_mask = Tensor::from_vec(
            attention_mask_data.clone(),
            (batch_size, max_len),
            &self.device,
        )
        .map_err(|e| {
            RpmSearchError::Embedding(format!("Failed to create attention_mask: {}", e))
        })?;

        // Run model with attention mask
        let embeddings = self
            .model
            .forward(&ids_tensor, &token_type_ids, Some(&attention_mask))
            .map_err(|e| RpmSearchError::Embedding(format!("Model forward failed: {}", e)))?;

        // Attention-masked mean pooling using matmul (efficient, no broadcast):
        // mask (batch, seq) -> (batch, 1, seq) @ embeddings (batch, seq, hidden) -> (batch, 1, hidden) -> (batch, hidden)
        // Then divide by token count per sequence.
        let mask_f32 = attention_mask
            .to_dtype(candle_core::DType::F32)
            .map_err(|e| RpmSearchError::Embedding(format!("Mask dtype failed: {}", e)))?;

        // (batch, seq) -> (batch, 1, seq)
        let mask_row = mask_f32
            .unsqueeze(1)
            .map_err(|e| RpmSearchError::Embedding(format!("Mask unsqueeze failed: {}", e)))?;

        // matmul: (batch, 1, seq) x (batch, seq, hidden) = (batch, 1, hidden)
        let sum_embeddings = mask_row
            .matmul(&embeddings)
            .map_err(|e| RpmSearchError::Embedding(format!("Matmul pooling failed: {}", e)))?
            .squeeze(1)
            .map_err(|e| RpmSearchError::Embedding(format!("Squeeze failed: {}", e)))?;
        // sum_embeddings: (batch, hidden)

        // Token counts: (batch,) -> (batch, 1) for broadcasting division
        let token_counts = mask_f32
            .sum(1)
            .map_err(|e| RpmSearchError::Embedding(format!("Token count failed: {}", e)))?
            .clamp(1.0f64, f64::MAX)
            .map_err(|e| RpmSearchError::Embedding(format!("Token count clamp failed: {}", e)))?
            .unsqueeze(1)
            .map_err(|e| {
                RpmSearchError::Embedding(format!("Token count unsqueeze failed: {}", e))
            })?;

        // Mean pooling: (batch, hidden) / (batch, 1) - broadcasting handles the division
        let pooled = sum_embeddings
            .broadcast_div(&token_counts)
            .map_err(|e| RpmSearchError::Embedding(format!("Mean division failed: {}", e)))?;

        // L2 normalize: norm = sqrt(sum(x^2)), normalized = x / norm
        let norms = pooled
            .sqr()
            .map_err(|e| RpmSearchError::Embedding(format!("Norm sqr failed: {}", e)))?
            .sum(1)
            .map_err(|e| RpmSearchError::Embedding(format!("Norm sum failed: {}", e)))?
            .sqrt()
            .map_err(|e| RpmSearchError::Embedding(format!("Norm sqrt failed: {}", e)))?
            .clamp(1e-12f64, f64::MAX)
            .map_err(|e| RpmSearchError::Embedding(format!("Norm clamp failed: {}", e)))?
            .unsqueeze(1)
            .map_err(|e| RpmSearchError::Embedding(format!("Norm unsqueeze failed: {}", e)))?;

        let normalized = pooled
            .broadcast_div(&norms)
            .map_err(|e| RpmSearchError::Embedding(format!("Normalization failed: {}", e)))?;

        // Convert to Vec<Vec<f32>>
        let pooled_data = normalized
            .to_vec2::<f32>()
            .map_err(|e| RpmSearchError::Embedding(format!("Conversion failed: {}", e)))?;

        Ok(pooled_data)
    }
}
