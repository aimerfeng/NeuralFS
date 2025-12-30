//! Tantivy-based full-text search index for NeuralFS
//!
//! Provides:
//! - Multi-language full-text indexing
//! - Schema version control
//! - Incremental index updates

use std::path::{Path, PathBuf};
use std::sync::Arc;
use tantivy::{
    collector::TopDocs,
    query::QueryParser,
    schema::{
        Field, IndexRecordOption, Schema, TextFieldIndexing, TextOptions, INDEXED, STORED, STRING,
    },
    Index, IndexReader, IndexWriter, ReloadPolicy, TantivyDocument,
};
use thiserror::Error;
use uuid::Uuid;

use super::tokenizer::register_tokenizers;

/// Current schema version - increment when schema changes
const SCHEMA_VERSION: u32 = 1;

/// Schema version file name
const SCHEMA_VERSION_FILE: &str = ".schema_version";

/// Error types for TextIndex operations
#[derive(Error, Debug)]
pub enum TextIndexError {
    #[error("Tantivy error: {0}")]
    Tantivy(#[from] tantivy::TantivyError),

    #[error("Query parse error: {0}")]
    QueryParse(#[from] tantivy::query::QueryParserError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Schema version mismatch: expected {expected}, found {found}")]
    SchemaVersionMismatch { expected: u32, found: u32 },

    #[error("Index not found at path: {0}")]
    IndexNotFound(PathBuf),

    #[error("Field not found: {0}")]
    FieldNotFound(String),
}

/// Configuration for TextIndex
#[derive(Debug, Clone)]
pub struct TextIndexConfig {
    /// Path to the index directory
    pub index_path: PathBuf,

    /// Memory budget for the index writer (in bytes)
    pub writer_memory_bytes: usize,

    /// Number of indexing threads
    pub num_threads: usize,

    /// Whether to auto-rebuild on schema mismatch
    pub auto_rebuild_on_mismatch: bool,
}

impl Default for TextIndexConfig {
    fn default() -> Self {
        Self {
            index_path: PathBuf::from("data/text_index"),
            writer_memory_bytes: 50_000_000, // 50MB
            num_threads: 1,
            auto_rebuild_on_mismatch: true,
        }
    }
}


/// Schema field references for quick access
#[derive(Debug, Clone)]
pub struct SchemaFields {
    pub file_id: Field,
    pub chunk_id: Field,
    pub filename: Field,
    pub content: Field,
    pub tags: Field,
    pub modified_at: Field,
}

/// Full-text search index using Tantivy
pub struct TextIndex {
    index: Index,
    reader: IndexReader,
    schema: Schema,
    fields: SchemaFields,
    config: TextIndexConfig,
}

impl TextIndex {
    /// Create or open a TextIndex at the specified path
    pub fn new(config: TextIndexConfig) -> Result<Self, TextIndexError> {
        let index_path = &config.index_path;

        // Check if index exists and verify schema version
        if index_path.exists() {
            let stored_version = Self::read_schema_version(index_path)?;
            if stored_version != SCHEMA_VERSION {
                if config.auto_rebuild_on_mismatch {
                    tracing::warn!(
                        "Schema version mismatch (expected {}, found {}), rebuilding index",
                        SCHEMA_VERSION,
                        stored_version
                    );
                    std::fs::remove_dir_all(index_path)?;
                } else {
                    return Err(TextIndexError::SchemaVersionMismatch {
                        expected: SCHEMA_VERSION,
                        found: stored_version,
                    });
                }
            }
        }

        // Build schema
        let (schema, fields) = Self::build_schema();

        // Create or open index
        let index = if index_path.exists() {
            Index::open_in_dir(index_path)?
        } else {
            std::fs::create_dir_all(index_path)?;
            let index = Index::create_in_dir(index_path, schema.clone())?;
            Self::write_schema_version(index_path, SCHEMA_VERSION)?;
            index
        };

        // Register multilingual tokenizers
        register_tokenizers(index.tokenizers());

        // Create reader with automatic reload
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        Ok(Self {
            index,
            reader,
            schema,
            fields,
            config,
        })
    }

    /// Build the index schema with multilingual support
    fn build_schema() -> (Schema, SchemaFields) {
        let mut schema_builder = Schema::builder();

        // File ID - stored and indexed as string (for exact match)
        let file_id = schema_builder.add_text_field("file_id", STRING | STORED);

        // Chunk ID - stored and indexed as string
        let chunk_id = schema_builder.add_text_field("chunk_id", STRING | STORED);

        // Filename - use multilingual tokenizer
        let filename_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("multilingual")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions),
            )
            .set_stored();
        let filename = schema_builder.add_text_field("filename", filename_options);

        // Content - use multilingual tokenizer (not stored to save space)
        let content_options = TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("multilingual")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        );
        let content = schema_builder.add_text_field("content", content_options);

