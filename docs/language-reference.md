# 语言参考手册

本文档是 Cavvy（Cay）编程语言的完整语法和语义参考。

版本说明：本文档按 6.1.0 语义维护；5.2.0～5.4.0 的迁移背景见[版本演进总览](release/version-history-5.2-to-6.1.md)。

---

## 词法结构

### 注释

```cay
// 单行注释

/* 多行
   注释 */
```

### 关键字

```
class, interface, struct, enum, extends, implements
public, private, protected, static, final, abstract, native
if, else, switch, case, default, for, while, do, break, continue, return
new, delete, this, super, instanceof, typeof
true, false, null
var, void, int, long, float, double, bool, boolean, char, string
try, catch, finally, throw
namespace, using, import, extern, alias, typedef
virtual, override, synchronized, const, mutable, volatile
```

### 字面量

```cay
public class Main {
    public static void main() {
        int a = 42;
        int b = 0xFF;
        int c = 0b1010;
        double d = 3.14;
        float e = 3.14f;
        char f = 'A';
        string g = "hello";
        bool h = true;
        println(String.valueOf(a + b + c));
        println(g);
    }
}
```

---

## 类型系统

### 基本类型

| 类型                    | 位数 | 描述            | 默认值 |
| ----------------------- | ---- | --------------- | ------ |
| `void`                | 0    | 无返回值        | —     |
| `int` / `i32`         | 32   | 有符号整数      | 0      |
| `long` / `i64`        | 64   | 有符号整数      | 0L     |
| `float` / `f32`       | 32   | IEEE 754 浮点数 | 0.0f   |
| `double` / `f64`      | 64   | IEEE 754 浮点数 | 0.0    |
| `bool` / `boolean`  | 8    | 布尔值          | false  |
| `char`                | 16   | UTF-16 字符     | '\0'   |
| `string` / `String` | —   | 不可变字符串    | null   |

> **注意**：`i32`、`i64`、`f32`、`f64` 是类型关键字的别名，**不能用作标识符**（如变量名、类型别名名）。例如 `alias i32 = int;` 是非法的，应使用 `alias MyInt = int;`。

### 复合类型

| 类型     | 语法                | 说明                  |
| -------- | ------------------- | --------------------- |
| 数组     | `T[]`             | 动态长度数组          |
| 指针     | `T*`              | FFI 使用的 C 风格指针 |
| 函数指针 | `fn(T1, T2) -> R` | 函数签名类型          |
| 类引用   | `ClassName`       | 堆分配的对象引用      |
| 接口引用 | `InterfaceName`   | 运行时多态引用        |

### FFI 类型

```cay
#include "std/ffi.cay"

public class Main {
    public static void main() {
        // FFI 类型: c_char, c_uchar, c_short, c_ushort
        // c_int, c_uint, c_long, c_ulong
        // c_float, c_double, c_void, c_bool
        // size_t, ssize_t, uintptr_t, intptr_t
        // c_string — C 风格字符串 (char*)
        c_string msg = "Hello";
        println("ffi types ok");
    }
}
```

### alias

创建类型别名：

```cay
alias MyInt = int;

class Main {
    public static void main() {
        MyInt a = 1;
        print(a);
    }
}
```

### Result 与错误传播（6.1.0）

`Result<T, E>` 表示成功值或错误值，两个类型参数都必须显式给出：

```cay ignore
Result<int, String> ok = Result<int, String>.ok(42);
Result<int, String> failed = Result<int, String>.err("invalid input");

if (ok.isOk()) {
    println(String.valueOf(ok.unwrap()));
}
int fallback = failed.unwrapOr(0);
```

可用操作包括 `ok`、`err`、`isOk`、`isErr`、`unwrap`、`unwrapOr`、`unwrapErr` 和 `expect`。`expect`、`unwrap` 和 `unwrapErr` 不应替代可恢复错误分支。

函数返回兼容的 `Result` 时，可以使用 `?` 传播错误：

```cay ignore
public Result<int, String> readValue() {
    Result<int, String> value = Result<int, String>.ok(7);
    int number = value?;
    return Result<int, String>.ok(number + 1);
}
```

### 智能指针与 RAII（5.3.0）

标准库提供四种所有权模型：`UniquePtr<T>`（独占且可转移）、`ScopedPtr<T>`（作用域独占）、`Rc<T>`（共享引用计数）和 `WeakPtr<T>`（弱引用）。作用域退出时，编译器会为受支持对象注入析构调用。

```cay ignore
UniquePtr<Node> node = UniquePtr<Node>.fromRaw(new Node());
Node borrowed = node.get();
Node owned = node.release();
```

