# EOL 语言开发路线图 (Roadmap)

## 项目概述
EOL (Ethernos Object Language) 是一个始终编译为原生机器码的静态类型编程语言。

**核心定位：**
- 编译为原生可执行文件（Windows EXE / Linux ELF / macOS Mach-O）
- 无运行时依赖，无 VM，无 GC
- Java 语法风格，C++ 级别性能
- 显式内存管理（Arena、栈分配、手动堆分配）

**当前版本：0.3.2.0**

---

## 版本号规范 (0.B.M.P)

| 位置 | 名称 | 含义 | 示例 |
|------|------|------|------|
| 0 | Generation | 架构代际 | 0=LLVM后端, 1=自托管, 2=内存安全 |
| B | Big | 功能域里程碑 | 0.1=原型, 0.2=当前, 0.3=控制流完善... |
| M | Middle | 特性集群 | 0.3.1.x=循环家族 |
| P | Patch | 每日构建修复 | 0.3.1.0->0.3.1.1 |

---

## 已完成功能 (0.1.x - 0.2.x)

### 0.1.x 原型阶段
- [x] 基础词法/语法分析器
- [x] LLVM IR 代码生成
- [x] Windows EXE 输出
- [x] 基础类型（int, String, void）
- [x] 类和方法定义
- [x] if/else 和 while

### 0.2.x 当前阶段
- [x] 版本号集成（0.2.0）
- [x] 编译优化选项（LTO, PGO, SIMD）
- [x] IR 阶段优化（--opt-ir）
- [x] 完整的编译器驱动（eolc/eolll/ir2exe）

---

## 阶段一：控制流完善 (0.3.x.x)

### 0.3.1.x 循环家族
- [x] **for 循环** - Java 风格 `for (int i = 0; i < n; i++)`
- [x] **增强 for 循环** - `for (Type item : collection)` 遍历集合
- [x] **do-while 循环** - `do { ... } while (condition);`
- [x] **switch 语句** - Java 风格，支持 `case` 穿透和 `break`
- [x] **break/continue 标签** - 嵌套循环控制 `outer: for (...) ... break outer;`

### 0.3.2.x 类型系统扩展（已完成）
- [x] **浮点类型** - `float`, `double` 支持（词法分析器、类型定义、代码生成已实现）
- [x] **字符类型** - `char` 类型和字符字面量 `'A'`（词法分析器、类型定义、代码生成已实现）
- [x] **布尔类型** - 原生 `boolean` 类型（true/false）（词法分析器、类型定义、代码生成已实现）
- [x] **long 类型** - 64位有符号整数（类型定义已实现）
- [x] **类型转换** - 显式强制转换 `(int)value`（语法解析器、AST、代码生成已实现）
- [x] **优化当前系统** - 将字面量类型标准化 (如数字默认int，小数默认double等)（0.3.2.1 完成）
- [x] **字面量隐式类型转换** - 支持字面量在赋值、算术运算等场景中的隐式类型转换（0.3.2.1 完成）
- [x] **业内规范的字面量方法** - 支持十六进制 (`0x`)、二进制 (`0b`)、八进制 (`0o`) 字面量，支持下划线分隔符，支持后缀 `L`、`f`、`d` 等（0.3.2.1 完成）
- [x] **数组功能** - 数组功能 (0.3.2.2完成)
- [x] **print** (0.3.2.2完成)
- [x] **println** (0.3.2.2完成)
- [x] **readInt** (0.3.2.2完成)
- [x] **readFloat** (0.3.2.2完成)
- [x] **readLine** (0.3.2.2完成)

### 0.3.3.x 数组完备
- [x] **多维数组** - `int[][] matrix = new int[3][3];`
- [x] **数组初始化** - `int[] arr = {1, 2, 3};`
- [x] **数组长度** - `arr.length` 属性
- [x] **数组边界检查** - 运行时安全检查

### 0.3.4.x 字符串与方法
- [x] **字符串增强** - `String` 类方法（substring, indexOf, replace等）
  - `int length()` - 获取字符串长度
  - `String substring(int begin)` / `String substring(int begin, int end)` - 截取子串
  - `int indexOf(String str)` - 查找子串位置
  - `String replace(String old, String new)` - 替换子串
  - `char charAt(int index)` - 获取指定位置字符
  - 示例见: `examples/test_0_3_4_features.eol`
