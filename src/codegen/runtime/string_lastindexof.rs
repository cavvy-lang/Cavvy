//! 字符串反向查找运行时函数

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成字符串反向查找运行时函数
    pub(super) fn emit_string_lastindexof_runtime(&mut self) {
        self.emit_raw("define i32 @__cay_string_lastindexof(i8* %str, i8* %substr) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 空指针安全检查");
        self.emit_raw("  %str_null = icmp eq i8* %str, null");
        self.emit_raw("  %substr_null = icmp eq i8* %substr, null");
        self.emit_raw("  %either_null = or i1 %str_null, %substr_null");
        self.emit_raw("  br i1 %either_null, label %not_found, label %search");
        self.emit_raw("");
        self.emit_raw("not_found:");
        self.emit_raw("  ret i32 -1");
        self.emit_raw("");
        self.emit_raw("search:");
        self.emit_raw("  %str_len = call i64 @strlen(i8* %str)");
        self.emit_raw("  %substr_len = call i64 @strlen(i8* %substr)");
        self.emit_raw("  ; 如果子串为空，返回字符串长度");
        self.emit_raw("  %substr_empty = icmp eq i64 %substr_len, 0");
        self.emit_raw("  br i1 %substr_empty, label %found_at_end, label %loop_setup");
        self.emit_raw("");
        self.emit_raw("found_at_end:");
        self.emit_raw("  %end_i32 = trunc i64 %str_len to i32");
        self.emit_raw("  ret i32 %end_i32");
        self.emit_raw("");
        self.emit_raw("loop_setup:");
        self.emit_raw("  ; 如果子串比原串长，返回-1");
        self.emit_raw("  %substr_too_long = icmp sgt i64 %substr_len, %str_len");
        self.emit_raw("  br i1 %substr_too_long, label %not_found, label %loop_start");
        self.emit_raw("");
        self.emit_raw("loop_start:");
        self.emit_raw("  %max_pos = sub i64 %str_len, %substr_len");
        self.emit_raw("  br label %loop_check");
        self.emit_raw("");
        self.emit_raw("loop_check:");
        self.emit_raw("  %i = phi i64 [%max_pos, %loop_start], [%i_prev, %loop_continue]");
        self.emit_raw("  %i_ge_0 = icmp sge i64 %i, 0");
        self.emit_raw("  br i1 %i_ge_0, label %loop_body, label %not_found");
        self.emit_raw("");
        self.emit_raw("loop_body:");
        self.emit_raw("  %curr_ptr = getelementptr i8, i8* %str, i64 %i");
        self.emit_raw("  %cmp_result = call i32 @__cay_strncmp(i8* %curr_ptr, i8* %substr, i64 %substr_len)");
        self.emit_raw("  %found = icmp eq i32 %cmp_result, 0");
        self.emit_raw("  br i1 %found, label %found_match, label %loop_continue");
        self.emit_raw("");
        self.emit_raw("found_match:");
        self.emit_raw("  %result_i32 = trunc i64 %i to i32");
        self.emit_raw("  ret i32 %result_i32");
        self.emit_raw("");
        self.emit_raw("loop_continue:");
        self.emit_raw("  %i_prev = sub i64 %i, 1");
        self.emit_raw("  br label %loop_check");
        self.emit_raw("}");
        self.emit_raw("");
        // __cay_strncmp 已在 string_indexof.rs 中定义，这里直接调用
        self.emit_raw("");
    }
}
