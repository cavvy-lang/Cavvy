# 编译器架构

核心库 crate 是 `cavvy`。所有命令行入口都在 `src/bin/*.rs`，通过库 crate 调用编译流程。

## 模块

| 模块 | 职责 |
|---|---|
| `src/preprocessor/` | `#include`、宏和条件编译，生成源映射 |
| `src/lexer/` | 基于 `logos` 的词法分析 |
| `src/parser/` | 递归下降解析器 |
| `src/semantic/` | 符号表、类型检查、继承和方法解析 |
| `src/codegen/` | LLVM IR 文本生成 |
| `src/ir2exe_lib/` | IR 到可执行文件的共享链接逻辑 |
| `src/cavly/` | 包管理器 |
| `src/bytecode/` | CayBC 字节码格式和实验性 JIT |
| `src/rcpl/` | 交互式环境 |

## Compiler

`src/lib.rs` 暴露 `Compiler`：

```text
Compiler::compile_file(input, output_ir)
  -> read source
  -> preprocess with include paths
  -> lex with source map
  -> parse AST
  -> semantic analysis
  -> generate LLVM IR
  -> write .ll
```

`cayc` 在生成 `.ll` 后调用 `ir2exe_lib::compile_ir_to_exe`，再根据选项清理或保留 IR。

## 错误处理

编译器内部统一使用 `cayError` 和 `cayResult<T>`。`miette` 只用于 CLI 显示，不应在核心编译流程中传播。
