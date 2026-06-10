# Cavvy 5.1.0 Bug 修复清单

本文件按模块分类列出 5.1.0 版本中修复的所有问题。

---

## 编译器核心

### 类型系统

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| 泛型类字段类型未替换 | 在 `class_analysis.rs` 中添加字段类型的泛型参数替换 | `198399ad` |
| 泛型方法参数/返回类型未替换 | 在 `type_check.rs` 中对方法参数和返回类型进行泛型参数替换 | `198399ad` |
| 多类型参数解析失败 | 在 `expr_inference.rs` 中修复类型参数解析逻辑，支持 `Pair<K, V>` | `198399ad` |
| 泛型类方法查找失败 | 在 `TypeRegistry::find_method` 中支持泛型类名解析为基础类名 | `198399ad` |
| 泛型类型匹配失败 | 在 `ClassInfo::types_match_exact` 中支持模板与实例化类型匹配 | `198399ad` |
| 嵌套泛型与移位符冲突 | 提取泛型类型实参解析为公共函数，支持 `>>`/`>>>` 作为嵌套泛型结束符 | `05d469eb` |
| 指针类型命名空间兼容 | 新增指针类型命名空间兼容检查 | `b18a00f7` |
| using 别名查找类型 | 为 `TypeRegistry::find_qualified_class` 添加 using 别名匹配逻辑 | `619baad9` |
| 类型兼容检查不完善 | 重构类型兼容检查逻辑，支持命名空间和泛型类型匹配 | `b18a00f7` |
| 加法运算类型限制 | 优化加法运算类型支持，允许非字面量数值与字符串相加 | `b18a00f7` |

### 语义分析

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| AddressOf 返回错误类型 | 修复为返回 `Type::Pointer` 而非 `Type::Int64` | `f65e3e08` |
| Deref 未正确处理 Pointer | 修复语义分析器中 Deref 对 `Type::Pointer` 的处理 | `f65e3e08` |
| 控制流条件类型检查缺失 | 补全 if/while/for/do/switch 等控制流的条件类型检查 | `5f90c9f1` |
| 静态方法调用解析错误 | 重构静态方法调用解析逻辑，优先匹配当前类静态方法 | `5f90c9f1` |
| 方法名拼写错误提示差 | 新增方法名拼写建议功能，优化未找到方法的错误提示 | `5f90c9f1` |
| 源文件路径前缀问题 | 修复源文件路径处理，移除 Windows `\\?\` 前缀 | `5f90c9f1` |
| 错误重映射逻辑缺陷 | 重构错误重映射逻辑，传递并使用 `source_map` 参数 | `5f90c9f1` |

### 代码生成

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| 解引用赋值不支持 | 添加代码生成器对解引用赋值的支持 (`generate_deref_assignment`) | `f65e3e08` |
| store 指令类型不匹配 | 修正代码生成时 store 语句中目标指针类型错误 | `ddfde110` |
| if 语句 merge 块缺少 terminator | 检测 then 和 else 块是否都返回，如果是则不创建 merge 块 | `5c9a1a4f` |
| 静态成员 codegen null fallback | 修复静态成员代码生成时的空值回退问题 | `7ffd308d` |
| 对象地址获取偏移量错误 | 修复对象地址获取时的偏移量错误 | `87c71260` |
| 布尔值打印逻辑错误 | 直接存储并输出 true/false 而非 1 | `17c1b871` |
| 字符串拼接整数转换 | 统一使用 i32 类型处理 | `17c1b871` |
| this 关键字类型识别 | 修复 this 关键字类型识别问题 | `9258c96a` |
| 数组字段访问生成 | 优化数组字段访问代码生成逻辑 | `9258c96a` |
| switch 语句生成逻辑 | 优化 switch 语句生成逻辑 | `9258c96a` |
| switch 分支终止判断 | 修正 `all_cases_terminate` 判定规则，仅将 return 作为终止语句 | `a8f44d8e` |

### 错误报告

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| 错误报告文件名显示错误 | 语义分析错误现在正确显示错误发生的源文件路径 | `1d9eb313` |
| 错误报告行号映射问题 | 修复错误报告系统在处理包含文件时的行号映射问题 | `ebd3f9ee` |
| 字段赋值错误信息不足 | 改进错误报告，添加字段名和类名信息 | `8c9d5b08` |

---

## 预处理器

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| #define 行尾注释处理 | 添加 `remove_line_comments` 辅助函数，支持 `//` 和 `/* */` 注释移除 | `84734c98` |
| 符号链接下无法找到 caylibs | 使用 `canonicalize` 解析符号链接，兼容 Linux 下 `/proc/self/exe` | `b3574c22` |
| 包含文件行号对齐 | 修复预处理器包含文件行号对齐问题 | `c9a2e2f3` |
| 宏替换边界检查 | 修复预处理器宏替换边界检查问题 | `8c9d5b08` |
| include 示例文件名错误 | 修正为正确的 `File.cay` | `b3574c22` |

---

