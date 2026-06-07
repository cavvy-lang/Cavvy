//! 字符串子串运行时函数

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成字符串子串运行时函数
    pub(super) fn emit_string_substring_runtime(&mut self) {
        // substring(beginIndex, endIndex) - 两个参数版本
        self.emit_raw("define i8* @__cay_string_substring(i8* %str, i32 %begin, i32 %end) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 空指针安全检查");
        self.emit_raw("  %is_null = icmp eq i8* %str, null");
        self.emit_raw("  br i1 %is_null, label %null_case, label %check_bounds");
        self.emit_raw("");
        self.emit_raw("null_case:");
        self.emit_raw(
            "  ret i8* getelementptr ([1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0)",
        );
        self.emit_raw("");
        self.emit_raw("check_bounds:");
        self.emit_raw("  %total_len = call i64 @strlen(i8* %str)");
        self.emit_raw("  %total_len_i32 = trunc i64 %total_len to i32");
        self.emit_raw("  ; 处理负数索引");
        self.emit_raw("  %begin_neg = icmp slt i32 %begin, 0");
        self.emit_raw("  %begin_final = select i1 %begin_neg, i32 0, i32 %begin");
        self.emit_raw("  ; 处理end > length的情况");
        self.emit_raw("  %end_too_large = icmp sgt i32 %end, %total_len_i32");
        self.emit_raw("  %end_final = select i1 %end_too_large, i32 %total_len_i32, i32 %end");
        self.emit_raw("  ; 确保begin <= end");
        self.emit_raw("  %begin_gt_end = icmp sgt i32 %begin_final, %end_final");
        self.emit_raw(
            "  %begin_clamped = select i1 %begin_gt_end, i32 %end_final, i32 %begin_final",
        );
        self.emit_raw("  ; 计算子串长度");
        self.emit_raw("  %sub_len = sub i32 %end_final, %begin_clamped");
        self.emit_raw("  %sub_len_i64 = sext i32 %sub_len to i64");
        self.emit_raw("  %buf_size = add i64 %sub_len_i64, 1");
        self.emit_raw("  ; 分配内存");
        self.emit_raw("  %result = call i8* @calloc(i64 1, i64 %buf_size)");
        self.emit_raw("  ; calloc 失败保护：返回空字符串而非崩溃");
        self.emit_raw("  %result_null = icmp eq i8* %result, null");
        self.emit_raw("  br i1 %result_null, label %alloc_fail, label %do_memcpy");
        self.emit_raw("");
        self.emit_raw("alloc_fail:");
        self.emit_raw(
            "  ret i8* getelementptr ([1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0)",
        );
        self.emit_raw("");
        self.emit_raw("do_memcpy:");
        self.emit_raw("  ; 计算源地址偏移");
        self.emit_raw("  %begin_i64 = sext i32 %begin_clamped to i64");
        self.emit_raw("  %src_ptr = getelementptr i8, i8* %str, i64 %begin_i64");
        self.emit_raw("  ; 复制子串");
        self.emit_raw("  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %result, i8* %src_ptr, i64 %sub_len_i64, i1 false)");
        self.emit_raw("  ; 添加null终止符");
        self.emit_raw("  %end_ptr = getelementptr i8, i8* %result, i64 %sub_len_i64");
        self.emit_raw("  store i8 0, i8* %end_ptr");
        self.emit_raw("  ret i8* %result");
        self.emit_raw("}");
        self.emit_raw("");
    }
}
