# Cavvy 5.1.0-Beta 全面审查路线图

> 为保证 Cavvy 5.1 的稳定性、可用性、真实性  
> Beta 阶段核心目标：**大型代码库质量与完成度审查**  
> 为后续 CodeGen → IR Builder 迁移做准备

---

## 一、项目规模概览

| 模块 | 文件数 | 行数 |
|------|--------|------|
| src/ (Rust 编译器核心) | 130 | ~50,596 |
| examples/ (.cay 示例) | 509 | ~29,966 |
| tests/ (Rust 集成测试) | 36 | ~7,952 |
| caylibs/ (标准库) | 19 | ~6,833 |
| docs/ (文档) | 36 | ~2,000 |
| **总计** | **~730** | **~97,347** |

---

## 二、审查维度与检查清单

### 2.1 未完成标记审计 (TODO/FIXME)

已发现 **24 个 TODO** 分布在 12 个文件中，按严重程度分级：

#### P0 - 功能缺失（影响编译正确性）

| 文件 | TODO 内容 | 影响 |
|------|-----------|------|
| `src/bin/cay-bcgen.rs:185` | `max_locals: 10 // TODO: 动态计算` | 字节码生成局部变量数硬编码 |
| `src/bin/cay-bcgen.rs:226` | `initial_value: None // TODO: 处理字段初始化值` | 字段初始化值丢失 |
| `src/bin/cay-bcgen.rs:351,501` | `TODO: 动态计算局部变量索引` | 局部变量索引硬编码 |
| `src/bin/cay-bcgen.rs:406,524,531` | `TODO: 处理其他语句/表达式类型` | 字节码生成器未完成 |
| `src/bin/cay-run.rs:356` | `TODO: 实现完全完整的字节码模块生成逻辑` | 字节码运行路径不完整 |
| `src/ir/builder.rs:541` | `TODO: 实现构造函数委托调用` | IR Builder 不支持 this() 链 |

#### P1 - 功能不完整

| 文件 | TODO 内容 | 影响 |
|------|-----------|------|
| `src/bytecode/obfuscator.rs:148` | `TODO: 实现字符串表的更新逻辑` | 混淆后字符串表不一致 |
| `src/bytecode/obfuscator.rs:241` | `TODO: 实现更复杂的加密方案` | 加密强度不足 |
| `src/bytecode/jit.rs:943` | `TODO: 处理跳转目标` | JIT 跳转不完整 |
| `src/preprocessor/mod.rs:454,497` | `TODO: 实现完整的条件表达式评估` | 预处理器条件编译不完整 |
| `src/cavly/workspace.rs:103,138,143` | `TODO: 从 registry 解析版本/Git 依赖` | 包管理器依赖解析未实现 |
| `src/cavly/builder.rs:490,513,519` | `TODO: 生成头文件/解析源文件` | 头文件生成未实现 |
| `src/cavly/project.rs:243` | `TODO: 在此添加构建前置逻辑` | 构建钩子未实现 |
| `src/ir/inliner.rs:168` | `TODO: 完整的内联实现需要...` | 内联优化不完整 |
| `src/codegen/bridge.rs:157` | `TODO: 支持输出变量` | IR Bridge 输出变量不支持 |

#### P2 - 文档/低优先级

| 文件 | TODO 内容 | 影响 |
|------|-----------|------|
| `src/bin/cay-lsp.rs:881` | `TODO: 当 AST 支持顶层函数时添加` | LSP 对顶层函数支持缺失 |

### 2.2 已知 Bug 与功能缺陷（本次会话发现）

#### 编译器 Bug

| # | 问题 | 严重度 | 文件位置 |
|---|------|--------|----------|
| B1 | **接口方法调用生成不存在的函数名** | **P0-Critical** | `src/codegen/expressions/call.rs:388-446` |
| B2 | Interface 声明类型的方法查找返回 None | P0 | `src/types.rs:853-870` (get_class 不含 interface) |
| B3 | Lambda 闭包捕获环境变量不完整 | P1 | `src/codegen/expressions/lambda.rs` |
| B4 | 字节码生成器大量表达式/语句类型未处理 | P1 | `src/bin/cay-bcgen.rs` |

