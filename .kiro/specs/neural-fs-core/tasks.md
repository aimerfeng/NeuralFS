# Implementation Plan: NeuralFS Core

## Overview

本任务列表按照分层架构组织，从底层基础设施到上层功能逐步构建。每个阶段都有明确的检查点，确保增量验证。

## Phase 1: 骨架搭建 (Project Skeleton)

- [x] 1. 项目结构与基础配置
  - [x] 1.1 创建 Rust 模块结构
    - 创建 `src-tauri/src/core/` 目录结构
    - 定义模块: `config`, `error`, `types`, `utils`
    - 设置 `mod.rs` 导出
    - _Requirements: 项目架构_

  - [x] 1.2 配置 Cargo.toml 依赖
    - 添加所有必需依赖 (sqlx, qdrant-client, ort, tantivy, jieba-rs, etc.)
    - 配置 features (cuda, wal)
    - 设置编译优化选项
    - _Requirements: 技术栈确认_

  - [x] 1.3 实现核心错误类型
    - 创建 `src-tauri/src/core/error.rs`
    - 实现 `NeuralFSError` 枚举
    - 实现 `IndexError`, `SearchError`, `CloudError` 等子类型
    - 实现 `ErrorRecovery` trait
    - _Requirements: Error Handling_

  - [x] 1.4 编写错误类型单元测试
    - 测试错误转换
    - 测试 `is_retryable()` 逻辑
    - _Requirements: Error Handling_

  - [x] 1.5 运行时依赖与构建脚本配置
    - 编写 `src-tauri/build.rs`：自动复制 deps/ 目录下的 DLL 到 target
    - 配置 `tauri.conf.json` 的 `externalBin`：注册 watchdog 为外部二进制
    - **重要**: Tauri Sidecar 要求严格的命名规则，需确保构建步骤：
      1. 编译 watchdog 二进制
      2. 重命名为平台特定格式 (如 `watchdog-x86_64-pc-windows-msvc.exe`)
      3. 移动到 `src-tauri/binaries/` 目录
    - 配置 ONNX Runtime DLL 路径搜索顺序：
      ```rust
      let onnx_paths = [
          "deps/onnxruntime",           // 项目本地
          "C:/Program Files/onnxruntime", // 系统安装
          // ... 其他路径
      ];
      ```
    - 实现 `RuntimeDependencies::check_cuda()` 桩代码 (无显卡 CI 环境兼容)
    - _Requirements: Installer Specification, DLL Side-loading_

- [x] 2. 核心数据结构定义
  - [x] 2.1 实现文件记录结构
    - 创建 `src-tauri/src/core/types/file.rs`
    - 实现 `FileRecord`, `FileType`, `IndexStatus`, `PrivacyLevel`
    - 实现序列化/反序列化
    - _Requirements: 1.3, 1.4_

  - [x] 2.2 实现内容片段结构
    - 创建 `src-tauri/src/core/types/chunk.rs`
    - 实现 `ContentChunk`, `ChunkType`, `ChunkLocation`
    - _Requirements: 3.2_

  - [x] 2.3 实现标签系统结构
    - 创建 `src-tauri/src/core/types/tag.rs`
    - 实现 `Tag`, `TagType`, `FileTagRelation`, `TagSource`
    - _Requirements: 5.1, 5.2_

  - [x] 2.4 实现关联系统结构
    - 创建 `src-tauri/src/core/types/relation.rs`
    - 实现 `FileRelation`, `RelationType`, `UserFeedback`, `RelationBlockRule`
    - _Requirements: 6.1, Human-in-the-Loop_

  - [x] 2.5 编写属性测试: 数据结构序列化往返
    - **Property 17: Vector Database Serialization Round-Trip**
    - **Property 18: FileRecord Serialization Round-Trip**
    - **Validates: Requirements 21, 22**

