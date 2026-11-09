# `caylibs/__builtins__` —— Cavvy 内建类型与内建函数的唯一事实来源

本目录是 Cavvy 编译器**内建（built-in）类型、方法与全局函数**的机器可读唯一事实来源
（single source of truth）。它面向外部编辑器分析器、语言服务器（LSP）、文档生成器等
消费者，而不是面向最终用户程序。

## 定位与约定

- 本目录中的 `.cay` 文件**不会**被编译进用户程序，也不参与标准库链接；它们只是
  “签名存根（stub）”，用来描述编译器在语义层已经内置的能力。
- 外部分析器 / LSP **不应**在代码里硬编码内建名称与签名，而应读取本目录：先读
  `index.json`，再按映射打开对应的 `.cay` 文件。
- Cavvy 内建发生变化（新增方法、修改签名、调整返回类型、增删全局函数）时，
  **只需修改本目录**，所有消费者即可自动获得更新，无需跟随编译器版本改代码。
- 所有文件统一使用 **UTF-8（无 BOM）** 编码，保证跨平台工具稳定读取。
- 存根文件只做**声明**，不包含实现；方法一律使用 `native` 修饰并以 `;` 结束，
  字段则直接声明（如 `public int length;`）。

## 映射规则

1. **文件名即规范内建名**：文件名词干（stem）就是内建类型在语言中的拼写，
   例如 `String.cay` 对应内建类型 `String`。
2. **关键字类名使用 `Cay` 前缀**：`int`、`long`、`float`、`double`、`bool`、
   `char`、`string`/`String` 是词法关键字，不能直接作为类名。因此这些内建类型的
   存根类名统一加 `Cay` 前缀（`CayInt`、`CayLong`、……），由 `index.json` 把
   `builtin` 拼写映射到 `stub` 类名。
3. **`String` 的存根符号是 `CayString`**：`String.cay` 中声明的类为 `CayString`，
   `index.json` 中记录 `"builtin": "String", "stub": "CayString"`。
4. **数组**：语言用 `T[]` 表示数组；`array.cay` 定义泛型类 `array<T>`，并提供
   `int length` 字段。分析器应把 `T[]` 归一化为 `array<T>`，并据此提供
   `.length` 字段补全与类型检查。
5. **全局内建函数**：Cavvy 没有顶层自由函数。`functions.cay` 中 `Builtins` 类的
   `public static native` 方法就是内建全局函数；`index.json` 的
   `"functions": { "stub": "Builtins", "file": "functions.cay" }` 表示
   `print(...)` 等调用应解析为 `Builtins.print(...)`。
6. **基本类型**：`int`、`long`、`float`、`double`、`bool`、`char` 等由
   `index.json` 的 `types` 数组映射到各自的 `Cay*` 存根类。
7. **跨文件引用**：存根可以互相引用（例如 `CayInt` 的 `toString()` 返回
   `CayString`），即使 `CayString` 并不声明在同一个文件中。分析器应把本目录
   视为**一个虚拟库**整体加载；单个文件必须能独立通过语法解析。

## `index.json` schema

```json
{
  "version": 1,
  "types": [
    { "builtin": "String", "stub": "CayString", "file": "String.cay" },
    { "builtin": "array",  "stub": "array",     "file": "array.cay",
      "note": "T[] is equivalent to array<T>" }
  ],
  "functions": { "stub": "Builtins", "file": "functions.cay" }
}
```

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `version` | `number` | schema 版本号，当前为 `1`。不兼容变更时递增。 |
| `types` | `array` | 内建类型映射表。 |
| `types[].builtin` | `string` | 语言中书写的内建类型拼写（如 `int`、`String`、`array`）。 |
| `types[].stub` | `string` | 存根文件中实际声明的类名（如 `CayInt`、`CayString`）。 |
| `types[].file` | `string` | 相对本目录的存根文件名。 |
| `types[].note` | `string?` | 可选备注，仅用于说明特殊映射（如 `T[]` 与 `array<T>`）。 |
| `functions.stub` | `string` | 承载全局内建函数的类名，固定为 `Builtins`。 |
| `functions.file` | `string` | 该类的存根文件名，固定为 `functions.cay`。 |

## 如何新增一个内建

1. **新增类型**：在本目录创建 `<Name>.cay`。如果 `<Name>` 是词法关键字，类名改用
   `Cay<Name>`；否则可直接使用原名（如 `Object`、`Integer` 已使用原名）。
2. **在 `index.json` 的 `types` 中登记**：填写 `builtin`、`stub`、`file`；如有特殊
   映射语义，补充 `note`。
3. **新增全局函数**：在 `functions.cay` 的 `Builtins` 类中添加
   `public static native` 声明（可变参数写作 `Type... name`），并在方法上写明中文文档注释。
4. **无需改动其他位置**：不要修改编译器源码或硬编码列表；本目录就是唯一入口。
5. **验证**：对每个新增/修改的 `.cay` 文件运行
   `.\target\release\cay-ast.exe <file>`，确认无解析错误且成员输出符合预期；
   再用 `Get-Content index.json | ConvertFrom-Json` 校验 JSON 合法。
6. 如新增了新的映射规则（例如新的前缀约定），同步更新本 README。

## 示例：`T[]` 到 `array<T>`

```cay
int[] v = new int[10];
int n = v.length;   // 分析器将 int[] 归一化为 array<int>，因此 length 字段可用
```

分析器处理过程：

1. 读到类型 `int[]`；
2. 按规则归一化为 `array<int>`（对应 `array.cay` 中的 `array<T>`）；
3. 在 `array<T>` 上查找成员 `length`，命中 `public int length;`；
4. 因此 `v.length` 的类型为 `int`，可用于补全、跳转与类型检查。

## 文件一览

| 文件 | 内容 |
| --- | --- |
| `index.json` | 内建名到存根符号的机器可读映射。 |
| `array.cay` | `array<T>`，对应语言中的 `T[]`，含 `int length` 字段。 |
| `String.cay` | `CayString`，内建 `String` 的存根。 |
| `Object.cay` | `Object`，所有引用类型的根类。 |
| `Integer.cay` | `CayInteger`，内建 `Integer` 的静态工具方法。 |
| `int.cay` / `long.cay` / `float.cay` / `double.cay` / `bool.cay` / `char.cay` | 基本类型存根（`CayInt` 等）。 |
| `functions.cay` | `Builtins`，内建全局函数。 |
