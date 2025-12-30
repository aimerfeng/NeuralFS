# NeuralFS 工程设计规格书

## Overview

本文档是 NeuralFS 的工程落地规格书，面向后端工程师和性能优化专家，提供可直接指导编码的详细设计。

### 技术栈确认

| 层级 | 技术选型 | 版本 | 理由 |
|------|----------|------|------|
| 前端 Shell | Tauri + SolidJS | Tauri 1.5+ | 原生性能，跨平台 |
| 后端核心 | Rust | 1.75+ | 内存安全，高性能 |
| 嵌入引擎 | ONNX Runtime | 1.16+ | 跨平台本地推理，CUDA支持 |
| 向量存储 | Qdrant (嵌入式) | 1.7+ | Rust原生，无需外部服务 |
| 文件监控 | notify-rs | 6.1+ | 高效跨平台文件系统事件 |
| 异步运行时 | Tokio | 1.35+ | 高性能异步I/O |
| 序列化 | serde + bincode | - | 高效二进制序列化 |

### 构建配置要点

#### Tauri Sidecar (Watchdog) 二进制路径

Tauri 的 Sidecar 机制要求严格的命名规则。`tauri.conf.json` 中配置的 `externalBin: ["binaries/watchdog"]` 不会自动从 `target/release/` 查找，而是期望在 `src-tauri/binaries/` 目录下存在符合平台命名规则的文件：

| 平台 | 文件名 |
|------|--------|
| Windows x64 | `watchdog-x86_64-pc-windows-msvc.exe` |
| macOS x64 | `watchdog-x86_64-apple-darwin` |
| macOS ARM | `watchdog-aarch64-apple-darwin` |
| Linux x64 | `watchdog-x86_64-unknown-linux-gnu` |

**构建流程**：在 `tauri build` 之前，需要先编译 watchdog 并移动到正确位置。建议通过构建脚本或 Makefile 自动化此流程。

#### ONNX Runtime DLL 路径配置

`build.rs` 中的 ONNX Runtime 路径搜索顺序：

```rust
let onnx_paths = [
    "deps/onnxruntime",              // 项目本地 (推荐用于分发)
    "C:/Program Files/onnxruntime",  // Windows 系统安装
    "/usr/local/lib",                // Unix 系统安装
    std::env::var("ONNXRUNTIME_DIR").ok(), // 环境变量
];
```

**注意**：确保 `deps/onnxruntime/` 目录包含正确版本的 DLL/dylib/so 文件，并在 CI/CD 中正确配置。

#### SQLite WAL 模式

`Cargo.toml` 中定义的 `wal = []` feature 需要在数据库初始化时读取：

```rust
// 在 create_database_pool() 中
.journal_mode(if cfg!(feature = "wal") { 
    SqliteJournalMode::Wal 
} else { 
    SqliteJournalMode::Delete 
})
```

## Architecture

### 系统架构图

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              NeuralFS Shell (Tauri)                          │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌──────────────────┐ │
│  │  SearchBar   │  │  FileGrid    │  │  TagPanel    │  │  RelationGraph   │ │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘  └────────┬─────────┘ │
│         │                 │                 │                    │           │
│  ┌──────┴─────────────────┴─────────────────┴────────────────────┴────────┐  │
│  │                         Tauri IPC Bridge                               │  │
│  └────────────────────────────────┬───────────────────────────────────────┘  │
└───────────────────────────────────┼──────────────────────────────────────────┘
                                    │
