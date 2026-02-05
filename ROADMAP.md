# EOL 语言开发路线图 (Roadmap)

## 项目概述
EOL (Ethernos Object Language) 是一个编译为 Windows 可执行文件的静态类型编程语言，语法设计目标与 Java 高度兼容。

---

## 阶段一：语言核心完善 (v0.2.x)

### 1.1 控制流增强
- [ ] **for 循环** - Java 风格 `for (int i = 0; i < n; i++)`
- [ ] **增强 for 循环** - `for (Type item : collection)` 遍历集合
- [ ] **do-while 循环** - `do { ... } while (condition);`
- [ ] **switch 语句** - Java 风格，支持 `case` 穿透和 `break`
- [ ] **break/continue 标签** - 嵌套循环控制 `outer: for (...) ... break outer;`

### 1.2 数据类型扩展
- [ ] **浮点类型** - `float`, `double` 支持
- [ ] **字符类型** - `char` 类型和字符字面量 `'A'`
- [ ] **布尔类型** - 原生 `boolean` 类型（true/false）
- [ ] **long 类型** - 64位有符号整数
- [ ] **类型转换** - 显式强制转换 `(int)value`

### 1.3 数组与集合
- [ ] **多维数组** - `int[][] matrix = new int[3][3];`
- [ ] **数组初始化** - `int[] arr = {1, 2, 3};`
- [ ] **数组长度** - `arr.length` 属性
- [ ] **数组边界检查** - 运行时安全检查
- [ ] **字符串增强** - `String` 类方法（substring, indexOf, replace等）

### 1.4 方法改进
- [ ] **方法重载** - 同名不同参数列表
- [ ] **可变参数** - `void method(String fmt, Object... args)`
- [ ] **方法引用** - 静态/实例方法引用 `ClassName::methodName`
- [ ] **Lambda 表达式** - `(params) -> { body }`

---

## 阶段二：面向对象特性 (v0.3.x)

### 2.1 类系统完善
- [ ] **继承** - `class Child extends Parent`
- [ ] **方法重写** - `@Override` 注解支持
- [ ] **多态** - 父类引用指向子类对象
- [ ] **抽象类** - `abstract class` 定义
- [ ] **接口** - `interface` 多实现 `implements`
- [ ] **访问修饰符** - `public/protected/private/default` 完整支持

### 2.2 构造与初始化
- [ ] **构造函数重载** - 多构造函数支持
- [ ] **构造函数链** - `this(...)` 和 `super(...)` 调用
- [ ] **初始化块** - 实例初始化块 `{ ... }`
- [ ] **静态初始化** - `static { ... }` 类级别初始化

### 2.3 核心类特性
- [ ] **final 类/方法** - 不可继承/重写
- [ ] **static 导入** - `import static ...`
- [ ] **内部类** - 成员内部类、静态内部类
- [ ] **匿名类** - `new Interface() { ... }`

### 2.4 泛型编程
- [ ] **泛型类** - `class Container<T>`
- [ ] **泛型方法** - `<T> T max(T a, T b)`
- [ ] **类型边界** - `<T extends Number>`
- [ ] **通配符** - `?`, `? extends T`, `? super T`
- [ ] **泛型擦除** - 编译时类型处理

---

## 阶段三：标准库建设 (v0.4.x)

### 3.1 核心库 (java.lang 等效)
- [ ] **System 类** - `System.out.println()`, `System.currentTimeMillis()`
- [ ] **Math 类** - `Math.sin()`, `Math.sqrt()`, `Math.pow()`
- [ ] **Object 类** - 所有类的根类，`toString()`, `equals()`, `hashCode()`
- [ ] **包装类** - `Integer`, `Double`, `Boolean` 等
- [ ] **String 类** - 不可变字符串，完整方法集
- [ ] **StringBuilder/StringBuffer** - 可变字符串

### 3.2 集合框架 (java.util 等效)
- [ ] **List 接口** - `ArrayList<T>`, `LinkedList<T>`
- [ ] **Set 接口** - `HashSet<T>`, `TreeSet<T>`
- [ ] **Map 接口** - `HashMap<K,V>`, `TreeMap<K,V>`
- [ ] **Queue/Deque** - `ArrayDeque<T>`, `PriorityQueue<T>`
- [ ] **Iterator** - `iterator()`, `hasNext()`, `next()`
- [ ] **Collections 工具** - `sort()`, `binarySearch()`, `shuffle()`

### 3.3 实用工具
- [ ] **Arrays 类** - `Arrays.sort()`, `Arrays.toString()`
- [ ] **Random 类** - 随机数生成
- [ ] **Date/Time API** - `LocalDate`, `LocalTime`, `LocalDateTime`
- [ ] **Formatter** - `String.format()`, `printf()`
- [ ] **Scanner** - 控制台输入解析
- [ ] **正则表达式** - `Pattern`, `Matcher`

### 3.4 IO 与 NIO
- [ ] **File 类** - 文件/目录操作
- [ ] **Stream** - `InputStream`, `OutputStream`, `Reader`, `Writer`
- [ ] **Buffered IO** - `BufferedReader`, `BufferedWriter`
- [ ] **File IO** - `FileInputStream`, `FileOutputStream`
- [ ] **NIO.2** - `Path`, `Files`, `Paths`

---

## 阶段四：高级特性 (v0.5.x)