- [x] 2.6 创建 Sidecar 构建脚本
  - 创建 `scripts/build-sidecar.ps1` (Windows) 或 `scripts/build-sidecar.sh` (Unix)
  - 实现 watchdog 编译和重命名逻辑：
    ```powershell
    # Windows 示例
    cargo build --release --bin watchdog
    $target = "x86_64-pc-windows-msvc"
    Copy-Item "target/release/watchdog.exe" "src-tauri/binaries/watchdog-$target.exe"
    ```
  - 集成到 `tauri build` 前置步骤 (可通过 npm scripts 或 Makefile)
  - _Requirements: Process Supervisor, Installer Specification_

- [x] 3. Checkpoint - 骨架验证
  - 确保所有数据结构编译通过
  - 运行单元测试和属性测试
  - 确保 cargo clippy 无警告


## Phase 2: 系统霸权 (OS Integration)

- [x] 4. Watchdog 进程
  - [x] 4.1 创建独立 Watchdog 可执行文件
    - 创建 `src-tauri/src/bin/watchdog.rs`
    - 实现 `Watchdog` 结构体
    - 实现心跳检测循环
    - _Requirements: Process Supervisor_

  - [x] 4.2 实现共享内存心跳通信 (含跨平台 Mock)
    - 实现 `SharedMemory` trait (跨平台抽象)
    - Windows: 使用 `CreateFileMappingW` 命名共享内存
    - 非 Windows (macOS/Linux): 实现基于文件的 Mock 共享内存
    - 关键：确保开发团队可在非 Windows 机器上编译和测试
    - 实现心跳读写
    - _Requirements: Process Supervisor_

  - [x] 4.3 实现进程重启与 Explorer 恢复
    - 实现 `start_main_process()`
    - 实现 `restore_windows_explorer()`
    - 实现错误通知
    - _Requirements: Process Supervisor_

  - [x] 4.4 编写属性测试: Watchdog 心跳可靠性
    - **Property 26: Watchdog Heartbeat Reliability**
    - **Validates: Process Supervisor**

- [x] 5. Windows 桌面接管
  - [x] 5.1 实现 WorkerW 挂载
    - 创建 `src-tauri/src/os/windows/desktop.rs`
    - 实现 `WindowsDesktopManager`
    - 实现 `take_over_desktop()` - WorkerW 挂载
    - _Requirements: 1.1, OS Integration Layer_

  - [x] 5.2 实现快捷键拦截
    - 实现低级键盘钩子
    - 拦截 Win+D
    - 实现自定义快捷键处理
    - _Requirements: 1.5_

  - [x] 5.3 实现任务栏控制
    - 实现 `hide_taskbar()` / `restore_taskbar()`
    - 处理 Shell_TrayWnd
    - _Requirements: OS Integration Layer_

  - [x] 5.4 实现多显示器支持
    - 实现 `setup_multi_monitor()`
    - 实现 `MultiMonitorStrategy` 枚举
    - 处理显示器枚举
    - _Requirements: OS Integration Layer_

  - [x] 5.5 实现显示器变更监听
    - 监听 WM_DISPLAYCHANGE
    - 监听 WM_DEVICECHANGE
    - 实现 `handle_display_change()`
    - _Requirements: Display Change Handling_

  - [x] 5.6 编写属性测试: 显示器变更恢复
    - **Property 36: Display Change Recovery**
    - **Validates: Display Change Handling**

  - [x] 5.7 窗口句柄生命周期管理
    - 实现 `WindowHandleManager`
    - 处理 Tauri 窗口句柄与 Win32 HWND 的映射
    - 追踪 Webview 重建时的 HWND 变化
    - 确保 SetParent 始终使用最新 HWND
    - _Requirements: OS Integration Layer_

- [x] 6. 系统缩略图提取
  - [x] 6.1 实现 Windows 缩略图提取
    - 创建 `src-tauri/src/os/thumbnail.rs`
    - 实现 `ThumbnailExtractor`
    - 使用 IShellItemImageFactory
    - _Requirements: OS Integration Layer_

  - [x] 6.2 实现缩略图缓存
    - 实现 LRU 缓存
    - 实现磁盘持久化
    - _Requirements: 7.1_