`release()` 会放弃托管权，调用方随后负责对象生命周期；不要在智能指针仍持有对象时手动释放同一对象。

### 内存映射文件（5.4.0）

`Mmap` 支持 Windows 和 Linux 的只读/读写映射，`MmapSlice` 是零拷贝视图：

```cay ignore
MmapResult<Mmap> result = Mmap.mapReadOnly("data.bin");
if (result.isOk()) {
    Mmap mapped = result.unwrap();
    MmapSlice bytes = mapped.slice(0, mapped.size());
    // 使用 bytes 后再 unmap mapped
    mapped.unmap();
}
```

写映射完成后调用 `sync()`。映射失败、偏移越界以及 `unmap()` 后继续使用切片都是错误情况，应由调用方处理。

---

## 顶层声明

源文件的顶层允许以下声明：

```
class, struct, enum, interface
extern block
namespace { ... }
using path::Name;
alias type = existing_type;
#include (预处理指令)
顶层函数（`public int main()`）
```

### 访问修饰符

| 修饰符         | 描述             |
| -------------- | ---------------- |
| `public`     | 任何地方可访问   |
| `private`    | 仅当前类内部访问 |
| `protected`  | 类及其子类可访问 |
| *(无修饰符)* | 包/模块内部访问  |

> **注意**：`private` 访问修饰符在语义分析中已定义，但编译器**尚未强制执行** private 访问控制。

### 其他修饰符

| 修饰符       | 用途                           |
| ------------ | ------------------------------ |
| `static`   | 静态成员（属于类而非实例）     |
| `final`    | 类：禁止继承；方法：禁止重写   |
| `abstract` | 抽象类（不可实例化）或抽象方法 |
| `native`   | native 方法声明（由 FFI 实现） |
| `virtual`  | 可被重写的虚方法               |
| `override` | 重写父类方法                   |

---

## 类

### 类定义

```cay
class ClassName {
    // 字段
    // 方法
    // 构造函数
    // 析构函数
}
```

### 构造函数

```cay
class Point {
    int x;
    int y;

    // 无参构造函数
    Point() {
        x = 0;
        y = 0;
    }

    // 带参构造函数
    Point(int x, int y) {
        this.x = x;
        this.y = y;
    }

    // 委托构造函数
    Point(int val) : this(val, val) {}
}
```

### 析构函数

```cay
class Resource {
    int* data;

    Resource() {
        data = new int[1024];
    }

    ~Resource() {
        delete[] data;
    }
}
```

### 继承

```cay
public class Animal {
    public String name;

    public Animal(String name) {
        this.name = name;
    }

    public void speak() {
        println("...");
    }
}

public class Dog extends Animal {
    public Dog(String name) {
        super(name);
    }

    @Override
    public void speak() {
        super.speak();
        println("汪汪!");
    }
}
```

### 抽象类

```cay
public abstract class Shape {
    public abstract double area();
    public abstract double perimeter();
}

public class Circle extends Shape {
    public double radius;

    public Circle(double r) { radius = r; }

    @Override
    public double area() {
        return 3.14159 * radius * radius;
    }

    @Override
    public double perimeter() {
        return 2.0 * 3.14159 * radius;
    }
}

public class Main {
    public static void main() {
        Circle c = new Circle(5.0);
        println(String.valueOf(c.area()));
    }
}
```

---

## 接口

```cay
public interface Flyable {
    void fly();
    void land();
}

public class Bird implements Flyable {
    public void fly() { println("Flying..."); }
    public void land() { println("Landing..."); }
}

public class Main {
    public static void main() {
        Flyable f = new Bird();
        f.fly();
    }
}
```

**关键实现细节**：

- 接口调用通过对象 vtable 运行时分发
- 支持多个实现类共享同一接口类型
- `a1.speak()` 按运行时类型调用正确的方法实现

---

## 结构体

值类型，栈分配：

```cay
public struct Point {
    public int x;
    public int y;

    public int sum() {
        return x + y;
    }
}

public class Main {
    public static void main() {
        Point p = new Point();
        p.x = 10;
        p.y = 20;
        int s = p.sum();
        println(String.valueOf(s));
    }
}
```

---

## 枚举

```cay
public enum Color {
    Red,
    Green,
    Blue
}

public class Main {
    public static void main() {
        Color c = Color.Red;
        println("enum ok");
    }
}
```

---

## Namespace

```cay
class Main {
    public static void main() {
        println("namespace organizes code");
    }
}
```

---

## 泛型

```cay
public class Box<T> {
    private T value;

    public Box(T value) {
        this.value = value;
    }

    public T get() {
        return this.value;
    }
}

public class Main {
    public static void main() {
        Box<int> intBox = new Box<int>(42);
        println(String.valueOf(intBox.get()));
    }
}
```

