//! File relation types
//! 
//! Defines structures for the logic chain association system.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(test)]
use proptest::prelude::*;
#[cfg(test)]
use proptest_derive::Arbitrary;

/// File relation - core data structure supporting human-in-the-loop correction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct FileRelation {
    /// Relation ID
    #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
    pub id: Uuid,
    
    /// Source file ID
    #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
    pub source_file_id: Uuid,
    
    /// Target file ID
    #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
    pub target_file_id: Uuid,
    
    /// Relation type
    pub relation_type: RelationType,
    
    /// Relation strength (0.0 - 1.0)
    #[cfg_attr(test, proptest(strategy = "0.0f32..1.0"))]
    pub strength: f32,
    
    /// Relation source
    pub source: RelationSource,
    
    /// User feedback status - key field for human-in-the-loop
    pub user_feedback: UserFeedback,
    
    /// Creation time
    #[cfg_attr(test, proptest(strategy = "any::<i64>().prop_map(|ts| DateTime::from_timestamp(ts.abs() % 4102444800, 0).unwrap_or_else(|| Utc::now()))"))]
    pub created_at: DateTime<Utc>,
    
    /// Last update time
    #[cfg_attr(test, proptest(strategy = "any::<i64>().prop_map(|ts| DateTime::from_timestamp(ts.abs() % 4102444800, 0).unwrap_or_else(|| Utc::now()))"))]
    pub updated_at: DateTime<Utc>,
    
    /// User action time
    #[cfg_attr(test, proptest(strategy = "proptest::option::of(any::<i64>().prop_map(|ts| DateTime::from_timestamp(ts.abs() % 4102444800, 0).unwrap_or_else(|| Utc::now())))"))]
    pub user_action_at: Option<DateTime<Utc>>,
}

/// Type of file relation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum RelationType {
    /// Content similarity
    ContentSimilar,
    /// Opened in same session
    SameSession,
    /// Same project
    SameProject,
    /// Same author
    SameAuthor,
    /// Reference relationship
    Reference,
    /// Derivative relationship (e.g., video and its source materials)
    Derivative,
    /// Workflow association
    Workflow,
    /// User-defined relationship
    UserDefined,
}

/// Source of relation
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum RelationSource {
    /// AI auto-generated
    AIGenerated,
    /// Session tracking
    SessionTracking,
    /// User manually created
    UserManual,
    /// Metadata extraction
    MetadataExtract,
}

/// User feedback status - core of human-in-the-loop correction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum UserFeedback {
    /// No action - default state
    None,
    
    /// User confirmed - relation is valid
    Confirmed,
    
    /// User rejected - one-click unlink
    Rejected {
        /// Rejection reason (optional)
        #[cfg_attr(test, proptest(strategy = "proptest::option::of(\"[a-zA-Z0-9 ]{0,50}\")"))]
        reason: Option<String>,
        /// Whether to block similar relations (prevent regeneration)
        block_similar: bool,
    },
    
    /// User adjusted - modified relation strength
    Adjusted {
        /// Original strength
        #[cfg_attr(test, proptest(strategy = "0.0f32..1.0"))]
        original_strength: f32,
        /// User-set strength
        #[cfg_attr(test, proptest(strategy = "0.0f32..1.0"))]
        user_strength: f32,
    },
}

/// Relation block rule - prevents AI from regenerating rejected relations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct RelationBlockRule {
    /// Rule ID
    #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
    pub id: Uuid,
    
    /// Rule type
    pub rule_type: BlockRuleType,
    
    /// Rule detail
    pub rule_detail: BlockRuleDetail,
    
    /// Creation time
    #[cfg_attr(test, proptest(strategy = "any::<i64>().prop_map(|ts| DateTime::from_timestamp(ts.abs() % 4102444800, 0).unwrap_or_else(|| Utc::now()))"))]
    pub created_at: DateTime<Utc>,
    
    /// Expiration time (optional, None means permanent)
    #[cfg_attr(test, proptest(strategy = "proptest::option::of(any::<i64>().prop_map(|ts| DateTime::from_timestamp(ts.abs() % 4102444800, 0).unwrap_or_else(|| Utc::now())))"))]
    pub expires_at: Option<DateTime<Utc>>,
    
    /// Whether active
    pub is_active: bool,
}

/// Type of block rule
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum BlockRuleType {
    /// Block relations between two specific files
    FilePair,
    /// Block relations between a file and all files under a tag
    FileToTag,
    /// Block relations between all files under two tags
    TagPair,
    /// Block all AI relations for a specific file
    FileAllAI,
    /// Block a specific relation type
    RelationType,
}

