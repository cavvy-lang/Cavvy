<p align="center">
  <img src="docs/README/images/Cavvy.png" alt="Cavvy Logo" width="100%">
</p>
<p align="center">
  <a href="README_EN.md">English</a> | 简体中文
</p>

<p align="center">
  <img src="https://img.shields.io/badge/license-GPL3-blue.svg" alt="License">
  <img src="https://img.shields.io/badge/rust-2024%20edition-orange.svg" alt="Rust">
  <img src="https://img.shields.io/badge/platform-Windows%20%7C%20Linux-lightgrey.svg" alt="Platform">
  <img src="https://img.shields.io/badge/build-passing-brightgreen.svg" alt="Build Status">
  <img src="https://img.shields.io/badge/version-5.1.0--Beta.2-blue.svg" alt="Version">
</p>

<p align="center">
  <img src="https://img.shields.io/badge/features-compiler%20%7C%20runtime%20%7C%20FFI-success.svg" alt="Features">
  <img src="https://img.shields.io/badge/tools-6%20binaries-blue.svg" alt="Tools">
</p>

---

Cavvy (Cay) 是一个静态类型的面向对象编程语言，编译为原生机器码，无运行时依赖，无 VM，无 GC。

**核心特性：**
- 🚀 **原生性能**：编译为 Windows EXE / Linux ELF，零开销抽象
- 🛡️ **内存安全**：显式内存管理，RAII 模式支持
- ☕ **Java 风格语法**：熟悉的面向对象编程体验
- 🔧 **完整工具链**：从源码到可执行文件的一站式编译
- 🌉 **FFI 支持**：无缝调用 C 函数和系统库
- 📦 **字节码系统**：支持 `.caybc` 格式和代码混淆

---

## 目录

