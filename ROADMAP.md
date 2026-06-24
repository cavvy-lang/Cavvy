# Cavvy 语言开发路线图 (Roadmap)

## 项目概述

Cavvy (原Ethernos Object Language) 是一个始终编译为原生机器码的静态类型编程语言。

**核心定位：**

- 编译为原生可执行文件（Windows EXE / Linux ELF / macOS Mach-O）
- 无运行时依赖，无 VM，无 GC
- Java 语法风格，C++ 级别性能
- 显式内存管理（Arena、栈分配、手动堆分配）

---

## 版本号规范 (GB.M.P)

| 位置 | 名称       | 含义         | 示例                                  |
| ---- | ---------- | ------------ | ------------------------------------- |
| G    | Generation | 架构代际     | 0=LLVM后端, 1=自托管, 2=内存安全      |
| B    | Big        | 功能域里程碑 | 0.1=原型, 0.2=当前, 0.3=控制流完善... |
| M    | Middle     | 特性集群     | 0.3.1.x=循环家族                      |
| P    | Patch      | 每日构建修复 | 0.3.1.0->0.3.1.1                      |

---

## 已完成功能 (0.1.x - 0.2.x)

### 0.1.x 原型阶段

- [X] 基础词法/语法分析器
- [X] LLVM IR 代码生成
- [X] Windows EXE 输出
- [X] 基础类型（int, String, void）
- [X] 类和方法定义
- [X] if/else 和 while

### 0.2.x 当前阶段

- [X] 版本号集成（0.2.0）
- [X] 编译优化选项（LTO, PGO, SIMD）
- [X] IR 阶段优化（--opt-ir）
- [X] 完整的编译器驱动（cayc/cay-ir/ir2exe）

---

## 阶段一：控制流完善 (0.3.x.x)

### 0.3.1.x 循环家族

- [X] **for 循环** - Java 风格 `for (int i = 0; i < n; i++)`
- [X] **增强 for 循环** - `for (Type item : collection)` 遍历集合
- [X] **do-while 循环** - `do { ... } while (condition);`
- [X] **switch 语句** - Java 风格，支持 `case` 穿透和 `break`
- [X] **break/continue 标签** - 嵌套循环控制 `outer: for (...) ... break outer;`

### 0.3.2.x 类型系统扩展（已完成）

- [X] **浮点类型** - `float`, `double` 支持（词法分析器、类型定义、代码生成已实现）
- [X] **字符类型** - `char` 类型和字符字面量 `'A'`（词法分析器、类型定义、代码生成已实现）
- [X] **布尔类型** - 原生 `boolean` 类型（true/false）（词法分析器、类型定义、代码生成已实现）
- [X] **long 类型** - 64位有符号整数（类型定义已实现）
- [X] **类型转换** - 显式强制转换 `(int)value`（语法解析器、AST、代码生成已实现）
- [X] **优化当前系统** - 将字面量类型标准化 (如数字默认int，小数默认double等)（0.3.2.1 完成）
- [X] **字面量隐式类型转换** - 支持字面量在赋值、算术运算等场景中的隐式类型转换（0.3.2.1 完成）
- [X] **业内规范的字面量方法** - 支持十六进制 (`0x`)、二进制 (`0b`)、八进制 (`0o`) 字面量，支持下划线分隔符，支持后缀 `L`、`f`、`d` 等（0.3.2.1 完成）
- [X] **数组功能** - 数组功能 (0.3.2.2完成)
- [X] **print** (0.3.2.2完成)
- [X] **println** (0.3.2.2完成)
- [X] **readInt** (0.3.2.2完成)
- [X] **readFloat** (0.3.2.2完成)
- [X] **readLine** (0.3.2.2完成)

### 0.3.3.x 数组完备

- [X] **多维数组** - `int[][] matrix = new int[3][3];`
- [X] **数组初始化** - `int[] arr = {1, 2, 3};`
- [X] **数组长度** - `arr.length` 属性
- [X] **数组边界检查** - 运行时安全检查

### 0.3.4.x 字符串与方法

- [X] **字符串增强** - `String` 类方法（substring, indexOf, replace等）

  - `int length()` - 获取字符串长度
  - `String substring(int begin)` / `String substring(int begin, int end)` - 截取子串
  - `int indexOf(String str)` - 查找子串位置
  - `String replace(String old, String new)` - 替换子串
  - `char charAt(int index)` - 获取指定位置字符
  - 示例见: `examples/test_0_3_4_features.cay`
- [X] **方法重载** - 同名不同参数列表
  *因Cavvy语法高亮各大平台基本都不支持，这里使用Java语法高亮做演示*

  ```java
  public static int add() { return 0; }
  public static int add(int a) { return a; }
  public static int add(int a, int b) { return a + b; }
  public static double add(double a, double b) { return a + b; }
  ```

  - 示例见: `examples/test_0_3_4_features.cay`
- [X] **可变参数** - `void method(String fmt, Object... args)`
  *因Cavvy语法高亮各大平台基本都不支持，这里使用Java语法高亮做演示*

  ```java
  public static int sum(int... numbers) { /* ... */ }
  public static int multiplyAndAdd(int multiplier, int... numbers) { /* ... */ }
  ```

  - 示例见: `examples/test_0_3_4_features.cay`
- [X] **方法引用** - 静态/实例方法引用 `ClassName::methodName`
- [X] **Lambda 表达式** - `(params) -> { body }`

---

### 阶段二：面向对象核心 (0.4.x.x)

**目标**：建立完整的 OOP 语义，支持典型的系统级抽象（如设备驱动框架、资源管理器）。

#### 0.4.0.x 基础继承体系（基础里程碑）

- [X] **单继承模型** - `class Child extends Parent`，严格单继承避免菱形继承复杂性
- [ ] **虚函数表（vtable）布局** - 确定 C++ 兼容的 vtable 结构，支持后续 FFI
  - **状态**：未实现。当前接口方法调用使用声明类型解析，不支持运行时动态分发。
  - **已知限制**：`Animal a = new Dog(); a.speak();` 会调用第一个实现类的方法，而非 Dog.speak()。
  - **修复方案**：需要实现 vtable 结构和动态分派机制。
- [X] **方法重写与隐藏** - `@Override` 编译期检查，默认虚函数（非 Java 的默认 final）
  - **状态**：已实现。语义分析会检查 @Override 注解的方法是否存在父类方法。
- [ ] **访问控制基础** - `public/private/protected`，其中 `protected` 允许包内访问（同 Java）
  - **状态**：部分实现。is_private 和 is_protected 标志已存储在类型系统中，但代码生成时未强制执行。
  - **已知限制**：private 成员可以从外部访问，编译器不会报错。
  - **修复方案**：需要在语义分析或代码生成阶段添加访问控制检查。

#### 0.4.1.x 多态与抽象（设计模式支持）

- [X] **动态分派** - 通过 vtable 实现运行时多态，确保零开销（不采用 fat pointer）
- [X] **抽象类** - `abstract class` 与纯虚函数（`= 0` 语法或 `abstract` 方法）
- [X] **接口（单实现版本）** - 先支持单接口实现 `implements Interface`，为后续多接口预留 vtable 空间
- [X] **类型转换** - `instanceof` 运算符与安全的向下转型（生成类型检查代码）

#### 0.4.2.x 构造体系与初始化顺序

- [X] **构造函数基础** - 默认构造函数、显式构造函数定义
- [X] **构造链** - `this(...)` 同链调用，`super(...)` 父类构造（强制首行）
- [X] **成员初始化顺序** - 定义字段初始化、实例块、构造函数的执行顺序规范
- [X] **析构函数（核心差异）** - `~ClassName()` 或 `dispose()` 方法，配合 RAII 模式（为 G2 所有权做铺垫）