        // Tags - use multilingual tokenizer
        let tags_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("multilingual")
                    .set_index_option(IndexRecordOption::WithFreqs),
            )
            .set_stored();
        let tags = schema_builder.add_text_field("tags", tags_options);

        // Modified timestamp - indexed for range queries
        let modified_at = schema_builder.add_u64_field("modified_at", INDEXED | STORED);

        let schema = schema_builder.build();
        let fields = SchemaFields {
            file_id,
            chunk_id,
            filename,
            content,
            tags,
            modified_at,
        };

        (schema, fields)
    }

    /// Read schema version from index directory
    fn read_schema_version(index_path: &Path) -> Result<u32, TextIndexError> {
        let version_file = index_path.join(SCHEMA_VERSION_FILE);
        if version_file.exists() {
            let content = std::fs::read_to_string(&version_file)?;
            content
                .trim()
                .parse()
                .map_err(|_| TextIndexError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "Invalid schema version",
                )))
        } else {
            // No version file means version 0 (legacy)
            Ok(0)
        }
    }

    /// Write schema version to index directory
    fn write_schema_version(index_path: &Path, version: u32) -> Result<(), TextIndexError> {
        let version_file = index_path.join(SCHEMA_VERSION_FILE);
        std::fs::write(version_file, version.to_string())?;
        Ok(())
    }

    /// Get the current schema version
    pub fn schema_version(&self) -> u32 {
        SCHEMA_VERSION
    }

    /// Check if the index needs rebuilding due to schema changes
    pub fn needs_rebuild(&self) -> Result<bool, TextIndexError> {
        let stored_version = Self::read_schema_version(&self.config.index_path)?;
        Ok(stored_version != SCHEMA_VERSION)
    }

    /// Get schema information for diagnostics
    pub fn schema_info(&self) -> SchemaInfo {
        let field_names: Vec<String> = self
            .schema
            .fields()
            .map(|(_, entry)| entry.name().to_string())
            .collect();

        SchemaInfo {
            version: SCHEMA_VERSION,
            field_count: field_names.len(),
            field_names,
            index_path: self.config.index_path.clone(),
        }
    }

    /// Validate that the existing index schema matches expected schema
    /// Returns Ok(true) if compatible, Ok(false) if incompatible
    pub fn validate_schema_compatibility(&self) -> Result<bool, TextIndexError> {
        let expected_fields = vec![
            "file_id", "chunk_id", "filename", "content", "tags", "modified_at",
        ];

        let existing_fields: Vec<&str> = self
            .schema
            .fields()
            .map(|(_, entry)| entry.name())
            .collect();

        // Check all expected fields exist
        for field in &expected_fields {
            if !existing_fields.contains(field) {
                tracing::warn!("Missing expected field: {}", field);
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// Get the stored schema version without opening the full index
    pub fn get_stored_version(index_path: &Path) -> Result<u32, TextIndexError> {
        Self::read_schema_version(index_path)
    }
}

/// Schema information for diagnostics
#[derive(Debug, Clone)]
pub struct SchemaInfo {
    /// Current schema version
    pub version: u32,
    /// Number of fields in schema
    pub field_count: usize,
    /// Names of all fields
    pub field_names: Vec<String>,
    /// Path to index directory
    pub index_path: PathBuf,
}


impl TextIndex {
    /// Create an index writer for batch operations
    pub fn writer(&self) -> Result<IndexWriter, TextIndexError> {
        let writer = self.index.writer(self.config.writer_memory_bytes)?;
        Ok(writer)
    }

    /// Index a document (file or chunk)
    pub fn index_document(
        &self,
        writer: &IndexWriter,
        file_id: &Uuid,
        chunk_id: Option<&Uuid>,
        filename: &str,
        content: &str,
        tags: &[String],
        modified_at: u64,
    ) -> Result<(), TextIndexError> {
        let mut doc = TantivyDocument::new();

        doc.add_text(self.fields.file_id, &file_id.to_string());
        doc.add_text(
            self.fields.chunk_id,
            &chunk_id.map(|id| id.to_string()).unwrap_or_default(),
        );
        doc.add_text(self.fields.filename, filename);
        doc.add_text(self.fields.content, content);
        doc.add_text(self.fields.tags, &tags.join(" "));
        doc.add_u64(self.fields.modified_at, modified_at);

        writer.add_document(doc)?;
        Ok(())
    }

    /// Delete documents by file ID
    pub fn delete_by_file_id(
        &self,
        writer: &IndexWriter,
        file_id: &Uuid,
    ) -> Result<(), TextIndexError> {
        let term = tantivy::Term::from_field_text(self.fields.file_id, &file_id.to_string());
        writer.delete_term(term);
        Ok(())
    }

    /// Commit pending changes
    pub fn commit(&self, writer: &mut IndexWriter) -> Result<(), TextIndexError> {
        writer.commit()?;
        Ok(())
    }

    /// Search the index
    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, TextIndexError> {
        let searcher = self.reader.searcher();

        // Create query parser for content and filename fields
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![self.fields.content, self.fields.filename, self.fields.tags],
        );

        let parsed_query = query_parser.parse_query(query)?;

        // Execute search
        let top_docs = searcher.search(&parsed_query, &TopDocs::with_limit(limit))?;

        // Convert results
        let mut results = Vec::with_capacity(top_docs.len());
        for (score, doc_address) in top_docs {
            let doc: TantivyDocument = searcher.doc(doc_address)?;

            let file_id = doc
                .get_first(self.fields.file_id)
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok());

            let chunk_id = doc
                .get_first(self.fields.chunk_id)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .and_then(|s| Uuid::parse_str(s).ok());

            let filename = doc
                .get_first(self.fields.filename)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let tags = doc
                .get_first(self.fields.tags)
                .and_then(|v| v.as_str())
                .map(|s| s.split_whitespace().map(|t| t.to_string()).collect())
                .unwrap_or_default();

            let modified_at = doc
                .get_first(self.fields.modified_at)
                .and_then(|v| v.as_u64());

            if let Some(file_id) = file_id {
                results.push(SearchResult {
                    file_id,
                    chunk_id,
                    filename,
                    tags,
                    modified_at,
                    score,
                });
            }
        }

        Ok(results)
    }

    /// Search with filters
    pub fn search_with_filters(
        &self,
        query: &str,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<Vec<SearchResult>, TextIndexError> {
        // For now, perform basic search and filter in memory
        // TODO: Implement proper Tantivy filter queries
        let mut results = self.search(query, limit * 2)?;

        // Apply filters
        if let Some(ref tag_filter) = filters.tags {
            results.retain(|r| {
                tag_filter.iter().any(|t| r.tags.contains(t))
            });
        }

        if let Some(min_modified) = filters.min_modified_at {
            results.retain(|r| {
                r.modified_at.map(|m| m >= min_modified).unwrap_or(false)
            });
        }

        if let Some(max_modified) = filters.max_modified_at {
            results.retain(|r| {
                r.modified_at.map(|m| m <= max_modified).unwrap_or(false)
            });
        }

        results.truncate(limit);
        Ok(results)
    }

    /// Get the number of documents in the index
    pub fn num_docs(&self) -> u64 {
        self.reader.searcher().num_docs()
    }

    /// Rebuild the entire index (for schema migration)
    pub fn rebuild(&mut self) -> Result<(), TextIndexError> {
        let index_path = &self.config.index_path;

        // Remove existing index
        if index_path.exists() {
            std::fs::remove_dir_all(index_path)?;
        }

        // Recreate index
        std::fs::create_dir_all(index_path)?;
        let (schema, fields) = Self::build_schema();
        let index = Index::create_in_dir(index_path, schema.clone())?;
        Self::write_schema_version(index_path, SCHEMA_VERSION)?;

        // Register tokenizers
        register_tokenizers(index.tokenizers());

        // Create new reader
        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()?;

        self.index = index;
        self.reader = reader;
        self.schema = schema;
        self.fields = fields;

        Ok(())
    }
}


/// Search result from TextIndex
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// File UUID
    pub file_id: Uuid,

    /// Chunk UUID (if segment-level result)
    pub chunk_id: Option<Uuid>,

    /// Filename
    pub filename: Option<String>,

    /// Associated tags
    pub tags: Vec<String>,

    /// Last modified timestamp
    pub modified_at: Option<u64>,

    /// BM25 relevance score
    pub score: f32,
}

