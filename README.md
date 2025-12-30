# NeuralFS

AI 驱动的沉浸式文件系统外壳 - 将传统的"基于路径的存储"转变为"基于意图的检索"。

## 项目概述

NeuralFS 是一个本地 AI 驱动的桌面替代应用，提供：
- 🔍 **语义搜索** - 使用自然语言描述查找文件
- 🏷️ **智能标签** - AI 自动分类和标签管理
- 🔗 **逻辑链条** - 智能关联相关文件
- 🖥️ **桌面接管** - 替代传统桌面环境
- 🎮 **游戏模式** - 自动检测全屏应用并释放资源

## 技术栈

| 层级 | 技术 | 说明 |
|------|------|------|
| 前端 | Tauri + SolidJS | 原生性能，跨平台 |
| 后端 | Rust | 内存安全，高性能 |
| AI 推理 | ONNX Runtime | 本地推理，CUDA 支持 |
| 向量存储 | Qdrant (嵌入式) | 语义搜索 |
| 全文检索 | Tantivy | 中文分词支持 |

## 项目结构

```
src-tauri/
├── src/
│   ├── core/           # 核心模块
│   │   ├── config.rs   # 配置管理
│   │   ├── error.rs    # 错误处理
│   │   ├── types/      # 数据类型定义
│   │   └── utils.rs    # 工具函数
│   ├── watchdog/       # 进程监控
│   │   ├── heartbeat.rs    # 心跳检测
│   │   ├── shared_memory.rs # 共享内存
│   │   └── supervisor.rs   # 进程重启
│   ├── os/             # OS 集成层
│   │   ├── windows/    # Windows 特定实现
│   │   │   ├── desktop.rs      # WorkerW 挂载
│   │   │   ├── keyboard.rs     # 快捷键拦截
│   │   │   ├── taskbar.rs      # 任务栏控制
│   │   │   ├── monitor.rs      # 多显示器
│   │   │   └── display_listener.rs # 显示器变更
│   │   └── stub.rs     # 非 Windows 平台桩
│   └── bin/
│       └── watchdog.rs # Watchdog 可执行文件
├── Cargo.toml
└── tauri.conf.json
```

## 开发进度

### Phase 1: 骨架搭建 ✅
- [x] 项目结构与基础配置
- [x] 核心数据结构定义
- [x] 错误处理系统

### Phase 2: 系统霸权 🚧
- [x] Watchdog 进程
- [x] Windows 桌面接管
- [ ] 系统缩略图提取

### Phase 3-10: 待实现
详见 `.kiro/specs/neural-fs-core/tasks.md`

## 构建

```bash
# 安装依赖
cargo build

# 运行测试
cargo test

# 构建发布版本
cargo build --release
```

## 许可证

MIT License