#### 0.4.3.x 语法增强与现代特性

- [X] **`var`, `let` 后置类型声明** - 支持变量声明时类型后置语法
  - `var x: int = 10;` - `var` 关键字声明可变变量，类型后置
  - `let y: String = "hello";` - `let` 与 `var` 完全相同
    - 都可以在前加 `final` 关键字，声明不可变变量，类型后置
  - 与现有 `int x = 10;` 语法并存，提供更现代的声明风格
- [X] **`auto` 自动类型推断** - 编译器自动推断变量类型
  - `auto x = 10;` - 推断为 `int` 类型
  - `auto s = "hello";` - 推断为 `String` 类型
  - `auto d = 3.14;` - 推断为 `double` 类型（默认浮点字面量类型）
  - 适用于局部变量、for循环变量等场景
- [X] **顶层 `main` 函数支持** - 允许在类外定义程序入口点
  - 必须是 `public` 访问修饰符
  - 返回值必须是 `int` 类型（程序退出码）
  - 支持参数 `String[] args` 或无参数版本
  - 示例：
    ```java
    public int main() {
        println("Hello, World!");
        return 0;
    }
    ```
  - 与现有的类内 `public static int main(String[] args)` 并存
    - 并存规则
      - 优先使用顶层 `main` 函数，避免与类内 `main` 冲突
      - 如果某类含有 `@main`注解，将使用该类的 `main`方法作为程序入口点

#### 0.4.4.x 静态与 Final 语义（已完成）

- [X] **final 类/方法** - 禁止继承与重写，允许编译器去虚拟化（devirtualization）
- [X] **静态成员** - `static` 字段与方法，明确静态存储期（BSS/data 段）
- [X] **静态初始化** - `static { ... }` 块，定义模块加载时的初始化顺序（解决循环依赖检测）
- [X] **常量表达式** - `static final` 编译期常量，用于数组大小等（类似 C++ 的 `constexpr` 基础）

**0.4.4.x 特性说明：**

1. **final 类语义**：声明为 `final class` 的类不能被继承，子类尝试继承会报错 "Class 'X' cannot inherit from final class 'Y'"
2. **final 方法语义**：声明为 `final` 的方法不能被重写，子类尝试重写会报错 "Method 'X' cannot override final method from class 'Y'"
3. **静态方法访问限制**：静态方法中访问非静态成员会报错 "non-static variable X cannot be referenced from a static context"
4. **编译期常量**：`static final` 修饰的字段且初始化值为字面量时，会被标记为编译期常量

**阶段交付物**：可编写典型的资源管理类（如文件句柄包装器、网络连接管理器），支持基本的 RAII 模式。

---

### 阶段2.5: 实现另类的"一次编译，到处运行"

- [X] **目标** - 实现Cavvy程序在不同平台上的可移植性，无需修改代码
- [X] **先编译出Cavvy的linux可执行文件** -

- [-] **能在Linux下编译**(需单独测试)
  -**迁移提示**（从 Windows MinGW 到 Linux）：
  - **目标三元组**：从 `x86_64-w64-mingw64` 改为 `x86_64-unknown-linux-gnu`（或 `x86_64-pc-linux-gnu`）
  - **链接方式**：Linux 默认动态链接 libc，如需分发独立二进制文件，记得加 `-static` 或 `-static-libgcc`
  - **系统调用**：之前 Windows 版用的 `SetConsoleOutputCP` 这类 Win32 API 需要条件编译或替换为 Linux 的 `setlocale`/`nl_langinfo`
- 什么是另类的"一次编译，到处运行"？
  - 一次编译，到处运行是指在不同操作系统上运行相同的文件，无需修改代码。
  - 这需要在编译时考虑到不同平台的差异，如系统调用、库函数等。
  - 具体运行流程是：cay-ir -> 生成的IR代码 -> cay -> 直接运行
  - 注意：在不同操作系统上运行时，需要确保IR代码在不同平台上的兼容性，避免依赖特定平台的指令集。

#### 0.4.5.x 多平台适配IR代码

- [X] **目标** - 实现Cavvy程序在不同平台上的可移植性，无需修改代码
- [X] **IR 代码适配** - 生成的 IR 代码在不同平台上的兼容性，避免依赖特定平台的指令集
- [X] **可选生成参数** - 原来的特定平台代码改为可选参数，使用 `-f:XX`或 `--feature:XX`开启，`-No:XX`关闭，`-D:XX`定义宏，`-U:XX`取消定义宏
- [X] **目标平台** - 支持 Windows、Linux、macOS 等主要操作系统
- [X] **支持混淆** - 支持混淆IR代码，防止被反编译和修改

#### 0.4.6.x 实现到处运行IR代码

- [X] **伪运行时**
  - 将IR代码编译到一个临时目录，运行时从该目录打开可执行文件
  - 支持动态链接库（如 `dlopen`），避免静态链接
- [X] **支持动态链接库** - 支持在不同平台上的动态链接库加载，避免依赖特定平台的链接器

#### 0.4.7.x Cavvy字节码系统 (CayBC)

- [X] **Cavvy字节码格式 (CayBC)** - 设计并实现Cavvy专属字节码格式
  - 基于栈的虚拟机指令集，类似JVM但针对Cavvy优化
  - 支持常量池、类型定义、函数定义、全局变量等
  - 字节码文件扩展名 `.caybc`
- [X] **字节码生成器 (cay-bcgen)** - 新增字节码生成工具
  - 将Cavvy源码编译为字节码文件
  - 支持三级混淆级别：light, normal, deep
  - 命令行：`cay-bcgen [--obfuscate] [--obfuscate-level <level>] <source.cay>`
- [X] **字节码混淆系统** - 保护字节码防止逆向工程
  - 符号名称混淆（将函数/类名替换为_0x前缀的十六进制）
  - 控制流混淆（插入不透明谓词）
  - 字符串加密
  - 调试信息剥离
- [X] **增强cay-run** - 支持多种输入格式
  - 支持 `.cay` 源码文件（直接编译运行）
  - 支持 `.caybc` 字节码文件（反序列化后编译运行）
  - 支持 `.ll` LLVM IR文件（直接编译运行）
  - 支持 `--obfuscate` 选项对源码进行字节码混淆后运行
  - 支持 `-l<lib>` 和 `-L<path>` 链接库选项
  - 自动检测并链接所需库（自动链接器）
- [X] **删除cay-dll** - 移除虚假/未实现的cay-dll工具
- [X] **自动链接器** - 智能库检测与链接
  - 分析源代码/IR自动推断需要的库
  - 支持Windows (user32, kernel32, ws2_32等)
  - 支持Linux (m, pthread, dl等)
  - 自动搜索系统库路径

#### 0.4.8.x 生态兼容

- [X] **手动 extern 声明** - 在 Cavvy 代码中直接声明 C 函数签名
  - 语法：`extern { 类型 函数(参数); }`
  - 支持调用约定标记：`extern stdcall { ... }` (Windows), `extern sysv64 { ... }` (Linux)
  - 类型映射：使用显式 FFI 类型（`c_int`, `c_long`, `size_t` 等）
- [X] **链接器集成** - 编译时自动链接系统库（`-lc`, `-lm`, `-luser32` 等）
  - 链接器自动分析 extern 声明中的 C 库函数并链接相应库

### 阶段三：零开销标准库 (0.5.x.x)

**目标**：建立无 GC、显式内存管理的标准库，证明 Cavvy 可以替代 C++ 用于系统编程。

