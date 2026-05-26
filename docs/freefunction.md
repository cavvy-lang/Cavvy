# @FreeFunction — 透传顶层函数注解

`@FreeFunction` 是一个方法级注解，将类中的**静态方法**导出为可直接通过函数名调用的自由函数，无需类名前缀。

## 概述

正常情况下，调用类的静态方法需要写 `ClassName.methodName()`。`@FreeFunction` 让被标记的方法可以直接通过函数名调用，类似于 Swift 的全局函数或 C++ 的 `using` 声明。

**版本：** v5.1.0 引入

## 基本用法

```cay
public class Calculator {
    @FreeFunction
    public static int add(int a, int b) {
        return a + b;
    }

    @FreeFunction
    public static int multiply(int a, int b) {
        return a * b;
    }
}

// 直接调用，无需 Calculator. 前缀
int sum = add(10, 20);           // → 30
int product = multiply(6, 7);    // → 42
```

## 注解约束

- 只能用于 `public static` 方法
- 方法必须定义在类内部（不能是顶层函数）
- 每个导出函数名在全局范围内必须唯一

## 命名空间内使用

如果 `@FreeFunction` 方法所在类位于命名空间内，调用时必须加命名空间前缀：

```cay
namespace math {
    public class Operations {
        @FreeFunction
        public static int square(int x) {
            return x * x;
        }
    }
}

// 调用时必须带命名空间前缀
int result = math::square(5);    // ✓ 正确：25
// int wrong = square(5);        // ✗ 错误：找不到 square
```

## 冲突检测

当两个**不同类**中的方法都标记了 `@FreeFunction` 且函数名相同时，编译器报告重复定义错误：

```cay
// ✗ 编译错误
public class Foo {
    @FreeFunction
    public static int greet() { return 1; }
}

public class Bar {
    @FreeFunction
    public static int greet() { return 2; }
}
```

**错误信息示例：**

```
重复定义 'greet'
@FreeFunction 函数 'greet' 已在类 'Foo' (行 3:18) 中定义，
类 'Bar' 中的同名 @FreeFunction 方法冲突。请使用不同的函数名。
→ 第一个定义位于: examples/conflict.cay:3:18
```

### 源映射

冲突错误报告包含**源映射信息**，即使方法定义在 `#include` 引入的文件中，也能准确指向原始位置的行号。

## 内部实现

`@FreeFunction` 在编译期转换：

```
调用: add(10, 20)
  → 解析: free_functions["add"] → 类 Calculator::add
  → IR:   call i32 @Calculator.add(i32 10, i32 20)
```

命名空间限定名同时注册简化名和限定名：
```
math::square → 注册: free_functions["square"] + free_functions["math::square"]
```

## 使用建议

| 场景 | 推荐 |
|------|------|
| 工具函数（如数学计算） | ✓ 使用 @FreeFunction |
| 有状态的类方法 | ✗ 保持 ClassName.method() |
| 可能冲突的命名 | ✗ 避免 @FreeFunction |
| 命名空间内的函数 | ✓ 调用时带前缀 |

## 参见

- [struct](./struct.md) — 值类型结构体
- [enum](./enum.md) — 标记联合体
- [泛型语法](./generics.md) — 类型参数
- [语法参考](./syntax-reference.md)
