# 实现状态

本页按当前源码、测试和最近 git 历史重新整理，不沿用旧文档中的限制清单。进入这里的结论需要能在 `tests/`、`examples/` 或源码实现中找到依据。

## 已验证功能

| 功能 | 当前状态 | 依据 |
|---|---|---|
| 类、构造函数、继承、重写 | 可用 | `tests/inheritance_tests.rs`、`examples/test_vtable_dynamic_dispatch.cay` |
| 基类类型的动态分发 | 已通过 vtable 实现 | `test_vtable_dynamic_dispatch` 断言 `Animal` 变量调用到 `Dog`/`Cat` 重写方法 |
| 接口声明、实现与动态分发 | 可用 | `tests/interface_tests.rs` 覆盖接口变量、接口参数、多接口、继承组合和多实现运行时分派 |
| `private` / `protected` 访问控制 | 已在语义分析中检查 | `tests/access_control_tests.rs` 覆盖字段、方法、静态成员和构造函数 |
| Lambda 与闭包捕获 | 可用 | `tests/lambda_tests.rs` 覆盖表达式体、多行体、返回 lambda、循环捕获和嵌套 lambda |
| 泛型类和泛型字段/方法 | 可用 | `examples/test_generics_comprehensive.cay`、泛型解析与类型替换相关测试 |
| 数组字面量初始化 | 可用 | `tests/array_tests.rs`、`examples/test_array_init_inline.cay` |
| `@FreeFunction` | 可用 | `tests/new_features_tests.rs` |
| 顶层函数 | 作为 feature gate 可用 | 需要 `-F=top_level_function` 或 `--feature=top_level_function` |

## 需要注意的行为

接口类型调用现在通过对象运行时 vtable 分派，两个不同实现类经同一个接口类型调用同名方法时，会按运行时类型选择实现：

```text
Animal a1 = new Dog();
a1.speak();
Animal a2 = new Cat();
a2.speak();
```

当前 release 编译器对这个探针输出 `Dog`、`Cat`。`tests/interface_tests.rs` 中的 `test_interface_assignment_compatibility`、`test_interface_dispatch_uses_runtime_type_with_different_class_slots` 和 `test_interface_dispatch_with_args_and_return_uses_runtime_type` 分别覆盖了基础多实现调用、实现类 vtable 槽位不同的场景，以及带参数和返回值的接口动态分发。

## 实验性工具

`cay-bcgen`、字节码/JIT 相关模块、`cay-dt` 和 `cay-dp` 已有入口或实现片段，但文档站目前只把它们作为工具列出，不把它们描述为稳定发布接口。给这些工具补充使用文档前，应先增加对应的集成测试或可重复的命令示例。
