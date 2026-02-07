use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Database file path
    pub db_path: PathBuf,

    /// Embedding model path (local)
    pub model_path: PathBuf,

    /// Tokenizer path (local)
    pub tokenizer_path: PathBuf,

    /// Vector dimension (384 for MiniLM-L6-v2)
    pub embedding_dim: usize,

    /// Batch size for embedding
    pub batch_size: usize,

    /// Top-N results for vector search
    pub top_k: usize,

    /// SQLite-vec extension path (optional, for accelerated vector search)
    pub sqlite_vec_path: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("rpm_search.db"),
            model_path: PathBuf::from("models/all-MiniLM-L6-v2"),
            tokenizer_path: PathBuf::from("models/all-MiniLM-L6-v2/tokenizer.json"),
            embedding_dim: 384,
            batch_size: 32,
            top_k: 50,
            sqlite_vec_path: None,
        }
    }
}

impl Config {
    pub fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            ..Default::default()
        }
    }
}
