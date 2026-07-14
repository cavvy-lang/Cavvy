# Cavvy 5.4.0 新特性详解

## 内存映射文件

新增 `std::Mmap` 与 `std::MmapSlice`：

- `mapReadOnly(path)` 和 `mapReadWrite(path, size)` 返回映射结果。
- 支持 `data()`、`size()`、`sync()`、`unmap()` 以及零拷贝 `slice()`。
- `MmapSlice` 支持按偏移 `get`/`set`。
- Windows 使用 `CreateFileMappingA`/`MapViewOfFile`，Linux 使用 `mmap`/`munmap`/`msync`。
- 析构时自动解除映射并释放系统句柄。

新增 `examples/test_mmap.cay` 和文件库集成测试，覆盖空文件、越界、持久化和 64KB 压力场景。