#### 0.5.0.x 内存管理与分配器基础（关键基础设施）

- [X] **分配器接口（Allocator trait）** - `interface Allocator { allocate(size, align); deallocate(ptr); }`
- [X] **GlobalAlloc** - 默认堆分配器（封装 malloc/free 或系统调用）
- [X] **Arena 分配器** - 线性分配器，支持批量释放（适合编译器、游戏帧分配）
- [X] **栈分配标记** - `scope` 关键字或注解，支持栈上对象（值类型语义准备）

**实现文件**: `caylibs/Allocator.cay` | **测试**: `tests/allocator_tests.rs`

<details>
<summary>已实现的分配器 API</summary>

```java
public interface Allocator {
    long allocate(long size);
    long allocateAligned(long size, long alignBytes);
    void deallocate(long ptr);
}

public class GlobalAlloc implements Allocator {
    // 线程安全的全局堆分配器，封装 malloc/free
    public static GlobalAlloc getInstance();
    public long allocate(long size);
    public long allocateAligned(long size, long alignBytes);
    public void deallocate(long ptr);
}

public class Arena implements Allocator {
    // 线性分配器，仅记录偏移量，O(1) 分配
    public static Arena create(long cap);
    public long allocate(long size);
    public void reset();                   // 批量释放所有内存
    public long used();                    // 已用字节
    public long remaining();               // 剩余字节
}

public class ScopeAlloc implements Allocator {
    // 栈作用域分配器占位，实际栈分配由 scope 关键字处理
    public static ScopeAlloc create();
    public void setMarker(long m);
    public long getMarker();
}
```

</details>

#### 5.1.x 基础类型与字符串（无 Object 根类）

- [X] **基础值类型** - 内存布局已确定：`int`→i32, `long`→i64, `float`→f32, `double`→f64, `boolean`→i1, `char`→i8
- [X] **String 设计（不可变）** - 结构体 `{ char* data; usize len; }`，支持 SSO（短字符串优化，16/23 字节内栈存储）
- [X] **StringBuilder** - 基于 Arena 或显式容量预分配的可变字符串
  - 实现文件: `caylibs/StringBuilder.cay`
  - 支持 `append(String/int/long/boolean/char/char[])`, `insert()`, `delete()`, `reverse()`, `substring()`, `replace()`, `indexOf()`
- [X] **Optional `<T>`** - 取代 null，显式空值处理 `Option<String>`，编译期非空检查基础
- [X] **FFI 基础类型包** - 标准库新增 `std.ffi` 模块
  - `CInt`, `CLong`, `SizeT` 等跨平台固定宽度别名
  - 固定宽度整数: `Int8T`, `Int16T`, `Int32T`, `Int64T`, `UInt8T`~`UInt64T`
  - 指针整数: `IntPtrT`, `UIntPtrT`
  - `RawPtr<T>` 裸指针类型（不参与 GC，用于接 C 指针）
  - 实现文件: `caylibs/std/ffi.cay`, `caylibs/std/ffia.cay`

<details>
<summary>Optional<T> 设计草案（依赖 0.5.2.x 泛型）</summary>

```java
// 底层实现：tagged union，零开销（无堆分配）
public class Optional<T> {
    private bool hasValue;
    private T value;          // 未初始化时占位，编译器保证不访问

    // 构造
    public static Optional<T> of(T value);
    public static Optional<T> empty();

    // 检查
    public bool isPresent();
    public bool isEmpty();

    // 取值（不安全 - 需手动检查）
    public T get();                        // throw if empty

    // 安全取值
    public T orElse(T defaultValue);
    public T orElseGet(fn() -> T supplier);
    public Optional<U> map(fn(T) -> U mapper);
    public Optional<U> flatMap(fn(T) -> Optional<U> mapper);

    // 条件操作
    public void ifPresent(fn(T) -> void consumer);
    public Optional<T> filter(fn(T) -> bool predicate);
}
```

</details>

#### 5.2.x 泛型集合（单态化实现）

- [ ] **泛型语法基础** - `class Box<T>` 语法解析、AST 节点、类型参数绑定

  - 单态化（monomorphization）：每个具体类型参数组合生成独立代码
  - 示例：`ArrayList<int>` → 生成 `ArrayList_i32` 特化版本
  - 类型擦除仅在 IR 层，前端保留完整类型信息
- [ ] **泛型类型检查** - 类型参数边界验证、泛型方法调用点类型推导

  - 协变/逆变暂不支持（保持与 Java 数组的协变不同，更接近 C++ 模板）
- [ ] **显式分配器参数** - 所有集合必须携带分配器：`ArrayList<int> list = new ArrayList<>(arena);`

  - 分配器作为泛型参数: `class ArrayList<T, A: Allocator = GlobalAlloc>`
  - 默认使用 GlobalAlloc，可通过参数指定 Arena 等
- [ ] **核心集合**：

  <details>
  <summary>ArrayList<T> API 设计</summary>

  ```java
  public class ArrayList<T, A: Allocator = GlobalAlloc> {
      // 构造
      public ArrayList();                           // 默认 GlobalAlloc
      public ArrayList(A allocator);                // 指定分配器
      public ArrayList(int initialCapacity);
      public ArrayList(int initialCapacity, A allocator);

      // 容量管理
      public void reserve(int capacity);            // 预分配
      public void shrinkToFit();                    // 释放多余内存
      public int capacity();
      public int size();
      public bool isEmpty();

      // 元素访问
      public T get(int index);                      // 含边界检查
      public void set(int index, T element);
      public T first();
      public T last();

      // 修改
      public void add(T element);                   // 尾部追加, O(1) amortized
      public void add(int index, T element);        // 指定位置插入, O(n)
      public T remove(int index);                   // O(n)
      public bool removeElement(T element);         // 移除第一个匹配
      public void clear();
      public void addAll(ArrayList<T> other);       // 批量追加

      // 查找
      public int indexOf(T element);
      public int lastIndexOf(T element);
      public bool contains(T element);

      // 迭代
      public Iterator<T> iterator();
      public void forEach(fn(T) -> void action);

      // 转换
      public T[] toArray();
  }
  ```

  </details>

  <details>
  <summary>HashMap<K,V> API 设计</summary>

  ```java
  // 开放寻址法 (Robin Hood hashing)，无二次指针间接
  // 默认负载因子 0.7
  public class HashMap<K, V, A: Allocator = GlobalAlloc> {
      public HashMap();
      public HashMap(int initialCapacity);
      public HashMap(A allocator);

      // 操作
      public void put(K key, V value);          // 插入或更新
      public V get(K key);                      // 返回 null 表示不存在
      public Optional<V> getOptional(K key);    // 安全获取
      public V remove(K key);
      public bool containsKey(K key);
      public int size();
      public bool isEmpty();
      public void clear();

      // 批量
      public void putAll(HashMap<K,V> other);

      // 视图
      public Set<K> keySet();
      public ArrayList<V> values();
      public Set<Entry<K,V>> entrySet();

      // 默认方法
      public V getOrDefault(K key, V defaultValue);
      public V putIfAbsent(K key, V value);
  }
  ```

  </details>

  <details>
  <summary>HashSet<T> API 设计</summary>

  ```java
  // 基于 HashMap<T, bool> 的特化实现
  public class HashSet<T, A: Allocator = GlobalAlloc> {
      public HashSet();
      public HashSet(int initialCapacity);

      public bool add(T element);
      public bool remove(T element);
      public bool contains(T element);
      public int size();
      public bool isEmpty();
      public void clear();

      public Iterator<T> iterator();
      public ArrayList<T> toList();
  }
  ```

  </details>