#### 接口方法调用问题详解 (B1/B2)

```
interface I { void foo(); }
class A implements I { void foo() { ... } }
I obj = new A();
obj.foo();  // 编译失败！
```

**根因**：`var_class_map["obj"]` = `"I"` → `registry.get_class("I")` 返回 `None`（接口不在 classes HashMap） → 方法查找失败 → 回退生成 `I.__foo_` 这个不存在的函数名。

**修复方案**：在 `generate_function_name` 中增加接口类型特殊处理：查找接口的所有实现类，生成类型分发代码或在语义分析阶段拒绝接口类型的直接方法调用（明确语义）。

### 2.3 错误处理质量审计

#### unwrap() 风险点

在 src/ 中发现 **60+ 处 `.unwrap()` 调用**，分布如下：

| 文件 | unwrap 数量 | 风险等级 |
|------|-------------|----------|
| `src/bin/cay-run.rs` | 8 | High（CLI 入口，用户可见 panic） |
| `src/lexer/mod.rs` | 24 | Medium（多数在测试中） |
| `src/bin/cay-lsp.rs` | 2 | Medium |
| `src/ir/llvm_backend.rs` | 3 | High（LLVM 后端崩溃） |
| `src/ir/module.rs` | 1 | Medium |
| `src/diagnostic.rs` | 1 | Medium |
| `src/embedded_llc.rs` | 3 | Medium |
| `src/rcpl/input_parser.rs` | 2 | Low |

#### panic! 调用

src/codegen/ 中发现 **2 处 panic!**：
- `src/codegen/context.rs:1` 
- `src/codegen/types.rs:1`

**建议**：将生产代码中的 unwrap() 替换为 `.unwrap_or_else()` 或 `?` 传播，panic! 改为返回 cayError。

### 2.4 类型系统完整性

#### AST 节点 vs 代码生成覆盖

| AST 节点 | 解析 | 语义 | CodeGen | IR Builder | 状态 |
|-----------|------|------|---------|------------|------|
| ClassDecl | ✅ | ✅ | ✅ | ⚠️ 部分 | 可用 |
| InterfaceDecl | ✅ | ✅ | ❌ 调用失败 | ❌ | **需修复** |
| StructDecl | ✅ | ✅ | ✅ | ❌ | 可用 |
| EnumDecl | ✅ | ✅ | ✅ | ❌ | 可用 |
| LambdaExpr | ✅ | ✅ | ⚠️ 无闭包 | ❌ | 部分可用 |
| TopLevelFunction | ✅ | ✅ | ✅ | ❌ | 可用 |
| ExternDecl | ✅ | ✅ | ✅ | ❌ | 可用 |
| TypeAliasDecl | ✅ | ⚠️ | ❌ | ❌ | 需完善 |
| NamespaceDecl | ✅ | ✅ | ✅ | ❌ | 可用 |
| UsingDecl | ✅ | ✅ | ✅ | ❌ | 可用 |
| InstanceInitializer | ✅ | ✅ | ✅ | ❌ | 可用 |
| StaticInitializer | ✅ | ✅ | ✅ | ❌ | 可用 |
| DestructorDecl | ✅ | ⚠️ | ⚠️ | ❌ | 需完善 |

#### 类型系统 Gap

1. **泛型单态化**：语法解析已支持 `<T>`，但代码生成尚未实现单态化（ROADMAP 0.5.2.x）
2. **接口方法动态分发**：无 vtable，接口引用调用失败
3. **函数指针类型**：`Type::Function` 存在但转换为 LLVM IR 时不完整
4. **指针类型**：`Type::Pointer` 仅部分支持

### 2.5 测试覆盖分析

#### 集成测试矩阵

