/**
 * ptr_ops.c — Cavvy 运行时指针/缓冲区操作函数
 *
 * 提供对原始内存的读写操作，用于 FFI 交互。
 * 所有指针参数以 int64_t 形式传入，内部转换为 void*。
 * 
 * SPDX-License-Identifier: GPL-3.0 WITH Cavvy-RLE
 * This file is part of Cavvy Runtime Library.
 * See LICENSE-EXCEPTION.md for the exact exception terms.
 */

#include "cayrt.h"
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

/** 空字符串常量 */
static const char __cay_empty_string[1] = "";

/** 从指定地址读取 64 位指针值 */
int64_t __cay_read_ptr(int64_t ptr) {
    return *(int64_t*)(intptr_t)ptr;
}

/** 将 C 字符串指针转换为 Cavvy 字符串 (复制数据) */
char* __cay_ptr_to_string(int64_t ptr) {
    if (ptr == 0) return (char*)__cay_empty_string;

    const char* str_ptr = (const char*)(intptr_t)ptr;
    int64_t len = (int64_t)strlen(str_ptr);

    if (len == 0) return (char*)__cay_empty_string;

    char* new_ptr = calloc(1, (size_t)(len + 1));
    if (!new_ptr) return (char*)__cay_empty_string;

    memcpy(new_ptr, str_ptr, (size_t)len);
    new_ptr[len] = '\0';
    return new_ptr;
}

/** 向指定地址写入 64 位指针值 */
void __cay_write_ptr(int64_t ptr, int64_t value) {
    *(int64_t*)(intptr_t)ptr = value;
}

/** 向指定地址写入 32 位整数值 */
void __cay_write_int(int64_t ptr, int32_t value) {
    *(int32_t*)(intptr_t)ptr = value;
}

/** 从指定地址读取 32 位整数值 */
int32_t __cay_read_int(int64_t ptr) {
    return *(int32_t*)(intptr_t)ptr;
}

/** 向指定地址写入 8 位字节值 */
void __cay_write_byte(int64_t ptr, int32_t value) {
    *(char*)(intptr_t)ptr = (char)value;
}

/** 将缓冲区内容转换为字符串 */
char* __cay_buffer_to_string(int64_t buffer, int32_t length) {
    if (length <= 0) return (char*)__cay_empty_string;

    char* ptr = calloc(1, (size_t)(length + 1));
    if (!ptr) return (char*)__cay_empty_string;

    const char* src = (const char*)(intptr_t)buffer;
    memcpy(ptr, src, (size_t)length);
    ptr[length] = '\0';
    return ptr;
}
