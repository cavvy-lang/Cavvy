# Cavvy 6.1.0 迁移指南

- 将文件和系统调用的返回值改为 `Result<Value, IOError>`，不要用空值作为唯一错误信号。
- 用 `Result.ok(value)` 与 `Result.err(error)` 构造结果。
- 对必须成功的内部不变量使用 `expect("说明")`；对用户输入和 I/O 使用 `?` 或显式 `isErr` 检查。
- 将 `MmapResult`、`FileResult` 等专用结果逐步统一为 `Result<T, E>`。
- 完整重建编译器：`cargo build --release`，然后执行 `cargo test --release --verbose`。
