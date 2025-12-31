//! Logic Chain Engine for NeuralFS
//!
//! This module provides file relation management including:
//! - Content similarity-based file associations
//! - Session tracking for files opened together
//! - Human-in-the-loop relation correction API
//! - Block rules to prevent unwanted relation regeneration
//!
//! # Requirements
//! - 6.1: Display related files based on content similarity
//! - 6.2: Show files previously opened in the same session
//! - Human-in-the-Loop: Relation confirmation/rejection/blocking API

pub mod error;
pub mod engine;
pub mod session;
pub mod correction;
pub mod block_rules;

#[cfg(test)]
mod tests;

pub use error::{RelationError, Result};
pub use engine::{LogicChainEngine, LogicChainConfig, RelatedFile, SimilarityResult};
pub use session::{SessionTracker, SessionConfig, SessionInfo, SessionEvent};
pub use correction::{RelationCommand, RelationCorrectionService, RelationCorrectionResult, BlockScope};
pub use block_rules::{BlockRuleStore, BlockRuleFilter};
