# AGENTS.md — Cavvy 编译器项目

## 项目标识

Cavvy（Cay）是一个静态类型、面向对象的编程语言编译器，使用 Rust（2024 edition）编写。它将 `.cay`/`.eol` 源码通过 LLVM IR → clang 编译为原生机器码。采用 GPL3 许可证。

- **真实版本**（来自 `.verinfo` 与 `Cargo.toml`）：`6.2.0`。
- **旧名称**：该项目曾用名 "EOL"（Ethernos Object Language）。`.eol` 扩展名和 CI 中的 `eol-*` 产物名称均为历史遗留。规范扩展名是 `.cay`。

## 构建

```bash
cargo build --release     # release 是正常模式（仅在 release 模式下才会复制捆绑工具）
```

仓库在 `llvm-minimal/`、`mingw-minimal/`、`lib/` 下捆绑了 LLVM、MinGW 和链接库。这些目录**必须存在于磁盘上**，`cargo build` 才能成功 —— `build.rs` 会将它们复制到 `target/<profile>/`。如果缺失，请检查 `.gitignore` —— 它们可能被 git 忽略，需要先通过 `setup-llvm.py` 下载。

`setup-llvm.py` 从 GitHub 上的 `cavvy-lang/Cavvy-src-Assets` 下载 LLVM+MinGW 捆绑包。它读取 `.verinfo` 获取版本锁定信息。当工具链目录为空时，在新克隆的仓库上构建前先运行此脚本。

`.cargo/config.toml` 仅包含 linux-musl 交叉编译配置 —— 在 Windows 上请忽略。

## 测试

```bash
# 必须先构建 release —— 测试会以子进程调用 release 编译器二进制文件：
cargo build --release
cargo test --release --verbose
```

测试分布在两个位置：

- `src/lib.rs` —— 少量内联的 `#[cfg(test)]` 单元测试（词法分析器、解析器、预处理器）。
- `tests/*.rs` —— 集成测试，使用 release 构建的 `cayc` 编译 `examples/` 下的 `.cay` 文件，运行输出的 `.exe`，并断言 stdout。**如果未先构建 release，这些测试将全部失败。**

测试辅助函数位于 `tests/common/mod.rs`：`compile_and_run_eol()`、`compile_eol_expect_error()`、断言辅助函数。所有集成测试使用全局 `Mutex` 串行执行（避免文件冲突）。

**遗留临时文件**：测试运行会在 `tests/` 和 `examples/` 中留下 `temp_*.exe`、`temp_*.ll`、`temp_*.cay`。这些文件被 git 忽略但会在本地累积。请勿提交它们。

## 二进制文件（15 个）

| 二进制文件    | 用途                                           |
| ------------- | ---------------------------------------------- |
| `cayc`      | 一站式：`.cay` → 可执行文件（内部调用 clang） |
| `cay-ir`    | `.cay` → `.ll`（LLVM IR 文本）            |
| `ir2exe`    | `.ll` → 可执行文件                        |
| `cay-check` | 仅进行语法 + 语义检查（不生成代码）            |
| `cay-run`   | 编译 + 运行一步完成                            |
| `cay-rcpl`  | 交互式环境（RCPL）                             |
| `cay-bcgen` | `.caybc` 字节码生成（实验性，见下文）        |
| `cay-lsp`   | LSP 语言服务器                                 |
| `cavly`     | 包管理器                                       |
| `cay-dt`    | 文档工具                                       |
| `cay-dp`    | 依赖工具                                       |
| `cay-pre`   | 独立预处理器                                   |
| `cay-ast`   | AST 查看器（serde_json 输出）                  |
| `cay-pl`    | 源码结构/位置查看器                            |
| `cay-sir`   | 内联 IR 查看器                                 |

注意 `CAY-IR` 的编译期版本环境变量名是 `CAY-IR_VERSION`（带连字符，历史遗留），其余工具均为下划线形式（如 `CAY_RUN_VERSION`）。

主编译器工具入口点位于 `src/bin/*.rs`，并依赖库 crate（`src/lib.rs`，crate 名 `cavvy`）。
`cay-setup` 是例外：它位于独立 workspace 包 `cay-setup/`，不依赖 `cavvy` 或 LLVM，确保能在全新环境中单独构建和运行。

## 架构

```
.cay 源码
  → 预处理器（src/preprocessor/）— #include, #define, #ifdef
  → 词法分析器（src/lexer/）— 基于 logos 的分词器
  → 解析器（src/parser/）— 递归下降
  → 语义分析（src/semantic/）— 类型检查、符号解析
  → 代码生成（src/codegen/）— LLVM IR 文本生成
  → clang（捆绑：llvm-minimal/）— IR → 机器码
```

`src/` 中的关键模块：

- `ast.rs` —— 所有 AST 节点类型
- `types.rs` —— 类型系统（Type 枚举、ClassInfo、MethodInfo）
- `miette_diagnostic.rs` —— `CayError` 枚举（thiserror）、`CayResult<T>`、结构化诊断与 miette 美观打印（`src/error.rs` 和 `src/diagnostic.rs` 已不存在，错误类型统一在此文件）
- `ir/` —— 第二套 IR 管线（实验性，主代码生成路径是 `codegen/`）
- `preprocessor/` —— 带源映射的 C 风格预处理器
- `cavly/` —— 包管理器逻辑
- `bytecode/` —— CayBC 字节码格式（实验性，见下文）
- `rcpl/` —— RCPL 逻辑

`lib.rs` 暴露一个 `Compiler` 结构体，封装完整流水线。二进制文件通过 `use cavvy::Compiler` 使用它。

## 错误处理约定