/// Detail of block rule
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum BlockRuleDetail {
    FilePair {
        #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
        file_id_a: Uuid,
        #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
        file_id_b: Uuid,
    },
    FileToTag {
        #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
        file_id: Uuid,
        #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
        tag_id: Uuid,
    },
    TagPair {
        #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
        tag_id_a: Uuid,
        #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
        tag_id_b: Uuid,
    },
    FileAllAI {
        #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
        file_id: Uuid,
    },
    RelationType {
        #[cfg_attr(test, proptest(strategy = "proptest::option::of(any::<u128>().prop_map(|n| Uuid::from_u128(n)))"))]
        file_id: Option<Uuid>, // None means global
        relation_type: RelationType,
    },
}

impl Default for UserFeedback {
    fn default() -> Self {
        UserFeedback::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        
        /// **Feature: neural-fs-core, Property 17: FileRelation Serialization Round-Trip**
        /// *For any* valid FileRelation, serializing then deserializing should produce an equivalent object
        /// **Validates: Requirements 21**
        #[test]
        fn prop_file_relation_json_roundtrip(relation in any::<FileRelation>()) {
            // Serialize to JSON
            let json = serde_json::to_string(&relation).expect("Failed to serialize FileRelation to JSON");
            
            // Deserialize back
            let deserialized: FileRelation = serde_json::from_str(&json).expect("Failed to deserialize FileRelation from JSON");
            
            // Verify equality
            prop_assert_eq!(relation, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 17: FileRelation Serialization Round-Trip (bincode)**
        /// *For any* valid FileRelation, serializing then deserializing with bincode should produce an equivalent object
        /// **Validates: Requirements 21**
        #[test]
        fn prop_file_relation_bincode_roundtrip(relation in any::<FileRelation>()) {
            // Serialize to bincode
            let bytes = bincode::serialize(&relation).expect("Failed to serialize FileRelation to bincode");
            
            // Deserialize back
            let deserialized: FileRelation = bincode::deserialize(&bytes).expect("Failed to deserialize FileRelation from bincode");
            
            // Verify equality
            prop_assert_eq!(relation, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 17: RelationBlockRule Serialization Round-Trip**
        /// *For any* valid RelationBlockRule, serializing then deserializing should produce an equivalent object
        /// **Validates: Requirements 21**
        #[test]
        fn prop_relation_block_rule_roundtrip(rule in any::<RelationBlockRule>()) {
            let json = serde_json::to_string(&rule).expect("Failed to serialize RelationBlockRule");
            let deserialized: RelationBlockRule = serde_json::from_str(&json).expect("Failed to deserialize RelationBlockRule");
            prop_assert_eq!(rule, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 17: UserFeedback Serialization Round-Trip**
        /// *For any* valid UserFeedback, serializing then deserializing should produce an equivalent value
        /// **Validates: Requirements 21**
        #[test]
        fn prop_user_feedback_roundtrip(feedback in any::<UserFeedback>()) {
            let json = serde_json::to_string(&feedback).expect("Failed to serialize UserFeedback");
            let deserialized: UserFeedback = serde_json::from_str(&json).expect("Failed to deserialize UserFeedback");
            prop_assert_eq!(feedback, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 17: RelationType Serialization Round-Trip**
        /// *For any* valid RelationType, serializing then deserializing should produce an equivalent value
        /// **Validates: Requirements 21**
        #[test]
        fn prop_relation_type_roundtrip(rel_type in any::<RelationType>()) {
            let json = serde_json::to_string(&rel_type).expect("Failed to serialize RelationType");
            let deserialized: RelationType = serde_json::from_str(&json).expect("Failed to deserialize RelationType");
            prop_assert_eq!(rel_type, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 17: BlockRuleDetail Serialization Round-Trip**
        /// *For any* valid BlockRuleDetail, serializing then deserializing should produce an equivalent value
        /// **Validates: Requirements 21**
        #[test]
        fn prop_block_rule_detail_roundtrip(detail in any::<BlockRuleDetail>()) {
            let json = serde_json::to_string(&detail).expect("Failed to serialize BlockRuleDetail");
            let deserialized: BlockRuleDetail = serde_json::from_str(&json).expect("Failed to deserialize BlockRuleDetail");
            prop_assert_eq!(detail, deserialized);
        }
    }
}
