# NeuralFS 开发日志 (Changelog)

## [0.1.0] - 2024-12-30

### 架构设计决策

#### Bounding Box 空间索引策略
- **当前方案**: `content_chunks.bounding_box` 使用 JSON 文本存储 `[x, y, width, height]`
- **查询策略**: 应用层过滤 (Rust 代码中进行区域匹配)
- **原因**: SQLite 原生不支持空间索引，JSON 存储最简单且灵活
- **未来扩展**: 
  - 方案 A: 分离坐标列 + 复合索引
  - 方案 B: SQLite R*Tree 扩展
  - 方案 C: 向量数据库 payload 过滤
- **详见**: `.kiro/specs/neural-fs-core/design.md` - Data Models 章节

---

### Phase 1: 骨架搭建 (Project Skeleton) ✅

#### 1.1 Rust 模块结构
- 创建 `src-tauri/src/core/` 目录结构
- 定义模块: `config`, `error`, `types`, `utils`
- 设置 `mod.rs` 导出

#### 1.2 Cargo.toml 依赖配置
- 添加核心依赖: sqlx, tantivy, jieba-rs, uuid, chrono, serde
- 配置 features: `wal` (SQLite WAL模式)
- 设置编译优化选项

#### 1.3 核心错误类型
- 实现 `NeuralFSError` 枚举
- 实现 `IndexError`, `SearchError`, `CloudError`, `DatabaseError` 等子类型
- 实现 `ErrorRecovery` trait 用于错误恢复策略

#### 1.4-1.5 运行时依赖与构建脚本
- 编写 `src-tauri/build.rs`: 自动复制 deps/ 目录下的 DLL
- 配置 `tauri.conf.json` 的 `externalBin`: 注册 watchdog 为外部二进制
- 创建 `scripts/build-sidecar.ps1` 和 `scripts/build-sidecar.sh`

### Phase 2: 核心数据结构 ✅

#### 2.1 文件记录结构 (FileRecord)
- `src-tauri/src/core/types/file.rs`
- 实现 `FileRecord`, `FileType`, `IndexStatus`, `PrivacyLevel`
- 支持序列化/反序列化

#### 2.2 内容片段结构 (ContentChunk)
- `src-tauri/src/core/types/chunk.rs`
- 实现 `ContentChunk`, `ChunkType`, `ChunkLocation`

#### 2.3 标签系统结构 (Tag)
- `src-tauri/src/core/types/tag.rs`
- 实现 `Tag`, `TagType`, `FileTagRelation`, `TagSource`

#### 2.4 关联系统结构 (FileRelation)
- `src-tauri/src/core/types/relation.rs`
- 实现 `FileRelation`, `RelationType`, `UserFeedback`, `RelationBlockRule`

#### 2.5 搜索类型结构
- `src-tauri/src/core/types/search.rs`
- 实现 `SearchRequest`, `SearchResponse`, `SearchResult`, `SearchFilters`

### Phase 3: 系统霸权 (OS Integration) ✅

#### 4.1-4.4 Watchdog 进程
- `src-tauri/src/bin/watchdog.rs` - 独立可执行文件
- `src-tauri/src/watchdog/` - 心跳检测、共享内存、进程监控
- 跨平台支持 (Windows 命名共享内存 / Unix 文件模拟)

#### 5.1-5.7 Windows 桌面接管
- `src-tauri/src/os/windows/desktop.rs` - WorkerW 挂载
- `src-tauri/src/os/windows/keyboard.rs` - 低级键盘钩子 (Win+D 拦截)
- `src-tauri/src/os/windows/taskbar.rs` - 任务栏控制
- `src-tauri/src/os/windows/monitor.rs` - 多显示器支持
- `src-tauri/src/os/windows/display_listener.rs` - 显示器变更监听
- `src-tauri/src/os/windows/handle_manager.rs` - 窗口句柄生命周期管理

#### 6.1-6.2 系统缩略图提取
- `src-tauri/src/os/thumbnail/` - 缩略图提取模块
- Windows: IShellItemImageFactory
- LRU 缓存 + 磁盘持久化

### Phase 4: 数据底层 (Data Layer) ✅

