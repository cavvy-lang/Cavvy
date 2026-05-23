//! 字符转字符串运行时函数

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成字符到字符串运行时函数
    pub(super) fn emit_char_to_string_runtime(&mut self) {
        self.emit_raw("define i8* @__cay_char_to_string(i8 %value) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 分配堆内存缓冲区（2字节：字符 + 终止符）");
        self.emit_raw("  %buf = call i8* @calloc(i64 1, i64 2)");
        self.emit_raw("  ; calloc 失败保护：返回空字符串而非崩溃");
        self.emit_raw("  %is_null = icmp eq i8* %buf, null");
        self.emit_raw("  br i1 %is_null, label %fail, label %do_store");
        self.emit_raw("");
        self.emit_raw("fail:");
        self.emit_raw("  ret i8* getelementptr ([1 x i8], [1 x i8]* @.cay_empty_str, i64 0, i64 0)");
        self.emit_raw("");
        self.emit_raw("do_store:");
        self.emit_raw("  ; 存储字符");
        self.emit_raw("  store i8 %value, i8* %buf");
        self.emit_raw("  ; 存储终止符");
        self.emit_raw("  %end_ptr = getelementptr i8, i8* %buf, i64 1");
        self.emit_raw("  store i8 0, i8* %end_ptr");
        self.emit_raw("  ret i8* %buf");
        self.emit_raw("}");
        self.emit_raw("");
    }
}
