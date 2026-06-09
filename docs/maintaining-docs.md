# 维护文档

本文档说明如何维护 Cavvy 文档站。

---

## 原则

1. **准确性优先**：文档只描述源码中已实现或明确标注为受限的行为
2. **可测试性**：标为 `cay` 的代码块必须能被自动测试（`doc-test.py` 验证）
3. **完整性**：每个功能应有对应的文档页面
4. **时效性**：实现状态变更时同步更新 `current-status.md`

---

## 文档结构

```
docs/
├── README.md               # 文档索引（mdBook 入口重定向）
├── index.md                # mdBook 首页
├── SUMMARY.md              # mdBook 导航结构
├── getting-started.md      # 快速开始
├── language-overview.md    # 语言总览
├── language-reference.md   # 语言参考手册
├── compiler-architecture.md # 编译器架构
├── cli.md                  # CLI 工具参考
├── preprocessor.md         # 预处理器指南
├── ffi.md                  # FFI 外部函数接口
├── toolchain.md            # 工具链与构建
├── testing.md              # 测试指南
├── cavly.md                # 包管理器
├── bytecode-format.md      # CayBC 字节码格式
├── current-status.md       # 实现状态
├── maintaining-docs.md     # 本文档
```

---

## 本地预览

```powershell
# 安装 mdBook
cargo install mdbook --locked

# 本地预览文档站
mdbook serve --open

# 构建静态站点
mdbook build
```

---

## 新增页面

1. 在 `docs/` 下创建 Markdown 文件
2. 在 `docs/SUMMARY.md` 中添加导航链接
3. 确保 `cay` 标记的代码块是完整程序
4. 运行文档测试验证

```powershell
.\scripts\test-docs.ps1
```

---

## 文档测试

文档测试由 `scripts/doc-test.py` 自动执行：

- 扫描 `docs/**/*.md` 和 `README.md`
- 抽取语言标记为 `cay`、`cavvy`、`eol` 的代码块
- 默认使用 `cay-check` 进行语法检查
- `cay run` 标记的块会编译并运行
- `cay ignore` 标记的块会被跳过

---

## 写作规范

1. **代码块语言标记**：

   - `cay` — 完整程序，会被自动检查
   - `cay run` — 完整程序，会被编译并运行
   - `cay ignore` — 片段，跳过检查
   - `text`、`bash`、`powershell` — 非 Cavvy 代码
   - `cay run` — 编译并运行
2. **章节标题**：使用 ATX 标题（`#` 符号），层级不超过 4 级
3. **链接**：文档内部链接使用相对路径（如 `[架构](compiler-architecture.md)`）
4. **版本信息**：`.verinfo` 中的版本号变更后，检查 `README.md` 和 `index.md` 是否需要更新

---

## CI/CD

`.github/workflows/docs.yml`（或 `jekyll-gh-pages.yml`）：

- 推送到 `main` 分支时自动构建 mdBook
- 部署到 GitHub Pages
- 文档测试在 Windows 环境中执行（编译器工具链验证）

---

## 常见问题

### 文档中的代码块无法编译

1. 确认代码块是完整程序（包含 `Main` 类和 `main` 方法）
2. 确认使用了正确的语言标记
3. 运行 `python scripts/doc-test.py --build` 查看具体错误

### 新增页面不显示在导航中

检查 `docs/SUMMARY.md` 是否添加了对应的链接条目。
