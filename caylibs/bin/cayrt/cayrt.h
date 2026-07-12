/**
 * cavyrt.h — Cavvy Runtime Library
 *
 * Cavvy 编译器的内置运行时函数，以静态链接库形式提供。
 * 所有运行时函数均使用 C 调用约定（默认 cdecl），
 * 由 cavy 编译器生成的 LLVM IR 通过 declare + call 调用。
 *
 * 类型映射（LLVM IR → C）:
 *   i8*    → char*
 *   i32    → int32_t
 *   i64    → int64_t / void* (当用作指针时)
 *   i1     → bool (stdbool.h)
 *   float  → float
 *   double → double
 *   void   → void
 *   i8**   → char**
 *
 * ABI 注意：
 *   - 所有函数使用默认 cdecl 调用约定
 *   - 64位系统上 i64 和 void* 大小相同（8字节），可安全互转
 *   - 结构体布局必须与 LLVM IR 中的定义精确匹配
 */

#ifndef CAYRT_H
#define CAYRT_H

#include <stdint.h>
#include <stdbool.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ================================================================
 * 分配器类型定义
 *
 * 结构体字段顺序和偏移量必须与 src/codegen/allocator.rs 中
 * llvm_struct_def() 的定义完全一致。
 * ================================================================ */

/** GlobalAlloc — 全局堆分配器的标记结构体 */
typedef struct {
    char _dummy;  /* 对应 LLVM: %GlobalAlloc = type { i8 } */
} GlobalAlloc;

/** ArenaAllocator — Arena 线性分配器
 *
 * LLVM: %ArenaAllocator = type { i8*, i8*, i8*, %ArenaAllocator* }
 * 字段:
 *   buffer  — 内存块起始地址
 *   current — 当前分配位置
 *   end     — 内存块结束地址
 *   prev    — 前一个 Arena（用于链式分配）
 */
typedef struct ArenaAllocator {
    char*                buffer;
    char*                current;
    char*                end;
    struct ArenaAllocator* prev;
} ArenaAllocator;

/** StackAllocator — 栈分配器
 *
 * LLVM: %StackAllocator = type { i8*, i64 }
 */
typedef struct {
    char*    base;
    int64_t  marker;
} StackAllocator;

/* ================================================================
 * 字符串操作 (string_ops.c)
 * ================================================================ */

char*    __cay_string_concat(const char* a, const char* b);
int32_t  __cay_string_length(const char* str);
char*    __cay_string_substring(const char* str, int32_t begin, int32_t end);
int32_t  __cay_string_indexof(const char* str, const char* substr);
int32_t  __cay_string_indexof_from(const char* str, const char* substr, int32_t start);
int32_t  __cay_string_lastindexof(const char* str, const char* substr);
bool     __cay_string_startswith(const char* str, const char* prefix);
bool     __cay_string_endswith(const char* str, const char* suffix);
char     __cay_string_charat(const char* str, int32_t index);
char*    __cay_string_replace(const char* str, const char* old, const char* new_str);
bool     __cay_string_isempty(const char* str);
bool     __cay_string_equals(const char* str1, const char* str2);
bool     __cay_string_equals_ignorecase(const char* str1, const char* str2);
char*    __cay_string_trim(const char* str);

/* ================================================================
 * 类型转换 (type_conv.c)
 * ================================================================ */

char*    __cay_int_to_string(int32_t value);
char*    __cay_long_to_string(int64_t value);
char*    __cay_float_to_string(float value);
char*    __cay_double_to_string(double value);
char*    __cay_bool_to_string(bool value);
char*    __cay_char_to_string(char value);

/* ================================================================
 * 指针/缓冲区操作 (ptr_ops.c)
 * ================================================================ */

int64_t  __cay_read_ptr(int64_t ptr);
char*    __cay_ptr_to_string(int64_t ptr);
void     __cay_write_ptr(int64_t ptr, int64_t value);
void     __cay_write_int(int64_t ptr, int32_t value);
int32_t  __cay_read_int(int64_t ptr);
void     __cay_write_byte(int64_t ptr, int32_t value);
char*    __cay_buffer_to_string(int64_t buffer, int32_t length);

/* ================================================================
 * 内存操作 (memory.c)
 * ================================================================ */

void     __cay_memset_byte(int64_t ptr, int32_t value, int32_t n);
void     __cay_memcpy_byte(int64_t dest, int64_t src, int32_t n);

/* ================================================================
 * 数组/参数操作 (array_ops.c)
 *
 * Cavvy 数组布局:
 *   [长度:i32][padding:4 bytes][元素0][元素1]...
 *   返回指针指向元素0，长度在 -8 偏移处
 * ================================================================ */

char**   __cay_create_string_array(int32_t size);
char*    __cay_cstr_to_string(const char* cstr);
void     __cay_array_set_ref(char** arr, int32_t idx, char* value);
char*    __cay_array_get_ref(char** arr, int32_t idx);
int32_t  __cay_array_length(char** arr);

/* ================================================================
 * 分配器 (allocator.c)
 * ================================================================ */

GlobalAlloc*     __cay_global_alloc_get(void);
ArenaAllocator*  __cay_arena_new(int64_t capacity);
char*            __cay_arena_alloc(ArenaAllocator* arena, int64_t size, int64_t align);
void             __cay_arena_reset(ArenaAllocator* arena);
void             __cay_arena_free(ArenaAllocator* arena);

/* ================================================================
 * Rc 循环引用检测 (rc_cycle.c)
 * ================================================================ */

void __cay_rc_set_detect(int enabled);
void __cay_rc_register(void* block, void* object);
void __cay_rc_unregister(void* block);
void __cay_rc_edge_add(void* owner, void* target_block);
void __cay_rc_check_cycle(void* block);

#ifdef __cplusplus
}
#endif

#endif /* CAYRT_H */
