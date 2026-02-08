//! 运行时支持函数生成
use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 发射IR头部（外部声明和运行时函数）
    pub fn emit_header(&mut self) {
        self.emit_raw("; EOL (Ethernos Object Language) Generated LLVM IR");
        self.emit_raw("target triple = \"x86_64-w64-mingw32\"");
        self.emit_raw("");

        // 声明外部函数 (printf 和标准C库函数)
        self.emit_raw("declare i32 @printf(i8*, ...)");
        self.emit_raw("declare i32 @scanf(i8*, ...)");
        self.emit_raw("declare void @SetConsoleOutputCP(i32)");
        self.emit_raw("declare i64 @strlen(i8*)");
        self.emit_raw("declare i8* @calloc(i64, i64)");
        self.emit_raw("declare void @llvm.memcpy.p0i8.p0i8.i64(i8* noalias nocapture writeonly, i8* noalias nocapture readonly, i64, i1 immarg)");
        self.emit_raw("declare i32 @snprintf(i8*, i64, i8*, ...)");
        self.emit_raw("@.str.float_fmt = private unnamed_addr constant [3 x i8] c\"%f\\00\", align 1");
        self.emit_raw("");

        // 空字符串常量（用于 null 安全）
        self.emit_raw("@.eol_empty_str = private unnamed_addr constant [1 x i8] c\"\\00\", align 1");
        self.emit_raw("");

        // 生成运行时函数
        self.emit_string_concat_runtime();
        self.emit_float_to_string_runtime();
        self.emit_string_length_runtime();
        self.emit_string_substring_runtime();
        self.emit_string_indexof_runtime();
        self.emit_string_charat_runtime();
        self.emit_string_replace_runtime();
    }

    /// 生成字符串拼接运行时函数
    fn emit_string_concat_runtime(&mut self) {
        self.emit_raw("define i8* @__eol_string_concat(i8* %a, i8* %b) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 空指针安全检查：null → 空字符串 \"\"");
        self.emit_raw("  %a_is_null = icmp eq i8* %a, null");
        self.emit_raw("  %a_ptr = select i1 %a_is_null,");
        self.emit_raw("    i8* getelementptr ([1 x i8], [1 x i8]* @.eol_empty_str, i64 0, i64 0),");
        self.emit_raw("    i8* %a");
        self.emit_raw("  ");
        self.emit_raw("  %b_is_null = icmp eq i8* %b, null");
        self.emit_raw("  %b_ptr = select i1 %b_is_null,");
        self.emit_raw("    i8* getelementptr ([1 x i8], [1 x i8]* @.eol_empty_str, i64 0, i64 0),");
        self.emit_raw("    i8* %b");
        self.emit_raw("  ");
        self.emit_raw("  ; 计算长度");
        self.emit_raw("  %len_a = call i64 @strlen(i8* %a_ptr)");
        self.emit_raw("  %len_b = call i64 @strlen(i8* %b_ptr)");
        self.emit_raw("  %total_len = add i64 %len_a, %len_b");
        self.emit_raw("  %buf_size = add i64 %total_len, 1  ; +1 for '\\0'");
        self.emit_raw("  ");
        self.emit_raw("  ; 内存分配（使用 calloc 自动零初始化）");
        self.emit_raw("  %result = call i8* @calloc(i64 1, i64 %buf_size)");
        self.emit_raw("  ");
        self.emit_raw("  ; malloc 失败保护：返回空字符串而非崩溃");
        self.emit_raw("  %is_null = icmp eq i8* %result, null");
        self.emit_raw("  br i1 %is_null, label %fail, label %copy");
        self.emit_raw("  ");
        self.emit_raw("fail:");
        self.emit_raw("  ret i8* getelementptr ([1 x i8], [1 x i8]* @.eol_empty_str, i64 0, i64 0)");
        self.emit_raw("  ");
        self.emit_raw("copy:");
        self.emit_raw("  ; 快速内存复制（LLVM 会优化为 SSE/AVX 或 rep movsb）");
        self.emit_raw("  call void @llvm.memcpy.p0i8.p0i8.i64(");
        self.emit_raw("    i8* %result,");
        self.emit_raw("    i8* %a_ptr,");
        self.emit_raw("    i64 %len_a,");
        self.emit_raw("    i1 false");
        self.emit_raw("  )");
        self.emit_raw("  ");
        self.emit_raw("  ; 复制 b 到 offset = len_a");
        self.emit_raw("  %dest_b = getelementptr i8, i8* %result, i64 %len_a");
        self.emit_raw("  call void @llvm.memcpy.p0i8.p0i8.i64(");
        self.emit_raw("    i8* %dest_b,");
        self.emit_raw("    i8* %b_ptr,");
        self.emit_raw("    i64 %len_b,");
        self.emit_raw("    i1 false");
        self.emit_raw("  )");
        self.emit_raw("  ");
        self.emit_raw("  ; 写入 null terminator");
        self.emit_raw("  %end_ptr = getelementptr i8, i8* %result, i64 %total_len");
        self.emit_raw("  store i8 0, i8* %end_ptr");
        self.emit_raw("  ");
        self.emit_raw("  ret i8* %result");
        self.emit_raw("}");
        self.emit_raw("");
    }

    /// 生成浮点数转字符串运行时函数
    fn emit_float_to_string_runtime(&mut self) {
        // 使用一个包装函数来确保正确的调用约定
        // 注意：使用 calloc 分配堆内存（自动零初始化），而不是 alloca 分配栈内存
        self.emit_raw("define i8* @__eol_float_to_string(double %value) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 分配堆内存缓冲区（64字节，8字节对齐，使用 calloc 自动零初始化）");
        self.emit_raw("  %buf = call i8* @calloc(i64 1, i64 64)");
        self.emit_raw("  %fmt_ptr = getelementptr [3 x i8], [3 x i8]* @.str.float_fmt, i64 0, i64 0");
        self.emit_raw("  ; 调用 snprintf（指定缓冲区大小）");
        self.emit_raw("  call i32 (i8*, i64, i8*, ...) @snprintf(i8* %buf, i64 64, i8* %fmt_ptr, double %value)");
        self.emit_raw("  ret i8* %buf");
        self.emit_raw("}");
        self.emit_raw("");
    }

    /// 生成字符串长度运行时函数
    fn emit_string_length_runtime(&mut self) {
        self.emit_raw("define i32 @__eol_string_length(i8* %str) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 空指针安全检查");
        self.emit_raw("  %is_null = icmp eq i8* %str, null");
        self.emit_raw("  br i1 %is_null, label %null_case, label %normal_case");
        self.emit_raw("");
        self.emit_raw("null_case:");
        self.emit_raw("  ret i32 0");
        self.emit_raw("");
        self.emit_raw("normal_case:");
        self.emit_raw("  %len = call i64 @strlen(i8* %str)");
        self.emit_raw("  %len_i32 = trunc i64 %len to i32");
        self.emit_raw("  ret i32 %len_i32");
        self.emit_raw("}");
        self.emit_raw("");
    }

    /// 生成字符串子串运行时函数
    fn emit_string_substring_runtime(&mut self) {
        // substring(beginIndex, endIndex) - 两个参数版本
        self.emit_raw("define i8* @__eol_string_substring(i8* %str, i32 %begin, i32 %end) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 空指针安全检查");
        self.emit_raw("  %is_null = icmp eq i8* %str, null");
        self.emit_raw("  br i1 %is_null, label %null_case, label %check_bounds");
        self.emit_raw("");
        self.emit_raw("null_case:");
        self.emit_raw("  ret i8* getelementptr ([1 x i8], [1 x i8]* @.eol_empty_str, i64 0, i64 0)");
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
        self.emit_raw("  %begin_clamped = select i1 %begin_gt_end, i32 %end_final, i32 %begin_final");
        self.emit_raw("  ; 计算子串长度");
        self.emit_raw("  %sub_len = sub i32 %end_final, %begin_clamped");
        self.emit_raw("  %sub_len_i64 = sext i32 %sub_len to i64");
        self.emit_raw("  %buf_size = add i64 %sub_len_i64, 1");
        self.emit_raw("  ; 分配内存");
        self.emit_raw("  %result = call i8* @calloc(i64 1, i64 %buf_size)");
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

    /// 生成字符串查找运行时函数
    fn emit_string_indexof_runtime(&mut self) {
        self.emit_raw("define i32 @__eol_string_indexof(i8* %str, i8* %substr) {");
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
        self.emit_raw("  %cmp_result = call i32 @strncmp(i8* %curr_ptr, i8* %substr, i64 %substr_len)");
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
        self.emit_raw("declare i32 @strncmp(i8*, i8*, i64)");
        self.emit_raw("");
    }

    /// 生成字符串字符获取运行时函数
    fn emit_string_charat_runtime(&mut self) {
        self.emit_raw("define i8 @__eol_string_charat(i8* %str, i32 %index) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 空指针安全检查");
        self.emit_raw("  %is_null = icmp eq i8* %str, null");
        self.emit_raw("  br i1 %is_null, label %out_of_bounds, label %check_bounds");
        self.emit_raw("");
        self.emit_raw("check_bounds:");
        self.emit_raw("  %len = call i64 @strlen(i8* %str)");
        self.emit_raw("  %len_i32 = trunc i64 %len to i32");
        self.emit_raw("  %index_neg = icmp slt i32 %index, 0");
        self.emit_raw("  %index_too_large = icmp sge i32 %index, %len_i32");
        self.emit_raw("  %out_of_range = or i1 %index_neg, %index_too_large");
        self.emit_raw("  br i1 %out_of_range, label %out_of_bounds, label %get_char");
        self.emit_raw("");
        self.emit_raw("out_of_bounds:");
        self.emit_raw("  ret i8 0");
        self.emit_raw("");
        self.emit_raw("get_char:");
        self.emit_raw("  %idx_i64 = sext i32 %index to i64");
        self.emit_raw("  %char_ptr = getelementptr i8, i8* %str, i64 %idx_i64");
        self.emit_raw("  %char_val = load i8, i8* %char_ptr");
        self.emit_raw("  ret i8 %char_val");
        self.emit_raw("}");
        self.emit_raw("");
    }

    /// 生成字符串替换运行时函数
    fn emit_string_replace_runtime(&mut self) {
        self.emit_raw("define i8* @__eol_string_replace(i8* %str, i8* %old, i8* %new) {");
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
        self.emit_raw("  %cmp = call i32 @strncmp(i8* %search_ptr, i8* %old, i64 %old_len)");
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
        self.emit_raw("  %count_next = phi i32 [%count_inc, %count_found], [%count_same, %count_not_found]");
        self.emit_raw("  %pos_next = phi i64 [%pos_inc, %count_found], [%pos_same, %count_not_found]");
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
        self.emit_raw("  br label %build_loop");
        self.emit_raw("");
        self.emit_raw("build_loop:");
        self.emit_raw("  %src_pos = phi i64 [0, %allocate_result], [%src_pos_next, %build_continue]");
        self.emit_raw("  %dst_pos = phi i64 [0, %allocate_result], [%dst_pos_next, %build_continue]");
        self.emit_raw("  %can_search2 = icmp sle i64 %src_pos, %max_count_pos");
        self.emit_raw("  br i1 %can_search2, label %build_check, label %copy_remainder");
        self.emit_raw("");
        self.emit_raw("build_check:");
        self.emit_raw("  %src_ptr = getelementptr i8, i8* %str, i64 %src_pos");
        self.emit_raw("  %cmp2 = call i32 @strncmp(i8* %src_ptr, i8* %old, i64 %old_len)");
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
