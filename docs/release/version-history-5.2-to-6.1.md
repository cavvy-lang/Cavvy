# Cavvy 5.2.0～6.1.0 版本演进

本文档把 5.2.0 到 6.1.0 的功能变化按用户可见的主题汇总。每个版本的完整变更仍以对应目录中的发布说明、破坏性变更、迁移指南和已知问题为准。

## 版本路线

| 版本 | 核心主题 | 主要用户影响 |
|---|---|---|
| 5.2.0 | 诊断、嵌入式 LLVM、分析工具 | 统一 `CayError`/`CayResult`，新增 `cay-ast`、`cay-pl`、`cay-sir`，扩展 `--use-embedded-llc` |
| 5.3.0 | 调用语法、泛型推断、资源管理 | 支持 `Type::method()`、省略 `new`、智能指针和自动 RAII、`-g` |
| 5.4.0 | 内存映射文件 | 新增跨平台 `Mmap`/`MmapSlice` 零拷贝访问 |
| 6.1.0 | 显式错误传播 | 新增 `Result<T,E>`、`Error` 层级、`?`、`panic` 和 `abort` |

## 5.2.0：诊断和工具链

- `CayError` 直接实现 miette 诊断接口，错误码从错误消息中独立出来。
- 测试辅助函数直接返回 `Vec<CayError>`；新代码应按 `error_code` 判断错误，不要匹配完整文本。
- `--use-embedded-llc` 可用于更多编译/运行入口；Linux 下可自动构建缺失的 `libcayrt-linux.a`。
- `cay-ast` 输出 AST，`cay-pl` 输出预处理结果，`cay-sir` 输出语义 IR；这些工具适合诊断宏、类型解析和符号绑定问题。
- 泛型替换支持嵌套类型、泛型返回值和接口 vtable 后缀。

## 5.3.0：调用、资源和调试

```cay ignore
// 静态调用可以使用类型限定名
int value = Integer::parseInt("42");

// 简单构造场景可以省略 new
Box<int> box = Box<int>(value);
```

- `std::sys` 提供进程、环境变量和命令行参数访问。
- `ArrayList<T>`、`vector<T>` 和迭代器提供容器基础设施。
- `UniquePtr<T>`、`ScopedPtr<T>`、`Rc<T>`、`WeakPtr<T>` 提供独占、作用域、共享和弱引用所有权模型。
- 作用域退出时会触发受支持对象的析构；需要转移所有权时使用 `UniquePtr.move()` 或 `release()`，不要重复 `delete`。
- `-g` 生成调试信息；内联 IR 支持 `atomicrmw` 和 `cmpxchg`。

## 5.4.0：内存映射文件

`Mmap.mapReadOnly(path)` 和 `Mmap.mapReadWrite(path, size)` 返回 `MmapResult<T>`。使用映射前必须检查 `isOk()`，完成后调用 `sync()`（写映射）和 `unmap()`。`MmapSlice` 只是底层映射的视图，解除映射后不可继续使用。

## 6.1.0：Result 错误传播

```cay ignore
Result<int, String> result = Result<int, String>.ok(42);
if (result.isOk()) {
    int value = result.unwrap();
}
```

- 使用 `Result<T,E>.ok(value)` 和 `Result<T,E>.err(error)` 构造结果。
- `isOk`/`isErr` 用于分支，`unwrap`/`unwrapErr` 用于已确认分支，`unwrapOr` 用于默认值，`expect` 用于必须成功的不变量。
- `?` 只能出现在返回兼容 `Result` 的函数中，并会把错误分支直接传播给调用方。
- `std::Error`、`std::IOError` 和 `std::ParseError` 为错误分类、系统错误码、位置和描述提供统一接口。
- `panic` 用于带消息终止，`abort` 用于直接终止；它们不是可恢复错误处理机制。

## 升级顺序

1. 从 5.2.0 开始先按错误码更新诊断和测试断言。
2. 从 5.3.0 开始检查资源所有权，选择智能指针或显式所有权转移。
3. 从 5.4.0 开始检查 mmap 的失败、越界和生命周期分支。
4. 升级到 6.1.0 后，把可恢复错误统一为 `Result<T,E>`，再逐步引入 `?`。

详细文档：

- [5.2.0 发布说明](5.2.0/index.md)
- [5.3.0 发布说明](5.3.0/index.md)
- [5.4.0 发布说明](5.4.0/index.md)
- [6.1.0 发布说明](6.1.0/index.md)
