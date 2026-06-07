//! 字符串后缀检查运行时函数

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成字符串后缀检查运行时函数
    pub(super) fn emit_string_endswith_runtime(&mut self) {
        self.emit_raw("define i1 @__cay_string_endswith(i8* %str, i8* %suffix) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 空指针安全检查");
        self.emit_raw("  %str_null = icmp eq i8* %str, null");
        self.emit_raw("  %suffix_null = icmp eq i8* %suffix, null");
        self.emit_raw("  %either_null = or i1 %str_null, %suffix_null");
        self.emit_raw("  br i1 %either_null, label %return_false, label %check");
        self.emit_raw("");
        self.emit_raw("return_false:");
        self.emit_raw("  ret i1 false");
        self.emit_raw("");
        self.emit_raw("check:");
        self.emit_raw("  %str_len = call i64 @strlen(i8* %str)");
        self.emit_raw("  %suffix_len = call i64 @strlen(i8* %suffix)");
        self.emit_raw("  ; 如果后缀为空，返回true");
        self.emit_raw("  %suffix_empty = icmp eq i64 %suffix_len, 0");
        self.emit_raw("  br i1 %suffix_empty, label %return_true, label %check_length");
        self.emit_raw("");
        self.emit_raw("return_true:");
        self.emit_raw("  ret i1 true");
        self.emit_raw("");
        self.emit_raw("check_length:");
        self.emit_raw("  ; 如果后缀比原串长，返回false");
        self.emit_raw("  %suffix_too_long = icmp sgt i64 %suffix_len, %str_len");
        self.emit_raw("  br i1 %suffix_too_long, label %return_false, label %compare");
        self.emit_raw("");
        self.emit_raw("compare:");
        self.emit_raw("  ; 计算后缀在原串中的起始位置");
        self.emit_raw("  %start_offset = sub i64 %str_len, %suffix_len");
        self.emit_raw("  %start_ptr = getelementptr i8, i8* %str, i64 %start_offset");
        self.emit_raw(
            "  %cmp_result = call i32 @__cay_strncmp(i8* %start_ptr, i8* %suffix, i64 %suffix_len)",
        );
        self.emit_raw("  %equal = icmp eq i32 %cmp_result, 0");
        self.emit_raw("  ret i1 %equal");
        self.emit_raw("}");
        self.emit_raw("");
        self.emit_raw("");
    }
}