- [x] 7. Checkpoint - 系统集成验证
  - 验证桌面接管功能
  - 验证 Watchdog 进程
  - 测试多显示器场景
  - 确保 Explorer 恢复正常工作


## Phase 3: 数据底层 (Data Layer)

- [x] 8. SQLite 数据库
  - [x] 8.1 实现数据库连接池
    - 创建 `src-tauri/src/db/mod.rs`
    - 实现 `DatabaseConfig`
    - 实现 `create_database_pool()` with WAL 模式
    - 根据 Cargo.toml 的 `wal` feature 动态配置 journal_mode:
      ```rust
      .journal_mode(if cfg!(feature = "wal") { SqliteJournalMode::Wal } else { SqliteJournalMode::Delete })
      ```
    - _Requirements: SQLite High Concurrency_

  - [x] 8.2 创建数据库 Schema
    - 创建 `src-tauri/migrations/` 目录
    - 创建 `001_initial_schema.sql`
    - 实现所有表结构 (files, content_chunks, tags, file_tags, file_relations, etc.)
    - _Requirements: Data Models_

  - [x] 8.3 实现迁移管理器
    - 创建 `src-tauri/src/db/migration.rs`
    - 实现 `MigrationManager`
    - 实现自动迁移和回滚
    - _Requirements: Schema Migration_

  - [x] 8.4 编写属性测试: 迁移原子性
    - **Property 32: Migration Atomicity**
    - **Validates: Schema Migration**

  - [x] 8.5 编写属性测试: WAL 并发性
    - **Property 35: WAL Mode Concurrency**
    - **Validates: SQLite High Concurrency**

- [x] 9. 向量数据库 (Qdrant)
  - [x] 9.1 实现 Qdrant 嵌入式初始化 (含锁文件处理)
    - 创建 `src-tauri/src/vector/mod.rs`
    - 实现 `VectorStore` 结构体
    - 配置 HNSW 索引
    - 实现启动时清除残留 `.lock` 文件 (Qdrant 常见问题)
    - 配置 Qdrant 日志输出到应用日志系统 (默认 stdout 会被 Tauri 吞掉)
    - _Requirements: 21.1, 21.2_

  - [x] 9.2 实现向量 CRUD 操作
    - 实现 `upsert()`, `search()`, `delete()`
    - 实现批量操作
    - 实现过滤查询
    - _Requirements: 21.3, 21.4_

  - [x] 9.3 编写属性测试: 向量搜索正确性
    - **Property 4: Search Result Ordering**
    - **Validates: Requirements 2.2, 2.3**

- [x] 10. 全文检索 (Tantivy)
  - [x] 10.1 实现多语言分词器
    - 创建 `src-tauri/src/search/tokenizer.rs`
    - 实现 `JiebaTokenizer` (中文)
    - 实现 `LinderaTokenizer` (日文)
    - 实现 `MultilingualTokenizer`
    - _Requirements: Tokenizer Strategy, 19.2_

  - [x] 10.2 实现 Tantivy 索引
    - 创建 `src-tauri/src/search/text_index.rs`
    - 实现 `TextIndex` 结构体
    - 注册多语言分词器
    - _Requirements: Hybrid Search Logic_

  - [x] 10.3 编写属性测试: 中文分词质量
    - **Property 31: Chinese Tokenization Quality**
    - **Validates: Tokenizer Strategy, Requirements 19**

  - [x] 10.4 索引版本控制
    - 在 `TextIndex` 中实现 Schema 版本检查
    - 检测 Tantivy Schema 变更 (字段增删)
    - 实现不兼容变更时的重建索引逻辑
    - _Requirements: Schema Migration (Tantivy)_

- [x] 11. Checkpoint - 数据层验证
  - 验证 SQLite WAL 模式
  - 验证数据库迁移
  - 验证向量搜索
  - 验证中文全文检索


## Phase 4: 文件感知 (File Awareness)

