# Optional<T> — 零开销可选值容器

`Optional<T>` 是 Cavvy 标准库提供的**零开销可选值容器**，用于显式处理可能缺失的值，取代传统的 `null` 引用。

## 概述

`Optional<T>` 基于 **tagged union** 模式实现：内部包含一个 `hasValue` 标记和一个 `value` 字段。无堆分配，无 RTTI，编译器通过单态化为每个具体类型生成特化代码，达到与手写空值检查相同的性能。

**设计参考：** Java `Optional`、Rust `Option<T>`、Swift `Optional`

**实现文件：** `caylibs/Optional.cay`

**版本：** 0.5.1.x

---

## 快速开始

```cay
#include <Optional.cay>

using std::OptionalInt;

public class Main {
    public static void main() {
        // 创建包含值的 Optional
        OptionalInt some = OptionalInt.of(42);

        // 创建空的 Optional
        OptionalInt none = OptionalInt.empty();

        // 安全访问
        if (some.isPresent()) {
            println("value: " + String.valueOf(some.get()));
        }

        // 默认值回退
        int safe = none.orElse(100);  // → 100
    }
}
```

---

## 预定义特化类型

在当前泛型单态化尚未完全实现的阶段，标准库提供了以下预定义特化类型：

| 类型 | 包含类型 | 说明 |
|------|---------|------|
| `OptionalInt` | `int` | int 类型可选值 |
| `OptionalBool` | `boolean` | boolean 类型可选值 |
| `OptionalDouble` | `double` | double 类型可选值 |

> 完整的 `Optional<T>` 泛型版本将在 0.5.2.x 泛型系统完成后自动由单态化生成。

---

## API 参考

### OptionalInt

包含 `int` 类型值的可选容器。

#### 构造方法

##### `of`

```cay
public static OptionalInt of(int value)
```

创建一个包含指定值的 Optional。

| 参数 | 类型 | 说明 |
|------|------|------|
| `value` | `int` | 要包装的值 |

| 返回值 | 说明 |
|--------|------|
| `OptionalInt` | 包含值的 Optional 实例 |

```cay
OptionalInt opt = OptionalInt.of(42);
```

---

##### `empty`

```cay
public static OptionalInt empty()
```

创建一个空的 Optional。

| 返回值 | 说明 |
|--------|------|
| `OptionalInt` | 空的 Optional 实例 |

```cay
OptionalInt opt = OptionalInt.empty();
```

---

#### 检查方法

##### `isPresent`

```cay
public boolean isPresent()
```

检查是否包含值。

| 返回值 | 说明 |
|--------|------|
| `boolean` | 包含值返回 `true`，否则 `false` |

```cay
if (maybeValue.isPresent()) {
    // 安全使用 maybeValue.get()
}
```

---

##### `isEmpty`

```cay
public boolean isEmpty()
```

检查是否为空。

| 返回值 | 说明 |
|--------|------|
| `boolean` | 为空返回 `true`，否则 `false` |

```cay
if (maybeValue.isEmpty()) {
    println("no value present");
}
```

---

#### 取值方法

##### `get`

```cay
public int get()
```

获取包含的值。**调用前必须先检查 `isPresent()`**。

| 返回值 | 说明 |
|--------|------|
| `int` | 包含的值 |

**注意：** 在空 Optional 上调用 `get()` 会导致未定义行为（release 模式下无运行时检查，以保持零开销）。

```cay
// ✓ 安全用法
if (opt.isPresent()) {
    int value = opt.get();
}

// ✗ 不安全用法
int value = opt.get();  // 如果 opt 为空，行为未定义
```

---

##### `orElse`

```cay
public int orElse(int defaultValue)
```

如果包含值则返回，否则返回提供的默认值。

| 参数 | 类型 | 说明 |
|------|------|------|
| `defaultValue` | `int` | 空时返回的默认值 |

| 返回值 | 说明 |
|--------|------|
| `int` | 包含的值或默认值 |

```cay
int result = maybeValue.orElse(0);        // 安全：永远不会失败
int config = getConfig().orElse(8080);     // 带默认值的配置读取
```

---

### OptionalBool

包含 `boolean` 类型值的可选容器。

#### 构造方法

| 方法 | 签名 | 说明 |
|------|------|------|
| `of` | `public static OptionalBool of(boolean value)` | 创建包含值的 Optional |
| `empty` | `public static OptionalBool empty()` | 创建空的 Optional |

#### 检查方法

| 方法 | 签名 | 说明 |
|------|------|------|
| `isPresent` | `public boolean isPresent()` | 检查是否包含值 |
| `isEmpty` | `public boolean isEmpty()` | 检查是否为空 |

