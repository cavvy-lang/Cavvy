# Cavvy 编译器已知问题记录

> 本文档记录 Cavvy 编译器开发过程中遇到的所有编译器 bug 和限制。
> 每个条目应包含：问题描述、复现步骤、影响范围、临时解决方案、修复状态。

---

## 目录

- [静态方法调用解析错误](#bug-001-静态方法调用解析错误)
- [语义分析阶段缺失方法存在性检查](#bug-002-语义分析阶段缺失方法存在性检查)
- [IR编译错误源映射丢失](#bug-003-ir编译错误源映射丢失)
- [IR编译错误缺少Cavvy编译器警告](#bug-004-ir编译错误缺少cavvy编译器警告)
- [IR文件源映射信息错误](#bug-005-ir文件源映射信息错误)

---

## Bug 001: 静态方法调用解析错误

**状态**: 已修复  
**发现日期**: 2026-06-07  
**影响版本**: v5.1.0-RC.1+f65e3e0-dirty  
**严重程度**: 高

### 问题描述

编译器错误地将类内部的静态方法调用解析为 `String.contains()` 方法，而不是用户定义的静态方法。

当在类内部定义一个名为 `contains` 的静态方法，并尝试通过 `ClassName.contains()` 调用时，编译器报错：
```
[E5004] String.contains() takes 1 argument
```

### 复现代码

```cay
public class TextBuffer {
    public int search(String pattern, int startLine) {
        // ... 其他代码 ...
        if (TextBuffer.contains(text, pattern)) {  // 这里报错
            return i;
        }
    }

    private static bool contains(String text, String pattern) {
        // 实现...
    }
}
```

### 错误信息

```
× [E5004] String.contains() takes 1 argument
   ╭─[.\text_editor.cay:737:31]
736 │                 String text = line.toString();
737 │                 if (TextBuffer.contains(text, pattern)) {
    ·                               ───────┬───────
    ·                                      ╰── 代码生成错误
738 │                     return i;
   ╰────
```

### 根本原因分析

编译器的名称解析器在处理 `ClassName.methodName()` 形式的调用时，可能：
1. 未正确优先解析当前类的作用域
2. 错误地匹配到了 `String` 类的 `contains` 方法
3. 静态方法调用的作用域解析存在优先级问题

### 影响范围

- 任何在类内部定义与标准库方法同名静态方法的代码
- 使用 `ClassName.staticMethod()` 显式调用静态方法的模式

### 临时解决方案

将静态方法改为实例方法，或重命名方法避免与标准库方法冲突：

```cay
// 方案1: 改为实例方法
private bool contains(String text, String pattern) {
    // ...
}
// 调用: this.contains(text, pattern)

// 方案2: 重命名方法
private static bool stringContains(String text, String pattern) {
    // ...
}
// 调用: TextBuffer.stringContains(text, pattern)
```

### 修复建议

1. 修复编译器的名称解析逻辑，确保 `ClassName.method()` 优先在指定类中查找
2. 改进错误信息，区分标准库方法和用户定义方法
3. 添加静态方法调用的作用域优先级测试用例

### 修复记录

2026-06-07：已调整语义分析中的成员调用解析顺序，`ClassName.method()` 会在实例/String 方法解析前优先按静态方法解析，并添加回归测试。

---

## 记录规范

### 添加新 Bug 的模板

```markdown
## Bug XXX: 标题

**状态**: 未修复/已修复/已确认  
**发现日期**: YYYY-MM-DD  
**影响版本**: 版本号  
**严重程度**: 高/中/低

### 问题描述

简要描述问题。

### 复现代码

```cay
// 最小复现代码
```

### 错误信息

```
编译器输出
```

### 根本原因分析

分析问题的根本原因。

### 影响范围

描述影响的用户场景。

### 临时解决方案

提供绕过问题的方法。

### 修复建议

建议如何修复。
```

---

## Bug 002: 语义分析阶段缺失方法存在性检查

**状态**: 已修复  
**发现日期**: 2026-06-07  
**影响版本**: v5.1.0-RC.1+f65e3e0-dirty  
**严重程度**: 高

### 问题描述

语义分析阶段未能检测到不存在的方法调用。当代码调用一个类中不存在的方法时（如调用 `isOpen()` 而实际方法名为 `isOpened()`），编译器在语义分析阶段未报错，而是在IR生成/编译阶段才暴露错误。

### 复现代码

```cay
public class Example {
    public void test() {
        File file = new File("test.txt", FileMode.READ);
        // isOpen() 方法不存在，正确方法名是 isOpened()
        if (!file.isOpen()) {  // 语义分析应该报错但未报错
            println("Failed to open");
        }
    }
}
```

### 错误信息

错误在IR编译阶段才暴露：
```
× cavvy::tool_error: ir2exe 执行失败
   │
   │ IR→EXE编译失败
   │
  help: llc 编译失败 (exit code: 1): llc.exe: error: llc.exe: text_editor.ll:4834:21: error: use of undefined value '@_ZN3std4FileE.isOpen'
    %t11 = call i64 @_ZN3std4FileE.isOpen()
                    ^
```

### 根本原因分析

1. 语义分析阶段的方法解析可能存在漏洞
2. 可能未正确处理通过 `new` 创建的对象的方法查找
3. 类型检查器可能遗漏了对不存在方法的验证

### 影响范围

- 所有使用错误方法名的代码
- 开发体验严重受损（错误延迟到IR阶段才暴露）
- 源映射丢失导致难以定位问题

### 临时解决方案

仔细检查API文档，确保方法名正确：
```cay
// 错误
if (!file.isOpen()) { }

// 正确
if (!file.isOpened()) { }
```

### 修复建议

1. 强化语义分析阶段的方法存在性检查
2. 在方法解析失败时立即报告语义错误
3. 提供"Did you mean: isOpened()"类似的建议

### 修复记录

2026-06-07：已补齐控制流语句中的表达式类型检查，`if/while/for/do/switch` 中的不存在方法调用会在语义分析阶段报错，并提供相近方法名建议。

---

## Bug 003: IR编译错误源映射丢失

**状态**: 已修复  
**发现日期**: 2026-06-07  
**影响版本**: v5.1.0-RC.1+f65e3e0-dirty  
**严重程度**: 中

### 问题描述

当IR编译阶段（llc+lld模式）发生错误时，错误信息未能正确映射回Cavvy源文件位置。编译器显示的是LLVM IR文件中的位置，而不是原始的 `.cay` 源文件位置。

### 实际输出

```
help: llc 编译失败 (exit code: 1): llc.exe: error: llc.exe: text_editor.ll:4834:21: error: use of undefined value '@_ZN3std4FileE.isOpen'
    %t11 = call i64 @_ZN3std4FileE.isOpen()
                    ^
```

### 期望输出

```
error: 调用未定义的方法 'isOpen'
   ╭─[text_editor.cay:637:18]
637 │         if (!file.isOpen()) {
    ·                  ───┬───
    ·                     ╰── 方法 'isOpen' 在类型 'File' 中不存在
    ·                     help: 是否想调用 'isOpened()'?
```

### 根本原因分析

1. IR编译阶段的错误处理未使用源映射信息
2. llc的错误输出未被解析和转换
3. 源映射表可能未正确传递到错误处理流程

### 影响范围

- 所有在IR编译阶段暴露的错误
- 严重影响调试体验

### 修复建议

1. 捕获llc的错误输出并解析
2. 使用源映射表将IR位置转换回Cavvy源位置
3. 生成用户友好的错误信息

### 修复记录

2026-06-07：`llc+lld` 编译路径已使用 IR source map 重写底层错误输出；`llc` 风格的 `.ll:line:column` 错误会映射回 `.cay` 源位置。

---

## Bug 004: IR编译错误缺少Cavvy编译器警告

**状态**: 已修复  
**发现日期**: 2026-06-07  
**影响版本**: v5.1.0-RC.1+f65e3e0-dirty  
**严重程度**: 中

### 问题描述

当IR编译阶段发生错误时，错误信息以原始LLVM错误形式呈现，缺少Cavvy编译器的上下文和警告。用户看到的是底层工具链错误，而不是高层的语义错误提示。

### 实际输出

```
× cavvy::tool_error: ir2exe 执行失败
   │
   │ IR→EXE编译失败
   │
  help: llc 编译失败 (exit code: 1): llc.exe: error: llc.exe: ...
```

### 期望行为

编译器应该：
1. 在语义分析阶段捕获此类错误
2. 提供清晰的错误描述
3. 提供修复建议
4. 显示相关的Cavvy源代码上下文

### 根本原因分析

1. 错误处理流程过于依赖底层工具
2. 缺少对常见IR错误的预处理
3. 错误分类和转换机制不完善

### 修复建议

1. 建立IR错误模式匹配库
2. 将常见IR错误映射到高级语义错误
3. 改进错误处理流程，优先使用Cavvy风格的错误报告

### 修复记录

2026-06-07：已优先在语义分析阶段捕获缺失方法调用；仍落入 IR 编译阶段的错误会通过 source map 附带 Cavvy 源位置和上下文提示。

---

## Bug 005: IR文件源映射信息错误

**状态**: 已修复  
**发现日期**: 2026-06-07  
**影响版本**: v5.1.0-RC.1+f65e3e0-dirty  
**严重程度**: 高

### 问题描述

生成的LLVM IR文件中的源映射信息（`!source` 元数据）是错误的。IR中的源位置指向了错误的文件，例如 `TextBuffer.saveToFile` 方法的IR代码被标记为来自 `StringBuilder.cay`，而实际上应该来自 `text_editor.cay`。

### 实际IR输出示例

```llvm
define i32 @_ZN10TextBufferE.__saveToFile_s(i8* %this, i8* %TextBuffer.filename) {
entry:
  %this_s1 = alloca i8*
  store i8* %this, i8** %this_s1
  ; !source \\?\E:\spj\EOL\target\release\caylibs\StringBuilder.cay:621:9
  %filename_s1 = alloca i8*
  store i8* %TextBuffer.filename, i8** %filename_s1
  ; !source \\?\E:\spj\EOL\target\release\caylibs\StringBuilder.cay:621:9
  %file_s2 = alloca i8*, align 8
  ; !source \\?\E:\spj\EOL\target\release\caylibs\StringBuilder.cay:630:9
```

### 期望IR输出

```llvm
define i32 @_ZN10TextBufferE.__saveToFile_s(i8* %this, i8* %TextBuffer.filename) {
entry:
  %this_s1 = alloca i8*
  store i8* %this, i8** %this_s1
  ; !source E:\spj\EOL\examples\text_editor.cay:621:9
  %filename_s1 = alloca i8*
  store i8* %TextBuffer.filename, i8** %filename_s1
  ; !source E:\spj\EOL\examples\text_editor.cay:621:9
```

### 根本原因分析

1. 代码生成器在处理 `#include` 导入的文件后，未正确恢复原始源文件上下文
2. 源位置跟踪器可能在处理包含文件后指向了错误的文件
3. 可能缺少源文件切换的边界标记

### 影响范围

- 所有使用 `#include` 导入其他文件的代码
- 调试体验严重受损（无法正确映射到源代码）
- 崩溃时无法定位到正确的源文件位置

### 修复建议

1. 修复代码生成器的源位置跟踪逻辑
2. 在处理 `#include` 时保存和恢复源文件上下文
3. 添加源文件边界标记，确保每个IR指令关联到正确的源文件
4. 规范化路径格式（移除 `\\?\` 前缀）

### 修复记录

2026-06-07：`set_source_from_loc()` 现在优先使用 AST `SourceLocation.file`，仅在缺失文件信息时才回退预处理器映射，并会移除 Windows `\\?\` 路径前缀。

---

## 修复状态图例

- **未修复**: Bug 已确认，尚未修复
- **修复中**: 正在开发修复
- **已修复**: 修复已合并到主分支
- **已确认**: 确认为预期行为或设计限制
- **无法复现**: 无法稳定复现的问题

---

*最后更新: 2026-06-07*
