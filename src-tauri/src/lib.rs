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
//! - Tauri IPC commands for frontend communication

pub mod core;
pub mod config;
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
pub mod asset;
pub mod preview;
pub mod highlight;
pub mod update;
pub mod commands;
pub mod protocol;
pub mod logging;
pub mod telemetry;

// Re-export commonly used items
pub use core::error::{NeuralFSError, Result};
pub use core::config::AppConfig as CoreAppConfig;
pub use config::{
    ConfigStore, ConfigStoreConfig, ConfigError, ConfigResult,
    AppConfig, CloudConfig, PerformanceConfig, PrivacyConfig, UIConfig,
    ConfigMigration, MigrationManager, MigrationError, MigrationResult,
    ConfigVersion, VersionedConfig,
};
pub use db::{DatabaseConfig, create_database_pool, WalCheckpointManager};
pub use os::{DesktopManager, MonitorInfo, MultiMonitorStrategy};
pub use os::{
    SystemActivityMonitor, ActivityMonitorConfig, SystemState, StateChangeCallback,
    GameModePolicy, GameModePolicyConfig, GameModeStatus, GameModeController,
};
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
pub use asset::{
    SecureAssetStreamServer, AssetServerConfig, AssetServerState,
    CachedThumbnail, CachedPreview, AssetError,
};
pub use preview::{
    PreviewService, PreviewConfig, PreviewError, GeneratedPreview,
    TextPreviewGenerator, TextPreview, HighlightRange,
    ImagePreviewGenerator, ImagePreview, RegionMarker,
    DocumentPreviewGenerator, DocumentPreview, PagePreview,
};
pub use highlight::{
    HighlightNavigator, NavigatorConfig, NavigationTarget, NavigationResult,
    NavigationError, FileOpenMode, AppLauncher, AppInfo, LaunchResult,
    LaunchError, InstalledApp, FileTypeAssociation,
};
pub use update::{
    ModelDownloader, ModelDownloaderConfig, ModelSource, ModelManifest, ModelInfo,
    ModelType, DownloadProgress, DownloadStatus, DownloadError,
};
pub use protocol::{
    register_custom_protocol, ProtocolState, ProtocolConfig,
    get_session_token, AssetProtocolHandler, SessionTokenResponse,
    build_thumbnail_url, build_preview_url, build_file_url,
};
pub use logging::{
    LoggingSystem, LoggingConfig, LoggingError, LoggingResult,
    LogLevel, LogFormat, LogOutput,
    LogRotator, RotationConfig, RotationStrategy,
    LogExporter, ExportConfig, ExportFormat, ExportResult,
    PerformanceMetrics, MetricEntry, MetricType, MetricsCollector,
    init_default_logging, init_logging,
};
pub use telemetry::{
    TelemetrySystem, TelemetryConfig, TelemetryError, TelemetryResult,
    TelemetryEndpoint, TelemetryCollector, TelemetryBatch,
    ConsentManager, ConsentStatus, ConsentRecord,
    TelemetryEvent, EventType, EventData, FeatureUsage, PerformanceEvent, ErrorEvent,
    SessionStats, init_telemetry, init_telemetry_with_config,
};
