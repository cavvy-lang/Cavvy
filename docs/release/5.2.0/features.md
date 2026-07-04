# Cavvy 5.2.0 新特性详解

## 目录

- [诊断系统重构](#诊断系统重构)
- [嵌入式 LLVM 工具链增强](#嵌入式-llvm-工具链增强)
- [泛型系统巩固](#泛型系统巩固)
- [REPL 与输入解析增强](#repl-与输入解析增强)
- [运行时库自动构建](#运行时库自动构建)
- [新子命令工具](#新子命令工具)
- [基础设施](#基础设施)

---

## 诊断系统重构

### 1. CayError 直接实现 miette::Diagnostic

**核心改动**（76 个文件，+1336/-2566 行）：

5.2.0 对错误诊断系统进行了彻底的扁平化重构。此前，一个编译错误需要经过四层包装转换：`CompilerError` → `DisplayDiagnostic` → 旧 `Diagnostic` → 旧 `DiagnosticCollector` 才能到达用户界面。现在 `CayError` 直接实现 `miette::Diagnostic`，所有中间层全部删除。

```
之前:  cayError → CompilerError → DisplayDiagnostic → Diagnostic → miette 输出
之后:  CayError ─────────────────────────────────────────────→ miette 输出 (直接)
```

**关键改进**：

| 方面 | 之前 | 之后 |
|------|------|------|
| 类型名称 | `cayError` / `cayResult`（小驼峰） | `CayError` / `CayResult`（PascalCase） |
| 错误识别 | `message.contains()` 字符串匹配 | 显式 `error_code` 字段，构造时指定 |
| 诊断链 | 4 层包装转换 | CayError 直接实现 miette::Diagnostic |
| 文件写入 | `print_diagnostics` 写入 debug 文件 | 纯内存处理，无副作用 |
| 测试断言 | `DiagnosticCollector` 收集 | `Vec<CayError>` 直接断言 |
| 代码量 | ~2566 行 | ~1336 行（减少 48%） |

**错误码示例**：

每个 `CayError` 变体现在携带一个显式的 `error_code` 字段：

```rust
// 之前：必须通过字符串包含判断
assert!(err.to_string().contains("type mismatch"));

// 之后：通过 error_code 精确匹配
assert_eq!(err.error_code, "E0001");
```

**测试迁移**：

所有集成测试从 `DiagnosticCollector` 迁移到 `Vec<CayError>`，断言更直接、更可靠：

```rust
// 之前
let collector = compile_eol_expect_error("test.cay");
assert!(collector.has_error_containing("type mismatch"));

// 之后
let errors: Vec<CayError> = compile_eol_expect_error("test.cay");
assert!(errors.iter().any(|e| e.error_code == "E0001"));
```

---

### 2. 模块路径统一

将所有错误、诊断相关的导入从 `error` 和 `diagnostic` 模块统一迁移到 `miette_diagnostic` 模块，完成项目诊断系统的整合。

---

## 嵌入式 LLVM 工具链增强

### 1. `--use-embedded-llc` 选项扩展

在 5.1.0 引入的 `--use-llc-lld` 基础上，本版本将其重命名为 `--use-embedded-llc` 并扩展支持到更多子命令：

| 工具 | 新增选项 | 说明 |
|------|----------|------|
| `cayc` | `--use-embedded-llc` | 使用内置 llc+lld 替代 clang（已有） |
| `cay-ir` | `--use-embedded-llc` | IR 生成后直接编译（已有） |
| `cay-rcpl` | `--use-embedded-llc` | **新增**：交互式环境中使用嵌入式工具链 |
| `cay-run` | `--use-embedded-llc` | **新增**：编译运行时使用嵌入式工具链 |

### 2. ir2exe 嵌入式链接全面修复

修复了嵌入式链接模式下的一系列链接问题：

- **系统库搜索路径**: 为 `ld.lld` 添加默认系统库搜索路径
- **C 运行时启动文件**: 自动检测并添加 crt 启动文件，提供正确的入口点
- **动态链接器**: 补充链接 `dl` 库并自动设置动态链接器路径
- **链接参数顺序**: 调整链接参数顺序确保符号解析正确

### 3. LLVM C API 内存安全修复

修复了 Windows 环境下调用 `LLVMDisposeMessage` 导致访问冲突的崩溃问题。由于 LLVM C API 在 Windows 上对 `LLVMDisposeMessage` 的实现存在兼容性问题（某些版本中返回的字符串不是由 `malloc` 分配的），此版本将该调用注释掉并添加了详细注释说明原因。

---

## 泛型系统巩固

### 1. 全局泛型类型替换函数

新增 `substitute_type_params` 工具函数，提供全局统一的泛型类型替换能力：

```rust
// 将所有泛型参数 T, U 替换为实际类型
let substituted = substitute_type_params(&generic_type, &type_args);
```

### 2. 递归嵌套泛型替换

重构泛型返回类型解析逻辑，支持递归替换嵌套泛型。例如 `Pair<Box<int>, String>` 中的嵌套泛型现在能被正确展开。

### 3. 接口 vtable 泛型后缀支持

修复接口 vtable 查找时未处理泛型后缀的问题。当接口方法包含泛型参数时，vtable 槽位名称现在能正确包含泛型后缀信息，确保动态分发在泛型接口下正常工作。

### 4. 调用方法返回值泛型替换

为调用方法自动替换返回值中的泛型参数，确保泛型方法的调用方获得正确的具体类型。

---

## REPL 与输入解析增强

### 1. 访问修饰符支持

RCPL 输入解析器新增对 `pub` 和 `priv` 修饰符的解析支持，允许在交互式环境中定义带访问控制的类和成员：

```cay ignore
>> pub class MyClass {
..     priv int x;
..     pub int getX() { return this.x; }
.. }
```

### 2. `fn` 风格方法定义

扩展输入解析器支持 `fn` 关键字风格的方法定义，与文件中的语法保持一致：

```cay ignore
>> pub fn hello() -> void {
..     println("Hello from REPL!");
.. }
```

### 3. 帮助信息排版优化

优化了 `cay-rcpl` 和 `cay-run` 的帮助信息排版对齐，提升命令行交互体验。

---

## 运行时库自动构建

### 自动编译 `libcayrt-linux.a`

`ir2exe` 现在能够在 Linux 平台上自动检测并编译 Cavvy 运行时库：

- 当目标平台为 Linux 且缺失对应运行时库时，自动执行构建脚本
- 无需手动预编译运行时库，简化跨平台使用体验
- 编译产物缓存，避免重复构建

---

## 新子命令工具

### 1. cay-ast（AST 可视化工具）

输出解析后的抽象语法树（AST），支持格式化显示和 JSON 格式导出：

```bash
cay-ast program.cay          # 树状格式输出
cay-ast program.cay --json   # JSON 格式输出
```

### 2. cay-pl（预处理输出工具）

展示预处理后的源代码，便于调试宏展开和条件编译：

```bash
cay-pl program.cay           # 输出预处理后源码
cay-pl program.cay --lines   # 带行号输出
```

### 3. cay-sir（语义 IR 工具）

展示语义分析后的中间表示，帮助理解类型解析和符号绑定：

```bash
cay-sir program.cay          # 输出语义 IR
```

### 4. 类型序列化支持

为核心 AST/IR 数据结构添加 `Serialize` 派生支持，所有三个新工具均支持 `--json` 选项输出结构化数据，便于与其他工具集成。

---

## 基础设施

### 1. 增量编译关闭

全局关闭 Rust 增量编译，修复 Windows 环境下多线程编译导致的竞态错误。增量编译在大型项目中虽然能加速重编译，但在 Windows 上会因文件锁定竞争导致偶发编译失败。

### 2. 版本号升级

所有 14 个组件版本号从 5.1.x 系列统一升级到 **5.2.0**，构建号同步更新：

| 组件 | 版本 | 构建号 |
|------|------|--------|
| cayc / cay-ir / ir2exe | 5.2.0 | 47 |
| cay-check | 5.2.0 | 39 |
| cay-run | 5.2.0 | 23 |
| cavly | 5.2.0 | 13 |
| cay-pre / cay-bcgen / cay-dt / cay-dp / cay-rcpl | 5.2.0 | 12 |
| cay-setup | 5.2.0 | 2 |
| cay-ast / cay-pl / cay-sir | 5.2.0 | 2 |

### 3. 迭代器协议完成

标记 ROADMAP 中的迭代器协议为已完成状态，相关实现已在之前的版本中落地。

### 4. 调试模块跟踪修复

取消忽略并正确跟踪 `debug_common` 模块，确保该模块参与版本控制和 CI 构建。
