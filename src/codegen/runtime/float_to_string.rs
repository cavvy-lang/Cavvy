//! 浮点数转字符串运行时函数

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成浮点数转字符串运行时函数
    pub(super) fn emit_float_to_string_runtime(&mut self) {
        // float -> String
        // 注意：C 的 printf/snprintf 中 float 参数会被提升为 double
        self.emit_raw("define i8* @__cay_float_to_string(float %value) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 分配堆内存缓冲区（64字节，8字节对齐，使用 calloc 自动零初始化）");
        self.emit_raw("  %buf = call i8* @calloc(i64 1, i64 64)");
        self.emit_raw("  ; calloc 失败保护：返回空字符串而非崩溃");
        self.emit_raw("  %is_null = icmp eq i8* %buf, null");
        self.emit_raw("  br i1 %is_null, label %fail, label %do_fmt");
        self.emit_raw("");
        self.emit_raw("fail:");
        self.emit_raw(
            "  ret i8* getelementptr ([1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0)",
        );
        self.emit_raw("");
        self.emit_raw("do_fmt:");
        self.emit_raw("  ; 将 float 转换为 double（C 的可变参数函数会自动提升 float 为 double）");
        self.emit_raw("  %value_d = fpext float %value to double");
        self.emit_raw(
            "  %fmt_ptr = getelementptr [3 x i8], [3 x i8]* @.str.float_fmt, i64 0, i64 0",
        );
        self.emit_raw("  ; 调用 snprintf（指定缓冲区大小，使用 %f 格式，传递 double）");
        self.emit_raw("  call i32 (i8*, i64, i8*, ...) @snprintf(i8* %buf, i64 64, i8* %fmt_ptr, double %value_d)");
        self.emit_raw("  ret i8* %buf");
        self.emit_raw("}");
        self.emit_raw("");

        // double -> String
        self.emit_raw("define i8* @__cay_double_to_string(double %value) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 分配堆内存缓冲区（64字节，8字节对齐，使用 calloc 自动零初始化）");
        self.emit_raw("  %buf = call i8* @calloc(i64 1, i64 64)");
        self.emit_raw("  ; calloc 失败保护：返回空字符串而非崩溃");
        self.emit_raw("  %is_null = icmp eq i8* %buf, null");
        self.emit_raw("  br i1 %is_null, label %fail, label %do_fmt");
        self.emit_raw("");
        self.emit_raw("fail:");
        self.emit_raw(
            "  ret i8* getelementptr ([1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0)",
        );
        self.emit_raw("");
        self.emit_raw("do_fmt:");
        self.emit_raw(
            "  %fmt_ptr = getelementptr [4 x i8], [4 x i8]* @.str.double_fmt, i64 0, i64 0",
        );
        self.emit_raw("  ; 调用 snprintf（指定缓冲区大小）");
        self.emit_raw("  call i32 (i8*, i64, i8*, ...) @snprintf(i8* %buf, i64 64, i8* %fmt_ptr, double %value)");
        self.emit_raw("  ret i8* %buf");
        self.emit_raw("}");
        self.emit_raw("");
    }
}
