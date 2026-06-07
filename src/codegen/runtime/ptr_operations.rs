//! 指针操作运行时函数
//!
//! 本模块提供对原始内存的读写操作，用于FFI交互。
//! 时间复杂度: O(1) 空间复杂度: O(1)

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成 __cay_read_ptr 运行时函数
    /// 从指定内存地址读取指针值 (64位)
    /// 时间复杂度: O(1) 空间复杂度: O(1)
    pub(super) fn emit_read_ptr_runtime(&mut self) {
        // __cay_read_ptr: 从指定地址读取64位指针值
        self.emit_raw("define i64 @__cay_read_ptr(i64 %ptr) {");
        self.emit_raw("entry:");
        // 将i64转换为i64*指针
        self.emit_raw("  %ptr_i64 = inttoptr i64 %ptr to i64*");
        // 读取指针值
        self.emit_raw("  %value = load i64, i64* %ptr_i64");
        self.emit_raw("  ret i64 %value");
        self.emit_raw("}");
        self.emit_raw("");
    }

    /// 生成 __cay_ptr_to_string 运行时函数
    /// 将C字符串指针转换为Cavvy字符串
    /// 时间复杂度: O(n) n为字符串长度 空间复杂度: O(n)
    pub(super) fn emit_ptr_to_string_runtime(&mut self) {
        // __cay_ptr_to_string: 将C字符串指针转换为Cavvy字符串
        self.emit_raw("define i8* @__cay_ptr_to_string(i64 %ptr) {");
        self.emit_raw("entry:");
        // 检查空指针
        self.emit_raw("  %is_null = icmp eq i64 %ptr, 0");
        self.emit_raw("  br i1 %is_null, label %return_empty, label %process");

        // 返回空字符串
        self.emit_raw("return_empty:");
        self.emit_raw(
            "  %empty_str = getelementptr [1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0",
        );
        self.emit_raw("  ret i8* %empty_str");

        // 处理字符串
        self.emit_raw("process:");
        // 将i64转换为i8*指针
        self.emit_raw("  %str_ptr = inttoptr i64 %ptr to i8*");
        // 计算字符串长度
        self.emit_raw("  %len = call i64 @strlen(i8* %str_ptr)");
        // 检查长度是否为0
        self.emit_raw("  %is_zero = icmp eq i64 %len, 0");
        self.emit_raw("  br i1 %is_zero, label %return_empty, label %alloc");

        // 分配内存 (长度 + 1 for null terminator)
        self.emit_raw("alloc:");
        self.emit_raw("  %len_plus_1 = add i64 %len, 1");
        self.emit_raw("  %new_ptr = call i8* @calloc(i64 1, i64 %len_plus_1)");
        // 检查分配是否成功
        self.emit_raw("  %alloc_null = icmp eq i8* %new_ptr, null");
        self.emit_raw("  br i1 %alloc_null, label %return_empty, label %copy");

        // 复制字符串
        self.emit_raw("copy:");
        self.emit_raw("  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %new_ptr, i8* %str_ptr, i64 %len, i1 false)");
        // 添加null terminator
        self.emit_raw("  %end_ptr = getelementptr i8, i8* %new_ptr, i64 %len");
        self.emit_raw("  store i8 0, i8* %end_ptr");
        self.emit_raw("  ret i8* %new_ptr");
        self.emit_raw("}");
        self.emit_raw("");
    }

    /// 生成 __cay_write_ptr 运行时函数
    /// 向指定内存地址写入指针值 (64位)
    /// 时间复杂度: O(1) 空间复杂度: O(1)
    pub(super) fn emit_write_ptr_runtime(&mut self) {
        // __cay_write_ptr: 向指定地址写入64位指针值
        self.emit_raw("define void @__cay_write_ptr(i64 %ptr, i64 %value) {");
        self.emit_raw("entry:");
        // 将i64转换为i64*指针
        self.emit_raw("  %ptr_i64 = inttoptr i64 %ptr to i64*");
        // 写入指针值
        self.emit_raw("  store i64 %value, i64* %ptr_i64");
        self.emit_raw("  ret void");
        self.emit_raw("}");
        self.emit_raw("");
    }

    /// 生成 __cay_write_int 运行时函数
    /// 向指定内存地址写入32位整数值
    /// 时间复杂度: O(1) 空间复杂度: O(1)
    pub(super) fn emit_write_int_runtime(&mut self) {
        // __cay_write_int: 向指定地址写入32位整数值
        self.emit_raw("define void @__cay_write_int(i64 %ptr, i32 %value) {");
        self.emit_raw("entry:");
        // 将i64转换为i32*指针
        self.emit_raw("  %ptr_i32 = inttoptr i64 %ptr to i32*");
        // 写入整数值
        self.emit_raw("  store i32 %value, i32* %ptr_i32");
        self.emit_raw("  ret void");
        self.emit_raw("}");
        self.emit_raw("");
    }

    /// 生成 __cay_read_int 运行时函数
    /// 从指定内存地址读取32位整数值
    /// 时间复杂度: O(1) 空间复杂度: O(1)
    pub(super) fn emit_read_int_runtime(&mut self) {
        // __cay_read_int: 从指定地址读取32位整数值
        self.emit_raw("define i32 @__cay_read_int(i64 %ptr) {");
        self.emit_raw("entry:");
        // 将i64转换为i32*指针
        self.emit_raw("  %ptr_i32 = inttoptr i64 %ptr to i32*");
        // 读取整数值
        self.emit_raw("  %value = load i32, i32* %ptr_i32");
        self.emit_raw("  ret i32 %value");
        self.emit_raw("}");
        self.emit_raw("");
    }

    /// 生成 __cay_write_byte 运行时函数
    /// 向指定内存地址写入8位字节值
    /// 时间复杂度: O(1) 空间复杂度: O(1)
    pub(super) fn emit_write_byte_runtime(&mut self) {
        // __cay_write_byte: 向指定地址写入8位字节值
        self.emit_raw("define void @__cay_write_byte(i64 %ptr, i32 %value) {");
        self.emit_raw("entry:");
        // 将i64转换为i8*指针
        self.emit_raw("  %ptr_i8 = inttoptr i64 %ptr to i8*");
        // 将i32值截断为i8
        self.emit_raw("  %val_i8 = trunc i32 %value to i8");
        // 写入字节值
        self.emit_raw("  store i8 %val_i8, i8* %ptr_i8");
        self.emit_raw("  ret void");
        self.emit_raw("}");
        self.emit_raw("");
    }
}
