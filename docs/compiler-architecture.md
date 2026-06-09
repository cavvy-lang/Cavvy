# 编译器架构

本文档说明 Cavvy 编译器的整体架构、编译流水线和各模块职责。

---

## 整体架构概览

```
.cay 源码
  │
  ▼
┌──────────────────┐
│  预处理器          │  src/preprocessor/
│  (#include,       │
│   #define, #ifdef)│
└──────┬───────────┘
       │ 预处理后源码
       ▼
┌──────────────────┐
│  词法分析器        │  src/lexer/
│  (基于 logos)      │
└──────┬───────────┘
       │ Token 流
       ▼
┌──────────────────┐
│  解析器            │  src/parser/
│  (递归下降)        │
└──────┬───────────┘
       │ AST
       ▼
┌──────────────────┐
│  语义分析          │  src/semantic/
│  (类型检查/符号表)  │
└──────┬───────────┘
       │ 带类型注解的 AST
       ▼
┌──────────────────┐
│  IR 生成           │  src/ir/
│  (自定义 SSA IR)   │
└──────┬───────────┘
       │ IR (自定义数据结构)
       ▼
┌──────────────────┐
│  LLVM 后端         │  src/ir/llvm_backend.rs
│  (IR → LLVM IR)   │  + src/codegen/（旧代码生成路径）
└──────┬───────────┘
       │ LLVM IR 文本 (.ll)
       ▼
┌──────────────────┐
│  clang (捆绑)      │  llvm-minimal/
│  (.ll → .exe)     │
└──────────────────┘
```

---

## 模块详解

### 1. 预处理器（`src/preprocessor/`）

完整的 C 风格预处理器，支持：
- `#include "file"` / `#include <file>` — 文件包含
- `#define MACRO value` / `#define MACRO(args) body` — 宏定义
- `#undef MACRO` — 取消宏定义
- `#ifdef` / `#ifndef` / `#if` / `#elif` / `#else` / `#endif` — 条件编译
- `#pragma once` — 防止重复包含
- `#line` — 行标记（用于错误定位）
- `#error "message"` — 编译错误
- 嵌套宏展开和字符串化
- 源映射（source map）维护，将预处理后的位置映射回原始位置

### 2. 词法分析器（`src/lexer/`）

基于 **logos** 库的快速词法分析器。

- `TokenKind` 枚举 — 所有词法单元类型（关键字、运算符、字面量、标识符等）
- `Lexer` 结构体包装 logos，提供位置跟踪和错误恢复
- `Span` — 源码位置信息（行、列、偏移）
- 支持 `//` 和 `/* */` 注释
- 支持原始字符串字面量

### 3. 解析器（`src/parser/`）

递归下降解析器，从 Token 流构建 AST。

- `mod.rs` — 主解析器入口，`Parser` 结构体
- `expressions/` 子模块 — 7 个文件：
  - `assignment.rs` — 赋值表达式
  - `binary.rs` — 二元运算
  - `lambda.rs` — Lambda 表达式
  - `mod.rs` — 统一调度
  - `postfix.rs` — 后缀表达式（方法调用、数组访问）
  - `primary.rs` — 基本表达式（字面量、变量、括号）
  - `unary.rs` — 一元运算
- 解析各种语句：声明、控制流、类/接口/结构体/枚举定义

### 4. 语义分析（`src/semantic/`）

语义分析阶段包括类型检查、符号解析和作用域管理。

- `mod.rs` — `SemanticAnalyzer` 入口
- `symbol_table.rs` — 嵌套作用域的符号表：
  - `SymbolTable` — 作用域栈
  - `Symbol` 枚举 — 变量、类、方法、接口、枚举等
- `type_check.rs` — `SemanticAnalyzer` 实现，类型兼容性校验
- `type_inference_result.rs` — 带错误收集的类型推导
- `expr_inference.rs` — 表达式类型推导
- `class_analysis.rs` — 类层级分析（方法重写、接口实现验证）

### 5. IR 系统（`src/ir/`）

自定义 SSA 形式中间表示，共 12 个文件。这是编译器的核心新架构。

**关键文件**：

| 文件 | 职责 |
|---|---|
| `mod.rs` | IR 模块入口 |
| `module.rs` | `IrModule` — 顶层容器（全局变量、函数声明） |
| `function.rs` | `IrFunction` — 函数定义（参数、基本块列表） |
| `block.rs` | `IrBasicBlock` — 基本块（指令列表 + 终止符） |
| `value.rs` | `IrValue` — 值枚举（常量、变量、临时寄存器） |
| `types.rs` | `IrType` — 类型枚举（整数、浮点、指针、数组、函数签名） |
| `builder.rs` | `IrBuilder` — AST → IR 核心转换（约 2000+ 行） |
| `llvm_backend.rs` | `LlvmBackend` — IR → LLVM IR 文本渲染 |
| `inliner.rs` | 函数内联优化器 |
| `inline_ir.rs` | 内联 IR 支持（`__ir { }` 块） |
| `verification.rs` | `IrVerifier` — IR 正确性验证（SSA 合规性） |

**IR 设计特点**：
- SSA 形式，每个赋值产生新的版本
- Phi 节点用于控制流合并点
- 强类型，每个值有确定的 `IrType`
- 支持内联 IR 嵌入（`__ir { }`）

