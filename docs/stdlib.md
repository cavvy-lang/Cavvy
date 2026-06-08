# 标准库

标准库源码位于 `caylibs/`。构建时 `build.rs` 会把它复制到 `target/release/caylibs/`，编译器也会从当前工作目录查找 `caylibs/`。

| 文件 | 主要内容 |
|---|---|
| `Allocator.cay` | 分配器辅助 |
| `EasyHTTP.cay` | 简易 HTTP |
| `File.cay` | 文件、读取器、写入器、路径工具 |
| `IOPlus.cay` | I/O 辅助 |
| `Math.cay` | 数学函数、随机数、向量 |
| `Network.cay` | 网络接口 |
| `Optional.cay` | Optional 风格容器 |
| `StringBuilder.cay` | 可变字符串构建器 |
| `StringPlus.cay` | split、format 等字符串工具 |

## 使用标准库

```text
#include <Math.cay>
using std::Math;
```

部分库依赖 FFI、平台 C 库或运行时 helper。文档示例中优先使用内置 `String.valueOf`、`println` 和基础语法，标准库 API 的行为以 `caylibs/` 源码为准。
