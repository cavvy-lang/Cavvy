# 测试指南

本文档说明 Cavvy 编译器的测试策略、如何编写和运行测试。

---

## 测试概览

编译器测试分布在三个层级：

| 层级 | 位置 | 类型 | 测试什么 |
|---|---|---|---|
| 单元测试 | `src/lib.rs` | `#[cfg(test)]` | 词法分析器、解析器、预处理器 |
| 集成测试 | `tests/*.rs` | 独立测试文件 | 编译并运行 `.cay` 文件 |
| 文档测试 | `scripts/doc-test.py` | 自动化脚本 | 文档中代码示例的正确性 |

---

## 运行测试

### 基本命令

```powershell
# 必须先构建 release 版本
cargo build --release

# 运行全部测试
cargo test --release --verbose
```

### 单独运行特定测试

```powershell
# 按测试文件
cargo test --release --test interface_tests -- --nocapture
cargo test --release --test lambda_tests -- --nocapture
cargo test --release --test inheritance_tests -- --nocapture
cargo test --release --test array_tests -- --nocapture
cargo test --release --test access_control_tests -- --nocapture

# 按测试名称模式
cargo test --release -- test_interface_dispatch --nocapture

# 仅编译测试（不运行）
cargo test --release --no-run
```

---

## 集成测试

集成测试位于 `tests/` 目录，每个文件对应一组相关测试。

### 测试辅助函数

位于 `tests/common/mod.rs`：

```rust
// 编译并运行 .cay 文件，断言成功
compile_and_run_eol("examples/test_filename.cay");

// 编译并断言产生预期错误
compile_eol_expect_error("examples/test_error.cay", "expected error message");
```

### 测试文件约定

- 源文件放在 `examples/` 目录下
- 以 `test_` 前缀命名
- 测试函数使用 `#[test]` 标记
- 使用全局 `Mutex` 串行执行（避免文件冲突）

### 已存在的测试文件（40+ 个）

```
tests/
├── access_control_tests.rs
├── array_tests.rs
├── class_declaration_tests.rs
├── control_flow_tests.rs
├── enum_tests.rs
├── error_tests.rs
├── expression_tests.rs
├── ffi_tests.rs
├── function_tests.rs
├── generic_tests.rs
├── inheritance_tests.rs
├── interface_tests.rs
├── lambda_tests.rs
├── lexer_tests.rs
├── method_tests.rs
├── operator_tests.rs
├── parser_tests.rs
├── preprocessor_tests.rs
├── string_tests.rs
├── struct_tests.rs
├── type_tests.rs
├── vtable_tests.rs
├── common/mod.rs
└── ...（更多）
```

---

## 文档测试

文档中的代码示例会被自动测试，确保示例始终可用。

### 运行

```powershell
# Windows 一键命令
.\scripts\test-docs.ps1

# 跨平台（Python）
python scripts/doc-test.py --build
```

### 代码块标记

文档中的代码块通过语言标记控制测试行为：

````markdown
<!-- 默认：仅语法检查（cay-check） -->
```cay
class Example {
    static void main() {
        println("checked");
    }
}
```

<!-- 编译并运行 -->
```cay run
public int main() {
    println("runs");
}
```

<!-- 顶层 main -->
```cay
public int main() {
    return 0;
}
```

<!-- 跳过测试的代码片段 -->
```cay ignore
// 这个不会被测试
```
````

### 写文档测试的最佳实践

1. **完整程序**：所有被测试的代码块必须是包含 `main` 方法的完整程序
2. **自包含**：不依赖外部文件（除非使用 `#include`）
3. **有输出**：`cay run` 标记的示例应有可验证的控制台输出
4. **不要过度标记**：仅代码片段（非完整程序）不要标记为 `cay`

---

## 添加新测试

### 为一个新特性添加测试

```rust
// tests/my_feature_tests.rs
use std::process::Command;

#[test]
fn test_my_feature_basic() {
    let output = Command::new("target/release/cayc.exe")
        .arg("examples/test_my_feature.cay")
        .output()
        .expect("编译失败");
    assert!(output.status.success(), "编译错误: {:?}", output.stderr);
}
```

### 在 examples/ 中添加测试源文件

```cay
// examples/test_my_feature.cay
class Main {
    static void main() {
        // 测试逻辑
        println("expected output");
    }
}
```

---

## 测试注意事项

1. **必须先构建 release**：集成测试调用 `target/release/cayc.exe`，debug 构建的二进制不会被测试使用
2. **临时文件清理**：测试会生成 `temp_*.exe`、`temp_*.ll` 等文件，它们被 git 忽略但会累积
3. **串行执行**：测试使用全局锁串行运行，避免文件冲突
4. **不要删除测试**：测试失败时应修复代码，而非删除测试
