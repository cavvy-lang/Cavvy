// Cavvy 内存分配器系统 - 0.5.0.0
// 提供 Allocator trait、GlobalAlloc、Arena 分配器实现

use crate::codegen::context::IRGenerator;
use crate::types::Type;

/// 分配器类型枚举
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AllocatorType {
    /// 全局堆分配器（封装 malloc/free）
    GlobalAlloc,
    /// Arena 线性分配器
    Arena,
    /// 栈分配器（scope 块）
    Stack,
}

impl AllocatorType {
    /// 获取分配器的 LLVM 类型名称
    pub fn llvm_type_name(&self) -> &'static str {
        match self {
            AllocatorType::GlobalAlloc => "GlobalAlloc",
            AllocatorType::Arena => "ArenaAllocator",
            AllocatorType::Stack => "StackAllocator",
        }
    }

    /// 获取分配器结构体的 LLVM IR 定义
    pub fn llvm_struct_def(&self) -> &'static str {
        match self {
            AllocatorType::GlobalAlloc => {
                // GlobalAlloc 是一个空结构体，仅作为标记类型
                "%GlobalAlloc = type { i8 }"
            }
            AllocatorType::Arena => {
                // Arena 分配器结构体
                // { buffer: i8*, current: i8*, end: i8*, prev: %ArenaAllocator* }
                "%ArenaAllocator = type { i8*, i8*, i8*, %ArenaAllocator* }"
            }
            AllocatorType::Stack => {
                // 栈分配器结构体
                // { base: i8*, marker: i64 }
                "%StackAllocator = type { i8*, i64 }"
            }
        }
    }
}

/// 分配器接口方法定义
pub struct AllocatorMethods;

impl AllocatorMethods {
    /// 生成 Allocator.allocate 方法声明
    ///
    /// # Arguments
    /// * `allocator_type` - 分配器类型
    /// * `return_ptr` - 返回值指针
    /// * `size` - 分配大小
    /// * `align` - 对齐要求
    ///
    /// # Returns
    /// LLVM IR 代码字符串
    pub fn generate_allocate(
        allocator_type: &AllocatorType,
        return_ptr: &str,
        size: &str,
        align: &str,
    ) -> String {
        match allocator_type {
            AllocatorType::GlobalAlloc => {
                // 使用 malloc 进行堆分配
                format!(
                    "  ; GlobalAlloc.allocate - 使用 malloc 分配\n\
                     {} = call i8* @malloc(i64 {})\n",
                    return_ptr, size
                )
            }
            AllocatorType::Arena => {
                // Arena 线性分配
                format!(
                    "  ; Arena.allocate - 线性分配\n\
                   {} = call i8* @__cay_arena_alloc(%ArenaAllocator* %arena, i64 {}, i64 {})\n",
                    return_ptr, size, align
                )
            }
            AllocatorType::Stack => {
                // 栈分配（使用 alloca）
                format!(
                    "  ; Stack.allocate - 栈上分配（使用 alloca）\n\
                   {} = alloca i8, i64 {}\n",
                    return_ptr, size
                )
            }
        }
    }

    /// 生成 Allocator.deallocate 方法声明
    pub fn generate_deallocate(allocator_type: &AllocatorType, ptr: &str) -> String {
        match allocator_type {
            AllocatorType::GlobalAlloc => {
                // 使用 free 释放堆内存
                format!(
                    "  ; GlobalAlloc.deallocate - 使用 free 释放\n\
                   call void @free(i8* {})\n",
                    ptr
                )
            }
            AllocatorType::Arena => {
                // Arena 不单独释放，批量释放
                format!(
                    "  ; Arena.deallocate - 忽略（由 Arena.reset 批量释放）\n\
                   ; 注意：Arena 分配器不支持单独释放\n"
                )
            }
            AllocatorType::Stack => {
                // 栈分配自动释放，无需操作
                format!(
                    "  ; Stack.deallocate - 栈内存自动释放\n\
                   ; 栈内存在 scope 结束时自动释放\n"
                )
            }
        }
    }

    /// 生成 Arena.reset 方法（批量释放）
    pub fn generate_arena_reset(arena_ptr: &str) -> String {
        format!(
            "  ; Arena.reset - 重置分配器，批量释放所有内存\n\
           call void @__cay_arena_reset(%ArenaAllocator* {})\n",
            arena_ptr
        )
    }