- [ ] **迭代器协议** - 基础迭代器接口，支持范围 for 循环

  ```java
  public interface Iterator<T> {
      bool hasNext();
      T next();
  }

  public interface Iterable<T> {
      Iterator<T> iterator();
  }

  // 编译器支持: for (auto item : collection) → 展开为 while (iter.hasNext()) { item = iter.next(); }
  ```

<details>
<summary>泛型实现关键技术要点</summary>

1. **单态化流程**：解析泛型定义 → 收集所有实例化点 → 为每个 (T=具体类型) 组合复制 AST → 类型替换 → 生成特化 IR
2. **符号命名**：`ArrayList<int>` → 内部符号 `__ArrayList_i32` 避免链接冲突
3. **类型边界**：初期仅支持无边界泛型 `class Box<T>`，后续支持 `class Comparable<T: Comparable<T>>`
4. **与 FFI 的交互**：单态化后的代码可与 C ABI 兼容（特化版本有确定的大小和布局）
5. **编译性能**：单态化增加编译时间和二进制大小。考虑实现共享单态化（shared monomorphization）优化相同布局的类型共享一份代码
6. **错误信息质量**：泛型实例化错误需追溯到模板定义位置，参考 Rust 的 error chain 机制

</details>

#### 5.3.x 智能指针与资源管理

- [ ] **UniquePtr `<T>`** - 独占所有权，可移动（move），不可复制，自动调用析构

  ```java
  public class UniquePtr<T> {
      // 构造（接管裸指针所有权）
      public static UniquePtr<T> fromRaw(T* ptr);

      // 禁止复制
      // UniquePtr(UniquePtr& other) = delete;

      // 移动语义（转移所有权，原指针置空）
      public static UniquePtr<T> move(UniquePtr<T> other);

      // 访问
      public T* get();               // 获取裸指针
      public T& operator*();         // 解引用
      public T* operator->();        // 成员访问

      // 释放
      public T* release();           // 放弃所有权，返回裸指针
      public void reset(T* newPtr);  // 替换管理的指针

      // 析构时自动 delete 管理的对象
  }
  ```
- [ ] **ScopedPtr `<T>`** - 栈作用域指针，禁止堆分配

  ```java
  // 编译器保证 ScopedPtr 仅在栈上创建
  // 离开作用域自动析构
  public class ScopedPtr<T> {
      @stack_only  // 编译器属性：禁止堆分配
      public ScopedPtr(T* ptr);
      public T* get();
      public T& operator*();
  }
  ```
- [ ] **Rc `<T>`（引用计数）**- 循环依赖检测（debug 模式），为 G2 的借用检查做过渡

  ```java
  public class Rc<T> {
      public static Rc<T> fromRaw(T* ptr);   // 引用计数 = 1
      public Rc<T> clone();                  // 引用计数 +1
      public T* get();
      public int refCount();
      // 析构时 refCount--, 为 0 时 delete 对象

      // Debug 模式：循环引用检测
      // 启用 --detect-cycles 编译选项时插入运行时检测代码
  }
  ```
- [ ] **弱引用基础** - `WeakPtr<T>`，解决循环引用（此时需手动打破循环）

  ```java
  public class WeakPtr<T> {
      public static WeakPtr<T> fromRc(Rc<T> rc);  // 不增加引用计数
      public Optional<Rc<T>> upgrade();            // 尝试提升为 Rc
      public bool isExpired();                     // 原始对象是否已释放
  }
  ```

<details>
<summary>智能指针与0.6.1.x的错误处理集成</summary>

```java
// 智能指针与 Result<T,E> 的组合使用模式
public static Result<UniquePtr<File>, IOError> openFile(String path) {
    UniquePtr<File> file = UniquePtr<File>.fromRaw(fopen_impl(path));
    if (file.get() == null) {
        return Result.err(new IOError("File not found"));
    }
    return Result.ok(file);
}
// 使用: 自动析构，无内存泄漏
```

</details>

#### 5.4.x 系统级 I/O

- [X] **File 与 Path** - 封装系统调用（Windows: HANDLE, Linux: fd），支持 RAII 关闭

  - 实现文件: `caylibs/File.cay`
  - `File` 类: open/close/readChar/writeChar/readLine/writeString/readAllText/writeAllText
  - `FileReader` / `FileWriter`: 简化的流式读写封装
  - `FileUtils`: getFileName/getExtension/getDirectoryName/combine/changeExtension
  - `FileMode`: read/write/append/readWrite 等打开模式
  - `SeekOrigin`: begin/current/end
  - `FileInfo`: stat-based 文件信息（exists, size, path）
  - `LineIterator`: 流式逐行读取迭代器
  - `File.copy()`: 缓冲区复制, `File.move()`: rename 封装
  - `File.exists()`: 使用 access() 系统调用，避免修改 atime
- [X] **缓冲区 I/O** - `FileReader/Writer`，显式缓冲区大小参数（默认 8KB）

  - `readAllLines()`: 流式两遍读取，避免双倍内存峰值
  - `writeInterpolated()`: 使用 StringBuilder 优化格式化写入，O(n) 复杂度
- [ ] **内存映射文件** - `Mmap` 类型，支持大文件零拷贝处理

  ```java
  public class Mmap {
      // 只读映射
      public static Result<Mmap, IOError> mapReadOnly(String path);
      // 读写映射
      public static Result<Mmap, IOError> mapReadWrite(String path, long size);

      public long data();                // 映射区域起始地址
      public long size();                // 映射区域大小
      public void sync();                // 刷回磁盘 (msync/FlushViewOfFile)
      public void unmap();               // 解除映射 (析构时自动调用)

      // 切片视图（零拷贝）
      public MmapSlice slice(long offset, long length);
  }

  public class MmapSlice {
      public byte get(long offset);
      public void set(long offset, byte value);
      public long size();
  }
  ```
- [X] **错误处理基础** - `FileResult`/`FileError` 已有基础实现

  - 当前为非泛型版本，使用 `Object` 作为值容器
  - 待 0.5.2.x 泛型完成后升级为 `Result<T, FileError>`

**阶段交付物**：可编写无内存泄漏的文件复制工具、HTTP 服务器基础框架，性能与 C++ 同级。

---

### 阶段四：错误处理与并发 (0.6.x.x)

**目标**：建立系统级的错误传播机制和零成本并发抽象。

#### 6.1.x 错误处理机制（非异常体系）

- [ ] **Result<T, E> 泛型** - 显式错误传播 `Result<File, IOError>`

  - 底层实现：tagged union `{ tag: u8, value: union { T ok; E err; } }`
  - 零开销：无堆分配，无 RTTI，无栈回退

  ```java
  public class Result<T, E> {
      private bool isOk;
      private T value;
      private E error;

      // 构造
      public static Result<T,E> ok(T value);
      public static Result<T,E> err(E error);

      // 检查
      public bool isOk();
      public bool isErr();

      // 取值
      public T unwrap();                        // panic if err
      public T unwrapOr(T defaultValue);
      public T unwrapOrElse(fn(E) -> T handler);
      public T expect(String msg);              // panic with message if err
      public E unwrapErr();                     // panic if ok

      // 转换
      public <U> Result<U,E> map(fn(T) -> U mapper);
      public <F> Result<T,F> mapErr(fn(E) -> F mapper);
      public <U> Result<U,E> andThen(fn(T) -> Result<U,E> handler);
      public <U> Result<U,E> flatMap(fn(T) -> Result<U,E> mapper);

      // 副作用
      public Result<T,E> inspect(fn(T) -> void action);
      public Result<T,E> inspectErr(fn(E) -> void action);
  }
  ```
