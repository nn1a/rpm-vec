use thiserror::Error;

#[derive(Error, Debug)]
pub enum RpmSearchError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("XML parsing error: {0}")]
    XmlParse(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Model loading error: {0}")]
    ModelLoad(String),

    #[error("Configuration error: {0}")]
    #[allow(dead_code)]
    Config(String),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Invalid package data: {0}")]
    #[allow(dead_code)]
    InvalidPackage(String),

    #[error("Fetch error: {0}")]
    #[allow(dead_code)]
    Fetch(String),

    #[error("Parse error: {0}")]
    #[allow(dead_code)]
    Parse(String),
}

pub type Result<T> = std::result::Result<T, RpmSearchError>;