- [x] **方法重载** - 同名不同参数列表
  *因EOL语法高亮各大平台基本都不支持，这里使用Java语法高亮做演示*
  ```java
  public static int add() { return 0; }
  public static int add(int a) { return a; }
  public static int add(int a, int b) { return a + b; }
  public static double add(double a, double b) { return a + b; }
  ```
  - 示例见: `examples/test_0_3_4_features.eol`
- [x] **可变参数** - `void method(String fmt, Object... args)`
  *因EOL语法高亮各大平台基本都不支持，这里使用Java语法高亮做演示*
  ```java
  public static int sum(int... numbers) { /* ... */ }
  public static int multiplyAndAdd(int multiplier, int... numbers) { /* ... */ }
  ```
  - 示例见: `examples/test_0_3_4_features.eol`
- [ ] **方法引用** - 静态/实例方法引用 `ClassName::methodName`
- [ ] **Lambda 表达式** - `(params) -> { body }`

---

### 阶段二：面向对象核心 (0.4.x.x)
**目标**：建立完整的 OOP 语义，支持典型的系统级抽象（如设备驱动框架、资源管理器）。

#### 0.4.0.x 基础继承体系（基础里程碑）
- [ ] **单继承模型** - `class Child extends Parent`，严格单继承避免菱形继承复杂性
- [ ] **虚函数表（vtable）布局** - 确定 C++ 兼容的 vtable 结构，支持后续 FFI
- [ ] **方法重写与隐藏** - `@Override` 编译期检查，默认虚函数（非 Java 的默认 final）
- [ ] **访问控制基础** - `public/private/protected`，其中 `protected` 允许包内访问（同 Java）

#### 0.4.1.x 多态与抽象（设计模式支持）
- [ ] **动态分派** - 通过 vtable 实现运行时多态，确保零开销（不采用 fat pointer）
- [ ] **抽象类** - `abstract class` 与纯虚函数（`= 0` 语法或 `abstract` 方法）
- [ ] **接口（单实现版本）** - 先支持单接口实现 `implements Interface`，为后续多接口预留 vtable 空间
- [ ] **类型转换** - `instanceof` 运算符与安全的向下转型（生成类型检查代码）

#### 0.4.2.x 构造体系与初始化顺序
- [ ] **构造函数基础** - 默认构造函数、显式构造函数定义
- [ ] **构造链** - `this(...)` 同链调用，`super(...)` 父类构造（强制首行）
- [ ] **成员初始化顺序** - 定义字段初始化、实例块、构造函数的执行顺序规范
- [ ] **析构函数（核心差异）** - `~ClassName()` 或 `dispose()` 方法，配合 RAII 模式（为 G2 所有权做铺垫）

#### 0.4.3.x 静态与 Final 语义
- [ ] **final 类/方法** - 禁止继承与重写，允许编译器去虚拟化（devirtualization）
- [ ] **静态成员** - `static` 字段与方法，明确静态存储期（BSS/data 段）
- [ ] **静态初始化** - `static { ... }` 块，定义模块加载时的初始化顺序（解决循环依赖检测）
- [ ] **常量表达式** - `static final` 编译期常量，用于数组大小等（类似 C++ 的 `constexpr` 基础）

**阶段交付物**：可编写典型的资源管理类（如文件句柄包装器、网络连接管理器），支持基本的 RAII 模式。

---

### 阶段三：零开销标准库 (0.5.x.x)
**目标**：建立无 GC、显式内存管理的标准库，证明 EOL 可以替代 C++ 用于系统编程。

#### 0.5.0.x 内存管理与分配器基础（关键基础设施）
- [ ] **分配器接口（Allocator trait）** - `interface Allocator { allocate(size, align); deallocate(ptr); }`
- [ ] **GlobalAlloc** - 默认堆分配器（封装 malloc/free 或系统调用）
- [ ] **Arena 分配器** - 线性分配器，支持批量释放（适合编译器、游戏帧分配）
- [ ] **栈分配标记** - `scope` 关键字或注解，支持栈上对象（值类型语义准备）

#### 0.5.1.x 基础类型与字符串（无 Object 根类）
- [ ] **基础值类型** - 明确 `int`, `long`, `float`, `double`, `bool` 的内存布局（固定宽度，如 i32/i64）
- [ ] **String 设计（不可变）** - 结构体 `{ char* data; usize len; }`，支持 SSO（短字符串优化，16/23 字节内栈存储）
- [ ] **StringBuilder** - 基于 Arena 或显式容量预分配的可变字符串
- [ ] **Optional<T>** - 取代 null，显式空值处理 `Option<String>`，编译期非空检查基础

