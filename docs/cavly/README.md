# Cavly — Cavvy 包管理器

Cavly 是 Cavvy 语言的官方包管理器和构建工具，提供项目初始化、依赖管理、编译构建、测试运行、FFI 配置等一站式开发体验。设计上参考了 Rust 的 Cargo，融入 Cavvy 语言自身的特点。

## 快速开始

```bash
# 创建新项目
cavly init hello-cavvy
cd hello-cavvy

# 构建项目
cavly build

# 构建并运行
cavly run

# 运行测试
cavly test
```

初始化后项目结构：

```
hello-cavvy/
├── cavly.toml          # 项目配置文件
├── build.cay           # 构建脚本（可选）
├── src/
│   └── main.cay        # 主入口文件
├── tests/
│   └── test_basic.cay  # 示例测试文件
└── .gitignore
```

## 核心功能

| 功能 | 命令 | 说明 |
|---|---|---|
| 项目初始化 | `cavly init [名称]` | 创建 bin 或 lib 项目 |
| 构建 | `cavly build` | 编译所有二进制目标 |
| 运行 | `cavly run` | 构建并运行 |
| 测试 | `cavly test` | 发现、编译、运行测试 |
| 清理 | `cavly clean` | 删除构建产物 |
| 信息 | `cavly info` | 显示项目信息 |
| 依赖 | `cavly add <库>` | 添加系统库 |
| FFI | `cavly ffi <名称> <库>` | 配置 FFI 外部库 |

## 项目类型

Cavly 支持两种项目类型：

- **Bin（可执行项目）**：编译为 `.exe`（Windows）或 ELF（Linux）可执行文件
- **Lib（库项目）**：编译为静态库 `.lib`/`.a` 或动态库 `.dll`/`.so`

```bash
cavly init my-app           # 创建可执行项目
cavly init --lib my-lib     # 创建库项目
```

## 文件约定

| 文件/目录 | 用途 |
|---|---|
| `cavly.toml` | 项目配置（包信息、构建选项、依赖、FFI） |
| `build.cay` | 构建脚本（在编译前自动执行） |
| `src/main.cay` | 可执行项目默认入口 |
| `src/lib.cay` | 库项目默认入口 |
| `tests/` | 测试文件目录（自动发现 `*.cay` 文件） |
| `target/` | 构建产物输出目录 |
| `target/tests/` | 编译后的测试可执行文件 |

## 多二进制目标

一个项目可以包含多个可执行入口（类似 Cargo 的 `[[bin]]`）：

```toml
# cavly.toml
[[bin]]
name = "my-app"
path = "src/main.cay"

[[bin]]
name = "my-cli-tool"
path = "src/bin/cli.cay"
default_build = true

[[bin]]
name = "my-bench"
path = "src/bin/bench.cay"
default_build = false   # 不包含在默认构建中
```

```bash
cavly build                    # 构建所有 default_build = true 的 bin
cavly build --bin my-cli-tool  # 只构建指定 bin
cavly run --bin my-cli-tool    # 运行指定 bin
```

## 测试系统

Cavly 内置测试框架，支持两种测试模式：

### 1. @Test 注解模式（推荐）

```cay
// tests/my_tests.cay
public class MyTests {
    @Test
    public static void testAddition() {
        int result = 1 + 1;
        if (result != 2) {
            println("FAILED: testAddition");
        }
    }

    @Test
    public static void testStringOps() {
        String s = "Hello, Cavvy!";
        if (s.length() != 13) {
            println("FAILED: testStringOps");
        }
    }
}
```

### 2. 约定模式

在 `tests/` 目录下放置任意 `.cay` 文件，`cavly test` 自动发现并编译运行。程序 `exit(0)` 表示通过。

```bash
cavly test                      # 运行所有测试
cavly test --filter addition    # 按名称过滤
cavly test --verbose            # 显示详细输出
```

## 构建脚本

`build.cay` 在主项目编译前自动执行，可用于代码生成、下载依赖等：

```cay
// build.cay
public class BuildScript {
    public static void main() {
        // 环境变量: OUT_DIR, PROJECT_ROOT, PROFILE, OPT_LEVEL, TARGET
        println("Running build script...");
        // 在此执行代码生成等任务
    }
}
```

## 详细文档

- [配置参考](./config-reference.md) — `cavly.toml` 完整配置项说明
- [命令参考](./commands.md) — 所有 CLI 命令详解
- [测试系统](./test-system.md) — 测试编写与运行完整指南
- [构建脚本](./build-scripts.md) — `build.cay` 深入指南
- [依赖与工作区](./dependencies.md) — 依赖管理、本地工作区
- [FFI 配置](./ffi-config.md) — 外部 C 库链接配置
