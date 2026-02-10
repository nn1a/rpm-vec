use crate::error::Result;
use crate::normalize::Package;
use crate::search::{SemanticSearch, StructuredSearch};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query_text: String,
    pub filters: SearchFilters,
    pub top_k: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilters {
    pub name: Option<String>,
    pub arch: Option<String>,
    pub repos: Vec<String>,
    pub not_requiring: Option<String>,
    pub providing: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub packages: Vec<Package>,
    pub scores: Vec<f32>,
}

/// Weight configuration for hybrid scoring
const STRUCTURED_WEIGHT: f32 = 0.45;
const SEMANTIC_WEIGHT: f32 = 0.55;

/// Minimum score threshold - results below this are filtered out
const MIN_SCORE_THRESHOLD: f32 = 0.15;

pub struct QueryPlanner<'a> {
    semantic_search: SemanticSearch,
    structured_search: StructuredSearch<'a>,
    default_top_k: usize,
}

impl<'a> QueryPlanner<'a> {
    pub fn new(
        semantic_search: SemanticSearch,
        structured_search: StructuredSearch<'a>,
        default_top_k: usize,
    ) -> Self {
        Self {
            semantic_search,
            structured_search,
            default_top_k,
        }
    }

    /// Execute a search query with hybrid planning (structured + semantic)
    pub fn search(&self, query: SearchQuery) -> Result<SearchResult> {
        use tracing::{debug, info};

        let top_k = query.top_k.unwrap_or(self.default_top_k);

        // Step 1: If exact name filter is requested, use structured search only
        if let Some(ref name) = query.filters.name {
            if query.query_text.is_empty() {
                let packages = self.structured_search.search_by_name(name)?;
                let scores = vec![1.0; packages.len()];
                return Ok(SearchResult { packages, scores });
            }
        }

        // Step 2: Hybrid search - run BOTH structured and semantic in parallel
        // The key insight: always run both and combine results

        // 2a: Structured search with ranked scoring
        let structured_results = self
            .structured_search
            .search_by_name_ranked(&query.query_text)?;
        debug!(
            structured_count = structured_results.len(),
            "Structured search results"
        );

        // 2b: Semantic/vector search
        // Expand search to get more candidates for merging
        let semantic_top_k = (top_k * 3).max(30);

        let use_prefilter = query.filters.arch.is_some() || !query.filters.repos.is_empty();

        let vector_results = if use_prefilter {
            let candidates = self
                .structured_search
                .get_filtered_candidates(query.filters.arch.as_deref(), &query.filters.repos)?;

            debug!(
                total_candidates = candidates.len(),
                arch = ?query.filters.arch,
                repos = ?query.filters.repos,
                "Pre-filtered search space"
            );

            if candidates.is_empty() {
                vec![]
            } else {
                self.semantic_search.search_filtered(
                    &query.query_text,
                    &candidates,
                    semantic_top_k,
                )?
            }
        } else {
            self.semantic_search
                .search(&query.query_text, semantic_top_k)?
        };

        debug!(
            semantic_count = vector_results.len(),
            "Semantic search results"
        );

        // Step 3: Merge and score results
        // Use a HashMap to combine scores from both sources
        let mut combined_scores: HashMap<i64, f32> = HashMap::new();

        // Normalize structured scores (already 0-1 from search_by_name_ranked)
        for (pkg_id, score) in &structured_results {
            let weighted = score * STRUCTURED_WEIGHT;
            let entry = combined_scores.entry(*pkg_id).or_insert(0.0);
            *entry += weighted;
        }

        // Semantic scores are now proper cosine similarity in [0, 1] range
        // Use raw scores directly (no min-max normalization to preserve absolute quality)
        for (pkg_id, cos_sim) in &vector_results {
            let weighted = cos_sim * SEMANTIC_WEIGHT;
            let entry = combined_scores.entry(*pkg_id).or_insert(0.0);
            *entry += weighted;
        }

        // Step 4: Sort by combined score
        let mut scored_results: Vec<(i64, f32)> = combined_scores.into_iter().collect();
        scored_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Filter by minimum threshold
        scored_results.retain(|(_, score)| *score >= MIN_SCORE_THRESHOLD);

        // Limit to top_k
        scored_results.truncate(top_k);

        debug!(
            combined_count = scored_results.len(),
            "Combined hybrid results"
        );

        // Step 5: Load package details and apply post-filters
        let mut final_packages: Vec<(Package, f32)> = Vec::new();

        for (pkg_id, score) in &scored_results {
            if let Some(pkg) = self.structured_search.get_package(*pkg_id)? {
                // Apply post-filters
                if let Some(ref arch) = query.filters.arch {
                    if pkg.arch != *arch {
                        continue;
                    }
                }
                if !query.filters.repos.is_empty() && !query.filters.repos.contains(&pkg.repo) {
                    continue;
                }
                if let Some(ref not_requiring) = query.filters.not_requiring {
                    if pkg.requires.iter().any(|r| r.name == *not_requiring) {
                        continue;
                    }
                }
                if let Some(ref providing) = query.filters.providing {
                    if !pkg.provides.iter().any(|prov| prov.name == *providing) {
                        continue;
                    }
                }
                final_packages.push((pkg, *score));
            }
        }

        let packages: Vec<Package> = final_packages.iter().map(|(p, _)| p.clone()).collect();
        let scores: Vec<f32> = final_packages.iter().map(|(_, s)| *s).collect();

        info!(
            results = packages.len(),
            structured_hits = structured_results.len(),
            semantic_hits = vector_results.len(),
            "Hybrid search completed"
        );

        Ok(SearchResult { packages, scores })
    }

    /// Simple search by name only
    #[allow(dead_code)]
    pub fn search_by_name(&self, name: &str) -> Result<Vec<Package>> {
        self.structured_search.search_by_name(name)
    }

    /// Natural language search
    #[allow(dead_code)]
    pub fn semantic_search(&self, query: &str, top_k: Option<usize>) -> Result<SearchResult> {
        let k = top_k.unwrap_or(self.default_top_k);
        let vector_results = self.semantic_search.search(query, k)?;

        let pkg_ids: Vec<i64> = vector_results.iter().map(|(id, _)| *id).collect();
        let scores: Vec<f32> = vector_results.iter().map(|(_, score)| *score).collect();

        let packages = self.structured_search.get_packages(&pkg_ids)?;

        Ok(SearchResult { packages, scores })
    }
}
