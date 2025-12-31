# NeuralFS 开发日志 (Changelog)

## [0.1.0] - 2024-12-31 🎉 完整版本发布

### 🚀 项目完成状态

**所有 10 个开发阶段已全部完成！**

| 阶段 | 名称 | 状态 |
|------|------|------|
| Phase 1 | 骨架搭建 (Project Skeleton) | ✅ 完成 |
| Phase 2 | 系统霸权 (OS Integration) | ✅ 完成 |
| Phase 3 | 数据底层 (Data Layer) | ✅ 完成 |
| Phase 4 | 文件感知 (File Awareness) | ✅ 完成 |
| Phase 5 | AI 推理引擎 (AI Inference) | ✅ 完成 |
| Phase 6 | 搜索与标签 (Search & Tags) | ✅ 完成 |
| Phase 7 | 视觉预览 (Visual Preview) | ✅ 完成 |
| Phase 8 | 游戏模式与更新 (Game Mode & Updates) | ✅ 完成 |
| Phase 9 | 前端集成 (Frontend Integration) | ✅ 完成 |
| Phase 10 | 首次启动与配置 (Onboarding & Config) | ✅ 完成 |

### 📊 代码统计

- **91 个文件** 新增/修改
- **25,904 行代码** 新增
- **30+ 属性测试** 实现
- **20+ 模块** 完成

---

## 架构设计决策

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
│   │   ├── intent.rs    # 意图解析器
│   │   ├── hybrid.rs    # 混合搜索引擎 ⭐
│   │   └── tests.rs     # 属性测试 (Property 3, 7, 19, 22, 31)
│   ├── tag/            # 标签管理系统 ⭐ (Phase 6 新增)
│   │   ├── mod.rs       # 模块导出
│   │   ├── manager.rs   # TagManager - 标签 CRUD 和自动标签
│   │   ├── hierarchy.rs # TagHierarchy - 标签层级管理
│   │   ├── correction.rs # TagCorrectionService - 人工修正 API
│   │   ├── sensitive.rs # SensitiveTagDetector - 敏感标签检测
│   │   ├── error.rs     # 错误类型
│   │   └── tests.rs     # 属性测试 (Property 8, 9, 24)
│   ├── relation/       # 逻辑链条引擎 ⭐ (Phase 6 新增)
│   │   ├── mod.rs       # 模块导出
│   │   ├── engine.rs    # LogicChainEngine - 关联管理
│   │   ├── session.rs   # SessionTracker - 会话追踪
│   │   ├── correction.rs # RelationCorrectionService - 人工修正 API
│   │   ├── block_rules.rs # BlockRuleStore - 屏蔽规则
│   │   ├── error.rs     # 错误类型
│   │   └── tests.rs     # 属性测试 (Property 10, 14, 15, 16)
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
│   ├── 002_add_file_id.sql
│   └── 003_add_session_columns.sql  # ⭐ Phase 6 新增
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

### Phase 6: 搜索与标签 (Search & Tags) ✅

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

#### 23. 标签管理系统 ✅

##### 23.1 实现标签管理器
- **文件**: `src-tauri/src/tag/manager.rs`
- **实现内容**:
  - `TagManager` 结构体 - 标签 CRUD 操作
  - `TagManagerConfig` - 配置 (最小置信度、最大自动标签数)
  - `create_tag()` - 创建标签 (名称验证、重复检查)
  - `get_tag()` / `get_tag_by_name()` - 查询标签
  - `update_tag()` / `delete_tag()` - 更新/删除标签
  - `add_tag_to_file()` / `remove_tag_from_file()` - 文件-标签关联
  - `get_file_tags()` / `get_files_by_tag()` - 关联查询
  - `auto_tag_file()` - 自动标签生成
    - 基于文件扩展名分配文件类型标签
    - 基于内容关键词分析分配分类标签
    - 敏感标签检测与确认要求
  - `suggest_tags()` - 标签建议 (不自动应用)

##### 23.2 实现标签层级
- **文件**: `src-tauri/src/tag/hierarchy.rs`
- **实现内容**:
  - `TagHierarchy` 结构体 - 标签层级管理
  - `get_path()` - 获取标签路径 (从根到当前)
  - `get_depth()` - 获取标签深度
  - `get_children()` - 获取子标签
  - `get_ancestors()` - 获取祖先标签
  - `set_parent()` - 设置父标签 (深度验证)
  - `get_stats()` - 层级统计信息
  - **深度限制**: 最大 3 层 (0, 1, 2)

