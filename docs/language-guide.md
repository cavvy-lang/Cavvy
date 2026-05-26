# Cavvy 语言文档

本文档全面介绍 Cavvy 编程语言的特性和用法。

## 目录

- [概述](#概述)
- [基础概念](#基础概念)
- [类型系统](#类型系统)
- [变量与常量](#变量与常量)
- [运算符](#运算符)
- [控制流](#控制流)
- [函数与方法](#函数与方法)
- [面向对象编程](#面向对象编程)
- [struct — 值类型结构体](#struct--值类型结构体)
- [enum — 标记联合体](#enum--标记联合体)
- [@FreeFunction 注解](#freefunction-注解)
- [泛型语法](#泛型语法)
- [数组与集合](#数组与集合)
- [字符串处理](#字符串处理)
- [内存管理](#内存管理)
- [预处理器](#预处理器)
- [标准库](#标准库)
- [高级特性](#高级特性)

---

## 概述

Cavvy 是一个静态类型的面向对象编程语言，设计目标是：

- **原生性能**：编译为 Windows EXE / Linux ELF，零运行时开销
- **内存安全**：显式内存管理，RAII 模式支持
- **Java 风格语法**：降低学习成本
- **完整工具链**：从源码到可执行文件的一站式编译
- **FFI 支持**：无缝调用 C 函数和系统库

### 版本信息

当前版本：**0.5.1.0**

---

## 基础概念

### 程序入口点

Cavvy 支持三种方式定义程序入口：

#### 1. 顶层 main 函数（推荐）

```cay
public int main() {
    println("Hello, World!");
    return 0;
}
```

#### 2. 带参数的顶层 main

```cay
public int main(String[] args) {
    for (int i = 0; i < args.length; i++) {
        println("Arg " + i + ": " + args[i]);
    }
    return 0;
}
```

#### 3. 类内静态 main 方法（传统 Java 风格）

```cay
@main
public class Main {
    public static void main(String[] args) {
        println("Hello from class main!");
    }
}
```

### 代码结构

```cay
// 预处理器指令
#define DEBUG

// 类定义 - 将顶层函数改为静态方法
public class Helper {
    public static void helper() {
        println("Helper function called");
    }
}

// 类定义
public class MyClass {
    // 字段
    private int value;
    
    // 构造函数
    public MyClass(int v) {
        this.value = v;
    }
    
    // 方法
    public int getValue() {
        return value;
    }
}

// 程序入口
public int main() {
    MyClass obj = new MyClass(42);
    println(obj.getValue());
    Helper.helper();
    return 0;
}
```

---

## 类型系统

### 基本类型

| 类型 | 大小 | 范围 | 示例 |
|------|------|------|------|
| `boolean` | 1 byte | `true`, `false` | `boolean flag = true;` |
| `char` | 2 bytes | Unicode 字符 | `char c = 'A';` |
| `byte` | 1 byte | -128 ~ 127 | `byte b = 127;` |
| `short` | 2 bytes | -32768 ~ 32767 | `short s = 1000;` |
| `int` | 4 bytes | -2^31 ~ 2^31-1 | `int i = 42;` |
| `long` | 8 bytes | -2^63 ~ 2^63-1 | `long l = 100L;` |
| `float` | 4 bytes | IEEE 754 单精度 | `float f = 3.14f;` |
| `double` | 8 bytes | IEEE 754 双精度 | `double d = 3.14159;` |

### 引用类型

- **String**：字符串类型
- **数组**：`int[]`, `String[][]` 等
- **类**：用户自定义类型
- **接口**：抽象类型定义

### 类型转换

#### 隐式转换（自动）

```cay
int i = 100;
long l = i;        // int -> long（扩大转换）
float f = i;       // int -> float
double d = f;      // float -> double
```

#### 显式转换（强制）

```cay
double d = 3.14;
int i = (int)d;    // double -> int（截断小数）

long l = 1000;
int j = (int)l;    // long -> int（可能溢出）
```

#### 字符串转换

```cay
// int -> String：使用 String.valueOf()
int i = 42;
String s = String.valueOf(i);

// String -> int：使用 Integer.parseInt()
// int n = Integer.parseInt("123");
```

---

## 变量与常量

### 变量声明方式

```cay
public int main() {
    // 1. 传统方式（类型前置）
    int x = 10;
    String name = "Cavvy";
    
    // 2. 现代方式（类型后置）
    var y: int = 20;
    let greeting: String = "Hello";
    
    // 3. 自动类型推断
    auto count = 100;      // int
    auto pi = 3.14159;     // double
    auto message = "Hi";   // String
    
    // 4. 不可变变量
    final int MAX = 100;
    final var MIN: int = 0;
    
    return 0;
}
```

### 变量修饰符

| 修饰符 | 说明 |
|--------|------|
| `final` | 变量不可重新赋值 |
| `static` | 静态变量（类级别） |
| `public` | 公开访问 |
| `private` | 私有访问 |
| `protected` | 包内和子类可访问 |

### 作用域规则

```cay
public class ScopeDemo {
    private int classVar = 10;  // 类作用域
    
    public void method() {
        int methodVar = 20;     // 方法作用域
        
        if (true) {
            int blockVar = 30;  // 块作用域
            println(classVar);  // 可访问
            println(methodVar); // 可访问
            println(blockVar);  // 可访问
        }
        
        // println(blockVar);   // 错误！blockVar 不可访问
    }
}
```

---

## 运算符

### 算术运算符

| 运算符 | 描述 | 示例 |
|--------|------|------|
| `+` | 加法 | `a + b` |
| `-` | 减法 | `a - b` |
| `*` | 乘法 | `a * b` |
| `/` | 除法 | `a / b` |
| `%` | 取模 | `a % b` |

### 赋值运算符

| 运算符 | 描述 | 示例 |
|--------|------|------|
| `=` | 赋值 | `a = b` |
| `+=` | 加并赋值 | `a += b` |
| `-=` | 减并赋值 | `a -= b` |
| `*=` | 乘并赋值 | `a *= b` |
| `/=` | 除并赋值 | `a /= b` |
| `%=` | 取模并赋值 | `a %= b` |

### 自增/自减运算符

```cay
int a = 5;
int b = ++a;   // 前置自增：a=6, b=6
int c = a++;   // 后置自增：c=6, a=7
```

### 比较运算符

| 运算符 | 描述 | 示例 |
|--------|------|------|
| `==` | 等于 | `a == b` |
| `!=` | 不等于 | `a != b` |
| `<` | 小于 | `a < b` |
| `>` | 大于 | `a > b` |
| `<=` | 小于等于 | `a <= b` |
| `>=` | 大于等于 | `a >= b` |

### 逻辑运算符

| 运算符 | 描述 | 示例 |
|--------|------|------|
| `&&` | 逻辑与 | `a && b` |
| `\|\|` | 逻辑或 | `a \|\| b` |
| `!` | 逻辑非 | `!a` |

### 位运算符

| 运算符 | 描述 | 示例 |
|--------|------|------|
| `&` | 按位与 | `a & b` |
| `\|` | 按位或 | `a \| b` |
| `^` | 按位异或 | `a ^ b` |
| `~` | 按位取反 | `~a` |
| `<<` | 左移 | `a << 2` |
| `>>` | 右移 | `a >> 2` |

### 三元运算符

```cay
int a = 10;
int b = 20;
int max = (a > b) ? a : b;  // max = 20
```

---

## 控制流

### 条件语句

#### if-else

```cay
int score = 85;

if (score >= 90) {
    println("A");
} else if (score >= 80) {
    println("B");
} else if (score >= 70) {
    println("C");
} else {
    println("D");
}
```

#### switch

```cay
int day = 3;

switch (day) {
    case 1:
        println("Monday");
        break;
    case 2:
        println("Tuesday");
        break;
    case 3:
        println("Wednesday");
        break;
    case 4:
    case 5:
        println("Thursday or Friday");
        break;
    default:
        println("Weekend");
        break;
}
```

### 循环语句

#### for 循环

```cay
// 基本 for 循环
for (int i = 0; i < 10; i++) {
    println("i = " + i);
}

// 多个变量
for (int i = 0, j = 10; i < j; i++, j--) {
    println("i=" + i + ", j=" + j);
}

// 遍历数组
int[] arr = {1, 2, 3, 4, 5};
for (int i = 0; i < arr.length; i++) {
    println(arr[i]);
}
```

#### while 循环

```cay
int count = 0;
while (count < 5) {
    println("Count: " + count);
    count++;
}
```

#### do-while 循环

```cay
int num = 0;
do {
    println("Number: " + num);
    num++;
} while (num < 5);
```

#### 数组遍历

```cay
int[] numbers = {1, 2, 3, 4, 5};
for (int i = 0; i < numbers.length; i++) {
    println(numbers[i]);
}
```

### 跳转语句

```cay
// break
for (int i = 0; i < 10; i++) {
    if (i == 5) break;
    println(i);  // 输出 0-4
}

// continue
for (int i = 0; i < 10; i++) {
    if (i % 2 == 0) continue;
    println(i);  // 输出奇数
}

// 带标签的 break
outer: for (int i = 0; i < 3; i++) {
    for (int j = 0; j < 3; j++) {
        if (i == 1 && j == 1) break outer;
        println("i=" + i + ", j=" + j);
    }
}
```

---

## 函数与方法

### 函数定义

> **注意**：当前版本暂不支持顶层函数（除 `main` 外），请在类中使用静态方法：

```cay
// 使用类的静态方法模拟顶层函数
class MathUtils {
    public static int add(int a, int b) {
        return a + b;
    }
    
    public static void greet(String name) {
        println("Hello, " + name);
    }
    
    public static int max(int a, int b, int c) {
        int m = a;
        if (b > m) m = b;
    if (c > m) m = c;
    return m;
}
```

### 方法重载

```cay
public class Calculator {
    // 无参数
    public int add() {
        return 0;
    }
    
    // 一个参数
    public int add(int a) {
        return a;
    }
    
    // 两个参数
    public int add(int a, int b) {
        return a + b;
    }
    
    // 不同类型
    public double add(double a, double b) {
        return a + b;
    }
}
```

### 可变参数

> **注意**：可变参数当前版本暂不支持，请使用方法重载替代。

```cay
// 使用方法重载替代可变参数
public int sum(int a, int b) {
    return a + b;
}

public int sum(int a, int b, int c) {
    return a + b + c;
}

// 使用
int result1 = sum(1, 2);
int result2 = sum(1, 2, 3);
```

### 递归函数

```cay
public int factorial(int n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}

public int fibonacci(int n) {
    if (n <= 1) return n;
    return fibonacci(n - 1) + fibonacci(n - 2);
}
```

### Lambda 表达式

> **注意**：Lambda 表达式和函数式接口当前版本暂不支持。

```cay
// 使用匿名类替代 Lambda（如果支持）
// 或使用方法引用
```

---

## 面向对象编程

### 类定义

```cay
public class Person {
    // 字段
    private String name;
    private int age;
    
    // 静态字段
    private static int count = 0;
    
    // 构造函数
    public Person(String name, int age) {
        this.name = name;
        this.age = age;
        count++;
    }
    
    // 默认构造函数
    public Person() {
        this("Unknown", 0);
    }
    
    // 方法
    public void introduce() {
        println("I'm " + name + ", " + age + " years old.");
    }
    
    // Getter
    public String getName() {
        return name;
    }
    
    // Setter
    public void setName(String name) {
        this.name = name;
    }
    
    // 静态方法
    public static int getCount() {
        return count;
    }
}
```

### 继承

```cay
// 基类
public class Animal {
    protected String name;
    
    public Animal(String name) {
        this.name = name;
    }
    
    public void speak() {
        println(name + " makes a sound");
    }
}

// 子类
public class Dog : Animal {
    private String breed;
    
    public Dog(String name, String breed) {
        super(name);
        this.breed = breed;
    }
    
    public void speak() {
        println(name + " barks");
    }
    
    public void fetch() {
        println(name + " is fetching");
    }
}
```

### 抽象类

```cay
public abstract class Shape {
    protected String color;
    
    public Shape(String color) {
        this.color = color;
    }
    
    // 抽象方法
    public abstract double getArea();
    
    // 具体方法
    public void printColor() {
        println("Color: " + color);
    }
}

public class Circle : Shape {
    private double radius;
    
    public Circle(String color, double radius) {
        super(color);
        this.radius = radius;
    }
    
    public double getArea() {
        return 3.14159 * radius * radius;
    }
}
```

### 接口

> **注意**：接口当前版本暂不支持，请使用抽象类替代。

```cay
// 使用抽象类替代接口
public abstract class Drawable {
    public abstract void draw();
    public abstract void move(int x, int y);
}

public abstract class Resizable {
    public abstract void resize(double factor);
}

// 单继承（Cavvy 暂不支持多继承）
public class Rectangle : Drawable {
    private int x, y;
    private int width, height;
    
    public void draw() {
        println("Drawing rectangle at (" + x + "," + y + ")");
    }
    
    public void move(int dx, int dy) {
        this.x += dx;
        this.y += dy;
    }
}
```

### 访问控制

| 修饰符 | 同类 | 同包 | 子类 | 所有 |
|--------|------|------|------|------|
| `public` | ✓ | ✓ | ✓ | ✓ |
| `protected` | ✓ | ✓ | ✓ | ✗ |
| 默认（包私有） | ✓ | ✓ | ✗ | ✗ |
| `private` | ✓ | ✗ | ✗ | ✗ |

### final 关键字

```cay
// final 类 - 不可继承
public final class String {
    // ...
}

// final 方法 - 不可重写
public class Parent {
    public final void cannotOverride() {
        // ...
    }
}

// final 变量 - 不可重新赋值
public void method() {
    final int MAX = 100;
    // MAX = 200;  // 错误！
}
```

---

---

## struct — 值类型结构体

`struct` 是 Cavvy 新增的**值类型**，用于创建栈分配的轻量级复合数据，无需 GC 追踪。

```cay
public struct Point {
    public int x;
    public int y;
}
```

### 使用示例

```cay
Point p = new Point();
p.x = 10;
p.y = 20;
int val = p.getX();  // 10
```

### 特点

- 栈分配，无堆开销
- 支持字段和方法
- 不参与继承体系
- 字段和方法默认公开

参见：[struct 详细文档](struct.md)

---

## enum — 标记联合体

`enum` 是 **Tagged Union / ADT**，用于定义带类型安全变体的联合体。

```cay
public enum Option<T> {
    Some(T),
    None
}

public enum Result {
    Ok(int),
    Err(String)
}
```

参见：[enum 详细文档](enum.md)

---

## @FreeFunction 注解

```cay
public class MathUtils {
    @FreeFunction
    public static int add(int a, int b) {
        return a + b;
    }
}

int sum = add(1, 2);
```

参见：[@FreeFunction 详细文档](freefunction.md)

---

## 泛型语法

```cay
class Box<T> { private T value; }
enum Optional<T> { Some(T), None }
Optional<int> opt = Optional<int>.of(42);
```

参见：[泛型语法详细文档](generics.md)

---

## 数组与集合

### 一维数组

```cay
// 声明和创建
int[] numbers = new int[5];

// 初始化
int[] values = {1, 2, 3, 4, 5};

// 访问元素
numbers[0] = 10;
int first = values[0];

// 数组长度
int len = values.length;

// 遍历
for (int i = 0; i < values.length; i++) {
    println(values[i]);
}

for (int v : values) {
    println(v);
}
```

### 多维数组

```cay
// 二维数组
int[][] matrix = new int[3][3];
matrix[0][0] = 1;
matrix[1][1] = 2;
matrix[2][2] = 3;

// 初始化
int[][] grid = {
    {1, 2, 3},
    {4, 5, 6},
    {7, 8, 9}
};

// 不规则数组
int[][] jagged = new int[3][];
jagged[0] = new int[2];
jagged[1] = new int[4];
jagged[2] = new int[3];

// 遍历
for (int i = 0; i < grid.length; i++) {
    for (int j = 0; j < grid[i].length; j++) {
        print(grid[i][j] + " ");
    }
    println();
}
```

### 数组操作

```cay
// 数组拷贝
int[] src = {1, 2, 3, 4, 5};
int[] dest = new int[5];
System.arraycopy(src, 0, dest, 0, 5);

// 数组填充
Arrays.fill(numbers, 0);

// 数组排序
Arrays.sort(values);

// 二分查找
int index = Arrays.binarySearch(values, 3);
```

---

## 字符串处理

### 字符串创建

```cay
String s1 = "Hello";                    // 字符串字面量
String s2 = new String("World");        // 构造函数
String s3 = String.valueOf(123);        // 从其他类型转换
```

### 字符串方法

```cay
String s = "Hello, World!";

// 基本信息
int len = s.length();                   // 13

// 字符访问
char c = s.charAt(0);                   // 'H'

// 子串
String sub1 = s.substring(7);           // "World!"
String sub2 = s.substring(0, 5);        // "Hello"

// 查找
int idx1 = s.indexOf("World");          // 7
int idx2 = s.indexOf('o');              // 4
int idx3 = s.lastIndexOf('o');          // 8

// 替换
String replaced = s.replace("World", "Cavvy");  // "Hello, Cavvy!"

// 大小写转换
String upper = s.toUpperCase();         // "HELLO, WORLD!"
String lower = s.toLowerCase();         // "hello, world!"

// 去除空白
String trimmed = "  hello  ".trim();     // "hello"

// 分割
String[] parts = "a,b,c".split(",");    // ["a", "b", "c"]

// 连接
String joined = String.join("-", parts); // "a-b-c"

// 比较
boolean eq = s.equals("Hello, World!"); // true
boolean eqIgn = s.equalsIgnoreCase("hello, world!"); // true
int cmp = s.compareTo("Hello");         // 正数（s 更大）

// 包含/前缀/后缀
boolean contains = s.contains("World"); // true
boolean starts = s.startsWith("Hello"); // true
boolean ends = s.endsWith("!");         // true
```

### StringBuilder

```cay
#include <StringBuilder.cay>

StringBuilder sb = new StringBuilder();
sb.append("Hello");
sb.append(", ");
sb.append("World");

String result = sb.toString();          // "Hello, World"

// 链式调用
String result2 = new StringBuilder()
    .append("Count: ")
    .append(42)
    .toString();
```

---

## 内存管理

### 内存分配函数

```cay
// 从 libc 导入
extern long malloc(int size);
extern long calloc(int num, int size);
extern void free(long ptr);
extern long realloc(long ptr, int size);

// 使用示例
public int main() {
    // 分配内存
    long ptr = malloc(100);
    
    // 使用内存...
    
    // 释放内存
    free(ptr);
    
    return 0;
}
```

### RAII 模式

```cay
public class FileHandle {
    private long handle;
    
    public FileHandle(String path) {
        this.handle = fopen(path, "r");
    }
    
    // 析构函数（ dispose 模式）
    public void dispose() {
        if (handle != 0) {
            fclose(handle);
            handle = 0;
        }
    }
}

// 使用
public void readFile(String path) {
    FileHandle file = new FileHandle(path);
    // 使用 file...
    file.dispose();  // 显式释放
}
```

### 数组分配

```cay
// 栈分配（推荐小数组）
int[] small = new int[10];

// 堆分配（大数组）
long bigArray = calloc(1000, 4);  // 1000 个 int

// 使用指针访问
__cay_write_int(bigArray, 42);  // 写入
int val = __cay_read_int(bigArray);  // 读取

free(bigArray);
```

---

## 预处理器

### 文件包含

```cay
// 系统库
#include <Network.cay>
#include <StringBuilder.cay>

// 用户库
#include "mylib.cay"
```

### 宏定义

```cay
// 对象宏
#define DEBUG
#define VERSION "1.0.0"
#define MAX_SIZE 100

// 使用
#ifdef DEBUG
    println("Debug mode");
#endif
```

### 条件编译

```cay
#define PLATFORM_WINDOWS

#ifdef PLATFORM_WINDOWS
    #include <windows.cay>
#else
    #include <linux.cay>
#endif

// 嵌套条件
#if VERSION_MAJOR >= 1
    // 1.x 功能
#elif VERSION_MAJOR == 0
    // 0.x 功能
#else
    // 其他
#endif
```

### 防止重复包含

```cay
// MyLib.cay
#ifndef MYLIB_CAY
#define MYLIB_CAY

// 库内容...

#endif
```

---

## 标准库

### IOPlus - 输入输出扩展

```cay
#include <IOPlus.cay>

// 格式化输出
printf("Name: %s, Age: %d\n", name, age);

// 文件操作
File file = fopen("data.txt", "r");
String line = fgets(file);
fclose(file);

// 读取输入
int num = readInt();
float f = readFloat();
String line = readLine();
```

### StringPlus - 字符串扩展

```cay
#include <StringPlus.cay>

// 高级字符串操作
String reversed = StringPlus.reverse("hello");  // "olleh"
boolean isPal = StringPlus.isPalindrome("radar");  // true
String replaced = StringPlus.replaceAll("a,b,c", ",", "-");  // "a-b-c"
```

### Network - 网络编程

```cay
#include <Network.cay>

// TCP 服务器
TcpServer server = new TcpServer();
server.bind("0.0.0.0", 8080);
server.listen(5);

TcpSocket client = server.accept();
String request = client.receive(1024);
client.send("HTTP/1.1 200 OK\r\n\r\nHello");
client.close();
```

### EasyHTTP - HTTP 客户端

```cay
#include <EasyHTTP.cay>

// GET 请求
HttpResponse response = EasyHTTP.get("https://api.example.com/data");
println(response.body);

// POST 请求
HttpResponse resp2 = EasyHTTP.post(
    "https://api.example.com/users",
    "{\"name\":\"John\"}",
    "application/json"
);
```

### Allocator - 内存分配器

```cay
#include <Allocator.cay>

// 创建内存池
Allocator pool = new Allocator(1024 * 1024);  // 1MB 池

// 从池中分配
long ptr = pool.alloc(100);

// 释放整个池
pool.freeAll();
```

---

## 高级特性

### 泛型（计划中）

```cay
// 泛型类（未来版本）
public class Box<T> {
    private T value;
    
    public void set(T value) {
        this.value = value;
    }
    
    public T get() {
        return value;
    }
}

// 使用
Box<Integer> intBox = new Box<>();
Box<String> strBox = new Box<>();
```

### 注解

```cay
// @main 注解 - 指定程序入口类
@main
public class Main {
    public static void main() {
        // ...
    }
}

// 注解当前版本暂不支持
// 未来版本将支持 @Override、@Deprecated 等注解
```

### 异常处理

> **注意**：异常处理当前版本暂不支持。

```cay
// 异常处理（未来版本将支持）
// try {
//     riskyOperation();
// } catch (Exception e) {
//     println("Error: " + e.message);
// }
```

### 类型别名

Cavvy 支持使用 `alias` 关键字定义类型别名，简化复杂类型的使用：

```cay
// 基本类型别名
alias MyInt = int;
alias MyString = String;

// C 类型别名
alias CInt = c_int;
alias CLong = c_long;
alias VoidPtr = ptr;
alias SizeT = size_t;

// 使用类型别名
MyInt count = 10;
VoidPtr data = malloc(100);
```

### 函数指针

Cavvy 支持函数指针类型定义，用于回调函数和动态函数调用：

```cay
// 定义函数指针类型
alias CompareFn = fn(int, int) -> int;
alias Callback = fn(ptr) -> void;
alias Predicate = fn(int) -> bool;

// 实现函数
fn compare_asc(a: int, b: int) -> int {
    return a - b;
}

// 使用函数指针
CompareFn cmp = compare_asc;
int result = cmp(5, 3);  // result = 2

// 函数指针作为参数
void sort(int[] arr, CompareFn comparator) {
    // 使用 comparator 进行排序
}

// 函数指针作为返回值
CompareFn getComparator(bool ascending) {
    if (ascending) {
        return compare_asc;
    } else {
        return compare_desc;
    }
}
```

函数指针类型语法：
- `fn(参数类型列表) -> 返回类型`
- 参数类型列表中的参数名是可选的
- 返回类型可以是任何有效类型，包括 `void`

---

## 最佳实践

### 命名规范

| 类型 | 规范 | 示例 |
|------|------|------|
| 类名 | PascalCase | `MyClass`, `HttpRequest` |
| 方法名 | camelCase | `getName()`, `processData()` |
| 变量名 | camelCase | `userName`, `totalCount` |
| 常量名 | UPPER_SNAKE_CASE | `MAX_SIZE`, `PI` |
| 包名 | 小写 | `com.example.utils` |

### 代码组织

```
project/
├── src/
│   ├── main.cay          # 程序入口
│   ├── models/           # 数据模型
│   │   ├── User.cay
│   │   └── Order.cay
│   ├── utils/            # 工具类
│   │   └── StringUtils.cay
│   └── services/         # 业务逻辑
│       └── UserService.cay
├── caylibs/              # 第三方库
├── examples/             # 示例代码
└── docs/                 # 文档
```

### 性能建议

1. **优先使用栈分配**：小数组和对象优先使用栈分配
2. **避免不必要的装箱**：基本类型比包装类更高效
3. **使用 StringBuilder**：大量字符串拼接时使用 StringBuilder
4. **及时释放资源**：使用完堆内存后立即 free
5. **使用 final**：标记不可变变量和方法，帮助编译器优化

---

## 相关文档

- [快速开始](quickstart.md) - 5 分钟上手 Cavvy
- [FFI 使用指南](ffi-guide.md) - 调用 C 函数和系统库
- [语法参考](syntax-reference.md) - 完整的语法规范
