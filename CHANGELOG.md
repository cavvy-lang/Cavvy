# Changelog

所有对 Cavvy 编译器项目的显著变更都会记录在此文件中。

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 规范，
并且版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/) 语义化版本规范。

## [Unreleased]

## [5.3.0] - 2026-07-11

### Added

- **语言特性**

  - 支持命名空间风格的静态方法调用 `ClassName::staticMethod(args)`，
    包括 `Namespace::ClassName::staticMethod(args)` 限定形式。
  - 支持省略 `new` 关键字的类实例化语法：`ClassName(args)` 与
    `ClassName<T>(args)` 等价于对应的 `new` 表达式。
  - 添加泛型静态工厂方法推断与实例方法类型推断支持，
    允许链式调用如 `ClassName(args).method()`。
  - 新增代码风格警告机制，统一收集并打印编译过程中的警告信息。
  - 增强错误处理，在诊断行号为 0 时输出调试信息，便于定位无源码位置的错误。
- **标准库**

  - 新增 `std::sys` 系统标准库，提供进程控制、环境变量访问和命令行参数包装功能。
  - 实现分配器支撑的 `std::ArrayList` 动态数组，支持泛型与自定义分配器。
  - 新增 `std::vector` 容器与迭代器实现。
  - 引入四种智能指针类型：`UniquePtr`、`ScopedPtr`、`Rc` 和 `WeakPtr`，
    并支持自动 RAII 资源管理。
  - 增强内置函数支持，新增 `eprint`、`eprintln` 和 `exit` 函数。
- **代码生成与调试**

  - 新增 `-g` 参数支持，使 Cavvy 编译产物可被 GDB 等调试器调试。
  - 新增 `__cay_alloc_array` 内建表达式及配套的语法解析、语义分析与代码生成支持。
  - 增强特化收集器以支持嵌套泛型类型解析。
  - 新增内联 IR 解析支持原子操作 `atomicrmw` 与 `cmpxchg`。
- **测试与示例**

  - 为智能指针、数组/向量、命名空间静态方法、`optional-new` 实例化、
    `std::sys` 等特性新增大量示例程序与集成/回归测试。
- **文档与工程**

  - 新增 ESSO 文档子模块并更新 `.gitmodules`。
  - 更新 EBNF 以反映 `ClassName::staticMethod`、`ClassName(args)` 等新语法。

### Fixed

- **代码生成**

  - 修复 Itanium C++ ABI 名称修饰中 `E` 终止符位置错误的问题。
  - 对 native/abstract 方法与构造函数仅生成 `declare` 而不再 `define`。
  - 修复 interop 类默认包含 Cavvy 对象头与默认构造函数的问题。
  - 允许在命名空间块内声明 interop 类。
  - 修复命名参数与可变参数调用点的名称修饰不匹配问题。
  - 修复结构体实例方法 `this` 指针类型不匹配问题。
  - 在 RAII 析构与 `Optional` 注入中使用 Itanium D1 析构函数命名。
  - 缓存链式调用的对象表达式求值结果，避免重复生成带副作用的代码。
- **脚本与文档**

  - 修复 `doc-test.py` 中代码块标识与命令执行编码问题，
    将 `text=True` 替换为 `encoding=utf-8` 并添加 `errors=replace`。
  - 更新预处理器文档，移除不必要的宏定义说明。
  - 修正当前实现状态文档中泛型单态化与 Lambda 闭包捕获的实现状态。

### Changed

- 将 Windows MSVC 目标链接器切换为 `rust-lld`。
- 重构代码结构，提升可读性与可维护性。
- 统一多个测试文件与核心模块的格式化风格。
