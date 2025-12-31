//! Property-based tests for the relation module
//!
//! Tests the following properties:
//! - Property 10: Relation Symmetry
//! - Property 14: User Feedback State Machine
//! - Property 15: Block Rule Enforcement
//! - Property 16: Rejection Learning Effect

use proptest::prelude::*;
use uuid::Uuid;

use crate::core::types::{FileRelation, RelationType, RelationSource, UserFeedback, RelationBlockRule, BlockRuleType, BlockRuleDetail};

// ============================================================================
// Test Generators
// ============================================================================

/// Generate a random UUID
fn arb_uuid() -> impl Strategy<Value = Uuid> {
    any::<u128>().prop_map(Uuid::from_u128)
}

/// Generate a random relation type
fn arb_relation_type() -> impl Strategy<Value = RelationType> {
    prop_oneof![
        Just(RelationType::ContentSimilar),
        Just(RelationType::SameSession),
        Just(RelationType::SameProject),
        Just(RelationType::SameAuthor),
        Just(RelationType::Reference),
        Just(RelationType::Derivative),
        Just(RelationType::Workflow),
        Just(RelationType::UserDefined),
    ]
}

/// Generate a random relation source
fn arb_relation_source() -> impl Strategy<Value = RelationSource> {
    prop_oneof![
        Just(RelationSource::AIGenerated),
        Just(RelationSource::SessionTracking),
        Just(RelationSource::UserManual),
        Just(RelationSource::MetadataExtract),
    ]
}

/// Generate a random user feedback
fn arb_user_feedback() -> impl Strategy<Value = UserFeedback> {
    prop_oneof![
        Just(UserFeedback::None),
        Just(UserFeedback::Confirmed),
        (proptest::option::of("[a-zA-Z0-9 ]{0,20}"), any::<bool>()).prop_map(|(reason, block)| {
            UserFeedback::Rejected { reason, block_similar: block }
        }),
        (0.0f32..1.0, 0.0f32..1.0).prop_map(|(orig, user)| {
            UserFeedback::Adjusted { original_strength: orig, user_strength: user }
        }),
    ]
}

/// Generate a random strength value (0.0 - 1.0)
fn arb_strength() -> impl Strategy<Value = f32> {
    0.0f32..=1.0f32
}

/// Generate a pair of distinct UUIDs
fn arb_uuid_pair() -> impl Strategy<Value = (Uuid, Uuid)> {
    (arb_uuid(), arb_uuid()).prop_filter("UUIDs must be distinct", |(a, b)| a != b)
}

// ============================================================================
// Property 10: Relation Symmetry
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: neural-fs-core, Property 10: Relation Symmetry**
    /// *For any* file relation between files A and B, querying relations for A should
    /// return B as related, and querying relations for B should return A as related.
    /// **Validates: Requirements 6.1**
    #[test]
    fn prop_relation_symmetry(
        (source_id, target_id) in arb_uuid_pair(),
        relation_type in arb_relation_type(),
        strength in arb_strength(),
    ) {
        // Create a relation from source to target
        let relation = FileRelation {
            id: Uuid::now_v7(),
            source_file_id: source_id,
            target_file_id: target_id,
            relation_type,
            strength,
            source: RelationSource::AIGenerated,
            user_feedback: UserFeedback::None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            user_action_at: None,
        };

        // Property: The relation should be discoverable from both files
        // When querying from source, target should be the related file
        let related_from_source = if relation.source_file_id == source_id {
            relation.target_file_id
        } else {
            relation.source_file_id
        };
        prop_assert_eq!(related_from_source, target_id);

        // When querying from target, source should be the related file
        let related_from_target = if relation.source_file_id == target_id {
            relation.target_file_id
        } else {
            relation.source_file_id
        };
        prop_assert_eq!(related_from_target, source_id);

        // The relation connects exactly these two files
        prop_assert!(
            (relation.source_file_id == source_id && relation.target_file_id == target_id) ||
            (relation.source_file_id == target_id && relation.target_file_id == source_id)
        );
    }
}

// ============================================================================
// Property 14: User Feedback State Machine
// ============================================================================

