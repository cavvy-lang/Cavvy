//! 实现 __cay_buffer_to_string 函数，用于将缓冲区转换为字符串。

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成 __cay_buffer_to_string 运行时函数
    /// 将指定长度的缓冲区内容复制到新分配的字符串中
    pub(super) fn emit_buffer_to_string_runtime(&mut self) {
        self.emit_raw("define i8* @__cay_buffer_to_string(i64 %buffer, i32 %length) {");
        self.emit_raw("entry:");
        // 检查长度是否为0
        self.emit_raw("  %is_zero = icmp sle i32 %length, 0");
        self.emit_raw("  br i1 %is_zero, label %return_empty, label %alloc");

        // 返回空字符串
        self.emit_raw("return_empty:");
        self.emit_raw(
            "  %empty_str = getelementptr [1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0",
        );
        self.emit_raw("  ret i8* %empty_str");

        // 分配内存
        self.emit_raw("alloc:");
        self.emit_raw("  %len_plus_1 = add i32 %length, 1");
        self.emit_raw("  %size = sext i32 %len_plus_1 to i64");
        self.emit_raw("  %ptr = call i8* @calloc(i64 1, i64 %size)");

        // 检查分配是否成功
        self.emit_raw("  %is_null = icmp eq i8* %ptr, null");
        self.emit_raw("  br i1 %is_null, label %return_empty, label %copy");

        // 复制数据
        self.emit_raw("copy:");
        self.emit_raw("  %src = inttoptr i64 %buffer to i8*");
        self.emit_raw("  %len_i64 = sext i32 %length to i64");
        self.emit_raw(
            "  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %ptr, i8* %src, i64 %len_i64, i1 false)",
        );

        // 添加null terminator
        self.emit_raw("  %end_ptr = getelementptr i8, i8* %ptr, i64 %len_i64");
        self.emit_raw("  store i8 0, i8* %end_ptr");

        self.emit_raw("  ret i8* %ptr");
        self.emit_raw("}");
        self.emit_raw("");
    }
}
