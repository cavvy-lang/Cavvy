/**
 * allocator.c — Cavvy 运行时分配器函数
 *
 * 提供 GlobalAlloc 单例和 Arena 线性分配器的运行时实现。
 *
 * 结构体布局必须与 src/codegen/allocator.rs 中的 LLVM IR 定义完全一致:
 *   GlobalAlloc   = { i8 }           → struct { char dummy; }
 *   ArenaAllocator = { i8*, i8*, i8*, ArenaAllocator* }
 *                 → struct { char* buffer; char* current; char* end; ArenaAllocator* prev; }
 * 
 * SPDX-License-Identifier: GPL-3.0 WITH Cavvy-RLE
 * This file is part of Cavvy Runtime Library.
 * See LICENSE-EXCEPTION.md for the exact exception terms.
 */

#include "cayrt.h"
#include <stdlib.h>
#include <stdint.h>

/* ----------------------------------------------------------------
 * GlobalAlloc 单例
 * ---------------------------------------------------------------- */

/** GlobalAlloc 全局单例实例 */
static GlobalAlloc __cay_global_alloc_instance;

/** 获取 GlobalAlloc 单例的指针 */
GlobalAlloc* __cay_global_alloc_get(void) {
    return &__cay_global_alloc_instance;
}

/* ----------------------------------------------------------------
 * Arena 分配器
 * ---------------------------------------------------------------- */

/** 创建新的 Arena 分配器 */
ArenaAllocator* __cay_arena_new(int64_t capacity) {
    /* 分配 Arena 结构体 */
    ArenaAllocator* arena = malloc(sizeof(ArenaAllocator));
    if (!arena) return NULL;

    /* 分配缓冲区 */
    char* buffer = malloc((size_t)capacity);
    if (!buffer) {
        free(arena);
        return NULL;
    }

    /* 初始化字段 */
    arena->buffer  = buffer;
    arena->current = buffer;
    arena->end     = buffer + capacity;
    arena->prev    = NULL;

    return arena;
}

/** 从 Arena 分配内存 (带对齐) */
char* __cay_arena_alloc(ArenaAllocator* arena, int64_t size, int64_t align) {
    if (!arena) return NULL;

    /* 对齐当前指针 */
    uintptr_t addr = (uintptr_t)arena->current;
    uintptr_t aligned_addr = (addr + (uintptr_t)(align - 1)) & ~(uintptr_t)(align - 1);

    char* aligned_ptr = (char*)aligned_addr;
    char* new_current = aligned_ptr + size;

    /* 检查是否越界 */
    if (new_current > arena->end) {
        return NULL;  /* 空间不足 */
    }

    /* 更新 current 并返回对齐后的指针 */
    arena->current = new_current;
    return aligned_ptr;
}

/** 重置 Arena (批量释放所有分配) */
void __cay_arena_reset(ArenaAllocator* arena) {
    if (!arena) return;
    arena->current = arena->buffer;
}

/** 释放 Arena 及其缓冲区 */
void __cay_arena_free(ArenaAllocator* arena) {
    if (!arena) return;
    if (arena->buffer) {
        free(arena->buffer);
    }
    free(arena);
}
