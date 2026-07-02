/**
 * string_ops.c — Cavvy 运行时字符串操作函数
 *
 * 实现 Cavvy String 类的运行时支持函数。
 * Cavvy 字符串在内部表示为以 null 结尾的 C 字符串 (char*)。
 * 所有函数在空指针输入时均进行安全检查，返回空字符串或 false/-1。
 */

#include "cayrt.h"
#include <stdlib.h>
#include <string.h>
#include <ctype.h>

/** 空字符串常量，用于在分配失败或空指针时返回 */
static const char __cay_empty_string[1] = "";

/* ----------------------------------------------------------------
 * 内部辅助函数 (等价于原 LLVM IR 中的 internal 函数)
 * ---------------------------------------------------------------- */

/** 比较两个字符串的前 n 个字符 (相当于 strncmp 的纯 C 实现) */
static int32_t __cay_strncmp(const char* s1, const char* s2, int64_t n) {
    int64_t i;
    for (i = 0; i < n; i++) {
        char c1 = s1[i];
        char c2 = s2[i];
        if (c1 == 0 || c2 == 0) {
            if (c1 == 0 && c2 == 0) return 0;
            return c1 == 0 ? -1 : 1;
        }
        if ((unsigned char)c1 < (unsigned char)c2) return -1;
        if ((unsigned char)c1 > (unsigned char)c2) return 1;
    }
    return 0;
}

/** 将字符转换为小写 (仅处理 A-Z) */
static char __cay_to_lower(char c) {
    if (c >= 'A' && c <= 'Z') {
        return c + 32;
    }
    return c;
}

/** 判断是否为空白字符 (空格、制表符、换行、回车) */
static bool __cay_is_whitespace(char c) {
    return c == ' ' || c == '\t' || c == '\n' || c == '\r';
}

/* ----------------------------------------------------------------
 * 公共运行时函数
 * ---------------------------------------------------------------- */

/** 字符串拼接 */
char* __cay_string_concat(const char* a, const char* b) {
    if (!a) a = __cay_empty_string;
    if (!b) b = __cay_empty_string;

    int64_t len_a = (int64_t)strlen(a);
    int64_t len_b = (int64_t)strlen(b);
    int64_t total_len = len_a + len_b;

    char* result = calloc(1, (size_t)(total_len + 1));
    if (!result) return (char*)__cay_empty_string;

    memcpy(result, a, (size_t)len_a);
    memcpy(result + len_a, b, (size_t)len_b);
    result[total_len] = '\0';

    return result;
}

/** 字符串长度 */
int32_t __cay_string_length(const char* str) {
    if (!str) return 0;
    return (int32_t)strlen(str);
}

/** 子串提取: substring(beginIndex, endIndex) */
char* __cay_string_substring(const char* str, int32_t begin, int32_t end) {
    if (!str) return (char*)__cay_empty_string;

    int32_t total_len = (int32_t)strlen(str);

    /* 处理负数索引 */
    if (begin < 0) begin = 0;
    /* 处理 end > length */
    if (end > total_len) end = total_len;
    /* 确保 begin <= end */
    if (begin > end) begin = end;

    int32_t sub_len = end - begin;
    char* result = calloc(1, (size_t)(sub_len + 1));
    if (!result) return (char*)__cay_empty_string;

    memcpy(result, str + begin, (size_t)sub_len);
    result[sub_len] = '\0';
    return result;
}

/** 查找子串位置 (indexOf) */
int32_t __cay_string_indexof(const char* str, const char* substr) {
    if (!str || !substr) return -1;

    int64_t str_len = (int64_t)strlen(str);
    int64_t substr_len = (int64_t)strlen(substr);

    if (substr_len == 0) return 0;
    if (substr_len > str_len) return -1;

    int64_t max_pos = str_len - substr_len;
    for (int64_t i = 0; i <= max_pos; i++) {
        if (__cay_strncmp(str + i, substr, substr_len) == 0) {
            return (int32_t)i;
        }
    }
    return -1;
}