- [快速开始](#快速开始)
- [安装](#安装)
- [语言特性](#语言特性)
- [工具链](#工具链)
- [代码示例](#代码示例)
- [项目结构](#项目结构)
- [开发状态](#开发状态)
- [许可证](#许可证)

---

## 快速开始

### 编写第一个程序

创建文件 `hello.cay`:

```cay
public class Hello {
    public static void main() {
        println("Hello, World!");
    }
}
```

或使用顶层 main 函数（0.4.3+）：

```cay
public int main() {
    println("Hello from top-level main!");
    return 0;
}
```

### 编译运行

```bash
# 使用 cayc 一站式编译
./target/release/cayc hello.cay hello.exe

# 运行
./hello.exe
```

---

## 安装

### 从源码构建

```bash
# 克隆仓库
git clone https://github.com/cavvy-lang/cavvy.git
cd cavvy

# 构建编译器（Release 模式）
cargo build --release

# 运行测试
cargo test --release
```

### 系统要求

- **Windows**: Windows 10/11 x64
- **Linux**: x86_64 Linux 发行版
- **依赖**: LLVM 17.0+, MinGW-w64 13.2+ (Windows)

---

## 语言特性

### 基础类型系统

```cay
// 整数类型
int a = 10;
long b = 100L;

// 浮点类型
float f = 3.14f;
double d = 3.14159;

// 其他基础类型
boolean flag = true;
char c = 'A';
String s = "Hello, Cavvy!";

// 自动类型推断（0.4.3+）
auto x = 42;        // int
auto pi = 3.14;     // double
auto msg = "hi";    // String
```

### 数组

```cay
// 一维数组
int[] arr = new int[5];
int[] initArr = {1, 2, 3, 4, 5};

// 多维数组
int[][] matrix = new int[3][3];
int[][] grid = {{1, 2}, {3, 4}, {5, 6}};

// 数组长度
int len = arr.length;

// 数组访问
arr[0] = 100;
int val = arr[0];
```

### 控制流

```cay
// if-else
if (a > b) {
    println("a is greater");
} else if (a == b) {
    println("a equals b");
} else {
    println("a is smaller");
}

// switch 语句
switch (value) {
    case 1:
        println("one");
        break;
    case 2:
        println("two");
        break;
    default:
        println("other");
        break;
}

// 循环
for (int i = 0; i < 10; i++) {
    println(i);
}

long j = 0;
while (j < 10) {
    println(j);
    j++;
}

// do-while
int k = 0;
do {
    println(k);
    k++;
} while (k < 5);
```

### 面向对象编程

```cay
// 类定义与继承
public class Animal {
    protected String name;
    
    public Animal(String name) {
        this.name = name;
    }
    
    public void speak() {
        println("Some sound");
    }
}

public class Dog extends Animal {
    public Dog(String name) {
        super(name);
    }
    
    @Override
    public void speak() {
        println(name + " says: Woof!");
    }
}

// 抽象类与接口
public abstract class Shape {
    public abstract double area();
}

public interface Drawable {
    void draw();
}
```

### 方法重载与可变参数

```cay
public class Calculator {
    // 方法重载
    public static int add(int a, int b) {
        return a + b;
    }
    
    public static double add(double a, double b) {
        return a + b;
    }
    
    // 可变参数
    public static int sum(int... numbers) {
        int total = 0;
        for (int i = 0; i < numbers.length; i++) {
            total = total + numbers[i];
        }
        return total;
    }
}
```

### Lambda 表达式与方法引用

```cay
// Lambda 表达式
var add = (int a, int b) -> { return a + b; };
int result = add(3, 4);

// 简写形式
var multiply = (int a, int b) -> a * b;

// 方法引用
var ref = Calculator::add;
```

### 字符串操作

```cay
String s = "Hello World";

// 字符串方法
int len = s.length();
String sub = s.substring(0, 5);
int idx = s.indexOf("World");
String replaced = s.replace("World", "Cavvy");
char ch = s.charAt(0);

// 字符串拼接
String msg = "Hello, " + name + "!";
```

### FFI - 调用 C 函数

```cay
// 声明外部 C 函数
extern {
    int abs(int x);
    double sqrt(double x);
    int strlen(String s);
}

public class MathExample {
    public static void main() {
        int result = abs(-42);
        double root = sqrt(2.0);
        println("Abs: " + result);
        println("Sqrt: " + root);
    }
}
```

### Final 与静态成员

```cay
public class Constants {
    // 编译期常量
    public static final double PI = 3.14159;
    
    // 静态初始化块
    static {
        println("Class initialized");
    }
    
    // Final 类/方法
    public final class Immutable { }
}
```

---

## 工具链

本项目提供六个可执行文件：

| 工具 | 功能 | 用法 |
|------|------|------|
| `cayc` | Cavvy → EXE (一站式) | `cayc source.cay output.exe` |
| `cay-ir` | Cavvy → LLVM IR | `cay-ir source.cay output.ll` |
| `ir2exe` | LLVM IR → EXE | `ir2exe input.ll output.exe` |
| `cay-check` | 语法检查 | `cay-check source.cay` |
| `cay-run` | 直接运行 | `cay-run source.cay` |
| `cay-bcgen` | 生成字节码 | `cay-bcgen source.cay output.caybc` |

### 编译选项

```bash
# 基础编译
cayc hello.cay hello.exe

# 优化级别
cayc -O3 hello.cay hello.exe        # 最高优化
cayc -O0 hello.cay hello.exe        # 无优化（调试）

# 字节码混淆
cayc --obfuscate --obfuscate-level deep hello.cay hello.exe

# 链接库
cayc -lm hello.cay hello.exe        # 链接数学库

# 跨平台目标
cay-ir --target x86_64-linux-gnu hello.cay hello.ll
```

---

## 代码示例

### 九九乘法表

```cay
public class Multiplication {
    public static void main() {
        for (int i = 1; i <= 9; i++) {
            for (int j = 1; j <= i; j++) {
                print(j + "x" + i + "=" + (i*j) + "\t");
            }
            println("");
        }
    }
}
```

### 斐波那契数列

```cay
public class Fibonacci {
    // 递归实现
    public static long fib(int n) {
        if (n <= 1) return n;
        return fib(n - 1) + fib(n - 2);
    }
    
    // 迭代实现
    public static long fibIterative(int n) {
        if (n <= 1) return n;
        long a = 0, b = 1;
        for (int i = 2; i <= n; i++) {
            long temp = a + b;
            a = b;
            b = temp;
        }
        return b;
    }
    
    public static void main() {
        for (int i = 0; i < 20; i++) {
            println("fib(" + i + ") = " + fibIterative(i));
        }
    }
}
```

### 冒泡排序

```cay
public class Sorting {
    public static void bubbleSort(int[] arr) {
        int n = arr.length;
        for (int i = 0; i < n - 1; i++) {
            for (int j = 0; j < n - i - 1; j++) {
                if (arr[j] > arr[j + 1]) {
                    int temp = arr[j];
                    arr[j] = arr[j + 1];
                    arr[j + 1] = temp;
                }
            }
        }
    }
    
    public static void main() {
        int[] numbers = {64, 34, 25, 12, 22, 11, 90};
        bubbleSort(numbers);
        
        print("Sorted: ");
        for (int i = 0; i < numbers.length; i++) {
            print(numbers[i] + " ");
        }
        println("");
    }
}
```

---

## 项目结构

```
cavvy/
├── src/                    # 源代码
│   ├── bin/               # 可执行文件
│   │   ├── cayc.rs        # 一站式编译器
│   │   ├── cay-ir.rs      # Cavvy → IR 编译器
│   │   ├── ir2exe.rs      # IR → EXE 编译器
│   │   ├── cay-check.rs   # 语法检查工具
│   │   ├── cay-run.rs     # 直接运行工具
│   │   ├── cay-bcgen.rs   # 字节码生成器
│   │   └── cay-lsp.rs     # LSP 语言服务器
│   ├── lexer/             # 词法分析器
│   ├── parser/            # 语法分析器
│   ├── semantic/          # 语义分析器
│   ├── codegen/           # 代码生成器
│   ├── ast.rs             # AST 定义
│   ├── types.rs           # 类型系统
│   └── error.rs           # 错误处理
├── examples/              # 示例程序
├── caylibs/               # 标准库
├── docs/                  # 文档
│   └── README/images/     # README 图片资源
├── tests/                 # 测试套件
├── llvm-minimal/          # LLVM 工具链
├── mingw-minimal/         # MinGW 链接器
└── Cargo.toml             # Rust 项目配置
```

---

## 开发状态

### 当前版本: 5.1.0-Beta.2

**已完成功能 (0.4.x):**

- [x] 基础类型系统 (int, long, float, double, boolean, char, String, void)
- [x] 变量声明和赋值（支持 var/let/auto）
- [x] 算术运算符 (+, -, *, /, %)
- [x] 比较运算符 (==, !=, <, <=, >, >=)
- [x] 逻辑运算符 (&&, ||, !)
- [x] 位运算符 (&, |, ^, ~, <<, >>)
- [x] 自增自减运算符 (++, --)
- [x] 复合赋值运算符 (+=, -=, *=, /=, %=)
- [x] 条件语句 (if-else, switch)
- [x] 循环语句 (while, for, do-while)
- [x] break/continue 支持
- [x] 数组（一维和多维）
- [x] 数组初始化器和长度属性
- [x] 字符串拼接和方法
- [x] 类型转换（显式和隐式）
- [x] 方法重载
- [x] 可变参数
- [x] Lambda 表达式
- [x] 方法引用
- [x] 类和单继承
- [x] 抽象类和接口
- [x] 访问控制 (public/private/protected)
- [x] 构造函数和析构函数
- [x] Final 类和 Final 方法
- [x] 静态成员和静态初始化
- [x] @Override 注解
- [x] 顶层 main 函数
- [x] FFI 外部函数接口
- [x] 自动链接器
- [x] 字节码系统 (CayBC)
- [x] 代码混淆
- [x] LSP 语言服务器
- [x] Windows / Linux 跨平台支持

### 开发路线图

详见 [ROADMAP.md](ROADMAP.md)

**即将推出 (0.5.x):**
- 分配器接口和 Arena 分配器
- 泛型集合 (ArrayList, HashMap)
- 智能指针 (UniquePtr, ScopedPtr)
- Result<T, E> 错误处理
- 操作系统线程封装

---

## 技术栈

<p align="center">
  <img src="https://img.shields.io/badge/tech%20stack-Rust%20%7C%20LLVM%20%7C%20MinGW-success.svg" alt="Tech Stack">
</p>

- **前端**: Rust 实现的词法分析、语法分析、语义分析
- **中端**: LLVM IR 代码生成
- **后端**: MinGW-w64 / GCC 工具链
- **字节码**: 自定义 CayBC 格式（基于栈的虚拟机）

---

## 许可证

<p align="center">
  <img src="https://img.shields.io/badge/license-GPL3-blue.svg" alt="License">
</p>

本项目采用 GPL3 许可证。详见 [LICENSE](LICENSE) 文件。

---

## 贡献

欢迎提交 Issue 和 Pull Request。

- 🐛 **Bug 报告**: 使用 GitHub Issues
- 💡 **功能建议**: 查看 ROADMAP.md 后提交 PR
- 📖 **文档改进**: 直接编辑文档并提交

---

## 致谢

- [LLVM Project](https://llvm.org/)
- [MinGW-w64](https://www.mingw-w64.org/)
- [Rust Programming Language](https://www.rust-lang.org/)

---

<p align="center">
  <strong>Cavvy - 编译未来</strong>
</p>