#### 取值方法

| 方法 | 签名 | 说明 |
|------|------|------|
| `get` | `public boolean get()` | 获取值（需先检查） |
| `orElse` | `public boolean orElse(boolean defaultValue)` | 安全取值，空时返回默认值 |

```cay
OptionalBool flag = OptionalBool.of(true);
boolean value = flag.orElse(false);  // → true
```

---

### OptionalDouble

包含 `double` 类型值的可选容器。

#### 构造方法

| 方法 | 签名 | 说明 |
|------|------|------|
| `of` | `public static OptionalDouble of(double value)` | 创建包含值的 Optional |
| `empty` | `public static OptionalDouble empty()` | 创建空的 Optional |

#### 检查方法

| 方法 | 签名 | 说明 |
|------|------|------|
| `isPresent` | `public boolean isPresent()` | 检查是否包含值 |
| `isEmpty` | `public boolean isEmpty()` | 检查是否为空 |

#### 取值方法

| 方法 | 签名 | 说明 |
|------|------|------|
| `get` | `public double get()` | 获取值（需先检查） |
| `orElse` | `public double orElse(double defaultValue)` | 安全取值，空时返回默认值 |

```cay
OptionalDouble pi = OptionalDouble.of(3.14159);
double value = pi.orElse(0.0);  // → 3.14159
```

---

## 使用模式

### 模式 1：安全访问

```cay
OptionalInt result = findById(42);

if (result.isPresent()) {
    int value = result.get();
    process(value);
} else {
    println("not found");
}
```

### 模式 2：默认值

```cay
// 无需 if 检查，始终安全
int timeout = getTimeoutConfig().orElse(5000);
int port = getPortConfig().orElse(8080);
```

### 模式 3：空值传播

```cay
// 链式处理（完整泛型版本将支持 map/flatMap）
OptionalInt userId = getCurrentUserId();
if (userId.isPresent()) {
    OptionalInt profile = findProfile(userId.get());
    if (profile.isPresent()) {
        int data = profile.get();
        // ...
    }
}
```

### 模式 4：替代 null 返回值

```cay
// ✗ 旧风格：返回 -1 表示"未找到"
public static int findUserId(String name) {
    // ...
    return -1;  // magic number
}

// ✓ 新风格：显式表达"可能没有结果"
public static OptionalInt findUserId(String name) {
    if (/* found */) {
        return OptionalInt.of(userId);
    } else {
        return OptionalInt.empty();
    }
}
```

---

## 内存布局

```
OptionalInt:
┌──────────┬──────────┐
│ hasValue │  value   │
│ (i1)     │  (i32)   │
│ 1 byte   │  4 bytes │
└──────────┴──────────┘
总大小：8 字节（对齐后）
```

- **零堆分配**：数据直接存储在 Optional 对象内部
- **无虚函数表**：所有方法调用均为静态分派
- **与手写等价**：`if (opt.isPresent()) { x = opt.get(); }` 编译后等价于 `if (hasValue) { x = value; }`

---

## 未来扩展（计划中）

完整的 `Optional<T>` API 将在泛型系统完成后支持以下方法：

| 方法 | 签名 | 说明 |
|------|------|------|
| `orElseGet` | `T orElseGet(fn() -> T supplier)` | 延迟计算默认值 |
| `map` | `Optional<U> map(fn(T) -> U mapper)` | 值转换 |
| `flatMap` | `Optional<U> flatMap(fn(T) -> Optional<U> mapper)` | 扁平化转换 |
| `filter` | `Optional<T> filter(fn(T) -> boolean predicate)` | 条件过滤 |
| `ifPresent` | `void ifPresent(fn(T) -> void consumer)` | 值存在时执行回调 |

---

## 与 Result<T, E> 的关系

`Optional<T>` 和 `Result<T, E>` 是 Cavvy 错误处理体系的两个基石：

| | Optional<T> | Result<T, E> |
|---|---|---|
| 用途 | 可能缺失的值 | 可能失败的操作 |
| 空/错误含义 | "没有值" | "操作失败，原因：E" |
| 典型场景 | 查找、配置读取 | I/O、解析、网络 |
| 空值替代 | `null` | 异常/错误码 |

```cay
// Optional 用于"可能没有"
OptionalInt user = findUser("alice");

// Result 用于"可能失败"（计划中）
Result<File, IOError> file = File.open("data.txt");
```

---

## 参见

- [新特性文档](../new-features.md) — struct、enum、@FreeFunction、泛型语法
- [ROADMAP](../../ROADMAP.md) — Optional<T> 设计草案与泛型实现计划
- [标准库索引](./index.md) — 所有标准库模块
