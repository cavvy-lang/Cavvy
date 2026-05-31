# AGENTS.md — Cavvy 编译器项目

## 项目标识

Cavvy（Cay）是一个静态类型、面向对象的编程语言编译器，使用 Rust（2024 edition）编写。它将 `.cay`/`.eol` 源码通过 LLVM IR → clang 编译为原生机器码。采用 GPL3 许可证。

- **真实版本**（来自 `.verinfo`）：`5.1.0-Beta.2`。README 中写的 5.1.0-Beta.2 —— 已同步。
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

## 二进制文件（11 个，而非 README 声称的 6 个）

| 二进制文件 | 用途 |
|---|---|
| `cayc` | 一站式：`.cay` → `.exe`（内部调用 clang） |
| `cay-ir` | `.cay` → `.ll`（LLVM IR 文本） |
| `ir2exe` | `.ll` → `.exe` |
| `cay-check` | 仅进行语法 + 语义检查（不生成代码） |
| `cay-run` | 编译 + 运行一步完成 |
| `cay-rcpl` | 交互式环境（RCPL） |
| `cay-bcgen` | `.caybc` 字节码生成 |
| `cay-lsp` | LSP 语言服务器 |
| `cavly` | 包管理器 |
| `cay-dt` | 文档工具 |
| `cay-dp` | 依赖工具 |
| `cay-pre` | 独立预处理器 |

所有入口点位于 `src/bin/*.rs`。它们都依赖库 crate（`src/lib.rs`，crate 名 `cavvy`）。

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
- `error.rs` —— `cayError` 枚举（thiserror）、`cayResult<T>`
- `diagnostic.rs` —— 结构化诊断（严重级别、阶段、错误代码）
- `miette_diagnostic.rs` —— 美观打印的 miette 诊断
- `ir/` —— IR 相关类型和工具
- `preprocessor/` —— 带源映射的 C 风格预处理器
- `cavly/` —— 包管理器逻辑
- `bytecode/` —— CayBC 字节码格式
- `rcpl/` —— RCPL 逻辑

`lib.rs` 暴露一个 `Compiler` 结构体，封装完整流水线。二进制文件通过 `use cavvy::Compiler` 使用它。

## 错误处理约定

- `cayError`（位于 `src/error.rs`）是编译器流水线的**唯一错误类型**。使用 `thiserror::Error` 定义。每个变体都携带 `suggestion: String` 字段，用于面向用户的帮助文本。
- `cayResult<T> = Result<T, cayError>` —— 在整个编译器中使用。
- `miette` 仅用于**显示** —— 将 `cayError` 转换为美观的 CLI 输出。请勿在编译器内部使用 miette 类型。
- `anyhow` 被列为依赖项，但在核心编译器中**未使用**。请勿引入它。
- 错误报告函数：`print_miette_error()`、`print_error_with_context()`、`print_tool_error()`、`print_warning()` —— 均从 `cavvy::error` 导入。

## 源码约定

- **注释语言**：中英文混用。文档注释（`//!`、`///`）主要为中文。实现注释视情况而定。请勿强制单一语言。
- **无格式化配置**：没有 `rustfmt.toml`。遵循现有风格：4 空格缩进，内部导入使用 `use crate::` 前缀，标准库使用 `use std::`。
- **无严格 lint**：没有 `#![deny(...)]` 或 `#![warn(...)]` 的 crate 级属性。CI 使用 `RUSTFLAGS="-A warnings"` 运行（抑制所有警告）。除非明确要求，否则请勿添加 deny 属性。
- **`use` 风格**：模块级导入置于顶部；函数局部 `use` 仅用于一次性使用的项。`pub use` 在 `mod.rs` 文件中重新导出，用于模块公共 API。
- **`.bak` 文件**：若干模块存在 `.rs.bak.N` 备份文件（例如 `lexer/mod.rs.bak.5`、`parser/statements.rs.bak.4`）。这些是陈旧副本。**切勿编辑它们。** 如果看到，请忽略。

## CI

- **夜间构建**（`.github/workflows/nb.yml`）：每天 UTC 02:00 在 `windows-latest` 上运行。使用 `stable` 工具链，目标 `x86_64-pc-windows-gnu`。产物命名为 `eol-*`（历史遗留）。如果通过 workflow dispatch 传入 `skip_tests=true`，则跳过测试。
- **GitHub Pages**（`.github/workflows/jekyll-gh-pages.yml`）：从 `main` 分支部署文档。与代码变更无关。

## 文件扩展名与 .gitignore 注意事项

- `.gitignore` 全局忽略 `*.cay` 和 `*.eol`，**但** `examples/`、`test_docs/` 和 `caylibs/` 下的除外。新的 `.cay` 测试文件请放在 `examples/` 中。
- `*.exe` 被全局 git 忽略（`llvm-minimal/bin/` 下除外）。构建的测试可执行文件不会被跟踪。
- 仓库根目录存在许多旧测试运行遗留的 `.exe` 文件。请勿提交它们。

## VSCode 扩展

`vscode-extension/` 包含一个树内的 VS Code 扩展，带有语法高亮（`syntaxes/cavvy.tmLanguage.json`）和 LSP 客户端。VSIX 文件直接提交。如果修改 LSP，请同时检查 `src/bin/cay-lsp.rs` 和 `vscode-extension/src/`。

## 版本管理

版本号位于 `.verinfo`（类 INI 格式）。`build.rs` 解析此文件，将每个版本与 git 提交哈希组合，并设置 `CARGO_*_VERSION` 环境变量用于编译时嵌入。修改 `.verinfo` 后，`cargo build` 将自动重新编译。

## 已知限制 (5.1.0-Beta.2)

1. **接口方法动态分发**：通过接口类型调用方法时，使用声明类型解析方法（第一个实现类），而非运行时类型。例如 `Animal a = new Dog(); a.speak();` 可能调用错误的实现。需要 vtable 支持才能正确实现动态分发。

2. **Lambda 闭包**：Lambda 语法已解析，但闭包捕获环境变量尚未完整实现。

3. **泛型单态化**：语法解析支持 `<T>`，但代码生成尚未实现单态化。

4. **private 访问控制**：编译器不强制执行 private 访问修饰符。

5. **数组初始化语法**：不支持 `new Type[] { 1, 2, 3 }` 语法，需要先声明大小再赋值。