┌───────────────────────────────────┼──────────────────────────────────────────┐
│                              Rust Backend                                     │
│  ┌────────────────────────────────┴───────────────────────────────────────┐  │
│  │                         Command Router (AppState)                       │  │
│  └────┬──────────┬──────────┬──────────┬──────────┬──────────┬───────────┘  │
│       │          │          │          │          │          │              │
│  ┌────┴────┐ ┌───┴────┐ ┌───┴────┐ ┌───┴────┐ ┌───┴────┐ ┌───┴─────┐       │
│  │ Search  │ │ Index  │ │  Tag   │ │ Logic  │ │ Hybrid │ │ Config  │       │
│  │ Engine  │ │ Service│ │Manager │ │ Chain  │ │Inference│ │ Manager │       │
│  └────┬────┘ └───┬────┘ └───┬────┘ └───┬────┘ └───┬────┘ └─────────┘       │
│       │          │          │          │          │                         │
│  ┌────┴──────────┴──────────┴──────────┴──────────┴────────────────────┐   │
│  │                      Core Data Layer                                 │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │   │
│  │  │ VectorStore │  │ MetadataDB  │  │ RelationDB  │  │ ConfigStore│  │   │
│  │  │  (Qdrant)   │  │  (SQLite)   │  │  (SQLite)   │  │   (JSON)   │  │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  └────────────┘  │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐   │
│  │                      Background Services                              │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │   │
│  │  │FileWatcher  │  │ContentParser│  │EmbeddingEng │  │CloudBridge │  │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘  └────────────┘  │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────────────┘
```


## Components and Interfaces

### 1. 核心数据结构

#### 1.1 文件索引记录 (FileRecord)

```rust
/// 文件索引记录 - 存储在 MetadataDB (SQLite)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRecord {
    /// 唯一标识符 (UUID v7 - 时间有序)
    pub id: Uuid,
    
    /// 文件绝对路径
    pub path: PathBuf,
    
    /// 文件名 (不含路径)
    pub filename: String,
    
    /// 文件扩展名 (小写)
    pub extension: String,
    
    /// 文件类型枚举
    pub file_type: FileType,
    
    /// 文件大小 (字节)
    pub size_bytes: u64,
    
    /// 文件内容哈希 (BLAKE3, 用于变更检测)
    pub content_hash: String,
    
    /// 创建时间 (UTC)
    pub created_at: DateTime<Utc>,
    
    /// 修改时间 (UTC)
    pub modified_at: DateTime<Utc>,
    
    /// 索引时间 (UTC)
    pub indexed_at: DateTime<Utc>,
    
    /// 最后访问时间 (用于逻辑链条)
    pub last_accessed_at: Option<DateTime<Utc>>,
    
    /// 索引状态
    pub index_status: IndexStatus,
    
    /// 隐私级别 (用户可设置)
    pub privacy_level: PrivacyLevel,
    
    /// 是否被用户手动排除
    pub is_excluded: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum IndexStatus {
    Pending,      // 等待索引
    Indexing,     // 正在索引
    Indexed,      // 已索引
    Failed,       // 索引失败
    Skipped,      // 跳过 (不支持的格式)
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PrivacyLevel {
    Normal,       // 正常 - 可发送到云端
    Sensitive,    // 敏感 - 仅本地处理
    Private,      // 私密 - 不参与关联推荐
}
```

#### 1.2 内容片段 (ContentChunk)

```rust
/// 内容片段 - 文档被切分后的语义单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentChunk {
    /// 片段唯一标识符
    pub id: Uuid,
    
    /// 所属文件ID
    pub file_id: Uuid,
    
    /// 片段在文件中的序号 (从0开始)
    pub chunk_index: u32,
    
    /// 片段类型
    pub chunk_type: ChunkType,
    
    /// 片段文本内容 (用于预览)
    pub content: String,
    
    /// 片段在原文件中的位置
    pub location: ChunkLocation,
    
    /// 向量ID (Qdrant中的point_id)
    pub vector_id: u64,
    
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ChunkType {
    Paragraph,    // 段落
    Heading,      // 标题
    CodeBlock,    // 代码块
    Table,        // 表格
    Image,        // 图片区域
    Caption,      // 图片/表格说明
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkLocation {
    /// 起始字节偏移
    pub start_offset: u64,
    /// 结束字节偏移
    pub end_offset: u64,
    /// 起始行号 (文本文件)
    pub start_line: Option<u32>,
    /// 结束行号 (文本文件)
    pub end_line: Option<u32>,
    /// 页码 (PDF)
    pub page_number: Option<u32>,
    /// 图片区域坐标 (x, y, width, height) - 归一化到0-1
    pub bounding_box: Option<(f32, f32, f32, f32)>,
}
```


#### 1.3 标签系统 (Tag)

```rust
/// 标签定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tag {
    /// 标签唯一标识符
    pub id: Uuid,
    
    /// 标签名称 (支持多语言)
    pub name: String,
    
    /// 标签显示名称 (本地化)
    pub display_name: HashMap<String, String>, // locale -> name
    
    /// 父标签ID (用于层级结构)
    pub parent_id: Option<Uuid>,
    
    /// 标签类型
    pub tag_type: TagType,
    
    /// 标签颜色 (Hex)
    pub color: String,
    
    /// 标签图标 (emoji或图标名)
    pub icon: Option<String>,
    
    /// 是否为系统标签 (不可删除)
    pub is_system: bool,
    
    /// 创建时间
    pub created_at: DateTime<Utc>,
    
    /// 使用次数 (用于排序)
    pub usage_count: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TagType {
    Category,     // 分类标签 (工作、个人、学习)
    FileType,     // 文件类型标签 (文档、图片、代码)
    Project,      // 项目标签
    Status,       // 状态标签 (进行中、已完成)
    Custom,       // 用户自定义
    AutoGenerated,// AI自动生成
}

/// 文件-标签关联
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileTagRelation {
    /// 关联ID
    pub id: Uuid,
    
    /// 文件ID
    pub file_id: Uuid,
    
    /// 标签ID
    pub tag_id: Uuid,
    
    /// 关联来源
    pub source: TagSource,
    
    /// 置信度 (AI生成时)
    pub confidence: Option<f32>,
    
    /// 是否被用户确认
    pub is_confirmed: bool,
    
    /// 是否被用户拒绝 (用于学习)
    pub is_rejected: bool,
    
    /// 创建时间
    pub created_at: DateTime<Utc>,
    
    /// 用户操作时间 (确认/拒绝)
    pub user_action_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TagSource {
    Manual,       // 用户手动添加
    AIGenerated,  // AI自动生成
    Inherited,    // 从父目录继承
    Imported,     // 从外部导入
}
```

#### 1.4 逻辑链条关联 (FileRelation)

```rust
/// 文件关联关系 - 核心数据结构，支持人工介入修正
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileRelation {
    /// 关联ID
    pub id: Uuid,
    
    /// 源文件ID
    pub source_file_id: Uuid,
    
    /// 目标文件ID
    pub target_file_id: Uuid,
    
    /// 关联类型
    pub relation_type: RelationType,
    
    /// 关联强度 (0.0 - 1.0)
    pub strength: f32,
    
    /// 关联来源
    pub source: RelationSource,
    
    /// 用户反馈状态 - 关键字段，支持人工介入
    pub user_feedback: UserFeedback,
    
    /// 创建时间
    pub created_at: DateTime<Utc>,
    
    /// 最后更新时间
    pub updated_at: DateTime<Utc>,
    
    /// 用户操作时间
    pub user_action_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RelationType {
    ContentSimilar,   // 内容相似
    SameSession,      // 同一会话打开
    SameProject,      // 同一项目
    SameAuthor,       // 同一作者
    Reference,        // 引用关系
    Derivative,       // 衍生关系 (如视频和其素材)
    Workflow,         // 工作流关联
    UserDefined,      // 用户手动定义
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RelationSource {
    AIGenerated,      // AI自动生成
    SessionTracking,  // 会话追踪
    UserManual,       // 用户手动创建
    MetadataExtract,  // 元数据提取
}

/// 用户反馈状态 - 人工介入修正的核心
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum UserFeedback {
    /// 未操作 - 默认状态
    None,
    
    /// 用户确认 - 关联有效
    Confirmed,
    
    /// 用户拒绝 - 一键解除关联
    Rejected {
        /// 拒绝原因 (可选)
        reason: Option<String>,
        /// 是否屏蔽此类关联 (防止再次生成)
        block_similar: bool,
    },
    
    /// 用户调整 - 修改关联强度
    Adjusted {
        /// 原始强度
        original_strength: f32,
        /// 用户设置的强度
        user_strength: f32,
    },
}
```


#### 1.5 关联屏蔽规则 (RelationBlockRule)

```rust
/// 关联屏蔽规则 - 防止AI重复生成被拒绝的关联
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationBlockRule {
    /// 规则ID
    pub id: Uuid,
    
    /// 规则类型
    pub rule_type: BlockRuleType,
    
    /// 规则详情
    pub rule_detail: BlockRuleDetail,
    
    /// 创建时间
    pub created_at: DateTime<Utc>,
    
    /// 过期时间 (可选，None表示永久)
    pub expires_at: Option<DateTime<Utc>>,
    
    /// 是否激活
    pub is_active: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BlockRuleType {
    /// 屏蔽两个特定文件之间的关联
    FilePair,
    
    /// 屏蔽特定文件与某标签下所有文件的关联
    FileToTag,
    
    /// 屏蔽两个标签之间所有文件的关联
    TagPair,
    
    /// 屏蔽特定文件的所有AI关联
    FileAllAI,
    
    /// 屏蔽特定关联类型
    RelationType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockRuleDetail {
    FilePair {
        file_id_a: Uuid,
        file_id_b: Uuid,
    },
    FileToTag {
        file_id: Uuid,
        tag_id: Uuid,
    },
    TagPair {
        tag_id_a: Uuid,
        tag_id_b: Uuid,
    },
    FileAllAI {
        file_id: Uuid,
    },
    RelationType {
        file_id: Option<Uuid>, // None表示全局
        relation_type: RelationType,
    },
}
```

#### 1.6 搜索请求与响应

```rust
/// 搜索请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    /// 用户原始查询
    pub query: String,
    
    /// 解析后的意图
    pub intent: Option<SearchIntent>,
    
    /// 过滤条件
    pub filters: SearchFilters,
    
    /// 分页
    pub pagination: Pagination,
    
    /// 是否启用云端增强
    pub enable_cloud: bool,
    
    /// 请求ID (用于追踪)
    pub request_id: Uuid,
    
    /// 请求时间
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchIntent {
    /// 查找文件
    FindFile {
        /// 文件类型提示
        file_type_hint: Option<FileType>,
        /// 时间范围提示
        time_hint: Option<TimeRange>,
    },
    
    /// 查找内容片段
    FindContent {
        /// 期望的内容类型
        content_type: Option<ChunkType>,
        /// 是否需要精确位置
        need_location: bool,
    },
    
    /// 模糊查询 (需要澄清)
    Ambiguous {
        /// 可能的解释
        possible_intents: Vec<SearchIntent>,
        /// 建议的澄清问题
        clarification_questions: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchFilters {
    /// 文件类型过滤
    pub file_types: Option<Vec<FileType>>,
    
    /// 标签过滤 (AND逻辑)
    pub tags: Option<Vec<Uuid>>,
    
    /// 排除标签
    pub exclude_tags: Option<Vec<Uuid>>,
    
    /// 时间范围
    pub time_range: Option<TimeRange>,
    
    /// 路径前缀
    pub path_prefix: Option<PathBuf>,
    
    /// 最小相似度
    pub min_score: f32,
    
    /// 排除私密文件
    pub exclude_private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub offset: u32,
    pub limit: u32,
}

impl Default for Pagination {
    fn default() -> Self {
        Self { offset: 0, limit: 20 }
    }
}
```


#### 1.7 搜索响应

```rust
/// 搜索响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    /// 请求ID
    pub request_id: Uuid,
    
    /// 响应状态
    pub status: SearchStatus,
    
    /// 搜索结果
    pub results: Vec<SearchResult>,
    
    /// 总匹配数 (用于分页)
    pub total_count: u64,
    
    /// 是否有更多结果
    pub has_more: bool,
    
    /// 搜索耗时 (毫秒)
    pub duration_ms: u64,
    
    /// 数据来源
    pub sources: Vec<ResultSource>,
    
    /// 澄清建议 (如果意图模糊)
    pub clarifications: Option<Vec<Clarification>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SearchStatus {
    Success,
    PartialSuccess,  // 部分成功 (如云端超时)
    NeedsClarity,    // 需要用户澄清
    NoResults,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ResultSource {
    LocalVector,     // 本地向量搜索
    LocalTag,        // 本地标签匹配
    CloudEnhanced,   // 云端增强
}

/// 单个搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// 结果类型
    pub result_type: SearchResultType,
    
    /// 文件信息
    pub file: FileRecord,
    
    /// 匹配的内容片段 (如果是内容搜索)
    pub matched_chunk: Option<ContentChunk>,
    
    /// 相似度分数 (0.0 - 1.0)
    pub score: f32,
    
    /// 预览内容
    pub preview: ResultPreview,
    
    /// 高亮信息
    pub highlights: Vec<Highlight>,
    
    /// 相关标签
    pub tags: Vec<Tag>,
    
    /// 数据来源
    pub source: ResultSource,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SearchResultType {
    File,            // 文件级结果
    ContentChunk,    // 内容片段结果
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultPreview {
    /// 预览类型
    pub preview_type: PreviewType,
    
    /// 预览内容
    pub content: PreviewContent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PreviewType {
    Text,
    Image,
    Thumbnail,
    Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PreviewContent {
    Text {
        snippet: String,
        full_text: Option<String>,
    },
    Image {
        thumbnail_base64: String,
        width: u32,
        height: u32,
    },
    Metadata {
        entries: HashMap<String, String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Highlight {
    /// 高亮起始位置
    pub start: u32,
    /// 高亮结束位置
    pub end: u32,
    /// 高亮类型
    pub highlight_type: HighlightType,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum HighlightType {
    ExactMatch,      // 精确匹配
    SemanticMatch,   // 语义匹配
    KeywordMatch,    // 关键词匹配
}

/// 澄清建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Clarification {
    /// 澄清问题
    pub question: String,
    
    /// 可选答案
    pub options: Vec<ClarificationOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClarificationOption {
    /// 选项文本
    pub text: String,
    
    /// 选择后的搜索意图
    pub intent: SearchIntent,
    
    /// 预估结果数
    pub estimated_count: Option<u64>,
}
```


### 2. 人工介入修正 (Human-in-the-Loop) 接口设计

#### 2.1 关联修正 API

```rust
/// 关联修正命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RelationCommand {
    /// 确认关联有效
    Confirm {
        relation_id: Uuid,
    },
    
    /// 拒绝关联 (一键解除)
    Reject {
        relation_id: Uuid,
        reason: Option<String>,
        /// 是否屏蔽此类关联
        block_similar: bool,
        /// 屏蔽范围
        block_scope: Option<BlockScope>,
    },
    
    /// 调整关联强度
    Adjust {
        relation_id: Uuid,
        new_strength: f32,
    },
    
    /// 手动创建关联
    Create {
        source_file_id: Uuid,
        target_file_id: Uuid,
        relation_type: RelationType,
        strength: f32,
    },
    
    /// 批量拒绝 (如拒绝某文件的所有AI关联)
    BatchReject {
        file_id: Uuid,
        relation_types: Option<Vec<RelationType>>,
        block_future: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum BlockScope {
    /// 仅屏蔽这两个文件之间
    ThisPairOnly,
    
    /// 屏蔽源文件与目标文件所属标签的所有关联
    SourceToTargetTag {
        target_tag_id: Uuid,
    },
    
    /// 屏蔽源文件所属标签与目标文件所属标签的所有关联
    TagToTag {
        source_tag_id: Uuid,
        target_tag_id: Uuid,
    },
}

/// 关联修正服务
pub trait RelationCorrectionService {
    /// 执行关联修正命令
    async fn execute(&self, cmd: RelationCommand) -> Result<RelationCorrectionResult>;
    
    /// 获取文件的所有关联 (包含用户反馈状态)
    async fn get_relations(&self, file_id: Uuid) -> Result<Vec<FileRelation>>;
    
    /// 获取被屏蔽的关联规则
    async fn get_block_rules(&self, file_id: Option<Uuid>) -> Result<Vec<RelationBlockRule>>;
    
    /// 删除屏蔽规则
    async fn remove_block_rule(&self, rule_id: Uuid) -> Result<()>;
    
    /// 学习用户偏好 (基于确认/拒绝历史)
    async fn learn_preferences(&self) -> Result<UserPreferenceModel>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelationCorrectionResult {
    pub success: bool,
    pub affected_relations: Vec<Uuid>,
    pub created_block_rules: Vec<Uuid>,
    pub message: String,
}
```

#### 2.2 标签修正 API

```rust
/// 标签修正命令
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TagCommand {
    /// 确认AI生成的标签
    ConfirmTag {
        file_id: Uuid,
        tag_id: Uuid,
    },
    
    /// 拒绝AI生成的标签
    RejectTag {
        file_id: Uuid,
        tag_id: Uuid,
        /// 是否屏蔽此文件的此类标签
        block_similar: bool,
    },
    
    /// 手动添加标签
    AddTag {
        file_id: Uuid,
        tag_id: Uuid,
    },
    
    /// 移除标签
    RemoveTag {
        file_id: Uuid,
        tag_id: Uuid,
    },
    
    /// 批量操作
    BatchTag {
        file_ids: Vec<Uuid>,
        add_tags: Vec<Uuid>,
        remove_tags: Vec<Uuid>,
    },
    
    /// 创建新标签
    CreateTag {
        name: String,
        parent_id: Option<Uuid>,
        tag_type: TagType,
        color: Option<String>,
    },
    
    /// 合并标签
    MergeTags {
        source_tag_ids: Vec<Uuid>,
        target_tag_id: Uuid,
    },
}

/// 标签修正服务
pub trait TagCorrectionService {
    async fn execute(&self, cmd: TagCommand) -> Result<TagCorrectionResult>;
    
    /// 获取文件的标签 (包含来源和置信度)
    async fn get_file_tags(&self, file_id: Uuid) -> Result<Vec<FileTagRelation>>;
    
    /// 获取标签建议 (基于文件内容)
    async fn suggest_tags(&self, file_id: Uuid) -> Result<Vec<TagSuggestion>>;
    
    /// 学习用户标签偏好
    async fn learn_tag_preferences(&self) -> Result<TagPreferenceModel>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagSuggestion {
    pub tag: Tag,
    pub confidence: f32,
    pub reason: String,
}
```


### 3. 并行推理架构详细设计

#### 3.1 推理管道

```rust
/// 推理请求
#[derive(Debug, Clone)]
pub struct InferenceRequest {
    pub request_id: Uuid,
    pub query: String,
    pub context: InferenceContext,
    pub options: InferenceOptions,
}

#[derive(Debug, Clone)]
pub struct InferenceContext {
    /// 相关标签 (本地分析得出)
    pub relevant_tags: Vec<Tag>,
    
    /// 文件结构上下文
    pub file_structure: Option<FileStructureContext>,
    
    /// 用户历史 (最近搜索、最近打开)
    pub user_history: UserHistoryContext,
    
    /// 当前会话上下文
    pub session_context: SessionContext,
}

#[derive(Debug, Clone)]
pub struct InferenceOptions {
    /// 是否启用云端
    pub enable_cloud: bool,
    
    /// 云端超时 (毫秒)
    pub cloud_timeout_ms: u64,
    
    /// 本地模型选择
    pub local_model: LocalModelType,
    
    /// 云端模型选择
    pub cloud_model: Option<CloudModelType>,
}

#[derive(Debug, Clone, Copy)]
pub enum LocalModelType {
    Fast,      // MiniLM-L6 (快速)
    Balanced,  // all-MiniLM-L12 (平衡)
    Accurate,  // BGE-base (准确)
}

#[derive(Debug, Clone, Copy)]
pub enum CloudModelType {
    GPT4oMini,
    ClaudeHaiku,
    Custom(String),
}

/// 并行推理引擎
pub struct HybridInferenceEngine {
    local_engine: LocalInferenceEngine,
    cloud_bridge: CloudBridge,
    result_merger: ResultMerger,
    cache: InferenceCache,
}

impl HybridInferenceEngine {
    /// 执行并行推理
    pub async fn infer(&self, request: InferenceRequest) -> InferenceResult {
        let request_id = request.request_id;
        
        // 1. 检查缓存
        if let Some(cached) = self.cache.get(&request.query).await {
            return cached;
        }
        
        // 2. 并行启动本地和云端推理
        let local_future = self.local_engine.infer(request.clone());
        let cloud_future = self.cloud_bridge.infer(request.clone());
        
        // 3. 本地先返回初步结果
        let local_result = local_future.await;
        
        // 4. 等待云端结果 (带超时)
        let cloud_result = tokio::time::timeout(
            Duration::from_millis(request.options.cloud_timeout_ms),
            cloud_future
        ).await;
        
        // 5. 合并结果
        let merged = match cloud_result {
            Ok(Ok(cloud)) => self.result_merger.merge(local_result, Some(cloud)),
            Ok(Err(e)) => {
                tracing::warn!("Cloud inference failed: {}", e);
                self.result_merger.merge(local_result, None)
            }
            Err(_) => {
                tracing::warn!("Cloud inference timeout");
                self.result_merger.merge(local_result, None)
            }
        };
        
        // 6. 缓存结果
        self.cache.put(&request.query, merged.clone()).await;
        
        merged
    }
}

/// 本地推理引擎
pub struct LocalInferenceEngine {
    embedding_engine: EmbeddingEngine,
    tag_matcher: TagMatcher,
    intent_parser: IntentParser,
}

impl LocalInferenceEngine {
    pub async fn infer(&self, request: InferenceRequest) -> LocalInferenceResult {
        // 1. 生成查询嵌入向量
        let query_embedding = self.embedding_engine
            .embed_text_content(&request.query)
            .await?;
        
        // 2. 本地标签匹配
        let tag_matches = self.tag_matcher
            .match_tags(&request.query, &request.context.relevant_tags)
            .await;
        
        // 3. 意图解析
        let intent = self.intent_parser
            .parse(&request.query, &request.context)
            .await;
        
        // 4. 生成上下文增强的云端提示词
        let cloud_prompt = self.generate_cloud_prompt(
            &request.query,
            &tag_matches,
            &intent,
            &request.context
        );
        
        LocalInferenceResult {
            query_embedding,
            tag_matches,
            intent,
            cloud_prompt,
            duration_ms: 0, // 实际计时
        }
    }
    
    fn generate_cloud_prompt(
        &self,
        query: &str,
        tag_matches: &[TagMatch],
        intent: &SearchIntent,
        context: &InferenceContext,
    ) -> String {
        // 生成结构化提示词，帮助云端模型理解上下文
        format!(
            r#"用户查询: "{}"

上下文信息:
- 相关标签: {}
- 解析意图: {:?}
- 最近访问: {}
- 文件结构: {}

请分析用户意图并提供搜索建议。"#,
            query,
            tag_matches.iter().map(|t| &t.tag.name).collect::<Vec<_>>().join(", "),
            intent,
            context.user_history.recent_files.len(),
            context.file_structure.as_ref().map(|f| f.summary.as_str()).unwrap_or("无")
        )
    }
}
```


#### 3.2 云端桥接

```rust
/// 云端推理桥接
pub struct CloudBridge {
    client: reqwest::Client,
    config: CloudConfig,
    rate_limiter: RateLimiter,
    cost_tracker: CostTracker,
}

#[derive(Debug, Clone)]
pub struct CloudConfig {
    /// API端点
    pub endpoint: String,
    
    /// API密钥 (加密存储)
    pub api_key: SecretString,
    
    /// 模型选择
    pub model: CloudModelType,
    
    /// 每月成本限制 (美元)
    pub monthly_cost_limit: f64,
    
    /// 每分钟请求限制
    pub requests_per_minute: u32,
    
    /// 是否启用
    pub enabled: bool,
}

impl CloudBridge {
    pub async fn infer(&self, request: InferenceRequest) -> Result<CloudInferenceResult> {
        // 1. 检查成本限制
        if self.cost_tracker.is_limit_reached().await {
            return Err(CloudError::CostLimitReached);
        }
        
        // 2. 检查速率限制
        self.rate_limiter.acquire().await?;
        
        // 3. 准备请求 (匿名化处理)
        let anonymized_prompt = self.anonymize_prompt(&request);
        
        // 4. 发送请求
        let response = self.client
            .post(&self.config.endpoint)
            .header("Authorization", format!("Bearer {}", self.config.api_key.expose()))
            .json(&CloudRequest {
                model: self.config.model.to_string(),
                messages: vec![
                    CloudMessage {
                        role: "system".to_string(),
                        content: SYSTEM_PROMPT.to_string(),
                    },
                    CloudMessage {
                        role: "user".to_string(),
                        content: anonymized_prompt,
                    },
                ],
                max_tokens: 500,
                temperature: 0.3,
            })
            .send()
            .await?;
        
        // 5. 解析响应
        let cloud_response: CloudResponse = response.json().await?;
        
        // 6. 记录成本
        self.cost_tracker.record(cloud_response.usage.total_tokens).await;
        
        // 7. 解析结果
        self.parse_response(cloud_response)
    }
    
    fn anonymize_prompt(&self, request: &InferenceRequest) -> String {
        // 移除文件路径中的用户名等敏感信息
        let mut prompt = request.context.user_history
            .recent_files
            .iter()
            .map(|f| f.filename.clone())
            .collect::<Vec<_>>()
            .join(", ");
        
        // 替换敏感模式
        prompt = prompt
            .replace(std::env::var("USERNAME").unwrap_or_default().as_str(), "[USER]")
            .replace(std::env::var("HOME").unwrap_or_default().as_str(), "[HOME]");
        
        prompt
    }
}

/// 成本追踪器
pub struct CostTracker {
    db: SqlitePool,
    monthly_limit: f64,
}

impl CostTracker {
    pub async fn record(&self, tokens: u32) {
        let cost = self.calculate_cost(tokens);
        
        sqlx::query!(
            r#"
            INSERT INTO cloud_usage (timestamp, tokens, cost)
            VALUES (datetime('now'), ?, ?)
            "#,
            tokens,
            cost
        )
        .execute(&self.db)
        .await
        .ok();
    }
    
    pub async fn is_limit_reached(&self) -> bool {
        let result = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(SUM(cost), 0.0) as total
            FROM cloud_usage
            WHERE timestamp >= datetime('now', 'start of month')
            "#
        )
        .fetch_one(&self.db)
        .await
        .unwrap_or(0.0);
        
        result >= self.monthly_limit
    }
    
    fn calculate_cost(&self, tokens: u32) -> f64 {
        // GPT-4o-mini: $0.15/1M input, $0.60/1M output
        // 简化计算，假设平均
        (tokens as f64) * 0.0000003
    }
}
```

#### 3.3 结果合并器

```rust
/// 结果合并器 - 融合本地和云端结果
pub struct ResultMerger {
    config: MergerConfig,
}

#[derive(Debug, Clone)]
pub struct MergerConfig {
    /// 本地结果权重
    pub local_weight: f32,
    
    /// 云端结果权重
    pub cloud_weight: f32,
    
    /// 最小合并分数
    pub min_merge_score: f32,
}

impl ResultMerger {
    pub fn merge(
        &self,
        local: LocalInferenceResult,
        cloud: Option<CloudInferenceResult>,
    ) -> InferenceResult {
        let mut results = Vec::new();
        
        // 1. 添加本地结果
        for local_result in local.results {
            results.push(MergedResult {
                file_id: local_result.file_id,
                score: local_result.score * self.config.local_weight,
                source: ResultSource::LocalVector,
                local_data: Some(local_result),
                cloud_data: None,
            });
        }
        
        // 2. 合并云端结果
        if let Some(cloud) = cloud {
            for cloud_result in cloud.results {
                // 查找是否已有本地结果
                if let Some(existing) = results.iter_mut()
                    .find(|r| r.file_id == cloud_result.file_id)
                {
                    // 合并分数
                    existing.score = (existing.score + cloud_result.score * self.config.cloud_weight) / 2.0;
                    existing.cloud_data = Some(cloud_result);
                    existing.source = ResultSource::CloudEnhanced;
                } else {
                    // 添加新结果
                    results.push(MergedResult {
                        file_id: cloud_result.file_id,
                        score: cloud_result.score * self.config.cloud_weight,
                        source: ResultSource::CloudEnhanced,
                        local_data: None,
                        cloud_data: Some(cloud_result),
                    });
                }
            }
        }
        
        // 3. 排序并过滤
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.retain(|r| r.score >= self.config.min_merge_score);
        
        InferenceResult {
            results,
            intent: local.intent,
            cloud_enhanced: cloud.is_some(),
            duration_ms: 0,
        }
    }
}
```


### 4. 性能优化设计

#### 4.1 显存管理

```rust
/// 显存管理器
pub struct VRAMManager {
    /// 最大显存使用量 (字节)
    max_vram_bytes: u64,
    
    /// 当前使用量
    current_usage: AtomicU64,
    
    /// 模型加载状态
    loaded_models: RwLock<HashMap<ModelId, ModelInfo>>,
    
    /// LRU缓存
    model_cache: LruCache<ModelId, ModelHandle>,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: ModelId,
    pub name: String,
    pub vram_bytes: u64,
    pub last_used: Instant,
    pub use_count: u64,
}

impl VRAMManager {
    pub fn new(max_vram_mb: u64) -> Self {
        Self {
            max_vram_bytes: max_vram_mb * 1024 * 1024,
            current_usage: AtomicU64::new(0),
            loaded_models: RwLock::new(HashMap::new()),
            model_cache: LruCache::new(NonZeroUsize::new(10).unwrap()),
        }
    }
    
    /// 请求加载模型
    pub async fn request_model(&self, model_id: ModelId) -> Result<ModelHandle> {
        let model_info = self.get_model_info(model_id)?;
        
        // 检查是否已加载
        if let Some(handle) = self.model_cache.get(&model_id) {
            return Ok(handle.clone());
        }
        
        // 检查显存是否足够
        let current = self.current_usage.load(Ordering::SeqCst);
        if current + model_info.vram_bytes > self.max_vram_bytes {
            // 需要卸载其他模型
            self.evict_models(model_info.vram_bytes).await?;
        }
        
        // 加载模型
        let handle = self.load_model(model_id).await?;
        
        // 更新使用量
        self.current_usage.fetch_add(model_info.vram_bytes, Ordering::SeqCst);
        self.model_cache.put(model_id, handle.clone());
        
        Ok(handle)
    }
    
    /// 卸载模型以释放显存
    async fn evict_models(&self, needed_bytes: u64) -> Result<()> {
        let mut freed = 0u64;
        let mut to_evict = Vec::new();
        
        // 按LRU顺序选择要卸载的模型
        let models = self.loaded_models.read().await;
        let mut sorted: Vec<_> = models.values().collect();
        sorted.sort_by_key(|m| m.last_used);
        
        for model in sorted {
            if freed >= needed_bytes {
                break;
            }
            to_evict.push(model.id);
            freed += model.vram_bytes;
        }
        drop(models);
        
        // 执行卸载
        for model_id in to_evict {
            self.unload_model(model_id).await?;
        }
        
        Ok(())
    }
    
    /// 获取当前显存使用状态
    pub fn get_status(&self) -> VRAMStatus {
        VRAMStatus {
            used_bytes: self.current_usage.load(Ordering::SeqCst),
            max_bytes: self.max_vram_bytes,
            loaded_models: self.loaded_models.blocking_read().len(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct VRAMStatus {
    pub used_bytes: u64,
    pub max_bytes: u64,
    pub loaded_models: usize,
}
```

#### 4.2 批处理优化

```rust
/// 批处理索引器
pub struct BatchIndexer {
    /// 批处理大小
    batch_size: usize,
    
    /// 待处理队列
    pending_queue: Arc<Mutex<VecDeque<IndexTask>>>,
    
    /// 处理中的任务
    processing: Arc<AtomicUsize>,
    
    /// 嵌入引擎
    embedding_engine: Arc<EmbeddingEngine>,
    
    /// 向量存储
    vector_store: Arc<VectorStore>,
}

#[derive(Debug)]
pub struct IndexTask {
    pub file_id: Uuid,
    pub path: PathBuf,
    pub priority: TaskPriority,
    pub created_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TaskPriority {
    Low = 0,      // 后台索引
    Normal = 1,   // 正常索引
    High = 2,     // 用户触发
    Urgent = 3,   // 搜索时发现未索引
}

impl BatchIndexer {
    /// 启动批处理循环
    pub async fn start(&self) {
        loop {
            // 收集一批任务
            let batch = self.collect_batch().await;
            
            if batch.is_empty() {
                // 无任务，等待
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            
            // 批量处理
            self.process_batch(batch).await;
        }
    }
    
    async fn collect_batch(&self) -> Vec<IndexTask> {
        let mut queue = self.pending_queue.lock().await;
        let mut batch = Vec::with_capacity(self.batch_size);
        
        // 按优先级排序
        let mut tasks: Vec<_> = queue.drain(..).collect();
        tasks.sort_by(|a, b| b.priority.cmp(&a.priority));
        
        // 取出一批
        for task in tasks {
            if batch.len() >= self.batch_size {
                queue.push_back(task);
            } else {
                batch.push(task);
            }
        }
        
        batch
    }
    
    async fn process_batch(&self, batch: Vec<IndexTask>) {
        self.processing.fetch_add(batch.len(), Ordering::SeqCst);
        
        // 并行读取文件内容
        let contents: Vec<_> = futures::future::join_all(
            batch.iter().map(|task| self.read_file_content(&task.path))
        ).await;
        
        // 批量生成嵌入
        let texts: Vec<_> = contents.iter()
            .filter_map(|c| c.as_ref().ok())
            .map(|c| c.as_str())
            .collect();
        
        let embeddings = self.embedding_engine
            .batch_embed_text(&texts)
            .await
            .unwrap_or_default();
        
        // 批量写入向量存储
        let points: Vec<_> = batch.iter()
            .zip(embeddings.iter())
            .map(|(task, embedding)| VectorPoint {
                id: task.file_id,
                vector: embedding.clone(),
                payload: HashMap::new(),
            })
            .collect();
        
        self.vector_store.upsert_batch(points).await.ok();
        
        self.processing.fetch_sub(batch.len(), Ordering::SeqCst);
    }
}
```


#### 4.3 稀释注意力实现

```rust
/// 稀释注意力处理器 - 用于处理超长文档
pub struct DilutedAttentionProcessor {
    /// 窗口大小
    window_size: usize,
    
    /// 稀释因子
    dilution_factor: usize,
    
    /// 最大序列长度
    max_seq_length: usize,
}

impl DilutedAttentionProcessor {
    /// 处理长文档
    pub async fn process_long_document(
        &self,
        content: &str,
        embedding_engine: &EmbeddingEngine,
    ) -> Result<Vec<ContentChunk>> {
        let tokens = self.tokenize(content);
        
        if tokens.len() <= self.max_seq_length {
            // 短文档，直接处理
            return self.process_short_document(content, embedding_engine).await;
        }
        
        // 长文档，使用滑动窗口 + 稀释采样
        let mut chunks = Vec::new();
        let mut position = 0;
        
        while position < tokens.len() {
            // 计算当前窗口
            let window_end = (position + self.window_size).min(tokens.len());
            let window_tokens = &tokens[position..window_end];
            
            // 稀释采样 (每隔 dilution_factor 取一个token用于全局上下文)
            let global_context: Vec<_> = tokens.iter()
                .enumerate()
                .filter(|(i, _)| i % self.dilution_factor == 0)
                .map(|(_, t)| t.clone())
                .collect();
            
            // 组合局部窗口和全局上下文
            let combined = self.combine_context(window_tokens, &global_context);
            
            // 生成嵌入
            let embedding = embedding_engine
                .embed_tokens(&combined)
                .await?;
            
            // 创建chunk
            chunks.push(ContentChunk {
                id: Uuid::now_v7(),
                file_id: Uuid::nil(), // 由调用者设置
                chunk_index: chunks.len() as u32,
                chunk_type: ChunkType::Paragraph,
                content: self.detokenize(window_tokens),
                location: ChunkLocation {
                    start_offset: position as u64,
                    end_offset: window_end as u64,
                    start_line: None,
                    end_line: None,
                    page_number: None,
                    bounding_box: None,
                },
                vector_id: 0, // 由向量存储分配
                created_at: Utc::now(),
            });
            
            // 滑动窗口
            position += self.window_size / 2; // 50% 重叠
        }
        
        Ok(chunks)
    }
    
    fn combine_context(&self, local: &[Token], global: &[Token]) -> Vec<Token> {
        // [CLS] + global_context + [SEP] + local_window + [SEP]
        let mut combined = vec![Token::CLS];
        combined.extend(global.iter().take(128).cloned()); // 限制全局上下文长度
        combined.push(Token::SEP);
        combined.extend(local.iter().cloned());
        combined.push(Token::SEP);
        combined
    }
}
```

### 5. 数据库Schema设计

#### 5.1 SQLite Schema (MetadataDB)

```sql
-- 文件记录表
CREATE TABLE files (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    filename TEXT NOT NULL,
    extension TEXT,
    file_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,
    indexed_at TEXT NOT NULL,
    last_accessed_at TEXT,
    index_status TEXT NOT NULL DEFAULT 'pending',
    privacy_level TEXT NOT NULL DEFAULT 'normal',
    is_excluded INTEGER NOT NULL DEFAULT 0,
    
    -- 索引
    INDEX idx_files_path ON files(path),
    INDEX idx_files_filename ON files(filename),
    INDEX idx_files_file_type ON files(file_type),
    INDEX idx_files_index_status ON files(index_status),
    INDEX idx_files_modified_at ON files(modified_at)
);

-- 内容片段表
CREATE TABLE content_chunks (
    id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    chunk_type TEXT NOT NULL,
    content TEXT NOT NULL,
    start_offset INTEGER NOT NULL,
    end_offset INTEGER NOT NULL,
    start_line INTEGER,
    end_line INTEGER,
    page_number INTEGER,
    bounding_box TEXT, -- JSON: [x, y, width, height]
    vector_id INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    
    INDEX idx_chunks_file_id ON content_chunks(file_id),
    INDEX idx_chunks_vector_id ON content_chunks(vector_id),
    UNIQUE(file_id, chunk_index)
);

-- 标签表
CREATE TABLE tags (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    display_name TEXT, -- JSON: {"zh": "工作", "en": "Work"}
    parent_id TEXT REFERENCES tags(id),
    tag_type TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#808080',
    icon TEXT,
    is_system INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    usage_count INTEGER NOT NULL DEFAULT 0,
    
    INDEX idx_tags_parent_id ON tags(parent_id),
    INDEX idx_tags_tag_type ON tags(tag_type)
);

-- 文件-标签关联表
CREATE TABLE file_tags (
    id TEXT PRIMARY KEY,
    file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    source TEXT NOT NULL,
    confidence REAL,
    is_confirmed INTEGER NOT NULL DEFAULT 0,
    is_rejected INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    user_action_at TEXT,
    
    UNIQUE(file_id, tag_id),
    INDEX idx_file_tags_file_id ON file_tags(file_id),
    INDEX idx_file_tags_tag_id ON file_tags(tag_id)
);

-- 文件关联表
CREATE TABLE file_relations (
    id TEXT PRIMARY KEY,
    source_file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    target_file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    relation_type TEXT NOT NULL,
    strength REAL NOT NULL,
    source TEXT NOT NULL,
    user_feedback TEXT NOT NULL DEFAULT 'none', -- JSON
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    user_action_at TEXT,
    
    UNIQUE(source_file_id, target_file_id, relation_type),
    INDEX idx_relations_source ON file_relations(source_file_id),
    INDEX idx_relations_target ON file_relations(target_file_id),
    INDEX idx_relations_type ON file_relations(relation_type)
);

-- 关联屏蔽规则表
CREATE TABLE relation_block_rules (
    id TEXT PRIMARY KEY,
    rule_type TEXT NOT NULL,
    rule_detail TEXT NOT NULL, -- JSON
    created_at TEXT NOT NULL,
    expires_at TEXT,
    is_active INTEGER NOT NULL DEFAULT 1,
    
    INDEX idx_block_rules_type ON relation_block_rules(rule_type),
    INDEX idx_block_rules_active ON relation_block_rules(is_active)
);

-- 云端使用记录表
CREATE TABLE cloud_usage (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp TEXT NOT NULL,
    tokens INTEGER NOT NULL,
    cost REAL NOT NULL,
    model TEXT,
    request_type TEXT,
    
    INDEX idx_cloud_usage_timestamp ON cloud_usage(timestamp)
);

-- 用户会话表 (用于逻辑链条)
CREATE TABLE sessions (
    id TEXT PRIMARY KEY,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    
    INDEX idx_sessions_started ON sessions(started_at)
);

-- 会话-文件访问记录
CREATE TABLE session_file_access (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id TEXT NOT NULL REFERENCES sessions(id),
    file_id TEXT NOT NULL REFERENCES files(id) ON DELETE CASCADE,
    accessed_at TEXT NOT NULL,
    access_type TEXT NOT NULL, -- 'open', 'preview', 'search_result'
    
    INDEX idx_session_access_session ON session_file_access(session_id),
    INDEX idx_session_access_file ON session_file_access(file_id)
);
```


## Data Models

### Qdrant 向量存储配置

```rust
/// Qdrant Collection 配置
pub struct QdrantConfig {
    /// Collection名称
    pub collection_name: String,
    
    /// 向量维度
    pub vector_size: u64,
    
    /// 距离度量
    pub distance: Distance,
    
    /// HNSW索引配置
    pub hnsw_config: HnswConfig,
    
    /// 优化器配置
    pub optimizer_config: OptimizerConfig,
}

impl Default for QdrantConfig {
    fn default() -> Self {
        Self {
            collection_name: "neuralfs_vectors".to_string(),
            vector_size: 384, // all-MiniLM-L6-v2
            distance: Distance::Cosine,
            hnsw_config: HnswConfig {
                m: 16,                    // 每个节点的连接数
                ef_construct: 100,        // 构建时的搜索宽度
                full_scan_threshold: 10000, // 小于此数量时全扫描
            },
            optimizer_config: OptimizerConfig {
                deleted_threshold: 0.2,   // 删除比例阈值
                vacuum_min_vector_number: 1000,
                default_segment_number: 4,
                max_segment_size: 200000,
                memmap_threshold: 50000,
                indexing_threshold: 20000,
            },
        }
    }
}

/// 向量点结构
#[derive(Debug, Clone)]
pub struct VectorPoint {
    /// 点ID (使用file_id或chunk_id的u64表示)
    pub id: u64,
    
    /// 向量数据
    pub vector: Vec<f32>,
    
    /// 负载数据 (用于过滤)
    pub payload: HashMap<String, Value>,
}

/// 负载字段定义
pub mod payload_fields {
    pub const FILE_ID: &str = "file_id";
    pub const CHUNK_ID: &str = "chunk_id";
    pub const FILE_TYPE: &str = "file_type";
    pub const TAG_IDS: &str = "tag_ids";
    pub const CREATED_AT: &str = "created_at";
    pub const MODIFIED_AT: &str = "modified_at";
    pub const PRIVACY_LEVEL: &str = "privacy_level";
}
```

## Error Handling

### 错误类型定义

```rust
/// NeuralFS 错误类型
#[derive(Debug, thiserror::Error)]
pub enum NeuralFSError {
    // 索引错误
    #[error("Index error: {0}")]
    Index(#[from] IndexError),
    
    // 搜索错误
    #[error("Search error: {0}")]
    Search(#[from] SearchError),
    
    // 嵌入错误
    #[error("Embedding error: {0}")]
    Embedding(#[from] EmbeddingError),
    
    // 云端错误
    #[error("Cloud error: {0}")]
    Cloud(#[from] CloudError),
    
    // 数据库错误
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    
    // 向量存储错误
    #[error("Vector store error: {0}")]
    VectorStore(#[from] qdrant_client::QdrantError),
    
    // IO错误
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    // 配置错误
    #[error("Config error: {0}")]
    Config(String),
}

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    
    #[error("Unsupported file type: {0}")]
    UnsupportedFileType(String),
    
    #[error("Content extraction failed: {0}")]
    ContentExtractionFailed(String),
    
    #[error("Index corrupted: {0}")]
    IndexCorrupted(String),
    
    #[error("Queue full")]
    QueueFull,
}

#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("Query too short")]
    QueryTooShort,
    
    #[error("Query embedding failed: {0}")]
    QueryEmbeddingFailed(String),
    
    #[error("Vector search failed: {0}")]
    VectorSearchFailed(String),
    
    #[error("Timeout after {0}ms")]
    Timeout(u64),
}

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("Model not loaded: {0}")]
    ModelNotLoaded(String),
    
    #[error("VRAM insufficient: need {needed}MB, available {available}MB")]
    VRAMInsufficient { needed: u64, available: u64 },
    
    #[error("Inference failed: {0}")]
    InferenceFailed(String),
    
    #[error("Tokenization failed: {0}")]
    TokenizationFailed(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    
    #[error("Cost limit reached")]
    CostLimitReached,
    
    #[error("Cloud disabled")]
    Disabled,
    
    #[error("Invalid API key")]
    InvalidApiKey,
}

/// 错误恢复策略
pub trait ErrorRecovery {
    /// 是否可重试
    fn is_retryable(&self) -> bool;
    
    /// 建议的重试延迟
    fn retry_delay(&self) -> Option<Duration>;
    
    /// 降级策略
    fn fallback_strategy(&self) -> FallbackStrategy;
}

#[derive(Debug, Clone)]
pub enum FallbackStrategy {
    /// 无降级，直接失败
    None,
    
    /// 使用本地模式
    LocalOnly,
    
    /// 使用缓存结果
    UseCache,
    
    /// 跳过当前项
    Skip,
    
    /// 使用默认值
    UseDefault(String),
}

impl ErrorRecovery for CloudError {
    fn is_retryable(&self) -> bool {
        matches!(self, 
            CloudError::Network(_) | 
            CloudError::RateLimitExceeded |
            CloudError::ApiError { status, .. } if *status >= 500
        )
    }
    
    /// 返回重试延迟（毫秒）
    /// 注意：对于 RateLimitExceeded，实际实现应优先从 API Response Header 
    /// 的 Retry-After 字段动态获取等待时间，此处 60000ms 仅为默认回退值
    fn retry_delay_ms(&self) -> Option<u64> {
        match self {
            // 默认 60 秒，但 CloudBridge 实现时应从 Retry-After header 动态覆盖
            CloudError::RateLimitExceeded => Some(60000),
            CloudError::Network(_) => Some(5000),
            CloudError::ApiError { status, .. } if *status >= 500 => {
                Some(10000)
            }
            _ => None,
        }
    }
    
    fn fallback_strategy(&self) -> FallbackStrategy {
        match self {
            CloudError::CostLimitReached | 
            CloudError::Disabled |
            CloudError::InvalidApiKey => FallbackStrategy::LocalOnly,
            _ => FallbackStrategy::UseCache,
        }
    }
}

/// CloudBridge 中的动态 Retry-After 处理
impl CloudBridge {
    /// 从 API 响应中提取 Retry-After 值
    fn extract_retry_after(response: &reqwest::Response) -> Option<u64> {
        response
            .headers()
            .get("Retry-After")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| {
                // Retry-After 可以是秒数或 HTTP-date
                s.parse::<u64>().ok().map(|secs| secs * 1000)
            })
    }
}
```


## Testing Strategy

### 测试框架选择

| 测试类型 | 框架 | 说明 |
|---------|------|------|
| 单元测试 | Rust内置 + mockall | 模块级测试 |
| 属性测试 | proptest | 验证不变量 |
| 集成测试 | Rust内置 | 组件间交互 |
| 端到端测试 | Tauri测试框架 | 完整流程 |
| 性能测试 | criterion | 基准测试 |

### 单元测试策略

- 每个模块独立测试
- 使用 mockall 模拟外部依赖
- 覆盖正常路径和边界情况
- 测试错误处理逻辑

### 属性测试策略

- 使用 proptest 进行属性测试
- 验证数据结构不变量
- 测试序列化/反序列化往返
- 验证搜索结果排序正确性



## Correctness Properties

*A property is a characteristic or behavior that should hold true across all valid executions of a system—essentially, a formal statement about what the system should do. Properties serve as the bridge between human-readable specifications and machine-verifiable correctness guarantees.*

### Property 1: File Detection Completeness
*For any* file created in a monitored directory, the File_Watcher SHALL detect and report the file within 1 second.
**Validates: Requirements 1.3, 8.2**

### Property 2: Index Queue Consistency
*For any* detected file, it SHALL appear in the Content_Indexer pending queue before being processed.
**Validates: Requirements 3.1, 1.4**

### Property 3: Intent Classification Validity
*For any* search query, the Intent_Parser SHALL return exactly one valid SearchIntent variant (FindFile, FindContent, or Ambiguous).
**Validates: Requirements 2.1**

### Property 4: Search Result Ordering
*For any* search response with multiple results, the results SHALL be sorted by score in descending order.
**Validates: Requirements 2.2, 2.3**

### Property 5: Chunk Coverage Invariant
*For any* indexed document, the union of all chunk locations SHALL cover the entire document content without gaps, and chunk boundaries SHALL not overlap incorrectly.
**Validates: Requirements 3.2**

### Property 6: VRAM Usage Bound
*For any* sequence of embedding operations, the peak VRAM usage SHALL not exceed the configured limit (default 4GB).
**Validates: Requirements 4.1**

### Property 7: Search Latency Bound (Fast Mode)
*For any* search query in fast inference mode, the initial results SHALL be returned within 200ms.
**Validates: Requirements 4.8**

### Property 8: Tag Assignment Completeness
*For any* successfully indexed file, the Tag_Manager SHALL assign at least one tag.
**Validates: Requirements 5.1**

### Property 9: Tag Hierarchy Depth Bound
*For any* tag in the system, the path from root to that tag SHALL have at most 3 levels (enabling 2-3 click navigation).
**Validates: Requirements 5.7**

### Property 10: Relation Symmetry
*For any* file relation of type ContentSimilar, if file A is related to file B, then file B SHALL also be related to file A with the same strength.
**Validates: Requirements 6.1**

### Property 11: Parallel Inference Dispatch
*For any* search query with cloud enabled, both local and cloud inference paths SHALL be invoked simultaneously.
**Validates: Requirements 11.1**

### Property 12: Cache Hit Consistency
*For any* repeated search query within cache TTL, the second invocation SHALL return cached results without making a new cloud API call.
**Validates: Requirements 11.8**

### Property 13: Data Anonymization
*For any* cloud API request payload, the content SHALL NOT contain user home directory paths, usernames, or other PII patterns.
**Validates: Requirements 13.2**

### Property 14: User Feedback State Machine
*For any* FileRelation, the user_feedback field SHALL only transition through valid states: None → {Confirmed | Rejected | Adjusted}, and Rejected/Adjusted states SHALL be terminal (no further AI modifications).
**Validates: Requirements 6 (Human-in-the-Loop)**

### Property 15: Block Rule Enforcement
*For any* active RelationBlockRule, the Logic_Chain_Engine SHALL NOT generate new AI relations that match the blocked pattern.
**Validates: Requirements 6 (Human-in-the-Loop)**

### Property 16: Rejection Learning Effect
*For any* tag rejection by user, subsequent AI tag suggestions for similar files SHALL have lower confidence for that tag.
**Validates: Requirements 5.5, 10.4**

### Property 17: Vector Database Serialization Round-Trip
*For any* VectorPoint, serializing to storage and deserializing back SHALL produce an equivalent VectorPoint.
**Validates: Requirements 21**

### Property 18: FileRecord Serialization Round-Trip
*For any* FileRecord, serializing to JSON/bincode and deserializing back SHALL produce an equivalent FileRecord.
**Validates: Requirements 21, 22**

### Property 19: Search Filter Correctness
*For any* search with filters applied, all returned results SHALL satisfy all filter conditions (file type, tags, time range, privacy level).
**Validates: Requirements 2.2, 2.3**

### Property 20: Batch Processing Atomicity
*For any* batch of files being indexed, either all files in the batch are successfully indexed, or the batch is rolled back with no partial state.
**Validates: Requirements 3, 18**


## OS Integration Layer (操作系统深度集成)

### Windows 桌面替代策略

```rust
/// Windows 桌面集成管理器
pub struct WindowsDesktopManager {
    /// 原始桌面句柄
    original_desktop_hwnd: HWND,
    
    /// NeuralFS 主窗口句柄
    main_hwnd: HWND,
    
    /// 是否已接管桌面
    is_shell_replaced: bool,
    
    /// 系统托盘句柄
    tray_hwnd: Option<HWND>,
}

impl WindowsDesktopManager {
    /// 接管桌面 - 将窗口挂载到 WorkerW 之后
    pub fn take_over_desktop(&mut self) -> Result<()> {
        unsafe {
            // 1. 找到 Program Manager 窗口
            let progman = FindWindowW(w!("Progman"), None);
            if progman.is_invalid() {
                return Err(DesktopError::ProgmanNotFound);
            }
            
            // 2. 发送消息创建 WorkerW
            SendMessageTimeoutW(
                progman,
                0x052C, // 创建 WorkerW 的未公开消息
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                None,
            );
            
            // 3. 枚举找到 WorkerW
            let mut worker_w = HWND::default();
            EnumWindows(
                Some(Self::enum_windows_callback),
                LPARAM(&mut worker_w as *mut _ as isize),
            );
            
            if worker_w.is_invalid() {
                return Err(DesktopError::WorkerWNotFound);
            }
            
            // 4. 将 NeuralFS 窗口设为 WorkerW 的子窗口
            SetParent(self.main_hwnd, worker_w);
            
            // 5. 设置窗口样式
            let style = GetWindowLongW(self.main_hwnd, GWL_STYLE);
            SetWindowLongW(
                self.main_hwnd,
                GWL_STYLE,
                style & !WS_POPUP.0 as i32 | WS_CHILD.0 as i32,
            );
            
            // 6. 调整窗口大小覆盖整个桌面
            let (width, height) = self.get_desktop_size();
            SetWindowPos(
                self.main_hwnd,
                HWND_TOP,
                0, 0, width, height,
                SWP_SHOWWINDOW,
            );
            
            self.is_shell_replaced = true;
            Ok(())
        }
    }
    
    /// 处理 Win+D 快捷键
    pub fn register_hotkey_hooks(&self) -> Result<()> {
        // 使用低级键盘钩子拦截 Win+D
        unsafe {
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(Self::keyboard_hook_proc),
                None,
                0,
            )?;
            
            // 存储钩子句柄
            KEYBOARD_HOOK.store(hook.0 as usize, Ordering::SeqCst);
        }
        Ok(())
    }
    
    extern "system" fn keyboard_hook_proc(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code >= 0 {
            let kb = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            
            // 检测 Win+D
            if kb.vkCode == VK_D.0 as u32 {
                let win_pressed = unsafe {
                    GetAsyncKeyState(VK_LWIN.0 as i32) < 0 ||
                    GetAsyncKeyState(VK_RWIN.0 as i32) < 0
                };
                
                if win_pressed {
                    // 拦截 Win+D，改为切换 NeuralFS 视图
                    // 发送自定义消息到主窗口
                    return LRESULT(1); // 阻止默认行为
                }
            }
        }
        
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
    
    /// 隐藏系统任务栏
    pub fn hide_taskbar(&mut self) -> Result<()> {
        unsafe {
            let taskbar = FindWindowW(w!("Shell_TrayWnd"), None);
            if !taskbar.is_invalid() {
                ShowWindow(taskbar, SW_HIDE);
                self.tray_hwnd = Some(taskbar);
            }
        }
        Ok(())
    }
    
    /// 恢复系统任务栏
    pub fn restore_taskbar(&self) {
        if let Some(taskbar) = self.tray_hwnd {
            unsafe {
                ShowWindow(taskbar, SW_SHOW);
            }
        }
    }
    
    /// 多显示器支持
    pub fn setup_multi_monitor(&self) -> Result<Vec<MonitorInfo>> {
        let mut monitors = Vec::new();
        
        unsafe {
            EnumDisplayMonitors(
                None,
                None,
                Some(Self::monitor_enum_callback),
                LPARAM(&mut monitors as *mut _ as isize),
            );
        }
        
        Ok(monitors)
    }
}

#[derive(Debug, Clone)]
pub struct MonitorInfo {
    pub handle: HMONITOR,
    pub rect: RECT,
    pub is_primary: bool,
    pub dpi_scale: f32,
}

/// 多显示器渲染策略
#[derive(Debug, Clone, Copy)]
pub enum MultiMonitorStrategy {
    /// 主显示器运行 NeuralFS，其他显示器保持原样
    PrimaryOnly,
    
    /// 所有显示器统一渲染 (跨屏)
    Unified,
    
    /// 每个显示器独立实例
    Independent,
}
```

### 系统缩略图提取

```rust
/// 系统缩略图提取器
pub struct ThumbnailExtractor {
    #[cfg(windows)]
    shell_item_factory: IShellItemImageFactory,
}

impl ThumbnailExtractor {
    /// 获取文件缩略图
    pub async fn get_thumbnail(
        &self,
        path: &Path,
        size: ThumbnailSize,
    ) -> Result<ThumbnailData> {
        #[cfg(windows)]
        {
            self.get_windows_thumbnail(path, size).await
        }
        
        #[cfg(target_os = "macos")]
        {
            self.get_macos_thumbnail(path, size).await
        }
        
        #[cfg(target_os = "linux")]
        {
            self.get_linux_thumbnail(path, size).await
        }
    }
    
    #[cfg(windows)]
    async fn get_windows_thumbnail(
        &self,
        path: &Path,
        size: ThumbnailSize,
    ) -> Result<ThumbnailData> {
        use windows::Win32::UI::Shell::*;
        
        let path_wide: Vec<u16> = path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        
        unsafe {
            // 创建 ShellItem
            let shell_item: IShellItem = SHCreateItemFromParsingName(
                PCWSTR(path_wide.as_ptr()),
                None,
            )?;
            
            // 获取 IShellItemImageFactory
            let factory: IShellItemImageFactory = shell_item.cast()?;
            
            // 获取缩略图
            let (width, height) = size.dimensions();
            let hbitmap = factory.GetImage(
                SIZE { cx: width, cy: height },
                SIIGBF_THUMBNAILONLY | SIIGBF_BIGGERSIZEOK,
            )?;
            
            // 转换为 PNG 数据
            self.hbitmap_to_png(hbitmap)
        }
    }
    
    #[cfg(target_os = "macos")]
    async fn get_macos_thumbnail(
        &self,
        path: &Path,
        size: ThumbnailSize,
    ) -> Result<ThumbnailData> {
        use objc2::*;
        use objc2_foundation::*;
        use objc2_quartz_core::*;
        
        // 使用 QLThumbnailGenerator
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str().unwrap()));
        let request = QLThumbnailGenerationRequest::initWithFileAtURL_size_scale_representationTypes(
            &url,
            CGSize { width: size.width() as f64, height: size.height() as f64 },
            1.0,
            QLThumbnailGenerationRequestRepresentationTypeThumbnail,
        );
        
        let generator = QLThumbnailGenerator::sharedGenerator();
        let thumbnail = generator.generateBestRepresentationForRequest_completionHandler(
            &request,
            |representation, error| {
                // 处理结果
            }
        );
        
        // 转换为 PNG
        todo!()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ThumbnailSize {
    Small,   // 48x48
    Medium,  // 96x96
    Large,   // 256x256
    XLarge,  // 512x512
}

impl ThumbnailSize {
    pub fn dimensions(&self) -> (i32, i32) {
        match self {
            Self::Small => (48, 48),
            Self::Medium => (96, 96),
            Self::Large => (256, 256),
            Self::XLarge => (512, 512),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ThumbnailData {
    pub data: Vec<u8>,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
}
```


## Reconciliation Strategy (文件系统对账服务)

### 启动时文件系统 Diff

```rust
/// 文件系统对账服务
pub struct ReconciliationService {
    metadata_db: SqlitePool,
    file_watcher: Arc<FileWatcher>,
}

impl ReconciliationService {
    /// 启动时执行对账
    pub async fn reconcile_on_startup(&self, monitored_paths: &[PathBuf]) -> ReconcileResult {
        let mut result = ReconcileResult::default();
        
        // 1. 从数据库加载所有已知文件
        let db_files = self.load_db_files().await?;
        let db_file_map: HashMap<PathBuf, FileRecord> = db_files
            .into_iter()
            .map(|f| (f.path.clone(), f))
            .collect();
        
        // 2. 扫描文件系统
        let fs_files = self.scan_filesystem(monitored_paths).await?;
        let fs_file_map: HashMap<PathBuf, FsFileInfo> = fs_files
            .into_iter()
            .map(|f| (f.path.clone(), f))
            .collect();
        
        // 3. 计算差异
        
        // 3.1 新增文件 (在FS中存在，DB中不存在)
        for (path, fs_info) in &fs_file_map {
            if !db_file_map.contains_key(path) {
                // 检查是否是重命名 (通过 FileID)
                if let Some(old_record) = self.find_by_file_id(&fs_info.file_id, &db_file_map) {
                    // 这是重命名，更新路径
                    result.renamed.push(RenameEvent {
                        old_path: old_record.path.clone(),
                        new_path: path.clone(),
                        file_id: fs_info.file_id,
                    });
                } else {
                    // 真正的新文件
                    result.added.push(path.clone());
                }
            }
        }
        
        // 3.2 删除文件 (在DB中存在，FS中不存在)
        for (path, db_record) in &db_file_map {
            if !fs_file_map.contains_key(path) {
                // 检查是否是重命名 (已在上面处理)
                let is_renamed = result.renamed.iter()
                    .any(|r| r.old_path == *path);
                
                if !is_renamed {
                    result.deleted.push(path.clone());
                }
            }
        }
        
        // 3.3 修改文件 (两边都存在，但内容变化)
        for (path, fs_info) in &fs_file_map {
            if let Some(db_record) = db_file_map.get(path) {
                if fs_info.modified_at > db_record.modified_at ||
                   fs_info.size_bytes != db_record.size_bytes {
                    result.modified.push(path.clone());
                }
            }
        }
        
        // 4. 应用变更
        self.apply_reconcile_result(&result).await?;
        
        Ok(result)
    }
    
    /// 通过 FileID 查找文件 (用于检测重命名)
    fn find_by_file_id(
        &self,
        file_id: &FileId,
        db_files: &HashMap<PathBuf, FileRecord>,
    ) -> Option<&FileRecord> {
        db_files.values().find(|r| r.file_id == Some(*file_id))
    }
    
    /// 获取文件的系统级 FileID
    #[cfg(windows)]
    fn get_file_id(path: &Path) -> Result<FileId> {
        use windows::Win32::Storage::FileSystem::*;
        
        unsafe {
            let handle = CreateFileW(
                &path.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>(),
                FILE_READ_ATTRIBUTES,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS,
                None,
            )?;
            
            let mut info = BY_HANDLE_FILE_INFORMATION::default();
            GetFileInformationByHandle(handle, &mut info)?;
            CloseHandle(handle);
            
            Ok(FileId {
                volume_serial: info.dwVolumeSerialNumber,
                file_index_high: info.nFileIndexHigh,
                file_index_low: info.nFileIndexLow,
            })
        }
    }
    
    #[cfg(unix)]
    fn get_file_id(path: &Path) -> Result<FileId> {
        use std::os::unix::fs::MetadataExt;
        
        let metadata = std::fs::metadata(path)?;
        Ok(FileId {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct FileId {
    #[cfg(windows)]
    pub volume_serial: u32,
    #[cfg(windows)]
    pub file_index_high: u32,
    #[cfg(windows)]
    pub file_index_low: u32,
    
    #[cfg(unix)]
    pub device: u64,
    #[cfg(unix)]
    pub inode: u64,
}

#[derive(Debug, Default)]
pub struct ReconcileResult {
    pub added: Vec<PathBuf>,
    pub deleted: Vec<PathBuf>,
    pub modified: Vec<PathBuf>,
    pub renamed: Vec<RenameEvent>,
    pub errors: Vec<(PathBuf, String)>,
}

#[derive(Debug, Clone)]
pub struct RenameEvent {
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub file_id: FileId,
}

#[derive(Debug, Clone)]
pub struct FsFileInfo {
    pub path: PathBuf,
    pub file_id: FileId,
    pub size_bytes: u64,
    pub modified_at: DateTime<Utc>,
}
```

### 增量 Diff 算法

```rust
/// 增量对账配置
pub struct ReconcileConfig {
    /// 最大并行扫描数
    pub max_parallel_scans: usize,
    
    /// 扫描批次大小
    pub batch_size: usize,
    
    /// 是否使用快速模式 (仅检查 mtime 和 size)
    pub fast_mode: bool,
    
    /// 是否验证内容哈希
    pub verify_hash: bool,
}

impl Default for ReconcileConfig {
    fn default() -> Self {
        Self {
            max_parallel_scans: 4,
            batch_size: 1000,
            fast_mode: true,
            verify_hash: false,
        }
    }
}
```


## Hybrid Search Logic (混合搜索策略)

### 向量搜索 + 全文检索

```rust
/// 混合搜索引擎
pub struct HybridSearchEngine {
    /// 向量搜索 (语义)
    vector_store: Arc<VectorStore>,
    
    /// 全文检索 (关键词)
    text_index: Arc<TextIndex>,
    
    /// 元数据搜索
    metadata_db: SqlitePool,
    
    /// 搜索配置
    config: HybridSearchConfig,
}

#[derive(Debug, Clone)]
pub struct HybridSearchConfig {
    /// 向量搜索权重
    pub vector_weight: f32,
    
    /// BM25 搜索权重
    pub bm25_weight: f32,
    
    /// 精确匹配加分
    pub exact_match_boost: f32,
    
    /// 文件名匹配加分
    pub filename_match_boost: f32,
    
    /// 最小向量分数阈值
    pub min_vector_score: f32,
    
    /// 最小 BM25 分数阈值
    pub min_bm25_score: f32,
}

impl Default for HybridSearchConfig {
    fn default() -> Self {
        Self {
            vector_weight: 0.6,
            bm25_weight: 0.4,
            exact_match_boost: 2.0,
            filename_match_boost: 1.5,
            min_vector_score: 0.3,
            min_bm25_score: 0.1,
        }
    }
}

impl HybridSearchEngine {
    /// 执行混合搜索
    pub async fn search(&self, request: SearchRequest) -> Result<SearchResponse> {
        let query = &request.query;
        
        // 1. 判断查询类型
        let query_type = self.classify_query(query);
        
        // 2. 根据查询类型调整权重
        let weights = match query_type {
            QueryType::ExactKeyword => {
                // 精确关键词 (如错误码、文件名)，提高 BM25 权重
                (0.2, 0.8)
            }
            QueryType::NaturalLanguage => {
                // 自然语言描述，提高向量权重
                (0.8, 0.2)
            }
            QueryType::Mixed => {
                // 混合查询，使用默认权重
                (self.config.vector_weight, self.config.bm25_weight)
            }
        };
        
        // 3. 并行执行向量搜索和全文检索
        let (vector_results, bm25_results) = tokio::join!(
            self.vector_search(query, &request.filters),
            self.bm25_search(query, &request.filters),
        );
        
        // 4. 合并结果
        let merged = self.merge_results(
            vector_results?,
            bm25_results?,
            weights,
        );
        
        // 5. 应用精确匹配加分
        let boosted = self.apply_exact_match_boost(&merged, query);
        
        // 6. 排序并返回
        Ok(SearchResponse {
            request_id: request.request_id,
            status: SearchStatus::Success,
            results: boosted,
            total_count: merged.len() as u64,
            has_more: false,
            duration_ms: 0,
            sources: vec![ResultSource::LocalVector, ResultSource::LocalTag],
            clarifications: None,
        })
    }
    
    /// 分类查询类型
    fn classify_query(&self, query: &str) -> QueryType {
        // 检测精确关键词模式
        let exact_patterns = [
            r"0x[0-9a-fA-F]+",           // 十六进制错误码
            r"\d{4,}",                    // 长数字序列
            r"[A-Z_]{3,}",                // 全大写常量
            r"\w+\.\w{2,4}",              // 文件名模式
            r#""[^"]+""#,                 // 引号包围的精确搜索
        ];
        
        for pattern in &exact_patterns {
            if regex::Regex::new(pattern).unwrap().is_match(query) {
                return QueryType::ExactKeyword;
            }
        }
        
        // 检测自然语言模式
        let words: Vec<&str> = query.split_whitespace().collect();
        if words.len() >= 3 {
            return QueryType::NaturalLanguage;
        }
        
        QueryType::Mixed
    }
    
    /// 向量搜索
    async fn vector_search(
        &self,
        query: &str,
        filters: &SearchFilters,
    ) -> Result<Vec<ScoredResult>> {
        // 生成查询向量
        let query_embedding = self.embedding_engine
            .embed_text_content(query)
            .await?;
        
        // 构建过滤条件
        let qdrant_filter = self.build_qdrant_filter(filters);
        
        // 执行向量搜索
        let results = self.vector_store
            .search(&query_embedding, 100, Some(qdrant_filter))
            .await?;
        
        Ok(results.into_iter()
            .map(|r| ScoredResult {
                file_id: r.file_id,
                chunk_id: r.chunk_id,
                score: r.score,
                source: SearchSource::Vector,
            })
            .collect())
    }
    
    /// BM25 全文检索
    async fn bm25_search(
        &self,
        query: &str,
        filters: &SearchFilters,
    ) -> Result<Vec<ScoredResult>> {
        // 使用 Tantivy 或 SQLite FTS5
        let results = self.text_index.search(query, filters).await?;
        
        Ok(results.into_iter()
            .map(|r| ScoredResult {
                file_id: r.file_id,
                chunk_id: r.chunk_id,
                score: r.score,
                source: SearchSource::BM25,
            })
            .collect())
    }
    
    /// 合并搜索结果
    fn merge_results(
        &self,
        vector_results: Vec<ScoredResult>,
        bm25_results: Vec<ScoredResult>,
        weights: (f32, f32),
    ) -> Vec<MergedResult> {
        let mut merged_map: HashMap<Uuid, MergedResult> = HashMap::new();
        
        // 添加向量结果
        for result in vector_results {
            merged_map.entry(result.file_id)
                .or_insert_with(|| MergedResult {
                    file_id: result.file_id,
                    chunk_id: result.chunk_id,
                    vector_score: 0.0,
                    bm25_score: 0.0,
                    final_score: 0.0,
                })
                .vector_score = result.score;
        }
        
        // 添加 BM25 结果
        for result in bm25_results {
            merged_map.entry(result.file_id)
                .or_insert_with(|| MergedResult {
                    file_id: result.file_id,
                    chunk_id: result.chunk_id,
                    vector_score: 0.0,
                    bm25_score: 0.0,
                    final_score: 0.0,
                })
                .bm25_score = result.score;
        }
        
        // 计算最终分数
        let (vector_weight, bm25_weight) = weights;
        for result in merged_map.values_mut() {
            result.final_score = 
                result.vector_score * vector_weight +
                result.bm25_score * bm25_weight;
        }
        
        // 排序
        let mut results: Vec<_> = merged_map.into_values().collect();
        results.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap());
        
        results
    }
}

#[derive(Debug, Clone, Copy)]
pub enum QueryType {
    ExactKeyword,
    NaturalLanguage,
    Mixed,
}

/// Tantivy 全文索引
pub struct TextIndex {
    index: tantivy::Index,
    reader: tantivy::IndexReader,
}

impl TextIndex {
    pub fn new(index_path: &Path) -> Result<Self> {
        use tantivy::schema::*;
        
        // 定义 Schema
        let mut schema_builder = Schema::builder();
        
        schema_builder.add_text_field("file_id", STRING | STORED);
        schema_builder.add_text_field("chunk_id", STRING | STORED);
        schema_builder.add_text_field("filename", TEXT | STORED);
        schema_builder.add_text_field("content", TEXT);
        schema_builder.add_text_field("tags", TEXT);
        schema_builder.add_u64_field("modified_at", INDEXED | STORED);
        
        let schema = schema_builder.build();
        
        // 创建或打开索引
        let index = if index_path.exists() {
            tantivy::Index::open_in_dir(index_path)?
        } else {
            std::fs::create_dir_all(index_path)?;
            tantivy::Index::create_in_dir(index_path, schema)?
        };
        
        let reader = index.reader()?;
        
        Ok(Self { index, reader })
    }
    
    pub async fn search(
        &self,
        query: &str,
        filters: &SearchFilters,
    ) -> Result<Vec<ScoredResult>> {
        use tantivy::query::*;
        use tantivy::collector::TopDocs;
        
        let searcher = self.reader.searcher();
        let schema = self.index.schema();
        
        // 构建查询
        let content_field = schema.get_field("content").unwrap();
        let filename_field = schema.get_field("filename").unwrap();
        
        let query_parser = QueryParser::for_index(
            &self.index,
            vec![content_field, filename_field],
        );
        
        let parsed_query = query_parser.parse_query(query)?;
        
        // 执行搜索
        let top_docs = searcher.search(&parsed_query, &TopDocs::with_limit(100))?;
        
        // 转换结果
        let results = top_docs.into_iter()
            .map(|(score, doc_address)| {
                let doc = searcher.doc(doc_address).unwrap();
                let file_id = doc.get_first(schema.get_field("file_id").unwrap())
                    .unwrap()
                    .as_text()
                    .unwrap();
                
                ScoredResult {
                    file_id: Uuid::parse_str(file_id).unwrap(),
                    chunk_id: None,
                    score,
                    source: SearchSource::BM25,
                }
            })
            .collect();
        
        Ok(results)
    }
}
```


## Installer Specification (安装与分发策略)

### 微内核 + 动态下载模型

```rust
/// 模型下载管理器
pub struct ModelDownloader {
    /// 下载源配置
    sources: Vec<ModelSource>,
    
    /// 本地模型目录
    models_dir: PathBuf,
    
    /// 下载进度回调
    progress_callback: Option<Box<dyn Fn(DownloadProgress) + Send + Sync>>,
    
    /// HTTP 客户端
    client: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct ModelSource {
    /// 源名称
    pub name: String,
    
    /// 基础 URL
    pub base_url: String,
    
    /// 优先级 (越小越优先)
    pub priority: u32,
    
    /// 是否可用
    pub available: bool,
}

impl Default for ModelDownloader {
    fn default() -> Self {
        Self {
            sources: vec![
                // 国内镜像优先
                ModelSource {
                    name: "HuggingFace Mirror (China)".to_string(),
                    base_url: "https://hf-mirror.com".to_string(),
                    priority: 1,
                    available: true,
                },
                // 自建 CDN
                ModelSource {
                    name: "NeuralFS CDN".to_string(),
                    base_url: "https://models.neuralfs.io".to_string(),
                    priority: 2,
                    available: true,
                },
                // 官方源
                ModelSource {
                    name: "HuggingFace".to_string(),
                    base_url: "https://huggingface.co".to_string(),
                    priority: 3,
                    available: true,
                },
            ],
            models_dir: dirs::data_local_dir()
                .unwrap_or_default()
                .join("NeuralFS")
                .join("models"),
            progress_callback: None,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .unwrap(),
        }
    }
}

/// 模型清单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelManifest {
    pub models: Vec<ModelInfo>,
    pub version: String,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// 模型ID
    pub id: String,
    
    /// 显示名称
    pub name: String,
    
    /// 模型类型
    pub model_type: ModelType,
    
    /// 文件名
    pub filename: String,
    
    /// 文件大小 (字节)
    pub size_bytes: u64,
    
    /// SHA256 校验和
    pub sha256: String,
    
    /// 是否必需
    pub required: bool,
    
    /// 描述
    pub description: String,
    
    /// VRAM 需求 (MB)
    pub vram_mb: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum ModelType {
    TextEmbedding,    // 文本嵌入 (all-MiniLM-L6-v2)
    ImageEmbedding,   // 图像嵌入 (CLIP)
    IntentParser,     // 意图解析
    Tokenizer,        // 分词器
}

impl ModelDownloader {
    /// 检查并下载缺失的模型
    pub async fn ensure_models(&self) -> Result<()> {
        let manifest = self.fetch_manifest().await?;
        
        for model in &manifest.models {
            if model.required && !self.is_model_present(model) {
                self.download_model(model).await?;
            }
        }
        
        Ok(())
    }
    
    /// 下载单个模型
    pub async fn download_model(&self, model: &ModelInfo) -> Result<PathBuf> {
        let target_path = self.models_dir.join(&model.filename);
        
        // 尝试每个源
        for source in &self.sources {
            if !source.available {
                continue;
            }
            
            let url = format!("{}/{}", source.base_url, model.filename);
            
            match self.download_file(&url, &target_path, model.size_bytes).await {
                Ok(_) => {
                    // 验证校验和
                    if self.verify_checksum(&target_path, &model.sha256).await? {
                        return Ok(target_path);
                    } else {
                        // 校验失败，删除并尝试下一个源
                        tokio::fs::remove_file(&target_path).await.ok();
                    }
                }
                Err(e) => {
                    tracing::warn!("Download from {} failed: {}", source.name, e);
                    continue;
                }
            }
        }
        
        Err(ModelError::AllSourcesFailed)
    }
    
    /// 下载文件 (支持断点续传)
    async fn download_file(
        &self,
        url: &str,
        target: &Path,
        total_size: u64,
    ) -> Result<()> {
        let mut downloaded = 0u64;
        
        // 检查是否有部分下载
        let temp_path = target.with_extension("part");
        if temp_path.exists() {
            downloaded = tokio::fs::metadata(&temp_path).await?.len();
        }
        
        // 构建请求 (支持 Range)
        let mut request = self.client.get(url);
        if downloaded > 0 {
            request = request.header("Range", format!("bytes={}-", downloaded));
        }
        
        let response = request.send().await?;
        
        // 打开文件 (追加模式)
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&temp_path)
            .await?;
        
        // 流式下载
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            file.write_all(&chunk).await?;
            downloaded += chunk.len() as u64;
            
            // 报告进度
            if let Some(callback) = &self.progress_callback {
                callback(DownloadProgress {
                    downloaded,
                    total: total_size,
                    percentage: (downloaded as f64 / total_size as f64 * 100.0) as u8,
                });
            }
        }
        
        // 重命名为最终文件
        tokio::fs::rename(&temp_path, target).await?;
        
        Ok(())
    }
    
    /// 验证文件校验和
    async fn verify_checksum(&self, path: &Path, expected: &str) -> Result<bool> {
        use sha2::{Sha256, Digest};
        
        let mut file = tokio::fs::File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer
        
        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        
        let result = format!("{:x}", hasher.finalize());
        Ok(result == expected)
    }
}

#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub percentage: u8,
}
```

### DLL Side-loading 策略

```rust
/// 运行时依赖管理
pub struct RuntimeDependencies {
    /// 应用目录
    app_dir: PathBuf,
    
    /// 依赖目录
    deps_dir: PathBuf,
}

impl RuntimeDependencies {
    /// 初始化运行时依赖
    pub fn initialize() -> Result<Self> {
        let app_dir = std::env::current_exe()?
            .parent()
            .unwrap()
            .to_path_buf();
        
        let deps_dir = app_dir.join("deps");
        
        // 设置 DLL 搜索路径
        #[cfg(windows)]
        {
            use windows::Win32::System::LibraryLoader::*;
            
            unsafe {
                // 添加 deps 目录到 DLL 搜索路径
                let deps_wide: Vec<u16> = deps_dir.as_os_str()
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                
                AddDllDirectory(PCWSTR(deps_wide.as_ptr()));
                
                // 设置默认 DLL 搜索顺序
                SetDefaultDllDirectories(
                    LOAD_LIBRARY_SEARCH_APPLICATION_DIR |
                    LOAD_LIBRARY_SEARCH_USER_DIRS |
                    LOAD_LIBRARY_SEARCH_SYSTEM32
                );
            }
        }
        
        Ok(Self { app_dir, deps_dir })
    }
    
    /// 检查 CUDA 可用性
    pub fn check_cuda(&self) -> CudaStatus {
        #[cfg(windows)]
        {
            // 检查 cudart64_*.dll
            let cuda_dll = self.deps_dir.join("cudart64_12.dll");
            if cuda_dll.exists() {
                // 尝试加载
                unsafe {
                    let result = windows::Win32::System::LibraryLoader::LoadLibraryW(
                        &cuda_dll.to_string_lossy().encode_utf16().chain(std::iter::once(0)).collect::<Vec<_>>()
                    );
                    
                    if result.is_ok() {
                        return CudaStatus::Available {
                            version: "12.x".to_string(),
                            source: CudaSource::Bundled,
                        };
                    }
                }
            }
            
            // 检查系统 CUDA
            if let Ok(cuda_path) = std::env::var("CUDA_PATH") {
                return CudaStatus::Available {
                    version: "system".to_string(),
                    source: CudaSource::System,
                };
            }
            
            CudaStatus::NotAvailable
        }
        
        #[cfg(not(windows))]
        {
            // Linux/Mac 检查
            CudaStatus::NotAvailable
        }
    }
}

#[derive(Debug, Clone)]
pub enum CudaStatus {
    Available {
        version: String,
        source: CudaSource,
    },
    NotAvailable,
}

#[derive(Debug, Clone, Copy)]
pub enum CudaSource {
    Bundled,  // 应用自带
    System,   // 系统安装
}
```

### 安装包结构

```
NeuralFS-Setup.exe (约 80MB)
├── neuralfs.exe           # 主程序 (~50MB)
├── deps/
│   ├── onnxruntime.dll    # ONNX Runtime (~20MB)
│   ├── cudart64_12.dll    # CUDA Runtime (可选, ~5MB)
│   └── ...
├── resources/
│   ├── icons/
│   └── locales/
└── models/                # 首次启动时下载
    ├── all-MiniLM-L6-v2.onnx    (~90MB)
    ├── clip-vit-b-32.onnx       (~350MB)
    └── tokenizer.json           (~2MB)
```


## UI/UX Design Considerations (标签显示策略)

### 标签可视化区分

```rust
/// 标签显示状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagDisplayInfo {
    pub tag: Tag,
    pub relation: FileTagRelation,
    pub display_style: TagDisplayStyle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum TagDisplayStyle {
    /// 已确认标签 - 实心背景，完全不透明
    Verified {
        background_opacity: f32,  // 1.0
        border_style: BorderStyle,
    },
    
    /// AI 建议标签 - 虚线边框，半透明
    Suggested {
        background_opacity: f32,  // 0.3
        border_style: BorderStyle,
        confidence_indicator: bool,
    },
    
    /// 继承标签 - 浅色背景
    Inherited {
        background_opacity: f32,  // 0.5
        border_style: BorderStyle,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum BorderStyle {
    Solid,
    Dashed,
    Dotted,
    None,
}

impl TagDisplayInfo {
    /// 根据标签关系计算显示样式
    pub fn from_relation(tag: Tag, relation: FileTagRelation) -> Self {
        let display_style = match (relation.source, relation.is_confirmed) {
            // 用户手动添加或已确认
            (TagSource::Manual, _) | (_, true) => TagDisplayStyle::Verified {
                background_opacity: 1.0,
                border_style: BorderStyle::Solid,
            },
            
            // AI 生成但未确认
            (TagSource::AIGenerated, false) => TagDisplayStyle::Suggested {
                background_opacity: 0.3,
                border_style: BorderStyle::Dashed,
                confidence_indicator: relation.confidence.unwrap_or(0.0) < 0.8,
            },
            
            // 继承标签
            (TagSource::Inherited, false) => TagDisplayStyle::Inherited {
                background_opacity: 0.5,
                border_style: BorderStyle::Dotted,
            },
            
            // 导入标签
            (TagSource::Imported, false) => TagDisplayStyle::Verified {
                background_opacity: 0.8,
                border_style: BorderStyle::Solid,
            },
        };
        
        Self {
            tag,
            relation,
            display_style,
        }
    }
    
    /// 生成 CSS 类名
    pub fn css_class(&self) -> String {
        match self.display_style {
            TagDisplayStyle::Verified { .. } => "tag-verified".to_string(),
            TagDisplayStyle::Suggested { confidence_indicator, .. } => {
                if confidence_indicator {
                    "tag-suggested tag-low-confidence".to_string()
                } else {
                    "tag-suggested".to_string()
                }
            }
            TagDisplayStyle::Inherited { .. } => "tag-inherited".to_string(),
        }
    }
}

/// 敏感标签检测
pub struct SensitiveTagDetector {
    /// 敏感词列表
    sensitive_patterns: Vec<regex::Regex>,
    
    /// 需要人工确认的标签类型
    require_confirmation: HashSet<String>,
}

impl SensitiveTagDetector {
    pub fn new() -> Self {
        Self {
            sensitive_patterns: vec![
                regex::Regex::new(r"(?i)(私密|private|secret|confidential)").unwrap(),
                regex::Regex::new(r"(?i)(色情|porn|adult|nsfw)").unwrap(),
                regex::Regex::new(r"(?i)(密码|password|credential)").unwrap(),
            ],
            require_confirmation: HashSet::from([
                "personal".to_string(),
                "financial".to_string(),
                "medical".to_string(),
            ]),
        }
    }
    
    /// 检查标签是否敏感
    pub fn is_sensitive(&self, tag_name: &str) -> bool {
        self.sensitive_patterns.iter()
            .any(|p| p.is_match(tag_name))
    }
    
    /// 检查是否需要人工确认
    pub fn requires_confirmation(&self, tag_name: &str) -> bool {
        self.require_confirmation.contains(&tag_name.to_lowercase()) ||
        self.is_sensitive(tag_name)
    }
}

/// 标签搜索权重
pub struct TagSearchWeights {
    /// 已确认标签权重
    pub verified_weight: f32,
    
    /// AI 建议标签权重
    pub suggested_weight: f32,
    
    /// 继承标签权重
    pub inherited_weight: f32,
}

impl Default for TagSearchWeights {
    fn default() -> Self {
        Self {
            verified_weight: 1.0,
            suggested_weight: 0.5,  // AI 标签权重降低
            inherited_weight: 0.7,
        }
    }
}
```

### 前端组件接口

```typescript
// 标签组件 Props
interface TagProps {
  tag: Tag;
  relation: FileTagRelation;
  displayStyle: TagDisplayStyle;
  onConfirm?: () => void;
  onReject?: () => void;
  onRemove?: () => void;
}

// 标签显示样式
interface TagDisplayStyle {
  type: 'verified' | 'suggested' | 'inherited';
  backgroundOpacity: number;
  borderStyle: 'solid' | 'dashed' | 'dotted' | 'none';
  showConfidenceIndicator?: boolean;
}

// 标签组件样式
const tagStyles = {
  verified: {
    backgroundColor: 'var(--tag-color)',
    opacity: 1,
    border: '1px solid var(--tag-border)',
  },
  suggested: {
    backgroundColor: 'var(--tag-color)',
    opacity: 0.3,
    border: '1px dashed var(--tag-border)',
    // 悬停时显示确认/拒绝按钮
  },
  inherited: {
    backgroundColor: 'var(--tag-color)',
    opacity: 0.5,
    border: '1px dotted var(--tag-border)',
  },
};
```

## Additional Correctness Properties

基于新增的设计内容，补充以下正确性属性：

### Property 21: File ID Tracking Across Renames
*For any* file that is renamed in the filesystem, the ReconciliationService SHALL detect the rename and preserve all associated tags and relations.
**Validates: Requirements 8.4, Reconciliation Strategy**

### Property 22: Hybrid Search Score Normalization
*For any* search query, the final score SHALL be a weighted combination of vector score and BM25 score, with weights summing to 1.0.
**Validates: Requirements 2.2, Hybrid Search Logic**

### Property 23: Model Download Integrity
*For any* downloaded model file, the SHA256 checksum SHALL match the expected value in the manifest.
**Validates: Requirements 20, Installer Specification**

### Property 24: Sensitive Tag Confirmation Requirement
*For any* AI-generated tag that matches sensitive patterns, the tag SHALL be marked as requiring user confirmation before being used in search ranking.
**Validates: Requirements 5.5, 13.4, UI/UX Design**

### Property 25: Multi-Monitor Consistency
*For any* multi-monitor configuration, the NeuralFS Shell SHALL correctly enumerate all monitors and apply the configured rendering strategy.
**Validates: Requirements 1.1, OS Integration Layer**


## Process Supervisor (看门狗机制)

### Watchdog 进程设计

```rust
/// Watchdog 进程 - 独立的微型监控进程
/// 编译为独立的 neuralfs-watchdog.exe (~2MB)
pub struct Watchdog {
    /// 主进程 PID
    main_pid: Option<u32>,
    
    /// 心跳超时 (秒)
    heartbeat_timeout: u64,
    
    /// 最大重启次数
    max_restart_attempts: u32,
    
    /// 当前重启计数
    restart_count: u32,
    
    /// 心跳共享内存
    heartbeat_shm: SharedMemory,
    
    /// 配置
    config: WatchdogConfig,
}

#[derive(Debug, Clone)]
pub struct WatchdogConfig {
    /// 主进程路径
    pub main_exe_path: PathBuf,
    
    /// 心跳间隔 (毫秒)
    pub heartbeat_interval_ms: u64,
    
    /// 心跳超时 (秒)
    pub heartbeat_timeout_secs: u64,
    
    /// 最大连续重启次数
    pub max_restart_attempts: u32,
    
    /// 重启冷却时间 (秒)
    pub restart_cooldown_secs: u64,
    
    /// 是否在失败后恢复 Explorer
    pub restore_explorer_on_failure: bool,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            main_exe_path: std::env::current_exe()
                .unwrap()
                .parent()
                .unwrap()
                .join("neuralfs.exe"),
            heartbeat_interval_ms: 1000,
            heartbeat_timeout_secs: 5,
            max_restart_attempts: 3,
            restart_cooldown_secs: 10,
            restore_explorer_on_failure: true,
        }
    }
}

impl Watchdog {
    /// 启动 Watchdog 监控循环
    pub fn run(&mut self) -> ! {
        loop {
            // 1. 检查主进程是否存活
            if let Some(pid) = self.main_pid {
                if !self.is_process_alive(pid) {
                    self.handle_process_death();
                    continue;
                }
                
                // 2. 检查心跳
                if !self.check_heartbeat() {
                    tracing::warn!("Heartbeat timeout detected");
                    self.handle_heartbeat_timeout();
                    continue;
                }
            } else {
                // 主进程未启动，尝试启动
                self.start_main_process();
            }
            
            // 3. 休眠
            std::thread::sleep(Duration::from_millis(
                self.config.heartbeat_interval_ms
            ));
        }
    }
    
    /// 检查心跳
    fn check_heartbeat(&self) -> bool {
        let last_heartbeat = self.heartbeat_shm.read_timestamp();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        now - last_heartbeat < self.config.heartbeat_timeout_secs
    }
    
    /// 处理进程死亡
    fn handle_process_death(&mut self) {
        tracing::error!("Main process died unexpectedly");
        
        self.restart_count += 1;
        
        if self.restart_count > self.config.max_restart_attempts {
            tracing::error!(
                "Max restart attempts ({}) exceeded, restoring Explorer",
                self.config.max_restart_attempts
            );
            
            if self.config.restore_explorer_on_failure {
                self.restore_windows_explorer();
            }
            
            // 重置计数，等待冷却后再尝试
            std::thread::sleep(Duration::from_secs(
                self.config.restart_cooldown_secs
            ));
            self.restart_count = 0;
        } else {
            // 尝试重启
            self.start_main_process();
        }
    }
    
    /// 处理心跳超时 (进程可能卡死)
    fn handle_heartbeat_timeout(&mut self) {
        if let Some(pid) = self.main_pid {
            tracing::warn!("Killing unresponsive main process (PID: {})", pid);
            self.kill_process(pid);
            self.main_pid = None;
        }
        
        self.handle_process_death();
    }
    
    /// 启动主进程
    fn start_main_process(&mut self) {
        tracing::info!("Starting main process...");
        
        match std::process::Command::new(&self.config.main_exe_path)
            .arg("--watchdog-managed")
            .spawn()
        {
            Ok(child) => {
                self.main_pid = Some(child.id());
                tracing::info!("Main process started (PID: {})", child.id());
            }
            Err(e) => {
                tracing::error!("Failed to start main process: {}", e);
            }
        }
    }
    
    /// 恢复 Windows Explorer
    #[cfg(windows)]
    fn restore_windows_explorer(&self) {
        tracing::info!("Restoring Windows Explorer...");
        
        // 1. 显示任务栏
        unsafe {
            let taskbar = FindWindowW(w!("Shell_TrayWnd"), None);
            if !taskbar.is_invalid() {
                ShowWindow(taskbar, SW_SHOW);
            }
        }
        
        // 2. 启动 Explorer
        let _ = std::process::Command::new("explorer.exe").spawn();
        
        // 3. 显示通知
        self.show_error_notification(
            "NeuralFS 遇到问题",
            "已自动恢复 Windows 桌面。请检查日志文件了解详情。"
        );
    }
    
    #[cfg(windows)]
    fn show_error_notification(&self, title: &str, message: &str) {
        use windows::Win32::UI::Shell::*;
        
        // 使用 Windows Toast 通知
        // 简化实现，实际应使用 windows-rs 的通知 API
    }
}

/// 主进程中的心跳发送器
pub struct HeartbeatSender {
    shm: SharedMemory,
    interval: Duration,
}

impl HeartbeatSender {
    /// 启动心跳发送
    pub fn start(self) {
        std::thread::spawn(move || {
            loop {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                
                self.shm.write_timestamp(now);
                std::thread::sleep(self.interval);
            }
        });
    }
}

/// 共享内存 (用于心跳通信)
pub struct SharedMemory {
    #[cfg(windows)]
    handle: HANDLE,
    ptr: *mut u64,
}

impl SharedMemory {
    #[cfg(windows)]
    pub fn create(name: &str) -> Result<Self> {
        use windows::Win32::System::Memory::*;
        
        unsafe {
            let name_wide: Vec<u16> = name.encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            
            let handle = CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                PAGE_READWRITE,
                0,
                8, // 8 bytes for timestamp
                PCWSTR(name_wide.as_ptr()),
            )?;
            
            let ptr = MapViewOfFile(
                handle,
                FILE_MAP_ALL_ACCESS,
                0,
                0,
                8,
            ) as *mut u64;
            
            Ok(Self { handle, ptr })
        }
    }
    
    pub fn write_timestamp(&self, timestamp: u64) {
        unsafe {
            std::ptr::write_volatile(self.ptr, timestamp);
        }
    }
    
    pub fn read_timestamp(&self) -> u64 {
        unsafe {
            std::ptr::read_volatile(self.ptr)
        }
    }
}
```

## Asset Streaming (高性能资源传输)

### Custom Protocol 实现

```rust
/// 资源流服务 - 绕过 IPC 序列化
pub struct AssetStreamServer {
    /// 本地服务端口
    port: u16,
    
    /// 缩略图缓存
    thumbnail_cache: Arc<DashMap<Uuid, CachedThumbnail>>,
    
    /// 文件预览缓存
    preview_cache: Arc<DashMap<Uuid, CachedPreview>>,
}

#[derive(Clone)]
pub struct CachedThumbnail {
    pub data: Arc<Vec<u8>>,
    pub content_type: String,
    pub created_at: Instant,
}

impl AssetStreamServer {
    /// 启动资源流服务
    pub async fn start(&self) -> Result<()> {
        use axum::{Router, routing::get, extract::Path};
        
        let thumbnail_cache = self.thumbnail_cache.clone();
        let preview_cache = self.preview_cache.clone();
        
        let app = Router::new()
            // 缩略图路由: nfs://thumbnail/{uuid}
            .route("/thumbnail/:uuid", get(move |Path(uuid): Path<Uuid>| {
                let cache = thumbnail_cache.clone();
                async move {
                    Self::serve_thumbnail(uuid, cache).await
                }
            }))
            // 预览路由: nfs://preview/{uuid}
            .route("/preview/:uuid", get(move |Path(uuid): Path<Uuid>| {
                let cache = preview_cache.clone();
                async move {
                    Self::serve_preview(uuid, cache).await
                }
            }))
            // 原始文件路由: nfs://file/{uuid}
            .route("/file/:uuid", get(Self::serve_file));
        
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        tracing::info!("Asset stream server listening on {}", addr);
        
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await?;
        
        Ok(())
    }
    
    async fn serve_thumbnail(
        uuid: Uuid,
        cache: Arc<DashMap<Uuid, CachedThumbnail>>,
    ) -> impl axum::response::IntoResponse {
        use axum::http::{header, StatusCode};
        use axum::body::Body;
        
        if let Some(cached) = cache.get(&uuid) {
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, cached.content_type.clone())],
                Body::from(cached.data.as_ref().clone()),
            );
        }
        
        (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain".to_string())],
            Body::from("Thumbnail not found"),
        )
    }
    
    /// 获取资源 URL
    pub fn get_thumbnail_url(&self, uuid: Uuid) -> String {
        format!("http://127.0.0.1:{}/thumbnail/{}", self.port, uuid)
    }
    
    pub fn get_preview_url(&self, uuid: Uuid) -> String {
        format!("http://127.0.0.1:{}/preview/{}", self.port, uuid)
    }
}

/// Tauri Custom Protocol 注册
pub fn register_custom_protocol(app: &mut tauri::App) -> Result<()> {
    // 注册 nfs:// 协议
    app.register_uri_scheme_protocol("nfs", |_app, request| {
        let uri = request.uri();
        let path = uri.path();
        
        // 解析路径: /thumbnail/{uuid} 或 /preview/{uuid}
        let parts: Vec<&str> = path.split('/').collect();
        
        match parts.get(1) {
            Some(&"thumbnail") => {
                // 返回缩略图
                let uuid = parts.get(2).and_then(|s| Uuid::parse_str(s).ok());
                // ... 获取并返回缩略图数据
            }
            Some(&"preview") => {
                // 返回预览
            }
            _ => {
                // 404
            }
        }
        
        tauri::http::ResponseBuilder::new()
            .status(200)
            .body(vec![])
    });
    
    Ok(())
}
```

### 前端使用

```typescript
// 前端直接使用 URL，无需 IPC
const ThumbnailImage: Component<{ fileId: string }> = (props) => {
  // 使用 Custom Protocol URL
  const thumbnailUrl = `nfs://thumbnail/${props.fileId}`;
  
  // 或使用本地 HTTP 服务
  const httpUrl = `http://127.0.0.1:19283/thumbnail/${props.fileId}`;
  
  return (
    <img 
      src={thumbnailUrl} 
      loading="lazy"
      decoding="async"
    />
  );
};
```


## Game Mode Detection (游戏模式检测)

### 系统活动监控

```rust
/// 系统活动监控器
pub struct SystemActivityMonitor {
    /// 检测间隔
    check_interval: Duration,
    
    /// 当前状态
    current_state: Arc<RwLock<SystemState>>,
    
    /// 状态变化回调
    on_state_change: Option<Box<dyn Fn(SystemState, SystemState) + Send + Sync>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SystemState {
    /// 正常状态
    Normal,
    
    /// 全屏应用运行中 (游戏、视频播放等)
    FullscreenApp {
        app_name: Option<[u8; 256]>,
    },
    
    /// 演示模式 (PPT 等)
    PresentationMode,
    
    /// 勿扰模式
    DoNotDisturb,
    
    /// 低电量模式
    LowPower,
}

impl SystemActivityMonitor {
    /// 启动监控
    pub fn start(&self) {
        let state = self.current_state.clone();
        let interval = self.check_interval;
        let callback = self.on_state_change.clone();
        
        tokio::spawn(async move {
            loop {
                let new_state = Self::detect_system_state().await;
                
                let mut current = state.write().await;
                if *current != new_state {
                    let old_state = *current;
                    *current = new_state;
                    
                    if let Some(ref cb) = callback {
                        cb(old_state, new_state);
                    }
                }
                drop(current);
                
                tokio::time::sleep(interval).await;
            }
        });
    }
    
    /// 检测系统状态
    #[cfg(windows)]
    async fn detect_system_state() -> SystemState {
        use windows::Win32::UI::Shell::*;
        use windows::Win32::UI::WindowsAndMessaging::*;
        
        unsafe {
            // 1. 检查用户通知状态
            let mut state = QUERY_USER_NOTIFICATION_STATE::default();
            if SHQueryUserNotificationState(&mut state).is_ok() {
                match state {
                    QUNS_PRESENTATION_MODE => return SystemState::PresentationMode,
                    QUNS_RUNNING_D3D_FULL_SCREEN => {
                        return SystemState::FullscreenApp { app_name: None };
                    }
                    QUNS_BUSY => return SystemState::DoNotDisturb,
                    _ => {}
                }
            }
            
            // 2. 检查前台窗口是否全屏
            let foreground = GetForegroundWindow();
            if !foreground.is_invalid() {
                let mut rect = RECT::default();
                GetWindowRect(foreground, &mut rect);
                
                // 获取屏幕尺寸
                let screen_width = GetSystemMetrics(SM_CXSCREEN);
                let screen_height = GetSystemMetrics(SM_CYSCREEN);
                
                // 判断是否全屏
                let is_fullscreen = 
                    rect.left <= 0 &&
                    rect.top <= 0 &&
                    rect.right >= screen_width &&
                    rect.bottom >= screen_height;
                
                if is_fullscreen {
                    // 获取窗口进程名
                    let mut process_id = 0u32;
                    GetWindowThreadProcessId(foreground, Some(&mut process_id));
                    
                    // 排除自己和 Explorer
                    let current_pid = std::process::id();
                    if process_id != current_pid {
                        return SystemState::FullscreenApp { app_name: None };
                    }
                }
            }
            
            // 3. 检查电源状态
            let mut status = SYSTEM_POWER_STATUS::default();
            if GetSystemPowerStatus(&mut status).is_ok() {
                if status.BatteryLifePercent < 20 && 
                   status.ACLineStatus == 0 {
                    return SystemState::LowPower;
                }
            }
            
            SystemState::Normal
        }
    }
    
    /// 获取当前状态
    pub async fn get_state(&self) -> SystemState {
        *self.current_state.read().await
    }
}

/// 游戏模式响应策略
pub struct GameModePolicy {
    /// VRAM 管理器
    vram_manager: Arc<VRAMManager>,
    
    /// 索引服务
    indexer: Arc<RwLock<IndexerService>>,
    
    /// 云端桥接
    cloud_bridge: Arc<CloudBridge>,
}

impl GameModePolicy {
    /// 进入游戏模式
    pub async fn enter_game_mode(&self) {
        tracing::info!("Entering game mode");
        
        // 1. 卸载非必要模型，释放 VRAM
        self.vram_manager.evict_all_models().await.ok();
        
        // 2. 暂停后台索引
        let mut indexer = self.indexer.write().await;
        indexer.pause().await;
        
        // 3. 禁用云端请求
        self.cloud_bridge.set_enabled(false).await;
        
        // 4. 降低 UI 刷新率
        // (通过 IPC 通知前端)
    }
    
    /// 退出游戏模式
    pub async fn exit_game_mode(&self) {
        tracing::info!("Exiting game mode");
        
        // 1. 恢复索引
        let mut indexer = self.indexer.write().await;
        indexer.resume().await;
        
        // 2. 启用云端
        self.cloud_bridge.set_enabled(true).await;
        
        // 3. 预热常用模型
        self.vram_manager.prewarm_models().await.ok();
    }
}
```

## Self-Update Strategy (热更新策略)

### Swap & Restart 机制

```rust
/// 自更新管理器
pub struct SelfUpdater {
    /// 当前版本
    current_version: Version,
    
    /// 更新服务器 URL
    update_server: String,
    
    /// 下载目录
    download_dir: PathBuf,
    
    /// Watchdog 通信
    watchdog_ipc: WatchdogIpc,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: Version,
    pub release_date: DateTime<Utc>,
    pub download_url: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub changelog: String,
    pub is_critical: bool,
}

impl SelfUpdater {
    /// 检查更新
    pub async fn check_for_updates(&self) -> Result<Option<UpdateInfo>> {
        let url = format!("{}/api/updates/latest", self.update_server);
        
        let response: UpdateInfo = reqwest::get(&url)
            .await?
            .json()
            .await?;
        
        if response.version > self.current_version {
            Ok(Some(response))
        } else {
            Ok(None)
        }
    }
    
    /// 下载更新
    pub async fn download_update(&self, info: &UpdateInfo) -> Result<PathBuf> {
        let target_path = self.download_dir.join("neuralfs.new");
        
        // 下载文件
        let response = reqwest::get(&info.download_url).await?;
        let bytes = response.bytes().await?;
        
        // 写入临时文件
        tokio::fs::write(&target_path, &bytes).await?;
        
        // 验证校验和
        let hash = sha256_file(&target_path).await?;
        if hash != info.sha256 {
            tokio::fs::remove_file(&target_path).await?;
            return Err(UpdateError::ChecksumMismatch);
        }
        
        Ok(target_path)
    }
    
    /// 应用更新 (Swap & Restart)
    pub async fn apply_update(&self, new_exe_path: PathBuf) -> Result<()> {
        let current_exe = std::env::current_exe()?;
        let backup_exe = current_exe.with_extension("old");
        
        // 1. 通知 Watchdog 准备更新
        self.watchdog_ipc.send(WatchdogCommand::PrepareUpdate).await?;
        
        // 2. 创建更新脚本 (因为当前 exe 被锁定)
        let update_script = self.create_update_script(
            &current_exe,
            &new_exe_path,
            &backup_exe,
        )?;
        
        // 3. 启动更新脚本
        std::process::Command::new("cmd")
            .args(["/C", update_script.to_str().unwrap()])
            .spawn()?;
        
        // 4. 退出当前进程 (Watchdog 会重启新版本)
        std::process::exit(0);
    }
    
    /// 创建更新脚本
    #[cfg(windows)]
    fn create_update_script(
        &self,
        current: &Path,
        new: &Path,
        backup: &Path,
    ) -> Result<PathBuf> {
        let script_path = self.download_dir.join("update.bat");
        
        let script = format!(r#"
@echo off
:: 等待主进程退出
timeout /t 2 /nobreak > nul

:: 备份当前版本
move "{current}" "{backup}"

:: 替换为新版本
move "{new}" "{current}"

:: 删除更新脚本自身
del "%~f0"
"#,
            current = current.display(),
            new = new.display(),
            backup = backup.display(),
        );
        
        std::fs::write(&script_path, script)?;
        
        Ok(script_path)
    }
    
    /// 回滚更新
    pub async fn rollback(&self) -> Result<()> {
        let current_exe = std::env::current_exe()?;
        let backup_exe = current_exe.with_extension("old");
        
        if backup_exe.exists() {
            // 通知 Watchdog
            self.watchdog_ipc.send(WatchdogCommand::PrepareRollback).await?;
            
            // 创建回滚脚本
            let rollback_script = self.create_rollback_script(
                &current_exe,
                &backup_exe,
            )?;
            
            std::process::Command::new("cmd")
                .args(["/C", rollback_script.to_str().unwrap()])
                .spawn()?;
            
            std::process::exit(0);
        }
        
        Err(UpdateError::NoBackupAvailable)
    }
}

/// Watchdog IPC 通信
pub struct WatchdogIpc {
    pipe_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WatchdogCommand {
    /// 准备更新 (暂停自动重启)
    PrepareUpdate,
    
    /// 准备回滚
    PrepareRollback,
    
    /// 更新完成
    UpdateComplete,
    
    /// 正常关闭
    Shutdown,
}

impl WatchdogIpc {
    pub async fn send(&self, cmd: WatchdogCommand) -> Result<()> {
        #[cfg(windows)]
        {
            use tokio::net::windows::named_pipe::ClientOptions;
            
            let pipe = ClientOptions::new()
                .open(&self.pipe_name)?;
            
            let data = bincode::serialize(&cmd)?;
            pipe.write_all(&data).await?;
        }
        
        Ok(())
    }
}
```

## Additional Correctness Properties (补充)

### Property 26: Watchdog Heartbeat Reliability
*For any* running NeuralFS main process, the heartbeat SHALL be sent to shared memory at least once per heartbeat interval.
**Validates: Process Supervisor**

### Property 27: Asset Streaming Performance
*For any* thumbnail request via Custom Protocol, the response SHALL be returned without IPC serialization overhead (direct binary stream).
**Validates: Asset Streaming**

### Property 28: Game Mode Detection Accuracy
*For any* fullscreen application running, the SystemActivityMonitor SHALL detect it within the check interval and transition to FullscreenApp state.
**Validates: Game Mode Detection**

### Property 29: Update Atomicity
*For any* self-update operation, either the update completes successfully with the new version running, or the system rolls back to the previous version.
**Validates: Self-Update Strategy**

### Property 30: Watchdog Recovery Guarantee
*For any* main process crash, the Watchdog SHALL either restart the main process or restore Windows Explorer within (max_restart_attempts * restart_cooldown) seconds.
**Validates: Process Supervisor**


## Tokenizer Strategy (多语言分词策略)

### 中日文分词集成

```rust
/// 多语言分词器配置
pub struct MultilingualTokenizer {
    /// 中文分词器 (jieba)
    chinese_tokenizer: JiebaTokenizer,
    
    /// 日文分词器 (lindera)
    japanese_tokenizer: LinderaTokenizer,
    
    /// 英文分词器 (默认)
    english_tokenizer: SimpleTokenizer,
    
    /// 语言检测器
    language_detector: LanguageDetector,
}

impl MultilingualTokenizer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            chinese_tokenizer: JiebaTokenizer::new()?,
            japanese_tokenizer: LinderaTokenizer::new()?,
            english_tokenizer: SimpleTokenizer::default(),
            language_detector: LanguageDetector::new(),
        })
    }
    
    /// 根据文本语言选择分词器
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        let lang = self.language_detector.detect(text);
        
        match lang {
            Language::Chinese => self.chinese_tokenizer.tokenize(text),
            Language::Japanese => self.japanese_tokenizer.tokenize(text),
            _ => self.english_tokenizer.tokenize(text),
        }
    }
}

/// Jieba 中文分词器
pub struct JiebaTokenizer {
    jieba: jieba_rs::Jieba,
}

impl JiebaTokenizer {
    pub fn new() -> Result<Self> {
        // 使用默认词典，可选加载自定义词典
        let jieba = jieba_rs::Jieba::new();
        Ok(Self { jieba })
    }
    
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        self.jieba
            .cut(text, true) // 使用 HMM 模式
            .into_iter()
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.to_string())
            .collect()
    }
    
    /// 添加自定义词汇
    pub fn add_word(&mut self, word: &str, freq: Option<usize>, tag: Option<&str>) {
        self.jieba.add_word(word, freq, tag);
    }
}

/// Lindera 日文分词器
pub struct LinderaTokenizer {
    tokenizer: lindera::tokenizer::Tokenizer,
}

impl LinderaTokenizer {
    pub fn new() -> Result<Self> {
        use lindera::tokenizer::{Tokenizer, TokenizerConfig};
        use lindera::mode::Mode;
        
        let config = TokenizerConfig {
            mode: Mode::Normal,
            ..Default::default()
        };
        
        let tokenizer = Tokenizer::with_config(config)?;
        Ok(Self { tokenizer })
    }
    
    pub fn tokenize(&self, text: &str) -> Vec<String> {
        self.tokenizer
            .tokenize(text)
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.text.to_string())
            .collect()
    }
}

/// Tantivy 自定义分词器注册
pub fn register_multilingual_tokenizer(index: &tantivy::Index) -> Result<()> {
    use tantivy::tokenizer::*;
    
    // 注册中文分词器
    index.tokenizers().register(
        "chinese",
        TextAnalyzer::builder(JiebaTantivyTokenizer::new())
            .filter(LowerCaser)
            .filter(RemoveLongFilter::limit(40))
            .build(),
    );
    
    // 注册日文分词器
    index.tokenizers().register(
        "japanese",
        TextAnalyzer::builder(LinderaTantivyTokenizer::new())
            .filter(LowerCaser)
            .filter(RemoveLongFilter::limit(40))
            .build(),
    );
    
    // 注册多语言分词器 (自动检测)
    index.tokenizers().register(
        "multilingual",
        TextAnalyzer::builder(MultilingualTantivyTokenizer::new())
            .filter(LowerCaser)
            .filter(RemoveLongFilter::limit(40))
            .build(),
    );
    
    Ok(())
}

/// Tantivy 兼容的 Jieba 分词器
pub struct JiebaTantivyTokenizer {
    jieba: Arc<jieba_rs::Jieba>,
}

impl tantivy::tokenizer::Tokenizer for JiebaTantivyTokenizer {
    type TokenStream<'a> = JiebaTokenStream<'a>;
    
    fn token_stream<'a>(&'a mut self, text: &'a str) -> Self::TokenStream<'a> {
        let tokens: Vec<_> = self.jieba
            .cut(text, true)
            .into_iter()
            .map(|s| s.to_string())
            .collect();
        
        JiebaTokenStream {
            tokens,
            index: 0,
            token: tantivy::tokenizer::Token::default(),
        }
    }
}

/// 更新后的 TextIndex Schema
impl TextIndex {
    pub fn new_with_multilingual(index_path: &Path) -> Result<Self> {
        use tantivy::schema::*;
        
        let mut schema_builder = Schema::builder();
        
        // 使用多语言分词器
        let text_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("multilingual")
                    .set_index_option(IndexRecordOption::WithFreqsAndPositions)
            )
            .set_stored();
        
        schema_builder.add_text_field("file_id", STRING | STORED);
        schema_builder.add_text_field("chunk_id", STRING | STORED);
        schema_builder.add_text_field("filename", text_options.clone());
        schema_builder.add_text_field("content", text_options.clone());
        schema_builder.add_text_field("tags", text_options);
        schema_builder.add_u64_field("modified_at", INDEXED | STORED);
        
        let schema = schema_builder.build();
        
        let index = if index_path.exists() {
            tantivy::Index::open_in_dir(index_path)?
        } else {
            std::fs::create_dir_all(index_path)?;
            tantivy::Index::create_in_dir(index_path, schema)?
        };
        
        // 注册多语言分词器
        register_multilingual_tokenizer(&index)?;
        
        let reader = index.reader()?;
        
        Ok(Self { index, reader })
    }
}
```

## Schema Migration (数据库迁移策略)

### 自动迁移管理

```rust
/// 数据库迁移管理器
pub struct MigrationManager {
    db_path: PathBuf,
    migrations_dir: PathBuf,
}

impl MigrationManager {
    /// 在应用启动时执行迁移
    pub async fn run_migrations(&self, pool: &SqlitePool) -> Result<MigrationResult> {
        // 1. 获取当前数据库版本
        let current_version = self.get_current_version(pool).await?;
        
        // 2. 获取所有待执行的迁移
        let pending = self.get_pending_migrations(current_version)?;
        
        if pending.is_empty() {
            return Ok(MigrationResult {
                applied: 0,
                current_version,
            });
        }
        
        // 3. 备份数据库
        self.backup_database().await?;
        
        // 4. 执行迁移
        let mut applied = 0;
        for migration in pending {
            match self.apply_migration(pool, &migration).await {
                Ok(_) => {
                    applied += 1;
                    tracing::info!(
                        "Applied migration: {} ({})",
                        migration.version,
                        migration.name
                    );
                }
                Err(e) => {
                    tracing::error!(
                        "Migration {} failed: {}. Rolling back...",
                        migration.version,
                        e
                    );
                    self.rollback_to_backup().await?;
                    return Err(MigrationError::MigrationFailed {
                        version: migration.version,
                        error: e.to_string(),
                    });
                }
            }
        }
        
        let new_version = self.get_current_version(pool).await?;
        
        Ok(MigrationResult {
            applied,
            current_version: new_version,
        })
    }
    
    /// 获取当前数据库版本
    async fn get_current_version(&self, pool: &SqlitePool) -> Result<u32> {
        // 确保 schema_version 表存在
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL,
                name TEXT NOT NULL
            )
            "#
        )
        .execute(pool)
        .await?;
        
        let version: Option<(i32,)> = sqlx::query_as(
            "SELECT MAX(version) FROM schema_version"
        )
        .fetch_optional(pool)
        .await?;
        
        Ok(version.map(|v| v.0 as u32).unwrap_or(0))
    }
    
    /// 获取待执行的迁移
    fn get_pending_migrations(&self, current_version: u32) -> Result<Vec<Migration>> {
        let mut migrations = Vec::new();
        
        // 从嵌入的迁移文件中读取
        // 使用 include_str! 宏在编译时嵌入
        let embedded_migrations = [
            (1, "initial_schema", include_str!("../migrations/001_initial_schema.sql")),
            (2, "add_file_id", include_str!("../migrations/002_add_file_id.sql")),
            (3, "add_block_rules", include_str!("../migrations/003_add_block_rules.sql")),
            // ... 更多迁移
        ];
        
        for (version, name, sql) in embedded_migrations {
            if version > current_version {
                migrations.push(Migration {
                    version,
                    name: name.to_string(),
                    sql: sql.to_string(),
                });
            }
        }
        
        migrations.sort_by_key(|m| m.version);
        Ok(migrations)
    }
    
    /// 应用单个迁移
    async fn apply_migration(&self, pool: &SqlitePool, migration: &Migration) -> Result<()> {
        // 开启事务
        let mut tx = pool.begin().await?;
        
        // 执行迁移 SQL
        for statement in migration.sql.split(';') {
            let statement = statement.trim();
            if !statement.is_empty() {
                sqlx::query(statement).execute(&mut *tx).await?;
            }
        }
        
        // 记录迁移版本
        sqlx::query(
            r#"
            INSERT INTO schema_version (version, applied_at, name)
            VALUES (?, datetime('now'), ?)
            "#
        )
        .bind(migration.version as i32)
        .bind(&migration.name)
        .execute(&mut *tx)
        .await?;
        
        // 提交事务
        tx.commit().await?;
        
        Ok(())
    }
    
    /// 备份数据库
    async fn backup_database(&self) -> Result<PathBuf> {
        let backup_path = self.db_path.with_extension(format!(
            "backup.{}",
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        ));
        
        tokio::fs::copy(&self.db_path, &backup_path).await?;
        
        tracing::info!("Database backed up to: {:?}", backup_path);
        Ok(backup_path)
    }
    
    /// 回滚到备份
    async fn rollback_to_backup(&self) -> Result<()> {
        // 找到最新的备份
        let backup_dir = self.db_path.parent().unwrap();
        let mut backups: Vec<_> = std::fs::read_dir(backup_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.contains(".backup."))
                    .unwrap_or(false)
            })
            .collect();
        
        backups.sort_by_key(|e| e.metadata().unwrap().modified().unwrap());
        
        if let Some(latest_backup) = backups.last() {
            tokio::fs::copy(latest_backup.path(), &self.db_path).await?;
            tracing::info!("Rolled back to: {:?}", latest_backup.path());
        }
        
        Ok(())
    }
}

#[derive(Debug)]
pub struct Migration {
    pub version: u32,
    pub name: String,
    pub sql: String,
}

#[derive(Debug)]
pub struct MigrationResult {
    pub applied: u32,
    pub current_version: u32,
}
```


## Directory Blacklist (文件夹炸弹防御)

### 黑名单配置

```rust
/// 目录过滤配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryFilterConfig {
    /// 黑名单模式 (glob 格式)
    pub blacklist_patterns: Vec<String>,
    
    /// 白名单模式 (优先级高于黑名单)
    pub whitelist_patterns: Vec<String>,
    
    /// 最大目录深度
    pub max_depth: u32,
    
    /// 单目录最大文件数 (超过则跳过)
    pub max_files_per_dir: u32,
    
    /// 最大文件大小 (字节)
    pub max_file_size: u64,
    
    /// 是否跟随符号链接
    pub follow_symlinks: bool,
}

impl Default for DirectoryFilterConfig {
    fn default() -> Self {
        Self {
            blacklist_patterns: vec![
                // 开发目录
                "**/node_modules/**".to_string(),
                "**/.git/**".to_string(),
                "**/target/**".to_string(),
                "**/.idea/**".to_string(),
                "**/.vscode/**".to_string(),
                "**/vendor/**".to_string(),
                "**/__pycache__/**".to_string(),
                "**/.venv/**".to_string(),
                "**/venv/**".to_string(),
                "**/dist/**".to_string(),
                "**/build/**".to_string(),
                "**/.cache/**".to_string(),
                
                // 系统目录
                "**/System Volume Information/**".to_string(),
                "**/$Recycle.Bin/**".to_string(),
                "**/Windows/**".to_string(),
                "**/Program Files/**".to_string(),
                "**/Program Files (x86)/**".to_string(),
                
                // 临时文件
                "**/*.tmp".to_string(),
                "**/*.temp".to_string(),
                "**/*.swp".to_string(),
                "**/*~".to_string(),
                "**/.DS_Store".to_string(),
                "**/Thumbs.db".to_string(),
            ],
            whitelist_patterns: vec![],
            max_depth: 20,
            max_files_per_dir: 10000,
            max_file_size: 500 * 1024 * 1024, // 500MB
            follow_symlinks: false,
        }
    }
}

/// 目录过滤器
pub struct DirectoryFilter {
    config: DirectoryFilterConfig,
    blacklist_matchers: Vec<glob::Pattern>,
    whitelist_matchers: Vec<glob::Pattern>,
}

impl DirectoryFilter {
    pub fn new(config: DirectoryFilterConfig) -> Result<Self> {
        let blacklist_matchers = config.blacklist_patterns
            .iter()
            .map(|p| glob::Pattern::new(p))
            .collect::<Result<Vec<_>, _>>()?;
        
        let whitelist_matchers = config.whitelist_patterns
            .iter()
            .map(|p| glob::Pattern::new(p))
            .collect::<Result<Vec<_>, _>>()?;
        
        Ok(Self {
            config,
            blacklist_matchers,
            whitelist_matchers,
        })
    }
    
    /// 检查路径是否应该被过滤
    pub fn should_filter(&self, path: &Path) -> FilterResult {
        let path_str = path.to_string_lossy();
        
        // 1. 检查白名单 (优先)
        for matcher in &self.whitelist_matchers {
            if matcher.matches(&path_str) {
                return FilterResult::Include;
            }
        }
        
        // 2. 检查黑名单
        for matcher in &self.blacklist_matchers {
            if matcher.matches(&path_str) {
                return FilterResult::Exclude(FilterReason::Blacklisted);
            }
        }
        
        // 3. 检查深度
        let depth = path.components().count();
        if depth > self.config.max_depth as usize {
            return FilterResult::Exclude(FilterReason::TooDeep);
        }
        
        FilterResult::Include
    }
    
    /// 检查目录是否应该被跳过 (文件数过多)
    pub async fn should_skip_directory(&self, path: &Path) -> FilterResult {
        // 快速计数目录中的文件数
        let count = self.count_files_fast(path).await;
        
        if count > self.config.max_files_per_dir {
            tracing::warn!(
                "Skipping directory with {} files (limit: {}): {:?}",
                count,
                self.config.max_files_per_dir,
                path
            );
            return FilterResult::Exclude(FilterReason::TooManyFiles);
        }
        
        FilterResult::Include
    }
    
    /// 快速计数目录文件数 (不递归)
    async fn count_files_fast(&self, path: &Path) -> u32 {
        let mut count = 0u32;
        
        if let Ok(mut entries) = tokio::fs::read_dir(path).await {
            while let Ok(Some(_)) = entries.next_entry().await {
                count += 1;
                
                // 提前退出，避免在超大目录中浪费时间
                if count > self.config.max_files_per_dir {
                    break;
                }
            }
        }
        
        count
    }
    
    /// 检查文件大小
    pub fn check_file_size(&self, size: u64) -> FilterResult {
        if size > self.config.max_file_size {
            return FilterResult::Exclude(FilterReason::TooLarge);
        }
        FilterResult::Include
    }
}

#[derive(Debug, Clone, Copy)]
pub enum FilterResult {
    Include,
    Exclude(FilterReason),
}

#[derive(Debug, Clone, Copy)]
pub enum FilterReason {
    Blacklisted,
    TooDeep,
    TooManyFiles,
    TooLarge,
    Symlink,
}

/// 更新 ReconciliationService 以使用过滤器
impl ReconciliationService {
    pub async fn scan_directory_filtered(
        &self,
        path: &Path,
        filter: &DirectoryFilter,
        depth: u32,
    ) -> Result<Vec<FsFileInfo>> {
        let mut results = Vec::new();
        
        // 检查目录是否应该被过滤
        if let FilterResult::Exclude(reason) = filter.should_filter(path) {
            tracing::debug!("Filtered directory {:?}: {:?}", path, reason);
            return Ok(results);
        }
        
        // 检查目录文件数
        if let FilterResult::Exclude(reason) = filter.should_skip_directory(path).await {
            tracing::debug!("Skipped directory {:?}: {:?}", path, reason);
            return Ok(results);
        }
        
        // 扫描目录
        let mut entries = tokio::fs::read_dir(path).await?;
        
        while let Some(entry) = entries.next_entry().await? {
            let entry_path = entry.path();
            let metadata = entry.metadata().await?;
            
            // 检查符号链接
            if metadata.is_symlink() && !filter.config.follow_symlinks {
                continue;
            }
            
            if metadata.is_file() {
                // 检查文件大小
                if let FilterResult::Exclude(_) = filter.check_file_size(metadata.len()) {
                    continue;
                }
                
                // 检查文件路径
                if let FilterResult::Exclude(_) = filter.should_filter(&entry_path) {
                    continue;
                }
                
                results.push(FsFileInfo {
                    path: entry_path,
                    file_id: Self::get_file_id(&entry.path())?,
                    size_bytes: metadata.len(),
                    modified_at: metadata.modified()?.into(),
                });
            } else if metadata.is_dir() {
                // 递归扫描子目录
                let sub_results = Box::pin(self.scan_directory_filtered(
                    &entry_path,
                    filter,
                    depth + 1,
                )).await?;
                
                results.extend(sub_results);
            }
        }
        
        Ok(results)
    }
}

/// 用户可配置的过滤规则
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserFilterRules {
    /// 用户添加的黑名单
    pub custom_blacklist: Vec<String>,
    
    /// 用户添加的白名单
    pub custom_whitelist: Vec<String>,
    
    /// 是否使用默认黑名单
    pub use_default_blacklist: bool,
}

impl Default for UserFilterRules {
    fn default() -> Self {
        Self {
            custom_blacklist: vec![],
            custom_whitelist: vec![],
            use_default_blacklist: true,
        }
    }
}
```

## Final Correctness Properties

### Property 31: Chinese Tokenization Quality
*For any* Chinese text input, the JiebaTokenizer SHALL produce meaningful word segments (not single characters or entire sentences as single tokens).
**Validates: Tokenizer Strategy, Requirements 19**

### Property 32: Migration Atomicity
*For any* database migration, either all SQL statements in the migration succeed and the version is recorded, or the database is rolled back to the pre-migration state.
**Validates: Schema Migration, Requirements 18**

### Property 33: Directory Filter Effectiveness
*For any* path matching a blacklist pattern, the DirectoryFilter SHALL return FilterResult::Exclude, preventing indexing of that path.
**Validates: Directory Blacklist, Requirements 3**

### Property 34: Large Directory Protection
*For any* directory containing more than max_files_per_dir files, the scan SHALL be skipped to prevent CPU exhaustion.
**Validates: Directory Blacklist, Requirements 4**


## SQLite High Concurrency Configuration (WAL 模式)

### 数据库连接配置

```rust
/// SQLite 连接池配置
pub struct DatabaseConfig {
    /// 数据库文件路径
    pub db_path: PathBuf,
    
    /// 最大连接数
    pub max_connections: u32,
    
    /// 最小连接数
    pub min_connections: u32,
    
    /// 连接超时 (秒)
    pub connect_timeout_secs: u64,
    
    /// 空闲超时 (秒)
    pub idle_timeout_secs: u64,
    
    /// 是否启用 WAL 模式
    pub enable_wal: bool,
    
    /// 同步模式
    pub synchronous: SynchronousMode,
    
    /// 缓存大小 (页数, 负数表示 KB)
    pub cache_size: i32,
    
    /// 忙等待超时 (毫秒)
    pub busy_timeout_ms: u32,
}

#[derive(Debug, Clone, Copy)]
pub enum SynchronousMode {
    Off,      // 最快，但可能丢数据
    Normal,   // 平衡
    Full,     // 最安全
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            db_path: dirs::data_local_dir()
                .unwrap_or_default()
                .join("NeuralFS")
                .join("metadata.db"),
            max_connections: 10,
            min_connections: 2,
            connect_timeout_secs: 30,
            idle_timeout_secs: 600,
            enable_wal: true,           // 关键：启用 WAL
            synchronous: SynchronousMode::Normal,
            cache_size: -64000,         // 64MB 缓存
            busy_timeout_ms: 5000,      // 5秒忙等待
        }
    }
}

/// 创建数据库连接池
pub async fn create_database_pool(config: &DatabaseConfig) -> Result<SqlitePool> {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteJournalMode, SqliteSynchronous};
    
    // 确保目录存在
    if let Some(parent) = config.db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    
    // 构建连接选项
    let connect_options = SqliteConnectOptions::new()
        .filename(&config.db_path)
        .create_if_missing(true)
        // WAL 模式：允许并发读写
        .journal_mode(if config.enable_wal {
            SqliteJournalMode::Wal
        } else {
            SqliteJournalMode::Delete
        })
        // 同步模式
        .synchronous(match config.synchronous {
            SynchronousMode::Off => SqliteSynchronous::Off,
            SynchronousMode::Normal => SqliteSynchronous::Normal,
            SynchronousMode::Full => SqliteSynchronous::Full,
        })
        // 忙等待超时
        .busy_timeout(Duration::from_millis(config.busy_timeout_ms as u64))
        // 外键约束
        .foreign_keys(true);
    
    // 创建连接池
    let pool = SqlitePoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(Duration::from_secs(config.connect_timeout_secs))
        .idle_timeout(Duration::from_secs(config.idle_timeout_secs))
        .connect_with(connect_options)
        .await?;
    
    // 设置 PRAGMA (每个连接)
    sqlx::query(&format!(
        "PRAGMA cache_size = {}",
        config.cache_size
    ))
    .execute(&pool)
    .await?;
    
    // 启用内存映射 I/O (提升读取性能)
    sqlx::query("PRAGMA mmap_size = 268435456") // 256MB
        .execute(&pool)
        .await?;
    
    // 优化临时存储
    sqlx::query("PRAGMA temp_store = MEMORY")
        .execute(&pool)
        .await?;
    
    tracing::info!(
        "Database pool created: {:?} (WAL: {}, connections: {})",
        config.db_path,
        config.enable_wal,
        config.max_connections
    );
    
    Ok(pool)
}

/// WAL 检查点管理
pub struct WalCheckpointManager {
    pool: SqlitePool,
    checkpoint_interval: Duration,
}

impl WalCheckpointManager {
    /// 启动定期检查点
    pub fn start(&self) {
        let pool = self.pool.clone();
        let interval = self.checkpoint_interval;
        
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(interval).await;
                
                // 执行被动检查点 (不阻塞写入)
                if let Err(e) = sqlx::query("PRAGMA wal_checkpoint(PASSIVE)")
                    .execute(&pool)
                    .await
                {
                    tracing::warn!("WAL checkpoint failed: {}", e);
                }
            }
        });
    }
    
    /// 执行完整检查点 (应用退出时)
    pub async fn full_checkpoint(&self) -> Result<()> {
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
```

## Display Change Handling (屏幕分辨率变更)

### 显示器变更监听

```rust
/// 显示器变更处理器
impl WindowsDesktopManager {
    /// 注册显示器变更监听
    pub fn register_display_change_handler(&self) -> Result<()> {
        // 创建消息窗口用于接收系统消息
        let hwnd = self.create_message_window()?;
        
        // 启动消息循环
        std::thread::spawn(move || {
            unsafe {
                let mut msg = MSG::default();
                
                while GetMessageW(&mut msg, hwnd, 0, 0).as_bool() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        });
        
        Ok(())
    }
    
    /// 创建隐藏的消息窗口
    fn create_message_window(&self) -> Result<HWND> {
        unsafe {
            let class_name = w!("NeuralFS_MessageWindow");
            
            let wc = WNDCLASSW {
                lpfnWndProc: Some(Self::message_window_proc),
                lpszClassName: class_name,
                ..Default::default()
            };
            
            RegisterClassW(&wc);
            
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!(""),
                WINDOW_STYLE::default(),
                0, 0, 0, 0,
                HWND_MESSAGE, // 消息专用窗口
                None,
                None,
                None,
            );
            
            Ok(hwnd)
        }
    }
    
    /// 消息窗口过程
    extern "system" fn message_window_proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_DISPLAYCHANGE => {
                tracing::info!(
                    "Display change detected: {}x{} @ {} bpp",
                    lparam.0 as u16,
                    (lparam.0 >> 16) as u16,
                    wparam.0
                );
                
                // 获取 DesktopManager 实例并重新调整窗口
                if let Some(manager) = DESKTOP_MANAGER.get() {
                    manager.handle_display_change();
                }
                
                LRESULT(0)
            }
            
            WM_DEVICECHANGE => {
                // 设备变更 (显示器插拔)
                tracing::info!("Device change detected");
                
                if let Some(manager) = DESKTOP_MANAGER.get() {
                    manager.handle_device_change();
                }
                
                LRESULT(0)
            }
            
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
    
    /// 处理显示器变更
    pub fn handle_display_change(&self) {
        // 1. 重新枚举显示器
        let monitors = self.setup_multi_monitor().unwrap_or_default();
        
        // 2. 根据策略调整窗口
        match self.multi_monitor_strategy {
            MultiMonitorStrategy::PrimaryOnly => {
                // 找到主显示器
                if let Some(primary) = monitors.iter().find(|m| m.is_primary) {
                    self.resize_to_monitor(primary);
                }
            }
            
            MultiMonitorStrategy::Unified => {
                // 计算所有显示器的边界框
                let bounds = self.calculate_unified_bounds(&monitors);
                self.resize_to_bounds(&bounds);
            }
            
            MultiMonitorStrategy::Independent => {
                // 每个显示器独立处理
                // (需要多窗口支持)
            }
        }
        
        // 3. 重新挂载到 WorkerW (可能已重置)
        if self.is_shell_replaced {
            if let Err(e) = self.reattach_to_workerw() {
                tracing::error!("Failed to reattach to WorkerW: {}", e);
            }
        }
    }
    
    /// 重新挂载到 WorkerW
    fn reattach_to_workerw(&self) -> Result<()> {
        unsafe {
            // WorkerW 可能已被重建，需要重新查找
            let progman = FindWindowW(w!("Progman"), None);
            
            SendMessageTimeoutW(
                progman,
                0x052C,
                WPARAM(0),
                LPARAM(0),
                SMTO_NORMAL,
                1000,
                None,
            );
            
            let mut worker_w = HWND::default();
            EnumWindows(
                Some(Self::enum_windows_callback),
                LPARAM(&mut worker_w as *mut _ as isize),
            );
            
            if !worker_w.is_invalid() {
                SetParent(self.main_hwnd, worker_w);
            }
        }
        
        Ok(())
    }
    
    /// 调整窗口到指定显示器
    fn resize_to_monitor(&self, monitor: &MonitorInfo) {
        unsafe {
            SetWindowPos(
                self.main_hwnd,
                HWND_TOP,
                monitor.rect.left,
                monitor.rect.top,
                monitor.rect.right - monitor.rect.left,
                monitor.rect.bottom - monitor.rect.top,
                SWP_SHOWWINDOW,
            );
        }
    }
}

// 全局 DesktopManager 引用 (用于消息回调)
static DESKTOP_MANAGER: OnceCell<Arc<WindowsDesktopManager>> = OnceCell::new();
```


## Asset Server Security (资源服务安全)

### Session Token 验证

```rust
/// 安全的资源流服务
pub struct SecureAssetStreamServer {
    /// 本地服务端口
    port: u16,
    
    /// 会话令牌 (启动时随机生成)
    session_token: String,
    
    /// 缩略图缓存
    thumbnail_cache: Arc<DashMap<Uuid, CachedThumbnail>>,
    
    /// 允许的来源 (CORS)
    allowed_origins: Vec<String>,
}

impl SecureAssetStreamServer {
    pub fn new(port: u16) -> Self {
        // 生成随机会话令牌
        let session_token = Self::generate_session_token();
        
        Self {
            port,
            session_token,
            thumbnail_cache: Arc::new(DashMap::new()),
            allowed_origins: vec![
                format!("http://localhost:{}", port),
                format!("http://127.0.0.1:{}", port),
                "tauri://localhost".to_string(),
            ],
        }
    }
    
    /// 生成安全的会话令牌
    fn generate_session_token() -> String {
        use rand::Rng;
        
        let mut rng = rand::thread_rng();
        let bytes: [u8; 32] = rng.gen();
        hex::encode(bytes)
    }
    
    /// 获取会话令牌 (供前端使用)
    pub fn get_session_token(&self) -> &str {
        &self.session_token
    }
    
    /// 启动安全资源流服务
    pub async fn start(&self) -> Result<()> {
        use axum::{
            Router,
            routing::get,
            extract::{Path, Query, State},
            http::{StatusCode, header, HeaderMap, HeaderValue},
            middleware::{self, Next},
            response::Response,
        };
        
        let state = AppState {
            session_token: self.session_token.clone(),
            thumbnail_cache: self.thumbnail_cache.clone(),
            allowed_origins: self.allowed_origins.clone(),
        };
        
        let app = Router::new()
            .route("/thumbnail/:uuid", get(Self::serve_thumbnail))
            .route("/preview/:uuid", get(Self::serve_preview))
            .route("/file/:uuid", get(Self::serve_file))
            // 添加安全中间件
            .layer(middleware::from_fn_with_state(
                state.clone(),
                Self::security_middleware
            ))
            .with_state(state);
        
        let addr = SocketAddr::from(([127, 0, 0, 1], self.port));
        tracing::info!("Secure asset server listening on {}", addr);
        
        axum::Server::bind(&addr)
            .serve(app.into_make_service())
            .await?;
        
        Ok(())
    }
    
    /// 安全中间件
    async fn security_middleware(
        State(state): State<AppState>,
        Query(params): Query<TokenParams>,
        headers: HeaderMap,
        request: axum::http::Request<axum::body::Body>,
        next: Next<axum::body::Body>,
    ) -> Result<Response, StatusCode> {
        // 1. 验证会话令牌
        let token = params.token.as_deref()
            .or_else(|| {
                headers.get("X-Session-Token")
                    .and_then(|v| v.to_str().ok())
            });
        
        match token {
            Some(t) if t == state.session_token => {
                // 令牌有效
            }
            _ => {
                tracing::warn!(
                    "Invalid session token from {:?}",
                    headers.get("Origin")
                );
                return Err(StatusCode::FORBIDDEN);
            }
        }
        
        // 2. 验证 Origin (防止 CSRF)
        if let Some(origin) = headers.get("Origin") {
            let origin_str = origin.to_str().unwrap_or("");
            if !state.allowed_origins.iter().any(|o| o == origin_str) {
                tracing::warn!("Blocked request from origin: {}", origin_str);
                return Err(StatusCode::FORBIDDEN);
            }
        }
        
        // 3. 验证 Referer
        if let Some(referer) = headers.get("Referer") {
            let referer_str = referer.to_str().unwrap_or("");
            let is_allowed = state.allowed_origins.iter()
                .any(|o| referer_str.starts_with(o));
            
            if !is_allowed {
                tracing::warn!("Blocked request with referer: {}", referer_str);
                return Err(StatusCode::FORBIDDEN);
            }
        }
        
        // 4. 继续处理请求
        let mut response = next.run(request).await;
        
        // 5. 添加安全响应头
        let headers = response.headers_mut();
        headers.insert(
            "X-Content-Type-Options",
            HeaderValue::from_static("nosniff")
        );
        headers.insert(
            "X-Frame-Options",
            HeaderValue::from_static("DENY")
        );
        headers.insert(
            "Cache-Control",
            HeaderValue::from_static("private, no-store")
        );
        
        Ok(response)
    }
    
    async fn serve_thumbnail(
        State(state): State<AppState>,
        Path(uuid): Path<Uuid>,
    ) -> impl axum::response::IntoResponse {
        // ... 实现同前，但已通过中间件验证
        if let Some(cached) = state.thumbnail_cache.get(&uuid) {
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, cached.content_type.clone())],
                cached.data.as_ref().clone(),
            );
        }
        
        (
            StatusCode::NOT_FOUND,
            [(header::CONTENT_TYPE, "text/plain".to_string())],
            b"Not found".to_vec(),
        )
    }
}

#[derive(Clone)]
struct AppState {
    session_token: String,
    thumbnail_cache: Arc<DashMap<Uuid, CachedThumbnail>>,
    allowed_origins: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct TokenParams {
    token: Option<String>,
}

/// 前端使用示例
/// 
/// ```typescript
/// // 在应用启动时获取会话令牌
/// const sessionToken = await invoke<string>('get_asset_session_token');
/// 
/// // 使用令牌请求资源
/// const thumbnailUrl = `http://127.0.0.1:19283/thumbnail/${fileId}?token=${sessionToken}`;
/// 
/// // 或使用请求头
/// fetch(thumbnailUrl, {
///   headers: {
///     'X-Session-Token': sessionToken
///   }
/// });
/// ```
```

## Final Correctness Properties (补充)

### Property 35: WAL Mode Concurrency
*For any* concurrent read and write operations on the SQLite database, the WAL mode SHALL allow reads to proceed without blocking on writes.
**Validates: SQLite High Concurrency**

### Property 36: Display Change Recovery
*For any* display configuration change (resolution, monitor add/remove), the WindowsDesktopManager SHALL reattach to WorkerW and resize the window within 1 second.
**Validates: Display Change Handling**

### Property 37: Asset Server Token Validation
*For any* request to the AssetStreamServer without a valid session token, the server SHALL return HTTP 403 Forbidden.
**Validates: Asset Server Security**

### Property 38: CSRF Protection
*For any* request to the AssetStreamServer with an Origin header not in the allowed list, the server SHALL reject the request.
**Validates: Asset Server Security**


## Indexer Resilience (索引器韧性)

### 重试机制与死信队列

```rust
/// 增强的索引任务 (包含重试信息)
#[derive(Debug, Clone)]
pub struct IndexTask {
    /// 任务 ID
    pub id: Uuid,
    
    /// 文件 ID
    pub file_id: Uuid,
    
    /// 文件路径
    pub path: PathBuf,
    
    /// 任务优先级
    pub priority: TaskPriority,
    
    /// 创建时间
    pub created_at: Instant,
    
    /// 重试计数
    pub retry_count: u32,
    
    /// 最大重试次数
    pub max_retries: u32,
    
    /// 下次重试时间 (None 表示立即执行)
    pub next_retry_at: Option<Instant>,
    
    /// 上次失败原因
    pub last_error: Option<IndexError>,
    
    /// 任务状态
    pub status: TaskStatus,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TaskStatus {
    Pending,      // 等待执行
    Processing,   // 正在处理
    Completed,    // 已完成
    Failed,       // 失败 (可重试)
    DeadLetter,   // 死信 (不再重试)
}

impl IndexTask {
    pub fn new(file_id: Uuid, path: PathBuf, priority: TaskPriority) -> Self {
        Self {
            id: Uuid::now_v7(),
            file_id,
            path,
            priority,
            created_at: Instant::now(),
            retry_count: 0,
            max_retries: 5,
            next_retry_at: None,
            last_error: None,
            status: TaskStatus::Pending,
        }
    }
    
    /// 计算下次重试延迟 (指数退避)
    pub fn calculate_retry_delay(&self) -> Duration {
        // 基础延迟: 1秒
        // 指数退避: 1s, 2s, 4s, 8s, 16s
        // 加上随机抖动: ±25%
        let base_delay = Duration::from_secs(1 << self.retry_count.min(4));
        
        let jitter_factor = 0.75 + rand::random::<f64>() * 0.5; // 0.75 - 1.25
        Duration::from_secs_f64(base_delay.as_secs_f64() * jitter_factor)
    }
    
    /// 标记任务失败并安排重试
    pub fn mark_failed(&mut self, error: IndexError) {
        self.retry_count += 1;
        self.last_error = Some(error);
        
        if self.retry_count >= self.max_retries {
            self.status = TaskStatus::DeadLetter;
        } else {
            self.status = TaskStatus::Failed;
            self.next_retry_at = Some(Instant::now() + self.calculate_retry_delay());
        }
    }
    
    /// 检查是否可以执行
    pub fn is_ready(&self) -> bool {
        match self.status {
            TaskStatus::Pending => true,
            TaskStatus::Failed => {
                self.next_retry_at
                    .map(|t| Instant::now() >= t)
                    .unwrap_or(true)
            }
            _ => false,
        }
    }
}

/// 增强的批处理索引器
pub struct ResilientBatchIndexer {
    /// 待处理队列
    pending_queue: Arc<Mutex<VecDeque<IndexTask>>>,
    
    /// 死信队列
    dead_letter_queue: Arc<Mutex<VecDeque<IndexTask>>>,
    
    /// 处理中的任务
    processing: Arc<DashMap<Uuid, IndexTask>>,
    
    /// 嵌入引擎
    embedding_engine: Arc<EmbeddingEngine>,
    
    /// 向量存储
    vector_store: Arc<VectorStore>,
    
    /// 配置
    config: IndexerConfig,
    
    /// 统计信息
    stats: Arc<IndexerStats>,
}

#[derive(Debug, Clone)]
pub struct IndexerConfig {
    /// 批处理大小
    pub batch_size: usize,
    
    /// 最大并发任务数
    pub max_concurrent: usize,
    
    /// 任务超时 (秒)
    pub task_timeout_secs: u64,
    
    /// 死信队列最大大小
    pub dead_letter_max_size: usize,
    
    /// 文件锁定重试间隔 (秒)
    pub file_lock_retry_secs: u64,
}

impl Default for IndexerConfig {
    fn default() -> Self {
        Self {
            batch_size: 10,
            max_concurrent: 4,
            task_timeout_secs: 60,
            dead_letter_max_size: 1000,
            file_lock_retry_secs: 5,
        }
    }
}

#[derive(Debug, Default)]
pub struct IndexerStats {
    pub total_processed: AtomicU64,
    pub total_failed: AtomicU64,
    pub total_dead_letter: AtomicU64,
    pub current_queue_size: AtomicU64,
}

impl ResilientBatchIndexer {
    /// 启动索引循环
    pub async fn start(&self) {
        loop {
            // 1. 收集可执行的任务
            let batch = self.collect_ready_tasks().await;
            
            if batch.is_empty() {
                // 无任务，等待
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            
            // 2. 并行处理任务
            let results = self.process_batch_with_timeout(batch).await;
            
            // 3. 处理结果
            for result in results {
                self.handle_task_result(result).await;
            }
        }
    }
    
    /// 收集可执行的任务
    async fn collect_ready_tasks(&self) -> Vec<IndexTask> {
        let mut queue = self.pending_queue.lock().await;
        let mut batch = Vec::with_capacity(self.config.batch_size);
        
        // 按优先级和重试时间排序
        let mut tasks: Vec<_> = queue.drain(..).collect();
        tasks.sort_by(|a, b| {
            // 优先级高的先执行
            match b.priority.cmp(&a.priority) {
                std::cmp::Ordering::Equal => {
                    // 同优先级，按重试时间排序
                    a.next_retry_at.cmp(&b.next_retry_at)
                }
                other => other,
            }
        });
        
        for task in tasks {
            if task.is_ready() && batch.len() < self.config.batch_size {
                batch.push(task);
            } else {
                queue.push_back(task);
            }
        }
        
        batch
    }
    
    /// 带超时的批处理
    async fn process_batch_with_timeout(
        &self,
        batch: Vec<IndexTask>,
    ) -> Vec<TaskResult> {
        let timeout = Duration::from_secs(self.config.task_timeout_secs);
        
        let futures: Vec<_> = batch.into_iter()
            .map(|task| {
                let engine = self.embedding_engine.clone();
                let store = self.vector_store.clone();
                let task_id = task.id;
                
                // 记录处理中
                self.processing.insert(task_id, task.clone());
                
                async move {
                    let result = tokio::time::timeout(
                        timeout,
                        Self::process_single_task(task.clone(), engine, store),
                    ).await;
                    
                    match result {
                        Ok(Ok(())) => TaskResult::Success(task),
                        Ok(Err(e)) => TaskResult::Failed(task, e),
                        Err(_) => TaskResult::Timeout(task),
                    }
                }
            })
            .collect();
        
        futures::future::join_all(futures).await
    }
    
    /// 处理单个任务
    async fn process_single_task(
        mut task: IndexTask,
        engine: Arc<EmbeddingEngine>,
        store: Arc<VectorStore>,
    ) -> Result<(), IndexError> {
        task.status = TaskStatus::Processing;
        
        // 1. 检查文件是否可访问
        if !task.path.exists() {
            return Err(IndexError::FileNotFound(task.path.clone()));
        }
        
        // 2. 尝试打开文件 (检测锁定)
        let file = match tokio::fs::File::open(&task.path).await {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                // 文件被锁定 (如 Word 正在编辑)
                return Err(IndexError::FileLocked(task.path.clone()));
            }
            Err(e) => {
                return Err(IndexError::IoError(e.to_string()));
            }
        };
        drop(file);
        
        // 3. 读取文件内容
        let content = match Self::read_file_content(&task.path).await {
            Ok(c) => c,
            Err(e) => return Err(e),
        };
        
        // 4. 生成嵌入
        let embedding = engine.embed_file(&task.path).await
            .map_err(|e| IndexError::EmbeddingFailed(e.to_string()))?;
        
        // 5. 存储向量
        store.upsert(VectorPoint {
            id: task.file_id.as_u128() as u64,
            vector: embedding,
            payload: HashMap::new(),
        }).await
        .map_err(|e| IndexError::StorageFailed(e.to_string()))?;
        
        Ok(())
    }
    
    /// 处理任务结果
    async fn handle_task_result(&self, result: TaskResult) {
        match result {
            TaskResult::Success(task) => {
                self.processing.remove(&task.id);
                self.stats.total_processed.fetch_add(1, Ordering::SeqCst);
                tracing::debug!("Indexed: {:?}", task.path);
            }
            
            TaskResult::Failed(mut task, error) => {
                self.processing.remove(&task.id);
                
                // 特殊处理文件锁定错误
                let is_file_locked = matches!(error, IndexError::FileLocked(_));
                
                task.mark_failed(error.clone());
                
                if task.status == TaskStatus::DeadLetter {
                    // 移入死信队列
                    self.move_to_dead_letter(task).await;
                    self.stats.total_dead_letter.fetch_add(1, Ordering::SeqCst);
                } else {
                    // 重新入队
                    if is_file_locked {
                        // 文件锁定，使用固定延迟
                        task.next_retry_at = Some(
                            Instant::now() + Duration::from_secs(self.config.file_lock_retry_secs)
                        );
                    }
                    
                    let mut queue = self.pending_queue.lock().await;
                    queue.push_back(task);
                    
                    self.stats.total_failed.fetch_add(1, Ordering::SeqCst);
                }
            }
            
            TaskResult::Timeout(mut task) => {
                self.processing.remove(&task.id);
                
                task.mark_failed(IndexError::Timeout);
                
                if task.status == TaskStatus::DeadLetter {
                    self.move_to_dead_letter(task).await;
                } else {
                    let mut queue = self.pending_queue.lock().await;
                    queue.push_back(task);
                }
            }
        }
        
        // 更新队列大小统计
        let queue = self.pending_queue.lock().await;
        self.stats.current_queue_size.store(queue.len() as u64, Ordering::SeqCst);
    }
    
    /// 移入死信队列
    async fn move_to_dead_letter(&self, task: IndexTask) {
        let mut dlq = self.dead_letter_queue.lock().await;
        
        // 限制死信队列大小
        while dlq.len() >= self.config.dead_letter_max_size {
            dlq.pop_front();
        }
        
        tracing::warn!(
            "Task moved to dead letter queue: {:?} (retries: {}, error: {:?})",
            task.path,
            task.retry_count,
            task.last_error
        );
        
        dlq.push_back(task);
    }
    
    /// 获取死信队列中的任务 (供 UI 展示)
    pub async fn get_dead_letter_tasks(&self) -> Vec<IndexTask> {
        self.dead_letter_queue.lock().await.iter().cloned().collect()
    }
    
    /// 重试死信队列中的任务
    pub async fn retry_dead_letter_task(&self, task_id: Uuid) -> Result<()> {
        let mut dlq = self.dead_letter_queue.lock().await;
        
        if let Some(pos) = dlq.iter().position(|t| t.id == task_id) {
            let mut task = dlq.remove(pos).unwrap();
            
            // 重置重试计数
            task.retry_count = 0;
            task.status = TaskStatus::Pending;
            task.next_retry_at = None;
            task.last_error = None;
            
            // 重新入队
            let mut queue = self.pending_queue.lock().await;
            queue.push_back(task);
            
            Ok(())
        } else {
            Err(IndexError::TaskNotFound(task_id))
        }
    }
    
    /// 清空死信队列
    pub async fn clear_dead_letter_queue(&self) {
        self.dead_letter_queue.lock().await.clear();
    }
}

#[derive(Debug)]
enum TaskResult {
    Success(IndexTask),
    Failed(IndexTask, IndexError),
    Timeout(IndexTask),
}

/// 扩展的索引错误类型
#[derive(Debug, Clone, thiserror::Error)]
pub enum IndexError {
    #[error("File not found: {0}")]
    FileNotFound(PathBuf),
    
    #[error("File is locked by another process: {0}")]
    FileLocked(PathBuf),
    
    #[error("Unsupported file type: {0}")]
    UnsupportedFileType(String),
    
    #[error("Content extraction failed: {0}")]
    ContentExtractionFailed(String),
    
    #[error("Embedding generation failed: {0}")]
    EmbeddingFailed(String),
    
    #[error("Vector storage failed: {0}")]
    StorageFailed(String),
    
    #[error("IO error: {0}")]
    IoError(String),
    
    #[error("Task timeout")]
    Timeout,
    
    #[error("Task not found: {0}")]
    TaskNotFound(Uuid),
    
    #[error("Index corrupted: {0}")]
    IndexCorrupted(String),
    
    #[error("Queue full")]
    QueueFull,
}

impl IndexError {
    /// 判断错误是否可重试
    pub fn is_retryable(&self) -> bool {
        matches!(self,
            IndexError::FileLocked(_) |
            IndexError::IoError(_) |
            IndexError::Timeout |
            IndexError::StorageFailed(_)
        )
    }
}
```

## Final Correctness Properties (补充)

### Property 39: Exponential Backoff Correctness
*For any* failed IndexTask with retry_count n, the retry delay SHALL be approximately 2^n seconds (with jitter), up to a maximum of 16 seconds.
**Validates: Indexer Resilience**

### Property 40: Dead Letter Queue Bound
*For any* state of the indexer, the dead letter queue size SHALL not exceed dead_letter_max_size.
**Validates: Indexer Resilience**

### Property 41: File Lock Retry Behavior
*For any* IndexTask that fails due to FileLocked error, the task SHALL be retried after file_lock_retry_secs seconds, not using exponential backoff.
**Validates: Indexer Resilience**

### Property 42: Task State Machine Validity
*For any* IndexTask, the status SHALL only transition through valid states: Pending → Processing → {Completed | Failed | DeadLetter}, and Failed → Pending (on retry).
**Validates: Indexer Resilience**