##### 23.3 实现标签修正 API
- **文件**: `src-tauri/src/tag/correction.rs`
- **实现内容**:
  - `TagCommand` 枚举 - 标签修正命令
    - `ConfirmTag` - 确认 AI 生成的标签
    - `RejectTag` - 拒绝标签 (可选屏蔽类似标签)
    - `AddTag` / `RemoveTag` - 手动添加/移除标签
    - `BatchTag` - 批量标签操作
    - `CreateTag` - 创建新标签
    - `MergeTags` - 合并多个标签
    - `RenameTag` / `DeleteTag` - 重命名/删除标签
    - `SetTagParent` - 设置标签父级
  - `TagCorrectionService` - 执行修正命令
  - `TagCorrectionResult` - 操作结果
  - `get_tag_preferences()` - 获取用户标签偏好

##### 23.4 实现敏感标签检测
- **文件**: `src-tauri/src/tag/sensitive.rs`
- **实现内容**:
  - `SensitiveTagDetector` 结构体
  - `SensitivityLevel` 枚举: `None`, `Low`, `Medium`, `High`
  - `check_sensitivity()` - 检查标签敏感度
  - `analyze()` - 详细敏感度分析
  - **敏感关键词类别**:
    - 个人信息: personal, private, confidential
    - 财务信息: bank, account, tax, salary
    - 医疗信息: medical, health, diagnosis
    - 法律信息: legal, contract, nda

##### 23.5 编写属性测试: 标签系统
- **文件**: `src-tauri/src/tag/tests.rs`
- **Property 8: Tag Assignment Completeness**
  - 验证每个索引文件至少分配一个标签
  - **Validates: Requirements 5.1**
- **Property 9: Tag Hierarchy Depth Bound**
  - 验证标签层级深度不超过 3 层
  - **Validates: Requirements 5.7**
- **Property 24: Sensitive Tag Confirmation Requirement**
  - 验证敏感标签需要用户确认
  - **Validates: Requirements 5.5, 13.4, UI/UX Design**

---

#### 24. 逻辑链条引擎 ✅

##### 24.1 实现关联引擎
- **文件**: `src-tauri/src/relation/engine.rs`
- **实现内容**:
  - `LogicChainEngine` 结构体 - 文件关联管理
  - `LogicChainConfig` - 配置
    - `min_similarity_threshold`: 最小相似度阈值 (默认 0.5)
    - `max_related_files`: 最大关联文件数 (默认 10)
    - `content_similarity_weight`: 内容相似度权重 (默认 0.6)
    - `session_weight`: 会话权重 (默认 0.4)
    - `time_decay_factor`: 时间衰减因子 (默认 0.99)
  - `create_relation()` - 创建关联 (验证、屏蔽规则检查)
  - `get_relation()` / `get_relation_between()` - 查询关联
  - `get_relations_for_file()` - 获取文件的所有关联
  - `update_relation()` / `delete_relation()` - 更新/删除关联
  - `find_similar_files()` - 基于向量相似度查找相似文件
  - `generate_content_relations()` - 自动生成内容关联
  - `calculate_combined_score()` - 计算综合分数 (含时间衰减)

##### 24.2 实现会话追踪
- **文件**: `src-tauri/src/relation/session.rs`
- **实现内容**:
  - `SessionTracker` 结构体 - 会话追踪
  - `SessionConfig` - 配置 (会话超时、最小文件数)
  - `start_session()` / `end_session()` - 会话生命周期
  - `record_file_access()` - 记录文件访问
  - `get_session_files()` - 获取会话中的文件
  - `generate_session_relations()` - 生成会话关联
  - **数据库迁移**: `src-tauri/migrations/003_add_session_columns.sql`

##### 24.3 实现关联修正 API
- **文件**: `src-tauri/src/relation/correction.rs`
- **实现内容**:
  - `RelationCommand` 枚举 - 关联修正命令
    - `Confirm` - 确认关联有效
    - `Reject` - 拒绝关联 (一键解除)
    - `Adjust` - 调整关联强度
    - `Create` - 手动创建关联
    - `BatchReject` - 批量拒绝
  - `BlockScope` 枚举 - 屏蔽范围
    - `ThisPairOnly` - 仅屏蔽当前文件对
    - `SourceToTargetTag` - 屏蔽源文件与目标标签
    - `TagToTag` - 屏蔽标签对
  - `RelationCorrectionService` - 执行修正命令
  - `validate_feedback_transition()` - 状态机验证

