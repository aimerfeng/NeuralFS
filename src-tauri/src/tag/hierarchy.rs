//! Tag Hierarchy - Parent-child tag relationships and multi-dimensional navigation
//!
//! Provides:
//! - Tag tree structure with parent-child relationships
//! - Maximum depth enforcement (3 levels for 2-3 click navigation)
//! - Multi-dimensional tag navigation
//!
//! # Requirements
//! - 5.2: Tag hierarchy with expandable sub-categories
//! - 5.6: Multi-dimensional tag navigation
//! - 5.7: Find any file within 2-3 clicks

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::core::types::Tag;
use super::error::{TagError, Result};

/// Maximum allowed depth for tag hierarchy (0-indexed)
/// Level 0: Root tags
/// Level 1: First-level children
/// Level 2: Second-level children (max)
pub const MAX_HIERARCHY_DEPTH: u32 = 2;

/// A node in the tag hierarchy tree
#[derive(Debug, Clone)]
pub struct TagNode {
    /// The tag at this node
    pub tag: Tag,
    /// Child nodes
    pub children: Vec<TagNode>,
    /// Depth in the hierarchy (0 = root)
    pub depth: u32,
}

impl TagNode {
    /// Create a new tag node
    pub fn new(tag: Tag, depth: u32) -> Self {
        Self {
            tag,
            children: Vec::new(),
            depth,
        }
    }

    /// Get the total number of descendants
    pub fn descendant_count(&self) -> usize {
        self.children.iter()
            .map(|c| 1 + c.descendant_count())
            .sum()
    }

    /// Check if this node has children
    pub fn has_children(&self) -> bool {
        !self.children.is_empty()
    }

    /// Find a child node by tag ID
    pub fn find_child(&self, tag_id: Uuid) -> Option<&TagNode> {
        for child in &self.children {
            if child.tag.id == tag_id {
                return Some(child);
            }
            if let Some(found) = child.find_child(tag_id) {
                return Some(found);
            }
        }
        None
    }
}

/// Path from root to a specific tag
#[derive(Debug, Clone)]
pub struct TagPath {
    /// Tags from root to target (inclusive)
    pub tags: Vec<Tag>,
}

impl TagPath {
    /// Get the depth of this path (0-indexed)
    pub fn depth(&self) -> u32 {
        self.tags.len().saturating_sub(1) as u32
    }

    /// Get the root tag
    pub fn root(&self) -> Option<&Tag> {
        self.tags.first()
    }

    /// Get the leaf tag
    pub fn leaf(&self) -> Option<&Tag> {
        self.tags.last()
    }

    /// Get breadcrumb string representation
    pub fn breadcrumb(&self) -> String {
        self.tags.iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(" > ")
    }
}

/// Tag hierarchy manager
///
/// Maintains a cached tree structure of all tags for efficient navigation.
pub struct TagHierarchy {
    pool: SqlitePool,
    /// Cached root nodes (tags without parents)
    roots: Arc<RwLock<Vec<TagNode>>>,
    /// Tag ID to depth mapping for quick lookups
    depth_cache: Arc<RwLock<HashMap<Uuid, u32>>>,
}

impl TagHierarchy {
    /// Create a new TagHierarchy and load from database
    pub async fn new(pool: SqlitePool) -> Result<Self> {
        let hierarchy = Self {
            pool,
            roots: Arc::new(RwLock::new(Vec::new())),
            depth_cache: Arc::new(RwLock::new(HashMap::new())),
        };
        hierarchy.refresh().await?;
        Ok(hierarchy)
    }

    /// Refresh the hierarchy cache from database
    pub async fn refresh(&self) -> Result<()> {
        // Load all tags
        let rows: Vec<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, name, parent_id FROM tags ORDER BY name"
        )
        .fetch_all(&self.pool)
        .await?;

