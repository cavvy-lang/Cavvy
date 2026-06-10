# Cavvy 5.1.0 新特性详解

## 目录

- [语言特性](#语言特性)
- [编译器与工具链](#编译器与工具链)
- [标准库扩展](#标准库扩展)
- [基础设施](#基础设施)

---

## 语言特性

### 1. Lambda 表达式与函数式接口

Cavvy 5.1.0 正式支持 Lambda 表达式，允许以简洁语法创建匿名函数实例。

**语法示例**:

```cay ignore
// 无参数 Lambda
() -> { println("Hello"); }

// 单参数 Lambda（可省略括号）
x -> x * 2

// 多参数 Lambda
(int a, int b) -> a + b

// 函数式接口赋值
interface Comparator<T> {
    int compare(T a, T b);
}

Comparator<int> cmp = (a, b) -> a - b;
```

**实现要点**:
- Lambda 表达式可自动适配到单方法接口（函数式接口）
- 支持闭包捕获外部变量
- 修复了类型打印需手动调用 `String.valueOf` 的问题，统一自动类型转换

---

### 2. 指针类型系统完整支持

5.1.0 实现了 Cavvy 对指针的完整支持，包括声明、运算和作为参数/返回值。

**支持的语法**:

```cay ignore
// 指针类型声明
int* p;
int** pp;

// 取地址
int x = 10;
int* p = &x;

// 解引用
int y = *p;

// 通过指针赋值
*p = 20;

// 多级指针
int** pp = &p;
int z = **pp;

// 指针作为函数参数和返回值
int* allocInt() {
    // ...
}
```

**技术实现**:
- 修复语义分析器中 `AddressOf` 返回 `Type::Pointer` 而非 `Type::Int64`
- 修复语义分析器中 `Deref` 正确处理 `Type::Pointer` 类型
- 添加代码生成器对解引用赋值的支持 (`generate_deref_assignment`)
- 更新 EBNF 语法规范，添加 `pointer_type` 定义

**测试覆盖**:
- `test_pointer_basic.cay` - 基础指针操作
- `test_pointer_user_example.cay` - 用户示例代码
- `test_pointer_advanced.cay` - 高级用法（函数参数、多级指针）

---

### 3. 泛型类型系统

修复并完善了泛型类型系统的多项核心问题，现在泛型类可以稳定用于生产代码。

**修复的问题**:

1. **泛型类字段类型替换**: 在 `class_analysis.rs` 中添加字段类型的泛型参数替换，将 `T` 替换为 `GenericParam("T")`
2. **泛型方法参数/返回类型替换**: 在 `type_check.rs` 中对方法参数和返回类型进行泛型参数替换
3. **多类型参数解析**: 在 `expr_inference.rs` 中修复类型参数解析逻辑，支持 `Pair<K, V>` 等多参数泛型类
4. **泛型类方法查找**: 在 `types.rs` 的 `TypeRegistry::find_method` 中支持泛型类名解析为基础类名
5. **泛型类型匹配**: 在 `ClassInfo::types_match_exact` 中支持泛型模板与实例化类型的匹配

**语法示例**:

```cay ignore
class Pair<K, V> {
    K key;
    V value;

    Pair(K k, V v) {
        this.key = k;
        this.value = v;
    }
}

Pair<int, String> p = new Pair<int, String>(1, "hello");
```

**测试覆盖**:
- `test_generics_basic.cay` - 基础泛型字段测试
- `test_generics_method_param.cay` - 泛型方法参数测试

---

### 4. 接口方法运行时动态分发

重构 vtable 生成逻辑，支持全局分配接口方法槽位，实现接口类型调用的动态分派。

**特性说明**:
- 按运行时类型选择方法实现
- 支持多场景接口动态分发，覆盖参数、返回值场景
- 优化类型注册表，添加接口 vtable 槽位管理相关工具函数

---

### 5. 类型别名与函数指针

新增 `alias` 关键字支持类型别名定义，新增 `fn` 关键字用于声明函数指针类型。

**语法示例**:

```cay ignore
// 类型别名
alias IntVector = Vector<int>;
alias StringMap = Map<String, int>;

// 函数指针类型
alias CompareFn = fn(int, int) -> int;

// 函数指针使用
CompareFn cmp = (a, b) -> a - b;
```

---

### 6. 访问控制（public / protected / private / static）

新增完整的访问控制支持，覆盖类成员的可访问性规则。

**新增测试覆盖**:
- 11 个访问控制相关示例文件
- 覆盖 public / protected / private / static / 构造函数等场景
- 新增方法名拼写建议功能，优化未找到方法的错误提示
- 重构静态方法调用解析逻辑，优先匹配当前类静态方法

---

### 7. 内联 IR（Inline IR）

支持在 Cavvy 代码中直接嵌入 LLVM IR，实现与底层的高效交互。

**语法示例**:

```cay ignore
__ir {
    %result = add i32 %0, %1
    ret i32 %result
}
```

**技术实现**:
- 新增 `InlineIrBridge` 模块，实现 CodeGen 与 IR Builder 之间的安全协作
- 支持变量映射系统，支持参数索引（`%0`, `%1`）和变量名引用
- 参数使用原始 LLVM 名（`class_name.param_name`）而非 alloca 变量名

**测试覆盖**: 8 个完整测试用例，覆盖基础算术、浮点数、位运算、比较运算、数学函数、内存操作、类型转换、复杂表达式。

---

### 8. 复合赋值操作符

新增 `+=`, `-=`, `*=`, `/=`, `%=` 复合赋值操作的 IR 生成支持。

---

## 编译器与工具链

### 1. Cavly 包管理器

新增完整的 Cavly 包管理器模块，提供类似 Cargo 的项目管理体验。

**支持的命令**:
- `cavly init` - 初始化新项目
- `cavly build` - 构建项目
- `cavly run` - 运行项目
- `cavly clean` - 清理构建产物

**特性**:
- 配置解析（TOML 格式）
- 项目管理与依赖解析
- FFI 支持和工作区依赖解析
- 支持 `-I` 参数传递额外包含路径

---

### 2. llc + lld 工具链支持

为 `cayc` 和 `ir2exe` 添加 `--use-llc-lld` 选项，允许在无 Clang 环境下使用 llc + lld 工具链编译。

**平台适配**:
- MinGW: `ld.lld` (GNU 风格)
- MSVC: `lld-link` (COFF 风格)
- macOS: `ld64.lld` (Mach-O 风格)
- Linux: `ld.lld` (ELF 风格)
- WebAssembly: `wasm-ld`

---

### 3. 调试工具 cay-dt / cay-dp

新增两个调试工具，辅助编译器开发与问题诊断：

- **cay-dt** (Token PreViewer): 可视化词法分析结果，支持彩色输出和 JSON 格式
- **cay-dp** (Parse PreViewer): 可视化 AST 结构，支持紧凑模式和 JSON 输出

两个工具均支持 `--no-color` 选项，便于脚本集成。

---

### 4. 预处理器增强

- 新增预处理器指令: `error`, `warning`, `pragma`
- 修复 `#define` 行尾注释处理问题（支持 `//` 和 `/* */` 风格注释）
- 修复符号链接下无法找到 `caylibs` 的问题（使用 `canonicalize` 解析符号链接）
- `cay-dt` 和 `cay-dp` 默认启用预处理，支持 `--no-preprocess` 禁用

---

### 5. FFI 增强

- 新增 `extern` 函数别名支持（`extern fn foo as bar`）
- 新增 `CString` 类型支持
- 新增 `c_int64_t` 和 `c_uint64_t` FFI 类型
- 扩展 FFI 类型与语句支持
- 新增内联 IR、内存分配/释放语句

---

## 标准库扩展

### 1. File.cay 标准库

实现完整的文件操作标准库：

- `File` 类: 打开、关闭、读写、定位等文件操作
- `FileMode` 类: 类型安全的文件模式设置
- `SeekOrigin` 枚举: 文件定位支持
- `FileInfo` 类: stat-based 文件信息获取
- `LineIterator`: 流式逐行读取
- `FileReader.lines()`: 返回行迭代器

**设计修复**:
- `exists()` 使用 `access()` 替代 `fopen()`，避免修改 atime
- `size()` 使用 FileInfo.stat-based 方法，避免 TOCTOU 竞态条件
- `writeInterpolated` 使用 StringBuilder 优化，复杂度从 O(n^2) 降至 O(n)
- `readAllLines` 改为流式读取，内存使用从 O(file_size) 降至 O(max_line_length)

---

### 2. String 方法扩展

新增方法:
- `lastIndexOf`
- `startsWith`
- `endsWith`

---

### 3. Math.cay 修复

修复多个数学函数的设计问题：

- `Math.abs(int)`: 处理 `INT_MIN` 溢出
- `Math.abs(long)`: 处理 `LONG_MIN` 溢出
- `Math.smoothStep`: 添加 `a==b` 除零防护
- `Math.clamp`: 自动交换 min/max 如果顺序错误
- `Math.gcd`: 处理 `INT_MIN` 溢出问题
- `Math.frac`: 使用 `floor` 确保返回 `[0,1)` 范围
- `Math.approxEqualRelative`: 新增相对误差版本
- `Random.nextDouble`: 使用正确 `RAND_MAX` (2147483647)
- `Random.nextBool`: 使用位运算避免模运算偏差
- `Random.nextInt(min,max)`: 处理溢出情况
- `Random.nextGaussian`: 防护 `log(0)`，缓存第二个值
- `Vector2/3.div/normalize`: 除零返回 NaN 而非静默失败

---

### 4. Network.cay 修复

- 修复 socket 句柄类型不匹配问题
- 修复跨平台 socket 发送参数类型不兼容问题
- 修复 `setsockopt` 超时参数传递错误
- 修正 `TcpSocket` 构造函数访问修饰符
- 修复 `HttpClient.send` 的返回值判断逻辑
- 修复 `EasyHTTP` 中 `char` 隐式转 `int` 导致的符号问题

---

## 基础设施

### 1. 官方文档站

- 新增 mdBook 配置与主题样式
- 迁移所有文档到 `docs/` 目录并重构结构
- 添加官方文档站构建脚本与自动化部署配置
- 新增文档页面: LSP 协议、字节码格式、快速开始等

### 2. 版本号与构建

- 为所有二进制工具添加带 git commit 的版本号
- 支持 dirty 状态检测
- 新增 `.verinfo` 各子工具版本配置

### 3. 源映射系统重构

- 重构 `SourceLocation` 结构体，新增文件路径字段
- 实现完整的 IR 源映射生成与解析功能
- 修复错误报告中的文件名显示问题（现在正确显示被包含文件路径）
- 修复错误报告中的行号映射问题（显示原始源文件行号而非预处理后行号）
