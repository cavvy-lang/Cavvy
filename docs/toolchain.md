# 工具链与构建指南

---

## 构建编译器

### Release 是正常模式

测试、示例运行和日常使用都依赖 `target/release` 下的编译器二进制文件，因此日常构建使用 release：

```powershell
cargo build --release
```

Debug 构建仅用于开发，**不包含**捆绑工具链（`llvm-minimal/`、`mingw-minimal/`、`lib/`）：

```powershell
cargo build
```

### 构建过程

`build.rs` 在构建时会：
1. 读取 `.verinfo` 获取版本号
2. 将版本号与 git 提交哈希组合
3. 设置 `CARGO_*_VERSION` 环境变量用于编译时嵌入
4. 将以下目录复制到 `target/<profile>/`：
   - `llvm-minimal/` — LLVM/clang 工具链
   - `mingw-minimal/` — MinGW 运行时
   - `lib/` — 链接库
   - `caylibs/` — 标准库
   - `examples/` — 示例程序
   - `third-party/` — 第三方依赖

### 工具链目录

如果 `llvm-minimal/`、`mingw-minimal/` 或 `lib/` 缺失（新克隆仓库），需要先下载：

```powershell
python setup-llvm.py
```

此脚本从 GitHub `cavvy-lang/Cavvy-src-Assets` 下载 LLVM+MinGW 捆绑包，版本锁定信息从 `.verinfo` 读取。

### 交叉编译

`.cargo/config.toml` 包含 linux-musl 交叉编译配置。Windows 上可忽略此配置。

---

## 运行测试

```powershell
# 必须先构建 release 版本（测试以子进程调用 release 编译器的二进制文件）
cargo build --release
cargo test --release --verbose
```

### 测试结构

测试分布在两个位置：

**单元测试**（`src/lib.rs`）：
- 少量内联的 `#[cfg(test)]` 单元测试
- 覆盖词法分析器、解析器、预处理器

**集成测试**（`tests/*.rs`）：
- 调用 release 目录中的 `cayc` 编译 `examples/` 下的 `.cay` 文件
- 运行生成的可执行文件并断言 stdout
- 使用全局 `Mutex` 串行执行（避免临时文件冲突）
- 辅助函数位于 `tests/common/mod.rs`：`compile_and_run_eol()`、`compile_eol_expect_error()`

### 单独运行特定测试

```powershell
# 运行接口相关测试
cargo test --release --test interface_tests -- --nocapture

# 运行 Lambda 测试
cargo test --release --test lambda_tests -- --nocapture

# 运行继承测试
cargo test --release --test inheritance_tests -- --nocapture
```

### 临时文件

测试运行会在 `tests/` 和 `examples/` 目录中留下 `temp_*.exe`、`temp_*.ll`、`temp_*.cay` 等文件。这些文件被 git 忽略但会在本地累积，可随时清理。

---

## 版本管理

版本号存储在项目根目录的 `.verinfo` 文件中（类 INI 格式）：

```ini
version=5.1.0-Beta.2
```

`build.rs` 解析此文件，将版本号与当前 git 提交哈希组合，通过环境变量注入编译二进制。修改 `.verinfo` 后执行 `cargo build` 会自动重新编译。

---

## 文档站

文档站使用 mdBook：

```bash
# 安装 mdBook
cargo install mdbook --locked

# 构建文档站
mdbook build

# 本地预览
mdbook serve --open
```

- 文档源文件位于 `docs/` 目录
- 配置文件是 `book.toml`
- 输出目录是 `book/`（不提交到 git）

### 文档测试

```powershell
# 一键测试所有文档中的代码示例
.\scripts\test-docs.ps1

# 跨平台
python scripts/doc-test.py --build
```

`scripts/doc-test.py` 自动扫描 `README.md` 和 `docs/**/*.md` 中的代码块，抽取语言标记为 `cay`、`cavvy`、`eol` 的示例进行编译检查。

### 代码块标记

在文档中编写可测试的代码示例：

````markdown
<!-- 仅语法检查 -->
```cay
class Example {
    static void main() {
        println("checked");
    }
}
```

<!-- 编译并运行 -->
```cay run
public int main() {
    println("runs");
}
```

<!-- 顶层 main -->
```cay
public int main() {
    return 0;
}
```
````

---

## CI 持续集成

### 夜间构建（`.github/workflows/nb.yml`）

- 每天 UTC 02:00 触发
- 运行在 `windows-latest`
- 使用 `stable` Rust 工具链
- 目标：`x86_64-pc-windows-gnu`
- 产物命名：`eol-*`（历史遗留）
- 可通过 `skip_tests=true` 跳过测试

### GitHub Pages（`.github/workflows/jekyll-gh-pages.yml`）

- 从 `main` 分支部署文档到 GitHub Pages
- 与代码变更无关