## 网络模块 (Network.cay)

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| socket 句柄类型不匹配 | 修复 `TcpSocket`、`TcpServer` 和 `UdpSocket` 类的句柄类型 | `91eca938` |
| 跨平台 socket 发送参数类型不兼容 | 修复非 Windows 平台 `send`/`sendto` 长度参数类型不匹配 | `7cdaf483` |
| setsockopt 超时参数传递错误 | 修正 `TcpServer` 中 `setsockopt` 的超时参数传递 | `7cdaf483` |
| c_int 与 int 类型混用 | 将超时参数数组从 `c_int` 改为 `int` | `f67c30f2` |
| Cay 语言数组声明语法错误 | 将 C 风格静态数组改为 Cay 语言动态数组初始化 | `bb76c254` |
| TcpSocket 构造函数访问修饰符 | 修正为正确的访问修饰符 | `3d71041c` |
| HttpClient.send 返回值判断 | 修复返回值判断逻辑 | `3d71041c` |
| EasyHTTP char 隐式转 int | 修复导致的符号问题 | `3d71041c` |

---

## 标准库

### Math.cay

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| Math.abs(int) INT_MIN 溢出 | 添加溢出处理 | `d5dffc5e` |
| Math.abs(long) LONG_MIN 溢出 | 添加溢出处理 | `d5dffc5e` |
| Math.smoothStep 除零 | 添加 `a==b` 除零防护 | `d5dffc5e` |
| Math.clamp 参数顺序 | 自动交换 min/max 如果顺序错误 | `d5dffc5e` |
| Math.gcd INT_MIN 溢出 | 处理溢出问题 | `d5dffc5e` |
| Math.frac 范围错误 | 使用 `floor` 确保返回 `[0,1)` | `d5dffc5e` |
| Random.nextDouble RAND_MAX | 使用正确值 2147483647 | `d5dffc5e` |
| Random.nextBool 模运算偏差 | 使用位运算避免偏差 | `d5dffc5e` |
| Random.nextInt 溢出 | 处理溢出情况 | `d5dffc5e` |
| Random.nextGaussian log(0) | 防护 `log(0)`，缓存第二个值 | `d5dffc5e` |
| Vector2/3.div/normalize 除零 | 除零返回 NaN 而非静默失败 | `d5dffc5e` |
| Math.integrate 硬编码 | 移除硬编码 sin 函数 | `d5dffc5e` |
| Math 最小 int 字面量溢出 | 修复最小 int 字面量溢出问题 | `c9a2e2f3` |

### File.cay

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| exists() 修改 atime | 使用 `access()` 替代 `fopen()` | `d83ef93b` |
| size() TOCTOU 竞态条件 | 使用 FileInfo.stat-based 方法 | `d83ef93b` |
| writeFormat 命名混淆 | 重命名为 `writeInterpolated` | `d83ef93b` |
| writeFormat 复杂度 O(n^2) | 使用 StringBuilder 优化至 O(n) | `d83ef93b` |
| readAllLines 双倍内存峰值 | 改为流式读取 | `d83ef93b` |
| finalize() 错误传播 | 静默处理关闭错误 | `d83ef93b` |

### StringPlus

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| format 占位符拼接逻辑 | 修复占位符拼接逻辑 | `3d71041c` |

---

## 链接与构建 (ir2exe)

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| ELF 动态链接器未设置 | 自动检测并添加可用的系统动态链接器路径 | `b446bd5b` |
| 重复添加启动对象文件 | 检查是否已经添加过启动文件避免重复符号 | `235eac9a` |
| Linux 平台编译链接问题 | 添加 GNU ld 风格参数、加载 CRT 启动文件 | `211a454b` |
| Cavvy 运行时库缺失 | 自动检测并构建 Cavvy 运行时库 | `8443c429` |
| crt2.o 错误 | 将 `crt2.o` 改为 `crt1.o` | `8b511914` |
| lld 链接器风格错误 | 使用 GNU ld 风格参数而非 COFF 风格 | `8b511914` |

---

## CI / 构建脚本

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| Windows 链接缺少 xml2s.lib | 创建空静态库作为占位符 | `69c6e259` |
| llvm-sys 静态/动态链接配置 | 多次调整，最终移除 force-static 适配多平台 | `041045fa`, `d742a6b1`, `5810947c` |
| CI 环境变量未跨步骤生效 | 补全 LLVM 依赖安装和环境变量设置 | `07e99d75` |
| setup-llvm.py 解压路径错误 | 修改解压逻辑，将文件解压到 bin 子目录 | `af36ce46` |
| setup-llvm.py 下载链接层级 | 添加 bin 子目录层级匹配实际结构 | `023bb274` |
| setup-llvm.py 中文编码 | 全局 UTF-8 编码重定向标准输出/错误流 | `39208413` |
| git 版本检测误报 | 过滤编译生成的无后缀 ELF 可执行文件变更 | `bdf122e0` |

---

## 测试

| 问题 | 修复内容 | 相关 Commit |
|------|---------|------------|
| test_calling_conventions Linux 失败 | 将 Windows Sleep API 替换为 POSIX usleep | `580b936a` |
| 跨平台可执行文件路径 | 移除硬编码的 `.exe` 后缀 | `6846141c`, `ed3edba9` |
| inline-ir 测试触发 llvm-sys 重编译 | 替换 `cargo run --release` 为直接二进制执行 | `a09a93eb` |
| 测试可执行文件路径适配 | 优先使用 `CARGO_BIN_EXE_cavly` 环境变量 | `a626b3b0` |
| 非 Windows socket 类型转换 | 添加显式类型转换 | `44215680` |
