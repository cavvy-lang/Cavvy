# Changelog

所有对 Cavvy 编译器项目的显著变更都会记录在此文件中。

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/) 规范，
并且版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/) 语义化版本规范。

## [Unreleased]

### Fixed

- **构建**：`.verinfo` 为空或缺失 `[CAYC] version` 时 `build.rs` 回退到 `CARGO_PKG_VERSION`
  并打印警告（此前会静默缺失 `CAY*_VERSION` 宏导致编译失败）；移除对 `.git/index`
  的监听（任何 stage 操作都会触发全量重编）；16 段版本环境变量设置改为表驱动。
- **预处理器**：`#if !defined(X)`、`#if A&&B` 等无空格写法不再静默求值错误；
  宏展开跳过字符串/字符字面量且宏表排序移出逐字符循环；`#define` 值中的
  `http://` 不再被当作注释截断；`#if` 表达式解析失败发出警告而非静默为 false。
- **词法**：整数/浮点字面量超出范围时产生明确错误（启用此前从未构造的
  `InvalidNumberLiteral`），不再退化为 `IntegerLiteral(None)`；非法转义序列报
  `InvalidEscapeSequence`。
- **解析器**：内联 IR 不再静默丢弃 token（曾导致 `add i32 -5, 0` 负号被吞的
  silent miscompile）；`(a) + b` 类表达式可正确解析（cast 预读失败时回退）；
  `c_uint64_t` 报「暂不支持」（此前被静默映射为有符号 `Int64`）；`case -1:`
  支持负常量；重复/冲突修饰符报错；同名嵌套 namespace 路径不再被静默去重。
- **语义**：null 字面量获得独立类型标记，`Object` 实例不再能赋给任意类型；
  比较运算符检查操作数类型（`"abc" < 42` 报错）；数组初始化检查所有元素；
  重载决议失败候选的错误不再污染全局错误列表；`Type::is_integer` 补齐；
  `size_in_bytes` 不再对 `Type::Auto` panic；父链遍历全部加防环。
- **代码生成**：字段偏移查找失败、类大小未知、方法签名解析失败均改为硬错误
  （此前分别静默回退为偏移 0、8 字节、`"x"` 占位符）；IR 混淆器不再破坏
  `c"..."` 字符串字面量内容，且不再混淆外部符号（declare）与 `main`；
  默认目标三元组按宿主平台探测（此前硬编码 Windows）。
- **诊断**：移除 `emit_zero_line_debug_info`（任何行号为 0 的错误都会往用户
  工作目录写含完整源代码的 `debug_*.txt`）；`Severity::Fatal` 及中文魔法字符串
  判断移除，严重级别由结构化 `is_warning` 决定；多字节标识符高亮长度按字节
  计算；删除约 400 行零引用的「保留供未来使用」死类型。
- **cavly 安全**：未配置官方根公钥时拒绝携带官方签名的包（fail-closed，此前
  静默跳过验证）；本地包完整性校验失败不再被 `let _ =` 吞掉；修复 PowerShell
  下载分支命令注入与可预测临时文件名；服务器下发的 fingerprint 入路径前做
  格式校验（防路径遍历）；`curl` 加 `--fail`（404 不再被当作包数据）；证书
  过期时间纳入校验并记录审计事件。
- **工具**：`cay-lsp` 修复 UTF-16/字节混用导致的中文行 panic 与文档符号行号
  off-by-one；`cay-dt --json` 改用 serde_json（此前输出带尾随逗号的非法 JSON）；
  `cay-run` 被信号杀死时返回 128+signal；RCPL 修复 Ctrl-D 忙等死循环；
  工具链查找改为捆绑 LLVM 优先、PATH 兜底（hermetic）；删除永不编译的
  `src/main.rs`，`cay-pre` 接入 Cargo 构建。

### Changed

- **字节码子系统降级为实验性**：字节码混淆器（混淆为假功能且控制流混淆会改变
  程序语义）、`cay-run --obfuscate/--bytecode`（产出空程序）、JIT（生成非法
  LLVM IR）现在全部明确报错而非静默产出错误程序；`cay-bcgen` 对 break/continue
  生成真实跳转，未定义变量、不支持构造全部硬报错。
- **核心原则确立**：编译器/工具宁可 noisy 报错，不可 silently wrong。
  静默回退默认值、`let _ =` 吞错、`_ => {}` 丢弃构造均视为缺陷。

## [6.1.0] - 2026-07-14

### Added

- **错误处理**

  - 新增泛型 `std::Result<T, E>` 与 `std::Error` 错误类型层级，支持 `ok`、`err`、
    `unwrap`、`expect`、`map`、`mapErr`、`andThen` 等操作。
  - 新增 `std::IOError`，提供文件和 I/O 错误分类、原始操作系统错误码及错误信息。
  - 新增 `?` Result 问号运算符，支持函数中的错误自动传播。
  - 新增 `panic`/`abort` 内置错误终止函数。
- **语言与工具链**

  - 完成 `?` 运算符的词法/语法解析、语义检查和 LLVM IR 代码生成。
  - 更新 EBNF、错误处理路线图及相关示例和集成测试。

### Fixed

- 修复 Result 错误传播代码生成、返回类型推断及调用表达式处理中的兼容性问题。

### Changed

- 项目版本号升级至 `6.1.0`。

## [5.5.0] ～ [6.0.0]

### Changed

- 该阶段未创建独立版本标签；版本演进期间持续完善标准库、泛型、智能指针、
  系统 I/O、诊断系统和工具链能力，相关功能在 5.4.0 和 6.1.0 的正式记录中汇总。

## [5.4.0] - 2026-07-13

### Added

- **标准库**

  - 新增 `std::Mmap` 内存映射文件支持，提供跨平台只读/读写映射、
    同步回盘与 RAII 资源释放。
    - Windows: `CreateFileMappingA` / `MapViewOfFile`
    - Linux: `mmap` / `munmap` / `msync`
  - 新增 `std::MmapSlice` 零拷贝切片视图，支持 `get`/`set`/`size`。
  - 新增 `examples/test_mmap.cay`，覆盖只读/读写映射、切片、越界、
    空文件、读写持久化与 64KB 压力测试。
  - 在 `tests/file_lib_tests.rs` 中新增 `test_mmap_full` 与
    `test_mmap_empty_file` 集成测试。

### Fixed

- **代码生成**

  - 修复泛型类重载构造器解析：为泛型参数签名（`g*`）增加通配得分，
    避免泛型构造器被无关重载覆盖。
  - 修复整数到指针的显式类型转换 IR 生成：依据语义目标类型
    `Pointer(_)` 生成 `inttoptr`，确保 `(c_void*)longValue` 不再被错误编译为
    字符串转换，从而修复 `Mmap.unmap` 与 `INVALID_HANDLE` 比较。

### Changed

- 项目版本号升级至 `5.4.0`。

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