#### 0.5.2.x 泛型集合（单态化实现）
- [ ] **泛型基础** - `class ArrayList<T, A: Allocator>`，单态化生成专用代码（如 `ArrayList_i32`）
- [ ] **显式分配器参数** - 所有集合必须携带分配器：`ArrayList<int> list = new ArrayList<>(arena);`
- [ ] **核心集合**：
  - `ArrayList<T>` - 动态数组，支持 reserve/shrink_to_fit
  - `HashMap<K,V>` - 开放寻址法或 Robin Hood 哈希，无二次指针间接
  - `HashSet<T>` - 基于 HashMap 的特化
- [ ] **迭代器** - 基础迭代器协议 `interface Iterator<T> { bool hasNext(); T next(); }`，支持范围 for 循环

#### 0.5.3.x 智能指针与资源管理
- [ ] **UniquePtr<T>** - 独占所有权，可移动（move），不可复制，自动调用析构
- [ ] **ScopedPtr<T>** - 栈作用域指针，禁止堆分配
- [ ] **Rc<T>**（引用计数）- 循环依赖检测（debug 模式），为 G2 的借用检查做过渡
- [ ] **弱引用基础** - `WeakPtr<T>`，解决循环引用（此时需手动打破循环）

#### 0.5.4.x 系统级 I/O
- [ ] **File 与 Path** - 封装系统调用（Windows: HANDLE, Linux: fd），支持 RAII 关闭
- [ ] **缓冲区 I/O** - `BufferedReader/Writer`，显式缓冲区大小参数
- [ ] **内存映射文件** - `Mmap` 类型，支持大文件零拷贝处理
- [ ] **错误处理基础** - `Result<T, E>` 类型，替代异常用于 I/O 错误（为 0.6.x 做准备）

**阶段交付物**：可编写无内存泄漏的文件复制工具、HTTP 服务器基础框架，性能与 C++ 同级。

---

### 阶段四：错误处理与并发 (0.6.x.x)
**目标**：建立系统级的错误传播机制和零成本并发抽象。

#### 0.6.1.x 错误处理机制（非异常体系）
- [ ] **Result<T, E> 泛型** - 显式错误传播 `Result<File, IOError>`
- [ ] **问号运算符** - `file.read()?` 自动展开错误传播（类似 Rust 的 `?` 或 Zig 的 `try`）
- [ ] **错误类型层级** - `interface Error { string message(); }`，支持错误链（error chaining）
- [ ] **panic/abort** - 不可恢复错误，调用栈回退或立即终止（可选 unwind 实现）

*设计决策*：取消 Java 式异常，采用类似 Rust/Zig 的错误码机制，确保无运行时异常处理开销。

#### 0.6.2.x 轻量级并发（1:1 线程模型）
- [ ] **OS 线程封装** - `Thread` 类，直接映射 pthread/Windows Thread
- [ ] **线程参数传递** - 必须显式指定数据所有权转移（为 G2 所有权系统做铺垫）
- [ ] **原子操作** - `AtomicI32`, `AtomicPtr<T>`，封装 C++11 风格内存序（Relaxed/Release/Acquire/SeqCst）
- [ ] **互斥锁** - `Mutex<T>`，封装 OS 层 mutex（futex 或 CriticalSection），非语言级 synchronized

#### 0.6.3.x 异步 I/O 基础（非协程，基于 epoll/io_uring）
- [ ] **Reactor 模式** - 单线程事件循环，支持 Linux epoll/Windows IOCP
- [ ] **异步文件 I/O** - 基于 io_uring（Linux）或 Overlapped I/O（Windows）
- [ ] **Future/Promise 基础** - 回调式异步，显式状态机转换（无 async/await 语法糖，纯库实现）

**阶段交付物**：可编写高性能反向代理、键值存储服务，具备系统级错误处理和并发能力。

---

### 阶段五：模块系统与工具链 (0.7.x.x)
**目标**：建立生产级工程能力，支持中大型项目开发。

#### 0.7.1.x 包管理器（eolpm）
- [ ] **包声明** - `package com.ethernos.std;`
- [ ] **模块清单** - `eol.toml`（类似 Cargo），声明依赖、版本、编译选项
- [ ] **语义化版本** - 严格遵循 SemVer，支持 lock 文件确保可复现构建
- [ ] **本地/远程仓库** - 支持 Git 依赖和中央仓库（registry）

#### 0.7.2.x 编译单元与链接
- [ ] **模块化编译** - 增量编译，接口文件（.eoi）生成，类似 C++ 模块或 Swift 模块
- [ ] **静态/动态链接** - 生成 .a/.so/.lib/.dll，支持 C ABI 导出
- [ ] **LTO（链接时优化）** - 跨模块内联，基于 LLVM LTO

