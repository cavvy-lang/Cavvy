# Cavvy Runtime Library Exception

Version 1.0, 2026-07-23

## 前言

Cavvy 编译器及其运行时库（`libcavvy` / `cayrt`）的核心源码以 **GNU General Public License v3.0 only (GPL-3.0-only)** 发布。这意味着如果你修改了编译器本体或运行时本身，你必须在 GPL-3.0 下公开你的修改。

然而，我们希望 Cavvy 语言本身能够被广泛采用，包括用于开发闭源或商业软件。因此，对于 **Cavvy Runtime Library**（即 `caylibs/bin/cayrt/` 目录下的 C 运行时、`libcavvy` 静态/动态库，以及由编译器自动链接到目标可执行文件中的 GC、FFI 胶水代码等），我们在 GPL-3.0 的基础上增加以下额外授权。

## 额外授权

除 GPL-3.0 授予的权利外，Cavvy 项目的版权所有者额外授予你以下权利：

你可以将 **Cavvy Runtime Library 的已编译形态** 与任何独立程序（independent programs）相链接、调用或组合，无论这些独立程序采用何种许可证；你也可以在任意许可证下复制和分发由此产生的组合作品（combined work），**只要满足下列条件**：

1. **保留声明**：每份组合作品的分发必须附带书面声明，向接收者说明：
   - 该作品使用了 Cavvy Runtime Library；
   - Cavvy Runtime Library 受 GPL-3.0 + Cavvy Runtime Library Exception 约束；
   - 你可以在 <https://github.com/cavvy-lang/Cavvy> 获取 Cavvy Runtime Library 的对应源码。

2. **运行时修改仍需开源**：如果你修改了 Cavvy Runtime Library 的任何部分（包括 `libcavvy`、`cayrt`、GC、FFI 胶水代码等），这些修改后的文件仍必须在 GPL-3.0 + 本 Exception 下发布。你不能通过修改运行时来规避 GPL。

3. **不适用于编译器本体**：本例外 **不适用于** Cavvy 编译器本体（包括但不限于 `cayc`、`cay-ir`、`ir2exe`、`cay-check`、`cay-run`、`cay-rcpl`、`cay-bcgen`、`cay-lsp`、`cavly`、`cay-dt`、`cay-dp`、`cay-pre` 等 Rust 二进制及其依赖库 `cavvy`）。如果你修改编译器本体，仍需遵守 GPL-3.0 的完整 copyleft 要求。

## 定义

- **Cavvy Runtime Library**：指 `caylibs/bin/cayrt/` 目录下的 C 源码、编译产物（`libcayrt.a` 等），以及编译器在生成可执行文件时自动链接到用户程序中的最小运行时支持代码（GC、字符串/数组操作、内存分配、FFI 转换层等）。
- **独立程序（Independent Program）**：指不是基于 Cavvy 编译器源码衍生（derivative work）的程序。仅使用 Cavvy 语言编写、并通过 Cavvy 编译器编译得到的程序属于独立程序。
- **组合作品（Combined Work）**：指独立程序与 Cavvy Runtime Library 链接、调用或打包后形成的可执行文件或库。

## 与 GPL 的关系

本 Exception 是 GNU General Public License, Version 3 第 7 条所允许的 **Additional Permission**。如果本 Exception 与 GPL-3.0 有任何冲突，以 GPL-3.0 为准；但本 Exception 中授予的额外权利不会因此被取消。

---

*Cavvy Runtime Library Exception 由 Cavvy 项目版权所有者制定。*
*有关授权疑问，请参阅 [LICENSE](LICENSE) 与项目 README 中的「许可证」章节。*
