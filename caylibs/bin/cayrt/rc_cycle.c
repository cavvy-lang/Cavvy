/**
 * rc_cycle.c — Rc<T> 循环引用运行时检测
 *
 * 提供 --detect-cycles 开启后的运行时支持。
 * 所有导出函数均空指针安全；未启用检测时所有操作均为 O(1) 快速返回。
 *
 * 控制块布局（SmartPtr.cay 约定）：
 *   [i64 refcount, i64 weak_count, i64 object_ptr]
 *   offset 0: refcount
 *   offset 8: weak_count
 *   offset 16: object_ptr
 *
 * 检测策略：
 *   - 记录所有仍处于生命周期内的 Rc 控制块。
 *   - 当强引用计数归零时注销该块。
 *   - 当某次 drop 后强引用计数仍大于 0，说明仍有其它强引用持有该对象；
 *     若同一个控制块多次进入此状态且始终未归零，则极有可能存在循环引用。
 *   该实现为保守、最佳努力（best-effort）检测，不依赖对象内存布局扫描。
 * 
 * SPDX-License-Identifier: GPL-3.0 WITH Cavvy-RLE
 * This file is part of Cavvy Runtime Library.
 * See LICENSE-EXCEPTION.md for the exact exception terms.
 */

#include "cayrt.h"
#include <stdint.h>
#include <stdbool.h>
#include <stdlib.h>
#include <stdio.h>

/** 全局检测开关，由编译器生成的主入口在启动时设置 */
static volatile int g_detect_enabled = 0;

/** Rc 控制块字段偏移 */
#define RC_OFFSET_REFCOUNT   0
#define RC_OFFSET_WEAKCOUNT  8
#define RC_OFFSET_OBJECT_PTR 16

/** 活动 Rc 节点：登记每个仍处于生命周期内的控制块 */
typedef struct RcNode {
    void*  block;        /**< 控制块指针 */
    void*  object;       /**< 托管对象指针 */
    int    drop_hits;    /**< 进入 drop 且 refcount 未归零的次数 */
    struct RcNode* next;
} RcNode;

static RcNode* g_rc_nodes = NULL;

/** 读取 i64（空指针安全） */
static int64_t read_i64(void* ptr, int offset) {
    if (!ptr) return 0;
    return *(int64_t*)((char*)ptr + offset);
}

/** 在节点链表中查找 block 对应的节点；若不存在则返回 NULL */
static RcNode* rc_node_find(void* block) {
    for (RcNode* n = g_rc_nodes; n; n = n->next) {
        if (n->block == block) {
            return n;
        }
    }
    return NULL;
}

/** 启用/禁用循环引用检测 */
void __cay_rc_set_detect(int enabled) {
    g_detect_enabled = enabled;
}

/** 注册一个新创建的 Rc 控制块 */
void __cay_rc_register(void* block, void* object) {
    if (!g_detect_enabled || !block) return;

    /* 已存在则更新对象指针（例如同一 block 被复用） */
    RcNode* node = rc_node_find(block);
    if (node) {
        node->object = object;
        node->drop_hits = 0;
        return;
    }

    node = (RcNode*)malloc(sizeof(RcNode));
    if (!node) return;
    node->block = block;
    node->object = object;
    node->drop_hits = 0;
    node->next = g_rc_nodes;
    g_rc_nodes = node;
}

/** 注销一个已被释放的 Rc 控制块 */
void __cay_rc_unregister(void* block) {
    if (!g_detect_enabled || !block) return;

    RcNode** curr = &g_rc_nodes;
    while (*curr) {
        if ((*curr)->block == block) {
            RcNode* to_free = *curr;
            *curr = (*curr)->next;
            free(to_free);
            return;
        }
        curr = &(*curr)->next;
    }
}

/** 记录一次 Rc 字段赋值：owner 对象持有了 target_block 的强引用。
 *  当前实现保留 API 以便未来通过静态/动态对象布局扫描精确追踪边。 */
void __cay_rc_edge_add(void* owner, void* target_block) {
    (void)owner;
    (void)target_block;
    /* 当前 best-effort 策略不依赖显式边记录。 */
}

/** 检查 block 是否处于循环引用中。
 *  当某次 drop 后强引用计数仍大于 0 时输出警告。
 *  这是保守的 best-effort 检测：正常共享所有权也会触发，因此仅在
 *  --detect-cycles 调试模式下启用。 */
void __cay_rc_check_cycle(void* block) {
    if (!g_detect_enabled || !block) return;

    RcNode* node = rc_node_find(block);
    if (!node) {
        /* 延迟注册：drop 时首次见到该 block */
        void* object = (void*)read_i64(block, RC_OFFSET_OBJECT_PTR);
        __cay_rc_register(block, object);
        node = rc_node_find(block);
        if (!node) return;
    }

    node->drop_hits += 1;
    int64_t refcount = read_i64(block, RC_OFFSET_REFCOUNT);
    if (refcount > 0) {
        printf("[Cavvy] warning: potential Rc reference cycle detected "
               "at block %p (object %p, remaining refcount %lld)\n",
               block, node->object, (long long)refcount);
    }
}
