# Cavvy (Cay) 编程语言文档

# 文档还在测试阶段且未编写完，遇到任何问题可以提出issue，也可以提PR帮助我们丰富文档


**Cavvy**（也称作 **Cay**）是一门静态类型、面向对象的编程语言，以 Rust 编写，通过 LLVM IR → clang 编译为原生机器码。项目采用分层许可证：编译器本体为 GPL-3.0-only，运行时为 GPL-3.0-only + [Cavvy Runtime Library Exception](../LICENSE-EXCEPTION.md)，标准库为 MIT。详见根目录 [README.md](../README.md#许可证)。

- **当前版本**：6.1.0
- **源文件扩展名**：`.cay`（规范扩展名，曾用 `.eol`）
- **项目全称**：Cavvy（曾用名 EOL — Ethernos Object Language）

---

## 文档目录

| 文档                                              | 说明                            |
| ------------------------------------------------- | ------------------------------- |
| [README.md](README.md)                               | 本文档 — 项目总览与文档索引    |
| [getting-started.md](getting-started.md)             | 安装、Hello World、快速上手指南 |
| [language-overview.md](language-overview.md)         | 语言特性总览                    |
| [language-reference.md](language-reference.md)       | 完整语言参考手册                |
| [compiler-architecture.md](compiler-architecture.md) | 编译器架构与流水线              |
| [cli.md](cli.md)                                     | CLI 工具参考手册                |
| [preprocessor.md](preprocessor.md)                   | 预处理器指南                    |
| [ffi.md](ffi.md)                                     | FFI 外部函数接口                |
| [toolchain.md](toolchain.md)                         | 构建与测试指南                  |
| [cavly.md](cavly.md)                                 | 包管理器指南                    |
| [bytecode-format.md](bytecode-format.md)             | CayBC 字节码格式                |
| [lsp-protocol.md](lsp-protocol.md)                   | LSP 语言服务器协议              |
| [testing.md](testing.md)                             | 测试指南                        |
| [current-status.md](current-status.md)               | 当前实现状态                    |
| [maintaining-docs.md](maintaining-docs.md)           | 维护文档指南                    |

---

## 技术栈概要

- **语言实现**：Rust（2024 edition）
- **中间表示**：自定义 SSA 形式 IR（`src/ir/`）→ LLVM IR 文本
- **后端**：捆绑 clang（`llvm-minimal/`）将 LLVM IR 编译为机器码
- **字节码**：CayBC 格式（`src/bytecode/`），支持 JIT/AOT
- **包管理**：Cavly（`src/cavly/`），内置依赖解析和工作区管理
- **LSP**：内置语言服务器（`src/bin/cay-lsp.rs`）

---

## 快速链接

- [项目 GitHub](https://github.com/cavvy-lang)
- [EBNF 语法文件](../cavvy.ebnf) — 完整的形式化语法
- [标准库源码](../caylibs/) — 语言内置运行时库
- [示例程序](../examples/) — 各类语言特性示例
