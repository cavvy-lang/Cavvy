/**
 * memory.c — Cavvy 运行时内存操作函数
 *
 * 提供按字节设置和复制内存的运行时支持。
 * 指针参数以 int64_t 形式传入，内部转换为 char* 操作。
 * 包含空指针安全检查。
 * 
 * SPDX-License-Identifier: GPL-3.0 WITH Cavvy-RLE
 * This file is part of Cavvy Runtime Library.
 * See LICENSE-EXCEPTION.md for the exact exception terms.
 */

#include "cayrt.h"
#include <string.h>
#include <stdint.h>

/** 按字节设置内存 (空指针安全) */
void __cay_memset_byte(int64_t ptr, int32_t value, int32_t n) {
    if (ptr == 0) return;

    char* p = (char*)(intptr_t)ptr;
    memset(p, value & 0xFF, (size_t)n);
}

/** 按字节复制内存 (空指针安全) */
void __cay_memcpy_byte(int64_t dest, int64_t src, int32_t n) {
    if (dest == 0 || src == 0) return;

    char* d = (char*)(intptr_t)dest;
    const char* s = (const char*)(intptr_t)src;
    memcpy(d, s, (size_t)n);
}