##### 24.4 实现屏蔽规则
- **文件**: `src-tauri/src/relation/block_rules.rs`
- **实现内容**:
  - `BlockRuleStore` 结构体 - 屏蔽规则存储
  - `create_file_pair_rule()` - 创建文件对屏蔽规则
  - `create_file_to_tag_rule()` - 创建文件-标签屏蔽规则
  - `create_tag_pair_rule()` - 创建标签对屏蔽规则
  - `create_file_all_ai_rule()` - 屏蔽文件的所有 AI 关联
  - `is_blocked()` - 检查关联是否被屏蔽
  - `get_rules_for_file()` - 获取文件的屏蔽规则
  - `delete_rule()` / `deactivate_rule()` - 删除/停用规则

##### 24.5 编写属性测试: 关联系统
- **文件**: `src-tauri/src/relation/tests.rs`
- **Property 10: Relation Symmetry**
  - 验证关联的对称性 (A→B 可从 A 和 B 两侧查询)
  - **Validates: Requirements 6.1**
- **Property 14: User Feedback State Machine**
  - 验证用户反馈状态转换的有效性
  - 有效转换: None→Any, Confirmed→Rejected/Adjusted, Rejected→Confirmed
  - **Validates: Human-in-the-Loop**
- **Property 15: Block Rule Enforcement**
  - 验证屏蔽规则正确阻止关联
  - 测试 FilePair, FileAllAI, RelationType 规则
  - **Validates: Human-in-the-Loop**
- **Property 16: Rejection Learning Effect**
  - 验证拒绝关联时 block_similar=true 会创建屏蔽规则
  - 验证被拒绝关联的有效强度为 0
  - **Validates: Human-in-the-Loop**

---

#### 25. Checkpoint - 搜索与标签验证 ✅

##### 验证内容
1. **混合搜索** ✅
   - 查询分类 (ExactKeyword/NaturalLanguage/Mixed)
   - 分数归一化与加权合并
   - 搜索过滤 (文件类型、标签、时间范围)
   - 延迟测试 (核心操作 < 50ms)

2. **标签自动生成** ✅
   - 基于文件扩展名的类型标签
   - 基于内容关键词的分类标签
   - 敏感标签检测与确认要求
   - 标签层级管理 (最大 3 层)

3. **关联推荐** ✅
   - 内容相似度关联
   - 会话追踪关联
   - 时间衰减计算
   - 屏蔽规则执行

4. **人工修正功能** ✅
   - 标签确认/拒绝/批量操作
   - 关联确认/拒绝/强度调整
   - 状态机验证
   - 屏蔽规则创建

##### 属性测试覆盖
| Property | 描述 | 文件 |
|----------|------|------|
| Property 3 | Intent Classification Validity | `search/tests.rs` |
| Property 7 | Search Latency Bound | `search/tests.rs` |
| Property 8 | Tag Assignment Completeness | `tag/tests.rs` |
| Property 9 | Tag Hierarchy Depth Bound | `tag/tests.rs` |
| Property 10 | Relation Symmetry | `relation/tests.rs` |
| Property 14 | User Feedback State Machine | `relation/tests.rs` |
| Property 15 | Block Rule Enforcement | `relation/tests.rs` |
| Property 16 | Rejection Learning Effect | `relation/tests.rs` |
| Property 19 | Search Filter Correctness | `search/tests.rs` |
| Property 22 | Hybrid Search Score Normalization | `search/tests.rs` |
| Property 24 | Sensitive Tag Confirmation | `tag/tests.rs` |
| Property 31 | Chinese Tokenization Quality | `search/tests.rs` |

---

## Phase 7: 视觉预览 (Visual Preview) ✅

### 26. 资源流服务
- `src-tauri/src/asset/mod.rs` - 资源服务模块
- `src-tauri/src/asset/server.rs` - SecureAssetStreamServer
- `src-tauri/src/asset/routes.rs` - HTTP 路由
- `src-tauri/src/asset/error.rs` - 错误类型
- `src-tauri/src/asset/tests.rs` - 属性测试 (Property 27, 37, 38)

### 27. 文件预览生成
- `src-tauri/src/preview/mod.rs` - 预览服务模块
- `src-tauri/src/preview/text.rs` - 文本预览生成
- `src-tauri/src/preview/image.rs` - 图片预览生成
- `src-tauri/src/preview/document.rs` - 文档预览生成
- `src-tauri/src/preview/tests.rs` - 单元测试

