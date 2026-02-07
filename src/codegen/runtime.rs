//! 运行时支持函数生成
use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 发射IR头部（外部声明和运行时函数）
    pub fn emit_header(&mut self) {
        self.emit_raw("; EOL (Ethernos Object Language) Generated LLVM IR");
        self.emit_raw("target triple = \"x86_64-pc-windows-msvc\"");
        self.emit_raw("");
        
        // 声明外部函数 (printf 和标准C库函数)
        self.emit_raw("declare i32 @printf(i8*, ...)");
        self.emit_raw("declare i32 @scanf(i8*, ...)");
        self.emit_raw("declare void @SetConsoleOutputCP(i32)");
        self.emit_raw("declare i64 @strlen(i8*)");
        self.emit_raw("declare i8* @malloc(i64)");
        self.emit_raw("declare void @llvm.memcpy.p0i8.p0i8.i64(i8* noalias nocapture writeonly, i8* noalias nocapture readonly, i64, i1 immarg)");
        self.emit_raw("");
        
        // 空字符串常量（用于 null 安全）
        self.emit_raw("@.eol_empty_str = private unnamed_addr constant [1 x i8] c\"\\00\", align 1");
        self.emit_raw("");
        
        // 生成字符串拼接运行时函数
        self.emit_string_concat_runtime();
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
        self.emit_raw("  ; 内存分配");
        self.emit_raw("  %result = call i8* @malloc(i64 %buf_size)");
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
}
