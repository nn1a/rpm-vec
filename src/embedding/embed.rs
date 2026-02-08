use crate::embedding::model::EmbeddingModel;
use crate::error::{Result, RpmSearchError};
use std::path::Path;

use tokenizers::Tokenizer;

pub struct Embedder {
    model: EmbeddingModel,
    tokenizer: Tokenizer,
}

impl Embedder {
    /// Create a new embedder
    pub fn new<P: AsRef<Path>>(model_path: P, tokenizer_path: P) -> Result<Self> {
        let model = EmbeddingModel::load(&model_path)?;

        let tokenizer_path_ref = tokenizer_path.as_ref();
        if !tokenizer_path_ref.exists() {
            return Err(RpmSearchError::ModelLoad(format!(
                "Tokenizer not found: {}\n\n\
                Please download the all-MiniLM-L6-v2 model:\n\
                1. Visit: https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2\n\
                2. Download: config.json, model.safetensors, tokenizer.json\n\
                3. Place in: {}",
                tokenizer_path_ref.display(),
                model_path.as_ref().display()
            )));
        }

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| RpmSearchError::ModelLoad(format!("Failed to load tokenizer: {}", e)))?;

        Ok(Self { model, tokenizer })
    }

    /// Embed a single text
    pub fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| RpmSearchError::Embedding(format!("Tokenization failed: {}", e)))?;

        let token_ids = encoding.get_ids().to_vec();
        let attention_mask = encoding.get_attention_mask().to_vec();
        let embeddings = self.model.embed_batch(&[token_ids], &[attention_mask])?;

        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| RpmSearchError::Embedding("No embedding generated".to_string()))
    }

    /// Embed multiple texts in batch
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Tokenize all texts
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), true)
            .map_err(|e| RpmSearchError::Embedding(format!("Batch tokenization failed: {}", e)))?;

        let token_ids: Vec<Vec<u32>> = encodings.iter().map(|e| e.get_ids().to_vec()).collect();
        let attention_masks: Vec<Vec<u32>> = encodings
            .iter()
            .map(|e| e.get_attention_mask().to_vec())
            .collect();

        self.model.embed_batch(&token_ids, &attention_masks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // Requires model files to be present
    fn test_embedding() {
        let embedder = Embedder::new(
            "models/all-MiniLM-L6-v2",
            "models/all-MiniLM-L6-v2/tokenizer.json",
        )
        .unwrap();

        let text = "This is a test sentence for embedding.";
        let embedding = embedder.embed(text).unwrap();

        assert_eq!(embedding.len(), 384); // MiniLM-L6-v2 dimension
    }
}