- [ ] 12. 文件监控服务
  - [ ] 12.1 实现文件监控器
    - 更新 `src-tauri/src/watcher/mod.rs`
    - 实现 `FileWatcher` 增强版
    - 实现事件去重和节流
    - _Requirements: 8.1, 8.2_

  - [ ] 12.2 实现目录过滤器
    - 创建 `src-tauri/src/watcher/filter.rs`
    - 实现 `DirectoryFilter`
    - 实现黑名单/白名单匹配
    - _Requirements: Directory Blacklist_

  - [ ] 12.3 编写属性测试: 目录过滤有效性
    - **Property 33: Directory Filter Effectiveness**
    - **Property 34: Large Directory Protection**
    - **Validates: Directory Blacklist**

  - [ ] 12.4 文件系统事件去重压力测试
    - 编写集成测试：模拟 1 秒内 1000 个文件变更事件
    - 验证 FileWatcher 正确合并为 Batch 事件
    - 验证 CPU 使用率不超过阈值
    - _Requirements: 8.6, Directory Blacklist_

- [ ] 13. 文件系统对账
  - [ ] 13.1 实现对账服务
    - 创建 `src-tauri/src/reconcile/mod.rs`
    - 实现 `ReconciliationService`
    - 实现启动时 Diff 算法
    - _Requirements: Reconciliation Strategy_

  - [ ] 13.2 实现 FileID 追踪
    - 实现 Windows `GetFileInformationByHandle`
    - 实现 Unix inode 追踪
    - 实现重命名检测
    - _Requirements: Reconciliation Strategy_

  - [ ] 13.3 编写属性测试: 重命名追踪
    - **Property 21: File ID Tracking Across Renames**
    - **Validates: Reconciliation Strategy**

- [ ] 14. 内容解析器
  - [ ] 14.1 实现文本内容提取
    - 创建 `src-tauri/src/parser/mod.rs`
    - 实现 `ContentParser` trait
    - 实现 TXT, MD, JSON 解析
    - _Requirements: 22.1_

  - [ ] 14.2 实现 PDF 解析
    - 实现 PDF 文本提取
    - 实现页码定位
    - _Requirements: 22.1_

  - [ ] 14.3 实现代码文件解析
    - 实现语法树分析
    - 实现函数/类提取
    - _Requirements: 22.4_

- [ ] 15. 索引服务
  - [ ] 15.1 实现韧性索引器
    - 更新 `src-tauri/src/indexer/mod.rs`
    - 实现 `ResilientBatchIndexer`
    - 实现 `IndexTask` 增强版
    - _Requirements: Indexer Resilience_

  - [ ] 15.2 实现指数退避重试 (含文件句柄泄露检测)
    - 实现 `calculate_retry_delay()`
    - 实现文件锁定特殊处理
    - 确保 `File::open` 失败时显式 drop 文件句柄 (防止句柄泄露)
    - Rationale: Windows 上即使打开失败有时也会短暂持有句柄，导致连续重试失败
    - _Requirements: Indexer Resilience_

  - [ ] 15.3 实现死信队列
    - 实现 `dead_letter_queue`
    - 实现手动重试
    - 实现队列大小限制
    - _Requirements: Indexer Resilience_

  - [ ] 15.4 编写属性测试: 索引器韧性
    - **Property 39: Exponential Backoff Correctness**
    - **Property 40: Dead Letter Queue Bound**
    - **Property 41: File Lock Retry Behavior**
    - **Property 42: Task State Machine Validity**
    - **Validates: Indexer Resilience**

- [ ] 16. Checkpoint - 文件感知验证
  - 验证文件监控
  - 验证目录过滤
  - 验证对账服务
  - 验证索引器重试机制
  - 测试 node_modules 等大目录


## Phase 5: AI 推理引擎 (AI Inference)

