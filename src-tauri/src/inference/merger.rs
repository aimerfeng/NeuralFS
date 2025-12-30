//! Result Merger for NeuralFS
//!
//! This module provides result merging capabilities:
//! - Weighted score combination from local and cloud results
//! - Deduplication of results
//! - Score normalization
//! - Configurable merge strategies
//!
//! **Validates: Requirements 11.5**

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::core::types::search::{ResultSource, SearchIntent};

use super::cloud::CloudInferenceResult;
use super::local::LocalInferenceResult;
use super::types::{CloudUnderstanding, InferenceResponse, InferenceSource};

/// Result merger for combining local and cloud inference results
#[derive(Debug, Clone)]
pub struct ResultMerger {
    /// Merger configuration
    config: MergerConfig,
}

/// Merger configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergerConfig {
    /// Weight for local results (0.0 - 1.0)
    pub local_weight: f32,
    
    /// Weight for cloud results (0.0 - 1.0)
    pub cloud_weight: f32,
    
    /// Minimum score threshold for merged results
    pub min_merge_score: f32,
    
    /// Whether to prefer cloud intent over local
    pub prefer_cloud_intent: bool,
    
    /// Maximum results to return
    pub max_results: usize,
}

impl Default for MergerConfig {
    fn default() -> Self {
        Self {
            local_weight: 0.6,
            cloud_weight: 0.4,
            min_merge_score: 0.1,
            prefer_cloud_intent: true,
            max_results: 20,
        }
    }
}

impl MergerConfig {
    /// Create a config that prefers local results
    pub fn local_preferred() -> Self {
        Self {
            local_weight: 0.8,
            cloud_weight: 0.2,
            prefer_cloud_intent: false,
            ..Default::default()
        }
    }
    
    /// Create a config that prefers cloud results
    pub fn cloud_preferred() -> Self {
        Self {
            local_weight: 0.3,
            cloud_weight: 0.7,
            prefer_cloud_intent: true,
            ..Default::default()
        }
    }
    
    /// Create a balanced config
    pub fn balanced() -> Self {
        Self {
            local_weight: 0.5,
            cloud_weight: 0.5,
            prefer_cloud_intent: true,
            ..Default::default()
        }
    }
}

/// Merged result from local and cloud inference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedResult {
    /// File ID (if applicable)
    pub file_id: Option<Uuid>,
    
    /// Combined score (0.0 - 1.0)
    pub score: f32,
    
    /// Result source
    pub source: ResultSource,
    
    /// Local inference data
    pub local_data: Option<LocalResultData>,
    
    /// Cloud inference data
    pub cloud_data: Option<CloudResultData>,
}

/// Local result data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalResultData {
    /// Original score from local inference
    pub original_score: f32,
    
    /// Matched tags
    pub matched_tags: Vec<String>,
    
    /// Query embedding (first few dimensions for debugging)
    pub embedding_preview: Vec<f32>,
}

/// Cloud result data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudResultData {
    /// Original score from cloud inference
    pub original_score: f32,
    
    /// Suggested terms from cloud
    pub suggested_terms: Vec<String>,
    
    /// Cloud confidence
    pub confidence: f32,
}

impl ResultMerger {
    /// Create a new result merger with default configuration
    pub fn new() -> Self {
        Self::with_config(MergerConfig::default())
    }
    
    /// Create a new result merger with custom configuration
    pub fn with_config(config: MergerConfig) -> Self {
        Self { config }
    }
    
