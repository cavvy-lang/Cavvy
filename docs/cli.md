# CLI 工具参考手册

Cavvy 6.2.0 提供 16 个 CLI 入口；其中 `cay-setup` 是可独立构建和发布的工具链安装器，不依赖主编译器的 LLVM 构建环境。

---

## 总览

| 二进制文件 | 功能 | 源文件 |
|---|---|---|
| `cayc` | 一站式编译器：`.cay` → `.exe` | `src/bin/cayc.rs` |
| `cay-ir` | 仅生成 LLVM IR（`.cay` → `.ll`） | `src/bin/cay-ir.rs` |
| `ir2exe` | LLVM IR → 可执行文件（`.ll` → `.exe`） | `src/bin/ir2exe.rs` |
| `cay-check` | 仅语法 + 语义检查 | `src/bin/cay-check.rs` |
| `cay-run` | 编译 + 运行一步完成 | `src/bin/cay-run.rs` |
| `cay-rcpl` | 交互式编程环境 | `src/bin/cay-rcpl.rs` |
| `cay-bcgen` | CayBC 字节码生成 | `src/bin/cay-bcgen.rs` |
| `cay-lsp` | LSP 语言服务器 | `src/bin/cay-lsp.rs` |
| `cavly` | 包管理器 | `src/bin/cavly.rs` |
| `cay-dt` | Token显示工具 | `src/bin/cay-dt.rs` |
| `cay-dp` | Parser显示工具 | `src/bin/cay-dp.rs` |
| `cay-pre` | 独立预处理器 | `src/bin/cay-pre.rs` |
| `cay-ast` | AST 查看与 JSON 导出 | `src/bin/cay-ast.rs` |
| `cay-pl` | 预处理结果查看 | `src/bin/cay-pl.rs` |
| `cay-sir` | 语义 IR 查看 | `src/bin/cay-sir.rs` |
| `cay-setup` | 安装、更新、检查和卸载工具链 | `cay-setup/src/main.rs` |

---

## cay-setup — 工具链安装器

直接运行且不传参数时，安装最新稳定版：

```powershell
cay-setup
```

常用管理命令：

```powershell
cay-setup install --version 6.1.0
cay-setup update
cay-setup show
cay-setup doctor
cay-setup uninstall
```

自动化环境使用 `--yes` 跳过确认，使用 `--no-modify-path` 禁止修改用户 PATH。可以通过
`CAVVY_HOME` 或 `--root` 更改默认的 `~/.cavvy` 安装根目录；`show`、`doctor` 和
`uninstall` 同样接受 `--root`。`doctor` 会实际编译一个最小程序，以验证 LLVM 后端和
链接库，而不只是打印版本号。每个版本安装到 `~/.cavvy/toolchains/<版本>`，完成校验后
才切换 PATH；管理器来自 Release 的独立 `cay-setup-<平台>-<架构>` 资产。

---

## 1. cayc — 一站式编译器

**将 `.cay` 源文件直接编译为可执行文件。**

```bash
cayc [选项] <source1.cay> [source2.cay ...] [output.exe]
```

- 位置参数以 `.cay` 结尾的均视为源文件，可传入一个或多个。
- 最后一个不以 `.cay` 结尾的位置参数为输出可执行文件名；省略时默认使用第一个源文件的 stem。
- `-c` 表示**仅编译**（compile-only），把每个源文件编译成目标文件 `.obj` 后停止，不进行链接。

**选项**：

