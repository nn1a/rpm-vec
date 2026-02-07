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

    /// Initialize vector table (sqlite-vec if feature enabled, fallback otherwise)
    pub fn initialize(&self, dimension: usize) -> Result<()> {
        #[cfg(feature = "sqlite-vec")]
        {
            // Use sqlite-vec virtual table (statically linked)
            self.conn.execute(
                &format!(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS vec_pkg_embedding USING vec0(
                        pkg_id INTEGER PRIMARY KEY,
                        embedding FLOAT[{}]
                    )",
                    dimension
                ),
                [],
            )?;
        }

        #[cfg(not(feature = "sqlite-vec"))]
        {
            // Fallback: simple BLOB storage (when embedding is disabled)
            self.conn.execute(
                "CREATE TABLE IF NOT EXISTS pkg_embedding (
                    pkg_id INTEGER PRIMARY KEY,
                    embedding BLOB NOT NULL
                )",
                [],
            )?;
        }

        Ok(())
    }

    /// Reinitialize vector table (drop and recreate) - used when rebuilding embeddings
    pub fn reinitialize(&self, dimension: usize) -> Result<()> {
        #[cfg(feature = "sqlite-vec")]
        {
            use tracing::{debug, info};

            // Try to delete all existing rows first
            match self.conn.execute("DELETE FROM vec_pkg_embedding", []) {
                Ok(n) => info!(deleted = n, "Cleared existing vector embeddings"),
                Err(e) => debug!("No existing vec_pkg_embedding to clear: {}", e),
            }

            // Drop the table completely to get a fresh start
            match self
                .conn
                .execute("DROP TABLE IF EXISTS vec_pkg_embedding", [])
            {
                Ok(_) => info!("Dropped vec_pkg_embedding table"),
                Err(e) => debug!("Could not drop vec_pkg_embedding: {}", e),
            }

            // Recreate the virtual table
            self.conn.execute(
                &format!(
                    "CREATE VIRTUAL TABLE IF NOT EXISTS vec_pkg_embedding USING vec0(
                        pkg_id INTEGER PRIMARY KEY,
                        embedding FLOAT[{}]
                    )",
                    dimension
                ),
                [],
            )?;
            info!(dimension, "Created fresh vec_pkg_embedding table");
        }

        #[cfg(not(feature = "sqlite-vec"))]
        {
            let _ = self.conn.execute("DROP TABLE IF EXISTS pkg_embedding", []);
            self.conn.execute(
                "CREATE TABLE IF NOT EXISTS pkg_embedding (
                    pkg_id INTEGER PRIMARY KEY,
                    embedding BLOB NOT NULL
                )",
                [],
            )?;
        }

        Ok(())
    }

    /// Insert or update embedding for a package
    pub fn insert_embedding(&self, pkg_id: i64, embedding: &[f32]) -> Result<()> {
        #[cfg(feature = "sqlite-vec")]
        {
            // Use sqlite-vec format: convert to JSON array
            let embedding_json = serde_json::to_string(embedding).map_err(|e| {
                RpmSearchError::Storage(format!("Failed to serialize embedding: {}", e))
            })?;

            self.conn.execute(
                "INSERT OR REPLACE INTO vec_pkg_embedding (pkg_id, embedding) VALUES (?, ?)",
                rusqlite::params![pkg_id, embedding_json],
            )?;
        }

        #[cfg(not(feature = "sqlite-vec"))]
        {
            // Fallback: Convert f32 array to bytes
            let bytes = embedding
                .iter()
                .flat_map(|f| f.to_le_bytes())
                .collect::<Vec<u8>>();

            self.conn.execute(
                "INSERT OR REPLACE INTO pkg_embedding (pkg_id, embedding) VALUES (?, ?)",
                rusqlite::params![pkg_id, bytes],
            )?;
        }

        Ok(())
    }

    /// Get embedding for a package (fallback only, not used currently)
    #[allow(dead_code)]
    pub fn get_embedding(&self, _pkg_id: i64) -> Result<Option<Vec<f32>>> {
        #[cfg(feature = "sqlite-vec")]
        {
            // sqlite-vec doesn't support reading embeddings directly
            // This is not typically needed for vector search
            Ok(None)
        }

        #[cfg(not(feature = "sqlite-vec"))]
        {
            let bytes: Option<Vec<u8>> = self
                .conn
                .query_row(
                    "SELECT embedding FROM pkg_embedding WHERE pkg_id = ?",
                    [pkg_id],
                    |row| row.get(0),
                )
                .optional()?;

            if let Some(bytes) = bytes {
                let embedding: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                Ok(Some(embedding))
            } else {
                Ok(None)
            }
        }
    }

    /// Perform KNN search (using sqlite-vec if enabled, fallback to full scan)
    pub fn search_similar(&self, query_embedding: &[f32], top_k: usize) -> Result<Vec<(i64, f32)>> {
        #[cfg(feature = "sqlite-vec")]
        {
            // Use sqlite-vec's efficient KNN search
            let embedding_json = serde_json::to_string(query_embedding).map_err(|e| {
                RpmSearchError::Storage(format!("Failed to serialize query embedding: {}", e))
            })?;

            let mut stmt = self.conn.prepare(
                "SELECT pkg_id, distance 
                 FROM vec_pkg_embedding 
                 WHERE embedding MATCH ? 
                 ORDER BY distance 
                 LIMIT ?",
            )?;

            let results: Vec<(i64, f32)> = stmt
                .query_map(rusqlite::params![embedding_json, top_k as i64], |row| {
                    Ok((row.get(0)?, row.get(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            // Convert L2 distance to cosine similarity for normalized vectors:
            // L2_dist = sqrt(2 * (1 - cos_sim))
            // cos_sim = 1 - L2_dist^2 / 2
            let similarities: Vec<(i64, f32)> = results
                .into_iter()
                .map(|(id, dist)| {
                    let cos_sim = (1.0 - dist * dist / 2.0).clamp(0.0, 1.0);
                    (id, cos_sim)
                })
                .collect();

            Ok(similarities)
        }

        #[cfg(not(feature = "sqlite-vec"))]
        {
            // Fallback: Full scan with manual cosine similarity
            let mut stmt = self
                .conn
                .prepare("SELECT pkg_id, embedding FROM pkg_embedding")?;

            let mut results: Vec<(i64, f32)> = stmt
                .query_map([], |row| {
                    let pkg_id: i64 = row.get(0)?;
                    let bytes: Vec<u8> = row.get(1)?;
                    let embedding: Vec<f32> = bytes
                        .chunks_exact(4)
                        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                        .collect();

                    let similarity = cosine_similarity(query_embedding, &embedding);
                    Ok((pkg_id, similarity))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;

            // Sort by similarity (descending)
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            results.truncate(top_k);

            Ok(results)
        }
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

        #[cfg(feature = "sqlite-vec")]
        {
            // With sqlite-vec, we do a broader scan then filter by candidates
            // Request more results to account for filtered-out candidates
            let scan_limit = (top_k * 10).max(200);

            let embedding_json = serde_json::to_string(query_embedding).map_err(|e| {
                RpmSearchError::Storage(format!("Failed to serialize query embedding: {}", e))
            })?;

            let mut stmt = self.conn.prepare(
                "SELECT pkg_id, distance 
                 FROM vec_pkg_embedding 
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
                    let cos_sim = (1.0 - dist * dist / 2.0).clamp(0.0, 1.0);
                    (id, cos_sim)
                })
                .collect();

            // Sort by similarity (descending)
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            results.truncate(top_k);

            Ok(results)
        }

        #[cfg(not(feature = "sqlite-vec"))]
        {
            // Fallback: Filter embeddings by candidate_ids
            let mut stmt = self
                .conn
                .prepare("SELECT pkg_id, embedding FROM pkg_embedding")?;

            let mut results: Vec<(i64, f32)> = stmt
                .query_map([], |row| {
                    let pkg_id: i64 = row.get(0)?;
                    let bytes: Vec<u8> = row.get(1)?;
                    Ok((pkg_id, bytes))
                })?
                .filter_map(|result| result.ok())
                .filter_map(|(pkg_id, bytes)| {
                    // Only process if in candidate set
                    if !candidate_set.contains(&pkg_id) {
                        return None;
                    }

                    let embedding: Vec<f32> = bytes
                        .chunks_exact(4)
                        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                        .collect();

                    let similarity = cosine_similarity(query_embedding, &embedding);
                    Some((pkg_id, similarity))
                })
                .collect();

            // Sort by similarity (descending)
            results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            results.truncate(top_k);

            Ok(results)
        }
    }
}

/// Calculate cosine similarity between two vectors
#[cfg(not(feature = "sqlite-vec"))]
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot_product / (norm_a * norm_b)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(not(feature = "sqlite-vec"))]
    fn test_cosine_similarity() {
        use super::cosine_similarity;
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 0.001);

        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 0.0).abs() < 0.001);
    }
}
