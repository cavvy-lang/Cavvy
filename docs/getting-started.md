# 快速开始

## 安装

### 系统要求

- **操作系统**：Windows（主要目标平台）、Linux（musl 交叉编译支持）
- **Rust 工具链**：stable 版本
- **LLVM/clang**：捆绑在 `llvm-minimal/` 中，无需单独安装
- **MinGW**：捆绑在 `mingw-minimal/` 中
- **额外链接库**：`lib/` 目录下捆绑了运行时链接库
- **磁盘空间**：构建需要约 2GB 空间

### 从源码构建

```powershell
# 1. 克隆仓库
git clone https://github.com/cavvy-lang/Cavvy.git
cd Cavvy

# 2. 安装工具链依赖（如缺失）
python setup-llvm.py

# 3. 构建 release 版本
cargo build --release

# 4. 验证安装
.\target\release\cayc.exe --version
```

> **注意**：`release` 是正常构建模式，仅在 release 模式下才会复制捆绑工具链。debug 构建用于开发，不包含捆绑工具。
> `build.rs` 在构建时将 `llvm-minimal/`、`mingw-minimal/`、`lib/` 复制到 `target/<profile>/`。
> `.cargo/config.toml` 仅包含 linux-musl 交叉编译配置，Windows 上可忽略。

构建产物位于 `target/release/` 目录，共 11 个可执行文件（详见 [CLI 文档](cli.md)）。

---

## Hello World

创建一个文件 `hello.cay`：

```cay
public int main() {
    println("Hello, Cavvy!");
    return 0;
}
```

编译并运行：

```powershell
.\target\release\cayc.exe hello.cay
.\hello.exe
```

或一步到位：

```powershell
.\target\release\cay-run.exe hello.cay
```

---

## 基本工作流程

Cavvy 编译器工具链提供多种编译模式：

```powershell
# 完整编译：.cay → .exe
cayc input.cay -o output.exe

# 仅生成 LLVM IR（便于调试）
cay-ir input.cay -o output.ll

# 仅检查语法和语义（不生成代码）
cay-check input.cay

# IR → 可执行文件
ir2exe input.ll -o output.exe

# 预处理器输出
cay-pre input.cay -o output_preprocessed.cay
```

### 编译流水线（内部流程）

```
.cay 源码
  → 预处理器（#include, #define, #ifdef 展开）
  → 词法分析器（基于 logos 的分词）
  → 解析器（递归下降，生成 AST）
  → 语义分析（类型检查、符号解析）
  → IR 生成（自定义 SSA IR）
  → LLVM IR 文本生成
  → clang（捆绑）→ 原生机器码 .exe
```

详尽的架构说明见[编译器架构文档](compiler-architecture.md)。

---

## 第一个程序

```cay
class Calculator {
    static int add(int a, int b) {
        return a + b;
    }

    static int factorial(int n) {
        if (n <= 1) {
            return 1;
        }
        return n * factorial(n - 1);
    }

    static void main() {
        int x = 10;
        int y = 20;
        println("x + y = " + String.valueOf(add(x, y)));
        println("factorial(5) = " + String.valueOf(factorial(5)));

        // 字符串方法
        string msg = "Hello, World!";
        println("字符串长度: " + String.valueOf(msg.length()));
        println("大写: " + msg.toUpperCase());

        // 数组
        int[] arr = new int[5];
        for (int i = 0; i < 5; i = i + 1) {
            arr[i] = i * i;
        }

        int sum = 0;
        int j = 0;
        while (j < 5) {
            sum = sum + arr[j];
            j = j + 1;
        }
        println("数组求和: " + String.valueOf(sum));
    }
}
```

---

## 运行测试

```powershell
# 必须先构建 release 版本（测试以子进程调用 release 编译器二进制文件）
cargo build --release
cargo test --release --verbose
```

测试分布在两个位置：
- `src/lib.rs` — 少量内联的 `#[cfg(test)]` 单元测试（词法分析器、解析器、预处理器）
- `tests/*.rs` — 集成测试，编译 `examples/` 下的 `.cay` 文件并断言 stdout

> 集成测试会生成 `temp_*.exe`、`temp_*.ll`、`temp_*.cay` 等临时文件，被 git 忽略但会在本地累积。

---

## 下一步

- 阅读[语言概述](language-overview.md)了解 Cavvy 的核心特性
- 查阅[语言参考](language-reference.md)了解完整语法和语义
- 浏览[示例目录](../examples/)查看更多代码样例