| 选项 | 描述 |
|---|---|
| `-c` | 仅编译到目标文件，不链接（单源文件时可用 `-o` 或位置参数指定目标文件名；多源文件时每个源文件各生成一个 `.obj`，不能指定输出名） |
| `-o <file>` / `--output <file>` | 指定输出文件名（可替代最后一个位置参数） |
| `-O0` / `-O1` / `-O2` / `-O3` / `-Os` / `-Oz` | 优化级别（默认 `-O2`；`-Os`/`-Oz` 在 llc 工具链下先对 IR 做中端体积优化） |
| `--opt-ir` | 启用 IR 阶段优化（使用 LLVM 中端流水线优化 IR） |
| `--lto[=<type>]` | 链接时优化（`full` / `thin`；需要 clang 工具链，自动切换，与 `--use-llc-lld`/`--use-embedded-llc` 互斥） |
| `-march=<arch>` / `-mtune=<cpu>` / `-mcpu=<cpu>` | 目标 CPU 架构与调优（llc 工具链下优先级 `-mcpu` > `-march` > `-mtune`） |
| `-msse=<ver>` / `-mavx=<ver>` / `--mneon` | SIMD 指令集（SSE / AVX / NEON） |
| `-funroll-loops` / `-fvectorize` / `-fslp-vectorize` | 循环展开与自动向量化（IR 中端优化，llc 工具链下先优化 IR 再生成代码） |
| `-fomit-frame-pointer` | 省略帧指针 |
| `-fprofile-generate` / `-fprofile-use=<path>` / `-fcs-profile-generate` | PGO 性能分析优化（需要 clang 工具链，自动切换） |
| `-g` | 生成调试信息 |
| `--keep-ir` | 同时保留 `.ll` 文件 |
| `-I<path>` | 添加包含路径 |
| `-L<path>` / `-l<lib>` | 库搜索路径 / 链接额外的库 |
| `--ldflags <flags>` / `--cflags <flags>` | 传递额外的链接器 / 编译器标志 |
| `--static` / `-fPIC` | 静态链接 / 位置无关代码 |
| `--target <triple>` | 目标三元组 |
| `--use-clang` / `--use-llc-lld` / `--use-embedded-llc` | 选择工具链（默认 llc+lld；embedded 为实验性） |
| `--detect-cycles` | 启用 `Rc<T>` 循环引用运行时检测 |
| `--no-panic` | 将 `panic()`/`abort()` 调用转为编译错误（嵌入式等场景，6.2.0 起） |
| `-fno-exceptions` / `-fno-rtti` | 禁用异常处理 / 运行时类型信息 |
| `-F<feature>` / `--feature=<feature>` | 启用语言特性（如 `top_level_function`） |
| `-D<macro>[=<value>]` / `-U<macro>` | 预定义 / 取消预处理器宏 |
| `-v` / `--version` | 显示版本号 |
| `-h` / `--help` | 显示帮助 |

**示例**：

```bash
cayc hello.cay
cayc hello.cay hello.exe
cayc --keep-ir -O2 hello.cay
cayc -I./include -D DEBUG hello.cay

# 多文件编译并链接（6.x 起支持）
cayc helper.cay main.cay
cayc helper.cay main.cay myapp
cayc helper.cay main.cay -o myapp

# 仅编译为目标文件
cayc -c helper.cay
cayc -c helper.cay main.cay
```

> **Breaking change（6.x 起）**：`cayc` 的位置参数从「单一源文件 + 可选输出」改为「一个或多个源文件 + 可选输出」。单一源文件的旧用法完全兼容；多源文件时默认以第一个源文件的 stem 作为输出名。

---

## 2. cay-ir — LLVM IR 生成器

**从 `.cay` 源文件生成 LLVM IR 文本文件（`.ll`），不进行后续编译。**

```bash
cay-ir [选项] <input.cay> [output.ll]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-O0` / `-O1` / `-O2` / `-O3` / `-Os` / `-Oz` | 优化级别（默认 `-O2`） |
| `-g` | 生成 DWARF 调试信息 |
| `--opt-ir` | 使用 LLVM 优化 IR |
| `--emit-optimized` | 输出优化后的 IR（与 `--opt-ir` 一起使用） |
| `--target <os>` | 目标操作系统（windows / linux / macos） |
| `--obfuscate` | 混淆 IR 代码 |
| `--detect-cycles` | 启用 `Rc<T>` 循环引用运行时检测 |
| `-I<path>` | 添加包含路径 |
| `-D<macro>[=<value>]` / `-U<macro>` | 预定义 / 取消预处理器宏 |
| `-v` / `--version` | 显示版本号 |
| `-h` / `--help` | 显示帮助 |

**示例**：

```bash
cay-ir input.cay                  # 生成 input.ll
cay-ir input.cay output.ll        # 输出路径为位置参数
cay-ir --opt-ir --emit-optimized -O3 input.cay
```

---

## 3. ir2exe — LLVM IR → 可执行文件

**将 LLVM IR 文本文件编译为可执行文件。**

