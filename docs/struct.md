# struct — 值类型结构体

`struct` 是 Cavvy 的**值类型**声明关键字，用于创建栈分配、无继承的轻量级复合数据类型。

## 概述

与 `class`（引用类型，堆分配）不同，`struct` 实例在栈上创建，离开作用域自动释放，无需 GC 追踪。适合坐标系、配置项、RGB 颜色等轻量级数据聚合场景。

**版本：** v5.1.0 引入

## 语法

```ebnf
struct-decl = [modifiers] "struct" identifier "{" { struct-member } "}"

struct-member = field-decl | method-decl
```

```cay
// 完整语法
[modifiers] struct StructName {
    // 字段
    [modifiers] type fieldName [= initialValue];

    // 方法
    [modifiers] returnType methodName(params) { body }
}
```

## 基本示例

```cay
// 二维坐标点
public struct Point {
    public int x;
    public int y;

    public int getX() { return this.x; }
    public int getY() { return this.y; }
}

// 使用
Point p = new Point();
p.x = 10;
p.y = 20;
println("x = " + String.valueOf(p.x));        // 10
println("getX = " + String.valueOf(p.getX())); // 10
```

## 字段访问

使用 `.` 运算符读写字段。在 struct 方法内部访问字段需要显式使用 `this.` 前缀：

```cay
public struct Counter {
    public int count;

    public void increment() {
        this.count = this.count + 1;  // 必须 this.
    }
}
```

## 方法

struct 支持实例方法，`this` 指向当前实例指针：

```cay
public struct Rectangle {
    public int width;
    public int height;

    public int area() {
        return this.width * this.height;
    }

    public boolean isSquare() {
        return this.width == this.height;
    }
}
```

## 当前限制

| 不支持 | 说明 | 计划 |
|--------|------|------|
| 显式构造函数 | 自动零初始化 | 后续版本 |
| 析构函数 | 栈自动释放 | — |
| 静态成员 | 无 static 支持 | G2 阶段 |
| 继承 | 值类型无继承 | — |
| 接口实现 | — | 后续版本 |

## struct vs class

| 特性 | struct | class |
|------|--------|-------|
| 内存分配 | 栈 | 堆 |
| 继承 | 无 | 单继承 |
| 虚函数 | 无 | 默认虚函数 |
| 构造函数 | 暂不支持 | 支持 |
| 适用场景 | 轻量数据聚合 | OOP 抽象 |

## 内存布局

```
struct Point { int x; int y; }
// offset 0: x (i32, 4 bytes)
// offset 4: y (i32, 4 bytes)
// total: 8 bytes, alloca 栈分配
```

## 完整示例

```cay
public struct Color {
    public int r;
    public int g;
    public int b;

    public String toHex() {
        // 返回 #RRGGBB 格式
        return "#" + toHexComponent(this.r) + toHexComponent(this.g) + toHexComponent(this.b);
    }
}

public class Main {
    public static void main() {
        Color c = new Color();
        c.r = 255;
        c.g = 128;
        c.b = 0;
        println(c.toHex());
    }
}
```

## 参见

- [enum](./enum.md) — 标记联合体
- [泛型语法](./generics.md) — 类型参数
- [语法参考](./syntax-reference.md)
