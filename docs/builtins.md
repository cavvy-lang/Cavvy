# 内置类型与内置函数声明约定（`caylibs/__builtins__`）

## 为什么需要这份约定

Cavvy 有一部分能力由编译器/运行时**内建**，而不是由普通标准库源码实现，例如：

- `String` 的方法（`length()`、`substring()`、`indexOf()`……）
- 数组 `T[]` 的 `.length` 字段 / `.length()` 语法糖
- 基本类型（`int`、`long`、`float`、`double`、`bool`、`char`）的 `toString()`
- `Object.hashCode()` / `Object.equals()`
- `print`、`println`、`readInt`、`exit`、`panic` 等全局函数

如果编辑器分析器把这些名称和签名硬编码在插件里，那么编译器一更新（新增方法、修改签名、
调整返回类型），插件就会失效。因此项目约定：

> **`caylibs/__builtins__/` 是 Cavvy 内置类型与内置函数的唯一机器可读事实来源。**

语言服务器（`cay-lsp`）、外部编辑器分析器、文档生成器都应读取这里，而不是自己维护列表。

## 目录结构

```
caylibs/__builtins__/
├── README.md        # 本约定的详细版（维护者向）
├── index.json       # 内建名 -> 存根类/文件 的机器可读映射
├── array.cay        # array<T>，对应语言的 T[]
├── String.cay       # CayString，对应 String
├── Object.cay       # Object
├── Integer.cay      # CayInteger，对应 Integer 的静态方法
├── int.cay          # CayInt 等基本类型存根
├── long.cay
├── float.cay
├── double.cay
├── bool.cay
├── char.cay
└── functions.cay    # Builtins：全局内建函数
```

这些 `.cay` 文件**不会参与用户程序编译**，也不链接进最终产物；它们只是签名存根
（stub）。每个文件都能被普通 Cavvy 解析器独立解析通过。

## 映射规则

1. **文件名即规范内建名**：`String.cay` 对应 `String`，`array.cay` 对应数组 `T[]`。
2. **关键字类名加 `Cay` 前缀**：`String`、`int` 等是词法关键字，不能直接作类名。
   因此存根类名写作 `CayString`、`CayInt`，再由 `index.json` 映射。
3. **数组**：`T[]` 等价于 `array<T>`。`array.cay` 声明 `public final class array<T>`，
   其中 `public int length;` 就是 `v.length` 的依据。
4. **全局函数**：Cavvy 没有普通顶层自由函数；`functions.cay` 的 `Builtins` 类中
   `public static native` 方法就是 `print` / `println` / `readInt` 等全局内建函数。
5. **方法无方法体**：存根方法一律 `native`，以 `;` 结束；字段直接声明。
6. **整体虚拟库**：存根之间可以互相引用（如 `CayInt.toString()` 返回 `CayString`），
   分析器应把整个目录当成一个虚拟库加载。

## `index.json`

```json
{
  "version": 1,
  "types": [
    { "builtin": "String", "stub": "CayString", "file": "String.cay" },
    { "builtin": "int", "stub": "CayInt", "file": "int.cay" },
    { "builtin": "array", "stub": "array", "file": "array.cay",
      "note": "T[] is equivalent to array<T>" }
  ],
  "functions": { "stub": "Builtins", "file": "functions.cay" }
}
```

| 字段 | 说明 |
|---|---|
| `version` | schema 版本；不兼容变更时递增。 |
| `types[].builtin` | 语言中书写的内建类型名（`String`、`int`、`array`……）。 |
| `types[].stub` | 存根文件中实际声明的类名（`CayString`、`CayInt`……）。 |
| `types[].file` | 相对 `__builtins__/` 的文件名。 |
| `types[].note` | 可选说明（如数组映射）。 |
| `functions.stub` | 承载全局函数的类名，固定 `Builtins`。 |
| `functions.file` | 全局函数存根文件。 |

## 分析器如何消费

以 `int[] v` 为例：

1. 读到类型 `int[]`；
2. 按约定归一化为 `array<int>`；
3. 在 `array.cay` 的 `array<T>` 上查找成员 `length`；
4. 命中 `public int length;`，因此 `v.length` 的类型是 `int`，可用于补全、悬停、
   跳转和类型检查。

对于 `String`，分析器把 `String` 归一化为 `CayString` 存根，从而知道
`s.substring(1)` 返回 `String`、`s.length()` 返回 `int`。

## 如何新增/修改内建

1. 修改或新增 `caylibs/__builtins__/*.cay` 中的声明；
2. 如果是新类型，在 `index.json` 的 `types` 中登记；
3. 如果是新全局函数，在 `functions.cay` 的 `Builtins` 中加 `public static native`；
4. 不需要修改 LSP 或编辑器插件的硬编码列表；
5. 用 `cay-ast` 验证每个存根文件可独立解析，并校验 `index.json` 是合法 JSON。

## 与 `cay-lsp` 的关系

`cay-lsp` 在初始化时自动查找工作区、当前目录、可执行文件目录以及
`CARGO_MANIFEST_DIR/caylibs/__builtins__`，加载 `index.json` 与存根，并把它们用于：

- `.` 成员补全（如 `v.length`、`s.substring`）
- 悬停文档与签名
- 跳转到内建声明（打开 `array.cay` / `String.cay`）
- 签名帮助
- 快速修复的候选成员

如果找不到存根目录，LSP 会退回到一组最小的内置定义（仅保证基本可用），并在日志中提示。
