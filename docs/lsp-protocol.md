# LSP 语言服务器协议

Cavvy 内置 LSP（Language Server Protocol）语言服务器，提供 IDE 级别的代码辅助功能。

---

## 概述

LSP 服务器位于 `src/bin/cay-lsp.rs`，通过标准 LSP 协议与编辑器通信。对应的 VS Code 扩展位于 `vscode-extension/`。

---

## 支持的功能

| 功能 | LSP 请求 | 状态 |
|---|---|---|
| 语法高亮 | —（由扩展处理） | ✅ |
| 诊断信息 | `textDocument/publishDiagnostics` | ✅ |
| 跳转到定义 | `textDocument/definition` | ✅ |
| 自动补全 | `textDocument/completion` | ✅ |
| 悬停信息 | `textDocument/hover` | ✅ |
| 错误标记 | `textDocument/semanticTokens` | ✅ |
| 文档符号 | `textDocument/documentSymbol` | ✅ |

---

## 启动方式

```bash
# 启动 LSP 服务器
cay-lsp

# 或通过编辑器配置启动
# cay-lsp 从 stdin 读取 LSP 消息，输出到 stdout
```

LSP 服务器通过标准输入/输出（stdio）与编辑器通信，遵循 LSP 3.x 规范。

---

## 编辑器配置

### VS Code

项目包含内建的 VS Code 扩展（`vscode-extension/`）：

1. 在 VS Code 中打开 Cavvy 项目
2. 从 `vscode-extension/` 安装扩展
3. 打开 `.cay` 文件，扩展会自动启动 LSP 服务器

### 其他编辑器

对于支持 LSP 的其他编辑器（Neovim、Emacs、Sublime Text 等），配置 LSP 客户端连接到 `cay-lsp`：

```
LSP 命令: cay-lsp
传输方式: stdio
文件类型: cay, cavvy, eol
```

---

## VS Code 扩展结构

```
vscode-extension/
├── package.json            # 扩展配置
├── syntaxes/
│   └── cavvy.tmLanguage.json  # 语法高亮规则
└── src/
    └── extension.ts        # LSP 客户端
```

### 语法高亮

`cavvy.tmLanguage.json` 定义了完整的 TextMate 语法规则，覆盖：
- 关键字（`class`、`interface`、`if`、`for` 等）
- 字面量（数字、字符串、字符）
- 注释（`//`、`/* */`）
- 类型标识符
- 操作符

---

## 编译器集成

LSP 服务器使用 `cavvy::Compiler` 库进行实时代码分析：

1. 文件内容变化时触发诊断
2. 编译器在前端阶段（预处理 → 词法分析 → 解析 → 语义分析）运行
3. 收集错误和警告，发布为 LSP 诊断
4. 利用符号表提供定义跳转和自动补全

---

## 注意事项

- LSP 服务器仅执行编译器的前端阶段，不生成代码
- 大文件的诊断可能有一定延迟（取决于编译器性能）
- 扩展和 LSP 服务器共同维护时，需要同步更新语法规则和编译器能力