# Cavvy 6.1.0 新特性详解

## Result 错误处理

- 新增泛型 `std::Result<T, E>`，提供 `ok`、`err`、`isOk`、`isErr`、`unwrap`、`unwrapOr`、`unwrapErr` 和 `expect`。
- 新增 `std::Error` 类型层级及 `std::IOError`、`std::ParseError`，可携带错误分类、原始系统错误码、位置和描述。
- 新增 `?` 运算符，在函数中自动传播错误并保持返回类型检查。
- 新增 `panic` 与 `abort` 内建函数。

`Result` 的核心操作是 `ok`、`err`、`isOk`、`isErr`、`unwrap`、`unwrapOr`、`unwrapErr` 和 `expect`。错误类型层级还提供 `std::ParseError`，用于携带解析位置和源代码片段。

```cay
Result<int, String> result = Result<int, String>.ok(42);
if (result.isOk()) {
    println(String.valueOf(result.unwrap()));
}
```

Result 采用显式值/错误分支，不依赖异常、堆分配、RTTI 或栈回退，适合与 5.3/5.4 的资源管理和 I/O API 组合使用。
