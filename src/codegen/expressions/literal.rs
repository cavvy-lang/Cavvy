//! 字面量表达式代码生成
//!
//! 处理整数、浮点数、布尔、字符串、字符和 null 字面量。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成字面量代码
    ///
    /// # Arguments
    /// * `lit` - 字面量值
    ///
    /// # Returns
    /// 格式为 "type value" 的字符串
    pub fn generate_literal(&mut self, lit: &LiteralValue) -> cayResult<String> {
        match lit {
            LiteralValue::Int32(val) => Ok(format!("i32 {}", val)),
            LiteralValue::Int64(val) => Ok(format!("i64 {}", val)),
            LiteralValue::Float32(val) => {
                // 对于float字面量，生成double常量
                // 类型转换逻辑会将其转换为float
                // 确保浮点数常量有小数点
                let formatted = if val.fract() == 0.0 {
                    format!("double {}.0", val)
                } else {
                    format!("double {}", val)
                };
                Ok(formatted)
            }
            LiteralValue::Float64(val) => {
                // 对于double，使用十进制表示
                // 确保浮点数常量有小数点
                let formatted = if val.fract() == 0.0 {
                    format!("double {}.0", val)
                } else {
                    format!("double {}", val)
                };
                Ok(formatted)
            }
            LiteralValue::Bool(val) => Ok(format!("i1 {}", if *val { 1 } else { 0 })),
            LiteralValue::String(s) => {
                let global_name = self.get_or_create_string_constant(s);
                let temp = self.new_temp();
                let len = s.len() + 1;
                self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                    temp, len, len, global_name));
                Ok(format!("i8* {}", temp))
            }
            LiteralValue::Char(c) => Ok(format!("i8 {}", *c as u8)),
            LiteralValue::Null => Ok("i64 0".to_string()),
        }
    }
}
