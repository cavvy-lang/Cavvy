# LSP 语言服务器协议

Cavvy 内置 LSP（Language Server Protocol）语言服务器，提供 IDE 级别的代码辅助功能。

---

## 概述

LSP 服务器代码位于 `src/lsp/`，可执行文件为 `src/bin/cay-lsp.rs`。
它通过标准 LSP 3.x 协议与编辑器通信，对应 VS Code 扩展位于 `vscode-extension/`。

服务器内部使用与编译器相同的前端（预处理 → 词法 → 语法 → 语义分析），
但**不生成代码**；分析结果会转换为编辑器语义模型（符号、引用、作用域、类型），
供补全、悬停、跳转等功能共享。

---

## 支持的功能

| 功能 | LSP 请求 | 说明 |
|---|---|---|
| 诊断信息 | `textDocument/publishDiagnostics` | 词法/语法/语义错误；include 文件错误发布到对应文件 |
| 代码补全 | `textDocument/completion` | 成员、局部变量、当前类字段、工作区符号、内置类型/函数、关键字、代码片段、`#include` 路径 |
| 悬停信息 | `textDocument/hover` | 签名、类型、文档注释、内建存根文档 |
| 跳转到定义 | `textDocument/definition` | 局部变量、参数、字段、方法、类、include 路径、`__builtins__` 存根 |
| 查找引用 | `textDocument/references` | 当前文档、include 文件、已打开文档、工作区候选文件 |
| 重命名 | `textDocument/rename` / `prepareRename` | 局部/字段/方法/类；拒绝重命名语言内置符号 |
| 签名帮助 | `textDocument/signatureHelp` | 方法/构造函数/内建函数，高亮当前参数 |
| 文档符号 | `textDocument/documentSymbol` | 类、接口、结构体、枚举、字段、方法、构造函数 |
| 工作区符号 | `workspace/symbol` | 工作区索引 + 当前文档 + 内置类型/函数 |
| 语义高亮 | `textDocument/semanticTokens/full` | 类/接口/结构体/枚举、类型、变量、参数、字段、方法、关键字、字面量 |
| 折叠 | `textDocument/foldingRange` | 类/方法/函数/include 块 |
| 文档高亮 | `textDocument/documentHighlight` | 同名符号的全部引用 |
| 代码操作 | `textDocument/codeAction` | 未知成员/方法拼写错误的 quick fix |
| 内联提示 | `textDocument/inlayHint` | `var`/`let`/`auto` 推断出的类型 |
| 文档链接 | `textDocument/documentLink` | `#include` 路径可点击 |
| 选择范围 | `textDocument/selectionRange` | 单词 → 符号 → 所属类 → 文档 |

---

## 启动方式

```bash
# 启动 LSP 服务器
cay-lsp

# 查看版本 / 帮助
cay-lsp --version
cay-lsp --help
```

服务器从 stdin 读取 LSP 消息，向 stdout 写出 JSON-RPC 响应；日志走 stderr。

---

## 编辑器配置

### VS Code

项目包含内建的 VS Code 扩展（`vscode-extension/`）：

1. 在 VS Code 中打开 Cavvy 项目；
2. 从 `vscode-extension/` 安装扩展；
3. 打开 `.cay` 文件，扩展会自动启动 `cay-lsp`；
4. 确保 `cavvyAnalyzer.enableLSP` 为 `true`（默认值）。

启用 LSP 后，扩展不再注册旧的本地补全/悬停/跳转 provider，避免两套结果冲突；
关闭 LSP 时才会回退到本地 provider。

### 其他编辑器

对于支持 LSP 的其他编辑器（Neovim、Emacs、Sublime Text 等），配置 LSP 客户端连接到
`cay-lsp`：

```
LSP 命令: cay-lsp
传输方式: stdio
文件类型: cay, cavvy, eol
```

服务器使用 UTF-16 位置编码（LSP 默认值）。

---

## 内置类型与内置函数

`String` 方法、数组 `.length`、基本类型 `toString()`、`Object.hashCode()/equals()`、
`print`/`println`/`readInt` 等由编译器内建。为避免分析器硬编码导致版本更新后失效，
这些声明集中放在：

```
caylibs/__builtins__/
```

LSP 在初始化时自动加载该目录的 `index.json` 和 `.cay` 签名存根，用于补全、悬停、
跳转、签名帮助和快速修复。约定详情、映射规则与新增内建的步骤见
[内置声明约定](builtins.md)。

如果目录缺失，LSP 会退回到一组最小内置定义并在日志中提示；编译器本身不依赖该目录。

---

## 工作区索引

服务器在 `initialized` 后后台扫描工作区中的 `.cay` / `.eol` 文件（跳过 `target`、
`.git`、`node_modules` 等目录），建立轻量符号索引，用于：

- `workspace/symbol` 查询；
- 跳转到未在当前文档 include 的类/函数；
- 引用搜索的候选文件筛选。

保存文件或编辑器发送 `workspace/didChangeWatchedFiles` 时会增量刷新索引。

---

## 架构

```
src/lsp/
├── util.rs       # UTF-16/UTF-8 位置、括号匹配、文档注释、接收者提取
├── builtins.rs   # 读取 caylibs/__builtins__，提供内建成员查询
├── model.rs      # AST -> 符号/引用/作用域/类型/折叠/内联提示
├── analysis.rs   # 预处理 -> 词法 -> 语法 -> 语义 -> 模型
├── workspace.rs  # 工作区文件索引
└── server.rs     # tower-lsp 服务与各 LSP 功能
```

---

## 注意事项

- 服务器只执行编译器前端，不生成 LLVM IR；
- 大文件/大量 include 的分析有防抖（约 140ms），请求时若缓存过期会同步补算一次；
- 诊断只发布给已打开的文件；include 文件的错误不会错误地显示在主文件行号上；
- 内置存根不参与用户程序编译，修改它们只影响编辑器分析，不影响代码生成；
- 扩展和 LSP 服务器共同维护时，应同步更新语义高亮/语法规则与编译器能力。