    /// 生成 Arena 初始化代码
    pub fn generate_arena_init(arena_ptr: &str, capacity: usize) -> String {
        format!(
            "  ; Arena 初始化\n\
           {} = call %ArenaAllocator* @__cay_arena_new(i64 {})\n",
            arena_ptr, capacity
        )
    }

    /// 生成 Arena 销毁代码
    pub fn generate_arena_destroy(arena_ptr: &str) -> String {
        format!(
            "  ; Arena 销毁\n\
           call void @__cay_arena_free(%ArenaAllocator* {})\n",
            arena_ptr
        )
    }
}

/// 运行时分配器支持函数
pub struct AllocatorRuntime;

impl AllocatorRuntime {
    /// 生成 GlobalAlloc 单例获取代码
    pub fn generate_global_alloc_instance() -> &'static str {
        r#"
; GlobalAlloc 单例实例
@__cay_global_alloc_instance = global %GlobalAlloc zeroinitializer

; 获取 GlobalAlloc 单例
define %GlobalAlloc* @__cay_global_alloc_get() {
entry:
  ret %GlobalAlloc* @__cay_global_alloc_instance
}
"#
    }

    /// 生成 Arena 分配器运行时函数
    pub fn generate_arena_runtime() -> &'static str {
        r#"
; Arena 分配器运行时实现

; Arena 结构体: { buffer: i8*, current: i8*, end: i8*, prev: ArenaAllocator* }
; buffer: 内存块起始地址
; current: 当前分配位置
; end: 内存块结束地址
; prev: 前一个 Arena（用于链式分配）

; 创建新的 Arena 分配器
define %ArenaAllocator* @__cay_arena_new(i64 %capacity) {
entry:
  ; 分配 Arena 结构体内存
  %arena_size = ptrtoint %ArenaAllocator* getelementptr (%ArenaAllocator, %ArenaAllocator* null, i32 1) to i64
  %arena_raw = call i8* @malloc(i64 %arena_size)
  %arena = bitcast i8* %arena_raw to %ArenaAllocator*
  
  ; 分配缓冲区内存
  %buffer = call i8* @malloc(i64 %capacity)
  
  ; 计算 end 指针
  %end = getelementptr i8, i8* %buffer, i64 %capacity
  
  ; 初始化 Arena 字段
  %buffer_ptr = getelementptr %ArenaAllocator, %ArenaAllocator* %arena, i32 0, i32 0
  store i8* %buffer, i8** %buffer_ptr
  
  %current_ptr = getelementptr %ArenaAllocator, %ArenaAllocator* %arena, i32 0, i32 1
  store i8* %buffer, i8** %current_ptr
  
  %end_ptr = getelementptr %ArenaAllocator, %ArenaAllocator* %arena, i32 0, i32 2
  store i8* %end, i8** %end_ptr
  
  %prev_ptr = getelementptr %ArenaAllocator, %ArenaAllocator* %arena, i32 0, i32 3
  store %ArenaAllocator* null, %ArenaAllocator** %prev_ptr
  
  ret %ArenaAllocator* %arena
}

; Arena 分配函数
define i8* @__cay_arena_alloc(%ArenaAllocator* %arena, i64 %size, i64 %align) {
entry:
  ; 获取 current 指针
  %current_ptr = getelementptr %ArenaAllocator, %ArenaAllocator* %arena, i32 0, i32 1
  %current = load i8*, i8** %current_ptr
  
  ; 对齐计算
  %addr_int = ptrtoint i8* %current to i64
  %align_mask = sub i64 %align, 1
  %misalign = and i64 %addr_int, %align_mask
  %padding = sub i64 %align, %misalign
  %padding2 = and i64 %padding, %align_mask
  %aligned_addr = add i64 %addr_int, %padding2
  %aligned_ptr = inttoptr i64 %aligned_addr to i8*
  
  ; 计算新位置
  %new_current = getelementptr i8, i8* %aligned_ptr, i64 %size
  
  ; 检查边界
  %end_ptr = getelementptr %ArenaAllocator, %ArenaAllocator* %arena, i32 0, i32 2
  %end = load i8*, i8** %end_ptr
  %has_space = icmp ule i8* %new_current, %end
  br i1 %has_space, label %allocate, label %overflow

allocate:
  ; 更新 current
  store i8* %new_current, i8** %current_ptr
  ret i8* %aligned_ptr

overflow:
  ; 空间不足，返回 null（或实现链式分配）
  ret i8* null
}

