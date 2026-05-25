/**
 * array_ops.c — Cavvy 运行时数组操作与命令行参数支持函数
 *
 * Cavvy 数组内存布局:
 *   [长度:i32 (4B)][padding (4B)][元素0][元素1]...
 *   返回指针指向元素0，长度字段在 -8 偏移处。
 *
 * 字符串数组元素为 char* 指针 (8B each)。
 */

#include "cayrt.h"
#include <stdlib.h>
#include <string.h>

/** 空字符串常量 */
static const char __cay_empty_string[1] = "";

/**
 * 创建 String[] 数组
 * 布局: [4B length][4B pad][8B*size elements]
 * 返回: 指向数据区 (元素0) 的指针
 */
char** __cay_create_string_array(int32_t size) {
    int64_t header = 8;  /* 4B length + 4B padding */
    int64_t elem_size = (int64_t)size * 8;
    int64_t total_size = header + elem_size;

    char* arr_i8 = calloc(1, (size_t)total_size);

    /* 设置长度字段 (offset 0) */
    *(int32_t*)arr_i8 = size;

    /* 数据区从 offset 8 开始 */
    return (char**)(arr_i8 + 8);
}

/**
 * 将 C 字符串转换为 Cavvy String 对象
 * Cavvy String 布局: [4B length][4B ptr_to_data][...data...]
 * 注意: 此函数返回指向数据区的指针 (data start)，与 create_string_array 不同！
 *
 * 实际上在 args_support 中，cstr_to_string 创建一个更复杂的 String 对象结构。
 * 但经过代码审查，Cavvy 的内部 String 表示就是简单的 char* (null-terminated C string)。
 * 此函数作为安全包装器，复制 C 字符串到堆上。
 */
char* __cay_cstr_to_string(const char* cstr) {
    if (!cstr) return (char*)__cay_empty_string;

    int64_t len = (int64_t)strlen(cstr);
    char* data = calloc(1, (size_t)(len + 1));
    if (!data) return (char*)__cay_empty_string;

    memcpy(data, cstr, (size_t)(len + 1));
    return data;
}

/** 设置数组元素 (引用类型) */
void __cay_array_set_ref(char** arr, int32_t idx, char* value) {
    arr[idx] = value;
}

/** 获取数组元素 (引用类型) */
char* __cay_array_get_ref(char** arr, int32_t idx) {
    return arr[idx];
}

/**
 * 获取数组长度
 * 长度存储在 arr 指针前 8 字节处 (作为 i32 + 4B padding)
 */
int32_t __cay_array_length(char** arr) {
    /* arr 指向数据区 (元素0)，长度在 -8 偏移处 */
    char* base = (char*)arr - 8;
    return *(int32_t*)base;
}
