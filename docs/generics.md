# 泛型语法

Cavvy 支持 `<T>` 语法声明泛型类型参数和类型实参，为 `Optional<T>`、`Result<T, E>` 及集合类等容器类型提供编译期类型安全。

## 概述

泛型允许编写与类型无关的代码，编译器通过**单态化（monomorphization）**为每个具体类型组合生成独立代码，达到与手写特化版本相同的性能。

**版本：** v5.1.0 引入语法支持  
**完整单态化代码生成：** 计划于 0.5.2.x

## 类型参数声明

在类、struct 或 enum 名称后使用 `<T1, T2, ...>` 声明类型参数：

```cay
// 泛型类
public class Box<T> {
    private T value;
}

// 泛型枚举
public enum Optional<T> {
    Some(T),
    None
}

// 多类型参数
public enum Result<T, E> {
    Ok(T),
    Err(E)
}

// 泛型 struct（计划中）
public struct Pair<T, U> {
    public T first;
    public U second;
}
```

**语法规则：**

- 类型参数必须是有效标识符（字母或下划线开头）
- 多个参数用逗号分隔
- `<` 和 `>` 使用尖括号，与比较运算符共享符号（上下文区分）
- 当前版本不支持类型边界（计划于后续版本支持 `T extends SomeClass`）

## 类型实参使用

使用 `TypeName<Arg1, Arg2>` 语法指定类型实参：

```cay
// 使用泛型类型
Optional<int> opt = Optional<int>.of(42);
Box<String> box = new Box<String>();

// 类型实参可以是：
Optional<int>              // 基本类型
Optional<String>           // 内置类型
Optional<Box<int>>         // 嵌套泛型
```

**类型实参的合法值：**

- 基本类型：`int`、`long`、`float`、`double`、`boolean`、`char`
- 引用类型：`String`、类名、数组
- 泛型类型：`Optional<int>`、`Box<String>`

## 单态化原理

```
源码:                     单态化后:
class Box<T> { ... }      

Box<int> 使用 →           生成 __Box_i32  (T=i32 版本)
Box<String> 使用 →        生成 __Box_String (T=String 版本)
```

每个实例化点生成一个独立的特化版本，所有类型信息在编译时确定：

1. **解析阶段** — AST 中记录 `type_params = ["T"]`
2. **收集阶段** — 扫描全部 `Type<Arg>` 使用点，收集所有 `(T=具体类型)` 组合
3. **生成阶段** — 为每个组合复制 AST 并替换类型参数，生成特化 IR
4. **命名规约** — `Box<int>` → `__Box_i32`，避免链接符号冲突

## 与预定义特化

在泛型单态化完全实现前，标准库提供预定义特化版本作为过渡：

```cay
// 当前可用（预定义特化）
OptionalInt    → Optional<int>
OptionalBool   → Optional<boolean>
OptionalDouble → Optional<double>

// 未来将自动生成（单态化）
Optional<long>     → 自动生成 __Optional_i64
Optional<String>   → 自动生成 __Optional_String
```

## 语法示例集合

### 声明端

```cay
// 单参数
class Container<T> { ... }
enum Option<T> { ... }

// 多参数
class Dictionary<K, V> { ... }
enum Result<T, E> { ... }

// 无参数（兼容普通声明）
class Regular { ... }
```

### 使用端

```cay
// 变量声明
Optional<int> maybe = Optional<int>.of(42);

// new 表达式
Box<String> b = new Box<String>();

// 静态方法调用
Result<int, String> r = Result<int, String>.Ok(200);

// 嵌套
Optional<Box<int>> nested;
```

## 当前限制

| 限制 | 说明 | 计划 |
|------|------|------|
| 类型边界 `T extends X` | 暂不支持 | G1 阶段 |
| 泛型方法 `<T> void foo(T x)` | 暂不支持 | 0.5.2.x |
| 协变/逆变 | 暂不支持 | G1 阶段 |
| 多约束 `where T: A + B` | 暂不支持 | G1 阶段 |

## 参见

- [struct](./struct.md) — 值类型结构体
- [enum](./enum.md) — 标记联合体
- [@FreeFunction](./freefunction.md) — 透传顶层函数注解
- [Optional<T>](./stdlib/optional.md) — 泛型容器示例
- [语法参考](./syntax-reference.md)