- [ ] 17. 嵌入引擎
  - [ ] 17.1 实现 ONNX 模型加载
    - 更新 `src-tauri/src/embeddings/mod.rs`
    - 实现 `ModelManager`
    - 实现模型懒加载
    - _Requirements: 23.1_

  - [ ] 17.2 实现 VRAM 管理
    - 实现 `VRAMManager`
    - 实现 LRU 模型缓存
    - 实现模型卸载
    - _Requirements: 4.1, 4.2_

  - [ ] 17.3 实现文本嵌入
    - 实现 all-MiniLM-L6-v2 推理
    - 实现批量嵌入
    - _Requirements: 3.5_

  - [ ] 17.4 实现图像嵌入
    - 实现 CLIP 模型推理
    - 实现图像预处理
    - _Requirements: 3.3_

  - [ ] 17.5 编写属性测试: VRAM 使用限制
    - **Property 6: VRAM Usage Bound**
    - **Validates: Requirements 4.1**

  - [ ] 17.6 模型加载状态机
    - 实现 `ModelLoadingState` (Missing, Downloading, Loading, Ready, Failed)
    - 在 `EmbeddingEngine` 中处理 "模型未就绪" 时的请求
    - 实现降级处理策略 (返回空结果而非崩溃)
    - _Requirements: Installer Specification, 首次启动_

- [ ] 18. 稀释注意力
  - [ ] 18.1 实现长文档处理
    - 创建 `src-tauri/src/embeddings/diluted.rs`
    - 实现 `DilutedAttentionProcessor`
    - 实现滑动窗口 + 全局上下文
    - _Requirements: 4.2_

  - [ ] 18.2 编写属性测试: 内容片段覆盖
    - **Property 5: Chunk Coverage Invariant**
    - **Validates: Requirements 3.2**

- [ ] 19. 意图解析器
  - [ ] 19.1 实现意图分类
    - 创建 `src-tauri/src/search/intent.rs`
    - 实现 `IntentParser`
    - 实现文件级/段落级意图识别
    - _Requirements: 2.1_

  - [ ] 19.2 编写属性测试: 意图分类有效性
    - **Property 3: Intent Classification Validity**
    - **Validates: Requirements 2.1**

- [ ] 20. 混合推理引擎
  - [ ] 20.1 实现本地推理引擎
    - 创建 `src-tauri/src/inference/local.rs`
    - 实现 `LocalInferenceEngine`
    - 实现上下文增强提示词生成
    - _Requirements: 11.2, 11.4_

  - [ ] 20.2 实现云端桥接
    - 创建 `src-tauri/src/inference/cloud.rs`
    - 实现 `CloudBridge`
    - 实现速率限制和成本追踪
    - _Requirements: 11.6, 11.7_

  - [ ] 20.3 实现数据匿名化
    - 实现 `anonymize_prompt()`
    - 移除敏感路径和用户名
    - _Requirements: 13.2_

  - [ ] 20.4 实现结果合并器
    - 创建 `src-tauri/src/inference/merger.rs`
    - 实现 `ResultMerger`
    - 实现分数加权合并
    - _Requirements: 11.5_

  - [ ] 20.5 实现并行推理调度
    - 创建 `src-tauri/src/inference/hybrid.rs`
    - 实现 `HybridInferenceEngine`
    - 实现本地+云端并行调度
    - _Requirements: 11.1_

  - [ ] 20.6 编写属性测试: 并行推理与数据安全
    - **Property 11: Parallel Inference Dispatch**
    - **Property 12: Cache Hit Consistency**
    - **Property 13: Data Anonymization**
    - **Validates: Requirements 11, 13**

- [ ] 21. Checkpoint - AI 推理验证
  - 验证嵌入生成
  - 验证 VRAM 限制
  - 验证意图解析
  - 验证云端调用
  - 测试 200ms 响应时间


## Phase 6: 搜索与标签 (Search & Tags)

