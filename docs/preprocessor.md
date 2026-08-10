# 预处理器指南

Cavvy 在词法分析前运行完整的 C 风格预处理器，并保留源映射（source map）用于诊断错误定位。

---

## 支持的指令

| 指令       | 语法                        | 说明                   |
| ---------- | --------------------------- | ---------------------- |
| 文件包含   | `#include "file"`         | 从当前文件所在目录搜索 |
| 系统包含   | `#include <file>`         | 从系统包含路径搜索     |
| C 头包含   | `#include_c "header.h"`   | 导入 C/C++ 头文件的 FFI 声明（见 ffi.md） |
| Cay 声明文件 | `#include_h "header.cayh"` | 导入 Cavvy 声明文件（支持 enum 等 ADT） |
| 常量宏     | `#define NAME value`      | 定义常量宏             |
| 条件定义   | `#ifdef NAME`             | 如果宏已定义           |
| 条件未定义 | `#ifndef NAME`            | 如果宏未定义           |
| 条件表达式 | `#if expr`                | 整数常量表达式         |
| 否则条件   | `#elif expr`              | else-if 条件           |
| 否则       | `#else`                   | 条件分支的 else 部分   |
| 结束条件   | `#endif`                  | 结束条件编译块         |
| 编译错误   | `#error "msg"`            | 触发编译错误           |
| 编译警告   | `#warning "msg"`          | 触发编译警告           |
| 头文件保护 | `#pragma once`            | 防止重复包含           |
| 行标记     | `#line number "file"`     | 重置行号和文件名       |

---

## 文件包含

```cay ignore
// 当前目录搜索
#include "helper.cay"
#include "math/vector.cay"

// 系统包含路径搜索
#include <stdio.cay>
#include <math.cay>

// 嵌套包含自动去重
#include "helper.cay"    // 已包含，自动跳过
```

### 搜索路径顺序

1. 相对于当前源文件所在目录（`""` 形式）
2. 命令行 `-I` 选项指定的路径
3. `caylibs/`（当前工作目录或 release 目录旁的复制）
4. 内置的系统包含路径（`<>` 形式）

```powershell
# 通过 -I 添加额外包含路径
cayc app.cay -I./vendor -I./include
cay-pre source.cay -I./libs --stdout
```

---

## Cavvy 声明文件（.cayh）

`#include_h` 导入 `.cayh` 声明文件，实现 C 式 `.h`/`.c` 分离：声明放 `.cayh`，
实现放同名 `.cay`，各文件独立编译后由链接器解析：

```powershell
cayc helper.cay main.cay -o main
```

与 `.h`/`.hpp` 不同——后者由 `#include_c` 的提取器转成 Cay FFI 声明（仅限 C
类型集）——`.cayh` 是 Cavvy 源码，可以直接声明 **enum 等高级 ADT**。

### 三文件示例

```cay ignore
// helper.cayh —— 声明文件
public enum Color {          // 高级 ADT 可以直接声明
    Red, Green, Blue
}

impl Color {                 // enum 方法也可以放头文件（linkonce_odr 去重）
    public int index() {
        switch (this) {
            case Color.Red: { return 0; }
            case Color.Green: { return 1; }
            case Color.Blue: { return 2; }
            default: { return -1; }
        }
    }
}

public class Helper {        // 类声明：字段照列，方法/构造/析构只写签名（native）
    private int base;

    public native Helper(int b);
    public native ~Helper();
    public native static int add(int a, int b);
    public native static Color favorite();
    public native int scale(int factor);
}
```

```cay ignore
// helper.cay —— 实现文件：像 C 一样 #include_h 自己的头文件，
// 头中的纯声明类会与这里的实现类合并（不是重复定义）
#include_h "helper.cayh"

public class Helper {
    private int base;

    public Helper(int b) { this.base = b; }
    public ~Helper() { println("Helper dtor"); }

    public static int add(int a, int b) {
        return a + b;
    }
    public static Color favorite() {
        return Color.Blue;
    }
    public int scale(int factor) {
        return this.base * factor;
    }
}
```

