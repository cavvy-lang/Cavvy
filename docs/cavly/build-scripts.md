# Cavly 构建脚本 (build.cay)

构建脚本（`build.cay`）在主项目编译之前自动编译并运行，可用于代码生成、下载外部资源、配置构建环境等任务。设计上参考了 Rust 的 `build.rs`。

## 快速开始

初始化项目时自动生成 `build.cay` 模板：

```bash
cavly init my-project
# 自动创建 build.cay
```

模板内容：

```cay
// Cavvy 构建脚本 (build.cay)
public class BuildScript {
    public static void main() {
        println("Build script executed successfully!");
    }
}
```

## 执行流程

`cavly build` 的完整流程：

```
1. 编译 build.cay → target/build-script/build.exe
2. 运行 build.exe（传入环境变量）
3. 检查退出码（非 0 则中止构建）
4. 构建依赖库
5. 编译主项目源文件
```

## 环境变量

构建脚本运行时获得以下环境变量：

| 变量 | 示例值 | 说明 |
|---|---|---|
| `OUT_DIR` | `target/build-script-out` | 构建脚本的输出目录，可在此生成文件供主项目使用 |
| `PROJECT_ROOT` | `/path/to/my-project` | 项目根目录的绝对路径 |
| `PROFILE` | `release` | 构建配置：`debug` 或 `release` |
| `OPT_LEVEL` | `2` | 优化级别：`0`、`1`、`2`、`3`、`s`、`z` |
| `TARGET` | `x86_64-w64-mingw32` | 目标平台三元组 |

**读取环境变量**：

```cay
// 在 Cavvy 中通过 extern 调用 C 标准库的 getenv
extern {
    String getenv(String name);
}

public class BuildScript {
    public static void main() {
        String outDir = getenv("OUT_DIR");
        String profile = getenv("PROFILE");
        println("Building in " + profile + " mode");
        println("Output directory: " + outDir);
    }
}
```

## 常见用例

### 1. 代码生成

在 `OUT_DIR` 中生成 Cavvy 源文件：

```cay
public class BuildScript {
    public static void main() {
        String outDir = getenv("OUT_DIR");
        String projectRoot = getenv("PROJECT_ROOT");
        
        // 生成版本头文件
        String versionFile = outDir + "/version.cay";
        // 使用文件 I/O 写入生成的代码
        println("Generated version file: " + versionFile);
    }
}
```

**注意**：当前生成的源文件需要通过 `#include` 或 `-I` 路径引入主项目中。在 `cavly.toml` 中添加：

```toml
[build]
cflags = ["-I", "target/build-script-out"]
```

### 2. 下载外部依赖

```cay
// build.cay - 下载 SDL2 开发库
public class BuildScript {
    public static void main() {
        String target = getenv("TARGET");
        println("Downloading SDL2 for " + target + "...");
        // TODO: 使用 network 模块下载
    }
}
```

### 3. 编译 C/C++ 代码

如果你的项目包含 C/C++ 源文件，可以在构建脚本中调用外部编译器：

```cay
// build.cay - 编译 C 辅助代码
public class BuildScript {
    public static void main() {
        println("Compiling C helper library...");
        // 调用系统命令编译 C 代码
        // 将生成的 .o/.obj 文件放在 OUT_DIR
    }
}
```

### 4. 条件构建

根据目标平台或构建配置选择性地生成代码：

```cay
public class BuildScript {
    public static void main() {
        String target = getenv("TARGET");
        String outDir = getenv("OUT_DIR");
        
        if (target.contains("windows")) {
            println("Windows-specific setup");
            // Windows 特定配置
        } else if (target.contains("linux")) {
            println("Linux-specific setup");
            // Linux 特定配置
        }
    }
}
```

## 配置

在 `cavly.toml` 中指定构建脚本路径：

```toml
[package]
# ...
build_script = "build.cay"    # 默认值

# 自定义路径
build_script = "scripts/build.cay"

# 禁用构建脚本
build_script = ""
```

## 错误处理

构建脚本退出码非 0 会中止整个构建流程：

```cay
public class BuildScript {
    public static void main() {
        // 检查必要条件
        boolean hasRequired = false;  // 检查逻辑
        
        if (!hasRequired) {
            println("ERROR: Required dependency not found!");
            // 非 0 退出码会使构建失败
            return;  // 在 Cavvy main 中，void 返回视为成功
            // 要表示失败，可以调用 exit(1) 或抛出错误
        }
    }
}
```

## 限制

- 构建脚本使用 `-O0` 编译（优先编译速度）
- 构建脚本编译错误会中止整个构建
- 构建脚本的超时由操作系统进程管理
- 构建脚本的输出目录 `OUT_DIR` 在每次构建时保留（不会被 `cavly clean` 之外的操作清理）