- [ ] **问号运算符** - `file.read()?` 自动展开错误传播（类似 Rust 的 `?` 或 Zig 的 `try`）

  ```java
  // 编译器展开规则：
  // expr?  →  match expr {
  //              Result::ok(v) => v,
  //              Result::err(e) => return Result::err(e.into())
  //           }
  //
  // 使用示例：
  public static Result<String, IOError> readConfig() {
      File file = File.open("config.txt", FileMode.read())?;   // 自动传播 IOError
      String content = file.readAllText()?;
      return Result.ok(content);
  }
  ```
- [ ] **错误类型层级** - `interface Error { string message(); }`，支持错误链（error chaining）

  ```java
  public interface Error {
      String message();
      Optional<Error> cause();              // 错误链
  }

  // 内置错误类型
  public class IOError implements Error {
      public enum Kind { NotFound, PermissionDenied, UnexpectedEof, /* ... */ }
      public Kind kind();
      public int rawOsError();              // 原始 errno / GetLastError
  }

  public class ParseError implements Error {
      public int line();
      public int column();
      public String sourceSnippet();
  }
  ```
- [ ] **panic/abort** - 不可恢复错误，调用栈回退或立即终止（可选 unwind 实现）

  - `panic(String message)`：打印消息和调用栈，调用 `abort()`
  - Debug 模式展开栈帧以收集 backtrace；Release 模式直接 abort
  - 编译选项 `--no-panic` 将所有 panic 转为编译错误（适用于嵌入式环境）

*设计决策*：取消 Java 式异常，采用类似 Rust/Zig 的错误码机制，确保无运行时异常处理开销。

#### 6.2.x 轻量级并发（1:1 线程模型）

- [ ] **OS 线程封装** - `Thread` 类，直接映射 pthread/Windows Thread

  ```java
  public class Thread {
      // 创建并启动线程
      public static Result<Thread, ThreadError> spawn(fn() -> void entry);

      // 等待线程结束
      public void join();

      // 分离线程（不再 joinable）
      public void detach();

      // 线程标识
      public static long currentId();          // 当前线程 ID
      public long id();

      // 线程休眠
      public static void sleep(long millis);

      // 主动让出 CPU
      public static void yield();

      // 线程栈大小设置
      public static ThreadBuilder builder();
  }

  public class ThreadBuilder {
      public ThreadBuilder name(String name);
      public ThreadBuilder stackSize(long bytes);
      public Result<Thread, ThreadError> spawn(fn() -> void entry);
  }
  ```
- [ ] **线程参数传递** - 必须显式指定数据所有权转移（为 G2 所有权系统做铺垫）

  - 线程入口函数的捕获变量需显式 `move` 标记
  - 共享数据使用 `Arc<T>`（见 0.5.3.x）或 `Mutex<T>`
- [ ] **原子操作** - `AtomicI32`, `AtomicI64`, `AtomicPtr<T>`，封装 C++11 风格内存序

  ```java
  public enum MemoryOrder { Relaxed, Acquire, Release, AcqRel, SeqCst }

  public class AtomicI32 {
      public AtomicI32(int value);
      public int load(MemoryOrder order);
      public void store(int value, MemoryOrder order);
      public int fetchAdd(int delta, MemoryOrder order);
      public int fetchSub(int delta, MemoryOrder order);
      public bool compareExchange(int expected, int desired, MemoryOrder success, MemoryOrder failure);
      public int swap(int value, MemoryOrder order);
  }

  // 同样提供 AtomicI64, AtomicPtr<T>, AtomicBool
  ```
- [ ] **互斥锁** - `Mutex<T>`，封装 OS 层 mutex（futex 或 CriticalSection），非语言级 synchronized

  ```java
  public class Mutex<T> {
      public static Mutex<T> create(T value);
      public MutexGuard<T> lock();             // 阻塞直到获取锁
      public Optional<MutexGuard<T>> tryLock();// 非阻塞
  }

  public class MutexGuard<T> {
      public T& operator*();                    // 解引用获取被保护的值
      public T* operator->();
      // 析构时自动释放锁 (RAII)
  }

  public class RwLock<T> {
      public static RwLock<T> create(T value);
      public RwLockReadGuard<T> read();
      public RwLockWriteGuard<T> write();
  }
  ```

#### 6.3.x 异步 I/O 基础（非协程，基于 epoll/io_uring）

- [ ] **Reactor 模式** - 单线程事件循环，支持 Linux epoll/Windows IOCP

  ```java
  public class EventLoop {
      public static EventLoop create();
      public void register(int fd, EventMask mask, fn(int fd, EventMask) -> void callback);
      public void unregister(int fd);
      public void run();                         // 阻塞运行事件循环
      public void stop();
  }

  public enum EventMask {
      Readable = 1, Writable = 2, Error = 4, HangUp = 8
  }
  ```
- [ ] **异步文件 I/O** - 基于 io_uring（Linux）或 Overlapped I/O（Windows）

  ```java
  public class AsyncFile {
      public static AsyncFile open(String path, FileMode mode);
      public void read(long offset, long length, fn(Result<Buffer, IOError>) -> void callback);
      public void write(long offset, Buffer data, fn(Result<int, IOError>) -> void callback);
      public void close();
  }
  ```
- [ ] **Future/Promise 基础** - 回调式异步，显式状态机转换

  ```java
  public class Promise<T, E> {
      public void resolve(T value);              // 成功完成
      public void reject(E error);               // 失败
      public Future<T, E> getFuture();
  }

  public class Future<T, E> {
      public void onComplete(fn(Result<T,E>) -> void callback);
      public Future<U, E> map(fn(T) -> U mapper);
      public Future<U, E> andThen(fn(T) -> Future<U,E> handler);
  }
  ```

**阶段交付物**：可编写高性能反向代理、键值存储服务，具备系统级错误处理和并发能力。

---

### 阶段五：模块系统与工具链 (0.7.x.x)

**目标**：建立生产级工程能力，支持中大型项目开发。

#### 7.1.x 包管理器（cavly）

- [X] **包声明** - `package com.ethernos.std;` — 基础模块声明已支持
- [X] **模块清单** - `cavly.toml`（类似 Cargo），声明依赖、版本、编译选项 — 基础框架已存在
- [ ] **语义化版本** - 严格遵循 SemVer，支持 lock 文件确保可复现构建
  - `cavly.lock`: 记录依赖树的精确版本和哈希值
  - `cavly update`: 更新依赖到兼容的最新版本
  - `cavly outdated`: 检查过时依赖
- [ ] **本地/远程仓库** - 支持 Git 依赖和中央仓库（registry）
  - Git 依赖：`cavly add --git https://github.com/user/repo.git --tag v1.2.3`
  - 路径依赖：`cavly add --path ../local-lib`
  - Registry：预留 `registry.cavvy-lang.org` 域名

<details>
<summary>cavly.toml 完整格式规范</summary>

```toml
[package]
name = "my-project"
version = "0.1.0"
edition = "0.7"              # 编译器版本要求
authors = ["Name <email>"]
license = "MIT"

[dependencies]
stdlib = "0.5"               # 标准库版本
network = { version = "0.5", features = ["tcp", "http"] }
my-lib = { git = "https://github.com/user/repo.git", tag = "v1.0.0" }
local-dep = { path = "../local-lib" }

[dev-dependencies]
test-framework = "0.1"

[build]
target = "x86_64-pc-windows-msvc"
opt-level = 3                 # 0-3, s(ize), z(aggressive size)
lto = true
codegen-units = 1
debug = false

[features]
default = ["stdio"]
stdio = []
network = ["std/Network"]
http = ["network", "std/EasyHTTP"]

[profile.release]
opt-level = 3
lto = true
panic = "abort"

[profile.dev]
opt-level = 0
debug = true
```