```cay ignore
// main.cay —— 使用方只见过声明，符号在链接期由 helper.cay 的 object 解析
#include_h "helper.cayh"

public class Main {
    public static void main() {
        int sum = Helper.add(19, 23);       // 跨编译单元静态调用
        Color c = Helper.favorite();        // 使用头文件中的 enum
        Helper h = new Helper(21);          // 跨 TU 构造
        int scaled = h.scale(2);            // 跨 TU 实例方法 + 字段布局
    }
}
```

完整可运行示例见 `examples/include_h/`。

### .cayh 编写约定

`.cayh` 中只放**声明**，实现放对应的 `.cay`：

- **可以放**：enum 定义（含 `impl` 方法块，重复定义由链接器按
  linkonce_odr 去重）、`#define` 常量、类声明（字段照列；方法/构造/析构
  用 `native` 只写签名）、`extern` 函数原型；
- **字段要与实现文件镜像一致**（同 C++ 的 ODR 要求）：各编译单元按
  各自看到的声明计算对象布局，声明不一致是未定义行为；
- **不要放**：带方法体的类成员实现、静态字段（会生成 private 全局）、
  泛型类声明（跨编译单元特化尚不支持）、顶层函数实现；
- **顶层函数**没有"仅签名"声明语法，跨文件共享请用类的 `static` 方法。

虚方法/继承可以正常使用：vtable 全局符号按 linkonce_odr 去重，声明侧
与实现侧的槽位布局一致。

解析规则与 `#include_c` 一致：

1. `<...>` 系统形式且命中标准库白名单头 `caylibs/cayh/<name>.cayh` 时优先使用该头；
2. 其余情况（含全部 `"..."` 形式）按 `#include` 的搜索顺序定位真实 `.cayh`；
3. 共享 `#pragma once` 去重、循环包含检测与头名→库自动链接映射。

`"..."` 形式永不匹配白名单头——同名 `.cayh` 不会遮蔽你项目目录中的真实文件。

---

## 宏定义

---

### 常量宏

```cay
#define MAX_SIZE 1024
#define APP_NAME "MyApp"
#define DEBUG 1
#define PI 3.1415926535
```

## 条件编译

```cay
#define PLATFORM_WIN
#define DEBUG

#ifdef DEBUG
    #warning "调试模式已启用"
#endif

#ifndef RELEASE
    #warning "非发布版本"
#endif

#if PLATFORM_WIN
    // 包含 Windows 平台头文件
#elif PLATFORM_LINUX
    // 包含 Linux 平台头文件
#else
    #error "未知平台"
#endif
```

### `#if` 表达式支持

- 整数常量计算（`+`, `-`, `*`, `/`, `%`）
- 比较运算（`==`, `!=`, `<`, `>`, `<=`, `>=`）
- 逻辑运算（`&&`, `||`, `!`）
- 位运算（`&`, `|`, `^`, `~`, `<<`, `>>`）
- `defined(NAME)` 操作符

```cay
#define VERSION 2

#if VERSION >= 2
    // 版本 2 及以上特性
#endif

#if defined(DEBUG) && defined(VERBOSE)
    println("详细调试输出");
#endif
```

---

## 实用技巧

### 头文件保护

```cay
// mylib.cay
#pragma once

#define MYLIB_VERSION 1
// ... 库内容，确保只展开一次 ...
```

```

### 平台检测

```cay
#if defined(_WIN32) || defined(_WIN64)
    #define PATH_SEPARATOR "\\"
#else
    #define PATH_SEPARATOR "/"
#endif
```

---

## 结合编译器使用

```powershell
# 查看预处理后的输出
cay-pre source.cay --stdout

# 保存预处理结果
cay-pre source.cay -o preprocessed.cay

# 保留注释
cay-pre source.cay --keep-comments -o output.cay

# 在编译时定义宏
cayc source.cay -D DEBUG -D VERSION=2

# 发布构建
cayc source.cay -D RELEASE -O2
```

---

## 与标准 C 预处理器的差异

- `#pragma once` — 支持，同时隐式执行包含去重
- `#warning` — 支持
- `#include "/absolute/path"` — **不支持**绝对路径包含
- 宏展开终止于递归 — 避免无限递归
- 源映射维护 — 所有预处理后的位置都映射回原始源位置，确保错误定位准确
