# Cavvy 5.2.0 破坏性变更

本文档列出从 5.1.x 升级到 5.2.0 时需要注意的兼容性变更。

---

## 1. cayError / cayResult 重命名为 CayError / CayResult

**变更**: 核心错误类型和结果类型已从 `cayError` / `cayResult`（小驼峰）重命名为 `CayError` / `CayResult`（PascalCase），与 Rust 命名规范保持一致。

**影响**: 所有引用 `cayError` 或 `cayResult` 的代码需要更新。

**迁移方式**:

```diff
- use cavvy::error::cayError;
+ use cavvy::miette_diagnostic::CayError;

- type Result<T> = cayResult<T>;
+ type Result<T> = CayResult<T>;
```

**相关 Commit**: `c567ff9`

---

## 2. CayError 变体现在需要显式指定 error_code

**变更**: 每个 `CayError` 变体新增 `error_code: &'static str` 字段，构造时必须显式提供错误码。

**影响**: 所有直接构造 `CayError` 的地方需要添加 `error_code` 参数。旧版本中依赖 `message.contains()` 字符串匹配的错误判断逻辑需要更新为 error_code 匹配。

**迁移方式**:

```diff
- let err = cayError::TypeMismatch { expected: "int", found: "String" };
+ let err = CayError::TypeMismatch { expected: "int", found: "String", error_code: "E0001" };
```

**错误码匹配代替字符串匹配**:

```diff
- assert!(format!("{}", err).contains("type mismatch"));
+ assert_eq!(err.error_code, "E0001");
```

**相关 Commit**: `c567ff9`

---

## 3. DiagnosticCollector 已删除

**变更**: `DiagnosticCollector` 结构体和相关 API 已被完全删除。测试用例使用 `Vec<CayError>` 替代。

**影响**: 所有使用 `DiagnosticCollector` 的测试代码需要迁移。

**迁移方式**:

```diff
- let collector = compile_eol_expect_error("test.cay");
- assert!(collector.has_error_containing("type mismatch"));
+ let errors: Vec<CayError> = compile_eol_expect_error("test.cay");
+ assert!(errors.iter().any(|e| e.error_code == "E0001"));
```

**相关 Commit**: `c567ff9`

---

## 4. `print_diagnostics` 不再写入 debug 文件

**变更**: `print_diagnostics` 函数移除了文件写入逻辑，现在仅进行内存处理和终端输出。

**影响**: 任何依赖诊断系统写入 debug 文件进行分析的工作流需要调整。

**相关 Commit**: `c567ff9`

---

## 5. 错误/诊断模块导入路径变更

**变更**: 所有错误和诊断相关的导入从 `error` 和 `diagnostic` 模块迁移到 `miette_diagnostic` 模块。

**影响**: 导入路径需要更新。

**迁移方式**:

```diff
- use cavvy::error::CayError;
- use cavvy::diagnostic::print_diagnostics;
+ use cavvy::miette_diagnostic::{CayError, print_diagnostics};
```

**相关 Commit**: `7157da3`

---

## 6. 增量编译已全局关闭

**变更**: `Cargo.toml` 中新增配置全局关闭 Rust 增量编译（`incremental = false`）。

**影响**: 重复编译速度可能略有下降（因为不再利用增量缓存）。这是为了修复 Windows 平台上的多线程编译竞态错误。

**相关 Commit**: `0dd32b2`

---

## 7. `llc`/`lld` 选项名称变更

**变更**: 之前版本中的 `--use-llc-lld` 选项在部分工具有不同的命名方式，本版本统一为 `--use-embedded-llc`。

**影响**: 如果使用了旧的选项名称，需要更新。

**相关 Commit**: `e665ab9`
