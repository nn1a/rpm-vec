pub mod api;
pub mod config;
pub mod error;
pub mod gbs;
pub mod normalize;
pub mod repomd;
pub mod storage;
pub mod sync;

#[cfg(feature = "embedding")]
pub mod embedding;
#[cfg(feature = "embedding")]
pub mod mcp;
#[cfg(feature = "embedding")]
pub mod search;