; Arena 重置（批量释放）
define void @__cay_arena_reset(%ArenaAllocator* %arena) {
entry:
  ; 将 current 重置为 buffer 起始位置
  %buffer_ptr = getelementptr %ArenaAllocator, %ArenaAllocator* %arena, i32 0, i32 0
  %buffer = load i8*, i8** %buffer_ptr
  
  %current_ptr = getelementptr %ArenaAllocator, %ArenaAllocator* %arena, i32 0, i32 1
  store i8* %buffer, i8** %current_ptr
  
  ret void
}

; 释放 Arena
define void @__cay_arena_free(%ArenaAllocator* %arena) {
entry:
  ; 释放缓冲区
  %buffer_ptr = getelementptr %ArenaAllocator, %ArenaAllocator* %arena, i32 0, i32 0
  %buffer = load i8*, i8** %buffer_ptr
  call void @free(i8* %buffer)
  
  ; 释放 Arena 结构体
  %arena_raw = bitcast %ArenaAllocator* %arena to i8*
  call void @free(i8* %arena_raw)
  
  ret void
}
"#
    }

    /// 生成所有分配器的运行时声明
    /// 注意: malloc/free/realloc 不在这里声明，因为它们可能通过 extern 导入
    /// LLVM 允许隐式声明外部函数，或用户可通过 FFI 显式导入
    pub fn generate_runtime_declarations() -> &'static str {
        r#"
; 分配器运行时声明

; GlobalAlloc
declare %GlobalAlloc* @__cay_global_alloc_get()

; Arena 分配器
declare %ArenaAllocator* @__cay_arena_new(i64)
declare i8* @__cay_arena_alloc(%ArenaAllocator*, i64, i64)
declare void @__cay_arena_reset(%ArenaAllocator*)
declare void @__cay_arena_free(%ArenaAllocator*)
"#
    }

    /// 生成完整的分配器类型定义（运行时函数本体在 libcayrt.a 中）
    pub fn generate_full_allocator_support() -> String {
        let mut result = String::new();

        // 仅输出类型定义（函数定义已移入 libcayrt.a）
        result.push_str("; ==================== 分配器类型定义 ====================\n\n");
        result.push_str(AllocatorType::GlobalAlloc.llvm_struct_def());
        result.push_str("\n");
        result.push_str(AllocatorType::Arena.llvm_struct_def());
        result.push_str("\n");
        result.push_str(AllocatorType::Stack.llvm_struct_def());
        result.push_str("\n\n");

        result
    }
}

/// 扩展 CodeGenerator 以支持分配器
pub trait AllocatorCodegen {
    /// 生成分配器类型定义
    fn emit_allocator_types(&mut self);

    /// 生成 GlobalAlloc 单例访问
    fn emit_global_alloc_get(&mut self) -> String;

    /// 生成 Arena 分配器创建
    fn emit_arena_create(&mut self, capacity: usize) -> String;

    /// 生成使用指定分配器的内存分配
    fn emit_alloc_with_allocator(
        &mut self,
        allocator_type: &AllocatorType,
        size: &str,
        align: &str,
    ) -> String;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocator_type_enum() {
        assert_eq!(AllocatorType::GlobalAlloc.llvm_type_name(), "GlobalAlloc");
        assert_eq!(AllocatorType::Arena.llvm_type_name(), "ArenaAllocator");
        assert_eq!(AllocatorType::Stack.llvm_type_name(), "StackAllocator");
    }

    #[test]
    fn test_allocator_methods_generate() {
        let code =
            AllocatorMethods::generate_allocate(&AllocatorType::GlobalAlloc, "%ptr", "1024", "8");
        assert!(code.contains("malloc"));

        let code = AllocatorMethods::generate_deallocate(&AllocatorType::GlobalAlloc, "%ptr");
        assert!(code.contains("free"));
    }

    #[test]
    fn test_arena_runtime_generation() {
        let runtime = AllocatorRuntime::generate_arena_runtime();
        assert!(runtime.contains("__cay_arena_new"));
        assert!(runtime.contains("__cay_arena_alloc"));
        assert!(runtime.contains("__cay_arena_reset"));
        assert!(runtime.contains("__cay_arena_free"));
    }
}
