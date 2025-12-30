//! Hybrid Inference Engine for NeuralFS
//!
//! This module provides parallel local and cloud inference capabilities:
//! - Local inference using embedding engine and intent parsing
//! - Cloud inference via API bridges (GPT-4o-mini, Claude Haiku)
//! - Result merging with weighted scoring
//! - Data anonymization for privacy protection
//! - Inference caching for performance
//!
//! **Validates: Requirements 11, 13**

mod local;
mod cloud;
mod merger;
mod hybrid;
mod error;
mod types;
mod anonymizer;

#[cfg(test)]
mod tests;

pub use local::{LocalInferenceEngine, LocalInferenceResult, TagMatch, TagMatcher};
pub use cloud::{CloudBridge, CloudConfig, CloudInferenceResult, CostTracker, RateLimiter};
pub use merger::{ResultMerger, MergerConfig, MergedResult};
pub use hybrid::{HybridInferenceEngine, InferenceCache};
pub use error::{InferenceError, InferenceResult};
pub use types::{
    InferenceRequest, InferenceResponse, InferenceContext, InferenceOptions,
    LocalModelType, CloudModelType, FileStructureContext, UserHistoryContext,
    SessionContext, RecentFile,
};
pub use anonymizer::{DataAnonymizer, AnonymizationConfig};
