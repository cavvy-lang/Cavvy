# Cavvy C Runtime (cayrt) ABI 规范

本文档定义 Cavvy 编译器运行时库 (cayrt) 的应用程序二进制接口 (ABI)，包括类型映射、内存布局、函数调用约定和运行时服务。

---

## 目录

1. [概述](#概述)
2. [类型映射](#类型映射)
3. [内存分配器](#内存分配器)
4. [字符串操作](#字符串操作)
5. [类型转换](#类型转换)
6. [指针操作](#指针操作)
7. [内存操作](#内存操作)
8. [数组操作](#数组操作)
9. [构建与链接](#构建与链接)

---

## 概述

Cavvy C Runtime (cayrt) 是 Cavvy 编译器的内置运行时支持库，以静态库形式 (`libcayrt.a`) 提供。所有运行时函数使用 C 调用约定 (cdecl)，由 Cavvy 编译器生成的 LLVM IR 通过 `declare` + `call` 调用。

### 文件位置

```
caylibs/
└── bin/
    └── cayrt/
        ├── cayrt.h          # C 头文件
        ├── allocator.c      # 分配器实现
        ├── string_ops.c     # 字符串操作
        ├── type_conv.c      # 类型转换
        ├── ptr_ops.c        # 指针操作
        ├── memory.c         # 内存操作
        ├── array_ops.c      # 数组操作
        └── build.sh         # 构建脚本
```

### 调用约定

- **默认**: `cdecl` (C 声明调用约定)
- **64位系统**: `i64` 和 `void*` 大小相同（8字节），可安全互转
- **结构体布局**: 必须与 LLVM IR 中的定义精确匹配

---

## 类型映射

### LLVM IR 到 C 类型映射

| LLVM IR 类型 | C 类型 | 说明 |
|-------------|--------|------|
| `i8*` | `char*` | 字节指针/字符串 |
| `i32` | `int32_t` | 32位有符号整数 |
| `i64` | `int64_t` | 64位有符号整数 |
| `i1` | `bool` | 布尔值 (stdbool.h) |
| `float` | `float` | 32位 IEEE 754 浮点 |
| `double` | `double` | 64位 IEEE 754 浮点 |
| `void` | `void` | 无返回值 |
| `i8**` | `char**` | 字符串数组 |
| `i64` (指针) | `void*` / `intptr_t` | 指针作为整数传递 |

### 标准头文件包含

```c
#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>
```

---

## 内存分配器

### 分配器类型定义

#### GlobalAlloc - 全局堆分配器

```c
/** GlobalAlloc — 全局堆分配器的标记结构体 */
typedef struct {
    char _dummy;  /* 对应 LLVM: %GlobalAlloc = type { i8 } */
} GlobalAlloc;
```

**LLVM IR 结构**:
```llvm
%GlobalAlloc = type { i8 }  ; 单字节占位符
```

#### ArenaAllocator - Arena 线性分配器

```c
/** ArenaAllocator — Arena 线性分配器
 *
 * LLVM: %ArenaAllocator = type { i8*, i8*, i8*, %ArenaAllocator* }
 * 字段:
 *   buffer  — 内存块起始地址 (i8*)
 *   current — 当前分配位置 (i8*)
 *   end     — 内存块结束地址 (i8*)
 *   prev    — 前一个 Arena（用于链式分配）(%ArenaAllocator*)
 */
typedef struct ArenaAllocator {
    char*                buffer;
    char*                current;
    char*                end;
    struct ArenaAllocator* prev;
} ArenaAllocator;
```

**内存布局** (64位系统):
```
偏移量    字段        类型        大小
----------------------------------------
0x00      buffer      i8*         8 bytes
0x08      current     i8*         8 bytes
0x10      end         i8*         8 bytes
0x18      prev        ArenaAllocator*  8 bytes
----------------------------------------
总计: 32 bytes
```

#### StackAllocator - 栈分配器

```c
/** StackAllocator — 栈分配器
 *
 * LLVM: %StackAllocator = type { i8*, i64 }
 */
typedef struct {
    char*    base;      /* 栈基址 (i8*) */
    int64_t  marker;    /* 栈标记 (i64) */
} StackAllocator;
```

### 分配器 API

#### GlobalAlloc 函数

```c
/** 获取 GlobalAlloc 单例的指针
 * 
 * @return GlobalAlloc* 单例指针
 * @note 该函数线程安全（返回静态变量地址）
 */
GlobalAlloc* __cay_global_alloc_get(void);
```

**LLVM IR 声明**:
```llvm
declare %GlobalAlloc* @__cay_global_alloc_get()
```

#### Arena 分配器函数

```c
/** 创建新的 Arena 分配器
 *
 * @param capacity  初始容量（字节）
 * @return ArenaAllocator*  新分配器实例，失败返回 NULL
 * @note 使用 malloc 分配结构体和缓冲区
 */
ArenaAllocator* __cay_arena_new(int64_t capacity);

/** 从 Arena 分配内存（带对齐）
 *
 * @param arena   Arena 分配器实例
 * @param size    请求分配的大小
 * @param align   对齐要求（必须是2的幂）
 * @return char*  对齐后的内存指针，失败返回 NULL
 * @note 不会单独释放，调用 __cay_arena_reset 批量释放
 */
char* __cay_arena_alloc(ArenaAllocator* arena, int64_t size, int64_t align);

/** 重置 Arena（批量释放所有分配）
 *
 * @param arena  Arena 分配器实例
 * @note O(1) 操作，仅重置 current 指针到 buffer
 */
void __cay_arena_reset(ArenaAllocator* arena);

/** 释放 Arena 及其缓冲区
 *
 * @param arena  Arena 分配器实例
 * @note 释放所有内存，包括结构体本身
 */
void __cay_arena_free(ArenaAllocator* arena);
```

**LLVM IR 声明**:
```llvm
declare %ArenaAllocator* @__cay_arena_new(i64)
declare i8* @__cay_arena_alloc(%ArenaAllocator*, i64, i64)
declare void @__cay_arena_reset(%ArenaAllocator*)
declare void @__cay_arena_free(%ArenaAllocator*)
```

**使用示例** (Cavvy 代码):
```cay
extern {
    long __cay_arena_new(long capacity);
    long __cay_arena_alloc(long arena, long size, long align);
    void __cay_arena_reset(long arena);
    void __cay_arena_free(long arena);
}

class Main {
    static void main() {
        long arena = __cay_arena_new(1024);
        long ptr = __cay_arena_alloc(arena, 256, 8);
        // 使用内存...
        __cay_arena_reset(arena);  // 批量重置
        __cay_arena_free(arena);   // 释放
    }
}
```

---

## 字符串操作

Cavvy 字符串在内部表示为以 null 结尾的 C 字符串 (`char*`)。所有函数在空指针输入时进行安全检查，返回空字符串或错误码。

### 字符串操作函数

```c
/** 字符串拼接
 * 
 * @param a  第一个字符串（可为 NULL）
 * @param b  第二个字符串（可为 NULL）
 * @return char*  新分配的拼接结果，使用 calloc 分配
 * @note 返回字符串必须被释放（如果非空字符串常量）
 */
char* __cay_string_concat(const char* a, const char* b);

/** 字符串长度
 *
 * @param str  输入字符串（可为 NULL）
 * @return int32_t  字符串长度，NULL 返回 0
 */
int32_t __cay_string_length(const char* str);

/** 子串提取
 *
 * @param str   源字符串
 * @param begin 起始索引（包含）
 * @param end   结束索引（不包含）
 * @return char*  新分配的子串
 * @note 自动处理负数索引和越界情况
 */
char* __cay_string_substring(const char* str, int32_t begin, int32_t end);

/** 查找子串位置（首次出现）
 *
 * @param str     源字符串
 * @param substr  要查找的子串
 * @return int32_t  索引位置，-1 表示未找到
 */
int32_t __cay_string_indexof(const char* str, const char* substr);

/** 从指定位置查找子串
 *
 * @param str     源字符串
 * @param substr  要查找的子串
 * @param start   开始查找的位置
 * @return int32_t  索引位置，-1 表示未找到
 */
int32_t __cay_string_indexof_from(const char* str, const char* substr, int32_t start);

/** 反向查找子串位置（最后一次出现）
 *
 * @param str     源字符串
 * @param substr  要查找的子串
 * @return int32_t  索引位置，-1 表示未找到
 */
int32_t __cay_string_lastindexof(const char* str, const char* substr);

/** 检查前缀
 *
 * @param str     源字符串
 * @param prefix  前缀
 * @return bool   是否以 prefix 开头
 */
bool __cay_string_startswith(const char* str, const char* prefix);

/** 检查后缀
 *
 * @param str     源字符串
 * @param suffix  后缀
 * @return bool   是否以 suffix 结尾
 */
bool __cay_string_endswith(const char* str, const char* suffix);

/** 获取指定位置的字符
 *
 * @param str    源字符串
 * @param index  字符索引
 * @return char  指定位置的字符，越界返回 '\0'
 */
char __cay_string_charat(const char* str, int32_t index);

/** 字符串替换（替换所有出现）
 *
 * @param str      源字符串
 * @param old      要替换的子串
 * @param new_str  替换后的子串
 * @return char*   新分配的结果字符串
 */
char* __cay_string_replace(const char* str, const char* old, const char* new_str);

/** 检查是否为空字符串
 *
 * @param str  输入字符串
 * @return bool  是否为空（NULL 或长度为 0）
 */
bool __cay_string_isempty(const char* str);

/** 字符串比较（区分大小写）
 *
 * @param str1  第一个字符串
 * @param str2  第二个字符串
 * @return bool  是否相等
 */
bool __cay_string_equals(const char* str1, const char* str2);

/** 字符串比较（不区分大小写）
 *
 * @param str1  第一个字符串
 * @param str2  第二个字符串
 * @return bool  是否相等（忽略大小写）
 */
bool __cay_string_equals_ignorecase(const char* str1, const char* str2);

/** 去除首尾空白
 *
 * @param str  源字符串
 * @return char*  新分配的修剪后字符串
 */
char* __cay_string_trim(const char* str);

/** 转换为小写
 *
 * @param str  源字符串
 * @return char*  新分配的小写字符串
 */
char* __cay_string_to_lower(const char* str);

/** 转换为大写
 *
 * @param str  源字符串
 * @return char*  新分配的大写字符串
 */
char* __cay_string_to_upper(const char* str);

/** 检查字符串是否包含子串
 *
 * @param str     源字符串
 * @param substr  子串
 * @return bool   是否包含
 */
bool __cay_string_contains(const char* str, const char* substr);

/** 字符串比较（按字典序）
 *
 * @param str1  第一个字符串
 * @param str2  第二个字符串
 * @return int32_t  -1 (str1<str2), 0 (相等), 1 (str1>str2)
 */
int32_t __cay_string_compareto(const char* str1, const char* str2);
```

### LLVM IR 声明

```llvm
; 字符串操作
declare i8* @__cay_string_concat(i8*, i8*)
declare i32 @__cay_string_length(i8*)
declare i8* @__cay_string_substring(i8*, i32, i32)
declare i32 @__cay_string_indexof(i8*, i8*)
declare i32 @__cay_string_indexof_from(i8*, i8*, i32)
declare i32 @__cay_string_lastindexof(i8*, i8*)
declare i1 @__cay_string_startswith(i8*, i8*)
declare i1 @__cay_string_endswith(i8*, i8*)
declare i8 @__cay_string_charat(i8*, i32)
declare i8* @__cay_string_replace(i8*, i8*, i8*)
declare i1 @__cay_string_isempty(i8*)
declare i1 @__cay_string_equals(i8*, i8*)
declare i1 @__cay_string_equals_ignorecase(i8*, i8*)
declare i8* @__cay_string_trim(i8*)
```

---

## 类型转换

将 Cavvy 基础类型转换为字符串表示。所有函数在 `calloc` 失败时返回空字符串，避免崩溃。

### 类型转换函数

```c
/** int32_t → 字符串
 *
 * @param value  整数值
 * @return char*  新分配的字符串（最大32字节）
 */
char* __cay_int_to_string(int32_t value);

/** int64_t (long) → 字符串
 *
 * @param value  长整数值
 * @return char*  新分配的字符串（最大32字节）
 */
char* __cay_long_to_string(int64_t value);

/** float → 字符串
 *
 * @param value  浮点值
 * @return char*  新分配的字符串（最大64字节）
 */
char* __cay_float_to_string(float value);

/** double → 字符串
 *
 * @param value  双精度值
 * @return char*  新分配的字符串（最大64字节）
 */
char* __cay_double_to_string(double value);

/** bool → 字符串
 *
 * @param value  布尔值
 * @return char*  返回静态字符串 "true" 或 "false"
 * @note 返回静态常量，无需释放
 */
char* __cay_bool_to_string(bool value);

/** char → 字符串
 *
 * @param value  字符值
 * @return char*  新分配的单字符字符串
 */
char* __cay_char_to_string(char value);
```

### LLVM IR 声明

```llvm
; 类型转换
declare i8* @__cay_int_to_string(i32)
declare i8* @__cay_long_to_string(i64)
declare i8* @__cay_float_to_string(float)
declare i8* @__cay_double_to_string(double)
declare i8* @__cay_bool_to_string(i1)
declare i8* @__cay_char_to_string(i8)
```

---

## 指针操作

提供对原始内存的读写操作，用于 FFI 交互。所有指针参数以 `int64_t` 形式传入，内部转换为 `void*`。

### 指针操作函数

```c
/** 从指定地址读取 64 位指针值
 *
 * @param ptr  内存地址（i64 编码的指针）
 * @return int64_t  该地址存储的 64 位值
 * @warning 不检查地址有效性
 */
int64_t __cay_read_ptr(int64_t ptr);

/** 将 C 字符串指针转换为 Cavvy 字符串（复制数据）
 *
 * @param ptr  C 字符串指针地址
 * @return char*  新分配的 Cavvy 字符串副本
 * @note 如果 ptr 为 0 或指向空字符串，返回空字符串常量
 */
char* __cay_ptr_to_string(int64_t ptr);

/** 向指定地址写入 64 位指针值
 *
 * @param ptr    目标内存地址
 * @param value  要写入的 64 位值
 * @warning 不检查地址有效性
 */
void __cay_write_ptr(int64_t ptr, int64_t value);

/** 向指定地址写入 32 位整数值
 *
 * @param ptr    目标内存地址
 * @param value  要写入的 32 位值
 */
void __cay_write_int(int64_t ptr, int32_t value);

/** 从指定地址读取 32 位整数值
 *
 * @param ptr  内存地址
 * @return int32_t  该地址存储的 32 位值
 */
int32_t __cay_read_int(int64_t ptr);

/** 向指定地址写入 8 位字节值
 *
 * @param ptr    目标内存地址
 * @param value  要写入的 8 位值（低8位有效）
 */
void __cay_write_byte(int64_t ptr, int32_t value);

/** 将缓冲区内容转换为字符串
 *
 * @param buffer  缓冲区地址
 * @param length  缓冲区长度
 * @return char*  新分配的字符串（包含 length 个字符 + null 终止符）
 */
char* __cay_buffer_to_string(int64_t buffer, int32_t length);
```

### LLVM IR 声明

```llvm
; 指针操作
declare i64 @__cay_read_ptr(i64)
declare i8* @__cay_ptr_to_string(i64)
declare void @__cay_write_ptr(i64, i64)
declare void @__cay_write_int(i64, i32)
declare i32 @__cay_read_int(i64)
declare void @__cay_write_byte(i64, i32)
declare i8* @__cay_buffer_to_string(i64, i32)
```

### 使用示例

```cay
extern {
    long __cay_read_ptr(long ptr);
    void __cay_write_ptr(long ptr, long value);
    String __cay_ptr_to_string(long ptr);
    long malloc(long size);
    void free(long ptr);
}

class Main {
    static void main() {
        // 分配内存并写入指针值
        long ptr = malloc(16);
        long data = malloc(32);
        __cay_write_ptr(ptr, data);

        // 读取指针值
        long readData = __cay_read_ptr(ptr);

        // 清理
        free(ptr);
        free(data);
    }
}
```

---

## 内存操作

提供按字节设置和复制内存的运行时支持。包含空指针安全检查。

### 内存操作函数

```c
/** 按字节设置内存（空指针安全）
 *
 * @param ptr    目标内存地址（i64 编码）
 * @param value  要设置的值（低8位有效）
 * @param n      字节数
 * @note 如果 ptr 为 0，不执行任何操作
 */
void __cay_memset_byte(int64_t ptr, int32_t value, int32_t n);

/** 按字节复制内存（空指针安全）
 *
 * @param dest  目标地址（i64 编码）
 * @param src   源地址（i64 编码）
 * @param n     字节数
 * @note 如果 dest 或 src 为 0，不执行任何操作
 */
void __cay_memcpy_byte(int64_t dest, int64_t src, int32_t n);
```

### LLVM IR 声明

```llvm
; 内存操作
declare void @__cay_memset_byte(i64, i32, i32)
declare void @__cay_memcpy_byte(i64, i64, i32)
```

### 使用示例

```cay
extern {
    void __cay_memset_byte(long ptr, int value, int n);
    void __cay_memcpy_byte(long dest, long src, int n);
    long malloc(long size);
    void free(long ptr);
}

class Main {
    static void main() {
        long buffer = malloc(256);

        // 清零缓冲区
        __cay_memset_byte(buffer, 0, 256);

        // 复制数据
        long src = "Hello".c_str();
        __cay_memcpy_byte(buffer, src, 5);

        free(buffer);
    }
}
```

---

## 数组操作

Cavvy 数组内存布局:
```
[长度:i32 (4B)][padding (4B)][元素0][元素1]...
```
返回指针指向元素0，长度字段在 -8 偏移处。

### 数组操作函数

```c
/** 创建 String[] 数组
 *
 * 布局: [4B length][4B pad][8B*size elements]
 * 返回: 指向数据区（元素0）的指针
 *
 * @param size  数组元素个数
 * @return char**  数组数据指针
 * @note 使用 calloc 分配，自动清零
 */
char** __cay_create_string_array(int32_t size);

/** 将 C 字符串转换为 Cavvy String 对象
 *
 * @param cstr  C 字符串
 * @return char*  新分配的 Cavvy 字符串副本
 * @note 安全处理 NULL 输入
 */
char* __cay_cstr_to_string(const char* cstr);

/** 设置数组元素（引用类型）
 *
 * @param arr   字符串数组
 * @param idx   索引
 * @param value 要设置的值
 */
void __cay_array_set_ref(char** arr, int32_t idx, char* value);

/** 获取数组元素（引用类型）
 *
 * @param arr  字符串数组
 * @param idx  索引
 * @return char*  指定位置的元素
 */
char* __cay_array_get_ref(char** arr, int32_t idx);

/** 获取数组长度
 *
 * @param arr  字符串数组
 * @return int32_t  数组长度
 * @note 长度存储在 arr 指针前 8 字节处
 */
int32_t __cay_array_length(char** arr);
```

### 数组内存布局详解

```
地址偏移    内容                    大小
------------------------------------------
-8          length (i32)            4 bytes
-4          padding                 4 bytes
 0          element[0]              8 bytes (指针)
 8          element[1]              8 bytes (指针)
 16         element[2]              8 bytes (指针)
...         ...                     ...
```

### LLVM IR 声明

```llvm
; 数组操作
declare i8** @__cay_create_string_array(i32)
declare i8* @__cay_cstr_to_string(i8*)
declare void @__cay_array_set_ref(i8**, i32, i8*)
declare i8* @__cay_array_get_ref(i8**, i32)
declare i32 @__cay_array_length(i8**)
```

### 使用示例

```cay
extern {
    long __cay_create_string_array(int size);
    void __cay_array_set_ref(long arr, int idx, String value);
    String __cay_array_get_ref(long arr, int idx);
    int __cay_array_length(long arr);
}

class Main {
    static void main() {
        // 创建字符串数组
        long arr = __cay_create_string_array(3);
        
        // 设置元素
        __cay_array_set_ref(arr, 0, "Hello");
        __cay_array_set_ref(arr, 1, "World");
        __cay_array_set_ref(arr, 2, "!");
        
        // 获取长度
        int len = __cay_array_length(arr);
        println(len);  // 3
        
        // 读取元素
        String s = __cay_array_get_ref(arr, 0);
        println(s);  // "Hello"
    }
}
```

---

## 构建与链接

### 构建 cayrt 静态库

```bash
# 进入 cayrt 目录
cd caylibs/bin/cayrt

# 编译所有源文件
cc -c -O2 -fPIC allocator.c -o allocator.o
cc -c -O2 -fPIC string_ops.c -o string_ops.o
cc -c -O2 -fPIC type_conv.c -o type_conv.o
cc -c -O2 -fPIC ptr_ops.c -o ptr_ops.o
cc -c -O2 -fPIC memory.c -o memory.o
cc -c -O2 -fPIC array_ops.c -o array_ops.o

# 创建静态库
ar rcs libcayrt.a allocator.o string_ops.o type_conv.o ptr_ops.o memory.o array_ops.o
```

或使用提供的构建脚本:
```bash
./build.sh
```

### 链接到 Cavvy 程序

Cavvy 编译器会自动链接 `libcayrt.a`，无需手动指定。

如需手动链接:
```bash
# 直接链接静态库
cayc program.cay -L./caylibs/bin/cayrt -lcayrt

# 或指定完整路径
cayc program.cay ./caylibs/bin/cayrt/libcayrt.a
```

### 运行时依赖

- **Windows**: 需要 `msvcrt.dll` (C 运行时)
- **Linux**: 需要 `libc.so` (glibc)

---

## 版本信息

- **ABI 版本**: 5.1.0
- **库版本**: 0.5.1.x
- **最后更新**: 2025-06-09
- **兼容性**: Cavvy 5.1.0+

### 变更历史

| 版本 | 变更 |
|------|------|
| 5.1.0 | 初始 ABI 定义 |
| 0.5.1 | 添加 Arena 分配器 |
| 0.5.0 | 添加 GlobalAlloc |

---

## 附录：完整头文件

```c
#ifndef CAYRT_H
#define CAYRT_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* 分配器类型 */
typedef struct { char _dummy; } GlobalAlloc;

typedef struct ArenaAllocator {
    char* buffer;
    char* current;
    char* end;
    struct ArenaAllocator* prev;
} ArenaAllocator;

typedef struct {
    char* base;
    int64_t marker;
} StackAllocator;

/* 分配器函数 */
GlobalAlloc* __cay_global_alloc_get(void);
ArenaAllocator* __cay_arena_new(int64_t capacity);
char* __cay_arena_alloc(ArenaAllocator* arena, int64_t size, int64_t align);
void __cay_arena_reset(ArenaAllocator* arena);
void __cay_arena_free(ArenaAllocator* arena);

/* 字符串操作 */
char* __cay_string_concat(const char* a, const char* b);
int32_t __cay_string_length(const char* str);
char* __cay_string_substring(const char* str, int32_t begin, int32_t end);
int32_t __cay_string_indexof(const char* str, const char* substr);
int32_t __cay_string_indexof_from(const char* str, const char* substr, int32_t start);
int32_t __cay_string_lastindexof(const char* str, const char* substr);
bool __cay_string_startswith(const char* str, const char* prefix);
bool __cay_string_endswith(const char* str, const char* suffix);
char __cay_string_charat(const char* str, int32_t index);
char* __cay_string_replace(const char* str, const char* old, const char* new_str);
bool __cay_string_isempty(const char* str);
bool __cay_string_equals(const char* str1, const char* str2);
bool __cay_string_equals_ignorecase(const char* str1, const char* str2);
char* __cay_string_trim(const char* str);

/* 类型转换 */
char* __cay_int_to_string(int32_t value);
char* __cay_long_to_string(int64_t value);
char* __cay_float_to_string(float value);
char* __cay_double_to_string(double value);
char* __cay_bool_to_string(bool value);
char* __cay_char_to_string(char value);

/* 指针操作 */
int64_t __cay_read_ptr(int64_t ptr);
char* __cay_ptr_to_string(int64_t ptr);
void __cay_write_ptr(int64_t ptr, int64_t value);
void __cay_write_int(int64_t ptr, int32_t value);
int32_t __cay_read_int(int64_t ptr);
void __cay_write_byte(int64_t ptr, int32_t value);
char* __cay_buffer_to_string(int64_t buffer, int32_t length);

/* 内存操作 */
void __cay_memset_byte(int64_t ptr, int32_t value, int32_t n);
void __cay_memcpy_byte(int64_t dest, int64_t src, int32_t n);

/* 数组操作 */
char** __cay_create_string_array(int32_t size);
char* __cay_cstr_to_string(const char* cstr);
void __cay_array_set_ref(char** arr, int32_t idx, char* value);
char* __cay_array_get_ref(char** arr, int32_t idx);
int32_t __cay_array_length(char** arr);

#ifdef __cplusplus
}
#endif

#endif /* CAYRT_H */
```