#### 8.1-8.5 SQLite 数据库
- `src-tauri/src/db/mod.rs` - 数据库连接池 (WAL 模式支持)
- `src-tauri/src/db/migration.rs` - 迁移管理器 (原子事务、回滚支持)
- `src-tauri/migrations/001_initial_schema.sql` - 初始 Schema
- `src-tauri/src/db/tests.rs` - 属性测试 (Property 32, 35)

#### 9.1-9.3 向量数据库 (Qdrant)
- `src-tauri/src/vector/mod.rs` - VectorStore 模块
- `src-tauri/src/vector/store.rs` - 向量存储实现 (CRUD, 搜索, 过滤)
- `src-tauri/src/vector/config.rs` - 配置 (HNSW 索引参数)
- `src-tauri/src/vector/error.rs` - 错误类型
- `src-tauri/src/vector/tests.rs` - 属性测试 (Property 4, 17)

#### 10.1-10.4 全文检索 (Tantivy)
- `src-tauri/src/search/mod.rs` - 搜索模块
- `src-tauri/src/search/tokenizer.rs` - 多语言分词器
  - `JiebaTokenizer` - 中文分词 (jieba-rs)
  - `SimpleTokenizer` - 英文分词
  - `MultilingualTokenizer` - 自动语言检测
- `src-tauri/src/search/text_index.rs` - Tantivy 索引 (Schema 版本控制)
- `src-tauri/src/search/tests.rs` - 属性测试 (Property 31)

### Checkpoint 11: 数据层验证 ✅

通过代码审查验证:
1. **SQLite WAL 模式** - 动态配置，支持高并发读写
2. **数据库迁移** - 原子事务，checksum 验证，回滚支持
3. **向量搜索** - 余弦/欧氏/点积相似度，结果按分数排序
4. **中文全文检索** - jieba-rs 分词，Tantivy 集成

---

## 项目结构

```
src-tauri/
├── src/
│   ├── core/           # 核心模块
│   │   ├── config.rs   # 配置管理
│   │   ├── error.rs    # 错误类型
│   │   ├── types/      # 数据结构
│   │   │   ├── file.rs
│   │   │   ├── chunk.rs
│   │   │   ├── tag.rs
│   │   │   ├── relation.rs
│   │   │   └── search.rs
│   │   └── utils.rs    # 工具函数
│   ├── db/             # 数据库模块
│   │   ├── mod.rs      # 连接池
│   │   ├── migration.rs # 迁移管理
│   │   └── tests.rs    # 属性测试
│   ├── vector/         # 向量存储
│   │   ├── mod.rs
│   │   ├── store.rs    # VectorStore
│   │   ├── config.rs
│   │   ├── error.rs
│   │   └── tests.rs
│   ├── search/         # 全文检索 + 混合搜索
│   │   ├── mod.rs
│   │   ├── tokenizer.rs # 多语言分词
│   │   ├── text_index.rs # Tantivy 索引
│   │   ├── intent.rs    # 意图解析器 ⭐
│   │   ├── hybrid.rs    # 混合搜索引擎 ⭐ (新增)
│   │   └── tests.rs     # 属性测试 (Property 3, 7, 19, 22, 31)
│   ├── embeddings/     # 嵌入引擎
│   │   ├── mod.rs
│   │   ├── model_manager.rs
│   │   ├── vram_manager.rs
│   │   ├── text_embedder.rs
│   │   ├── image_embedder.rs
│   │   ├── diluted.rs   # 稀释注意力
│   │   ├── config.rs
│   │   ├── error.rs
│   │   └── tests.rs
│   ├── inference/      # 混合推理引擎
│   │   ├── mod.rs
│   │   ├── local.rs     # 本地推理
│   │   ├── cloud.rs     # 云端桥接
│   │   ├── hybrid.rs    # 混合推理
│   │   ├── merger.rs    # 结果合并
│   │   ├── anonymizer.rs # 数据匿名化
│   │   ├── types.rs
│   │   ├── error.rs
│   │   └── tests.rs
│   ├── indexer/        # 索引服务
│   │   ├── mod.rs       # ResilientBatchIndexer
│   │   ├── error.rs
│   │   └── tests.rs
│   ├── parser/         # 内容解析器
│   │   ├── mod.rs
│   │   ├── text.rs
│   │   ├── pdf.rs
│   │   ├── code.rs
│   │   └── tests.rs
│   ├── watcher/        # 文件监控
│   │   ├── mod.rs
│   │   ├── filter.rs
│   │   └── tests.rs
│   ├── reconcile/      # 文件对账
│   │   ├── mod.rs
│   │   └── tests.rs
│   ├── os/             # 系统集成
│   │   ├── windows/    # Windows 特定
│   │   │   ├── desktop.rs
│   │   │   ├── keyboard.rs
│   │   │   ├── taskbar.rs
│   │   │   ├── monitor.rs
│   │   │   └── ...
│   │   └── thumbnail/  # 缩略图
│   └── watchdog/       # 进程监控
├── migrations/         # SQL 迁移文件
│   ├── 001_initial_schema.sql
│   └── 002_add_file_id.sql
└── Cargo.toml
```

