# 命令行工具

Release 构建会生成 12 个命令行入口：`Cargo.toml` 显式列出 11 个，`src/bin/cay-pre.rs` 由 Cargo 自动发现。

| 工具 | 用途 |
|---|---|
| `cayc` | `.cay` -> `.exe` 或平台可执行文件 |
| `cay-check` | 只做预处理、词法、语法和语义检查 |
| `cay-ir` | `.cay` -> LLVM IR 文本 |
| `ir2exe` | LLVM IR -> 可执行文件 |
| `cay-run` | 编译并运行 `.cay`、`.caybc` 或 `.ll` |
| `cay-pre` | 独立预处理器 |
| `cay-rcpl` | 交互式 RCPL |
| `cay-bcgen` | 实验性 CayBC 字节码生成 |
| `cay-lsp` | LSP 语言服务器 |
| `cavly` | 包管理器和项目构建工具 |
| `cay-dt` | 文档工具 |
| `cay-dp` | AST/解析调试预览工具 |

## 编译

```powershell
.\target\release\cayc.exe hello.cay hello.exe
.\target\release\cayc.exe -O3 --lto=full hello.cay hello.exe
.\target\release\cayc.exe -I.\caylibs hello.cay hello.exe
```

## 生成 IR

```powershell
.\target\release\cay-ir.exe hello.cay hello.ll
.\target\release\ir2exe.exe hello.ll hello.exe
```

## 编译并运行

```powershell
.\target\release\cay-run.exe hello.cay
.\target\release\cay-run.exe --no-run -o hello.exe hello.cay
```

## Cavly

```powershell
.\target\release\cavly.exe init demo
.\target\release\cavly.exe build
.\target\release\cavly.exe run
.\target\release\cavly.exe test
```
