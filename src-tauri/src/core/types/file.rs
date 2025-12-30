//! File record types
//! 
//! Defines the core file metadata structures stored in the database.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

#[cfg(test)]
use proptest::prelude::*;
#[cfg(test)]
use proptest_derive::Arbitrary;

/// File index record - stored in MetadataDB (SQLite)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct FileRecord {
    /// Unique identifier (UUID v7 - time-ordered)
    #[cfg_attr(test, proptest(strategy = "any::<u128>().prop_map(|n| Uuid::from_u128(n))"))]
    pub id: Uuid,
    
    /// Absolute file path
    #[cfg_attr(test, proptest(strategy = "\"[a-zA-Z0-9_/]{1,50}\".prop_map(PathBuf::from)"))]
    pub path: PathBuf,
    
    /// Filename without path
    #[cfg_attr(test, proptest(strategy = "\"[a-zA-Z0-9_]{1,20}\""))]
    pub filename: String,
    
    /// File extension (lowercase)
    #[cfg_attr(test, proptest(strategy = "\"[a-z]{1,5}\""))]
    pub extension: String,
    
    /// File type enum
    pub file_type: FileType,
    
    /// File size in bytes
    pub size_bytes: u64,
    
    /// File content hash (BLAKE3, for change detection)
    #[cfg_attr(test, proptest(strategy = "\"[a-f0-9]{64}\""))]
    pub content_hash: String,
    
    /// Creation time (UTC)
    #[cfg_attr(test, proptest(strategy = "any::<i64>().prop_map(|ts| DateTime::from_timestamp(ts.abs() % 4102444800, 0).unwrap_or_else(|| Utc::now()))"))]
    pub created_at: DateTime<Utc>,
    
    /// Modification time (UTC)
    #[cfg_attr(test, proptest(strategy = "any::<i64>().prop_map(|ts| DateTime::from_timestamp(ts.abs() % 4102444800, 0).unwrap_or_else(|| Utc::now()))"))]
    pub modified_at: DateTime<Utc>,
    
    /// Index time (UTC)
    #[cfg_attr(test, proptest(strategy = "any::<i64>().prop_map(|ts| DateTime::from_timestamp(ts.abs() % 4102444800, 0).unwrap_or_else(|| Utc::now()))"))]
    pub indexed_at: DateTime<Utc>,
    
    /// Last access time (for logic chain)
    #[cfg_attr(test, proptest(strategy = "proptest::option::of(any::<i64>().prop_map(|ts| DateTime::from_timestamp(ts.abs() % 4102444800, 0).unwrap_or_else(|| Utc::now())))"))]
    pub last_accessed_at: Option<DateTime<Utc>>,
    
    /// Index status
    pub index_status: IndexStatus,
    
    /// Privacy level (user-settable)
    pub privacy_level: PrivacyLevel,
    
    /// Whether manually excluded by user
    pub is_excluded: bool,
}

/// File type classification
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum FileType {
    /// Text documents (txt, md, etc.)
    TextDocument,
    /// PDF documents
    Pdf,
    /// Office documents (docx, xlsx, pptx)
    OfficeDocument,
    /// Images (png, jpg, webp, svg)
    Image,
    /// Videos
    Video,
    /// Audio files
    Audio,
    /// Source code files
    Code,
    /// 3D models (obj, fbx, gltf)
    Model3D,
    /// Archive files (zip, tar, etc.)
    Archive,
    /// Unknown/other file types
    Other,
}

/// Index status for a file
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum IndexStatus {
    /// Waiting to be indexed
    Pending,
    /// Currently being indexed
    Indexing,
    /// Successfully indexed
    Indexed,
    /// Indexing failed
    Failed,
    /// Skipped (unsupported format)
    Skipped,
}

/// Privacy level for files
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(test, derive(Arbitrary))]
pub enum PrivacyLevel {
    /// Normal - can be sent to cloud
    Normal,
    /// Sensitive - local processing only
    Sensitive,
    /// Private - excluded from relation recommendations
    Private,
}

