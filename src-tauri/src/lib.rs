//! NeuralFS - AI-driven immersive file system shell
//! 
//! This crate provides the core functionality for NeuralFS including:
//! - Semantic search with intent parsing
//! - Intelligent tag management
//! - Logic chain file associations
//! - Hybrid local/cloud inference
//! - Process supervision via watchdog
//! - OS integration (desktop takeover, hotkeys, multi-monitor)
//! - SQLite database with WAL mode for high concurrency
//! - Full-text search with multi-language tokenization
//! - File system monitoring with event deduplication
//! - File system reconciliation with rename detection
//! - Content parsing for various file formats
//! - Embedding generation with ONNX Runtime

pub mod core;
pub mod db;
pub mod watchdog;
pub mod os;
pub mod vector;
pub mod search;
pub mod watcher;
pub mod reconcile;
pub mod parser;
pub mod indexer;
pub mod embeddings;
pub mod inference;
pub mod tag;
pub mod relation;

// Re-export commonly used items
pub use core::error::{NeuralFSError, Result};
pub use core::config::AppConfig;
pub use db::{DatabaseConfig, create_database_pool, WalCheckpointManager};
pub use os::{DesktopManager, MonitorInfo, MultiMonitorStrategy};
pub use vector::{VectorStore, VectorStoreConfig, VectorError};
pub use search::{TextIndex, TextIndexConfig, TextIndexError, MultilingualTokenizer};
pub use watcher::{FileWatcher, FileWatcherConfig, FileWatcherBuilder, FileEvent, EventBatch, DirectoryFilter, DirectoryFilterConfig, FilterResult, FilterReason};
pub use reconcile::{ReconciliationService, ReconcileConfig, ReconcileResult, FileId, RenameEvent};
pub use parser::{ContentParserService, ContentParser, ParseConfig, ParseResult, ParseMetadata, ParseError, TextParser, PdfParser, CodeParser};
pub use indexer::{ResilientBatchIndexer, IndexerConfig, IndexerStats, IndexTask, TaskStatus, TaskPriority, IndexError as IndexerError};
pub use embeddings::{EmbeddingEngine, EmbeddingConfig, EmbeddingError, ModelManager, ModelLoadingState, VRAMManager, VRAMStatus, ModelType, DilutedAttentionProcessor, DilutedAttentionConfig, ProcessedWindow, CoverageStats};
pub use inference::{
    HybridInferenceEngine, LocalInferenceEngine, CloudBridge, CloudConfig, ResultMerger,
    MergerConfig, DataAnonymizer, InferenceRequest, InferenceResponse, InferenceContext,
    InferenceOptions, InferenceError, LocalModelType, CloudModelType,
};
pub use tag::{
    TagManager, TagManagerConfig, TagSuggestion, AutoTagResult, TagHierarchy, TagNode, TagPath,
    TagCommand, TagCorrectionService, TagCorrectionResult, SensitiveTagDetector, SensitivePattern,
    SensitivityLevel, TagError,
};
pub use relation::{
    LogicChainEngine, LogicChainConfig, RelatedFile, SimilarityResult,
    SessionTracker, SessionConfig, SessionInfo, SessionEvent,
    RelationCommand, RelationCorrectionService, RelationCorrectionResult, BlockScope,
    BlockRuleStore, BlockRuleFilter, RelationError,
};
