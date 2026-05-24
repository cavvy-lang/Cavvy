# Cavvy Namespace 设计方案

版本: 0.6.0.0 | 日期: 2026-05-24

## 1. 概述

为 Cavvy 语言添加 namespace 支持，允许将类、接口等顶层声明组织到命名空间中。

## 2. 语法规范

### 2.1 文件级 namespace 声明（暂时不好使，别用）

```cay
// 必须是文件第一个非注释/非空行的语句
namespace std;

public class StringBuilder {
    // 自动属于 std
}
```

- 作用域隐式延伸到文件末尾
- 一个文件最多出现一次

### 2.2 块级 namespace 声明

```cay
namespace std {
    public class StringBuilder {
        // 属于 std
    }
  
    namespace io {   // 嵌套：std::io
        public class File {
            // 属于 std::io
        }
    }
}
```

- 可以出现在文件任意位置
- 大括号内所有顶层声明自动属于该 namespace
- 允许嵌套，形成 `std::io` 这样的多级路径
- 块级 namespace 结束后，外部回到上一作用域（或全局）

### 2.3 using 声明

```cay
// 导入单个名字
using std::StringBuilder;
using std::io::File;

// 禁止：
// using namespace std;      // 不行
// using std::*;             // 不行
// using std::io::*;         // 不行
```

### 2.4 名称改编（Itanium ABI）

| Cavvy 名称             | LLVM IR 符号名                |
| ---------------------- | ----------------------------- |
| `std::io::File`      | `_ZN3std2io4File...`        |
| `std::StringBuilder` | `_ZN3std14StringBuilder...` |

规则：`_Z` 前缀 + `N`（嵌套开始）+ 每个组件 `<len><name>` + `E`（嵌套结束）

## 3. 实现阶段

### Phase 1: Lexer — 添加 token

- `Token::Namespace` — `namespace` 关键字
- `Token::Using` — `using` 关键字

### Phase 2: AST — 添加节点

- `NamespaceDecl` — namespace 声明
- `UsingDecl` — using 声明
- 更新 `Program` 和 `ClassDecl` 以携带 namespace 路径

### Phase 3: Parser — 解析

- 文件级 `namespace std;`
- 块级 `namespace std { ... }` 支持嵌套
- `using std::ClassName;` 单名导入
- 拒绝 `using namespace std;` 和通配符导入

### Phase 4: Semantic — 语义分析

- 命名空间作用域栈
- `using` 声明的符号导入
- 类型查找时解析命名空间路径

### Phase 5: Codegen — 代码生成

- Itanium ABI 名称改编
- 静态字段/方法名的命名空间前缀

### Phase 6: 标准库迁移

- 所有 `caylibs/*.cay` 添加 `namespace std;`
- 所有 `examples/*.cay` 添加 `using std::*;` 声明
