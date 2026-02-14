//! new 表达式代码生成
//!
//! 处理对象创建和数组创建。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成 new 表达式代码
    ///
    /// # Arguments
    /// * `new_expr` - new 表达式
    pub fn generate_new_expression(&mut self, new_expr: &NewExpr) -> cayResult<String> {
        let class_name = &new_expr.class_name;
        let type_id_value = self.get_type_id_value(class_name).unwrap_or(0);

        let size = 16i64;
        let calloc_temp = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @calloc(i64 1, i64 {})", calloc_temp, size));

        let type_id_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", type_id_ptr, calloc_temp));
        self.emit_line(&format!("  store i32 {}, i32* {}", type_id_value, type_id_ptr));

        let cast_temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i8*", cast_temp, calloc_temp));
        Ok(format!("i8* {}", cast_temp))
    }
}