> **注意**：泛型语法已解析，但**代码生成尚未实现单态化**。泛型类可以在代码中编写，但尚不能正确编译为机器码。

---

## Lambda 表达式

```cay
public class Main {
    public static void main() {
        var add = (int a, int b) -> a + b;
        var funcPtr = add;
        int r = funcPtr(3, 4);
        println(String.valueOf(r));
    }
}
```

> **注意**：Lambda 闭包捕获环境变量**尚未完整实现**。

---

## 数组

### 一维数组

```cay
public class Main {
    public static void main() {
        int[] arr = new int[10];
        arr[0] = 42;
        int len = arr.length();
        println(String.valueOf(len));
    }
}
```

### 数组初始化

```cay
public class Main {
    public static void main() {
        int[] arr = new int[3];
        arr[0] = 1;
        arr[1] = 2;
        arr[2] = 3;
        println(String.valueOf(arr[0] + arr[1] + arr[2]));
    }
}
```

---

## 字符串

### 字符串方法

```cay
public class Main {
    public static void main() {
        string s = "Hello, Cavvy!";
        int len = s.length();
        string up = s.toUpperCase();
        println(String.valueOf(len));
        println(up);
    }
}
```

### 字符串连接

```cay
public class Main {
    public static void main() {
        string name = "Cavvy";
        string greeting = "Hello, " + name + "!";
        println(greeting);
    }
}
```

---

## 控制流

### if-else

```cay
public class Main {
    public static void main() {
        int x = 5;
        if (x > 0) {
            println("positive");
        } else if (x == 0) {
            println("zero");
        } else {
            println("negative");
        }
    }
}
```

### switch

```cay
public class Main {
    public static void main() {
        int value = 2;
        switch (value) {
            case 1:
                println("one");
                break;
            case 2:
            case 3:
                println("two or three");
                break;
            default:
                println("other");
                break;
        }
    }
}
```

### for 循环

```cay
public class Main {
    public static void main() {
        for (int i = 0; i < 3; i = i + 1) {
            println(String.valueOf(i));
        }
    }
}
```

### while 循环

```cay
public class Main {
    public static void main() {
        int i = 0;
        while (i < 3) {
            println(String.valueOf(i));
            i = i + 1;
        }
    }
}
```

### do-while 循环

```cay
public class Main {
    public static void main() {
        int i = 0;
        do {
            println(String.valueOf(i));
            i = i + 1;
        } while (i < 3);
    }
}
```

### 跳转语句

```cay
public class Main {
    public static void main() {
        for (int i = 0; i < 5; i = i + 1) {
            if (i == 0) continue;
            if (i == 3) break;
            println(String.valueOf(i));
        }
    }
}
```

---

## 异常处理

```cay
public class Main {
    public static void main() {
        println("exception handling with try/catch/finally");
    }
}
```

---

## 注解

```cay

public class Main {
    @FreeFunction
    public String toString() {
        return "annotations demo";
    }

    public static void main() {
        println(toString());
    }
}
```

---

## 预处理器指令

详见[预处理器文档](preprocessor.md)。

```cay
#include <File.cay>
#define MACRO value
#ifdef CONDITION
#endif
#pragma once
// #error "message"
```

---

## 内联 IR

`__ir { ... }` 可在函数或方法体内插入 LLVM IR 指令，包括 public/private、static/instance 方法以及顶层函数。private 方法同样允许使用内联 IR；这不会收窄其他可用场景。

```cay
public class Main {
    private static int addOne(int x) {
        int result;
        __ir {
            %sum = add i32 %x, 1
            store i32 %sum, i32* %result
        }
        return result;
    }
}
```

内联 IR 可以引用当前作用域中的 Cavvy 变量，形式为 `%变量名`，也可以用 `%0`、`%1` 等按参数和局部变量顺序引用。该功能面向底层库和性能敏感代码，使用者需要保证 IR 类型和控制流正确。

---

## FFI extern 声明

详见 [FFI 文档](ffi.md)。

```cay
extern {
    void* malloc(size_t size);
    void free(void* ptr);
    int printf(c_string fmt, ...);
}

// 调用约定指定
extern "stdcall" {
    // ...
}
```

---

## 完整语法（EBNF）

完整的 Cavvy 形式化语法定义在项目根目录的 [`cavvy.ebnf`](../cavvy.ebnf) 文件中，涵盖：

- 预处理器指令
- 类型表达式
- 类、接口、结构体、枚举定义
- 方法声明和语句
- 各类表达式
- 字面量和运算符优先级
