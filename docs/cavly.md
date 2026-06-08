# Cavly 包管理器

`cavly` 是 Cavvy 的项目工具。它负责初始化项目、读取 `cavly.toml`、构建 bin 目标、运行项目、发现并执行测试、管理系统库和 FFI 库配置。

## 常用命令

```powershell
.\target\release\cavly.exe init demo
.\target\release\cavly.exe init --lib mylib
.\target\release\cavly.exe build
.\target\release\cavly.exe build --bin app
.\target\release\cavly.exe run
.\target\release\cavly.exe test
.\target\release\cavly.exe add m
.\target\release\cavly.exe ffi sdl2 SDL2
```

## 项目根

Cavly 通过向上查找 `cavly.toml` 识别项目根目录。构建产物默认放在配置中的 target 目录。
