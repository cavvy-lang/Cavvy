# Cavvy

<p align="center">
  <img src="docs/README/images/Cavvy.png" alt="Cavvy Logo" width="600">
</p>

<p align="center">
  <strong>Rust Frontend · LLVM Backend · Native Lightweight</strong>
</p>

<p align="center">
  <a href="#特性">特性</a> •
  <a href="#快速开始">快速开始</a> •
  <a href="#工具链">工具链</a> •
  <a href="#语法概览">语法概览</a> •
  <a href="#文档">文档</a> •
  <a href="#贡献">贡献</a>
</p>

---

## 简介

Cavvy 是一个静态类型、面向对象的编程语言编译器。它采用 Rust 2024 Edition 编写，将 `.cay` 源码编译成 LLVM IR，再通过 LLVM/MinGW 工具链生成本地可执行文件。

## 特性

- **现代语法**: Java/C# 风格的面向对象语法，支持类、接口、继承、泛型
- **C 风格预处理器**: 完整的 `#include`、`#define`、`#if` 条件编译支持
- **FFI 支持**: 无缝调用 C 函数和库
- **Lambda 表达式**: 函数式编程支持
- **Struct 与 Enum**: 值类型和枚举类型
- **LLVM 后端**: 利用 LLVM 优化生成高性能本地代码
- **LSP 支持**: 完整的语言服务器协议支持
- **包管理器**: 内置 Cavly 包管理工具

## 快速开始

### 环境准备

```powershell
# 克隆仓库
git clone <repository-url>
cd cavvy

# 设置工具链（首次运行）
python setup-llvm.py

# 构建 Release 版本
cargo build --release
```

### 第一段程序

创建 `hello.cay`:

```cay
public int main() {
    println("Hello, Cavvy!");
    return 0;
}
```

编译并运行:

```powershell
.\target\release\cayc.exe hello.cay
.\hello.exe
# 输出: Hello, Cavvy!
```

## 工具链

| 工具          | 用途                                       |
| :------------ | :----------------------------------------- |
| `cayc`      | 一站式编译器:`.cay` → `.exe`          |
| `cay-check` | 代码检查: 预处理、词法、语法、语义分析     |
| `cay-ir`    | 生成 LLVM IR:`.cay` → `.ll`           |
| `ir2exe`    | IR 编译器:`.ll` → `.exe`              |
| `cay-run`   | 编译并运行 `.cay`、`.caybc` 或 `.ll` |
| `cavly`     | 包管理器和项目构建工具                     |
| `cay-lsp`   | LSP 语言服务器                             |
| `cay-rcpl`  | 交互式 RCPL 环境                           |
| `cay-dt`    | 文档工具                                   |
| `cay-dp`    | AST/解析调试预览工具                       |

### 常用命令

```powershell
# 编译程序
.\target\release\cayc.exe program.cay program.exe

# 启用优化
.\target\release\cayc.exe -O3 --lto=full program.cay program.exe

# 只检查代码
.\target\release\cay-check.exe program.cay

# 生成 LLVM IR
.\target\release\cay-ir.exe program.cay program.ll

# 编译并运行
.\target\release\cay-run.exe program.cay

# Cavly 包管理
.\target\release\cavly.exe init myproject
.\target\release\cavly.exe build
.\target\release\cavly.exe run
.\target\release\cavly.exe test
```

## 语法概览

### 类与方法

```cay
public class Counter {
    private int current;

    public Counter(int start) {
        this.current = start;
    }

    public void add(int value) {
        this.current = this.current + value;
    }

    public int value() {
        return this.current;
    }
}

public class App {
    public static void main() {
        Counter c = new Counter(2);
        c.add(3);
        println(String.valueOf(c.value()));
    }
}
```

### 控制流

```cay
public class Flow {
    public static void main() {
        int total = 0;
        for (int i = 0; i < 5; i = i + 1) {
            total = total + i;
        }

        if (total > 5) {
            println("Greater than 5");
        } else {
            println("5 or less");
        }

        switch (total) {
            case 10: println("Ten"); break;
            default: println("Other"); break;
        }
    }
}
```

### Lambda 与泛型

```cay
public class Box<T> {
    private T value;

    public Box(T value) {
        this.value = value;
    }

    public T get() {
        return this.value;
    }
}

public class ModernFeatures {
    public static fn(int) -> int makeAdder(int base) {
        return (int value) -> base + value;
    }

    public static void main() {
        Box<int> box = new Box<int>(7);
        var addBox = makeAdder(box.get());
        println(String.valueOf(addBox(5)));
    }
}
```

### Struct 与 Enum

```cay
public struct Point {
    public int x;
    public int y;

    public int sum() {
        return x + y;
    }
}

public enum Status {
    Ready,
    Done
}

public class DataDemo {
    public static void main() {
        Point p = new Point();
        p.x = 2;
        p.y = 5;

        Status status = Status.Done;
        switch (status) {
            case Status.Done: println(String.valueOf(p.sum())); break;
            default: println("waiting"); break;
        }
    }
}
```

### FFI 示例

```cay
extern {
    int strlen(c_string s);
    int printf(c_string fmt, ...);
}

public class FFIExample {
    public static void main() {
        c_string msg = "Hello from Cavvy!";
        int len = strlen(msg);
        printf("Length: %d\n", len);
    }
}
```

## 编译流水线

```
.cay 源码
    ↓
预处理器 (#include, #define, #if)
    ↓
词法分析器 (logos token stream)
    ↓
解析器 (递归下降 AST)
    ↓
语义分析 (类型检查、符号解析)
    ↓
代码生成 (LLVM IR 文本)
    ↓
ir2exe (LLVM/MinGW 本地可执行文件)
    ↓
.exe
```

## 文档

完整文档位于 `docs/` 目录：

- [语言总览](docs/language-overview.md) - Cavvy 语法特性详解
- [快速开始](docs/getting-started.md) - 详细入门指南
- [命令行工具](docs/cli.md) - 所有 CLI 工具参考
- [工具链与构建](docs/toolchain.md) - 构建和测试指南
- [编译器架构](docs/compiler-architecture.md) - 内部架构说明
- [语言参考](docs/language-reference.md) - 完整语言规范
- [FFI 指南](docs/ffi.md) - 外部函数接口
- [预处理器](docs/preprocessor.md) - 预处理器文档

### 构建文档站

```powershell
cargo install mdbook --locked
mdbook build
mdbook serve
```

## 测试

```powershell
# 构建 Release 版本
cargo build --release

# 运行所有测试
cargo test --release --verbose
```

测试包括：

- 单元测试
- 集成测试（编译 `examples/` 下的 `.cay` 文件并验证输出）
- 文档示例测试

## 项目结构

```
cavvy/
├── Cargo.toml           # 项目配置
├── src/
│   ├── lib.rs          # 核心库入口
│   ├── bin/            # CLI 工具入口
│   ├── preprocessor/   # 预处理器
│   ├── lexer/          # 词法分析器
│   ├── parser/         # 解析器
│   ├── semantic/       # 语义分析
│   ├── codegen/        # LLVM IR 生成
│   ├── ir2exe_lib/     # IR 到可执行文件
│   ├── cavly/          # 包管理器
│   ├── bytecode/       # CayBC 字节码
│   └── rcpl/           # 交互式环境
├── examples/           # 示例程序
├── docs/               # 文档
├── caylibs/            # 标准库
└── tests/              # 测试
```

## 贡献

欢迎贡献！请阅读 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) 了解行为准则。

## 许可证

GNU GPLv3

---

<p align="center">
  Made with ❤️ using Rust & LLVM
</p>