/** 从指定位置查找子串 (indexOf with start) */
int32_t __cay_string_indexof_from(const char* str, const char* substr, int32_t start) {
    if (!str || !substr) return -1;

    int64_t str_len = (int64_t)strlen(str);
    int64_t substr_len = (int64_t)strlen(substr);

    if (substr_len == 0) return 0;

    int64_t start_ext = (int64_t)start;
    if (start_ext < 0) return -1;
    if (start_ext >= str_len) return -1;
    if (substr_len > str_len) return -1;

    int64_t max_pos = str_len - substr_len;
    for (int64_t i = start_ext; i <= max_pos; i++) {
        if (__cay_strncmp(str + i, substr, substr_len) == 0) {
            return (int32_t)i;
        }
    }
    return -1;
}

/** 反向查找子串位置 (lastIndexOf) */
int32_t __cay_string_lastindexof(const char* str, const char* substr) {
    if (!str || !substr) return -1;

    int64_t str_len = (int64_t)strlen(str);
    int64_t substr_len = (int64_t)strlen(substr);

    if (substr_len == 0) return (int32_t)str_len;
    if (substr_len > str_len) return -1;

    int64_t max_pos = str_len - substr_len;
    for (int64_t i = max_pos; i >= 0; i--) {
        if (__cay_strncmp(str + i, substr, substr_len) == 0) {
            return (int32_t)i;
        }
    }
    return -1;
}

/** 检查前缀 */
bool __cay_string_startswith(const char* str, const char* prefix) {
    if (!str || !prefix) return false;

    int64_t str_len = (int64_t)strlen(str);
    int64_t prefix_len = (int64_t)strlen(prefix);

    if (prefix_len == 0) return true;
    if (prefix_len > str_len) return false;

    return __cay_strncmp(str, prefix, prefix_len) == 0;
}

/** 检查后缀 */
bool __cay_string_endswith(const char* str, const char* suffix) {
    if (!str || !suffix) return false;

    int64_t str_len = (int64_t)strlen(str);
    int64_t suffix_len = (int64_t)strlen(suffix);

    if (suffix_len == 0) return true;
    if (suffix_len > str_len) return false;

    return __cay_strncmp(str + (str_len - suffix_len), suffix, suffix_len) == 0;
}

/** 获取指定位置的字符 */
char __cay_string_charat(const char* str, int32_t index) {
    if (!str) return 0;

    int32_t len = (int32_t)strlen(str);
    if (index < 0 || index >= len) return 0;

    return str[index];
}

/** 字符串替换 (替换所有出现) */
char* __cay_string_replace(const char* str, const char* old, const char* new_str) {
    if (!str || !old || !new_str) {
        if (!str) return (char*)__cay_empty_string;
        /* 返回原串副本 */
        size_t len = strlen(str);
        char* copy = calloc(1, len + 1);
        if (!copy) return (char*)__cay_empty_string;
        memcpy(copy, str, len);
        return copy;
    }

    int64_t str_len = (int64_t)strlen(str);
    int64_t old_len = (int64_t)strlen(old);
    int64_t new_len = (int64_t)strlen(new_str);

    if (old_len == 0) {
        /* old 为空，返回原串副本 */
        char* copy = calloc(1, (size_t)(str_len + 1));
        if (!copy) return (char*)__cay_empty_string;
        memcpy(copy, str, (size_t)str_len);
        return copy;
    }

    /* 统计出现次数 */
    int64_t count = 0;
    int64_t max_pos = str_len - old_len;
    for (int64_t pos = 0; pos <= max_pos; ) {
        if (__cay_strncmp(str + pos, old, old_len) == 0) {
            count++;
            pos += old_len;
        } else {
            pos++;
        }
    }

    /* 计算新字符串大小 */
    int64_t result_size = str_len + count * (new_len - old_len);
    char* result = calloc(1, (size_t)(result_size + 1));
    if (!result) return (char*)__cay_empty_string;

    /* 构建新字符串 */
    int64_t src_pos = 0, dst_pos = 0;
    while (src_pos <= max_pos) {
        if (__cay_strncmp(str + src_pos, old, old_len) == 0) {
            memcpy(result + dst_pos, new_str, (size_t)new_len);
            src_pos += old_len;
            dst_pos += new_len;
        } else {
            result[dst_pos++] = str[src_pos++];
        }
    }

    /* 复制剩余部分 */
    int64_t remaining = str_len - src_pos;
    if (remaining > 0) {
        memcpy(result + dst_pos, str + src_pos, (size_t)remaining);
    }
    result[result_size] = '\0';

    return result;
}

