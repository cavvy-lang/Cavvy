# Cavvy 文档

欢迎阅读 Cavvy 编程语言文档。

Cavvy（曾用名 EOL — Ethernos Object Language）是一门静态类型、面向对象的编程语言编译器。它使用 Rust 编写，将 `.cay` 源码编译为 LLVM IR，再通过捆绑的 LLVM/MinGW 工具链生成原生可执行文件。

- **当前版本**：5.1.0-Beta.2（来自 `.verinfo`）
- **源文件扩展名**：`.cay`
- **旧名称**：EOL（`.eol` 扩展名和 CI 中的 `eol-*` 为历史遗留）
- **许可证**：GPL3

---

## 快速上手

```cay
public int main() {
    println("Hello, Cavvy!");
    return 0;
}
```

```powershell
cargo build --release
.\target\release\cayc.exe hello.cay
.\hello.exe
```

> 完整的安装和构建指南见[快速开始](getting-started.md)。

---

## 编译流水线

```
.cay 源码
  → 预处理器（#include, #define, #ifdef）
  → 词法分析器（logos）
  → 解析器（递归下降）
  → 语义分析（类型检查 + 符号解析）
  → IR 生成（自定义 SSA IR）
  → LLVM IR 文本生成
  → clang → 原生机器码 .exe
```

---

## 核心特性

| 特性                     | 描述                                     |
| ------------------------ | ---------------------------------------- |
| **面向对象**       | 类、继承、接口、运行时多态（vtable）     |
| **静态类型**       | 编译时类型安全，强类型检查               |
| **C 预处理器**     | `#include`、`#define`、`#ifdef` 等 |
| **FFI**            | 直接调用 C ABI 函数                      |
| **CayBC 字节码**   | JVM 风格字节码，支持 JIT/AOT             |
| **Cavly 包管理器** | 依赖管理、构建、测试                     |
| **RCPL**           | 交互式编程环境                           |
| **LSP**            | 语言服务器协议支持                       |
| **11 个 CLI 工具** | 编译、检查、格式化、分析等               |

---

## 文档导航

| 章节               | 文档                                                                                                      |
| ------------------ | --------------------------------------------------------------------------------------------------------- |
| **入门**     | [快速开始](getting-started.md)、[工具链](toolchain.md)、[CLI](cli.md)                                              |
| **语言**     | [总览](language-overview.md)、[参考](language-reference.md)、[预处理器](preprocessor.md)、[FFI](ffi.md)               |
| **项目与库** | [Cavly](cavly.md)                                                                                            |
| **编译器**   | [架构](compiler-architecture.md)、[字节码格式](bytecode-format.md)、[测试](testing.md)、[实现状态](current-status.md) |

---

## 文档约定

本文档中的代码块带有特殊标记：

- `cay` — 语法检查（`cay-check`）
- `cay run` — 编译并运行
- `cay ignore` — 跳过测试
- 所有示例都经过自动化测试，确保准确
