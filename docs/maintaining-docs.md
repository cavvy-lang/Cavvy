# 维护文档

## 原则

文档只描述源码中已经实现或清楚标注为受限的行为。示例代码必须能被自动测试；片段、伪代码和错误示例使用 `text`，不要标为 `cay`。

## 本地流程

```powershell
.\scripts\test-docs.ps1
mdbook build
```

## 新增页面

1. 在 `docs/` 下添加 Markdown 文件。
2. 在 `docs/SUMMARY.md` 中加入导航。
3. 标为 `cay` 的代码块必须是完整程序。
4. 运行文档测试。

## GitHub Pages

`.github/workflows/docs.yml` 会在推送到 `main` 时构建 mdBook 并部署到 GitHub Pages。文档测试在 Windows job 中执行，因为 release 编译器和捆绑工具链主要按 Windows 路径验证。