#### 0.7.3.x 开发工具
- [ ] **LSP 服务器** - 基于编译器前端，支持跳转、补全、重构
- [ ] **调试信息** - DWARF/PDB 生成，支持 GDB/LLDB/VS Debugger
- [ ] **格式化工具** - `eolfmt`，确定官方代码风格（类似 gofmt）
- [ ] **静态分析** - 基础 lint 规则（未使用变量、内存泄漏风险检测）

**阶段交付物**：可用 EOL 编写 10 万行级项目（如编译器自身前端），具备完整工具链支持。

---

### 阶段六：底层控制与优化 (0.8.x.x)
**目标**：提供底层硬件控制能力和极致性能优化。

#### 0.8.1.x Unsafe 子集（为 G2 做准备）
- [ ] **unsafe 块** - `unsafe { ... }`，内部允许：原始指针解引用、union 访问、调用 C 函数
- [ ] **原始指针** - `*T` 和 `*mut T`，支持指针运算
- [ ] **类型转换** - `transmute<T, U>`（位重解释），`as` 关键字基础转换
- [ ] **内联汇编** - `asm!()` 宏，支持 x86_64/ARM64 内联汇编（类似 Rust 的 asm!）

#### 0.8.2.x 编译器优化与 SIMD
- [ ] **自动向量化** - LLVM auto-vectorization 调优，支持 AVX2/AVX-512/NEON
- [ ] **显式 SIMD** - `std.simd.Vec4f` 等类型，封装 SIMD 指令
- [ ] **内存布局控制** - `#[repr(C)]`, `#[repr(packed)]`, `#[align(N)]` 属性
- [ ] **零成本抽象验证** - 确保泛型、迭代器等抽象最终编译为与手写 C 等价的机器码

#### 0.8.3.x 嵌入式与裸机支持
- [ ] **no_std** - 支持无标准库环境，不链接 libc
- [ ] **启动代码** - 自定义 `_start`，支持裸机 ARM/RISC-V 编程
- [ ] **内存映射 I/O** - `volatile` 读写语义，支持 MMIO 寄存器操作

**阶段交付物**：可编写操作系统内核模块、嵌入式固件、高性能计算库（如矩阵运算），完全替代 C/C++ 在系统编程领域的地位。

---

## G1 代：自举与现代化（1.x.x.x）
**目标**：用 EOL 重写自身编译器，引入现代语言特性，提升表达力。

### 1.0.x 编译器自举（里程碑版本）
- [ ] **前端迁移** - 词法分析器、语法分析器、AST 生成全部用 EOL 编写
- [ ] **LLVM IR 生成** - 继续使用 LLVM 后端，但驱动代码为 EOL
- [ ] **引导编译** - 使用 G0 编译器（0.8.x）编译 G1 编译器，再用 G1 编译器自举验证
- [ ] **性能基准** - 自举编译速度不低于 G0 版本的 90%

### 1.1.x 语法糖与提升开发体验
- [ ] **类型推断增强** - `var` 关键字局部变量推断，`auto` 返回值推断（限于单 return）
- [ ] **解构赋值** - `val (x, y) = point;`，支持元组和结构体
- [ ] **范围与迭代** - `for i in 0..100 { ... }`（半开区间），支持自定义迭代器
- [ ] **字符串模板** - `"Hello, \(name)"` 或 `"Hello, ${name}"`，编译期解析

### 1.2.x 函数式编程支持
- [ ] **Lambda 表达式** - `(x: int) => x * 2`，支持闭包（捕获环境）
- [ ] **高阶函数** - 函数作为一等公民，支持函数类型 `fn(int) -> int`
- [ ] **不可变集合** - `ImmutableList<T>`，基于持久化数据结构（HAMT 等）
- [ ] **管道操作符** - `value |> transform |> filter`，左结合

### 1.3.x 高级类型系统
- [ ] **代数数据类型（ADT）** - `enum Option<T> { Some(T), None }`，支持模式匹配
- [ ] **模式匹配基础** - `match` 表达式，支持常量、范围、元组匹配
- [ ] **泛型约束** - `where T: Comparable`，泛型边界细化
- [ ] **关联类型** - `interface Container { type Item; }`

