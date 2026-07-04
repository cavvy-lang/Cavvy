# 预处理器指南

Cavvy 在词法分析前运行完整的 C 风格预处理器，并保留源映射（source map）用于诊断错误定位。

---

## 支持的指令

| 指令       | 语法                        | 说明                   |
| ---------- | --------------------------- | ---------------------- |
| 文件包含   | `#include "file"`         | 从当前文件所在目录搜索 |
| 系统包含   | `#include <file>`         | 从系统包含路径搜索     |
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
