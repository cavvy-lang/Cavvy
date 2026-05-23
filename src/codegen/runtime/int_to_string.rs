//! 整数转字符串运行时函数

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成整数到字符串运行时函数
    pub(super) fn emit_int_to_string_runtime(&mut self) {
        // i32 -> String
        self.emit_raw("define i8* @__cay_int_to_string(i32 %value) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 分配堆内存缓冲区（32字节足够存储32位整数）");
        self.emit_raw("  %buf = call i8* @calloc(i64 1, i64 32)");
        self.emit_raw("  ; calloc 失败保护：返回空字符串而非崩溃");
        self.emit_raw("  %is_null = icmp eq i8* %buf, null");
        self.emit_raw("  br i1 %is_null, label %fail, label %do_fmt");
        self.emit_raw("");
        self.emit_raw("fail:");
        self.emit_raw("  ret i8* getelementptr ([1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0)");
        self.emit_raw("");
        self.emit_raw("do_fmt:");
        self.emit_raw("  ; 使用 %d 格式打印整数");
        self.emit_raw("  call i32 (i8*, i64, i8*, ...) @snprintf(i8* %buf, i64 32, i8* getelementptr ([3 x i8], [3 x i8]* @.str.int_fmt, i64 0, i64 0), i32 %value)");
        self.emit_raw("  ret i8* %buf");
        self.emit_raw("}");
        self.emit_raw("");

        // i64 -> String (long)
        self.emit_raw("define i8* @__cay_long_to_string(i64 %value) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 分配堆内存缓冲区（32字节足够存储64位整数）");
        self.emit_raw("  %buf = call i8* @calloc(i64 1, i64 32)");
        self.emit_raw("  ; calloc 失败保护：返回空字符串而非崩溃");
        self.emit_raw("  %is_null = icmp eq i8* %buf, null");
        self.emit_raw("  br i1 %is_null, label %fail, label %do_fmt");
        self.emit_raw("");
        self.emit_raw("fail:");
        self.emit_raw("  ret i8* getelementptr ([1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0)");
        self.emit_raw("");
        self.emit_raw("do_fmt:");
        self.emit_raw("  ; 使用 %lld 格式打印长整数");
        self.emit_raw("  call i32 (i8*, i64, i8*, ...) @snprintf(i8* %buf, i64 32, i8* getelementptr ([5 x i8], [5 x i8]* @.str.long_fmt, i64 0, i64 0), i64 %value)");
        self.emit_raw("  ret i8* %buf");
        self.emit_raw("}");
        self.emit_raw("");
    }
}
