use crate::config::ModelType;
use crate::embedding::model::EmbeddingModel;
use crate::error::{Result, RpmSearchError};
use std::path::Path;

use tokenizers::Tokenizer;

pub struct Embedder {
    model: EmbeddingModel,
    tokenizer: Tokenizer,
    model_type: ModelType,
}

impl Embedder {
    /// Create a new embedder with model type for automatic prefix handling
    pub fn new<P: AsRef<Path>>(
        model_path: P,
        tokenizer_path: P,
        model_type: ModelType,
    ) -> Result<Self> {
        let model = EmbeddingModel::load(&model_path, &model_type)?;

        let tokenizer_path_ref = tokenizer_path.as_ref();
        if !tokenizer_path_ref.exists() {
            return Err(RpmSearchError::ModelLoad(format!(
                "Tokenizer not found: {}\n\n\
                Please download the {} model:\n\
                1. Visit: {}\n\
                2. Download: config.json, model.safetensors, tokenizer.json\n\
                3. Place in: {}",
                tokenizer_path_ref.display(),
                model_type.display_name(),
                model_type.huggingface_url(),
                model_path.as_ref().display()
            )));
        }

        let tokenizer = Tokenizer::from_file(tokenizer_path)
            .map_err(|e| RpmSearchError::ModelLoad(format!("Failed to load tokenizer: {}", e)))?;

        Ok(Self {
            model,
            tokenizer,
            model_type,
        })
    }

    /// Get the model type
    pub fn model_type(&self) -> &ModelType {
        &self.model_type
    }

    /// Embed a single search query (auto-adds "query: " prefix for E5 models)
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        if self.model_type.requires_prefix() {
            self.embed(&format!("query: {}", text))
        } else {
            self.embed(text)
        }
    }

    /// Embed multiple documents/passages in batch (auto-adds "passage: " prefix for E5 models)
    pub fn embed_passages(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if self.model_type.requires_prefix() {
            let prefixed: Vec<String> = texts.iter().map(|t| format!("passage: {}", t)).collect();
            self.embed_batch(&prefixed)
        } else {
            self.embed_batch(texts)
        }
    }

    /// Embed a single text (raw, no prefix added)
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

    /// Embed multiple texts in batch (raw, no prefix added)
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
    fn test_embedding_minilm() {
        let embedder = Embedder::new(
            "models/all-MiniLM-L6-v2",
            "models/all-MiniLM-L6-v2/tokenizer.json",
            ModelType::Minilm,
        )
        .unwrap();

        let text = "This is a test sentence for embedding.";
        let embedding = embedder.embed(text).unwrap();

        assert_eq!(embedding.len(), 384); // MiniLM-L6-v2 dimension
    }

    #[test]
    #[ignore] // Requires model files to be present
    fn test_embedding_e5_multilingual() {
        let embedder = Embedder::new(
            "models/multilingual-e5-small",
            "models/multilingual-e5-small/tokenizer.json",
            ModelType::E5Multilingual,
        )
        .unwrap();

        // embed_query should auto-add "query: " prefix
        let embedding = embedder.embed_query("openssl library").unwrap();
        assert_eq!(embedding.len(), 384); // e5-multilingual-small dimension

        // embed_passages should auto-add "passage: " prefix
        let passages = vec!["OpenSSL is a cryptography library".to_string()];
        let embeddings = embedder.embed_passages(&passages).unwrap();
        assert_eq!(embeddings[0].len(), 384);
    }
}