</details>

#### 7.2.x 编译单元与链接

- [ ] **模块化编译** - 增量编译，接口文件（.cai）生成，类似 C++ 模块或 Swift 模块
  - `.cai` (Cavvy Interface): 编译后的接口文件，包含导出符号的类型签名和 vtable 布局
  - 增量编译：仅重编译变更文件及其依赖传递闭包
  - 预编译头等价物：`cavly build --precompile std/*` 生成预编译模块缓存
- [ ] **静态/动态链接** - 生成 .a/.so/.lib/.dll，支持 C ABI 导出
  - `cavly build --lib`：生成静态库
  - `cavly build --dylib`：生成动态库
  - `#[export]` 属性标记 C ABI 导出函数
- [ ] **LTO（链接时优化）** - 跨模块内联，基于 LLVM LTO
  - ThinLTO：平衡编译时间和优化效果
  - FullLTO：最大优化，仅用于 release

#### 7.3.x 开发工具

- [X] **LSP 服务器** - 基于编译器前端，支持跳转、补全、重构
  - 实现文件: `src/bin/cay-lsp.rs`
  - 待完善功能:
    - [ ] 语义高亮（semantic tokens）
    - [ ] 代码补全（含泛型和方法重载的上下文感知补全）
    - [ ] 悬停提示（类型信息、文档注释）
    - [ ] 重命名重构（`lsp_rename` 已支持基础版本）
    - [ ] 代码格式化集成
    - [ ] 诊断信息增强（快速修复建议）
- [ ] **调试信息** - DWARF/PDB 生成，支持 GDB/LLDB/VS Debugger
  - DWARF 5 (Linux/macOS)
  - PDB/CodeView (Windows, via LLVM CodeView support)
  - 源码级断点和变量观察
  - `cayc -g` 生成调试信息，`--debug-info=full|line-tables-only`
- [ ] **格式化工具** - `cayfmt`，确定官方代码风格（类似 gofmt）
  - 无配置选项（One True Style），消除代码风格争论
  - 基于解析器输出的 AST 再格式化
  - 集成到 LSP：保存时自动格式化
- [ ] **静态分析** - 基础 lint 规则
  - 未使用变量/导入/参数检测
  - 潜在空指针解引用
  - 资源泄漏检测（未关闭的 File、未释放的分配器）
  - 不可达代码检测
  - 全部用 `#[allow(...)]` / `#[deny(...)]` 属性控制

**阶段交付物**：可用 Cavvy 编写 10 万行级项目（如编译器自身前端），具备完整工具链支持。

---

### 阶段六：底层控制与优化 (0.8.x.x)

**目标**：提供底层硬件控制能力和极致性能优化。

#### 8.1.x Unsafe 子集（为 G2 做准备）

- [ ] **unsafe 块** - `unsafe { ... }`，内部允许：原始指针解引用、union 访问、调用 C 函数

  - 在 `unsafe` 块内的操作编译器不进行安全检查
  - 嵌套的 `unsafe` 无效（内部已在 unsafe 上下文）
  - 为 G2 的安全检查提供明确的"信任边界"
- [ ] **原始指针** - `*T` 和 `*mut T`，支持指针运算

  - `*const T` / `*mut T` 类型（区分只读/可写指针）
  - `ptr + offset`, `ptr - offset`, `ptr1 - ptr2`（按元素大小计算）
- [ ] **类型转换** - `transmute<T, U>`（位重解释），`as` 关键字基础转换

  - `ptr as *mut u8` — 指针类型间转换
  - `value as u64` — 数值类型间转换
  - `transmute<f64, u64>(3.14)` — 位级重解释（仅 unsafe 块内可用）
- [X] **内联IR** - `__ir { ... }` 宏，支持内联 LLVM IR 代码

  - 实现文件: `src/ir/inline_ir.rs`
  - 当前用于 StringBuilder、File 等标准库的内部实现
  - 待完善：对外文档和使用指南、安全性审计
- [ ] **内联汇编** - `asm!()` 宏，支持 x86_64/ARM64 内联汇编

  ```java
  // Intel 语法示例
  public static long readTSC() {
      long tsc;
      asm!("rdtsc" : "=A"(tsc) :: "memory");
      return tsc;
  }

  // ARM 语法示例
  public static void dmb() {
      asm!("dmb sy" ::: "memory");
  }
  ```

#### 8.2.x 编译器优化与 SIMD

- [ ] **自动向量化** - LLVM auto-vectorization 调优，支持 AVX2/AVX-512/NEON

  - 编译器传递 `--target-features=+avx2,+fma` 启用特定指令集
  - `cavly build --march=native` 自动检测并启用当前 CPU 全部特性
  - 基础选项已在 `cayc` 编译选项中预留
- [ ] **显式 SIMD** - `std.simd.Vec4f` 等类型，封装 SIMD 指令

  ```java
  public class Vec4f {
      // 用 128-bit SSE/NEON 寄存器存储 4 个 f32
      public Vec4f(float x, float y, float z, float w);

      public static Vec4f splat(float value);            // 广播
      public static Vec4f fromArray(float[] arr, int offset);

      public Vec4f add(Vec4f other);                     // 逐元素加
      public Vec4f mul(Vec4f other);                     // 逐元素乘
      public Vec4f mulAdd(Vec4f a, Vec4f b);             // FMA: this + a * b
      public float dot(Vec4f other);                     // 点积
      public Vec4f cross3(Vec4f other);                  // 3D 叉积

      public float x(); public float y();
      public float z(); public float w();
  }
  ```
- [ ] **内存布局控制** - `#[repr(C)]`, `#[repr(packed)]`, `#[align(N)]` 属性

  - `#[repr(C)]`：C 兼容布局，用于 FFI
  - `#[repr(packed)]`：取消对齐填充，最小化内存占用
  - `#[align(16)]`：指定对齐字节数
  - `#[repr(transparent)]`：单字段结构体保证与字段相同布局（用于 newtype 模式）
- [ ] **零成本抽象验证** - 确保泛型、迭代器等抽象最终编译为与手写 C 等价的机器码

  - 建立性能基准测试套件（microbenchmarks）
  - CI 中对比泛型版本与手写版本的汇编输出

#### 8.3.x 嵌入式与裸机支持

- [ ] **no_std** - 支持无标准库环境，不链接 libc

  - `cavly build --no-std` 编译标志
  - `#![no_std]` crate 级属性
  - 提供 `core` 最小运行时（仅含基础类型、编译器内置函数）
- [ ] **启动代码** - 自定义 `_start`，支持裸机 ARM/RISC-V 编程

  - 可自定义链接脚本
  - `#[link_section = ".vector_table"]` 属性放置中断向量表
  - `#[no_mangle]` 属性保留符号名
- [ ] **内存映射 I/O** - `volatile` 读写语义，支持 MMIO 寄存器操作

  ```java
  public class Volatile<T> {
      public T read();                                  // 易失读
      public void write(T value);                       // 易失写
  }

  // 使用: MMIO 寄存器访问
  const UART_BASE: long = 0x4000_1000;
  Volatile<u32> uart_data = Volatile.fromPtr(UART_BASE);
  uart_data.write(0x41);                               // 发送字符 'A'
  ```

**阶段交付物**：可编写操作系统内核模块、嵌入式固件、高性能计算库（如矩阵运算），完全替代 C/C++ 在系统编程领域的地位。

