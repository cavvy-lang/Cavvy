# Cavvy 编程语言

[English](README_EN.md) | 简体中文

![License](https://img.shields.io/badge/license-GPL3-blue.svg)
![Rust](https://img.shields.io/badge/rust-2024%20edition-orange.svg)
![Platform](https://img.shields.io/badge/platform-Windows-lightgrey.svg)

Cavvy (Cay) 是一个简单的面向对象编程语言，支持编译为~~原生 Windows 可执行文件~~原生 Windows 可执行文件和 Linux 可执行文件。

Cavvy 是整个 Ethernos 编程语言工具链中的里程碑，它是 Ethernos 发布的所有编程语言中，第一个编译型编程语言。

![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)
![Version](https://img.shields.io/badge/version-0.3.4-blue.svg)

> ~~**注意**: 当前版本主要针对 Windows 平台。如果您熟悉 Linux 开发环境，欢迎帮助我们移植到 Linux 平台！~~

## 特性

![Features](https://img.shields.io/badge/features-compiler%20%7C%20runtime-success.svg)

- **完整的编译链**: Cavvy 源代码 -> LLVM IR -> Windows EXE
- **面向对象**: 支持类、方法、静态成员、方法重载、可变参数
- **类型系统**: 支持 int、long、float、double、boolean、char、String、void、数组等类型
- **控制流**: 支持 if-else、while、for、do-while 循环、switch 语句
- **运算符**: 支持算术、比较、逻辑、位运算符、自增自减、复合赋值运算符
- **字符串操作**: 支持字符串字面量、字符串拼接、字符串方法（length, substring, indexOf, replace, charAt）
- **类型转换**: 支持显式类型转换和字面量隐式类型转换
- **Lambda 表达式**: 支持 `(params) -> { body }` 语法
- **方法引用**: 支持静态/实例方法引用 `ClassName::methodName`
- **MinGW-w64 支持**: 使用开源工具链，无 MSVC 版权依赖

## 快速开始

### 安装

```bash
# 克隆仓库
git clone https://github.com/Ethernos-Studio/Cavvy.git
cd eol

# 构建编译器
cargo build --release
```

### 编写第一个程序

创建文件 `hello.cay`:

```cay
public class Hello {
    public static void main() {
        println("Hello, World!");
    }
}
```

### 编译运行

```bash
# 使用 cayc 一站式编译
./target/release/cayc hello.cay hello.exe

# 运行
./hello.exe
```

输出:
```
Hello, World!
```

## 工具链

![Tools](https://img.shields.io/badge/tools-4%20binaries-blue.svg)

本项目提供四个可执行文件：

| 工具 | 功能 | 用法 |
|------|------|------|
| `cayc` | Cavvy -> EXE (一站式) | `cayc source.cay output.exe` |
| `cay-ir` | Cavvy -> LLVM IR | `cay-ir source.cay output.ll` |
| `ir2exe` | LLVM IR -> EXE | `ir2exe input.ll output.exe` |
| `cay-check` | 检查代码语法 | `cay-check source.cay` |

## 语言语法

### 变量声明

```cay
int a = 10;
long b = 100L;
float f = 3.14f;
double d = 3.14159;
boolean flag = true;
char c = 'A';
String s = "Hello";
```

### 数组

```cay
// 数组声明和初始化
int[] arr = new int[5];
int[] initArr = {1, 2, 3, 4, 5};

// 多维数组
int[][] matrix = new int[3][3];

// 数组长度
int len = arr.length;

// 数组访问
arr[0] = 100;
int val = arr[0];
```

### 算术运算

```cay
int sum = a + b;
int diff = a - b;
int prod = a * b;
int quot = a / b;
int rem = a % b;

// 自增自减
a++;
--b;

// 复合赋值
a += 5;
b *= 2;
```

### 条件语句

```cay
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
```

### 循环

```cay
// while 循环
long i = 0;
while (i < 10) {
    println(i);
    i = i + 1;
}

// for 循环
for (int j = 0; j < 10; j++) {
    println(j);
}

// do-while 循环
do {
    println(i);
    i++;
} while (i < 10);

// break 和 continue
for (int k = 0; k < 100; k++) {
    if (k == 50) break;
    if (k % 2 == 0) continue;
    println(k);
}
```

### 字符串操作

```cay
String name = "Cavvy";
String message = "Hello, " + name + "!";
println(message);

// 字符串方法
String s = "Hello World";
int len = s.length();
String sub = s.substring(0, 5);
int idx = s.indexOf("World");
String replaced = s.replace("World", "Cavvy");
char ch = s.charAt(0);
```

### 类型转换

```cay
// 显式类型转换
int i = (int)3.14;
double d = (double)10;

// 字面量隐式转换
float f = 3.14f;
long l = 100L;
```

### 方法定义与重载

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
    
    public static void main() {
        println(add(1, 2));
        println(add(1.5, 2.5));
        println(sum(1, 2, 3, 4, 5));
    }
}
```

### Lambda 表达式

```cay
// Lambda 表达式
var add = (int a, int b) -> { return a + b; };
int result = add(3, 4);

// 方法引用
var ref = Calculator::add;
```

## 示例

### 九九乘法表

```cay
public class Multiplication {
    public static void main() {
        long i = 1;
        while (i <= 9) {
            long j = 1;
            while (j <= i) {
                long product = i * j;
                print(i);
                print("x");
                print(j);
                print("=");
                print(product);
                if (product < 10) {
                    print("  ");
                } else {
                    print(" ");
                }
                j = j + 1;
            }
            println("");
            i = i + 1;
        }
    }
}
```

编译运行:
```bash
./target/release/cayc examples/multiplication.cay mult.exe
./mult.exe
```

## 项目结构

```
cavvy/
├── src/                    # 源代码
│   ├── bin/               # 可执行文件
│   │   ├── cayc.rs        # 一站式编译器
│   │   ├── cay-ir.rs      # Cavvy -> IR 编译器
│   │   ├── ir2exe.rs      # IR -> EXE 编译器
│   │   └── cay-check.rs   # 语法检查工具
│   ├── lexer/             # 词法分析器
│   ├── parser/            # 语法分析器
│   ├── semantic/          # 语义分析器
│   ├── codegen/           # 代码生成器
│   ├── ast.rs             # AST 定义
│   ├── types.rs           # 类型系统
│   └── error.rs           # 错误处理
├── examples/              # 示例程序
├── lib/mingw64/           # MinGW-w64 库
├── llvm-minimal/          # LLVM 工具链
├── mingw-minimal/         # MinGW 链接器
└── Cargo.toml             # Rust 项目配置
```

## 技术栈

![Tech Stack](https://img.shields.io/badge/tech%20stack-Rust%20%7C%20LLVM%20%7C%20MinGW-success.svg)

- **前端**: Rust 实现的词法分析、语法分析、语义分析
- **中端**: LLVM IR 代码生成
- **后端**: MinGW-w64 工具链（lld 链接器）

## 开发状态

![Status](https://img.shields.io/badge/status-active%20development-green.svg)

### 已完成功能 (0.3.x)

- [x] 基础类型系统 (int, long, float, double, boolean, char, String, void)
- [x] 变量声明和赋值
- [x] 算术运算符 (+, -, *, /, %)
- [x] 比较运算符 (==, !=, <, <=, >, >=)
- [x] 逻辑运算符 (&&, ||)
- [x] 位运算符 (&, |, ^, ~, <<, >>)
- [x] 自增自减运算符 (++, --)
- [x] 复合赋值运算符 (+=, -=, *=, /=, %=)
- [x] 条件语句 (if-else, switch)
- [x] 循环语句 (while, for, do-while)
- [x] break/continue 支持
- [x] 数组 (一维和多维)
- [x] 数组初始化器
- [x] 数组长度属性
- [x] 字符串拼接
- [x] 字符串方法 (length, substring, indexOf, replace, charAt)
- [x] 类型转换 (显式和隐式)
- [x] 方法重载
- [x] 可变参数
- [x] Lambda 表达式
- [x] 方法引用
- [x] 内置函数 (print, println, readInt, readFloat, readLine)
- [x] 完整的编译链

### 开发路线图

详见 [ROADMAP.md](ROADMAP.md)

## 许可证

![License](https://img.shields.io/badge/license-GPL3-blue.svg)

本项目采用 GPL3 许可证。详见 [LICENSE](LICENSE) 文件。

## 贡献

欢迎提交 Issue 和 Pull Request。

## 致谢

- [LLVM Project](https://llvm.org/)
- [MinGW-w64](https://www.mingw-w64.org/)
- [Rust Programming Language](https://www.rust-lang.org/)