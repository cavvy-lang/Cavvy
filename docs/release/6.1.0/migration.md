# Cavvy 6.1.0 迁移指南

本文档面向从 5.4.x 或更早版本升级的项目。6.1.0 的核心迁移是将可恢复错误统一为 `Result<T, E>`。

## 推荐模式

```cay ignore
public Result<int, IOError> readValue(string path) {
    Result<int, IOError> result = Result<int, IOError>.ok(42);
    int value = result?;
    return Result<int, IOError>.ok(value);
}
```

- 将文件和系统调用的返回值改为 `Result<Value, IOError>`，不要用空值作为唯一错误信号。
- 用 `Result.ok(value)` 与 `Result.err(error)` 构造结果。
- 对必须成功的内部不变量使用 `expect("说明")`；对用户输入和 I/O 使用 `?` 或显式 `isErr` 检查。
- 将 `MmapResult`、`FileResult` 等专用结果逐步统一为 `Result<T, E>`。
- 完整重建编译器：`cargo build --release`，然后执行 `cargo test --release --verbose`。
- 检查所有 `Result<T, E>` 的两个类型参数，并确保使用 `?` 的函数返回兼容的 `Result`。
- 更新文档、示例和 CI 中硬编码的旧版本号；版本来源以根目录 `.verinfo` 为准。
