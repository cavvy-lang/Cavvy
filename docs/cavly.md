# Cavly 包管理器

`cavly` 是 Cavvy 的包管理器和项目构建工具。它负责初始化项目、解析依赖、构建二进制目标、运行测试和配置 FFI 库。

Cavly 的实现位于 `src/cavly/`，包含 6 个模块：
- `mod.rs` — 入口和命令行解析
- `config.rs` — 配置类型（`PackageConfig`、`BuildConfig`、`FfiConfig`、`Dependency`、`WorkspaceConfig`、`LibConfig`）
- `builder.rs` — 构建状态机，依赖解析和拓扑排序
- `project.rs` — 项目创建和模板生成
- `ffi.rs` — FFI 库检测和绑定生成
- `tester.rs` — 测试运行器
- `workspace.rs` — 工作区管理

---

## 常用命令

```powershell
# 创建新项目
cavly new my-project
cavly new --lib my-library

# 在当前目录初始化
cavly init
cavly init --lib

# 构建
cavly build
cavly build --bin app
cavly build --release

# 运行
cavly run
cavly run -- "arg1" "arg2"

# 测试
cavly test
cavly test --verbose

# 依赖管理
cavly add some-lib
cavly add some-lib@1.0.0
cavly remove some-lib
cavly install          # 安装所有依赖

# FFI 配置
cavly ffi sdl2 SDL2   # 配置 SDL2 FFI 绑定

# 工作区
cavly workspace init

# 清理
cavly clean

# 发布
cavly publish
```

---

## 项目结构

Cavly 通过向上查找 `cavly.toml` 识别项目根目录。

### 默认项目布局

```
my-project/
├── cavly.toml          # 项目配置文件
├── src/
│   ├── main.cay        # 主入口
│   └── lib.cay         # 库入口（可选）
├── caylibs/            # 项目级标准库
├── tests/              # 测试文件
├── examples/           # 示例文件
├── ffi/                # FFI 绑定
└── target/             # 构建产物
```

---

## 配置文件（cavly.toml）

```toml
[package]
name = "my-project"
version = "0.1.0"
edition = "2024"
description = "我的 Cavvy 项目"

[build]
target = "bin"              # bin | lib | both
optimization = 2            # 0-3

[dependencies]
some-lib = "1.0.0"
another = { git = "https://...", branch = "main" }

[ffi]
sdl2 = { libs = ["SDL2"], include = ["SDL2/SDL.h"] }

[workspace]
members = ["sub-project1", "sub-project2"]
```

---

## 依赖管理系统

- **语义化版本**：支持 `^1.0.0`、`~1.0.0`、`>=1.0.0` 等范围
- **依赖解析**：拓扑排序解决依赖图
- **git 依赖**：直接从 git 仓库拉取
- **工作区**：多项目共享依赖配置

---

## 构建流程

```
cavly build
  → 读取 cavly.toml
  → 解析依赖关系（拓扑排序）
  → 编译依赖库
  → 编译主项目
  → 链接 FFI 库
  → 输出可执行文件或库
```

`builder.rs` 实现完整的构建状态机，处理依赖顺序和并行构建机会。

---

## 项目模板

`cavly new` 和 `cavly init` 从 `project.rs` 中的模板生成项目骨架：

```cay
// src/main.cay（默认模板）
class Main {
    static void main() {
        println("Hello from Cavvy!");
    }
}
```
