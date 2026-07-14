# Cavvy 5.4.0 迁移指南

- 执行 `cargo build --release` 后运行 `cargo test --release --verbose`。
- 大文件随机访问可使用 `Mmap.mapReadOnly(path)`，需要写回时使用 `mapReadWrite` 并在完成后调用 `sync()`。
- 将 `MmapSlice` 的偏移限制在 `0 <= offset < size()`，并妥善处理 `Result` 错误。
- 不要在 `unmap()` 后继续使用关联的 `MmapSlice`。