- [ ] 22. 混合搜索引擎
  - [ ] 22.1 实现混合搜索
    - 创建 `src-tauri/src/search/hybrid.rs`
    - 实现 `HybridSearchEngine`
    - 实现向量搜索 + BM25 融合
    - _Requirements: Hybrid Search Logic_

  - [ ] 22.2 实现查询类型分类
    - 实现 `classify_query()`
    - 识别精确关键词/自然语言/混合
    - _Requirements: Hybrid Search Logic_

  - [ ] 22.3 实现搜索过滤
    - 实现 `SearchFilters`
    - 实现标签过滤、时间范围、文件类型
    - _Requirements: 2.2, 2.3_

  - [ ] 22.4 编写属性测试: 搜索结果正确性
    - **Property 19: Search Filter Correctness**
    - **Property 22: Hybrid Search Score Normalization**
    - **Validates: Requirements 2.2, 2.3**

  - [ ] 22.5 编写属性测试: 搜索延迟
    - **Property 7: Search Latency Bound (Fast Mode)**
    - **Validates: Requirements 4.8**

- [ ] 23. 标签管理系统
  - [ ] 23.1 实现标签管理器
    - 创建 `src-tauri/src/tag/mod.rs`
    - 实现 `TagManager`
    - 实现自动标签生成
    - _Requirements: 5.1_

  - [ ] 23.2 实现标签层级
    - 实现父子标签关系
    - 实现多维度导航
    - _Requirements: 5.2, 5.6_

  - [ ] 23.3 实现标签修正 API
    - 实现 `TagCommand` 处理
    - 实现确认/拒绝/手动添加
    - _Requirements: Human-in-the-Loop_

  - [ ] 23.4 实现敏感标签检测
    - 实现 `SensitiveTagDetector`
    - 实现需确认标签标记
    - _Requirements: UI/UX Design_

  - [ ] 23.5 编写属性测试: 标签系统
    - **Property 8: Tag Assignment Completeness**
    - **Property 9: Tag Hierarchy Depth Bound**
    - **Property 24: Sensitive Tag Confirmation Requirement**
    - **Validates: Requirements 5**

- [ ] 24. 逻辑链条引擎
  - [ ] 24.1 实现关联引擎
    - 创建 `src-tauri/src/relation/mod.rs`
    - 实现 `LogicChainEngine`
    - 实现内容相似度关联
    - _Requirements: 6.1_

  - [ ] 24.2 实现会话追踪
    - 实现 `SessionTracker`
    - 记录同会话打开的文件
    - _Requirements: 6.2_

  - [ ] 24.3 实现关联修正 API
    - 实现 `RelationCommand` 处理
    - 实现确认/拒绝/屏蔽
    - _Requirements: Human-in-the-Loop_

  - [ ] 24.4 实现屏蔽规则
    - 实现 `RelationBlockRule` 存储
    - 实现屏蔽规则应用
    - _Requirements: Human-in-the-Loop_

  - [ ] 24.5 编写属性测试: 关联系统
    - **Property 10: Relation Symmetry**
    - **Property 14: User Feedback State Machine**
    - **Property 15: Block Rule Enforcement**
    - **Property 16: Rejection Learning Effect**
    - **Validates: Requirements 6, Human-in-the-Loop**

- [ ] 25. Checkpoint - 搜索与标签验证
  - 验证混合搜索
  - 验证标签自动生成
  - 验证关联推荐
  - 验证人工修正功能


## Phase 7: 视觉预览 (Visual Preview)

- [ ] 26. 资源流服务
  - [ ] 26.1 实现安全资源服务器
    - 创建 `src-tauri/src/asset/mod.rs`
    - 实现 `SecureAssetStreamServer`
    - 实现会话令牌验证
    - _Requirements: Asset Streaming, Asset Server Security_

  - [ ] 26.2 实现 CSRF 防护
    - 实现 Origin/Referer 检查
    - 实现安全响应头
    - _Requirements: Asset Server Security_

  - [ ] 26.3 实现缩略图路由
    - 实现 `/thumbnail/:uuid` 路由
    - 实现缓存策略
    - _Requirements: 7.1_

  - [ ] 26.4 编写属性测试: 资源服务安全
    - **Property 27: Asset Streaming Performance**
    - **Property 37: Asset Server Token Validation**
    - **Property 38: CSRF Protection**
    - **Validates: Asset Server Security**

