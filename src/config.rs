use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Embedding model type
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ModelType {
    /// all-MiniLM-L6-v2 (English, 384 dim, fast)
    #[default]
    Minilm,
    /// multilingual-e5-small (100 languages, 384 dim, requires prefix)
    E5Multilingual,
}

impl ModelType {
    /// Default model directory path
    pub fn default_model_path(&self) -> PathBuf {
        match self {
            ModelType::Minilm => PathBuf::from("models/all-MiniLM-L6-v2"),
            ModelType::E5Multilingual => PathBuf::from("models/multilingual-e5-small"),
        }
    }

    /// Default tokenizer file path
    pub fn default_tokenizer_path(&self) -> PathBuf {
        match self {
            ModelType::Minilm => PathBuf::from("models/all-MiniLM-L6-v2/tokenizer.json"),
            ModelType::E5Multilingual => {
                PathBuf::from("models/multilingual-e5-small/tokenizer.json")
            }
        }
    }

    /// Model display name for messages
    pub fn display_name(&self) -> &'static str {
        match self {
            ModelType::Minilm => "all-MiniLM-L6-v2",
            ModelType::E5Multilingual => "multilingual-e5-small",
        }
    }

    /// Model type string for DB metadata storage
    pub fn as_db_str(&self) -> &'static str {
        match self {
            ModelType::Minilm => "minilm",
            ModelType::E5Multilingual => "e5-multilingual",
        }
    }

    /// Parse from DB metadata string
    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "minilm" => Some(ModelType::Minilm),
            "e5-multilingual" => Some(ModelType::E5Multilingual),
            _ => None,
        }
    }

    /// HuggingFace model URL for download instructions
    pub fn huggingface_url(&self) -> &'static str {
        match self {
            ModelType::Minilm => "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2",
            ModelType::E5Multilingual => "https://huggingface.co/intfloat/multilingual-e5-small",
        }
    }

    /// Whether this model requires query/passage prefix
    pub fn requires_prefix(&self) -> bool {
        match self {
            ModelType::Minilm => false,
            ModelType::E5Multilingual => true,
        }
    }
}

impl std::fmt::Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Database file path
    pub db_path: PathBuf,

    /// Embedding model type
    pub model_type: ModelType,

    /// Embedding model path (local)
    pub model_path: PathBuf,

    /// Tokenizer path (local)
    pub tokenizer_path: PathBuf,

    /// Vector dimension (384 for both MiniLM-L6-v2 and multilingual-e5-small)
    pub embedding_dim: usize,

    /// Batch size for embedding
    pub batch_size: usize,

    /// Top-N results for vector search
    pub top_k: usize,
}

impl Default for Config {
    fn default() -> Self {
        let model_type = ModelType::default();
        Self {
            db_path: PathBuf::from("rpm_search.db"),
            model_path: model_type.default_model_path(),
            tokenizer_path: model_type.default_tokenizer_path(),
            model_type,
            embedding_dim: 384,
            batch_size: 32,
            top_k: 50,
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

    /// Create config with a specific model type, using its default paths
    #[allow(dead_code)]
    pub fn with_model_type(mut self, model_type: ModelType) -> Self {
        self.model_path = model_type.default_model_path();
        self.tokenizer_path = model_type.default_tokenizer_path();
        self.model_type = model_type;
        self
    }
}