---

## G1 代：自举与现代化（1.x.x.x）

**目标**：用 Cavvy 重写自身编译器，引入现代语言特性，提升表达力。

### 10.x 编译器自举（里程碑版本）

- [ ] **前端迁移** - 词法分析器、语法分析器、AST 生成全部用 Cavvy 编写
  - 评分标准：前端代码量缩减 <20%（利用模式匹配、ADT 等 G1 特性）
  - 错误信息质量不低于 G0 编译器
- [ ] **LLVM IR 生成** - 继续使用 LLVM 后端，但驱动代码为 Cavvy
  - IR 生成器用 Cavvy 编写，通过 FFI 调用 LLVM C API
- [ ] **引导编译** - 使用 G0 编译器（0.8.x）编译 G1 编译器，再用 G1 编译器自举验证
  - Stage 0: G0 编译器 → 编译 G1 编译器
  - Stage 1: Stage 0 产物 → 编译 G1 编译器（自举）
  - Stage 2: Stage 1 产物 → 编译 G1 编译器（验证自举一致性）
  - 要求：Stage 1 和 Stage 2 输出的二进制完全一致（bitwise identical）
- [ ] **性能基准** - 自举编译速度不低于 G0 版本的 90%
  - 编译 Cavvy 自身（~5 万行）时间 <30 秒 (release build)

### 11.x 语法糖与提升开发体验

- [ ] **类型推断增强** - `var` 关键字局部变量推断，`auto` 返回值推断（限于单 return）
- [ ] **解构赋值** - `var (x, y) = point;`，支持元组和结构体
  - `var (name, age, _) = getPerson();` — 使用 `_` 忽略字段
  - 支持嵌套解构：`var (x, (y, z)) = nested;`
- [ ] **范围与迭代** - `for i in 0..100 { ... }`（半开区间），支持自定义迭代器
  - `0..100` → Range 类型
  - `0..=100` → RangeInclusive 类型
  - `for i in 0..100 step 2` → 步进范围
- [ ] **字符串模板** - `"Hello, \(name)"` 或 `"Hello, ${name}"`，编译期解析
  - 编译期展开为 StringBuilder 调用链，零运行时解析开销

### 12.x 函数式编程支持

- [ ] **Lambda 表达式** - `(x: int) => x * 2`，支持闭包（捕获环境）
  - 无捕获 lambda → 函数指针（零开销）
  - 有捕获 lambda → 匿名结构体 + 虚函数（开销可控）
- [ ] **高阶函数** - 函数作为一等公民，支持函数类型 `fn(int) -> int`
  - 函数引用：`let f: fn(int) -> int = someFunction;`
- [ ] **不可变集合** - `ImmutableList<T>`，基于持久化数据结构（HAMT 等）
  - 结构共享，修改返回新版本而不复制全部
  - `list.prepend(x)`, `list.append(x)`, `list.removeAt(i)` 均为 O(log n) 或 O(1)
- [ ] **管道操作符** - `value |> transform |> filter`，左结合
  - `data |> parse |> validate |> process |> output`
  - 编译期展开为嵌套函数调用

### 13.x 高级类型系统

- [ ] **代数数据类型（ADT）** - `enum Option<T> { Some(T), None }`，支持模式匹配

  ```java
  public enum Option<T> {
      Some(T value),
      None
  }

  public enum Result<T, E> {
      Ok(T value),
      Err(E error)
  }

  // 使用模式匹配
  match result {
      Ok(value) => process(value),
      Err(e) => log("Error: \(e.message())"),
  }
  ```
- [ ] **模式匹配基础** - `match` 表达式，支持常量、范围、元组匹配

  - 必须穷举（exhaustiveness check）
  - 支持守卫条件：`case Point(x, y) if x > 0 => ...`
- [ ] **泛型约束** - `where T: Comparable`，泛型边界细化

  - `fn max<T>(a: T, b: T) -> T where T: Comparable`
  - 支持多重约束：`where T: Copy + Comparable + Hash`
- [ ] **关联类型** - `interface Container { type Item; fn get(self) -> Item; }`

### 14.x 异步与并发语法糖（基于 G0 的 I/O 基础）

- [ ] **async/await** - 基于 G0 阶段的手动 Future，编译器生成状态机
  - `async fn` 标记异步函数
  - `await` 表达式挂起当前任务
  - 生成的代码为无堆分配的栈上状态机（确定性大小）
- [ ] **协程（绿色线程）** - `async fn` 支持，M:N 线程模型可选
  - 默认 1:1（G0），可选 M:N（G1 runtime）
  - async 运行时：基于 mio/iocp 的事件循环
- [ ] **结构化并发** - `async { ... }` 块，自动取消传播
  - 父任务取消时自动取消子任务
  - scope 结束时保证所有子任务已完成

---

## G2 代：内存安全纪元（2.x.x.x）

**目标**：引入所有权与借用检查系统，实现编译期内存安全，消除 use-after-free 和数据竞争。

### 20.x 所有权系统核心（代际升级标记）

- [ ] **所有权语义** - 默认移动语义，复制需显式实现 `Copy` trait
  - `let x = y;` → y 的所有权移动到 x，y 不可再使用
  - `let x = y.clone();` → 显式深拷贝
  - 基本类型（int, float 等）自动实现 Copy
- [ ] **借用检查器（Borrow Checker）** - 编译期跟踪引用生命周期
  - NLL (Non-Lexical Lifetimes) 作为基础
  - 结合 Cavvy 已有的 vtable 信息，处理虚函数调用中的借用
- [ ] **引用类型** - `&T`（不可变借用），`&mut T`（可变借用），严格 XOR 规则
  - 同一时刻：多个 `&T` 或 一个 `&mut T`，不能同时
  - 引用的生命周期不长于被引用对象
- [ ] **生命周期标注** - 显式生命周期 `'a`，函数签名如 `fn max<'a>(x: &'a T, y: &'a T) -> &'a T`
  - 常见模式自动推导（lifetime elision）
  - 复杂场景需手动标注
- [ ] **RAII 强化** - Drop trait 自动调用，与所有权转移结合
  - 值离开作用域自动调用 `drop()`
  - 所有权转移不触发多次 drop

### 21.x 高级内存安全

- [ ] **内部可变性** - `Cell<T>`, `RefCell<T>`（单线程），`Mutex<T>`, `RwLock<T>`（多线程）的 unsafe 内部实现
- [ ] **智能指针集成** - `Box<T>`（堆唯一所有权），`Arc<T>`（原子引用计数，线程安全共享）
  - 此为 0.5.3.x 智能指针在 G2 的安全版本（所有权感知）
- [ ] **弱引用与循环检测** - `Weak<T>`，编译期警告潜在循环引用（辅助 lint）

### 22.x 并发安全

- [ ] **Send/Sync trait** - 标记类型是否可跨线程发送/共享，编译期数据竞争检测
  - `Send`: 类型的所有权可安全转移到另一线程
  - `Sync`: 类型的引用可安全在多个线程间共享
  - 由编译器自动推导（auto trait），错误时给出明确信息
- [ ] **通道（Channels）** - `Sender<T>/Receiver<T>`，所有权转移实现无锁消息传递
  - MPSC (多生产者单消费者) 为默认
  - SPSC 特化用于高性能场景
- [ ] **无锁数据结构** - `AtomicQueue<T>`, `AtomicStack<T>`，基于 CAS 操作

### 23.x 编译期计算与元编程