/** 检查是否为空字符串 */
bool __cay_string_isempty(const char* str) {
    if (!str) return true;
    return str[0] == '\0';
}

/** 字符串比较 (区分大小写) */
bool __cay_string_equals(const char* str1, const char* str2) {
    if (!str1 && !str2) return true;
    if (!str1 || !str2) return false;
    return strcmp(str1, str2) == 0;
}

/** 字符串比较 (不区分大小写) */
bool __cay_string_equals_ignorecase(const char* str1, const char* str2) {
    if (!str1 && !str2) return true;
    if (!str1 || !str2) return false;

    while (1) {
        char c1 = *str1;
        char c2 = *str2;

        if (c1 == 0 && c2 == 0) return true;
        if (c1 == 0 || c2 == 0) return false;

        if (__cay_to_lower(c1) != __cay_to_lower(c2)) return false;

        str1++;
        str2++;
    }
}

/** 去除首尾空白 */
char* __cay_string_trim(const char* str) {
    if (!str) return NULL;

    int64_t len = (int64_t)strlen(str);

    /* 找起始非空白位置 */
    int64_t start = 0;
    while (start < len && __cay_is_whitespace(str[start])) {
        start++;
    }

    /* 全部是空白 */
    if (start == len) {
        return calloc(1, 1);
    }

    /* 找结束非空白位置 */
    int64_t end = len - 1;
    while (end > start && __cay_is_whitespace(str[end])) {
        end--;
    }

    int64_t copy_len = end - start + 1;
    char* result = calloc(1, (size_t)(copy_len + 1));
    if (result) {
        memcpy(result, str + start, (size_t)copy_len);
        result[copy_len] = '\0';
    }
    return result;
}

/** 转换为小写 */
char* __cay_string_to_lower(const char* str) {
    if (!str) return (char*)__cay_empty_string;

    int64_t len = (int64_t)strlen(str);
    char* result = calloc(1, (size_t)(len + 1));
    if (!result) return (char*)__cay_empty_string;

    for (int64_t i = 0; i < len; i++) {
        result[i] = __cay_to_lower(str[i]);
    }
    result[len] = '\0';
    return result;
}

/** 转换为大写 (将字符转换为大写) */
static char __cay_to_upper(char c) {
    if (c >= 'a' && c <= 'z') {
        return c - 32;
    }
    return c;
}

/** 转换为大写 */
char* __cay_string_to_upper(const char* str) {
    if (!str) return (char*)__cay_empty_string;

    int64_t len = (int64_t)strlen(str);
    char* result = calloc(1, (size_t)(len + 1));
    if (!result) return (char*)__cay_empty_string;

    for (int64_t i = 0; i < len; i++) {
        result[i] = __cay_to_upper(str[i]);
    }
    result[len] = '\0';
    return result;
}

/** 检查字符串是否包含子串 */
bool __cay_string_contains(const char* str, const char* substr) {
    if (!str || !substr) return false;
    return __cay_string_indexof(str, substr) != -1;
}

/** 字符串比较 (按字典序，区分大小写)，返回 -1/0/1 */
int32_t __cay_string_compareto(const char* str1, const char* str2) {
    if (!str1 && !str2) return 0;
    if (!str1) return -1;
    if (!str2) return 1;

    int cmp = strcmp(str1, str2);
    if (cmp < 0) return -1;
    if (cmp > 0) return 1;
    return 0;
}

/** 字符串哈希值 (Java String.hashCode 算法: s[0]*31^(n-1) + ... + s[n-1]) */
int32_t __cay_string_hashcode(const char* str) {
    if (!str) return 0;
    int32_t h = 0;
    for (const char* p = str; *p; ++p) {
        h = h * 31 + (int32_t)(unsigned char)(*p);
    }
    return h;
}