### Phase 5: 文件感知 (File Awareness) ✅

#### 12.1-12.4 文件监控服务
- `src-tauri/src/watcher/mod.rs` - FileWatcher 增强版
- `src-tauri/src/watcher/filter.rs` - 目录过滤器 (黑名单/白名单)
- `src-tauri/src/watcher/tests.rs` - 属性测试 (Property 33, 34)

#### 13.1-13.3 文件系统对账
- `src-tauri/src/reconcile/mod.rs` - ReconciliationService
- `src-tauri/migrations/002_add_file_id.sql` - FileID 追踪
- `src-tauri/src/reconcile/tests.rs` - 属性测试 (Property 21)

#### 14.1-14.3 内容解析器
- `src-tauri/src/parser/mod.rs` - ContentParser trait
- `src-tauri/src/parser/text.rs` - TXT, MD, JSON 解析
- `src-tauri/src/parser/pdf.rs` - PDF 文本提取
- `src-tauri/src/parser/code.rs` - 代码文件解析 (语法树分析)
- `src-tauri/src/parser/tests.rs` - 单元测试

#### 15.1-15.4 索引服务
- `src-tauri/src/indexer/mod.rs` - ResilientBatchIndexer
- `src-tauri/src/indexer/error.rs` - 索引错误类型
- `src-tauri/src/indexer/tests.rs` - 属性测试 (Property 39-42)
- 实现指数退避重试、死信队列、文件锁定处理

### Phase 6: AI 推理引擎 (AI Inference) ✅

#### 17.1-17.6 嵌入引擎
- `src-tauri/src/embeddings/mod.rs` - 嵌入引擎模块
- `src-tauri/src/embeddings/model_manager.rs` - ModelManager (懒加载)
- `src-tauri/src/embeddings/vram_manager.rs` - VRAMManager (LRU 缓存)
- `src-tauri/src/embeddings/text_embedder.rs` - 文本嵌入 (all-MiniLM-L6-v2)
- `src-tauri/src/embeddings/image_embedder.rs` - 图像嵌入 (CLIP)
- `src-tauri/src/embeddings/config.rs` - 配置
- `src-tauri/src/embeddings/error.rs` - 错误类型
- `src-tauri/src/embeddings/tests.rs` - 属性测试 (Property 6)

#### 18.1-18.2 稀释注意力
- `src-tauri/src/embeddings/diluted.rs` - DilutedAttentionProcessor
- 滑动窗口 + 全局上下文处理长文档
- 属性测试 (Property 5)

#### 19.1-19.2 意图解析器
- `src-tauri/src/search/intent.rs` - IntentParser
- 文件级/段落级意图识别
- 支持中英文查询
- 属性测试 (Property 3)

#### 20.1-20.6 混合推理引擎
- `src-tauri/src/inference/mod.rs` - 推理模块
- `src-tauri/src/inference/local.rs` - LocalInferenceEngine
- `src-tauri/src/inference/cloud.rs` - CloudBridge (速率限制、成本追踪)
- `src-tauri/src/inference/anonymizer.rs` - 数据匿名化
- `src-tauri/src/inference/merger.rs` - ResultMerger
- `src-tauri/src/inference/hybrid.rs` - HybridInferenceEngine
- `src-tauri/src/inference/types.rs` - 类型定义
- `src-tauri/src/inference/error.rs` - 错误类型
- `src-tauri/src/inference/tests.rs` - 属性测试 (Property 11-13)

---

### Phase 6: 搜索与标签 (Search & Tags) - 进行中

#### 22. 混合搜索引擎 ✅

