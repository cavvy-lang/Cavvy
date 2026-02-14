//! 类型转换表达式代码生成
//!
//! 处理整数、浮点数、指针之间的类型转换，以及到字符串的转换。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error};

impl IRGenerator {
    /// 生成类型转换表达式代码
    ///
    /// # Arguments
    /// * `cast` - 类型转换表达式
    pub fn generate_cast_expression(&mut self, cast: &CastExpr) -> cayResult<String> {
        let expr_value = self.generate_expression(&cast.expr)?;
        let (from_type, val) = self.parse_typed_value(&expr_value);
        let to_type = self.type_to_llvm(&cast.target_type);
        
        let temp = self.new_temp();
        
        // 相同类型无需转换
        if from_type == to_type {
            return Ok(format!("{} {}", to_type, val));
        }
        
        // 指针类型转换 (bitcast)
        if from_type.ends_with("*") && to_type.ends_with("*") {
            self.emit_line(&format!("  {} = bitcast {} {} to {}",
                temp, from_type, val, to_type));
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 整数到整数
        if from_type.starts_with("i") && to_type.starts_with("i") && !from_type.ends_with("*") && !to_type.ends_with("*") {
            let from_bits: u32 = from_type.trim_start_matches('i').parse().unwrap_or(64);
            let to_bits: u32 = to_type.trim_start_matches('i').parse().unwrap_or(64);
            
            if to_bits > from_bits {
                // 符号扩展
                self.emit_line(&format!("  {} = sext {} {} to {}",
                    temp, from_type, val, to_type));
            } else {
                // 截断
                self.emit_line(&format!("  {} = trunc {} {} to {}",
                    temp, from_type, val, to_type));
            }
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 整数到浮点
        if from_type.starts_with("i") && !from_type.ends_with("*") && 
           (to_type == "float" || to_type == "double") {
            self.emit_line(&format!("  {} = sitofp {} {} to {}",
                temp, from_type, val, to_type));
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 浮点到整数
        if (from_type == "float" || from_type == "double") && 
           to_type.starts_with("i") && !to_type.ends_with("*") {
            self.emit_line(&format!("  {} = fptosi {} {} to {}",
                temp, from_type, val, to_type));
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 浮点到浮点
        if (from_type == "float" || from_type == "double") && 
           (to_type == "float" || to_type == "double") {
            if to_type == "double" {
                self.emit_line(&format!("  {} = fpext {} {} to {}",
                    temp, from_type, val, to_type));
            } else {
                self.emit_line(&format!("  {} = fptrunc {} {} to {}",
                    temp, from_type, val, to_type));
            }
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 浮点到字符串（float/double -> String）
        if (from_type == "float" || from_type == "double") && to_type == "i8*" {
            // 关键修复：C 的可变参数函数中，float 会被提升为 double
            // 所以即使原类型是 float，也必须 fpext 到 double 再传参
            let arg_val = if from_type == "float" {
                let promoted = self.new_temp();
                self.emit_line(&format!("  {} = fpext float {} to double", promoted, val));
                promoted
            } else {
                val.to_string()  // 已经是 double
            };

            // 调用专门的运行时函数来避免调用约定问题
            let result = self.new_temp();
            self.emit_line(&format!("  {} = call i8* @__cay_float_to_string(double {})",
                result, arg_val));

            return Ok(format!("{} {}", to_type, result));
        }
        
        // 字符到字符串（char -> String）- 必须在整数转字符串之前处理
        if from_type == "i8" && to_type == "i8*" {
            let result = self.new_temp();
            self.emit_line(&format!("  {} = call i8* @__cay_char_to_string(i8 {})",
                result, val));
            return Ok(format!("{} {}", to_type, result));
        }
        
        // 布尔到字符串（bool -> String）
        // 布尔可能是 i1 或 i8，需要处理两种情况
        if (from_type == "i1" || from_type == "i8") && to_type == "i8*" {
            let result = self.new_temp();
            let bool_val = if from_type == "i1" {
                val.to_string()
            } else {
                // 将 i8 截断为 i1
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = trunc i8 {} to i1", temp, val));
                temp
            };
            self.emit_line(&format!("  {} = call i8* @__cay_bool_to_string(i1 {})",
                result, bool_val));
            return Ok(format!("{} {}", to_type, result));
        }
        
        // 整数到字符串（int -> String）- 放在字符和布尔之后
        if from_type.starts_with("i") && !from_type.ends_with("*") && to_type == "i8*" {
            // 先将整数扩展到 i64（如果还不是的话），然后调用运行时函数
            let result = self.new_temp();
            let i64_val = if from_type == "i64" {
                val.to_string()
            } else {
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = sext {} {} to i64", temp, from_type, val));
                temp
            };
            self.emit_line(&format!("  {} = call i8* @__cay_int_to_string(i64 {})",
                result, i64_val));
            return Ok(format!("{} {}", to_type, result));
        }
        
        Err(codegen_error(format!("Unsupported cast from {} to {}", from_type, to_type)))
    }
}