### 1.4.x 异步与并发语法糖（基于 G0 的 I/O 基础）
- [ ] **async/await** - 基于 G0 阶段的手动 Future，编译器生成状态机
- [ ] **协程（绿色线程）** - `async fn` 支持，M:N 线程模型可选
- [ ] **结构化并发** - `async { ... }` 块，自动取消传播

---

## G2 代：内存安全纪元（2.x.x.x）
**目标**：引入所有权与借用检查系统，实现编译期内存安全，消除 use-after-free 和数据竞争。

### 2.0.x 所有权系统核心（代际升级标记）
- [ ] **所有权语义** - 默认移动语义，复制需显式实现 `Copy` trait
- [ ] **借用检查器（Borrow Checker）** - 编译期跟踪引用生命周期
- [ ] **引用类型** - `&T`（不可变借用），`&mut T`（可变借用），严格 XOR 规则
- [ ] **生命周期标注** - 显式生命周期 `'a`，函数签名如 `fn max<'a>(x: &'a T, y: &'a T) -> &'a T`
- [ ] **RAII 强化** - Drop trait 自动调用，与所有权转移结合

### 2.1.x 高级内存安全
- [ ] **非词法生命周期（NLL）** - 更精确的借用范围分析
- [ ] **内部可变性** - `Cell<T>`, `RefCell<T>`（单线程），`Mutex<T>`, `RwLock<T>`（多线程）的 unsafe 内部实现
- [ ] **智能指针集成** - `Box<T>`（堆唯一所有权），`Arc<T>`（原子引用计数，线程安全共享）
- [ ] **弱引用与循环检测** - `Weak<T>`，编译期警告潜在循环引用（辅助 lint）

### 2.2.x 并发安全
- [ ] **Send/Sync trait** - 标记类型是否可跨线程发送/共享，编译期数据竞争检测
- [ ] **通道（Channels）** - `Sender<T>/Receiver<T>`，所有权转移实现无锁消息传递
- [ ] **无锁数据结构** - `AtomicQueue<T>`, `AtomicStack<T>`，基于 CAS 操作

### 2.3.x 编译期计算与元编程
- [ ] **常量泛型** - `Array<T, N>` 其中 N 为编译期常量
- [ ] **编译期函数执行** - `const fn`，可在编译期计算复杂逻辑
- [ ] **宏系统** - 卫生宏（hygienic macros），`macro!()` 与 `macro_rules!`
- [ ] **反射（编译期）** - `typeof`, `offsetof`, 生成序列化代码（零成本反射）

### 2.4.x 与 G1/G0 的互操作
- [ ] ** unsafe 桥接** - 在 G2 代码中调用 G1/G0 的不安全代码，需显式 `unsafe` 块
- [ ] **迁移路径** - 允许 G1 代码逐步添加所有权标注升级为 G2，提供 `#[legacy]` 属性允许无所有权代码存在
- [ ] **FFI 安全封装** - 自动生成 C 头文件的安全包装层

---

## 演进时间线参考

| 代际 | 预计周期 | 关键里程碑 |
|------|----------|------------|
| **G0** | 2-3 年 | 生产可用（0.8.0），可替代 C++ 编写高性能服务 |
| **G1** | 1.5-2 年 | 自举完成（1.0.0），语言稳定，生态建设 |
| **G2** | 2-3 年 | 内存安全（2.0.0），进入 Linux 内核、嵌入式等最高安全要求领域 |

**总计**：5-8 年达到完全体，符合系统编程语言的成熟周期（参考 Rust 1.0 到广泛采用约 5 年，C++ 标准化周期）。

---

## 关键设计决策备忘

1. **G0 与 G1 的边界**：G0 证明 EOL 可以系统编程，G1 证明 EOL 可以大规模工程开发。G0 保留手动内存管理（类似 C++），G1 不引入所有权，但完善类型系统和语法糖。

2. **G1 与 G2 的边界**：G2 是可选的严格模式。G1 代码可在 G2 编译器中通过 `#[edition(G1)]` 继续运行，确保向后兼容。G2 的所有权系统是**渐进式**的，而非强制立即迁移。

3. **异常 vs 错误码**：G0 和 G1 采用 `Result<T,E>` 为主，G2 可能引入 `?` 传播和更复杂的错误处理，但始终保持零开销（no unwinding cost）。

4. **GC 永不引入**：所有代际均不提供垃圾回收，确保与 C/C++/Rust 同级的内存可控性。

5. **C++ 互操作优先**：每一代都保持与 C++ ABI 的兼容性（通过 extern "C++" 或类似机制），确保可以调用现有系统库。


**注意：** 本路线图会根据实际开发情况和社区反馈进行调整。
