# enum — 标记联合体

`enum` 是 Cavvy 的**标记联合体（Tagged Union / ADT）**声明，用于定义一组带名称、可携带数据的变体（variant）。类似 Rust 的 `enum`、Swift 的 `enum`、Haskell 的 ADT。

## 概述

与 C 语言的不安全 `union` 不同，Cavvy 的 enum 是类型安全的：运行时通过 tag 字段追踪当前激活的 variant，编译器保证不会错误地按另一种 variant 解释数据。

**版本：** v5.1.0 引入（声明语法）  
**模式匹配（match）：** 计划于 G1 阶段

## 语法

```ebnf
enum-decl = [modifiers] "enum" identifier [type-params] "{" variant { "," variant } [","] "}"

variant   = identifier [ "(" type { "," type } ")" ]
```

```cay
// 完整语法
[modifiers] enum EnumName<T1, T2, ...> {
    VariantName1(ContainedType, ...),
    VariantName2,
    ...
}
```

## 简单枚举（无 payload）

变体不携带数据，类似 C 风格的命名常量：

```cay
public enum Color {
    Red,
    Green,
    Blue
}

public enum HttpStatus {
    OK,
    NotFound,
    InternalError
}
```

## 带 Payload 的枚举

变体携带数据，形成 tagged union：

```cay
// 结果类型：成功携带 int，失败携带错误消息
public enum Result {
    Ok(int),
    Err(String)
}

// 形状类型：不同形状携带不同数据
public enum Shape {
    Circle(double),              // 半径
    Rectangle(double, double),    // 宽、高
    Point(double, double, double) // x, y, z
}
```

## 泛型 enum

enum 支持泛型类型参数，这是实现通用容器类型的核心：

```cay
// 标准 Optional 模式
public enum Option<T> {
    Some(T),
    None
}

// 标准 Result 模式
public enum Result<T, E> {
    Ok(T),
    Err(E)
}

// 多类型参数
public enum Either<L, R> {
    Left(L),
    Right(R)
}
```

## 实例化（计划中）

```cay
// 模式匹配语法（计划于 G1 阶段）
Option<int> opt = Option<int>.Some(42);

match opt {
    Some(value) => println("got: " + String.valueOf(value)),
    None => println("empty"),
}
```

## 运行时布局

```
enum Result<T, E> {
    Ok(T),
    Err(E)
}

┌─────────┬──────────────────────┐
│  tag:    │  payload (union):     │
│  i32     │  max(sizeof(T),       │
│          │       sizeof(E))      │
└─────────┴──────────────────────┘
```

- `tag = 0` → Ok
- `tag = 1` → Err
- payload 大小 = max(包含类型的尺寸)

## 当前状态

| 特性 | 状态 |
|------|------|
| enum 声明语法 | ✓ v5.1.0 |
| 带 payload variant | ✓ v5.1.0 |
| 泛型类型参数 | ✓ v5.1.0 |
| 命名空间支持 | ✓ v5.1.0 |
| 实例化 / 构造 | 计划中 |
| 模式匹配 | G1 阶段 |
| exhaustiveness 检查 | G1 阶段 |

## 参见

- [struct](./struct.md) — 值类型结构体
- [泛型语法](./generics.md) — 类型参数
- [Optional<T>](./stdlib/optional.md) — 基于 enum 的容器实现
- [语法参考](./syntax-reference.md)
