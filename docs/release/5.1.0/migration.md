# Cavvy 5.1.0 迁移指南

本文档提供从旧版本升级到 Cavvy 5.1.0 的详细步骤。

---

## 前置要求

### 系统要求

- **Windows**: Windows 10/11，安装 Python 3.8+
- **Linux**: Ubuntu 20.04+ 或兼容发行版
- **macOS**: 实验性支持（部分功能）

### 依赖要求

- Rust 1.78+ (推荐 1.80+)
- LLVM 22.1+ (Windows 用户可运行 `python setup-llvm.py` 自动安装)
- Python 3.8+ (用于 setup-llvm.py)

---

## 迁移步骤

### 步骤 1: 更新代码仓库

```bash
# 拉取最新代码
git pull origin main

# 更新子模块
git submodule update --init --recursive
```

### 步骤 2: 配置 LLVM (Windows)

```bash
# 自动检测并安装/配置 LLVM
python setup-llvm.py
```

Linux 用户请通过包管理器安装 LLVM：

```bash
# Ubuntu/Debian
sudo apt-get install llvm-22 llvm-22-dev clang-22 lld-22

# Arch Linux
sudo pacman -S llvm clang lld
```

### 步骤 3: 清理旧构建产物

```bash
cargo clean
```

### 步骤 4: 构建项目

```bash
cargo build --release
```

### 步骤 5: 运行测试验证

```bash
cargo test --release
```

所有测试必须通过才能确认迁移成功。

---

## 代码迁移清单

### 隐式类型转换

检查所有隐式类型转换，改为显式转换：

```cay ignore
// 修改前
int x = 42;
String s = x;  // 隐式转换

// 修改后
int x = 42;
String s = String.valueOf(x);  // 显式转换
```

### 标准库类型引用

检查所有标准库类型的使用，添加 `using` 别名：

```cay ignore
// 修改前
File f = new File("test.txt");
StringBuilder sb = new StringBuilder();

// 修改后
using File = std::File;
using StringBuilder = std::StringBuilder;

File f = new File("test.txt");
StringBuilder sb = new StringBuilder();
```

**注意**: 不支持 `using namespace std;`，必须对每个类型单独声明。

---

## 验证迁移

### 运行编译器测试

```bash
cargo test --release
```

### 验证工具链

```bash
# 检查各工具版本
cayc --version
cay-ir --version
ir2exe --version
cay-check --version
cay-run --version
cavly --version
```

### 编译示例程序

```bash
# 使用 cavly 构建示例项目
cd examples/CavvyN
cavly build
cavly run
```

---

## 常见问题

### Q: Windows 上 llvm-sys 链接失败？

A: 运行 `python setup-llvm.py` 会自动创建必要的占位符文件。如果仍失败，检查 `LLVM_SYS_221_PREFIX` 环境变量是否指向正确的 LLVM 目录。

### Q: Linux 上找不到动态链接器？

A: 确保系统已安装标准 C 库开发包：

```bash
# Ubuntu/Debian
sudo apt-get install libc6-dev

# 或安装 build-essential
sudo apt-get install build-essential
```

### Q: 旧项目使用 cavly 构建失败？

A: 检查 `Cavly.toml` 格式是否符合最新规范。可能需要更新依赖版本号。

### Q: 测试在 Windows 上通过但在 Linux 上失败？

A: 确保 Linux 环境已安装完整的 LLVM 开发包和 C 运行时库。检查 `cargo test --release` 的具体错误信息。

---

## 回滚方案

如果迁移遇到问题，可以通过以下方式回滚：

```bash
# 查看旧版本标签
git tag | grep 5.0

# 回滚到上一个稳定版本
git checkout 5.0.x

# 重新构建
cargo build --release
```

建议迁移前创建分支：

```bash
git checkout -b migration-5.1.0
git checkout main
git pull origin main
```