##### 22.1 实现混合搜索
- **文件**: `src-tauri/src/search/hybrid.rs`
- **实现内容**:
  - `HybridSearchEngine` 结构体 - 组合向量搜索和 BM25 搜索
  - `HybridSearchConfig` - 可配置的权重和阈值
    - `vector_weight`: 向量搜索权重 (默认 0.6)
    - `bm25_weight`: BM25 搜索权重 (默认 0.4)
    - `exact_match_boost`: 精确匹配加分 (默认 2.0)
    - `filename_match_boost`: 文件名匹配加分 (默认 1.5)
  - `merge_results()` - 加权分数合并与归一化
  - `apply_exact_match_boost()` - 文件名和标签匹配加分
  - `filter_by_score()` - 按分数阈值过滤
  - `limit_results()` - 限制结果数量

##### 22.2 实现查询类型分类
- **文件**: `src-tauri/src/search/hybrid.rs`
- **实现内容**:
  - `QueryType` 枚举: `ExactKeyword`, `NaturalLanguage`, `Mixed`
  - `classify_query()` 函数 - 查询类型分类
  - **ExactKeyword 检测**:
    - 十六进制错误码 (如 `0x80070005`)
    - 长数字序列 (如 `12345678`)
    - 全大写常量 (如 `ERROR_ACCESS_DENIED`)
    - 文件名模式 (如 `report.pdf`)
    - 引号包围的精确搜索
    - 路径模式 (如 `C:\Users\test`)
  - **NaturalLanguage 检测**:
    - 多词查询 (≥3 个词)
    - 疑问词开头 (what, where, how 等)
    - 描述性短语 (find, search, show me 等)
    - 中文查询支持 (找, 搜索, 查找 等)
  - `get_adjusted_weights()` - 根据查询类型调整权重

##### 22.3 实现搜索过滤
- **文件**: `src-tauri/src/search/hybrid.rs`
- **实现内容**:
  - `HybridSearchFilters` 结构体:
    - `file_types`: 文件类型过滤
    - `tag_ids`: 标签 ID 过滤 (AND 逻辑)
    - `exclude_tag_ids`: 排除标签
    - `time_range`: 时间范围过滤
    - `min_score`: 最小分数阈值
    - `exclude_private`: 排除私密文件
    - `path_prefix`: 路径前缀过滤
  - `to_vector_filter()` - 转换为向量存储过滤器
  - `to_text_filter()` - 转换为文本索引过滤器
  - `apply_filters()` - 应用过滤器到结果

##### 22.4 编写属性测试: 搜索结果正确性
- **文件**: `src-tauri/src/search/tests.rs`
- **Property 19: Search Filter Correctness**
  - 验证所有过滤后的结果满足过滤条件
  - 验证分数阈值过滤正确性
  - **Validates: Requirements 2.2, 2.3**
- **Property 22: Hybrid Search Score Normalization**
  - 验证权重之和为 1.0
  - 验证合并后分数在 [0, 1] 范围内
  - 验证结果按分数降序排列
  - 验证双来源结果标记为 `SearchSource::Both`
  - **Validates: Requirements 2.2, Hybrid Search Logic**

##### 22.5 编写属性测试: 搜索延迟
- **文件**: `src-tauri/src/search/tests.rs`
- **Property 7: Search Latency Bound (Fast Mode)**
  - 验证核心搜索操作在 50ms 内完成 (为 200ms 快速模式留余量)
  - 验证查询分类在 1ms 内完成
  - 验证结果合并延迟与结果数量线性相关
  - **Validates: Requirements 4.8**

---

## 下一步计划 (Phase 6 继续)

- [ ] 23. 标签管理系统 (TagManager)
- [ ] 24. 逻辑链条引擎 (LogicChainEngine)
- [ ] 25. Checkpoint - 搜索与标签验证

## 运行测试

```bash
cd src-tauri
cargo test --lib db::      # 数据库测试
cargo test --lib vector::  # 向量存储测试
cargo test --lib search::  # 搜索测试
```

## 注意事项

1. **Rust 工具链**: 需要安装 Rust (https://rustup.rs/)
2. **WAL 模式**: 通过 Cargo feature `wal` 启用
3. **跨平台**: Windows 特定功能在非 Windows 平台使用 stub 实现
