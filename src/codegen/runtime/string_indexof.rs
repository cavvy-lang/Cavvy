//! 字符串查找运行时函数

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 内部 strncmp 辅助函数（纯 LLVM IR 实现，不依赖外部 C 库）
    fn emit_internal_strncmp(&mut self) {
        self.emit_raw("; 内部 strncmp 辅助函数（纯 LLVM IR 实现）");
        self.emit_raw("define internal i32 @__cay_strncmp(i8* %s1, i8* %s2, i64 %n) {");
        self.emit_raw("entry:");
        self.emit_raw("  br label %loop");
        self.emit_raw("");
        self.emit_raw("loop:");
        self.emit_raw("  %i = phi i64 [0, %entry], [%i_next, %continue]");
        self.emit_raw("  %done = icmp eq i64 %i, %n");
        self.emit_raw("  br i1 %done, label %equal, label %check");
        self.emit_raw("");
        self.emit_raw("check:");
        self.emit_raw("  %p1 = getelementptr i8, i8* %s1, i64 %i");
        self.emit_raw("  %p2 = getelementptr i8, i8* %s2, i64 %i");
        self.emit_raw("  %c1 = load i8, i8* %p1");
        self.emit_raw("  %c2 = load i8, i8* %p2");
        self.emit_raw("  %c1_null = icmp eq i8 %c1, 0");
        self.emit_raw("  %c2_null = icmp eq i8 %c2, 0");
        self.emit_raw("  %either_null = or i1 %c1_null, %c2_null");
        self.emit_raw("  br i1 %either_null, label %terminated, label %compare");
        self.emit_raw("");
        self.emit_raw("compare:");
        self.emit_raw("  %lt = icmp ult i8 %c1, %c2");
        self.emit_raw("  %gt = icmp ugt i8 %c1, %c2");
        self.emit_raw("  br i1 %lt, label %less, label %check_gt");
        self.emit_raw("");
        self.emit_raw("check_gt:");
        self.emit_raw("  br i1 %gt, label %greater, label %continue");
        self.emit_raw("");
        self.emit_raw("continue:");
        self.emit_raw("  %i_next = add i64 %i, 1");
        self.emit_raw("  br label %loop");
        self.emit_raw("");
        self.emit_raw("terminated:");
        self.emit_raw("  %both_null = and i1 %c1_null, %c2_null");
        self.emit_raw("  br i1 %both_null, label %equal, label %check_which");
        self.emit_raw("");
        self.emit_raw("check_which:");
        self.emit_raw("  br i1 %c1_null, label %less, label %greater");
        self.emit_raw("");
        self.emit_raw("equal:");
        self.emit_raw("  ret i32 0");
        self.emit_raw("");
        self.emit_raw("less:");
        self.emit_raw("  ret i32 -1");
        self.emit_raw("");
        self.emit_raw("greater:");
        self.emit_raw("  ret i32 1");
        self.emit_raw("}");
        self.emit_raw("");
    }

    /// 生成字符串查找运行时函数
    pub(super) fn emit_string_indexof_runtime(&mut self) {
        self.emit_internal_strncmp();
        self.emit_raw("define i32 @__cay_string_indexof(i8* %str, i8* %substr) {");
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
        self.emit_raw("  ; 如果子串为空，返回0");
        self.emit_raw("  %substr_empty = icmp eq i64 %substr_len, 0");
        self.emit_raw("  br i1 %substr_empty, label %found_at_0, label %loop_setup");
        self.emit_raw("");
        self.emit_raw("found_at_0:");
        self.emit_raw("  ret i32 0");
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
        self.emit_raw("  %i = phi i64 [0, %loop_start], [%i_next, %loop_continue]");
        self.emit_raw("  %i_le_max = icmp sle i64 %i, %max_pos");
        self.emit_raw("  br i1 %i_le_max, label %loop_body, label %not_found");
        self.emit_raw("");
        self.emit_raw("loop_body:");
        self.emit_raw("  %curr_ptr = getelementptr i8, i8* %str, i64 %i");
        self.emit_raw(
            "  %cmp_result = call i32 @__cay_strncmp(i8* %curr_ptr, i8* %substr, i64 %substr_len)",
        );
        self.emit_raw("  %found = icmp eq i32 %cmp_result, 0");
        self.emit_raw("  br i1 %found, label %found_match, label %loop_continue");
        self.emit_raw("");
        self.emit_raw("found_match:");
        self.emit_raw("  %result_i32 = trunc i64 %i to i32");
        self.emit_raw("  ret i32 %result_i32");
        self.emit_raw("");
        self.emit_raw("loop_continue:");
        self.emit_raw("  %i_next = add i64 %i, 1");
        self.emit_raw("  br label %loop_check");
        self.emit_raw("}");
        self.emit_raw("");
        // 带起始位置的 indexOf(str, start)
        self.emit_raw("define i32 @__cay_string_indexof_from(i8* %str, i8* %substr, i32 %start) {");
        self.emit_raw("entry:");
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
        self.emit_raw("  %substr_empty = icmp eq i64 %substr_len, 0");
        self.emit_raw("  br i1 %substr_empty, label %found_at_0, label %check_start");
        self.emit_raw("");
        self.emit_raw("found_at_0:");
        self.emit_raw("  ret i32 0");
        self.emit_raw("");
        self.emit_raw("check_start:");
        self.emit_raw("  %start_ext = sext i32 %start to i64");
        self.emit_raw("  %start_neg = icmp slt i64 %start_ext, 0");
        self.emit_raw("  br i1 %start_neg, label %not_found, label %check_bounds");
        self.emit_raw("");
        self.emit_raw("check_bounds:");
        self.emit_raw("  %start_oob = icmp sge i64 %start_ext, %str_len");
        self.emit_raw("  br i1 %start_oob, label %not_found, label %loop_setup");
        self.emit_raw("");
        self.emit_raw("loop_setup:");
        self.emit_raw("  %substr_too_long = icmp sgt i64 %substr_len, %str_len");
        self.emit_raw("  br i1 %substr_too_long, label %not_found, label %loop_start");
        self.emit_raw("");
        self.emit_raw("loop_start:");
        self.emit_raw("  %max_pos = sub i64 %str_len, %substr_len");
        self.emit_raw("  br label %loop_check");
        self.emit_raw("");
        self.emit_raw("loop_check:");
        self.emit_raw("  %i = phi i64 [%start_ext, %loop_start], [%i_next, %loop_continue]");
        self.emit_raw("  %i_le_max = icmp sle i64 %i, %max_pos");
        self.emit_raw("  br i1 %i_le_max, label %loop_body, label %not_found");
        self.emit_raw("");
        self.emit_raw("loop_body:");
        self.emit_raw("  %curr_ptr = getelementptr i8, i8* %str, i64 %i");
        self.emit_raw(
            "  %cmp_result = call i32 @__cay_strncmp(i8* %curr_ptr, i8* %substr, i64 %substr_len)",
        );
        self.emit_raw("  %found = icmp eq i32 %cmp_result, 0");
        self.emit_raw("  br i1 %found, label %found_match, label %loop_continue");
        self.emit_raw("");
        self.emit_raw("found_match:");
        self.emit_raw("  %result_i32 = trunc i64 %i to i32");
        self.emit_raw("  ret i32 %result_i32");
        self.emit_raw("");
        self.emit_raw("loop_continue:");
        self.emit_raw("  %i_next = add i64 %i, 1");
        self.emit_raw("  br label %loop_check");
        self.emit_raw("}");
        self.emit_raw("");
    }
}
