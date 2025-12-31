//! Tag Management System for NeuralFS
//!
//! This module provides intelligent tag management including:
//! - Automatic tag generation based on file content
//! - Tag hierarchy with parent-child relationships
//! - Human-in-the-loop tag correction API
//! - Sensitive tag detection and confirmation requirements
//!
//! # Requirements
//! - 5.1: Automatic tag assignment on file indexing
//! - 5.2: Tag hierarchy with expandable sub-categories
//! - 5.6: Multi-dimensional tag navigation
//! - Human-in-the-Loop: Tag confirmation/rejection API

pub mod error;
pub mod manager;
pub mod hierarchy;
pub mod correction;
pub mod sensitive;

#[cfg(test)]
mod tests;

pub use error::TagError;
pub use manager::{TagManager, TagManagerConfig, TagSuggestion, AutoTagResult};
pub use hierarchy::{TagHierarchy, TagNode, TagPath};
pub use correction::{TagCommand, TagCorrectionService, TagCorrectionResult};
pub use sensitive::{SensitiveTagDetector, SensitivePattern, SensitivityLevel};
