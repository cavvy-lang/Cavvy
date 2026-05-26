# 依赖管理

Cavly 支持依赖其他 Cavvy 包，包括本地路径依赖和（计划中的）Git/Registry 依赖。

## 依赖类型

### 本地路径依赖

引用本地文件系统上的另一个 Cavvy 项目：

```toml
[dependencies]
my-utils = { path = "../my-utils" }
shared-lib = { path = "./libs/shared-lib" }
```

### Git 依赖（计划中）

```toml
[dependencies]
community-lib = { git = "https://github.com/user/community-lib", branch = "main" }
pinned-lib = { git = "https://github.com/user/pinned-lib", tag = "v1.2.0" }
```

### 版本依赖（计划中）

```toml
[dependencies]
some-lib = "1.0.0"
another = { version = ">=1.0, <2.0" }
```

## 工作区

工作区允许在本地管理多个相关的 Cavvy 项目：

```toml
[workspace]
# 成员项目路径
members = [
    "../core-lib",
    "../net-lib",
    "./libs/helper"
]

# 额外的库搜索路径
lib_paths = ["./lib", "/usr/local/lib/cavvy"]
```

### 构建顺序

Cavly 使用拓扑排序确定依赖的构建顺序，确保被依赖的项目先构建。

```
项目 A 依赖 B 和 C
B 依赖 D
构建顺序: D → B → C → A
```

### 循环依赖检测

Cavly 在解析依赖时检测循环依赖并报错：

```
错误: 检测到循环依赖: my-lib -> [my-lib, helper-lib]
```

## 开发依赖

仅开发时使用的依赖（测试框架、构建工具等）：

```toml
[dev-dependencies]
test-utils = { path = "../test-utils" }
```

开发依赖不会传递给下游项目。

## 依赖配置合并

当项目依赖一个库时，Cavly 会自动合并该库的以下配置到主项目中：

| 合并的配置 | 说明 |
|---|---|
| `build.lib_paths` | 库搜索路径（追加） |
| `build.libs` | 要链接的库（追加） |
| `build.cflags` | 编译器标志（追加） |
| `build.ldflags` | 链接器标志（追加） |
| `ffi.system_libs` | 系统库（追加） |
| `ffi.include_paths` | 头文件路径（追加） |

**优先级**：主项目配置 > 依赖配置。如果主项目已设置某选项，不会被依赖覆盖。

## 库项目配置

### 静态库 vs 动态库

```toml
[lib]
lib_type = "static"    # .lib / .a
# lib_type = "dynamic"  # .dll / .so
```

### 导出控制

```toml
[lib]
exports = ["public_class1", "public_class2"]  # 只导出指定类
# exports = []  # 空数组 = 导出所有 public 类
```

### 仅接口模式

```toml
[lib]
only_include = true  # 只做语法/语义检查，不编译链接
```

适用于纯头文件库或只提供类型定义的库。下游项目通过 `-I` 路径引用。

### 头文件生成

```toml
[lib.header]
generate = true           # 生成 C 头文件
name = "mylib.h"          # 自定义文件名
include_prefix = "mylib/" # 包含路径前缀
```