- `CayError`（位于 `src/miette_diagnostic.rs`）是编译器流水线的**唯一错误类型**。使用 `thiserror::Error` 定义。每个变体都携带 `suggestion: String` 字段，用于面向用户的帮助文本。
- `CayResult<T> = Result<T, CayError>` —— 在整个编译器中使用。
- `miette` 仅用于**显示** —— 将 `CayError` 转换为美观的 CLI 输出。请勿在编译器内部使用 miette 类型。
- **核心原则：宁可 noisy 报错，不可 silently wrong。** 查找失败、类型未知、签名解析失败等场景必须返回 `CayError`，禁止用 `unwrap_or(默认值)` / `let _ =` / `_ => {}` 静默兜底后继续生成代码。
- `anyhow` 仅在 `cavly/`、`rcpl/` 和 `src/bin/cavly.rs` 中使用（包管理器子系统的历史选择）。核心编译器管线（lexer/parser/semantic/codegen/ir）**禁止**引入 anyhow。
- 错误报告函数：`print_miette_error()`、`print_error_with_context()`、`print_tool_error()`、`print_warning()` —— 均从 `cavvy::miette_diagnostic` 导入。
- 库代码不得直写 stdout/stderr。调试输出用环境变量开关：`CAVVY_DEBUG_TOKENS=1`（打印 token 流）、`CAVVY_LLC_VERBOSE=1`（内嵌 llc 详细日志）。

## 源码约定

- **注释语言**：中英文混用。文档注释（`//!`、`///`）主要为中文。实现注释视情况而定。请勿强制单一语言。
- **无格式化配置**：没有 `rustfmt.toml`。遵循现有风格：4 空格缩进，内部导入使用 `use crate::` 前缀，标准库使用 `use std::`。
- **无严格 lint**：没有 `#![deny(...)]` 或 `#![warn(...)]` 的 crate 级属性。CI 使用 `RUSTFLAGS="-A warnings"` 运行（抑制所有警告）。除非明确要求，否则请勿添加 deny 属性。
- **`use` 风格**：模块级导入置于顶部；函数局部 `use` 仅用于一次性使用的项。`pub use` 在 `mod.rs` 文件中重新导出，用于模块公共 API。

## 实验性 / 当前不可用的功能

以下功能入口存在，但当前版本会**明确报错**而不是产出错误结果（修复前它们会静默产出错误程序）：

- **字节码混淆**：`BytecodeObfuscator::obfuscate()` 总是返回 `NotAvailable` 错误；`cay-bcgen --obfuscate`、`cay-run --obfuscate` 随之报错退出。
- **字节码 JIT 编译**（`src/bytecode/jit.rs`，实为 bytecode→LLVM IR 文本→clang）：入口打印实验性警告；未支持的 opcode、无法确定大小的 `New` 等会硬报错。
- **`cay-run --bytecode`**：字节码模块生成未实现，报错退出。
- **`c_uint64_t`**：类型系统没有无符号 64 位整数，解析期报「暂不支持」（修复前被静默映射为有符号 `Int64`）。
- **`cay-bcgen` 的高级构造**：for/foreach/内联 IR、部分二元/一元运算符等暂不支持，编译期硬报错。

IR 混淆器（`src/codegen/obfuscator.rs`，`cay-ir --obfuscate`）是**可用的**：字符串字面量内容原样保留，外部符号（declare）和 `main` 不混淆。

## CI

- **夜间构建**（`.github/workflows/nb.yml`）：每天 UTC 02:00 在 `windows-latest` 上运行。使用 `stable` 工具链，目标 `x86_64-pc-windows-gnu`。产物命名为 `eol-*`（历史遗留）。如果通过 workflow dispatch 传入 `skip_tests=true`，则跳过测试。
- **文档测试**（`.github/workflows/docs.yml`）：运行文档相关的 doc-tests。（此前引用的 `jekyll-gh-pages.yml` 已不存在。）

## 文件扩展名与 .gitignore 注意事项

- `.gitignore` 全局忽略 `*.cay` 和 `*.eol`，**但** `examples/`、`test_docs/` 和 `caylibs/` 下的除外。新的 `.cay` 测试文件请放在 `examples/` 中。
- `*.exe` 被全局 git 忽略（`llvm-minimal/bin/` 下除外）。构建的测试可执行文件不会被跟踪。
- 仓库根目录存在许多旧测试运行遗留的 `.exe` 文件。请勿提交它们。

## VSCode 扩展

`vscode-extension/` 包含一个树内的 VS Code 扩展，带有语法高亮（`syntaxes/cavvy.tmLanguage.json`）和 LSP 客户端。VSIX 文件直接提交。如果修改 LSP，请同时检查 `src/bin/cay-lsp.rs` 和 `vscode-extension/src/`。

## 版本管理

版本号位于 `.verinfo`（类 INI 格式），是唯一事实来源，`Cargo.toml` 的 `package.version` 必须与其保持一致。`build.rs` 解析此文件，将每个版本与 git 提交哈希组合，并设置 `CAY*_VERSION` 环境变量用于编译时嵌入（注意 `CAY-IR_VERSION` 带连字符，见"二进制文件"一节）。修改 `.verinfo` 后，`cargo build` 将自动重新编译。

如果 `.verinfo` 为空、缺失 `[CAYC] version` 或读取失败，`build.rs` 会打印警告并回退到 `CARGO_PKG_VERSION`（不再有硬编码的旧版本号），保证任何状态下都能构建。`build.rs` 监听 `.git/HEAD`（嵌入 commit hash 所需），但**不**监听 `.git/index`（避免任何 stage 操作触发全量重编）。