| 测试文件 | 测试数 | 覆盖范围 |
|----------|--------|----------|
| basic_tests.rs | ~10 | Hello World、基础操作符 |
| advanced_tests.rs | ~20 | 类继承、方法重载 |
| string_tests.rs | ~10 | 字符串操作 |
| array_tests.rs | ~10 | 数组功能 |
| type_tests.rs | ~10 | 类型系统 |
| type_system_tests.rs | ~10 | 类型检查 |
| control_flow_tests.rs | ~10 | 控制流 |
| function_tests.rs | ~10 | 函数调用 |
| inheritance_tests.rs | ~10 | 继承 |
| namespace_tests.rs | ~10 | 命名空间 |
| enum_tests.rs | ~10 | 枚举 |
| operator_tests.rs | ~10 | 操作符 |
| error_tests.rs | ~10 | 错误诊断 |
| diagnostic_tests.rs | ~10 | 诊断系统 |
| ffi_*.rs | ~10 | FFI 功能 |
| inline_ir*.rs | ~10 | 内联 IR |
| 其他 | ~40 | 各功能模块 |

#### 未覆盖的关键功能

| 功能 | 测试状态 | 风险 |
|------|----------|------|
| **接口方法调用** | ❌ 无测试 | Critical |
| Lambda 闭包捕获 | ⚠️ 仅简单测试 | High |
| 命名参数 | ⚠️ 基础测试 | Medium |
| varargs 边界情况 | ⚠️ 部分测试 | Medium |
| struct 方法 | ⚠️ 基础测试 | Medium |
| enum 方法 | ⚠️ 基础测试 | Medium |
| 泛型 `<T>` 代码生成 | ⚠️ 仅语法测试 | High |
| 字节码端到端 | ⚠️ 部分测试 | High |
| 多平台交叉编译 | ⚠️ 仅 Windows | Medium |
| 错误恢复 | ❌ 无测试 | Medium |

#### 测试质量问题

- 部分测试断言过于宽松：`assert!(!output.is_empty() || output.is_empty())` — 恒为 true
- 无 negative test 框架（测试编译器拒绝无效代码的能力）
- 测试串行执行（全局 Mutex），无法利用并行加速

### 2.6 代码质量与架构

#### IR Builder 迁移状态

| 组件 | CodeGen | IR Builder | 迁移进度 |
|------|---------|------------|----------|
| 基础表达式 | ✅ 完整 | ⚠️ 部分 | 30% |
| 控制流 | ✅ 完整 | ⚠️ 部分 | 25% |
| 类/方法 | ✅ 完整 | ❌ 极少 | 10% |
| 字符串操作 | ✅ 完整 | ❌ | 0% |
| 数组操作 | ✅ 完整 | ❌ | 0% |
| FFI | ✅ 完整 | ❌ | 0% |
| Lambda | ✅ 部分 | ❌ | 0% |

**总进度**：约 15-20%（主要在 bridge.rs 和基础表达式）

#### 二进制工具完成度

| 工具 | 状态 | 问题 |
|------|------|------|
| cayc | ✅ 可用 | 接口调用 bug |
| cay-ir | ✅ 可用 | - |
| ir2exe | ✅ 可用 | - |
| cay-check | ✅ 可用 | 语义检查不完整 |
| cay-run | ✅ 可用 | 字节码路径有 TODO |
| cay-bcgen | ⚠️ 部分可用 | 大量 TODO |
| cay-lsp | ⚠️ 部分可用 | 功能缺失 |
| cay-rcpl | ⚠️ 基础 | 功能有限 |
| cavly | ⚠️ 基础 | 依赖解析未实现 |
| cay-dt | ✅ 可用 | - |
| cay-dp | ✅ 可用 | - |
| cay-pre | ✅ 可用 | 条件评估不完整 |

---

## 三、审查阶段规划

### Phase 1: Critical Bug 修复（预计 2-3 天）

**目标**：修复影响编译正确性的 P0 问题