### 6. 代码生成（`src/codegen/`）

代码生成模块将语义分析后的 AST 转换为 LLVM IR 文本。

**核心文件**：
- `generator.rs` — `CodeGenerator` 主入口
- `context.rs` — `CodegenContext`（符号映射、作用域）
- `types.rs` — Cavvy 类型 ↔ LLVM 类型映射
- `source_map.rs` — 源码位置到 LLVM 元数据的映射（调试信息）
- `allocator.rs` — 内存分配（栈/全局）
- `bridge.rs` — 运行时桥接
- `platform.rs` — 平台 ABI/对齐/调用约定
- `obfuscator.rs` — 代码混淆（名称/控制流）

**表达式代码生成**（`expressions/`）— 19 个文件：allocator, array, assignment, binary, builtin, call, cast, identifier, instanceof, lambda, literal, main, member, new, string_methods, ternary, unary, utils, mod.rs

**语句代码生成**（`statements/`）— 10 个文件：block, if_stmt, jump_stmt, loops, return_stmt, scope_stmt, statement, switch_stmt, var_decl, mod.rs

**运行时支持**（`runtime/`）— 19 个文件：字符串操作、类型转换、指针操作、缓冲区转换等运行时函数声明

### 7. 字节码系统（`src/bytecode/`）

CayBC 字节码格式，支持 JIT/AOT 编译。

- `mod.rs` — `BytecodeModule` 顶层结构
- `constant_pool.rs` — JVM 风格的常量池（字符串、整数、浮点、类引用、方法句柄）
- `instructions.rs` — 100+ 指令操作码（`Opcode` 枚举）
- `jit.rs` — JIT/AOT 编译器（`JitOptions`、`jit_to_exe()`）
- `linker.rs` — 自动链接（`LinkerConfig`，自动检测依赖库）
- `serializer.rs` — 二进制序列化（魔数 `CAY\x01`）
- `obfuscator.rs` — 字节码混淆（名称、控制流、垃圾代码、字符串加密）

### 8. 库（`src/lib.rs`）

暴露 `Compiler` 结构体封装完整流水线。所有二进制文件通过 `use cavvy::Compiler` 使用。

关键导出：
- `Compiler` 结构体
- `CompilerStage` 枚举（编译阶段）
- `CompilerOptions` 结构体（优化级别、输出格式等）

---

## 编译流水线

```
Compiler::compile() 调用链：

1. read_source()            读取源文件
2. preprocess()             预处理器展开
3. tokenize()               词法分析
4. parse()                  解析为 AST
5. analyze()                语义分析
6. generate_ir()            AST → IR
7. optimize_ir()            IR 优化（内联等）
8. generate_llvm_ir()       IR → LLVM IR 文本
9. write_output()           写入 .ll 文件
10. run_clang()              调用捆绑 clang → .exe
```

每个阶段对应 `CompilerStage` 枚举，支持部分流水线执行。

---

## 二进制入口点

11 个可执行文件全部位于 `src/bin/`：

| 二进制 | 功能 | 流水线终点 |
|---|---|---|
| `cayc` | 一站式编译 | .exe |
| `cay-ir` | 生成 LLVM IR | .ll |
| `ir2exe` | IR → .exe | 仅 clang |
| `cay-check` | 语法语义检查 | 语义分析 |
| `cay-run` | 编译并运行 | .exe + 运行 |
| `cay-rcpl` | 交互式环境 | 循环编译 |
| `cay-bcgen` | 字节码生成 | CayBC |
| `cay-lsp` | LSP 服务器 | — |
| `cavly` | 包管理器 | — |
| `cay-dt` | 文档工具 | — |
| `cay-dp` | 依赖分析 | — |
| `cay-pre` | 预处理器 | 预处理 |

---

## 错误处理约定

- `cayError`（位于 `src/error.rs`）是编译器流水线的**唯一错误类型**
- 使用 `thiserror::Error` 定义，每个变体携带 `suggestion: String` 字段
- `cayResult<T> = Result<T, cayError>` 在整个编译器中统一使用
- `miette` 仅用于**显示**，将 `cayError` 转为美观的 CLI 输出
- CLI 错误报告函数：`print_miette_error()`、`print_error_with_context()`、`print_tool_error()`、`print_warning()`

---

## 额外系统

### 包管理器（Cavly，`src/cavly/`）
- `config.rs` — `CavlyConfig`、`PackageConfig`、`BuildConfig`、`FfiConfig`、`Dependency`、`WorkspaceConfig`、`LibConfig`
- `builder.rs` — 构建状态机、依赖解析、拓扑排序
- `project.rs` — 项目创建和模板
- `ffi.rs` — FFI 检测和绑定生成
- `tester.rs` — 测试运行器
- `workspace.rs` — 工作区管理

### RCPL 交互式环境（`src/rcpl/`）
- `mod.rs` — 主循环
- `input_parser.rs` — 输入分类（空输入、表达式、语句、类定义、预处理器指令等）
- `code_generator.rs` — 交互式代码生成（代码包装、输出注入）
- `context.rs` — 持久化上下文（变量、类型、导入跟踪）

### LSP 语言服务器（`src/bin/cay-lsp.rs`）
内置 LSP 服务器，与 `vscode-extension/` 配合使用，提供语法高亮、自动补全、诊断、跳转定义、悬停信息。