### 28. 高亮导航器
- `src-tauri/src/highlight/mod.rs` - 高亮导航模块
- `src-tauri/src/highlight/navigator.rs` - HighlightNavigator
- `src-tauri/src/highlight/launcher.rs` - AppLauncher
- `src-tauri/src/highlight/tests.rs` - 单元测试

---

## Phase 8: 游戏模式与更新 (Game Mode & Updates) ✅

### 30. 游戏模式检测
- `src-tauri/src/os/activity.rs` - SystemActivityMonitor
- GameModePolicy - VRAM 释放、索引暂停
- 属性测试 (Property 28)

### 31. 模型下载器
- `src-tauri/src/update/model.rs` - ModelDownloader
- 断点续传、校验和验证
- 属性测试 (Property 23)

### 32. 自更新系统
- `src-tauri/src/update/self_update.rs` - SelfUpdater
- Swap & Restart 机制
- 属性测试 (Property 29, 30)

---

## Phase 9: 前端集成 (Frontend Integration) ✅

### 34. Tauri Commands
- `src-tauri/src/commands/search.rs` - 搜索命令
- `src-tauri/src/commands/tags.rs` - 标签命令
- `src-tauri/src/commands/relations.rs` - 关联命令
- `src-tauri/src/commands/config.rs` - 配置命令
- `src-tauri/src/commands/status.rs` - 状态命令
- `src-tauri/src/commands/protocol.rs` - 协议命令
- `src-tauri/src/commands/onboarding.rs` - 引导命令

### 35. Custom Protocol 注册
- `src-tauri/src/protocol/mod.rs` - 协议模块
- `src-tauri/src/protocol/handler.rs` - nfs:// 协议处理
- `src-tauri/src/protocol/tests.rs` - 单元测试

### 36. 前端组件
- `src/components/SearchBar/` - 搜索栏组件
- `src/components/FileGrid/` - 文件网格组件
- `src/components/TagPanel/` - 标签面板组件
- `src/components/RelationGraph/` - 关联图谱组件
- `src/components/Settings/` - 设置面板组件
- `src/components/OnboardingWizard/` - 首次启动向导

---

## Phase 10: 首次启动与配置 (Onboarding & Config) ✅

### 38. 首次启动引导
- `src/components/OnboardingWizard/` - 引导向导组件
- 目录选择、云端配置、初始扫描

### 39. 配置管理
- `src-tauri/src/config/mod.rs` - 配置模块
- `src-tauri/src/config/storage.rs` - JSON 配置存储
- `src-tauri/src/config/migration.rs` - 配置迁移
- `src-tauri/src/config/tests.rs` - 单元测试

### 40. 日志与遥测
- `src-tauri/src/logging/mod.rs` - 日志系统
- `src-tauri/src/logging/rotation.rs` - 日志轮转
- `src-tauri/src/logging/export.rs` - 日志导出
- `src-tauri/src/logging/metrics.rs` - 性能指标
- `src-tauri/src/telemetry/mod.rs` - 遥测系统
- `src-tauri/src/telemetry/collector.rs` - 数据收集
- `src-tauri/src/telemetry/consent.rs` - 用户同意管理

### 41. Final Checkpoint ✅
- 所有属性测试验证
- 完整用户流程验证
- 错误恢复验证
- 性能基准测试

---

## 下一步计划

项目已完成所有核心功能开发，后续计划：

1. **性能优化** - 针对大规模文件库的优化
2. **UI 美化** - 完善视觉设计和动画效果
3. **多平台测试** - macOS 和 Linux 平台适配
4. **文档完善** - API 文档和用户手册
5. **发布准备** - 打包、签名、分发

## 运行测试

```bash
cd src-tauri

# 数据库测试
cargo test --lib db::

# 向量存储测试
cargo test --lib vector::

# 搜索测试 (包含 Property 3, 7, 19, 22, 31)
cargo test --lib search::

# 标签系统测试 (包含 Property 8, 9, 24)
cargo test --lib tag::

# 关联系统测试 (包含 Property 10, 14, 15, 16)
cargo test --lib relation::

# 运行所有测试
cargo test --lib
```

## 注意事项

1. **Rust 工具链**: 需要安装 Rust (https://rustup.rs/)
2. **WAL 模式**: 通过 Cargo feature `wal` 启用
3. **跨平台**: Windows 特定功能在非 Windows 平台使用 stub 实现
4. **属性测试**: 使用 proptest 库，每个属性测试运行 100 次