```bash
ir2exe [选项] <input.ll> [output.exe]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-O0` / `-O1` / `-O2` / `-O3` / `-Os` / `-Oz` | 优化级别（默认 `-O2`） |
| `--lto[=<type>]` | 链接时优化（`full` / `thin`；需要 clang 工具链，自动切换，与 `--use-llc-lld`/`--use-embedded-llc` 互斥） |
| `--march <arch>` / `--mtune <cpu>` / `--mcpu <cpu>` | 目标 CPU 架构与调优 |
| `--msse <ver>` / `--mavx <ver>` / `--mneon` | SIMD 指令集（SSE / AVX / NEON） |
| `--funroll-loops` / `--fvectorize` / `--fslp-vectorize` | 循环展开与自动向量化（IR 中端优化，llc 工具链下先优化 IR 再生成代码） |
| `--fomit-frame-pointer` | 省略帧指针 |
| `--pgo-gen` / `--pgo-use <path>` / `--pgo-cs` | PGO 性能分析优化（需要 clang 工具链，自动切换） |
| `-g` | 生成调试信息 |
| `-L<path>` / `-l<lib>` | 库搜索路径 / 链接额外的库 |
| `--ldflags <flags>` / `--cflags <flags>` | 传递额外的链接器 / 编译器标志 |
| `--static` / `-fPIC` | 静态链接 / 位置无关代码 |
| `--target <target>` | 指定目标平台 |
| `-fno-exceptions` / `-fno-rtti` | 禁用异常处理 / 运行时类型信息 |
| `--use-clang` / `--use-llc-lld` / `--use-embedded-llc` | 选择工具链（默认 llc+lld；embedded 为实验性） |

**示例**：

```bash
ir2exe output.ll program.exe
ir2exe -O2 output.ll optimized.exe
```

---

## 4. cay-check — 语法和语义检查

**仅执行编译流水线的前端（预处理 → 词法分析 → 解析 → 语义分析），不生成代码。用于快速验证源文件的正确性。**

```bash
cay-check [选项] <input.cay>
```

**选项**：

| 选项 | 描述 |
|---|---|
| `--lex-only` | 只进行词法分析 |
| `--parse-only` | 进行词法和语法分析（不进行语义分析） |
| `--no-preprocess` | 跳过预处理阶段 |
| `-v` / `--version` | 显示版本号 |
| `-h` / `--help` | 显示帮助 |

**退出码**：
- `0` — 源文件正确
- `1` — 存在编译错误

**示例**：

```bash
cay-check source.cay
cay-check --parse-only source.cay
```

---

## 5. cay-run — 编译并运行

**编译源代码并直接运行生成的可执行文件。也支持直接运行 `.caybc` 字节码和 `.ll` IR 文件。**

```bash
cay-run [选项] <文件>
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-o <file>` | 指定输出可执行文件名 |
| `--no-run` | 只编译不运行 |
| `-O<level>` | 优化级别（0 / 1 / 2 / 3 / s / z） |
| `-I<path>` | 添加包含路径 |
| `-D<macro>[=<value>]` / `-U<macro>` | 预定义 / 取消预处理器宏 |
| `-L<path>` / `-l<lib>` | 库搜索路径 / 链接指定库 |
| `-F<feature>` | 启用语言特性（如 `-F=top_level_function`） |
| `--obfuscate` / `--obfuscate-level <l>` | 混淆字节码（`light` / `normal` / `deep`，仅对 `.cay` 文件） |
| `--detect-cycles` | 启用 `Rc<T>` 循环引用运行时检测 |
| `--keep-temp` | 保留临时文件 |
| `--use-embedded-llc` | 使用内嵌 llc 编译 IR（实验性） |
| `-v` / `--verbose` | 显示详细编译信息 |

**示例**：

```bash
cay-run hello.cay
cay-run -O2 -o myapp hello.cay
cay-run program.caybc
```

---

## 6. cay-rcpl — 交互式编程环境

**启动交互式 REPL 环境，支持逐行输入和执行 Cavvy 代码。**

```bash
cay-rcpl [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `--use-embedded-llc` | 使用内嵌 llc 编译 IR（实验性） |
| `-v` / `--version` | 显示版本号 |
| `-h` / `--help` | 显示帮助 |

**支持的命令**：

| 命令 | 描述 |
|---|---|
| 任意表达式 | 计算并输出结果 |
| 变量声明 | 在会话上下文中持久化 |
| 类/接口定义 | 实时定义新类型 |
| 控制流语句 | 即时执行 |
| `#include` | 导入文件 |
| `:q` / `:quit` / `exit` | 退出 RCPL |
| `:h` / `:help` | 显示帮助 |
| `:c` / `:clear` | 清屏 |

**示例**：

```
> cay-rcpl
Cavvy RCPL v6.2.0
> int x = 42
> x * 2
84
> println("Hello from RCPL!")
Hello from RCPL!
> :quit
```

