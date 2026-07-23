/**
 * type_conv.c — Cavvy 运行时类型转换函数
 *
 * 将 Cavvy 基础类型转换为字符串表示。
 * 所有函数在 calloc 失败时返回空字符串，避免崩溃。
 * 
 * SPDX-License-Identifier: GPL-3.0 WITH Cavvy-RLE
 * This file is part of Cavvy Runtime Library.
 * See LICENSE-EXCEPTION.md for the exact exception terms.
 */

#include "cayrt.h"
#include <stdlib.h>
#include <stdio.h>

/** 空字符串常量 */
static const char __cay_empty_string[1] = "";

/** int32_t → 字符串 */
char* __cay_int_to_string(int32_t value) {
    char* buf = calloc(1, 32);
    if (!buf) return (char*)__cay_empty_string;
    snprintf(buf, 32, "%d", value);
    return buf;
}

/** int64_t (long) → 字符串 */
char* __cay_long_to_string(int64_t value) {
    char* buf = calloc(1, 32);
    if (!buf) return (char*)__cay_empty_string;
    snprintf(buf, 32, "%lld", (long long)value);
    return buf;
}

/** float → 字符串 */
char* __cay_float_to_string(float value) {
    char* buf = calloc(1, 64);
    if (!buf) return (char*)__cay_empty_string;
    snprintf(buf, 64, "%f", (double)value);
    return buf;
}

/** double → 字符串 */
char* __cay_double_to_string(double value) {
    char* buf = calloc(1, 64);
    if (!buf) return (char*)__cay_empty_string;
    snprintf(buf, 64, "%f", value);
    return buf;
}

/** bool → 字符串 ("true" / "false") */
char* __cay_bool_to_string(bool value) {
    if (value) {
        return "true";
    } else {
        return "false";
    }
}

/** char → 字符串 (单字符 + null) */
char* __cay_char_to_string(char value) {
    char* buf = calloc(1, 2);
    if (!buf) return (char*)__cay_empty_string;
    buf[0] = value;
    buf[1] = '\0';
    return buf;
}
