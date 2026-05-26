# Cavly 命令参考

本文档列出 Cavly 所有 CLI 命令及其参数、选项和示例。

## 全局选项

| 选项 | 说明 |
|---|---|
| `-v`, `--verbose` | 显示详细执行信息 |
| `-V`, `--version` | 显示版本号 |
| `-h`, `--help` | 显示帮助信息 |

---

## `cavly init`

初始化新项目。

```
cavly init [选项] [名称]
```

| 选项 | 说明 |
|---|---|
| `--lib`, `-l` | 创建库项目（而非可执行项目） |

**示例：**

```bash
# 创建可执行项目
cavly init my-app

# 在当前目录创建
cavly init

# 创建库项目
cavly init --lib my-lib
```

**生成的文件：**

| 文件 | 说明 |
|---|---|
| `cavly.toml` | 项目配置（含 `[[bin]]`、`[[test]]`、`build.cay` 模板） |
| `src/main.cay` | 主入口（bin）/ `src/lib.cay`（lib） |
| `tests/test_basic.cay` | 示例测试文件（含 `@Test` 注解示例） |
| `build.cay` | 构建脚本模板 |
| `.gitignore` | Git 忽略规则 |

---

## `cavly build`

构建项目。默认构建所有 `default_build = true` 的二进制目标。

```
cavly build [选项]
```

| 选项 | 说明 |
|---|---|
| `--bin <名称>` | 只构建指定的二进制目标 |
| `-v`, `--verbose` | 显示详细构建信息 |

**示例：**

```bash
# 构建所有默认 bin
cavly build

# 详细模式
cavly build -v

# 只构建指定 bin
cavly build --bin my-tool
```

**构建流程：**

1. 执行 `build.cay`（如果配置了）
2. 构建所有依赖库
3. 编译每个 bin 目标的源文件
4. 调用 cayc → LLVM IR → clang → 可执行文件
5. 如果是 lib 项目，安装库文件到 `target/lib/`

**输出：**

| 项目类型 | 输出位置 |
|---|---|
| Bin | `target/<name>.exe` (Windows) 或 `target/<name>` (Linux) |
| Lib (static) | `target/lib/<name>.lib` (Win) / `target/lib/lib<name>.a` (Linux) |
| Lib (dynamic) | `target/lib/<name>.dll` (Win) / `target/lib/lib<name>.so` (Linux) |

---

## `cavly run`

构建并运行项目。

```
cavly run [选项]
```

| 选项 | 说明 |
|---|---|
| `--bin <名称>` | 运行指定的二进制目标 |
| `-v`, `--verbose` | 显示详细输出 |

**示例：**

```bash
# 构建并运行默认程序
cavly run

# 运行指定的二进制
cavly run --bin my-cli-tool
```

---

## `cavly test`

编译并运行所有测试。

```
cavly test [选项]
```

| 选项 | 说明 |
|---|---|
| `--filter <名称>` | 按测试名称过滤（包含匹配） |
| `-v`, `--verbose` | 显示详细测试执行信息 |

**示例：**

```bash
# 运行所有测试
cavly test

# 只运行名称包含 "basic" 的测试
cavly test --filter basic

# 详细模式
cavly test --verbose
```

**测试发现规则：**

1. 读取 `cavly.toml` 中显式声明的 `[[test]]` 目标
2. 扫描 `tests/` 目录下的所有 `*.cay` 文件
3. 去重：显式声明的路径不会被自动发现重复添加
4. 自动发现的测试默认启用 `harness = true`

**输出示例：**

```
running 3 tests
test BasicTests::testAddition ... ok (12ms)
test BasicTests::testStringOps ... ok (8ms)
test MathTests::testMultiply ... FAILED
  test MathTests::testMultiply ... FAILED
  exit code: 1

test result: FAILED. 2 passed; 1 failed; finished in 0.5s
```

---

## `cavly clean`

清理构建产物。删除 `target/` 目录。

```bash
cavly clean
cavly clean -v   # 详细模式
```

---

## `cavly info`

显示当前项目的详细信息。

```bash
cavly info
```

**输出示例：**

```
项目: my-app (0.1.0)
描述: A Cavvy project
作者: Alice, Bob
许可证: MIT
主文件: src/main.cay [存在]
目标目录: target [空]
系统库: m, pthread
第三方库:
  - SDL2 (SDL2)
```

---

## `cavly add`

添加系统库依赖。

```
cavly add <库名>
```

**示例：**

```bash
# Linux: 添加数学库
cavly add m

# Windows: 添加 user32
cavly add user32
```

这会在 `cavly.toml` 的 `[ffi]` 段中添加 `system_libs = ["m"]`。

---

## `cavly ffi`

添加外部 C 库（FFI）配置。

```
cavly ffi <配置名称> <库名>
```

**示例：**

```bash
cavly ffi sdl2 SDL2
```

这会在 `cavly.toml` 中生成：

```toml
[ffi.libraries.sdl2]
name = "SDL2"
lib = "SDL2"
```

---

## `cavly help`

显示帮助信息。

```bash
cavly help
cavly -h
cavly --help
```
