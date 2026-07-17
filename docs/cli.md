# CLI 工具参考手册

Cavvy 6.1.0 提供 16 个 CLI 入口；其中 `cay-setup` 是可独立构建和发布的工具链安装器，不依赖主编译器的 LLVM 构建环境。

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
cayc <input.cay> [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-o <file>` | 指定输出文件路径 |
| `-O0` / `-O1` / `-O2` / `-O3` | 优化级别（默认 `-O0`） |
| `--emit-llvm` | 同时保留 `.ll` 文件 |
| `--verbose` | 显示详细编译日志 |
| `--stage <stage>` | 只运行到指定阶段 |
| `--target <triple>` | 目标三元组 |
| `--print-stages` | 打印编译流水线阶段并退出 |
| `-I <dir>` | 添加包含路径 |
| `-D <macro>` | 预定义宏 |
| `-h` / `--help` | 显示帮助 |

**示例**：

```bash
cayc hello.cay
cayc hello.cay -o hello.exe
cayc hello.cay --emit-llvm -O2
cayc hello.cay -I ./include -D DEBUG
```

---

## 2. cay-ir — LLVM IR 生成器

**从 `.cay` 源文件生成 LLVM IR 文本文件（`.ll`），不进行后续编译。**

```bash
cay-ir <input.cay> [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-o <file>` | 输出 `.ll` 文件路径 |
| `--stdout` | 输出到标准输出 |
| `-O0` / `-O1` / `-O2` / `-O3` | 优化级别 |
| `-I <dir>` | 添加包含路径 |
| `--verbose` | 显示详细日志 |

**示例**：

```bash
cay-ir input.cay -o output.ll
cay-ir input.cay --stdout        # 直接查看生成的 IR
cay-ir input.cay -O2 -o optimized.ll
```

---

## 3. ir2exe — LLVM IR → 可执行文件

**将 LLVM IR 文本文件编译为可执行文件。**

```bash
ir2exe <input.ll> [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-o <file>` | 输出可执行文件路径 |
| `-O0` / `-O1` / `-O2` / `-O3` | 优化级别 |
| `--verbose` | 显示详细日志 |

**示例**：

```bash
ir2exe output.ll -o program.exe
ir2exe output.ll -O2 -o optimized.exe
```

---

## 4. cay-check — 语法和语义检查

**仅执行编译流水线的前端（预处理 → 词法分析 → 解析 → 语义分析），不生成代码。用于快速验证源文件的正确性。**

```bash
cay-check <input.cay> [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-I <dir>` | 添加包含路径 |
| `--verbose` | 显示详细日志 |

**退出码**：
- `0` — 源文件正确
- `1` — 存在编译错误

**示例**：

```bash
cay-check source.cay
cay-check source.cay -I ./include
```

---

## 5. cay-run — 编译并运行

**编译源代码并直接运行生成的可执行文件。**

```bash
cay-run <input.cay> [程序参数...]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-I <dir>` | 添加包含路径 |
| `--verbose` | 显示详细日志 |

所有非选项参数会传递给生成的可执行文件。

**示例**：

```bash
cay-run hello.cay
cay-run program.cay arg1 arg2 arg3
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
| `-I <dir>` | 添加包含路径 |
| `--verbose` | 显示详细日志 |

**支持的命令**：

| 命令 | 描述 |
|---|---|
| 任意表达式 | 计算并输出结果 |
| 变量声明 | 在会话上下文中持久化 |
| 类/接口定义 | 实时定义新类型 |
| 控制流语句 | 即时执行 |
| `#include` | 导入文件 |
| `:exit` / `:quit` | 退出 RCPL |
| `:help` | 显示帮助 |

**示例**：

```
> cay-rcpl
Cavvy RCPL v6.1.0
> int x = 42
> x * 2
84
> println("Hello from RCPL!")
Hello from RCPL!
> :exit
```

---

## 7. cay-bcgen — 字节码生成器

**将 `.cay` 源文件编译为 CayBC 字节码。**

```bash
cay-bcgen <input.cay> [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-o <file>` | 输出 `.caybc` 文件路径 |
| `--obfuscate` | 启用字节码混淆 |
| `--obfuscation-level <0-3>` | 混淆级别 |
| `-I <dir>` | 添加包含路径 |
| `--verbose` | 显示详细日志 |

**示例**：

```bash
cay-bcgen input.cay -o output.caybc
cay-bcgen input.cay --obfuscate -o obfuscated.caybc
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

## 10. cay-dt — 文档工具

**从源码注释生成文档。**

```bash
cay-dt <input.cay> [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-o <dir>` | 输出目录 |
| `--format <fmt>` | 输出格式（html / markdown） |
| `-I <dir>` | 添加包含路径 |
| `--verbose` | 显示详细日志 |

---

## 11. cay-dp — 依赖分析工具

**分析项目的依赖关系图。**

```bash
cay-dp <input.cay> [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `--graph` | 输出 DOT 格式的依赖图 |
| `--json` | 输出 JSON 格式 |
| `-I <dir>` | 添加包含路径 |
| `--verbose` | 显示详细日志 |

---

## 12. cay-pre — 独立预处理器

**仅执行预处理阶段，输出预处理后的源代码。**

```bash
cay-pre <input.cay> [选项]
```

**选项**：

| 选项 | 描述 |
|---|---|
| `-o <file>` | 输出文件 |
| `--stdout` | 输出到标准输出 |
| `-I <dir>` | 添加包含路径 |
| `-D <macro>` | 预定义宏 |
| `--keep-comments` | 保留注释 |
| `--verbose` | 显示详细信息 |

**示例**：

```bash
cay-pre input.cay -o output_preprocessed.cay
cay-pre input.cay --stdout | grep "MAIN"
cay-pre input.cay -D DEBUG -D VERSION=2
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
