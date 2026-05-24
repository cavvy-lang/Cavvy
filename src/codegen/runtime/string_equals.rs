//! String equals 运行时函数
//!
//! 实现 __cay_string_equals 函数，用于比较两个字符串是否相等。

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成 string_equals 运行时函数
    /// 比较两个字符串是否相等，返回 i1 (boolean)
    pub(super) fn emit_string_equals_runtime(&mut self) {
        self.emit_raw("; String.equals() 运行时函数");
        self.emit_raw("define i1 @__cay_string_equals(i8* %str1, i8* %str2) {");
        self.emit_raw("entry:");
        
        // 处理 null 情况
        self.emit_raw("  %str1_is_null = icmp eq i8* %str1, null");
        self.emit_raw("  %str2_is_null = icmp eq i8* %str2, null");
        self.emit_raw("  br i1 %str1_is_null, label %str1_null_case, label %str1_not_null");
        
        // str1 是 null 的情况
        self.emit_raw("str1_null_case:");
        self.emit_raw("  ; 如果 str1 是 null，只有当 str2 也是 null 时才相等");
        self.emit_raw("  %both_null = icmp eq i1 %str1_is_null, %str2_is_null");
        self.emit_raw("  ret i1 %both_null");
        
        // str1 不是 null 的情况
        self.emit_raw("str1_not_null:");
        self.emit_raw("  br i1 %str2_is_null, label %str2_null_case, label %both_not_null");
        
        // str2 是 null 但 str1 不是 null
        self.emit_raw("str2_null_case:");
        self.emit_raw("  ret i1 0");
        
        // 两者都不是 null，使用 strcmp 比较
        self.emit_raw("both_not_null:");
        self.emit_raw("  %cmp_result = call i32 @strcmp(i8* %str1, i8* %str2)");
        self.emit_raw("  %is_equal = icmp eq i32 %cmp_result, 0");
        self.emit_raw("  ret i1 %is_equal");
        
        self.emit_raw("}");
        self.emit_raw("");

        // equalsIgnoreCase 运行时函数
        self.emit_raw("; String.equalsIgnoreCase() 运行时函数");
        self.emit_raw("define i1 @__cay_string_equals_ignorecase(i8* %str1, i8* %str2) {");
        self.emit_raw("entry:");
        self.emit_raw("  %str1_is_null = icmp eq i8* %str1, null");
        self.emit_raw("  %str2_is_null = icmp eq i8* %str2, null");
        self.emit_raw("  br i1 %str1_is_null, label %str1_null_case, label %str1_not_null");
        self.emit_raw("");
        self.emit_raw("str1_null_case:");
        self.emit_raw("  %both_null = icmp eq i1 %str1_is_null, %str2_is_null");
        self.emit_raw("  ret i1 %both_null");
        self.emit_raw("");
        self.emit_raw("str1_not_null:");
        self.emit_raw("  br i1 %str2_is_null, label %str2_null_case, label %both_not_null");
        self.emit_raw("");
        self.emit_raw("str2_null_case:");
        self.emit_raw("  ret i1 0");
        self.emit_raw("");
        self.emit_raw("both_not_null:");
        self.emit_raw("  br label %loop_start");
        self.emit_raw("");
        self.emit_raw("loop_start:");
        self.emit_raw("  %i = phi i64 [0, %both_not_null], [%i_next, %loop_continue]");
        self.emit_raw("  %c1_ptr = getelementptr i8, i8* %str1, i64 %i");
        self.emit_raw("  %c2_ptr = getelementptr i8, i8* %str2, i64 %i");
        self.emit_raw("  %c1_raw = load i8, i8* %c1_ptr");
        self.emit_raw("  %c2_raw = load i8, i8* %c2_ptr");
        self.emit_raw("  ; 如果两个字符都是 0，说明到达字符串末尾");
        self.emit_raw("  %c1_is_null = icmp eq i8 %c1_raw, 0");
        self.emit_raw("  %c2_is_null = icmp eq i8 %c2_raw, 0");
        self.emit_raw("  %both_null_term = and i1 %c1_is_null, %c2_is_null");
        self.emit_raw("  br i1 %both_null_term, label %match, label %check_single_null");
        self.emit_raw("");
        self.emit_raw("check_single_null:");
        self.emit_raw("  %either_null = or i1 %c1_is_null, %c2_is_null");
        self.emit_raw("  br i1 %either_null, label %not_match, label %compare");
        self.emit_raw("");
        self.emit_raw("compare:");
        self.emit_raw("  ; 转换为小写进行比较");
        self.emit_raw("  %c1_lower = call i8 @__cay_to_lower(i8 %c1_raw)");
        self.emit_raw("  %c2_lower = call i8 @__cay_to_lower(i8 %c2_raw)");
        self.emit_raw("  %chars_equal = icmp eq i8 %c1_lower, %c2_lower");
        self.emit_raw("  br i1 %chars_equal, label %loop_continue, label %not_match");
        self.emit_raw("");
        self.emit_raw("loop_continue:");
        self.emit_raw("  %i_next = add i64 %i, 1");
        self.emit_raw("  br label %loop_start");
        self.emit_raw("");
        self.emit_raw("match:");
        self.emit_raw("  ret i1 1");
        self.emit_raw("");
        self.emit_raw("not_match:");
        self.emit_raw("  ret i1 0");
        self.emit_raw("}");
        self.emit_raw("");
        
        // 辅助函数：将字符转换为小写
        self.emit_raw("; 辅助函数：字符转小写");
        self.emit_raw("define internal i8 @__cay_to_lower(i8 %c) {");
        self.emit_raw("entry:");
        self.emit_raw("  %is_upper = icmp uge i8 %c, 65");
        self.emit_raw("  %is_upper2 = icmp ule i8 %c, 90");
        self.emit_raw("  %is_upper_both = and i1 %is_upper, %is_upper2");
        self.emit_raw("  br i1 %is_upper_both, label %to_lower, label %done");
        self.emit_raw("");
        self.emit_raw("to_lower:");
        self.emit_raw("  %lower = add i8 %c, 32");
        self.emit_raw("  ret i8 %lower");
        self.emit_raw("");
        self.emit_raw("done:");
        self.emit_raw("  ret i8 %c");
        self.emit_raw("}");
        self.emit_raw("");
        
        // trim 运行时函数：去除首尾空白字符
        self.emit_raw("; String.trim() 运行时函数");
        self.emit_raw("define i8* @__cay_string_trim(i8* %str) {");
        self.emit_raw("entry:");
        self.emit_raw("  %str_null = icmp eq i8* %str, null");
        self.emit_raw("  br i1 %str_null, label %return_null, label %find_start");
        self.emit_raw("");
        self.emit_raw("return_null:");
        self.emit_raw("  ret i8* null");
        self.emit_raw("");
        self.emit_raw("find_start:");
        self.emit_raw("  %len = call i64 @strlen(i8* %str)");
        self.emit_raw("  br label %start_loop");
        self.emit_raw("");
        self.emit_raw("start_loop:");
        self.emit_raw("  %start_i = phi i64 [0, %find_start], [%start_next, %start_next_br]");
        self.emit_raw("  %start_done = icmp eq i64 %start_i, %len");
        self.emit_raw("  br i1 %start_done, label %all_whitespace, label %start_check");
        self.emit_raw("");
        self.emit_raw("start_check:");
        self.emit_raw("  %start_ptr = getelementptr i8, i8* %str, i64 %start_i");
        self.emit_raw("  %start_c = load i8, i8* %start_ptr");
        self.emit_raw("  %start_is_space = call i1 @__cay_is_whitespace(i8 %start_c)");
        self.emit_raw("  br i1 %start_is_space, label %start_next_br, label %find_end");
        self.emit_raw("");
        self.emit_raw("start_next_br:");
        self.emit_raw("  %start_next = add i64 %start_i, 1");
        self.emit_raw("  br label %start_loop");
        self.emit_raw("");
        self.emit_raw("all_whitespace:");
        self.emit_raw("  %empty = call i8* @calloc(i64 1, i64 1)");
        self.emit_raw("  ret i8* %empty");
        self.emit_raw("");
        self.emit_raw("find_end:");
        self.emit_raw("  %end_start = sub i64 %len, 1");
        self.emit_raw("  br label %end_loop");
        self.emit_raw("");
        self.emit_raw("end_loop:");
        self.emit_raw("  %end_i = phi i64 [%end_start, %find_end], [%end_prev, %end_prev_br]");
        self.emit_raw("  %end_ptr = getelementptr i8, i8* %str, i64 %end_i");
        self.emit_raw("  %end_c = load i8, i8* %end_ptr");
        self.emit_raw("  %end_is_space = call i1 @__cay_is_whitespace(i8 %end_c)");
        self.emit_raw("  br i1 %end_is_space, label %end_prev_br, label %copy");
        self.emit_raw("");
        self.emit_raw("end_prev_br:");
        self.emit_raw("  %end_prev = sub i64 %end_i, 1");
        self.emit_raw("  br label %end_loop");
        self.emit_raw("");
        self.emit_raw("copy:");
        self.emit_raw("  %copy_len = sub i64 %end_i, %start_i");
        self.emit_raw("  %copy_len_plus1 = add i64 %copy_len, 1");
        self.emit_raw("  %result = call i8* @calloc(i64 1, i64 %copy_len_plus1)");
        self.emit_raw("  %src_ptr = getelementptr i8, i8* %str, i64 %start_i");
        self.emit_raw("  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %result, i8* %src_ptr, i64 %copy_len, i1 0)");
        self.emit_raw("  %null_pos = getelementptr i8, i8* %result, i64 %copy_len");
        self.emit_raw("  store i8 0, i8* %null_pos");
        self.emit_raw("  ret i8* %result");
        self.emit_raw("}");
        self.emit_raw("");
        
        // 辅助函数：判断是否为空白字符
        self.emit_raw("define internal i1 @__cay_is_whitespace(i8 %c) {");
        self.emit_raw("entry:");
        self.emit_raw("  %is_space = icmp eq i8 %c, 32");
        self.emit_raw("  %is_tab = icmp eq i8 %c, 9");
        self.emit_raw("  %is_newline = icmp eq i8 %c, 10");
        self.emit_raw("  %is_cr = icmp eq i8 %c, 13");
        self.emit_raw("  %or1 = or i1 %is_space, %is_tab");
        self.emit_raw("  %or2 = or i1 %or1, %is_newline");
        self.emit_raw("  %or3 = or i1 %or2, %is_cr");
        self.emit_raw("  ret i1 %or3");
        self.emit_raw("}");
        self.emit_raw("");
    }
}