impl FileType {
    /// Determine file type from extension
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "txt" | "md" | "markdown" | "rst" | "rtf" => FileType::TextDocument,
            "pdf" => FileType::Pdf,
            "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt" | "ods" | "odp" => {
                FileType::OfficeDocument
            }
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp" | "ico" | "tiff" => {
                FileType::Image
            }
            "mp4" | "avi" | "mkv" | "mov" | "wmv" | "flv" | "webm" => FileType::Video,
            "mp3" | "wav" | "flac" | "aac" | "ogg" | "wma" | "m4a" => FileType::Audio,
            "rs" | "py" | "js" | "ts" | "jsx" | "tsx" | "java" | "c" | "cpp" | "h" | "hpp"
            | "cs" | "go" | "rb" | "php" | "swift" | "kt" | "scala" | "html" | "css" | "scss"
            | "json" | "yaml" | "yml" | "toml" | "xml" | "sql" => FileType::Code,
            "obj" | "fbx" | "gltf" | "glb" | "stl" | "3ds" | "blend" => FileType::Model3D,
            "zip" | "tar" | "gz" | "7z" | "rar" | "bz2" | "xz" => FileType::Archive,
            _ => FileType::Other,
        }
    }
}

impl Default for IndexStatus {
    fn default() -> Self {
        IndexStatus::Pending
    }
}

impl Default for PrivacyLevel {
    fn default() -> Self {
        PrivacyLevel::Normal
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]
        
        /// **Feature: neural-fs-core, Property 18: FileRecord Serialization Round-Trip**
        /// *For any* valid FileRecord, serializing then deserializing should produce an equivalent object
        /// **Validates: Requirements 22**
        #[test]
        fn prop_file_record_json_roundtrip(record in any::<FileRecord>()) {
            // Serialize to JSON
            let json = serde_json::to_string(&record).expect("Failed to serialize FileRecord to JSON");
            
            // Deserialize back
            let deserialized: FileRecord = serde_json::from_str(&json).expect("Failed to deserialize FileRecord from JSON");
            
            // Verify equality
            prop_assert_eq!(record, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 18: FileRecord Serialization Round-Trip (bincode)**
        /// *For any* valid FileRecord, serializing then deserializing with bincode should produce an equivalent object
        /// **Validates: Requirements 22**
        #[test]
        fn prop_file_record_bincode_roundtrip(record in any::<FileRecord>()) {
            // Serialize to bincode
            let bytes = bincode::serialize(&record).expect("Failed to serialize FileRecord to bincode");
            
            // Deserialize back
            let deserialized: FileRecord = bincode::deserialize(&bytes).expect("Failed to deserialize FileRecord from bincode");
            
            // Verify equality
            prop_assert_eq!(record, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 18: FileType Serialization Round-Trip**
        /// *For any* valid FileType, serializing then deserializing should produce an equivalent value
        /// **Validates: Requirements 22**
        #[test]
        fn prop_file_type_roundtrip(file_type in any::<FileType>()) {
            let json = serde_json::to_string(&file_type).expect("Failed to serialize FileType");
            let deserialized: FileType = serde_json::from_str(&json).expect("Failed to deserialize FileType");
            prop_assert_eq!(file_type, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 18: IndexStatus Serialization Round-Trip**
        /// *For any* valid IndexStatus, serializing then deserializing should produce an equivalent value
        /// **Validates: Requirements 22**
        #[test]
        fn prop_index_status_roundtrip(status in any::<IndexStatus>()) {
            let json = serde_json::to_string(&status).expect("Failed to serialize IndexStatus");
            let deserialized: IndexStatus = serde_json::from_str(&json).expect("Failed to deserialize IndexStatus");
            prop_assert_eq!(status, deserialized);
        }
        
        /// **Feature: neural-fs-core, Property 18: PrivacyLevel Serialization Round-Trip**
        /// *For any* valid PrivacyLevel, serializing then deserializing should produce an equivalent value
        /// **Validates: Requirements 22**
        #[test]
        fn prop_privacy_level_roundtrip(level in any::<PrivacyLevel>()) {
            let json = serde_json::to_string(&level).expect("Failed to serialize PrivacyLevel");
            let deserialized: PrivacyLevel = serde_json::from_str(&json).expect("Failed to deserialize PrivacyLevel");
            prop_assert_eq!(level, deserialized);
        }
    }
}
