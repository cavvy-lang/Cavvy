# Cavvy 5.3.0 迁移指南

- 先执行 `cargo build --release`，再运行集成测试。
- 需要调试时使用 `cayc -g program.cay`。
- 将手写的资源释放逻辑检查为 RAII 语义，避免重复释放；需要放弃托管时使用智能指针的 `release()`。
- 可将 `new Type(args)` 逐步改写为 `Type(args)`，但跨模块或存在重载时建议保留显式 `new`。
- 使用静态方法时可改为 `Type::method(args)`，嵌套命名空间使用完整限定名。
