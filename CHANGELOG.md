# NeuralFS 开发日志 (Changelog)

## [0.1.0] - 2024-12-30

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
│   ├── search/         # 全文检索
│   │   ├── mod.rs
│   │   ├── tokenizer.rs # 多语言分词
│   │   ├── text_index.rs # Tantivy 索引
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
└── Cargo.toml
```

## 下一步计划 (Phase 4: 文件感知)

- [ ] 12. 文件监控服务 (FileWatcher)
- [ ] 13. 文件系统对账 (Reconciliation)
- [ ] 14. 内容解析器 (ContentParser)
- [ ] 15. 索引服务 (ResilientBatchIndexer)
- [ ] 16. Checkpoint - 文件感知验证

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