        // Build parent-child map
        let mut tags_by_id: HashMap<Uuid, (String, Option<Uuid>)> = HashMap::new();
        let mut children_map: HashMap<Option<Uuid>, Vec<Uuid>> = HashMap::new();

        for (id_str, name, parent_id_str) in rows {
            let id = Uuid::parse_str(&id_str)
                .map_err(|e| TagError::Internal(e.to_string()))?;
            let parent_id = parent_id_str
                .as_deref()
                .map(|s| Uuid::parse_str(s))
                .transpose()
                .map_err(|e| TagError::Internal(e.to_string()))?;

            tags_by_id.insert(id, (name, parent_id));
            children_map.entry(parent_id).or_default().push(id);
        }

        // Build tree recursively
        fn build_node(
            id: Uuid,
            tags_by_id: &HashMap<Uuid, (String, Option<Uuid>)>,
            children_map: &HashMap<Option<Uuid>, Vec<Uuid>>,
            depth: u32,
            depth_cache: &mut HashMap<Uuid, u32>,
            pool: &SqlitePool,
        ) -> Option<TagNode> {
            let (name, _parent_id) = tags_by_id.get(&id)?;
            
            // Create a minimal tag for the node
            let tag = Tag {
                id,
                name: name.clone(),
                display_name: HashMap::new(),
                parent_id: None,
                tag_type: crate::core::types::TagType::Custom,
                color: "#808080".to_string(),
                icon: None,
                is_system: false,
                created_at: chrono::Utc::now(),
                usage_count: 0,
            };

            depth_cache.insert(id, depth);

            let mut node = TagNode::new(tag, depth);

            // Add children
            if let Some(child_ids) = children_map.get(&Some(id)) {
                for &child_id in child_ids {
                    if let Some(child_node) = build_node(
                        child_id,
                        tags_by_id,
                        children_map,
                        depth + 1,
                        depth_cache,
                        pool,
                    ) {
                        node.children.push(child_node);
                    }
                }
            }

            Some(node)
        }

        // Build root nodes
        let mut roots = Vec::new();
        let mut depth_cache = HashMap::new();

        if let Some(root_ids) = children_map.get(&None) {
            for &root_id in root_ids {
                if let Some(node) = build_node(
                    root_id,
                    &tags_by_id,
                    &children_map,
                    0,
                    &mut depth_cache,
                    &self.pool,
                ) {
                    roots.push(node);
                }
            }
        }

        // Update caches
        *self.roots.write().await = roots;
        *self.depth_cache.write().await = depth_cache;