### 4.1 异常处理
- [ ] **异常类层次** - `Throwable` > `Exception` > `RuntimeException`
- [ ] **try-catch-finally** - 完整异常处理
- [ ] **多重 catch** - `catch (A | B e)`
- [ ] **try-with-resources** - 自动资源管理
- [ ] **throw/throws** - 异常抛出声明
- [ ] **自定义异常** - 继承 `Exception` 或 `RuntimeException`

### 4.2 注解与反射
- [ ] **注解定义** - `@interface`
- [ ] **元注解** - `@Retention`, `@Target`
- [ ] **常用注解** - `@Override`, `@Deprecated`, `@SuppressWarnings`
- [ ] **反射 API** - `Class<?>`, `Method`, `Field`, `Constructor`

### 4.3 枚举与记录
- [ ] **枚举类型** - `enum Status { ACTIVE, INACTIVE }`
- [ ] **枚举方法** - 构造函数、字段、方法
- [ ] **记录类** - `record Point(int x, int y)`

### 4.4 并发编程 (java.util.concurrent 等效)
- [ ] **Thread 类** - 线程创建和启动
- [ ] **Runnable/Callable** - 任务接口
- [ ] **同步机制** - `synchronized`, `Lock`, `ReentrantLock`
- [ ] **线程池** - `ExecutorService`, `ThreadPoolExecutor`
- [ ] **并发集合** - `ConcurrentHashMap`, `CopyOnWriteArrayList`
- [ ] **原子类** - `AtomicInteger`, `AtomicBoolean`
- [ ] **CompletableFuture** - 异步编程

---

## 阶段五：模块系统与生态 (v0.6.x)

### 5.1 包管理
- [ ] **包声明** - `package com.example.project;`
- [ ] **导入语句** - `import`, `import static`
- [ ] **访问控制** - 包级私有 (default)
- [ ] **包管理器** - 类似 Maven/Gradle 的依赖管理

### 5.2 模块系统 (Java 9+ 风格)
- [ ] **module-info.java** - 模块声明
- [ ] **exports** - 导出包
- [ ] **requires** - 依赖声明
- [ ] **服务提供** - `provides ... with ...`

### 5.3 开发工具
- [ ] **LSP 支持** - 语言服务器协议
- [ ] **VSCode 插件** - 语法高亮、跳转、补全、调试
- [ ] **代码格式化** - 类似 Eclipse/IDEA 格式化规则
- [ ] **静态分析** - 代码检查工具
- [ ] **单元测试** - JUnit 风格测试框架

### 5.4 跨平台支持
- [ ] **Linux 后端** - ELF 可执行文件
- [ ] **macOS 支持** - Mach-O 格式
- [ ] **JVM 后端** - 可选编译为 JVM 字节码

---

## 阶段六：性能优化 (v0.7.x)

### 6.1 编译器优化
- [ ] **逃逸分析** - 栈上分配对象
- [ ] **内联优化** - 方法内联展开
- [ ] **常量折叠** - 编译期常量计算
- [ ] **死代码消除** - 移除未使用代码
- [ ] **SIMD 向量化** - 自动使用 SIMD 指令

### 6.2 运行时优化
- [ ] **JIT 编译** - 热点代码即时编译
- [ ] **GC 可选** - 垃圾回收器（可选启用）
- [ ] **AOT 编译** - 预编译为原生代码

---

## 当前版本 (v0.1.x)

### 已完成功能 ✓
- [x] 基础词法分析器和语法分析器
- [x] 语义分析（类型检查）
- [x] LLVM IR 代码生成
- [x] 编译器驱动 (eolc, eolll, ir2exe)
- [x] Java 风格基础语法（类、方法、字段）
- [x] 基础类型（int, String, void, boolean）
- [x] if/else 和 while 语句
- [x] 运算符（算术、比较、逻辑、位运算）
- [x] Windows EXE 输出
- [x] 编译优化选项（LTO, PGO, SIMD, IR优化）

---

## Java 语法兼容性目标

### 语法示例对比

```java
// EOL 目标语法（与 Java 兼容）
public class HelloWorld {
    public static void main(String[] args) {
        System.out.println("Hello, World!");
        
        // for 循环
        for (int i = 0; i < 10; i++) {
            System.out.println(i);
        }
        
        // 增强 for 循环
        int[] arr = {1, 2, 3};
        for (int x : arr) {
            System.out.println(x);
        }
        
        // 泛型集合
        List<String> list = new ArrayList<>();
        list.add("item");
    }
}

// Lambda 表达式
Runnable r = () -> System.out.println("Running");

// Stream API（远期）
List<Integer> result = list.stream()
    .filter(x -> x > 0)
    .map(x -> x * 2)
    .collect(Collectors.toList());
```

---

## 贡献指南

1. 优先实现阶段一的核心 Java 语法特性
2. 确保语法与 Java 高度兼容
3. 每个 PR 包含测试用例和文档更新
4. 保持向后兼容性
5. 性能回归测试通过后方可合并

## 时间线（预估）

| 阶段 | 版本 | 预计时间 |
|------|------|---------|
| 核心完善 | v0.2.x | 2026 Q1-Q2 |
| 面向对象 | v0.3.x | 2026 Q3-Q4 |
| 标准库 | v0.4.x | 2027 Q1-Q2 |
| 高级特性 | v0.5.x | 2027 Q3-Q4 |
| 模块系统 | v0.6.x | 2028 Q1-Q2 |
| 性能优化 | v0.7.x | 2028 Q3-Q4 |

---

**注意：** 本路线图会根据实际开发情况和社区反馈进行调整。