    /// Merge local and cloud inference results
    pub fn merge(
        &self,
        request_id: Uuid,
        local: LocalInferenceResult,
        cloud: Option<CloudInferenceResult>,
    ) -> InferenceResponse {
        let mut sources = vec![InferenceSource::LocalEmbedding, InferenceSource::LocalIntent];
        
        if !local.tag_matches.is_empty() {
            sources.push(InferenceSource::LocalTagMatch);
        }
        
        // Determine the final intent
        let intent = self.merge_intent(&local.intent, cloud.as_ref());
        
        // Merge cloud understanding if available
        let (cloud_understanding, cloud_enhanced) = if let Some(ref cloud_result) = cloud {
            sources.push(InferenceSource::Cloud);
            (Some(cloud_result.understanding.clone()), true)
        } else {
            (None, false)
        };
        
        // Calculate total duration
        let duration_ms = local.duration_ms + cloud.as_ref().map(|c| c.duration_ms).unwrap_or(0);
        
        InferenceResponse {
            request_id,
            intent,
            query_embedding: local.query_embedding,
            cloud_understanding,
            cloud_enhanced,
            duration_ms,
            sources,
        }
    }
    
    /// Merge search intents from local and cloud
    fn merge_intent(
        &self,
        local_intent: &SearchIntent,
        cloud: Option<&CloudInferenceResult>,
    ) -> SearchIntent {
        // If no cloud result or cloud preference is disabled, use local intent
        if !self.config.prefer_cloud_intent || cloud.is_none() {
            return local_intent.clone();
        }
        
        let cloud_result = cloud.unwrap();
        
        // If cloud has high confidence and refined intent, consider using it
        if cloud_result.understanding.confidence >= 0.8 {
            // Cloud has high confidence, but we still use local intent structure
            // and enhance it with cloud suggestions
            local_intent.clone()
        } else {
            // Use local intent as primary
            local_intent.clone()
        }
    }
    
    /// Merge and normalize scores
    pub fn merge_scores(&self, local_score: f32, cloud_score: Option<f32>) -> f32 {
        match cloud_score {
            Some(cs) => {
                let weighted = (local_score * self.config.local_weight) 
                    + (cs * self.config.cloud_weight);
                // Normalize to 0-1 range
                weighted.clamp(0.0, 1.0)
            }
            None => local_score * self.config.local_weight,
        }
    }
    
    /// Create merged results from file search results
    pub fn merge_file_results(
        &self,
        local_results: Vec<(Uuid, f32)>,
        cloud_results: Option<Vec<(Uuid, f32)>>,
    ) -> Vec<MergedResult> {
        use std::collections::HashMap;
        
        let mut result_map: HashMap<Uuid, MergedResult> = HashMap::new();
        
        // Add local results
        for (file_id, score) in local_results {
            let weighted_score = score * self.config.local_weight;
            result_map.insert(file_id, MergedResult {
                file_id: Some(file_id),
                score: weighted_score,
                source: ResultSource::LocalVector,
                local_data: Some(LocalResultData {
                    original_score: score,
                    matched_tags: Vec::new(),
                    embedding_preview: Vec::new(),
                }),
                cloud_data: None,
            });
        }
        
        // Merge cloud results if available
        if let Some(cloud_results) = cloud_results {
            for (file_id, score) in cloud_results {
                let weighted_score = score * self.config.cloud_weight;
                
                if let Some(existing) = result_map.get_mut(&file_id) {
                    // Merge with existing local result
                    existing.score = (existing.score + weighted_score) / 2.0;
                    existing.source = ResultSource::CloudEnhanced;
                    existing.cloud_data = Some(CloudResultData {
                        original_score: score,
                        suggested_terms: Vec::new(),
                        confidence: 0.5,
                    });
                } else {
                    // Add new cloud-only result
                    result_map.insert(file_id, MergedResult {
                        file_id: Some(file_id),
                        score: weighted_score,
                        source: ResultSource::CloudEnhanced,
                        local_data: None,
                        cloud_data: Some(CloudResultData {
                            original_score: score,
                            suggested_terms: Vec::new(),
                            confidence: 0.5,
                        }),
                    });
                }
            }
        }
        
        // Convert to vector, filter, and sort
        let mut results: Vec<MergedResult> = result_map
            .into_values()
            .filter(|r| r.score >= self.config.min_merge_score)
            .collect();
        
        // Sort by score descending
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        
        // Limit results
        results.truncate(self.config.max_results);
        
        results
    }
    