        Ok(())
    }

    /// Get all root tags (tags without parents)
    pub async fn get_roots(&self) -> Vec<TagNode> {
        self.roots.read().await.clone()
    }

    /// Get the depth of a tag in the hierarchy
    pub async fn get_depth(&self, tag_id: Uuid) -> Result<u32> {
        let cache = self.depth_cache.read().await;
        cache.get(&tag_id).copied().ok_or(TagError::TagNotFound { id: tag_id })
    }

    /// Get the path from root to a specific tag
    pub async fn get_path(&self, tag_id: Uuid) -> Result<TagPath> {
        // Load the tag and its ancestors
        let mut path = Vec::new();
        let mut current_id = Some(tag_id);

        while let Some(id) = current_id {
            let row: Option<(String, String, Option<String>, String, String, Option<String>, bool, String, i64)> = 
                sqlx::query_as(
                    "SELECT id, name, parent_id, tag_type, color, icon, is_system, created_at, usage_count FROM tags WHERE id = ?"
                )
                .bind(id.to_string())
                .fetch_optional(&self.pool)
                .await?;

            if let Some((id_str, name, parent_id_str, tag_type_str, color, icon, is_system, created_at_str, usage_count)) = row {
                let tag_type = match tag_type_str.as_str() {
                    "Category" => crate::core::types::TagType::Category,
                    "FileType" => crate::core::types::TagType::FileType,
                    "Project" => crate::core::types::TagType::Project,
                    "Status" => crate::core::types::TagType::Status,
                    "Custom" => crate::core::types::TagType::Custom,
                    "AutoGenerated" => crate::core::types::TagType::AutoGenerated,
                    _ => crate::core::types::TagType::Custom,
                };

                let tag = Tag {
                    id: Uuid::parse_str(&id_str).map_err(|e| TagError::Internal(e.to_string()))?,
                    name,
                    display_name: HashMap::new(),
                    parent_id: parent_id_str.as_deref()
                        .map(|s| Uuid::parse_str(s))
                        .transpose()
                        .map_err(|e| TagError::Internal(e.to_string()))?,
                    tag_type,
                    color,
                    icon,
                    is_system,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .map_err(|e| TagError::Internal(e.to_string()))?,
                    usage_count: usage_count as u64,
                };

                current_id = tag.parent_id;
                path.push(tag);
            } else {
                break;
            }
        }

        // Reverse to get root-to-leaf order
        path.reverse();

        if path.is_empty() {
            return Err(TagError::TagNotFound { id: tag_id });
        }

        Ok(TagPath { tags: path })
    }

    /// Get children of a tag
    pub async fn get_children(&self, parent_id: Uuid) -> Result<Vec<Tag>> {
        let rows: Vec<(String, String, Option<String>, String, String, Option<String>, bool, String, i64)> = 
            sqlx::query_as(
                "SELECT id, name, parent_id, tag_type, color, icon, is_system, created_at, usage_count FROM tags WHERE parent_id = ? ORDER BY name"
            )
            .bind(parent_id.to_string())
            .fetch_all(&self.pool)
            .await?;

        rows.into_iter()
            .map(|(id_str, name, parent_id_str, tag_type_str, color, icon, is_system, created_at_str, usage_count)| {
                let tag_type = match tag_type_str.as_str() {
                    "Category" => crate::core::types::TagType::Category,
                    "FileType" => crate::core::types::TagType::FileType,
                    "Project" => crate::core::types::TagType::Project,
                    "Status" => crate::core::types::TagType::Status,
                    "Custom" => crate::core::types::TagType::Custom,
                    "AutoGenerated" => crate::core::types::TagType::AutoGenerated,
                    _ => crate::core::types::TagType::Custom,
                };

                Ok(Tag {
                    id: Uuid::parse_str(&id_str).map_err(|e| TagError::Internal(e.to_string()))?,
                    name,
                    display_name: HashMap::new(),
                    parent_id: parent_id_str.as_deref()
                        .map(|s| Uuid::parse_str(s))
                        .transpose()
                        .map_err(|e| TagError::Internal(e.to_string()))?,
                    tag_type,
                    color,
                    icon,
                    is_system,
                    created_at: chrono::DateTime::parse_from_rfc3339(&created_at_str)
                        .map(|dt| dt.with_timezone(&chrono::Utc))
                        .map_err(|e| TagError::Internal(e.to_string()))?,
                    usage_count: usage_count as u64,
                })
            })
            .collect()
    }

    /// Check if setting a parent would create a circular reference
    pub async fn would_create_cycle(&self, tag_id: Uuid, new_parent_id: Uuid) -> Result<bool> {
        // A cycle would occur if new_parent_id is a descendant of tag_id
        let mut current_id = Some(new_parent_id);

        while let Some(id) = current_id {
            if id == tag_id {
                return Ok(true);
            }

            let row: Option<(Option<String>,)> = sqlx::query_as(
                "SELECT parent_id FROM tags WHERE id = ?"
            )
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await?;

            current_id = row
                .and_then(|(parent_id_str,)| parent_id_str)
                .map(|s| Uuid::parse_str(&s))
                .transpose()
                .map_err(|e| TagError::Internal(e.to_string()))?;
        }

        Ok(false)
    }

    /// Set the parent of a tag
    pub async fn set_parent(&self, tag_id: Uuid, new_parent_id: Option<Uuid>) -> Result<()> {
        // Check for circular reference
        if let Some(parent_id) = new_parent_id {
            if self.would_create_cycle(tag_id, parent_id).await? {
                return Err(TagError::CircularHierarchy {
                    tag_id,
                    parent_id,
                });
            }

            // Check depth constraint
            let parent_depth = self.get_depth(parent_id).await?;
            if parent_depth >= MAX_HIERARCHY_DEPTH {
                return Err(TagError::HierarchyDepthExceeded {
                    max_depth: MAX_HIERARCHY_DEPTH + 1,
                });
            }
        }

        // Update parent
        let parent_id_str = new_parent_id.map(|id| id.to_string());
        sqlx::query("UPDATE tags SET parent_id = ? WHERE id = ?")
            .bind(&parent_id_str)
            .bind(tag_id.to_string())
            .execute(&self.pool)
            .await?;

        // Refresh cache
        self.refresh().await?;

        Ok(())
    }

    /// Get all tags at a specific depth
    pub async fn get_tags_at_depth(&self, depth: u32) -> Vec<Tag> {
        let cache = self.depth_cache.read().await;
        let roots = self.roots.read().await;

        fn collect_at_depth(nodes: &[TagNode], target_depth: u32, result: &mut Vec<Tag>) {
            for node in nodes {
                if node.depth == target_depth {
                    result.push(node.tag.clone());
                }
                collect_at_depth(&node.children, target_depth, result);
            }
        }

        let mut result = Vec::new();
        collect_at_depth(&roots, depth, &mut result);
        result
    }

    /// Multi-dimensional navigation: Get files matching multiple tags
    ///
    /// Returns file IDs that have ALL specified tags (AND logic)
    pub async fn get_files_with_tags(&self, tag_ids: &[Uuid]) -> Result<Vec<Uuid>> {
        if tag_ids.is_empty() {
            return Ok(Vec::new());
        }

        // Build query with multiple tag conditions
        let placeholders: Vec<String> = tag_ids.iter().map(|_| "?".to_string()).collect();
        let query = format!(
            r#"
            SELECT file_id FROM file_tags 
            WHERE tag_id IN ({}) AND is_rejected = 0
            GROUP BY file_id 
            HAVING COUNT(DISTINCT tag_id) = ?
            "#,
            placeholders.join(", ")
        );

        let mut query_builder = sqlx::query_as::<_, (String,)>(&query);
        for tag_id in tag_ids {
            query_builder = query_builder.bind(tag_id.to_string());
        }
        query_builder = query_builder.bind(tag_ids.len() as i64);

        let rows = query_builder.fetch_all(&self.pool).await?;

        rows.into_iter()
            .map(|(id,)| Uuid::parse_str(&id).map_err(|e| TagError::Internal(e.to_string())))
            .collect()
    }

    /// Get tag statistics
    pub async fn get_stats(&self) -> HierarchyStats {
        let roots = self.roots.read().await;
        let depth_cache = self.depth_cache.read().await;

        let total_tags = depth_cache.len();
        let root_count = roots.len();
        let max_depth = depth_cache.values().max().copied().unwrap_or(0);

        let mut depth_distribution = HashMap::new();
        for &depth in depth_cache.values() {
            *depth_distribution.entry(depth).or_insert(0) += 1;
        }

        HierarchyStats {
            total_tags,
            root_count,
            max_depth,
            depth_distribution,
        }
    }
}

/// Statistics about the tag hierarchy
#[derive(Debug, Clone)]
pub struct HierarchyStats {
    /// Total number of tags
    pub total_tags: usize,
    /// Number of root tags
    pub root_count: usize,
    /// Maximum depth in the hierarchy
    pub max_depth: u32,
    /// Number of tags at each depth level
    pub depth_distribution: HashMap<u32, usize>,
}