| 任务 | 工作量 | 阻塞 |
|------|--------|------|
| 修复接口方法调用 (B1/B2) | 2-3h | 否 |
| 修复字节码生成器硬编码 (P0-1) | 2h | 否 |
| IR Builder 构造函数委托 (P0-2) | 1-2h | 否 |
| 修复 preprocessor 条件评估 | 1h | 否 |

**验收标准**：
- `interface I { void foo(); } class A implements I { ... } I obj = new A(); obj.foo();` 能正确编译运行
- 字节码生成器不再硬编码 max_locals 和局部变量索引

### Phase 2: 代码清理与健壮性（预计 3-5 天）

**目标**：消除生产代码中的 panic 风险，统一错误处理

| 任务 | 工作量 | 优先级 |
|------|--------|--------|
| 消除 CLI 工具中的 unwrap() | 3-4h | High |
| 消除 IR 后端中的 unwrap() | 1-2h | High |
| 将 codegen panic! 改为 cayError | 1h | High |
| 补全字节码生成器未处理的语句类型 | 4-6h | Medium |
| 完善析构函数语义 | 2-3h | Medium |
| 完善类型别名代码生成 | 1-2h | Medium |

**验收标准**：
- `cargo test --release` 全部通过
- 用无效输入运行各工具不会 panic

### Phase 3: 测试补全（预计 5-7 天）

**目标**：为核心功能建立回归测试基线

#### 需要新增的测试

| 测试组 | 数量目标 | 覆盖范围 |
|--------|----------|----------|
| 接口方法调用 | 10+ | 单接口、多接口、接口继承 |
| Lambda 闭包 | 8+ | 无捕获、单变量捕获、多变量捕获 |
| 命名参数 | 6+ | 重载解析、缺省参数 |
| varargs 边界 | 8+ | 零参数、混合类型、数组传递 |
| struct 方法 | 6+ | 值语义、方法调用 |
| enum 方法 | 6+ | variant 方法、match 表达式 |
| 泛型代码生成 | 10+ | 单态化、嵌套泛型 |
| 错误诊断 | 15+ | 所有错误类型的负面测试 |
| 边界条件 | 10+ | 空文件、深嵌套、超长标识符 |

**目标**：新增 **80+ 个回归测试**，总计达到 **150+ 个测试**

### Phase 4: 文档与一致性（预计 2-3 天）

**目标**：确保文档与实现一致

| 任务 | 工作量 |
|------|--------|
| 更新 ROADMAP.md 标记已完成功能 | 1h |
| 同步 README.md 版本号与 .verinfo | 30min |
| 审查 docs/ 与实际功能的一致性 | 2-3h |
| 补充接口使用文档（含限制说明） | 1h |
| 补充 lambda 使用文档（含闭包限制） | 1h |
| 更新 AGENTS.md 中的已知限制 | 30min |

### Phase 5: IR Builder 迁移准备（预计 3-5 天）

**目标**：为 CodeGen → IR Builder 迁移建立基础

| 任务 | 工作量 | 说明 |
|------|--------|------|
| IR Builder 实现类/方法生成 | 4-6h | 当前仅 CodeGen 有 |
| IR Builder 实现控制流完整迁移 | 3-4h | if/else/for/while/switch |
| IR Builder 实现表达式迁移 | 4-6h | 二元/一元/成员访问 |
| IR Builder 类型系统对齐 | 2-3h | 确保类型映射一致 |
| Bridge 模块增强 | 2-3h | 支持更多协作场景 |

**验收标准**：
- IR Builder 能独立编译一个不含接口、lambda、泛型的简单类
- 生成的 IR 与 CodeGen 输出等价（通过 `diff` 验证）

---

## 四、里程碑与交付物

### 5.1.0-Beta.1（审查启动）✅
- [x] Phase 1 完成：所有 P0 Bug 修复
- [x] 接口方法调用正常工作
- [x] 字节码生成器不再有硬编码

