# Cavvy 5.2.0 迁移指南

本指南帮助开发者从 Cavvy 5.1.x 升级到 5.2.0。

---

## 快速检查清单

- [ ] 全局搜索 `cayError` 替换为 `CayError`
- [ ] 全局搜索 `cayResult` 替换为 `CayResult`
- [ ] 更新所有 `CayError` 构造调用，添加 `error_code` 字段
- [ ] 替换所有 `message.contains()` 错误断言为 `error_code` 匹配
- [ ] 将 `DiagnosticCollector` 测试迁移到 `Vec<CayError>`
- [ ] 更新导入路径：`error`/`diagnostic` → `miette_diagnostic`
- [ ] 更新 CLI 参数：`--use-llc-lld` → `--use-embedded-llc`（如果使用过）
- [ ] 启用 `cargo build --release` 完整重编译（因增量编译已关闭）

---

## 逐步迁移

### 第一步：更新类型名称

全局替换：

| 旧名称 | 新名称 |
|--------|--------|
| `cayError` | `CayError` |
| `cayResult` | `CayResult` |
| `cayError::*` | `CayError::*` |

```bash
# 使用 sed 或类似工具进行批量替换
find . -name "*.rs" -exec sed -i 's/\bcayError\b/CayError/g' {} +
find . -name "*.rs" -exec sed -i 's/\bcayResult\b/CayResult/g' {} +
```

### 第二步：更新导入路径

```diff
- use cavvy::error::*;
+ use cavvy::miette_diagnostic::*;
```

主要的公开 API 重新导出保持不变，如果之前使用的是 `cavvy::error::CayError`，只需更改为 `cavvy::miette_diagnostic::CayError` 即可。

### 第三步：更新错误构造

查找所有 `CayError::` 的构造调用，为每个变体添加 `error_code` 字段。错误码格式为 `E` 加四位数字（如 `E0001`、`E0002`）。

可根据错误类型按以下规则选择 error_code（推荐）：

| 变体类型 | error_code 前缀 |
|---------|----------------|
| 类型错误 | `E0001` - `E0099` |
| 语法错误 | `E0100` - `E0199` |
| 语义错误 | `E0200` - `E0399` |
| 内部错误 | `E1000`+ |

### 第四步：更新测试断言

```diff
// 旧方式：字符串匹配
- let collector = compile_eol_expect_error("test.cay");
- assert!(collector.has_error_containing("type mismatch"));

// 新方式：error_code 匹配
+ let errors: Vec<CayError> = compile_eol_expect_error("test.cay");
+ assert!(errors.iter().any(|e| e.error_code == "E0001"));
```

### 第五步：检查 CLI 脚本

如果 CI 脚本或其他自动化流程中使用了 `--use-llc-lld` 参数，请更新为 `--use-embedded-llc`。

---

## 回滚方案

如果需要临时回退到 5.1.x，请执行：

```bash
git checkout v5.1.0  # 或 5.1.1 之前的分支
cargo build --release
```

注意：5.2.0 的诊断系统变更涉及 76 个文件，回退后如果切换回来需要完整重编译。