- [ ] **常量泛型** - `Array<T, N>` 其中 N 为编译期常量
- [ ] **编译期函数执行** - `const fn`，可在编译期计算复杂逻辑
- [ ] **宏系统** - 卫生宏（hygienic macros），`macro!()` 与 `macro_rules!`
  - 声明宏（declarative macros）：模式匹配 → 代码生成
  - 过程宏（procedural macros）：Cavvy 代码操作 AST
- [ ] **反射（编译期）** - `typeof`, `offsetof`, 生成序列化代码（零成本反射）

### 24.x 与 G1/G0 的互操作

- [ ] **unsafe 桥接** - 在 G2 代码中调用 G1/G0 的不安全代码，需显式 `unsafe` 块
- [ ] **迁移路径** - 允许 G1 代码逐步添加所有权标注升级为 G2，提供 `#[legacy]` 属性允许无所有权代码存在
  - `#edition G1` 模块：完整 G1 兼容模式
  - `#edition G2` 模块：强制所有权检查
  - 跨 edition 调用需 unsafe 桥接
- [ ] **FFI 安全封装** - 自动生成 C 头文件的安全包装层

---

## 演进时间线参考

| 代际         | 预计周期 | 关键里程碑                                                   |
| ------------ | -------- | ------------------------------------------------------------ |
| **G0** | 2-3 年   | 生产可用（0.8.0），可替代 C++ 编写高性能服务                 |
| **G1** | 1.5-2 年 | 自举完成（1.0.0），语言稳定，生态建设                        |
| **G2** | 2-3 年   | 内存安全（2.0.0），进入 Linux 内核、嵌入式等最高安全要求领域 |

**当前进度 (截至 0.5.x)**：

```
已完成: ████████████░░░░░░░░░░░░ ~40%
  ├── 0.1.x 原型          ██████████ 100%
  ├── 0.2.x 当前           ██████████ 100%
  ├── 0.3.x 控制流         ██████████ 100%
  ├── 0.4.x 面向对象       ██████████ 100%
  ├── 0.5.x 标准库         ██████░░░░  60%
  ├── 0.6.x 错误/并发      ░░░░░░░░░░   0%
  ├── 0.7.x 工具链         ████░░░░░░  35%
  └── 0.8.x 底层控制       ██░░░░░░░░  15%
```

**总计**：5-8 年达到完全体，符合系统编程语言的成熟周期（参考 Rust 1.0 到广泛采用约 5 年，C++ 标准化周期）。

---

## 关键设计决策备忘

1. **G0 与 G1 的边界**：G0 证明 Cavvy 可以系统编程，G1 证明 Cavvy 可以大规模工程开发。G0 保留手动内存管理（类似 C++），G1 不引入所有权，但完善类型系统和语法糖。
2. **G1 与 G2 的边界**：G2 是可选的严格模式。G1 代码可在 G2 编译器中通过 `#edition G1` 继续运行，确保向后兼容。G2 的所有权系统是**渐进式**的，而非强制立即迁移。
3. **异常 vs 错误码**：G0 和 G1 采用 `Result<T,E>` 为主，G2 可能引入 `?` 传播和更复杂的错误处理，但始终保持零开销（no unwinding cost）。
4. **GC 永不引入**：所有代际均不提供垃圾回收，确保与 C/C++/Rust 同级的内存可控性。
5. **单态化泛型**：采用 C++/Rust 风格的单态化泛型而非 Java 的类型擦除。确保泛型代码与手写特化代码性能相同，且与 C ABI 兼容。
6. **默认虚函数**：Cavvy 的方法默认是虚函数（与 Java 一致），final 方法可去虚拟化。不同于 C++ 的默认非虚，给予面向对象设计更大的灵活性。
7. **IR 层兼容性**：`.ll` 文件和 `.caybc` 字节码是平台无关的中间表示。一次编译、到处运行的实现基础。

| 依赖关系图                                                                                                                                              |
| ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 0.5.0 (Allocator) ──────┬──→ 0.5.2 (Generics) ──┬──→ 0.5.3 (SmartPtrs) ──→ 0.6.1 (Result) ──→ 0.6.3 (Async)                       |
| │                       │                                                                                                                             |
| 0.5.1 (Types/String) ───┘                       └──→ 0.5.4 (I/O) ───────────→ 0.7.1 (cavly) ──→ 0.7.2 (Modules)                  |
|                                                                                                                                                         |
| 0.6.2 (Threads) ────────────────────────────────────────────────→ 0.8.1 (Unsafe) ──→ G2 (Ownership) |
|                                                                                                                                                         |
| 0.7.3 (Tools) ──→ 0.8.2 (SIMD) ──→ 0.8.3 (Embedded) ──→ G1 (Self-hosting)                                                                      |

---

## 当前开发状态 (5.1.0-Beta)

### Beta 版本说明

5.1.0-Beta 是一个**质量审查与迁移准备**阶段，核心目标是：

- 修复影响编译正确性的 P0 Bug
- 消除生产代码中的 panic 风险
- 为核心功能建立回归测试基线
- 为 CodeGen → IR Builder 迁移做准备

### Beta 审查进度

| 阶段                         | 状态      | 说明                                              |
| ---------------------------- | --------- | ------------------------------------------------- |
| Phase 1: Critical Bug 修复   | ✅ 完成   | 接口方法调用、字节码生成器硬编码                  |
| Phase 2: 代码清理与健壮性    | ✅ 完成   | unwrap/panic 清理                                 |
| Phase 3: 测试补全            | 🔄 进行中 | 新增 30+ 测试（接口、Lambda、命名参数、错误诊断） |
| Phase 4: 文档与一致性        | 🔄 进行中 | 更新 ROADMAP、同步版本号                          |
| Phase 5: IR Builder 迁移准备 | ⏳ 待开始 | 实现类/方法生成                                   |

### Beta 关键指标

| 指标              | Beta.1 初始值 | Beta.2 当前值  | Beta 目标 |
| ----------------- | ------------- | -------------- | --------- |
| 集成测试数        | ~70           | **135+** | 150+      |
| TODO 数量         | 24            | **20**   | <15       |
| 生产代码 unwrap() | 60+           | **95**   | <30       |
| 生产代码 panic!   | 2             | **0**    | 0         |
| 已知 P0 Bug       | 2             | **0**    | 0         |

### 已知限制 (5.1.0-Beta.2)

1. **接口方法动态分发**：通过接口类型调用方法时，使用声明类型解析方法（第一个实现类），而非运行时类型。需要 vtable 支持才能正确实现动态分发。
2. **Lambda 闭包**：Lambda 语法已解析，但闭包捕获环境变量尚未完整实现。
3. **泛型单态化**：语法解析支持 `<T>`，但代码生成尚未实现单态化。
4. **private 访问控制**：编译器不强制执行 private 访问修饰符。
5. **数组初始化语法**：不支持 `new Type[] { 1, 2, 3 }` 语法，需要先声明大小再赋值。

### IR Builder 迁移状态

| 组件       | CodeGen | IR Builder | 迁移进度 |
| ---------- | ------- | ---------- | -------- |
| 基础表达式 | ✅ 完整 | ⚠️ 部分  | 30%      |
| 控制流     | ✅ 完整 | ⚠️ 部分  | 25%      |
| 类/方法    | ✅ 完整 | ❌ 极少    | 10%      |
| 字符串操作 | ✅ 完整 | ❌         | 0%       |
| 数组操作   | ✅ 完整 | ❌         | 0%       |
| FFI        | ✅ 完整 | ❌         | 0%       |
| Lambda     | ✅ 部分 | ❌         | 0%       |

**总进度**：约 15-20%（主要在 bridge.rs 和基础表达式）

---

**注意：** 本路线图会根据实际开发情况和社区反馈进行调整。
