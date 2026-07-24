# Cavvy License Define

本文件定义本仓库各路径的许可证生效范围。

## 许可证分层

| 路径 | 内容 | 许可证 |
|---|---|---|
| 仓库根目录及除 `caylibs/` 外的所有文件 | 编译器源码、工具链、文档等 | GPL-3.0-only |
| `caylibs/bin/` | 运行时二进制、构建脚本、静态库（如 `cayrt`, `build.sh`, `libcayrt.a` 等） | GPL-3.0-only + Cavvy Runtime Library Exception (Cavvy-RLE) |
| `caylibs/` 下除 `bin/` 外的其他文件 | Cavvy 标准库源码（如 `string_ops`, `vector` 等） | MIT |

## 说明

- **GPL-3.0-only**：修改、分发编译器本体必须遵循 GPL-3.0，不提供后续版本升级条款。
- **Cavvy Runtime Library Exception (Cavvy-RLE)**：作为 GPL-3.0-only 的附加例外，允许独立作品（Independent Works）链接或调用 `caylibs/bin/` 下的运行时组件，而不受 GPL 传染条款约束。修改运行时本身仍需遵循 GPL-3.0-only。
- **MIT**：`caylibs/` 下的 Cavvy 标准库源码可自由使用、修改、分发，包括闭源商用。

## 路径速查

```
仓库根目录及 caylibs/ 以外  →  GPL-3.0-only
caylibs/bin/                  →  GPL-3.0-only + Cavvy-RLE
caylibs/* (排除 bin/)          →  MIT
```

## 例外边界

- 若 `caylibs/` 下的 MIT 标准库源码内联或嵌入了 `caylibs/bin/` 下的 GPL+RLE 运行时代码（如宏、`static inline` 函数），该内联部分仍适用 GPL-3.0-only + Cavvy-RLE，不影响外层标准库文件的 MIT 许可。
- 通过 Cavvy 编译器生成的目标代码或二进制文件，其许可证由用户源码的许可证决定，编译器本身不附加额外许可要求。