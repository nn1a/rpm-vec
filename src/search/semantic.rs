use crate::embedding::Embedder;
use crate::error::Result;
use crate::storage::VectorStore;
use tracing::debug;

pub struct SemanticSearch {
    vector_store: VectorStore,
    embedder: Embedder,
}

impl SemanticSearch {
    pub fn new(vector_store: VectorStore, embedder: Embedder) -> Self {
        Self {
            vector_store,
            embedder,
        }
    }

    /// Search for similar packages using vector similarity
    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<(i64, f32)>> {
        // Embed the query (auto-adds prefix for E5 models)
        let query_embedding = self.embedder.embed_query(query)?;

        // Search similar vectors
        let results = self.vector_store.search_similar(&query_embedding, top_k)?;

        Ok(results)
    }

    /// Search with pre-filtered candidates (optimized for large datasets)
    pub fn search_filtered(
        &self,
        query: &str,
        candidate_ids: &[i64],
        top_k: usize,
    ) -> Result<Vec<(i64, f32)>> {
        debug!(
            candidates = candidate_ids.len(),
            "Performing pre-filtered vector search"
        );

        // Embed the query (auto-adds prefix for E5 models)
        let query_embedding = self.embedder.embed_query(query)?;

        // Search only within candidate IDs
        self.vector_store
            .search_similar_filtered(&query_embedding, candidate_ids, top_k)
    }
}