- [ ] 27. 文件预览生成
  - [ ] 27.1 实现文本预览
    - 实现文本片段提取
    - 实现高亮标记
    - _Requirements: 7.1_

  - [ ] 27.2 实现图片预览
    - 实现图片缩放
    - 实现区域标记
    - _Requirements: 7.4_

  - [ ] 27.3 实现文档预览
    - 实现 PDF 页面渲染
    - 实现段落定位
    - _Requirements: 7.3_

- [ ] 28. 高亮导航器
  - [ ] 28.1 实现高亮导航
    - 创建 `src-tauri/src/highlight/mod.rs`
    - 实现 `HighlightNavigator`
    - 实现文件打开和定位
    - _Requirements: 7.2, 7.3_

  - [ ] 28.2 实现应用启动
    - 实现系统默认应用打开
    - 实现 "打开方式" 菜单
    - _Requirements: 14.1, 14.3_

- [ ] 29. Checkpoint - 视觉预览验证
  - 验证资源服务安全
  - 验证缩略图生成
  - 验证文件预览
  - 验证高亮导航

## Phase 8: 游戏模式与更新 (Game Mode & Updates)

- [ ] 30. 游戏模式检测
  - [ ] 30.1 实现系统活动监控
    - 创建 `src-tauri/src/os/activity.rs`
    - 实现 `SystemActivityMonitor`
    - 实现全屏应用检测
    - _Requirements: Game Mode Detection_

  - [ ] 30.2 实现游戏模式策略
    - 实现 `GameModePolicy`
    - 实现 VRAM 释放
    - 实现索引暂停
    - _Requirements: Game Mode Detection_

  - [ ] 30.3 编写属性测试: 游戏模式检测
    - **Property 28: Game Mode Detection Accuracy**
    - **Validates: Game Mode Detection**

- [ ] 31. 模型下载器
  - [ ] 31.1 实现模型下载管理
    - 创建 `src-tauri/src/update/model.rs`
    - 实现 `ModelDownloader`
    - 实现多源下载
    - _Requirements: Installer Specification_

  - [ ] 31.2 实现断点续传
    - 实现 Range 请求
    - 实现校验和验证
    - _Requirements: Installer Specification_

  - [ ] 31.3 编写属性测试: 模型下载完整性
    - **Property 23: Model Download Integrity**
    - **Validates: Installer Specification**

- [ ] 32. 自更新系统
  - [ ] 32.1 实现更新检查
    - 创建 `src-tauri/src/update/self_update.rs`
    - 实现 `SelfUpdater`
    - 实现版本检查
    - _Requirements: Self-Update Strategy_

  - [ ] 32.2 实现 Swap & Restart
    - 实现更新脚本生成
    - 实现 Watchdog 协调
    - _Requirements: Self-Update Strategy_

  - [ ] 32.3 编写属性测试: 更新原子性
    - **Property 29: Update Atomicity**
    - **Property 30: Watchdog Recovery Guarantee**
    - **Validates: Self-Update Strategy**

- [ ] 33. Checkpoint - 游戏模式与更新验证
  - 验证游戏模式检测
  - 验证模型下载
  - 验证自更新流程


## Phase 9: Tauri IPC 与前端集成 (Frontend Integration)