---

## 7. cay-bcgen — 字节码生成器

**将 `.cay` 源文件编译为 CayBC 字节码。（实验性工具，可能包含严重错误和不稳定性。）**

```bash
cay-bcgen [选项] <input.cay>
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-o <file>` | 输出 `.caybc` 文件路径（默认：输入文件名 `.caybc`） |
| `--obfuscate` | 启用字节码混淆 |
| `--obfuscate-level <l>` | 混淆级别（`light` / `normal` / `deep`，默认 `normal`） |
| `-v` / `--verbose` | 显示详细编译信息 |

**示例**：

```bash
cay-bcgen input.cay -o output.caybc
cay-bcgen input.cay --obfuscate --obfuscate-level deep
```

---

## 8. cay-lsp — LSP 语言服务器

**启动 LSP 协议语言服务器，与支持 LSP 的编辑器（如 VS Code）配合使用。**

```bash
cay-lsp
```

**选项**：无命令行选项（通过 LSP 协议通信）。

**编辑器配置**（VS Code 扩展位于 `vscode-extension/`）：

工具链中包含 VS Code 扩展，提供：
- 语法高亮
- 自动补全
- 诊断信息（错误和警告）
- 跳转到定义
- 悬停信息

---

## 9. cavly — 包管理器

**完整的包管理工具，用于创建、构建和管理 Cavvy 项目。**

```bash
cavly <子命令> [选项]
```

**子命令**：

| 子命令 | 描述 |
|---|---|
| `new <name>` | 创建新项目 |
| `init` | 在当前目录初始化项目 |
| `build` | 构建项目 |
| `run` | 构建并运行 |
| `test` | 运行测试 |
| `clean` | 清理构建产物 |
| `add <dependency>` | 添加依赖 |
| `remove <dependency>` | 移除依赖 |
| `publish` | 发布包 |
| `install` | 安装依赖 |
| `workspace` | 工作区管理 |
| `help` | 显示帮助 |

**示例**：

```bash
cavly new my-project
cd my-project
cavly add some-lib
cavly build
cavly run
cavly test
```

> 详见 [Cavly 文档](cavly.md)。

---

## 10. cay-dt — Token 查看工具

**以可读形式显示源文件的词法分析结果（Token 流），用于调试词法分析器。**

```bash
cay-dt <input.cay> [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `--json` | 以 JSON 格式输出 tokens |
| `--no-color` | 禁用彩色输出 |
| `--show-location` | 显示详细位置信息 |
| `--no-preprocess` | 禁用预处理器 |
| `-v` / `--version` | 显示版本号 |
| `-h` / `--help` | 显示帮助 |

---

## 11. cay-dp — Parser 查看工具

**以可读形式显示源文件的语法分析结果（AST），用于调试语法分析器。**

```bash
cay-dp <input.cay> [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `--json` | 以 JSON 格式输出 AST |
| `--no-color` | 禁用彩色输出 |
| `--compact` | 紧凑输出模式 |
| `--no-preprocess` | 禁用预处理器 |
| `-v` / `--version` | 显示版本号 |
| `-h` / `--help` | 显示帮助 |

---

## 12. cay-pre — 独立预处理器

**仅执行预处理阶段，输出预处理后的源代码（默认输出到 stdout）。**

```bash
cay-pre [选项] <input.cay>
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-o <file>` | 输出文件（默认输出到 stdout） |
| `-I<path>` | 添加包含路径 |
| `--source-map` | 显示源映射信息 |
| `-v` / `--verbose` | 显示详细处理信息 |

**示例**：

```bash
cay-pre input.cay -o output_preprocessed.cay
cay-pre input.cay | grep "MAIN"
cay-pre -I./caylibs input.cay
```

---

## 通用行为

### 错误报告

所有工具使用 `miette` 进行格式化错误输出，提供：
- 彩色源码片段
- 错误位置标注
- 详细的错误描述和建议

### 退出码

| 退出码 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 编译错误 |
| 2 | 运行时错误 |
| 3 | 文件未找到 |
| 4 | 内部错误（应报告为 bug） |

### 环境变量

| 变量 | 描述 |
|---|---|
| `CAVVC_PATH` | cayc 编译器路径（用于测试） |
| `CAVVY_HOME` | Cavvy 安装目录 |
| `CAVVY_LIB_PATH` | 标准库路径 |
| `CAVVY_LLVM_PATH` | LLVM 工具链路径 |
