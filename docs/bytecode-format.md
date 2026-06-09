# CayBC 字节码格式

Cavvy 字节码格式（CayBC）是一种专为 Cavvy 语言设计的二进制中间表示格式。它支持 JIT 和 AOT 编译，并提供混淆能力。

---

## 概述

CayBC 字节码系统位于 `src/bytecode/`，由 7 个模块组成：

| 模块 | 职责 |
|---|---|
| `mod.rs` | `BytecodeModule` 顶层结构 |
| `constant_pool.rs` | JVM 风格的常量池 |
| `instructions.rs` | 100+ 指令操作码 |
| `jit.rs` | JIT/AOT 编译器 |
| `linker.rs` | 自动链接 |
| `serializer.rs` | 二进制序列化 |
| `obfuscator.rs` | 字节码混淆 |

---

## BytecodeModule（`mod.rs`）

`BytecodeModule` 是 CayBC 的顶层容器，包含：

- **常量池**（`ConstantPool`） — 所有常量的索引表
- **类定义** — 类的字段、方法、接口实现
- **方法体** — 字节码指令序列
- **元数据** — 版本号、源文件名、调试信息

---

## 常量池（`constant_pool.rs`）

JVM 风格的常量池，支持以下常量类型：

| 常量类型 | 描述 |
|---|---|
| `Utf8` | UTF-8 编码字符串 |
| `Integer` | 32 位整数常量 |
| `Long` | 64 位整数常量 |
| `Float` | 32 位浮点常量 |
| `Double` | 64 位浮点常量 |
| `String` | 字符串常量（引用 Utf8） |
| `Class` | 类引用 |
| `FieldRef` | 字段引用 |
| `MethodRef` | 方法引用 |
| `InterfaceMethodRef` | 接口方法引用 |
| `NameAndType` | 名称和类型描述符对 |
| `MethodHandle` | 方法句柄 |
| `MethodType` | 方法类型描述符 |
| `InvokeDynamic` | 动态调用点 |

常量池使用整数索引访问（从 1 开始，类似 JVM 规范）。

---

## 指令集（`instructions.rs`）

CayBC 定义 100+ 操作码（`Opcode` 枚举），按功能分类：

### 加载和存储

| 指令 | 描述 |
|---|---|
| `Load` | 从局部变量加载到栈 |
| `Store` | 从栈存储到局部变量 |
| `LoadConst` | 加载常量 |
| `LoadField` | 加载实例字段 |
| `StoreField` | 存储实例字段 |
| `LoadStatic` | 加载静态字段 |
| `StoreStatic` | 存储静态字段 |
| `LoadArray` | 从数组加载 |
| `StoreArray` | 存储到数组 |

### 算术运算

| 指令 | 描述 |
|---|---|
| `Add`, `Sub`, `Mul`, `Div`, `Rem` | 基本算术 |
| `Neg` | 取负 |
| `Shl`, `Shr`, `UShr` | 移位 |
| `And`, `Or`, `Xor` | 位运算 |
| `Inc` | 局部变量自增 |

### 类型转换

`I2L`, `L2I`, `F2D`, `D2F`, `I2B`, `I2C`, `I2S` 等

### 对象操作

| 指令 | 描述 |
|---|---|
| `New` | 创建新对象 |
| `NewArray` | 创建数组 |
| `ArrayLength` | 获取数组长度 |
| `InstanceOf` | 类型检查 |
| `CheckCast` | 类型强制转换 |

### 栈操作

| 指令 | 描述 |
|---|---|
| `Pop` | 弹出栈顶 |
| `Dup` | 复制栈顶 |
| `Swap` | 交换栈顶两个元素 |

### 控制流

| 指令 | 描述 |
|---|---|
| `Goto` | 无条件跳转 |
| `IfEq`, `IfNe`, `IfLt`, `IfGe`, `IfGt`, `IfLe` | 条件跳转 |
| `TableSwitch` | 表跳转（switch） |
| `LookupSwitch` | 查找跳转 |

### 方法调用

| 指令 | 描述 |
|---|---|
| `InvokeVirtual` | 虚方法调用（vtable） |
| `InvokeStatic` | 静态方法调用 |
| `InvokeSpecial` | 特殊方法调用（构造函数、父类） |
| `InvokeInterface` | 接口方法调用 |
| `InvokeDynamic` | 动态方法调用 |

### 返回

`Return`, `IReturn`, `LReturn`, `FReturn`, `DReturn`, `AReturn`

---

## JIT/AOT 编译（`jit.rs`）

`JitOptions` 结构体控制编译行为：

```rust
struct JitOptions {
    optimization_level: u8,    // 0-3
    dump_ir: bool,             // 是否输出 IR
    dump_asm: bool,            // 是否输出汇编
    verbose: bool,
}
```

`jit_to_exe()` 函数将 `BytecodeModule` 编译为可执行文件。

---

## 链接器（`linker.rs`）

`LinkerConfig` 支持自动链接检测：

- 自动检测依赖的本地库
- 根据目标平台选择正确的链接器
- 支持静态链接和动态链接

---

## 序列化格式（`serializer.rs`）

### 二进制文件结构

```
[魔数]        CAY\x01        (4 字节)
[版本号]      主版本:u16 + 次版本:u16  (4 字节)
[常量池]      ConstantPool 序列化
[类定义]      类、字段、方法定义
[字节码]      方法指令序列
[元数据]      调试信息等
[校验和]      CRC32          (4 字节)
```

### 魔数

所有 CayBC 文件以 `0xCA 0x59 0x01`（ASCII "CAY" + 版本 1）开头。

---

## 混淆器（`obfuscator.rs`）

`BytecodeObfuscator` 提供四种混淆技术：

| 技术 | 描述 | 可逆性 |
|---|---|---|
| 名称混淆 | 将标识符重命名为无意义名称 | 否 |
| 控制流混淆 | 插入冗余跳转和无关代码块 | 否 |
| 垃圾代码插入 | 插入不执行的无意义指令 | 否 |
| 字符串加密 | 运行时解密字符串字面量 | 运行时透明 |

### 混淆级别

| 级别 | 描述 |
|---|---|
| 0 | 无混淆 |
| 1 | 名称混淆 |
| 2 | 名称 + 控制流混淆 |
| 3 | 全部（名称 + 控制流 + 垃圾代码 + 字符串加密） |

---

## 使用方法

```bash
# 生成字节码
cay-bcgen input.cay -o output.caybc

# 带混淆生成
cay-bcgen input.cay --obfuscate --obfuscation-level 2 -o obfuscated.caybc

# 查看字节码信息
cay-bcgen input.cay --verbose
```

---

## 与 JVM 字节码的对比

| 特性 | CayBC | JVM |
|---|---|---|
| 常量池 | ✅ JVM 风格 | 标准 |
| 指令集 | 100+ 操作码 | 200+ 操作码 |
| 栈机模型 | ✅ 是 | 是 |
| 类型信息 | 强类型 | 类型描述符 |
| 混淆 | ✅ 内置 | 需外部工具 |
| 序列化 | ✅ 自定义格式 | .class 格式 |
| JIT | ✅ 基础实现 | 成熟 |