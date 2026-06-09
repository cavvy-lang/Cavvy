# 语言概述

Cavvy（Cay）是一门静态类型、面向对象的编程语言，语法风格接近 Java/C#，同时保留 C 风格预处理器和 FFI。本文档从宏观层面介绍语言的核心特性。

---

## 核心设计理念

1. **静态类型安全**：所有变量、参数、返回值在编译时具有确定类型
2. **面向对象**：支持类、继承、接口、运行时多态（vtable 方法分发）
3. **C 风格预处理**：支持 `#include`、`#define`、`#ifdef` 等指令
4. **原生编译**：通过 LLVM IR → clang 编译为高效机器码
5. **FFI 优先**：内建外部函数接口，直接调用 C 库
6. **渐进式支持**：CayBC 字节码格式支持 JIT/AOT 执行

---

## 类型系统

### 基本类型

| 类型 | 描述 | 默认值 |
|---|---|---|
| `int` | 32 位有符号整数 | 0 |
| `long` | 64 位有符号整数 | 0L |
| `float` | 32 位浮点数 | 0.0f |
| `double` | 64 位浮点数 | 0.0 |
| `char` | 16 位 Unicode 字符 | '\0' |
| `bool` | 布尔值 | false |
| `string` | 不可变字符串 | null |
| `void` | 无返回值 | — |

### 复合类型

| 类型 | 示例 | 说明 |
|---|---|---|
| 数组 | `int[]`、`string[]` | 动态数组，`new` 分配 |
| 类 | `class Foo` | 引用类型，堆分配 |
| 接口 | `interface Bar` | 纯抽象类型 |
| 结构体 | `struct Point` | 值类型，栈分配 |
| 枚举 | `enum Color` | 命名常量集合 |
| 函数指针 | `fn(int) -> int` | 函数签名类型 |

---

## 类与面向对象

### 类定义

```cay
public class Counter {
    private int current;

    public Counter(int start) {
        this.current = start;
    }

    public void add(int value) {
        this.current = this.current + value;
    }

    public int value() {
        return this.current;
    }
}
```

### 继承与方法重写

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
        println(this.name + " says: 汪汪!");
    }
}
```

### 接口与多态

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
        println(this.name + " says: 汪汪!");
    }
}

public interface Flyable {
    void fly();
    void land();
}

public class Bird extends Animal implements Flyable {
    public Bird(String name) {
        super(name);
    }

    @Override
    public void speak() {
        println(this.name + " says: 啾啾!");
    }

    public void fly() {
        println(this.name + " is flying");
    }

    public void land() {
        println(this.name + " landed");
    }
}

public class Main {
    public static void main() {
        // 运行时多态（通过 vtable 分发）
        Animal a1 = new Dog("Buddy");
        a1.speak();    // 输出: Buddy says: 汪汪!
    }
}
```

### 构造函数与析构函数

```cay
public class Resource {
    int* data;

    public Resource() {
        data = new int[1024];
    }

    ~Resource() {
        delete[] data;
    }
}
```

---

## 控制流

### 条件

```cay
class Main {
    public static void main() {
        int x = 5;
        if (x > 0) {
            println("正数");
        } else if (x == 0) {
            println("零");
        } else {
            println("负数");
        }
    }
}
```

### 循环

```cay
class Main {
    public static void main() {
        // while 循环
        int i = 0;
        while (i < 5) {
            println(String.valueOf(i));
            i = i + 1;
        }

        // for 循环（C 风格）
        for (int j = 0; j < 5; j = j + 1) {
            println(String.valueOf(j));
        }

        // do-while 循环
        int k = 0;
        do {
            println(String.valueOf(k));
            k = k + 1;
        } while (k < 5);
    }
}
```

### switch 语句

```cay
class Main {
    public static void main() {
        int value = 2;
        switch (value) {
            case 1:
                println("one");
                break;
            case 2:
                println("two");
                break;
            default:
                println("other");
        }
    }
}
```

---

## 数组

```cay
class Main {
    public static void main() {
        // 声明并分配
        int[] arr = new int[10];
        arr[0] = 42;

        // 数组长度
        int len = arr.length();
        println(String.valueOf(len));

        // 字符串数组
        string[] names = new string[5];
        names[0] = "Alice";
        println(names[0]);
    }
}
```

---

## 字符串

内置字符串方法和运算符重载：

```cay
class Main {
    public static void main() {
        string s = "Hello, Cavvy!";
        int len = s.length();
        string upper = s.toUpperCase();
        string sub = s.substring(0, 5);
        bool has = s.contains("Cavvy");
        println(String.valueOf(len));
        println(upper);
    }
}
```

---

## Lambda 表达式

```cay
class Main {
    public static void main() {
        // Lambda 语法（已解析，闭包捕获环境变量尚未完整实现）
        var func = (int x, int y) -> x + y;
        int result = func(3, 4);
        println(String.valueOf(result));
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
        Box<int> box = new Box<int>(7);
        int val = box.get();
        println(String.valueOf(val));
    }
}
```

> **注意**：泛型语法已解析，但代码生成尚未实现单态化。

---

## Struct 与 Enum

```cay
struct Point {
    int x;
    int y;

    int sum() {
        return x + y;
    }
}

enum Status {
    Ready,
    Running,
    Done
}

class Main {
    static void main() {
        Point p = new Point();
        p.x = 2;
        p.y = 5;

        Status status = Status.Done;
        switch (status) {
            case Status.Done: println("完成"); break;
            default: println("等待"); break;
        }
    }
}
```

---

## 预处理器

完整的 C 风格预处理器：

```cay
#define MAX_SIZE 1024
#define SQUARE(x) ((x) * (x))

public class Main {
    public static void main() {
        int size = MAX_SIZE;
        println(String.valueOf(size));
    }
}
```

> 详情见[预处理器文档](preprocessor.md)。

---

## FFI 外部函数接口

直接调用 C 标准库：

```cay
#include "std/ffi.cay"

public class Main {
    public static void main() {
        printf("Hello from C! %d\n", 42);
    }
}
```

> 详情见 [FFI 文档](ffi.md)。

---

## 完整示例

综合展示语言主要特性：

```cay ignore
#include "math.cay"

interface Shape {
    double area();
    double perimeter();
}

class Circle implements Shape {
    double radius;

    Circle(double r) {
        radius = r;
    }

    double area() {
        return PI * radius * radius;
    }

    double perimeter() {
        return 2.0 * PI * radius;
    }
}

class Rectangle implements Shape {
    double width;
    double height;

    Rectangle(double w, double h) {
        width = w;
        height = h;
    }

    double area() {
        return width * height;
    }

    double perimeter() {
        return 2.0 * (width + height);
    }
}

class Main {
    static void printInfo(Shape s) {
        println("面积: " + s.area());
        println("周长: " + s.perimeter());
    }

    static void main() {
        Shape[] shapes = new Shape[2];
        shapes[0] = new Circle(5.0);
        shapes[1] = new Rectangle(3.0, 4.0);

        for (int i = 0; i < shapes.length(); i = i + 1) {
            printInfo(shapes[i]);
        }
    }
}
```
