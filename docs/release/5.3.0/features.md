# Cavvy 5.3.0 新特性详解

## 语言与泛型

- 支持 `ClassName::staticMethod(args)` 及命名空间限定的静态调用。
- 支持省略 `new` 的 `ClassName(args)` 和 `ClassName<T>(args)` 实例化。
- 增强泛型静态工厂、实例方法返回类型和链式调用的类型推断。
- 增加代码风格警告，并改善无源码位置错误的调试信息。

## 标准库与资源管理

- 新增 `std::sys`，封装进程、环境变量和命令行参数。
- 新增泛型 `std::ArrayList`、`std::vector` 及迭代器支持。
- 新增 `UniquePtr`、`ScopedPtr`、`Rc` 和 `WeakPtr`，并支持作用域退出时自动析构。
- 新增 `eprint`、`eprintln`、`exit` 和数组分配内建能力。

## 工具链与测试

- 编译器支持 `-g`，生成可供 GDB 等调试器使用的调试信息。
- 内联 IR 解析支持 `atomicrmw` 与 `cmpxchg`。
- 新增智能指针、容器、静态调用和系统库示例及集成测试。
- 更新 EBNF 和 ESSO 文档。