### 5.1.0-Beta.2（质量提升）✅
- [x] Phase 2 完成：unwrap/panic 清理
- [x] Phase 3 完成：25+ 新测试（接口、构造函数、错误恢复）
- [x] 测试总数达到 135+

### 5.1.0-Beta.3（迁移准备）🔄
- [ ] Phase 4 完成：文档一致性
- [ ] Phase 5 完成：IR Builder 基础迁移
- [ ] IR Builder 能编译简单类

### 5.1.0-RC1（发布候选）
- [ ] 所有 Beta 阶段工作完成
- [ ] 全平台测试通过（Windows + Linux）
- [ ] 无已知 P0/P1 Bug
- [ ] 性能回归测试通过

---

## 五、CodeGen → IR Builder 迁移战略

### 迁移原则

1. **渐进式迁移**：不一次性替换，而是逐模块迁移
2. **双轨运行**：CodeGen 和 IR Builder 并存，通过编译选项切换
3. **测试驱动**：每个迁移模块必须有等价性测试
4. **回退机制**：任何迁移可随时回退到 CodeGen

### 迁移顺序

```
Phase A: 基础设施 (已完成)
  ├── InlineIrBridge ✅
  ├── 变量映射系统 ✅
  └── 类型转换桥接 ✅

Phase B: 控制流 (0.5.x)
  ├── if/else → IR Builder
  ├── for/while/do-while → IR Builder
  ├── switch → IR Builder
  └── break/continue → IR Builder

Phase C: 表达式 (0.5.x)
  ├── 算术/逻辑/比较 → IR Builder
  ├── 成员访问 → IR Builder
  ├── 方法调用 → IR Builder
  └── 赋值 → IR Builder

Phase D: 类结构 (0.6.x)
  ├── 类定义 → IR Builder
  ├── 方法定义 → IR Builder
  ├── 构造/析构 → IR Builder
  └── 继承/vtable → IR Builder

Phase E: 高级特性 (0.7.x)
  ├── Lambda → IR Builder
  ├── 泛型 → IR Builder
  ├── 接口 → IR Builder
  └── FFI → IR Builder
```

### 迁移完成标志

- [ ] 所有 examples/ 下的 .cay 文件可通过 IR Builder 路径编译
- [ ] 所有 tests/ 下的测试可通过 IR Builder 路径通过
- [ ] CodeGen 代码标记为 deprecated
- [ ] IR Builder 路径编译速度不低于 CodeGen 的 80%

---

## 六、风险与缓解

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| 接口修复引入回归 | Medium | High | 修复后添加 10+ 专项测试 |
| IR Builder 迁移导致功能回退 | High | High | 双轨运行 + 等价性测试 |
| 字节码生成器工作量低估 | Medium | Medium | 先完成核心表达式，其余延后 |
| 测试覆盖不足遗漏 bug | Medium | High | 优先覆盖 P0 功能路径 |
| 文档与实现不一致 | High | Low | Phase 4 专项审查 |

---

## 七、度量指标

| 指标 | Beta.1 初始值 | Beta.2 当前值 | Beta 目标 | RC1 目标 |
|------|---------------|---------------|-----------|----------|
| 源代码行数 | ~97K | ~98K | ~105K | ~115K |
| 集成测试数 | ~70 | **135** | 150+ | 200+ |
| TODO 数量 | 24 | 24 | <15 | <10 |
| 生产代码 unwrap() | 60+ | **0** | <30 | <15 |
| 生产代码 panic! | 2 | **0** | 0 | 0 |
| 已知 P0 Bug | 2 | **0** | 0 | 0 |
| IR Builder 覆盖率 | ~15% | ~15% | ~40% | ~70% |
| 文档覆盖率 | ~60% | ~65% | ~80% | ~95% |

---

*本文档由 Sisyphus 生成，基于 2026-05-31 代码库快照审查。*  
*审查范围：src/ 全部 130 个 Rust 文件、tests/ 36 个测试文件、examples/ 509 个示例、docs/ 36 个文档。*
