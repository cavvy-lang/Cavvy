# 当前实现状态

本文档记录 Cavvy 编译器各特性的实现状态。所有结论均基于源码、测试和示例程序。

---

## 已验证功能

| 功能 | 状态 | 依据 |
|---|---|---|
| **编译流水线** | ✅ 完整实现 | `src/lib.rs` — `Compiler` 结构体 |
| **预处理器**（#include, #define, #ifdef） | ✅ 完整实现 | `src/preprocessor/`、`tests/preprocessor_tests.rs` |
| **词法分析器**（logos 基础） | ✅ 完整实现 | `src/lexer/`、`src/lib.rs` 单元测试 |
| **解析器**（递归下降） | ✅ 完整实现 | `src/parser/`、`tests/parser_tests.rs` |
| **语义分析** — 类型检查 | ✅ 完整实现 | `src/semantic/type_check.rs` |
| **语义分析** — 符号表 | ✅ 完整实现 | `src/semantic/symbol_table.rs` |
| **语义分析** — 类型推导 | ✅ 完整实现 | `src/semantic/type_inference_result.rs` |
| **语义分析** — 类层级分析 | ✅ 完整实现 | `src/semantic/class_analysis.rs` |
| **LLVM IR 代码生成** | ✅ 完整实现 | `src/codegen/`（24+ 文件） |
| **IR 系统**（自定义 SSA IR） | ✅ 完整实现 | `src/ir/`（12 文件） |
| **LLVM 后端**（IR → LLVM IR） | ✅ 完整实现 | `src/ir/llvm_backend.rs` |
| **clang 集成**（捆绑） | ✅ 完整实现 | `ir2exe_lib.rs`、`llvm-minimal/` |
| **类、构造函数、继承、重写** | ✅ 可用 | `tests/inheritance_tests.rs` |
| **vtable 动态分发** | ✅ 已实现 | `test_vtable_dynamic_dispatch` |
| **接口声明、实现与动态分发** | ✅ 可用 | `tests/interface_tests.rs` |
| **private/protected 访问控制** | ✅ 语义分析中检查 | `tests/access_control_tests.rs` |
| **Lambda 与闭包捕获** | ✅ 可用 | `tests/lambda_tests.rs` |
| **泛型类（解析与类型替换）** | ✅ 解析可用 | `examples/test_generics_comprehensive.cay` |
| **数组字面量初始化** | ✅ 可用 | `tests/array_tests.rs` |
| **@FreeFunction** | ✅ 可用 | `tests/new_features_tests.rs` |
| **顶层函数**（public int main()） | ✅ 默认支持 | 0.4.3.0+ 版本推荐风格 |
| **switch 语句** | ✅ 可用 | `tests/control_flow_tests.rs` |
| **异常处理**（try/catch/finally） | ✅ 可用 | `tests/error_tests.rs` |
| **结构体**（struct） | ✅ 可用 | `tests/struct_tests.rs` |
| **枚举**（enum） | ✅ 可用 | `tests/enum_tests.rs` |
| **字符串操作** | ✅ 可用 | `tests/string_tests.rs` |
| **FFI extern** | ✅ 可用 | `tests/ffi_tests.rs` |
| **CayBC 字节码** | ✅ 完整实现 | `src/bytecode/`（7 文件） |
| **字节码序列化** | ✅ 完整实现 | `src/bytecode/serializer.rs` |
| **字节码 JIT/AOT** | ✅ 实现 | `src/bytecode/jit.rs` |
| **字节码混淆** | ✅ 实现 | `src/bytecode/obfuscator.rs` |
| **包管理器 Cavly** | ✅ 完整实现 | `src/cavly/`（6+ 文件） |
| **RCPL 交互式环境** | ✅ 完整实现 | `src/rcpl/`（4 文件） |
| **LSP 语言服务器** | ✅ 实现 | `src/bin/cay-lsp.rs` |
| **文档工具 cay-dt** | ✅ 实现 | `src/bin/cay-dt.rs` |
| **依赖分析 cay-dp** | ✅ 实现 | `src/bin/cay-dp.rs` |
| **独立预处理器 cay-pre** | ✅ 实现 | `src/bin/cay-pre.rs` |
| **IR 内联优化** | ✅ 实现 | `src/ir/inliner.rs` |
| **IR 验证器** | ✅ 实现 | `src/ir/verification.rs` |
| **内联 IR（__ir { } 块）** | ✅ 实现 | `src/ir/inline_ir.rs` |
| **源代码映射（debug info）** | ✅ 实现 | `src/codegen/source_map.rs` |

---

## 已知限制与未实现特性

| 特性 | 状态 | 说明 |
|---|---|---|
| **数组初始化语法** `new Type[] { 1, 2, 3 }` | ❌ 不支持 | 需先声明大小再逐个赋值 |

---

## 需要注意的行为

### 接口动态分发

接口调用通过对象 vtable 实现运行时分发。两个不同实现类经同一个接口类型调用同名方法时，按运行时类型选择实现：

```
Animal a1 = new Dog();   a1.speak();  // → "汪汪!"
Animal a2 = new Cat();   a2.speak();  // → "喵~"
```

相关测试：
- `test_interface_assignment_compatibility` — 基础多实现调用
- `test_interface_dispatch_uses_runtime_type_with_different_class_slots` — 不同 vtable 槽位
- `test_interface_dispatch_with_args_and_return_uses_runtime_type` — 带参数和返回值

---

## 实验性工具

以下工具已有入口或实现片段，但尚未作为稳定发布接口：

| 工具 | 状态 |
|---|---|
| `cay-bcgen`（字节码生成器） | ⚙️ 基础实现完成 |
| `cay-dt`（文档工具） | ⚙️ 基础实现完成 |
| `cay-dp`（依赖分析工具） | ⚙️ 基础实现完成 |

这些工具在补充完整的使用文档和集成测试前，暂不标记为稳定。

---

## 测试覆盖率

| 测试文件 | 覆盖内容 |
|---|---|
| `tests/inheritance_tests.rs` | 继承、重写、vtable 分发 |
| `tests/interface_tests.rs` | 接口声明、实现、多态分发 |
| `tests/lambda_tests.rs` | Lambda 表达式、闭包、捕获 |
| `tests/generic_tests.rs` | 泛型解析、类型替换 |
| `tests/array_tests.rs` | 数组声明、访问、初始化 |
| `tests/access_control_tests.rs` | private/protected 检查 |
| `tests/control_flow_tests.rs` | if/for/while/switch/break/continue |
| `tests/string_tests.rs` | 字符串方法和操作 |
| `tests/struct_tests.rs` | 结构体声明和使用 |
| `tests/enum_tests.rs` | 枚举声明和使用 |
| `tests/ffi_tests.rs` | FFI 外部函数调用 |
| `tests/error_tests.rs` | 异常处理 |
| `tests/preprocessor_tests.rs` | 所有预处理器指令 |
| `tests/vtable_tests.rs` | vtable 布局和动态分发 |
| 40+ 文件 | 全面覆盖编译器各模块 |