/// Search filters
#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    /// Filter by tags (any match)
    pub tags: Option<Vec<String>>,

    /// Minimum modified timestamp
    pub min_modified_at: Option<u64>,

    /// Maximum modified timestamp
    pub max_modified_at: Option<u64>,

    /// File type filter
    pub file_types: Option<Vec<String>>,
}

impl std::fmt::Debug for TextIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextIndex")
            .field("index_path", &self.config.index_path)
            .field("num_docs", &self.num_docs())
            .field("schema_version", &SCHEMA_VERSION)
            .finish()
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_index() -> (TextIndex, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = TextIndexConfig {
            index_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };
        let index = TextIndex::new(config).unwrap();
        (index, temp_dir)
    }

    #[test]
    fn test_create_index() {
        let (index, _temp_dir) = create_test_index();
        assert_eq!(index.num_docs(), 0);
        assert_eq!(index.schema_version(), SCHEMA_VERSION);
    }

    #[test]
    fn test_index_and_search_english() {
        let (index, _temp_dir) = create_test_index();
        let mut writer = index.writer().unwrap();

        let file_id = Uuid::new_v4();
        index
            .index_document(
                &writer,
                &file_id,
                None,
                "test_document.txt",
                "This is a test document about artificial intelligence",
                &["test".to_string(), "ai".to_string()],
                1234567890,
            )
            .unwrap();

        writer.commit().unwrap();

        // Wait for reader to reload
        std::thread::sleep(std::time::Duration::from_millis(100));

        let results = index.search("artificial intelligence", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].file_id, file_id);
    }

    #[test]
    fn test_index_and_search_chinese() {
        let (index, _temp_dir) = create_test_index();
        let mut writer = index.writer().unwrap();

        let file_id = Uuid::new_v4();
        index
            .index_document(
                &writer,
                &file_id,
                None,
                "测试文档.txt",
                "这是一个关于人工智能的测试文档",
                &["测试".to_string(), "人工智能".to_string()],
                1234567890,
            )
            .unwrap();

        writer.commit().unwrap();

        // Wait for reader to reload
        std::thread::sleep(std::time::Duration::from_millis(100));

        let results = index.search("人工智能", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].file_id, file_id);
    }

    #[test]
    fn test_delete_document() {
        let (index, _temp_dir) = create_test_index();
        let mut writer = index.writer().unwrap();

        let file_id = Uuid::new_v4();
        index
            .index_document(
                &writer,
                &file_id,
                None,
                "to_delete.txt",
                "This document will be deleted",
                &[],
                1234567890,
            )
            .unwrap();

        writer.commit().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Verify document exists
        let results = index.search("deleted", 10).unwrap();
        assert!(!results.is_empty());

        // Delete document
        index.delete_by_file_id(&writer, &file_id).unwrap();
        writer.commit().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Verify document is gone
        let results = index.search("deleted", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_schema_version_persistence() {
        let temp_dir = TempDir::new().unwrap();
        let config = TextIndexConfig {
            index_path: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        // Create index
        {
            let _index = TextIndex::new(config.clone()).unwrap();
        }

        // Reopen index
        let index = TextIndex::new(config).unwrap();
        assert_eq!(index.schema_version(), SCHEMA_VERSION);
    }

    #[test]
    fn test_schema_info() {
        let (index, _temp_dir) = create_test_index();
        let info = index.schema_info();

        assert_eq!(info.version, SCHEMA_VERSION);
        assert_eq!(info.field_count, 6); // file_id, chunk_id, filename, content, tags, modified_at
        assert!(info.field_names.contains(&"file_id".to_string()));
        assert!(info.field_names.contains(&"content".to_string()));
    }

    #[test]
    fn test_schema_compatibility_validation() {
        let (index, _temp_dir) = create_test_index();
        assert!(index.validate_schema_compatibility().unwrap());
    }

    #[test]
    fn test_schema_version_mismatch_auto_rebuild() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().to_path_buf();

        // Create index with current version
        {
            let config = TextIndexConfig {
                index_path: index_path.clone(),
                auto_rebuild_on_mismatch: true,
                ..Default::default()
            };
            let _index = TextIndex::new(config).unwrap();
        }

        // Manually write an old version
        let version_file = index_path.join(".schema_version");
        std::fs::write(&version_file, "0").unwrap();

        // Reopen with auto_rebuild_on_mismatch = true should succeed
        let config = TextIndexConfig {
            index_path: index_path.clone(),
            auto_rebuild_on_mismatch: true,
            ..Default::default()
        };
        let index = TextIndex::new(config).unwrap();
        assert_eq!(index.schema_version(), SCHEMA_VERSION);
    }

    #[test]
    fn test_get_stored_version() {
        let temp_dir = TempDir::new().unwrap();
        let index_path = temp_dir.path().to_path_buf();

        // Create index
        {
            let config = TextIndexConfig {
                index_path: index_path.clone(),
                ..Default::default()
            };
            let _index = TextIndex::new(config).unwrap();
        }

        // Check stored version without opening index
        let version = TextIndex::get_stored_version(&index_path).unwrap();
        assert_eq!(version, SCHEMA_VERSION);
    }
}
