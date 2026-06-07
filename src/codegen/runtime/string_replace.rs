//! 字符串替换运行时函数

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成字符串替换运行时函数
    pub(super) fn emit_string_replace_runtime(&mut self) {
        self.emit_raw("define i8* @__cay_string_replace(i8* %str, i8* %old, i8* %new) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 空指针安全检查");
        self.emit_raw("  %str_null = icmp eq i8* %str, null");
        self.emit_raw("  %old_null = icmp eq i8* %old, null");
        self.emit_raw("  %new_null = icmp eq i8* %new, null");
        self.emit_raw("  %any_null = or i1 %str_null, %old_null");
        self.emit_raw("  %any_null2 = or i1 %any_null, %new_null");
        self.emit_raw("  br i1 %any_null2, label %return_copy, label %check_empty");
        self.emit_raw("");
        self.emit_raw("check_empty:");
        self.emit_raw("  ; 如果old为空，返回原串副本");
        self.emit_raw("  %old_len = call i64 @strlen(i8* %old)");
        self.emit_raw("  %old_empty = icmp eq i64 %old_len, 0");
        self.emit_raw("  br i1 %old_empty, label %return_copy, label %count_occurrences");
        self.emit_raw("");
        self.emit_raw("return_copy:");
        self.emit_raw("  ; 返回原串的副本");
        self.emit_raw("  %str_len_copy = call i64 @strlen(i8* %str)");
        self.emit_raw("  %copy_size = add i64 %str_len_copy, 1");
        self.emit_raw("  %copy = call i8* @calloc(i64 1, i64 %copy_size)");
        self.emit_raw("  ; calloc 失败保护：返回空字符串而非崩溃");
        self.emit_raw("  %copy_null = icmp eq i8* %copy, null");
        self.emit_raw("  br i1 %copy_null, label %copy_fail, label %do_copy");
        self.emit_raw("");
        self.emit_raw("copy_fail:");
        self.emit_raw(
            "  ret i8* getelementptr ([1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0)",
        );
        self.emit_raw("");
        self.emit_raw("do_copy:");
        self.emit_raw("  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %copy, i8* %str, i64 %str_len_copy, i1 false)");
        self.emit_raw("  %copy_end = getelementptr i8, i8* %copy, i64 %str_len_copy");
        self.emit_raw("  store i8 0, i8* %copy_end");
        self.emit_raw("  ret i8* %copy");
        self.emit_raw("");
        self.emit_raw("count_occurrences:");
        self.emit_raw("  ; 统计old出现次数");
        self.emit_raw("  %str_len = call i64 @strlen(i8* %str)");
        self.emit_raw("  %new_len = call i64 @strlen(i8* %new)");
        self.emit_raw("  br label %count_loop");
        self.emit_raw("");
        self.emit_raw("count_loop:");
        self.emit_raw("  %count = phi i32 [0, %count_occurrences], [%count_next, %count_continue]");
        self.emit_raw("  %pos = phi i64 [0, %count_occurrences], [%pos_next, %count_continue]");
        self.emit_raw("  %max_count_pos = sub i64 %str_len, %old_len");
        self.emit_raw("  %can_search = icmp sle i64 %pos, %max_count_pos");
        self.emit_raw("  br i1 %can_search, label %count_check, label %allocate_result");
        self.emit_raw("");
        self.emit_raw("count_check:");
        self.emit_raw("  %search_ptr = getelementptr i8, i8* %str, i64 %pos");
        self.emit_raw("  %cmp = call i32 @__cay_strncmp(i8* %search_ptr, i8* %old, i64 %old_len)");
        self.emit_raw("  %found = icmp eq i32 %cmp, 0");
        self.emit_raw("  br i1 %found, label %count_found, label %count_not_found");
        self.emit_raw("");
        self.emit_raw("count_found:");
        self.emit_raw("  %count_inc = add i32 %count, 1");
        self.emit_raw("  %pos_inc = add i64 %pos, %old_len");
        self.emit_raw("  br label %count_continue");
        self.emit_raw("");
        self.emit_raw("count_not_found:");
        self.emit_raw("  %count_same = add i32 %count, 0");
        self.emit_raw("  %pos_same = add i64 %pos, 1");
        self.emit_raw("  br label %count_continue");
        self.emit_raw("");
        self.emit_raw("count_continue:");
        self.emit_raw(
            "  %count_next = phi i32 [%count_inc, %count_found], [%count_same, %count_not_found]",
        );
        self.emit_raw(
            "  %pos_next = phi i64 [%pos_inc, %count_found], [%pos_same, %count_not_found]",
        );
        self.emit_raw("  br label %count_loop");
        self.emit_raw("");
        self.emit_raw("allocate_result:");
        self.emit_raw("  ; 计算结果字符串大小");
        self.emit_raw("  %count_i64 = sext i32 %count to i64");
        self.emit_raw("  %old_new_diff = sub i64 %new_len, %old_len");
        self.emit_raw("  %size_diff = mul i64 %count_i64, %old_new_diff");
        self.emit_raw("  %result_size = add i64 %str_len, %size_diff");
        self.emit_raw("  %result_buf_size = add i64 %result_size, 1");
        self.emit_raw("  %result = call i8* @calloc(i64 1, i64 %result_buf_size)");
        self.emit_raw("  ; calloc 失败保护：返回空字符串而非崩溃");
        self.emit_raw("  %result_null = icmp eq i8* %result, null");
        self.emit_raw("  br i1 %result_null, label %result_fail, label %build_loop");
        self.emit_raw("");
        self.emit_raw("result_fail:");
        self.emit_raw(
            "  ret i8* getelementptr ([1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0)",
        );
        self.emit_raw("");
        self.emit_raw("build_loop:");
        self.emit_raw(
            "  %src_pos = phi i64 [0, %allocate_result], [%src_pos_next, %build_continue]",
        );
        self.emit_raw(
            "  %dst_pos = phi i64 [0, %allocate_result], [%dst_pos_next, %build_continue]",
        );
        self.emit_raw("  %can_search2 = icmp sle i64 %src_pos, %max_count_pos");
        self.emit_raw("  br i1 %can_search2, label %build_check, label %copy_remainder");
        self.emit_raw("");
        self.emit_raw("build_check:");
        self.emit_raw("  %src_ptr = getelementptr i8, i8* %str, i64 %src_pos");
        self.emit_raw("  %cmp2 = call i32 @__cay_strncmp(i8* %src_ptr, i8* %old, i64 %old_len)");
        self.emit_raw("  %found2 = icmp eq i32 %cmp2, 0");
        self.emit_raw("  br i1 %found2, label %do_replace, label %copy_char");
        self.emit_raw("");
        self.emit_raw("do_replace:");
        self.emit_raw("  %dst_ptr = getelementptr i8, i8* %result, i64 %dst_pos");
        self.emit_raw("  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %dst_ptr, i8* %new, i64 %new_len, i1 false)");
        self.emit_raw("  %src_pos_after = add i64 %src_pos, %old_len");
        self.emit_raw("  %dst_pos_after = add i64 %dst_pos, %new_len");
        self.emit_raw("  br label %build_continue");
        self.emit_raw("");
        self.emit_raw("copy_char:");
        self.emit_raw("  %char_to_copy = load i8, i8* %src_ptr");
        self.emit_raw("  %dst_ptr2 = getelementptr i8, i8* %result, i64 %dst_pos");
        self.emit_raw("  store i8 %char_to_copy, i8* %dst_ptr2");
        self.emit_raw("  %src_pos_after2 = add i64 %src_pos, 1");
        self.emit_raw("  %dst_pos_after2 = add i64 %dst_pos, 1");
        self.emit_raw("  br label %build_continue");
        self.emit_raw("");
        self.emit_raw("build_continue:");
        self.emit_raw("  %src_pos_next = phi i64 [%src_pos_after, %do_replace], [%src_pos_after2, %copy_char]");
        self.emit_raw("  %dst_pos_next = phi i64 [%dst_pos_after, %do_replace], [%dst_pos_after2, %copy_char]");
        self.emit_raw("  br label %build_loop");
        self.emit_raw("");
        self.emit_raw("copy_remainder:");
        self.emit_raw("  ; 复制剩余部分");
        self.emit_raw("  %remaining = sub i64 %str_len, %src_pos");
        self.emit_raw("  %src_remainder = getelementptr i8, i8* %str, i64 %src_pos");
        self.emit_raw("  %dst_remainder = getelementptr i8, i8* %result, i64 %dst_pos");
        self.emit_raw("  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %dst_remainder, i8* %src_remainder, i64 %remaining, i1 false)");
        self.emit_raw("  %final_end = getelementptr i8, i8* %result, i64 %result_size");
        self.emit_raw("  store i8 0, i8* %final_end");
        self.emit_raw("  ret i8* %result");
        self.emit_raw("}");
        self.emit_raw("");
    }
}