- [ ] 34. Tauri Commands
  - [ ] 34.1 实现搜索命令
    - 更新 `src-tauri/src/main.rs`
    - 实现 `search_files` 命令
    - 实现 `get_search_suggestions` 命令
    - _Requirements: 2.1, 2.2_

  - [ ] 34.2 实现标签命令
    - 实现 `get_tags`, `add_tag`, `remove_tag`
    - 实现 `confirm_tag`, `reject_tag`
    - _Requirements: 5.1, Human-in-the-Loop_

  - [ ] 34.3 实现关联命令
    - 实现 `get_relations`, `confirm_relation`, `reject_relation`
    - 实现 `block_relation`
    - _Requirements: 6.1, Human-in-the-Loop_

  - [ ] 34.4 实现配置命令
    - 实现 `get_config`, `set_config`
    - 实现 `get_cloud_status`, `set_cloud_enabled`
    - _Requirements: 15.1, 15.2_

  - [ ] 34.5 实现状态命令
    - 实现 `get_index_status`, `get_system_status`
    - 实现 `get_dead_letter_tasks`, `retry_dead_letter`
    - _Requirements: 16.1, Indexer Resilience_

- [ ] 35. Custom Protocol 注册
  - [ ] 35.1 注册 nfs:// 协议
    - 实现 `register_custom_protocol()`
    - 实现缩略图/预览路由
    - _Requirements: Asset Streaming_

  - [ ] 35.2 实现安全握手流程 (前后端 Token 传递)
    - 后端：生成 Session Token 并存储在内存
    - 实现 `get_session_token` Tauri 命令
    - 前端：在 App 挂载时 (onMount) 调用 `invoke('get_session_token')`
    - 前端：配置 Axios/Fetch 拦截器，自动将 Token 注入所有 nfs:// 或 http://localhost 请求头
    - 关键：确保握手在任何图片加载之前完成，否则首屏全是 403 Forbidden
    - _Requirements: Asset Server Security_

- [ ] 36. 前端组件更新
  - [ ] 36.1 更新 SearchBar 组件
    - 实现意图提示
    - 实现澄清选项
    - _Requirements: 2.6, 10.1_

  - [ ] 36.2 更新 FileGrid 组件
    - 实现标签显示 (已确认/建议)
    - 实现关联展示
    - _Requirements: UI/UX Design_

  - [ ] 36.3 实现 TagPanel 组件
    - 实现标签层级导航
    - 实现多维度筛选
    - _Requirements: 5.2, 5.6_

  - [ ] 36.4 实现 RelationGraph 组件
    - 实现关联可视化
    - 实现一键解除关联
    - _Requirements: 6.1, Human-in-the-Loop_

- [ ] 37. Checkpoint - 前端集成验证
  - 验证所有 Tauri 命令
  - 验证 Custom Protocol
  - 验证前端组件交互

## Phase 10: 首次启动与配置 (Onboarding & Config)

- [ ] 38. 首次启动引导
  - [ ] 38.1 实现引导向导
    - 创建引导页面组件
    - 实现目录选择
    - 实现云端配置
    - _Requirements: 17.1, 17.2, 17.3_

  - [ ] 38.2 实现初始扫描
    - 实现后台扫描
    - 实现进度显示
    - _Requirements: 17.4, 17.5_

- [ ] 39. 配置管理
  - [ ] 39.1 实现配置存储
    - 创建 `src-tauri/src/config/mod.rs`
    - 实现 JSON 配置文件
    - 实现配置迁移
    - _Requirements: 15.7_

  - [ ] 39.2 实现设置界面
    - 实现监控目录配置
    - 实现云端 API 配置
    - 实现主题切换
    - _Requirements: 15.1, 15.2, 15.3_

- [ ] 40. 日志与遥测
  - [ ] 40.1 实现日志系统
    - 配置 tracing-subscriber
    - 实现日志轮转
    - 实现日志导出
    - _Requirements: 24.1, 24.2, 24.5_

  - [ ] 40.2 实现遥测系统 (可选)
    - 实现匿名统计
    - 实现用户同意流程
    - _Requirements: 24.3, 24.4, 24.6_

- [ ] 41. Final Checkpoint - 完整功能验证
  - 运行所有属性测试
  - 验证完整用户流程
  - 验证错误恢复
  - 性能基准测试

## Notes

- 所有任务均为必需，包括属性测试和单元测试
- 每个 Checkpoint 确保增量验证
- 属性测试验证正确性属性
- 单元测试验证边界情况
