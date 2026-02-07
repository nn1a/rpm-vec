use crate::error::Result;
use crate::normalize::Package;
use crate::search::{SemanticSearch, StructuredSearch};
use serde::{Deserialize, Serialize};

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
    pub repo: Option<String>,
    pub not_requiring: Option<String>,
    pub providing: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub packages: Vec<Package>,
    #[allow(dead_code)]
    pub scores: Vec<f32>,
}

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

    /// Execute a search query with planning
    pub fn search(&self, query: SearchQuery) -> Result<SearchResult> {
        use tracing::{debug, info};

        let top_k = query.top_k.unwrap_or(self.default_top_k);

        // Step 1: If exact name search is requested, use structured search only
        if let Some(ref name) = query.filters.name {
            if query.query_text.is_empty() {
                let packages = self.structured_search.search_by_name(name)?;
                let scores = vec![1.0; packages.len()];
                return Ok(SearchResult { packages, scores });
            }
        }

        // Step 2: Pre-filtering optimization - reduce search space with SQL filters
        let use_prefilter = query.filters.arch.is_some() || query.filters.repo.is_some();

        let vector_results = if use_prefilter {
            // Get candidate pkg_ids using SQL filters (arch, repo)
            let candidates = self.structured_search.get_filtered_candidates(
                query.filters.arch.as_deref(),
                query.filters.repo.as_deref(),
            )?;

            info!(
                total_candidates = candidates.len(),
                arch = ?query.filters.arch,
                repo = ?query.filters.repo,
                "Pre-filtered search space"
            );

            if candidates.is_empty() {
                debug!("No candidates after pre-filtering");
                return Ok(SearchResult {
                    packages: vec![],
                    scores: vec![],
                });
            }

            // Vector search only on filtered candidates
            self.semantic_search
                .search_filtered(&query.query_text, &candidates, top_k)?
        } else {
            // Full vector search (no pre-filtering)
            debug!("Full vector search (no pre-filters)");
            self.semantic_search.search(&query.query_text, top_k)?
        };

        let pkg_ids: Vec<i64> = vector_results.iter().map(|(id, _)| *id).collect();
        let scores: Vec<f32> = vector_results.iter().map(|(_, score)| *score).collect();

        // Step 3: Get package details
        let mut packages = self.structured_search.get_packages(&pkg_ids)?;

        // Step 4: Apply filters
        if let Some(ref arch) = query.filters.arch {
            packages = self.structured_search.filter_by_arch(packages, arch);
        }

        if let Some(ref not_requiring) = query.filters.not_requiring {
            packages = self
                .structured_search
                .filter_not_requiring(packages, not_requiring);
        }

        if let Some(ref providing) = query.filters.providing {
            packages = self.structured_search.filter_providing(packages, providing);
        }

        if let Some(ref repo) = query.filters.repo {
            packages.retain(|p| &p.repo == repo);
        }

        // Adjust scores based on filtered results
        let final_scores = scores[..packages.len().min(scores.len())].to_vec();

        Ok(SearchResult {
            packages,
            scores: final_scores,
        })
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