    /// Get the current configuration
    pub fn config(&self) -> &MergerConfig {
        &self.config
    }
    
    /// Update the configuration
    pub fn set_config(&mut self, config: MergerConfig) {
        self.config = config;
    }
}

impl Default for ResultMerger {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merger_config_default() {
        let config = MergerConfig::default();
        assert_eq!(config.local_weight, 0.6);
        assert_eq!(config.cloud_weight, 0.4);
        assert!(config.prefer_cloud_intent);
    }

    #[test]
    fn test_merger_config_local_preferred() {
        let config = MergerConfig::local_preferred();
        assert!(config.local_weight > config.cloud_weight);
        assert!(!config.prefer_cloud_intent);
    }

    #[test]
    fn test_merger_config_cloud_preferred() {
        let config = MergerConfig::cloud_preferred();
        assert!(config.cloud_weight > config.local_weight);
        assert!(config.prefer_cloud_intent);
    }

    #[test]
    fn test_merge_scores_local_only() {
        let merger = ResultMerger::new();
        
        let score = merger.merge_scores(0.8, None);
        assert!((score - 0.48).abs() < 0.01); // 0.8 * 0.6 = 0.48
    }

    #[test]
    fn test_merge_scores_with_cloud() {
        let merger = ResultMerger::new();
        
        let score = merger.merge_scores(0.8, Some(0.9));
        // (0.8 * 0.6) + (0.9 * 0.4) = 0.48 + 0.36 = 0.84
        assert!((score - 0.84).abs() < 0.01);
    }

    #[test]
    fn test_merge_file_results_local_only() {
        let merger = ResultMerger::new();
        
        let local_results = vec![
            (Uuid::now_v7(), 0.9),
            (Uuid::now_v7(), 0.7),
            (Uuid::now_v7(), 0.5),
        ];
        
        let merged = merger.merge_file_results(local_results, None);
        
        assert_eq!(merged.len(), 3);
        // Results should be sorted by score descending
        assert!(merged[0].score >= merged[1].score);
        assert!(merged[1].score >= merged[2].score);
    }

    #[test]
    fn test_merge_file_results_with_cloud() {
        let merger = ResultMerger::new();
        
        let file_id = Uuid::now_v7();
        let local_results = vec![(file_id, 0.8)];
        let cloud_results = Some(vec![(file_id, 0.9)]);
        
        let merged = merger.merge_file_results(local_results, cloud_results);
        
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, ResultSource::CloudEnhanced);
        assert!(merged[0].local_data.is_some());
        assert!(merged[0].cloud_data.is_some());
    }

    #[test]
    fn test_merge_file_results_filter_low_scores() {
        let config = MergerConfig {
            min_merge_score: 0.5,
            ..Default::default()
        };
        let merger = ResultMerger::with_config(config);
        
        let local_results = vec![
            (Uuid::now_v7(), 0.9),
            (Uuid::now_v7(), 0.1), // This should be filtered out
        ];
        
        let merged = merger.merge_file_results(local_results, None);
        
        // Only one result should pass the threshold
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn test_merge_file_results_max_results() {
        let config = MergerConfig {
            max_results: 2,
            ..Default::default()
        };
        let merger = ResultMerger::with_config(config);
        
        let local_results = vec![
            (Uuid::now_v7(), 0.9),
            (Uuid::now_v7(), 0.8),
            (Uuid::now_v7(), 0.7),
            (Uuid::now_v7(), 0.6),
        ];
        
        let merged = merger.merge_file_results(local_results, None);
        
        // Should be limited to 2 results
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_merge_file_results_cloud_only() {
        let merger = ResultMerger::new();
        
        let file_id = Uuid::now_v7();
        let cloud_results = Some(vec![(file_id, 0.9)]);
        
        let merged = merger.merge_file_results(vec![], cloud_results);
        
        assert_eq!(merged.len(), 1);
        assert!(merged[0].local_data.is_none());
        assert!(merged[0].cloud_data.is_some());
    }
}
