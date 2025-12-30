//! Content chunk types
//! 
//! Defines structures for document segments after content splitting.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[cfg(test)]
use proptest::prelude::*;
#[cfg(test)]
use proptest_derive::Arbitrary;

/// Content chunk - semantic unit after document splitting
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct ContentChunk {
    /// Chunk unique identifier
    #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
    pub id: Uuid,
    
    /// Parent file ID
    #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
    pub file_id: Uuid,
    
    /// Chunk index within file (0-based)
    pub chunk_index: u32,
    
    /// Chunk type
    pub chunk_type: ChunkType,
    
    /// Chunk text content (for preview)
    #[cfg_attr(test, proptest(strategy = "\"[a-zA-Z0-9 ]{0,200}\""))]
    pub content: String,
    
    /// Chunk location in original file
    pub location: ChunkLocation,
    
    /// Vector ID (point_id in Qdrant)
    pub vector_id: u64,
    
    /// Creation time
    #[cfg_attr(test, proptest(strategy = "any::<i64>().prop_map(|ts| DateTime::from_timestamp(ts.abs() % 4102444800, 0).unwrap_or_else(|| Utc::now()))"))]
    pub created_at: DateTime<Utc>,
}

/// Type of content chunk
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum ChunkType {
    /// Paragraph text
    Paragraph,
    /// Heading/title
    Heading,
    /// Code block
    CodeBlock,
    /// Table content
    Table,
    /// Image region
    Image,
    /// Image/table caption
    Caption,
}

/// Location of chunk within original file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct ChunkLocation {
    /// Start byte offset
    pub start_offset: u64,
    /// End byte offset
    pub end_offset: u64,
    /// Start line number (text files)
    pub start_line: Option<u32>,
    /// End line number (text files)
    pub end_line: Option<u32>,
    /// Page number (PDF)
    pub page_number: Option<u32>,
    /// Image region coordinates (x, y, width, height) - normalized to 0-1
    #[cfg_attr(test, proptest(strategy = "proptest::option::of((0.0f32..1.0, 0.0f32..1.0, 0.0f32..1.0, 0.0f32..1.0))"))]
    pub bounding_box: Option<(f32, f32, f32, f32)>,
}

impl Default for ChunkLocation {
    fn default() -> Self {
        Self {
            start_offset: 0,
            end_offset: 0,
            start_line: None,
            end_line: None,
            page_number: None,
            bounding_box: None,
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        
        /// **Feature: neural-fs-core, Property 17: ContentChunk Serialization Round-Trip**
        /// *For any* valid ContentChunk, serializing then deserializing should produce an equivalent object
        /// **Validates: Requirements 21**
        #[test]
        fn prop_content_chunk_json_roundtrip(chunk in any::<ContentChunk>()) {
            // Serialize to JSON
            let json = serde_json::to_string(&chunk).expect("Failed to serialize ContentChunk to JSON");
            
            // Deserialize back
            let deserialized: ContentChunk = serde_json::from_str(&json).expect("Failed to deserialize ContentChunk from JSON");
            
            // Verify equality
            prop_assert_eq!(chunk, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 17: ContentChunk Serialization Round-Trip (bincode)**
        /// *For any* valid ContentChunk, serializing then deserializing with bincode should produce an equivalent object
        /// **Validates: Requirements 21**
        #[test]
        fn prop_content_chunk_bincode_roundtrip(chunk in any::<ContentChunk>()) {
            // Serialize to bincode
            let bytes = bincode::serialize(&chunk).expect("Failed to serialize ContentChunk to bincode");
            
            // Deserialize back
            let deserialized: ContentChunk = bincode::deserialize(&bytes).expect("Failed to deserialize ContentChunk from bincode");
            
            // Verify equality
            prop_assert_eq!(chunk, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 17: ChunkLocation Serialization Round-Trip**
        /// *For any* valid ChunkLocation, serializing then deserializing should produce an equivalent object
        /// **Validates: Requirements 21**
        #[test]
        fn prop_chunk_location_roundtrip(location in any::<ChunkLocation>()) {
            let json = serde_json::to_string(&location).expect("Failed to serialize ChunkLocation");
            let deserialized: ChunkLocation = serde_json::from_str(&json).expect("Failed to deserialize ChunkLocation");
            prop_assert_eq!(location, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 17: ChunkType Serialization Round-Trip**
        /// *For any* valid ChunkType, serializing then deserializing should produce an equivalent value
        /// **Validates: Requirements 21**
        #[test]
        fn prop_chunk_type_roundtrip(chunk_type in any::<ChunkType>()) {
            let json = serde_json::to_string(&chunk_type).expect("Failed to serialize ChunkType");
            let deserialized: ChunkType = serde_json::from_str(&json).expect("Failed to deserialize ChunkType");
            prop_assert_eq!(chunk_type, deserialized);
        }
    }
}
