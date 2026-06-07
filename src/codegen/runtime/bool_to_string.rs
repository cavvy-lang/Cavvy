//! 布尔值转字符串运行时函数

use crate::codegen::context::IRGenerator;

impl IRGenerator {
    /// 生成布尔到字符串运行时函数
    pub(super) fn emit_bool_to_string_runtime(&mut self) {
        self.emit_raw("define i8* @__cay_bool_to_string(i1 %value) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 根据布尔值返回 \"true\" 或 \"false\"");
        self.emit_raw("  br i1 %value, label %true_case, label %false_case");
        self.emit_raw("");
        self.emit_raw("true_case:");
        self.emit_raw("  ret i8* getelementptr ([5 x i8], [5 x i8]* @.str.true_str, i64 0, i64 0)");
        self.emit_raw("");
        self.emit_raw("false_case:");
        self.emit_raw(
            "  ret i8* getelementptr ([6 x i8], [6 x i8]* @.str.false_str, i64 0, i64 0)",
        );
        self.emit_raw("}");
        self.emit_raw("");
    }
}
