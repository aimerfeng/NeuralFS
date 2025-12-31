/**
 * Relation types matching Rust backend structures
 */

export interface FileRelation {
  id: string;
  source_file_id: string;
  target_file_id: string;
  relation_type: RelationType;
  strength: number;
  source: RelationSource;
  user_feedback: UserFeedback;
  created_at: string;
  updated_at: string;
  user_action_at?: string;
}

export type RelationType =
  | 'ContentSimilar'
  | 'SameSession'
  | 'SameProject'
  | 'SameAuthor'
  | 'Reference'
  | 'Derivative'
  | 'Workflow'
  | 'UserDefined';

export type RelationSource =
  | 'AIGenerated'
  | 'SessionTracking'
  | 'UserManual'
  | 'MetadataExtract';

export type UserFeedback =
  | { type: 'None' }
  | { type: 'Confirmed' }
  | { type: 'Rejected'; reason?: string; block_similar: boolean }
  | { type: 'Adjusted'; original_strength: number; user_strength: number };

export type BlockScope =
  | { type: 'ThisPairOnly' }
  | { type: 'SourceToTargetTag'; target_tag_id: string }
  | { type: 'TagToTag'; source_tag_id: string; target_tag_id: string };

export type RelationCommand =
  | { type: 'Confirm'; relation_id: string }
  | { type: 'Reject'; relation_id: string; reason?: string; block_similar: boolean; block_scope?: BlockScope }
  | { type: 'Adjust'; relation_id: string; new_strength: number }
  | { type: 'Create'; source_file_id: string; target_file_id: string; relation_type: RelationType; strength: number }
  | { type: 'BatchReject'; file_id: string; relation_types?: RelationType[]; block_future: boolean };

export interface RelationBlockRule {
  id: string;
  rule_type: BlockRuleType;
  rule_detail: BlockRuleDetail;
  created_at: string;
  expires_at?: string;
  is_active: boolean;
}

export type BlockRuleType =
  | 'FilePair'
  | 'FileToTag'
  | 'TagPair'
  | 'FileAllAI'
  | 'RelationType';

export type BlockRuleDetail =
  | { type: 'FilePair'; file_id_a: string; file_id_b: string }
  | { type: 'FileToTag'; file_id: string; tag_id: string }
  | { type: 'TagPair'; tag_id_a: string; tag_id_b: string }
  | { type: 'FileAllAI'; file_id: string }
  | { type: 'RelationType'; file_id?: string; relation_type: RelationType };

export interface RelationNode {
  file_id: string;
  filename: string;
  file_type: string;
  thumbnail_url?: string;
}

export interface RelationEdge {
  id: string;
  source: string;
  target: string;
  relation_type: RelationType;
  strength: number;
  user_feedback: UserFeedback;
}

export interface RelationGraph {
  nodes: RelationNode[];
  edges: RelationEdge[];
  center_file_id: string;
}
