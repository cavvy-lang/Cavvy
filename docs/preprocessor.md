# 预处理器

Cavvy 在词法分析前运行 C 风格预处理器，并保留源映射用于诊断。

## 指令

| 指令 | 说明 |
|---|---|
| `#include "path"` | 从当前文件目录包含文件 |
| `#include <path>` | 从系统包含路径包含文件 |
| `#define NAME value` | 常量宏 |
| `#ifdef NAME` / `#ifndef NAME` | 条件编译 |
| `#if expr` / `#elif expr` | 条件表达式 |
| `#else` / `#endif` | 条件分支 |
| `#error "message"` | 编译期错误 |
| `#warning "message"` | 编译期警告 |
| `#pragma once` | 兼容处理，包含去重也会隐式执行 |

## 示例

```cay
#define APP_NAME "Configured App"
#define ENABLED 1

public class Configured {
    public static void main() {
#if ENABLED
        println(APP_NAME);
#else
        println("disabled");
#endif
    }
}
```

## 包含路径

`cayc` 和 `cay-check` 会自动查找当前工作目录下的 `caylibs/`，也会查找 release 二进制目录旁边复制出的 `caylibs/`。额外路径可通过 `-I` 提供：

```powershell
.\target\release\cayc.exe -I.\vendor app.cay app.exe
```
