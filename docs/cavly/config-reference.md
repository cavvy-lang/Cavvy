# cavly.toml 配置参考

`cavly.toml` 是 Cavly 项目的核心配置文件，采用 TOML 格式。本文档列出所有可用配置项及其说明。

## 目录

- [`[package]`](#package) — 包基本信息
- [`[[bin]]`](#bin) — 二进制目标
- [`[[test]]`](#test) — 测试目标
- [`[test-config]`](#test-config) — 测试运行配置
- [`[build]`](#build) — 构建选项
- [`[lib]`](#lib) — 库项目配置
- [`[ffi]``](#ffi) — FFI 外部库
- [`[workspace]`](#workspace) — 工作区
- [`[dependencies]`](#dependencies) — 依赖
- [`[dev-dependencies]`](#dev-dependencies) — 开发依赖

---

## `[package]`

包基本信息。

```toml
[package]
name = "my-project"         # 包名（必填）
version = "0.1.0"           # 版本号（必填，遵循语义化版本）
description = "项目描述"     # 项目描述
authors = ["作者1", "作者2"] # 作者列表
license = "MIT"              # 许可证
project_type = "bin"         # 项目类型: "bin"（可执行）或 "lib"（库）
main = "main.cay"            # 主入口文件（相对于 src_dir）
src_dir = "src"              # 源代码目录
target_dir = "target"        # 构建产物输出目录
build_script = "build.cay"   # 构建脚本路径（可选，设为空字符串禁用）
```

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `name` | String | **是** | — | 包名，只能包含字母、数字、`-`、`_`，不能以数字开头 |
| `version` | String | **是** | — | 语义化版本号 |
| `description` | String | 否 | `""` | 项目描述 |
| `authors` | String[] | 否 | `[]` | 作者列表 |
| `license` | String | 否 | `""` | SPDX 许可证标识 |
| `project_type` | String | 否 | `"bin"` | `"bin"` 或 `"lib"` |
| `main` | String | 否 | `"main.cay"` | bin 项目入口文件，lib 项目为 `"lib.cay"` |
| `src_dir` | String | 否 | `"src"` | 源代码目录 |
| `target_dir` | String | 否 | `"target"` | 输出目录 |
| `build_script` | String | 否 | `""` | 构建脚本路径，空字符串禁用 |

---

## `[[bin]]`

二进制目标配置（数组）。一个项目可定义多个二进制目标，每个编译为独立可执行文件。

```toml
[[bin]]
name = "my-app"             # 二进制名称（必填）
path = "src/main.cay"       # 源文件路径（必填）
default_build = true        # 是否在 cavly build 时默认构建
test = true                 # 是否在 cavly test 时包含此目标
bench = false               # 是否启用基准测试（预留）

# 此二进制的独立构建配置（覆盖全局 [build]）
[bin.build]
opt_level = "3"
debug = true
```

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `name` | String | **是** | — | 输出文件名（不含扩展名） |
| `path` | String | **是** | — | 源文件路径（相对于项目根目录） |
| `default_build` | bool | 否 | `true` | `cavly build` 是否包含此目标 |
| `test` | bool | 否 | `true` | `cavly test` 是否可测试此目标 |
| `bench` | bool | 否 | `false` | 是否启用基准测试（预留） |
| `build` | Table | 否 | 继承全局 | 此目标的独立构建配置，覆盖全局 `[build]` |

**向后兼容**：如果 `[[bin]]` 为空且 `project_type = "bin"`，Cavly 自动将 `package.main` 作为默认的单一二进制目标。

---

## `[[test]]`

测试目标配置（数组）。定义独立的测试入口。

```toml
[[test]]
name = "unit_tests"         # 测试名称（必填）
path = "tests/unit.cay"     # 测试文件路径（必填）
harness = true              # 是否使用内置测试框架
```

| 字段 | 类型 | 必填 | 默认值 | 说明 |
|---|---|---|---|---|
| `name` | String | **是** | — | 测试名称 |
| `path` | String | **是** | — | 测试文件路径 |
| `harness` | bool | 否 | `true` | `true`: 编译器 `--test` 模式（自动调用 `@Test` 方法）；`false`: 作为普通程序编译运行，退出码 0 表示通过 |

**自动发现**：`cavly test` 会自动扫描 `tests/` 目录下的 `*.cay` 文件，即使没有在 `[[test]]` 中显式声明。

---

## `[test-config]`

测试运行配置。

```toml
[test-config]
threads = 0         # 并发线程数（0 = CPU 核心数）
timeout_secs = 0    # 单个测试超时秒数（0 = 无限）
fail_fast = true    # 失败时立即停止
show_output = false # 显示所有测试的输出（包括通过的）
```

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `threads` | usize | CPU 核心数 | 并发运行的测试线程数，`1` 表示串行 |
| `timeout_secs` | u64 | `0` | 单个测试超时时间（秒），`0` 表示无限制 |
| `fail_fast` | bool | `true` | 第一个测试失败后立即停止后续测试 |
| `show_output` | bool | `false` | 是否显示通过的测试的标准输出 |

---

## `[build]`

全局构建配置。bin 目标可通过 `[bin.build]` 覆盖。

```toml
[build]
opt_level = "2"             # 优化级别: 0, 1, 2, 3, s, z
debug = false               # 是否生成调试信息
static_link = false         # 是否静态链接
target = "x86_64-w64-mingw32"  # 目标平台（默认自动检测）
cflags = []                 # 额外编译器标志
ldflags = []                # 额外链接器标志
lib_paths = []              # 库搜索路径
libs = []                   # 要链接的库
lto = false                 # 启用链接时优化
opt_ir = false              # 启用 IR 优化
keep_ir = false             # 保留中间 IR 文件
output_name = ""            # 自定义输出文件名
```

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `opt_level` | String | `"2"` | `"0"`/`"1"`/`"2"`/`"3"`/`"s"`/`"z"` |
| `debug` | bool | `false` | 生成 DWARF 调试信息 |
| `static_link` | bool | `false` | 静态链接所有依赖 |
| `target` | String | 自动检测 | 目标三元组，如 `x86_64-w64-mingw32` |
| `cflags` | String[] | `[]` | 传递给 C 编译器的额外标志 |
| `ldflags` | String[] | `[]` | 传递给链接器的额外标志 |
| `lib_paths` | String[] | `[]` | 额外的库搜索路径 |
| `libs` | String[] | `[]` | 要链接的库名 |
| `lto` | bool | `false` | 启用链接时优化 |
| `opt_ir` | bool | `false` | 在 IR 级别启用优化 |
| `keep_ir` | bool | `false` | 保留 `.ll` 中间文件 |
| `output_name` | String | 包名 | 自定义输出文件名（不含扩展名） |

---

## `[lib]`

库项目专用配置。

```toml
[lib]
lib_type = "static"         # 库类型: "static" 或 "dynamic"
exports = []                # 导出的模块列表（空 = 导出所有 public）
install_path = "lib"        # 安装路径（相对于 target）
only_include = false        # 仅接口模式

[lib.header]
generate = true             # 是否生成 C 头文件
name = "mylib.h"            # 自定义头文件名
include_prefix = ""         # 头文件包含路径前缀
```

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `lib_type` | String | `"static"` | `"static"`（`.lib`/`.a`）或 `"dynamic"`（`.dll`/`.so`） |
| `exports` | String[] | `[]` | 限定导出的模块，空数组导出所有 public |
| `install_path` | String | `"lib"` | 库文件安装目录 |
| `only_include` | bool | `false` | 只做语法/语义检查，不编译链接；供下游项目通过 `-I` 引用 |

---

## `[ffi]`

FFI 外部 C 库配置。

```toml
[ffi]
system_libs = ["m", "pthread"]      # 系统库
include_paths = []                   # 头文件搜索路径
link_options = []                    # 额外链接选项
linker_script = ""                   # 链接器脚本

# 第三方库详细配置
[ffi.libraries.sdl2]
name = "SDL2"
lib = "SDL2"
path = "./lib"
static_lib = false
deps = ["SDL2main"]

# 平台特定配置（windows/linux/macos）
[ffi.libraries.sdl2.platform.windows]
lib = "SDL2"
path = "C:/SDL2/lib"
ldflags = ["-lSDL2main", "-lSDL2"]

[ffi.libraries.sdl2.platform.linux]
lib = "SDL2"
path = "/usr/lib/x86_64-linux-gnu"
```

---

## `[workspace]`

工作区配置，用于管理本地库项目和多项目构建。

```toml
[workspace]
members = ["../mylib", "./libs/helper"]  # 本地库项目路径
lib_paths = ["./lib", "/usr/local/lib"]  # 额外库搜索路径
```

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `members` | String[] | `[]` | 本地依赖库项目路径列表 |
| `lib_paths` | String[] | `[]` | 额外的库文件搜索路径 |
| `default_build` | Table | 无 | 成员项目的默认构建配置 |
| `default_ffi` | Table | 无 | 成员项目的默认 FFI 配置 |

---

## `[dependencies]`

运行时依赖。

```toml
[dependencies]
# 简单版本依赖
example = "1.0.0"

# Git 依赖
mylib = { git = "https://github.com/user/mylib", branch = "main" }

# 本地路径依赖
local-lib = { path = "../local-lib" }

# 可选依赖
optional-dep = { version = "1.0", optional = true }
```

---

## `[dev-dependencies]`

仅开发时使用的依赖（如测试框架），不会传递给下游项目。

```toml
[dev-dependencies]
test-utils = { path = "../test-utils" }
```
