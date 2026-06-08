# Cavvy 文档

Cavvy 是一个静态类型、面向对象的编程语言编译器。它用 Rust 2024 edition 编写，把 `.cay` 源码编译成 LLVM IR，再通过随仓库提供的 LLVM/MinGW 工具链生成原生可执行文件。

项目曾用名 EOL。旧扩展名 `.eol` 和部分 CI 产物名仍保留历史痕迹，但新代码和文档统一使用 `.cay`。

## 当前版本

真实工具版本来自 `.verinfo`，当前为 `5.1.0-RC.1`。Cargo 包版本和文档显示可能不会同时变更，判断发布工具版本时以 `.verinfo` 为准。

## 第一段程序

```cay run
public class Hello {
    public static void main() {
        println("Hello, Cavvy!");
    }
}
```

保存为 `hello.cay` 后编译：

```powershell
cargo build --release
.\target\release\cayc.exe hello.cay hello.exe
.\hello.exe
```

## 编译流水线

```text
.cay
  -> preprocessor: #include, #define, #if
  -> lexer: logos token stream
  -> parser: recursive descent AST
  -> semantic: type checking and symbol resolution
  -> codegen: textual LLVM IR
  -> ir2exe: LLVM/MinGW native executable
```

## 文档示例会被测试

文档中标为 `cay`、`cavvy` 或 `eol` 的代码块会被 `scripts/doc-test.py` 自动抽取并测试。默认模式使用 `cay-check`；标记为 `run` 或带 feature 的示例会调用 `cayc` 编译。