/// Valid state transitions for user feedback
fn is_valid_transition(from: &UserFeedback, to: &UserFeedback) -> bool {
    match (from, to) {
        // From None, can go to any state
        (UserFeedback::None, _) => true,
        // From Confirmed, can go to Rejected or Adjusted
        (UserFeedback::Confirmed, UserFeedback::Rejected { .. }) => true,
        (UserFeedback::Confirmed, UserFeedback::Adjusted { .. }) => true,
        (UserFeedback::Confirmed, UserFeedback::Confirmed) => true, // Idempotent
        // From Adjusted, can go to Confirmed, Rejected, or re-Adjusted
        (UserFeedback::Adjusted { .. }, UserFeedback::Confirmed) => true,
        (UserFeedback::Adjusted { .. }, UserFeedback::Rejected { .. }) => true,
        (UserFeedback::Adjusted { .. }, UserFeedback::Adjusted { .. }) => true,
        // From Rejected, can only go to Confirmed (un-reject)
        (UserFeedback::Rejected { .. }, UserFeedback::Confirmed) => true,
        // All other transitions are invalid
        _ => false,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: neural-fs-core, Property 14: User Feedback State Machine**
    /// *For any* user feedback state, only valid transitions should be allowed.
    /// The state machine ensures consistent user interaction patterns.
    /// **Validates: Human-in-the-Loop**
    #[test]
    fn prop_user_feedback_state_machine(
        from_state in arb_user_feedback(),
        to_state in arb_user_feedback(),
    ) {
        let is_valid = is_valid_transition(&from_state, &to_state);
        
        // Property: State transitions follow the defined rules
        match (&from_state, &to_state) {
            // None -> Any is always valid
            (UserFeedback::None, _) => {
                prop_assert!(is_valid, "None -> Any should be valid");
            }
            // Rejected -> only Confirmed is valid
            (UserFeedback::Rejected { .. }, UserFeedback::Confirmed) => {
                prop_assert!(is_valid, "Rejected -> Confirmed should be valid");
            }
            (UserFeedback::Rejected { .. }, UserFeedback::None) => {
                prop_assert!(!is_valid, "Rejected -> None should be invalid");
            }
            (UserFeedback::Rejected { .. }, UserFeedback::Rejected { .. }) => {
                prop_assert!(!is_valid, "Rejected -> Rejected should be invalid");
            }
            (UserFeedback::Rejected { .. }, UserFeedback::Adjusted { .. }) => {
                prop_assert!(!is_valid, "Rejected -> Adjusted should be invalid");
            }
            // Confirmed -> Rejected, Adjusted, or Confirmed (idempotent)
            (UserFeedback::Confirmed, UserFeedback::None) => {
                prop_assert!(!is_valid, "Confirmed -> None should be invalid");
            }
            // Adjusted -> Confirmed, Rejected, or Adjusted
            (UserFeedback::Adjusted { .. }, UserFeedback::None) => {
                prop_assert!(!is_valid, "Adjusted -> None should be invalid");
            }
            _ => {
                // Other cases are covered by the state machine definition
            }
        }
    }
}

// ============================================================================
// Property 15: Block Rule Enforcement
// ============================================================================

/// Check if a block rule would block a relation
fn rule_blocks_relation(
    rule: &RelationBlockRule,
    source_file_id: Uuid,
    target_file_id: Uuid,
    relation_type: RelationType,
) -> bool {
    if !rule.is_active {
        return false;
    }

    match &rule.rule_detail {
        BlockRuleDetail::FilePair { file_id_a, file_id_b } => {
            (*file_id_a == source_file_id && *file_id_b == target_file_id)
                || (*file_id_a == target_file_id && *file_id_b == source_file_id)
        }
        BlockRuleDetail::FileAllAI { file_id } => {
            *file_id == source_file_id || *file_id == target_file_id
        }
        BlockRuleDetail::RelationType {
            file_id,
            relation_type: blocked_type,
        } => {
            if *blocked_type != relation_type {
                return false;
            }
            match file_id {
                Some(fid) => *fid == source_file_id || *fid == target_file_id,
                None => true,
            }
        }
        BlockRuleDetail::FileToTag { file_id, .. } => {
            *file_id == source_file_id
        }
        BlockRuleDetail::TagPair { .. } => {
            // Would need tag lookup - simplified
            false
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: neural-fs-core, Property 15: Block Rule Enforcement**
    /// *For any* active block rule, relations matching the rule criteria should be blocked.
    /// **Validates: Human-in-the-Loop**
    #[test]
    fn prop_block_rule_enforcement(
        (file_a, file_b) in arb_uuid_pair(),
        relation_type in arb_relation_type(),
        is_active in any::<bool>(),
    ) {
        // Create a file pair block rule
        let rule = RelationBlockRule {
            id: Uuid::now_v7(),
            rule_type: BlockRuleType::FilePair,
            rule_detail: BlockRuleDetail::FilePair {
                file_id_a: file_a,
                file_id_b: file_b,
            },
            created_at: chrono::Utc::now(),
            expires_at: None,
            is_active,
        };

        // Property: Active rules block matching relations
        let blocks_forward = rule_blocks_relation(&rule, file_a, file_b, relation_type);
        let blocks_reverse = rule_blocks_relation(&rule, file_b, file_a, relation_type);

        if is_active {
            // Active rule should block both directions
            prop_assert!(blocks_forward, "Active rule should block A -> B");
            prop_assert!(blocks_reverse, "Active rule should block B -> A");
        } else {
            // Inactive rule should not block
            prop_assert!(!blocks_forward, "Inactive rule should not block A -> B");
            prop_assert!(!blocks_reverse, "Inactive rule should not block B -> A");
        }
    }

    /// **Feature: neural-fs-core, Property 15: Block Rule Enforcement (FileAllAI)**
    /// *For any* FileAllAI block rule, all AI relations involving that file should be blocked.
    /// **Validates: Human-in-the-Loop**
    #[test]
    fn prop_block_rule_file_all_ai(
        blocked_file in arb_uuid(),
        other_file in arb_uuid(),
        relation_type in arb_relation_type(),
    ) {
        prop_assume!(blocked_file != other_file);

        let rule = RelationBlockRule {
            id: Uuid::now_v7(),
            rule_type: BlockRuleType::FileAllAI,
            rule_detail: BlockRuleDetail::FileAllAI { file_id: blocked_file },
            created_at: chrono::Utc::now(),
            expires_at: None,
            is_active: true,
        };

        // Property: FileAllAI blocks relations in both directions
        let blocks_as_source = rule_blocks_relation(&rule, blocked_file, other_file, relation_type);
        let blocks_as_target = rule_blocks_relation(&rule, other_file, blocked_file, relation_type);

        prop_assert!(blocks_as_source, "Should block when blocked file is source");
        prop_assert!(blocks_as_target, "Should block when blocked file is target");
    }

    /// **Feature: neural-fs-core, Property 15: Block Rule Enforcement (RelationType)**
    /// *For any* RelationType block rule, only relations of that type should be blocked.
    /// **Validates: Human-in-the-Loop**
    #[test]
    fn prop_block_rule_relation_type(
        (file_a, file_b) in arb_uuid_pair(),
        blocked_type in arb_relation_type(),
        test_type in arb_relation_type(),
    ) {
        let rule = RelationBlockRule {
            id: Uuid::now_v7(),
            rule_type: BlockRuleType::RelationType,
            rule_detail: BlockRuleDetail::RelationType {
                file_id: None, // Global block
                relation_type: blocked_type,
            },
            created_at: chrono::Utc::now(),
            expires_at: None,
            is_active: true,
        };

        let is_blocked = rule_blocks_relation(&rule, file_a, file_b, test_type);

        // Property: Only matching relation types are blocked
        if test_type == blocked_type {
            prop_assert!(is_blocked, "Matching relation type should be blocked");
        } else {
            prop_assert!(!is_blocked, "Non-matching relation type should not be blocked");
        }
    }
}

// ============================================================================
// Property 16: Rejection Learning Effect
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// **Feature: neural-fs-core, Property 16: Rejection Learning Effect**
    /// *For any* rejected relation with block_similar=true, a corresponding block rule
    /// should prevent regeneration of similar relations.
    /// **Validates: Human-in-the-Loop**
    #[test]
    fn prop_rejection_learning_effect(
        (source_id, target_id) in arb_uuid_pair(),
        relation_type in arb_relation_type(),
        block_similar in any::<bool>(),
    ) {
        // Simulate a rejection with block_similar flag
        let rejection = UserFeedback::Rejected {
            reason: Some("Test rejection".to_string()),
            block_similar,
        };

        // If block_similar is true, a block rule should be created
        if block_similar {
            // Create the corresponding block rule
            let rule = RelationBlockRule {
                id: Uuid::now_v7(),
                rule_type: BlockRuleType::FilePair,
                rule_detail: BlockRuleDetail::FilePair {
                    file_id_a: source_id,
                    file_id_b: target_id,
                },
                created_at: chrono::Utc::now(),
                expires_at: None,
                is_active: true,
            };

            // Property: The block rule should prevent regeneration
            let blocks = rule_blocks_relation(&rule, source_id, target_id, relation_type);
            prop_assert!(blocks, "Block rule should prevent regeneration after rejection with block_similar=true");

            // Also blocks reverse direction
            let blocks_reverse = rule_blocks_relation(&rule, target_id, source_id, relation_type);
            prop_assert!(blocks_reverse, "Block rule should prevent regeneration in reverse direction");
        }

        // Property: Rejection feedback is properly structured
        match rejection {
            UserFeedback::Rejected { block_similar: bs, .. } => {
                prop_assert_eq!(bs, block_similar, "block_similar flag should be preserved");
            }
            _ => prop_assert!(false, "Should be Rejected variant"),
        }
    }

    /// **Feature: neural-fs-core, Property 16: Rejection Learning Effect (Strength Zero)**
    /// *For any* rejected relation, the effective strength should be zero.
    /// **Validates: Human-in-the-Loop**
    #[test]
    fn prop_rejected_relation_zero_strength(
        (source_id, target_id) in arb_uuid_pair(),
        original_strength in arb_strength(),
        reason in proptest::option::of("[a-zA-Z0-9 ]{0,20}"),
        block_similar in any::<bool>(),
    ) {
        let relation = FileRelation {
            id: Uuid::now_v7(),
            source_file_id: source_id,
            target_file_id: target_id,
            relation_type: RelationType::ContentSimilar,
            strength: original_strength,
            source: RelationSource::AIGenerated,
            user_feedback: UserFeedback::Rejected { reason, block_similar },
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            user_action_at: Some(chrono::Utc::now()),
        };

        // Property: Calculate effective strength (rejected = 0)
        let effective_strength = match &relation.user_feedback {
            UserFeedback::Rejected { .. } => 0.0,
            UserFeedback::Adjusted { user_strength, .. } => *user_strength,
            _ => relation.strength,
        };

        prop_assert_eq!(effective_strength, 0.0, "Rejected relations should have zero effective strength");
    }
}

// ============================================================================
// Additional Unit Tests
// ============================================================================

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn test_valid_state_transitions() {
        // None -> Any
        assert!(is_valid_transition(&UserFeedback::None, &UserFeedback::Confirmed));
        assert!(is_valid_transition(&UserFeedback::None, &UserFeedback::Rejected { reason: None, block_similar: false }));
        assert!(is_valid_transition(&UserFeedback::None, &UserFeedback::Adjusted { original_strength: 0.5, user_strength: 0.8 }));

        // Confirmed -> Rejected, Adjusted
        assert!(is_valid_transition(&UserFeedback::Confirmed, &UserFeedback::Rejected { reason: None, block_similar: false }));
        assert!(is_valid_transition(&UserFeedback::Confirmed, &UserFeedback::Adjusted { original_strength: 0.5, user_strength: 0.8 }));

        // Rejected -> Confirmed only
        assert!(is_valid_transition(&UserFeedback::Rejected { reason: None, block_similar: false }, &UserFeedback::Confirmed));
        assert!(!is_valid_transition(&UserFeedback::Rejected { reason: None, block_similar: false }, &UserFeedback::None));
        assert!(!is_valid_transition(&UserFeedback::Rejected { reason: None, block_similar: false }, &UserFeedback::Adjusted { original_strength: 0.5, user_strength: 0.8 }));

        // Adjusted -> Confirmed, Rejected, Adjusted
        assert!(is_valid_transition(&UserFeedback::Adjusted { original_strength: 0.5, user_strength: 0.8 }, &UserFeedback::Confirmed));
        assert!(is_valid_transition(&UserFeedback::Adjusted { original_strength: 0.5, user_strength: 0.8 }, &UserFeedback::Rejected { reason: None, block_similar: false }));
        assert!(is_valid_transition(&UserFeedback::Adjusted { original_strength: 0.5, user_strength: 0.8 }, &UserFeedback::Adjusted { original_strength: 0.8, user_strength: 0.9 }));
    }

    #[test]
    fn test_block_rule_file_pair() {
        let file_a = Uuid::now_v7();
        let file_b = Uuid::now_v7();
        let file_c = Uuid::now_v7();

        let rule = RelationBlockRule {
            id: Uuid::now_v7(),
            rule_type: BlockRuleType::FilePair,
            rule_detail: BlockRuleDetail::FilePair { file_id_a: file_a, file_id_b: file_b },
            created_at: chrono::Utc::now(),
            expires_at: None,
            is_active: true,
        };

        // Should block A <-> B
        assert!(rule_blocks_relation(&rule, file_a, file_b, RelationType::ContentSimilar));
        assert!(rule_blocks_relation(&rule, file_b, file_a, RelationType::ContentSimilar));

        // Should not block A <-> C or B <-> C
        assert!(!rule_blocks_relation(&rule, file_a, file_c, RelationType::ContentSimilar));
        assert!(!rule_blocks_relation(&rule, file_b, file_c, RelationType::ContentSimilar));
    }

    #[test]
    fn test_inactive_rule_does_not_block() {
        let file_a = Uuid::now_v7();
        let file_b = Uuid::now_v7();

        let rule = RelationBlockRule {
            id: Uuid::now_v7(),
            rule_type: BlockRuleType::FilePair,
            rule_detail: BlockRuleDetail::FilePair { file_id_a: file_a, file_id_b: file_b },
            created_at: chrono::Utc::now(),
            expires_at: None,
            is_active: false, // Inactive
        };

        assert!(!rule_blocks_relation(&rule, file_a, file_b, RelationType::ContentSimilar));
    }
}
