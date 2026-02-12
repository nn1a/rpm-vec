use crate::config::ModelType;
use crate::error::{Result, RpmSearchError};
use rusqlite::Connection;

pub struct VectorStore {
    conn: Connection,
}

impl VectorStore {
    /// Create a new vector store (using the same connection as PackageStore)
    pub fn new(conn: Connection) -> Result<Self> {
        Ok(Self { conn })
    }

    /// Record embedding model info in the metadata table
    pub fn set_embedding_model_info(&self, model_type: &ModelType) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        self.conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES ('embedding_model_type', ?)",
            [model_type.as_db_str()],
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES ('embedding_model_name', ?)",
            [model_type.display_name()],
        )?;
        self.conn.execute(
            "INSERT OR REPLACE INTO metadata (key, value) VALUES ('embedding_built_at', ?)",
            [&now],
        )?;
        Ok(())
    }

    /// Get the embedding model type string from DB metadata
    pub fn get_embedding_model_type(&self) -> Result<Option<String>> {
        match self.conn.query_row(
            "SELECT value FROM metadata WHERE key = 'embedding_model_type'",
            [],
            |row| row.get(0),
        ) {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => {
                // metadata table might not exist yet
                tracing::debug!("Could not read embedding_model_type: {}", e);
                Ok(None)
            }
        }
    }

    /// Reinitialize vector table (drop and recreate) - used when rebuilding embeddings
    pub fn reinitialize(&self, dimension: usize) -> Result<()> {
        use tracing::{debug, info};

        // Try to delete all existing rows first
        match self.conn.execute("DELETE FROM embeddings", []) {
            Ok(n) => info!(deleted = n, "Cleared existing vector embeddings"),
            Err(e) => debug!("No existing embeddings table to clear: {}", e),
        }

        // Drop the table completely to get a fresh start
        match self.conn.execute("DROP TABLE IF EXISTS embeddings", []) {
            Ok(_) => info!("Dropped embeddings table"),
            Err(e) => debug!("Could not drop embeddings table: {}", e),
        }

        // Recreate the virtual table with cosine distance metric
        self.conn.execute(
            &format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS embeddings USING vec0(
                        pkg_id INTEGER PRIMARY KEY,
                        embedding FLOAT[{}] distance_metric=cosine
                    )",
                dimension
            ),
            [],
        )?;
        info!(
            dimension,
            "Created fresh embeddings table with cosine metric"
        );

        Ok(())
    }

    /// Ensure embeddings table exists (create if not, keep data if it does)
    pub fn ensure_table(&self, dimension: usize) -> Result<()> {
        self.conn.execute(
            &format!(
                "CREATE VIRTUAL TABLE IF NOT EXISTS embeddings USING vec0(
                        pkg_id INTEGER PRIMARY KEY,
                        embedding FLOAT[{}] distance_metric=cosine
                    )",
                dimension
            ),
            [],
        )?;
        Ok(())
    }

    /// Get all pkg_ids that already have embeddings
    pub fn get_embedded_pkg_ids(&self) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT pkg_id FROM embeddings")
            .map_err(|e| {
                RpmSearchError::Storage(format!("Failed to query embeddings table: {}", e))
            })?;

        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<std::result::Result<Vec<i64>, _>>()?;

        Ok(ids)
    }

    /// Insert or update embedding for a package
    #[allow(dead_code)]
    pub fn insert_embedding(&self, pkg_id: i64, embedding: &[f32]) -> Result<()> {
        let embedding_json = serde_json::to_string(embedding).map_err(|e| {
            RpmSearchError::Storage(format!("Failed to serialize embedding: {}", e))
        })?;

        self.conn.execute(
            "INSERT OR REPLACE INTO embeddings (pkg_id, embedding) VALUES (?, ?)",
            rusqlite::params![pkg_id, embedding_json],
        )?;

        Ok(())
    }

    /// Batch insert embeddings in a single transaction
    pub fn insert_embeddings_batch(&self, items: &[(i64, Vec<f32>)]) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;

        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR REPLACE INTO embeddings (pkg_id, embedding) VALUES (?, ?)",
            )?;

            for (pkg_id, embedding) in items {
                let embedding_json = serde_json::to_string(embedding).map_err(|e| {
                    RpmSearchError::Storage(format!("Failed to serialize embedding: {}", e))
                })?;
                stmt.execute(rusqlite::params![pkg_id, embedding_json])?;
            }
        }

        tx.commit()?;
        Ok(())
    }

    /// Get embedding for a package (fallback only, not used currently)
    #[allow(dead_code)]
    pub fn get_embedding(&self, _pkg_id: i64) -> Result<Option<Vec<f32>>> {
        // sqlite-vec doesn't support reading embeddings directly
        // This is not typically needed for vector search
        Ok(None)
    }

    /// Perform KNN search (using sqlite-vec if enabled, fallback to full scan)
    pub fn search_similar(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<(i64, f32)>> {
        // Use sqlite-vec's efficient KNN search
        let embedding_json = serde_json::to_string(query_embedding).map_err(|e| {
            RpmSearchError::Storage(format!("Failed to serialize query embedding: {}", e))
        })?;

        let mut stmt = self.conn.prepare(
            "SELECT pkg_id, distance 
                 FROM embeddings 
                 WHERE embedding MATCH ? 
                 ORDER BY distance 
                 LIMIT ?",
        )?;

        let results: Vec<(i64, f32)> = stmt
            .query_map(rusqlite::params![embedding_json, top_k as i64], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        // Convert cosine distance to cosine similarity
        // cosine_distance = 1 - cosine_similarity
        // Therefore: cosine_similarity = 1 - cosine_distance
        let similarities: Vec<(i64, f32)> = results
            .into_iter()
            .map(|(id, dist)| {
                let cos_sim = (1.0 - dist).clamp(0.0, 1.0);
                (id, cos_sim)
            })
            .collect();

        Ok(similarities)
    }

    /// Perform KNN search within filtered candidates (pre-filtering optimization)
    pub fn search_similar_filtered(
        &self,
        query_embedding: &[f32],
        candidate_ids: &[i64],
        top_k: usize,
    ) -> Result<Vec<(i64, f32)>> {
        use std::collections::HashSet;

        // Convert to HashSet for O(1) lookup
        let candidate_set: HashSet<i64> = candidate_ids.iter().copied().collect();

        // With sqlite-vec, we do a broader scan then filter by candidates
        // Request more results to account for filtered-out candidates
        let scan_limit = (top_k * 10).max(200);

        let embedding_json = serde_json::to_string(query_embedding).map_err(|e| {
            RpmSearchError::Storage(format!("Failed to serialize query embedding: {}", e))
        })?;

        let mut stmt = self.conn.prepare(
            "SELECT pkg_id, distance 
                 FROM embeddings 
                 WHERE embedding MATCH ?
                 ORDER BY distance
                 LIMIT ?",
        )?;

        let mut results: Vec<(i64, f32)> = stmt
            .query_map(
                rusqlite::params![embedding_json, scan_limit as i64],
                |row| {
                    let pkg_id: i64 = row.get(0)?;
                    let dist: f64 = row.get(1)?;
                    Ok((pkg_id, dist as f32))
                },
            )?
            .filter_map(|result| result.ok())
            .filter(|(pkg_id, _)| candidate_set.contains(pkg_id))
            .map(|(id, dist)| {
                // Convert cosine distance to cosine similarity
                let cos_sim = (1.0 - dist).clamp(0.0, 1.0);
                (id, cos_sim)
            })
            .collect();

        // Sort by similarity (descending)
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        results.truncate(top_k);

        Ok(results)
    }
}
