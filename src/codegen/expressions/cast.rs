//! 类型转换表达式代码生成
//!
//! 处理整数、浮点数、指针之间的类型转换，以及到字符串的转换。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error_at};

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
        
        // 布尔到字符串（bool -> String）- 必须在整数到字符串之前处理
        // 因为 i1 也匹配 starts_with("i")
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
        
        // 整数到字符串的转换（int -> String）- 必须在整数到指针之前处理
        // 因为 i8* 也是指针类型
        // 排除 i1（布尔）和 i8（字符），它们已单独处理
        if from_type.starts_with("i") && !from_type.ends_with("*") && to_type == "i8*"
            && from_type != "i1" && from_type != "i8" {
            // 先将整数转换到 i32（如果还不是的话），然后调用运行时函数
            let result = self.new_temp();
            let i32_val = if from_type == "i32" {
                val.to_string()
            } else {
                let temp = self.new_temp();
                // 根据位宽选择正确的转换指令：扩展(sext)或截断(trunc)
                let from_bits: u32 = from_type.trim_start_matches('i').parse().unwrap_or(32);
                if from_bits < 32 {
                    // 小类型到大类型：符号扩展
                    self.emit_line(&format!("  {} = sext {} {} to i32", temp, from_type, val));
                } else {
                    // 大类型到小类型：截断
                    self.emit_line(&format!("  {} = trunc {} {} to i32", temp, from_type, val));
                }
                temp
            };
            self.emit_line(&format!("  {} = call i8* @__cay_int_to_string(i32 {})",
                result, i32_val));
            return Ok(format!("{} {}", to_type, result));
        }
        
        // 指针类型转换 (bitcast) - 优先检查
        if from_type.ends_with("*") && to_type.ends_with("*") {
            self.emit_line(&format!("  {} = bitcast {} {} to {}",
                temp, from_type, val, to_type));
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 指针到整数的转换（ptrtoint）- 优先检查
        if from_type.ends_with("*") && to_type.starts_with("i") && !to_type.ends_with("*") {
            self.emit_line(&format!("  {} = ptrtoint {} {} to {}",
                temp, from_type, val, to_type));
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 整数到指针的转换（inttoptr）- 优先检查（排除 i8* 因为已处理）
        // 使用 i64 作为中间类型（指针大小）
        if from_type.starts_with("i") && !from_type.ends_with("*") && to_type.ends_with("*") && to_type != "i8*" {
            // LLVM 不支持 void*，使用 i8* 代替
            let llvm_to_type = if to_type == "void*" { "i8*".to_string() } else { to_type.clone() };
            if from_type != "i64" {
                let i64_temp = self.new_temp();
                self.emit_line(&format!("  {} = sext {} {} to i64", i64_temp, from_type, val));
                self.emit_line(&format!("  {} = inttoptr i64 {} to {}", temp, i64_temp, llvm_to_type));
            } else {
                self.emit_line(&format!("  {} = inttoptr {} {} to {}", temp, from_type, val, llvm_to_type));
            }
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 严格检查整数类型（排除指针）
        let is_from_ptr = from_type.ends_with("*");
        let is_to_ptr = to_type.ends_with("*");
        let is_from_int = from_type.starts_with("i") && !is_from_ptr;
        let is_to_int = to_type.starts_with("i") && !is_to_ptr;
        
        // 整数到整数
        if is_from_int && is_to_int {
            let from_bits: u32 = from_type.trim_start_matches('i').parse().unwrap_or(64);
            let to_bits: u32 = to_type.trim_start_matches('i').parse().unwrap_or(64);
            
            // 判断源类型是否是无符号类型
            let is_from_unsigned = if let Some(src_type) = self.get_expression_type(&cast.expr) {
                matches!(src_type, crate::types::Type::CUChar | 
                                  crate::types::Type::CUShort | 
                                  crate::types::Type::CUInt |
                                  crate::types::Type::SizeT |
                                  crate::types::Type::UIntPtr)
            } else {
                false
            };
            
            if to_bits > from_bits {
                // 根据源类型选择扩展方式：无符号用零扩展(zext)，有符号用符号扩展(sext)
                let ext_op = if is_from_unsigned { "zext" } else { "sext" };
                self.emit_line(&format!("  {} = {} {} {} to {}",
                    temp, ext_op, from_type, val, to_type));
            } else {
                // 截断
                self.emit_line(&format!("  {} = trunc {} {} to {}",
                    temp, from_type, val, to_type));
            }
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 整数到浮点
        if is_from_int && (to_type == "float" || to_type == "double") {
            self.emit_line(&format!("  {} = sitofp {} {} to {}",
                temp, from_type, val, to_type));
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 浮点到整数
        if (from_type == "float" || from_type == "double") && is_to_int {
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
            // 根据原始类型选择正确的转换函数
            if from_type == "float" {
                self.emit_line(&format!("  {} = call i8* @__cay_float_to_string(float {})",
                    result, val));
            } else {
                self.emit_line(&format!("  {} = call i8* @__cay_double_to_string(double {})",
                    result, arg_val));
            }

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
        
        // 字符串到整数（String -> int）- 使用 atoi
        if from_type == "i8*" && to_type.starts_with("i") && !to_type.ends_with("*") {
            // 调用 atoi 函数将字符串转换为整数
            let atoi_result = self.new_temp();
            self.emit_line(&format!("  {} = call i32 @atoi(i8* {})", atoi_result, val));

            // 如果目标类型不是 i32，需要转换
            if to_type == "i32" {
                return Ok(format!("i32 {}", atoi_result));
            } else {
                // 转换到目标整数类型
                let final_result = self.new_temp();
                let to_bits: u32 = to_type.trim_start_matches('i').parse().unwrap_or(32);
                if to_bits > 32 {
                    self.emit_line(&format!("  {} = sext i32 {} to {}", final_result, atoi_result, to_type));
                } else if to_bits < 32 {
                    self.emit_line(&format!("  {} = trunc i32 {} to {}", final_result, atoi_result, to_type));
                } else {
                    return Ok(format!("i32 {}", atoi_result));
                }
                return Ok(format!("{} {}", to_type, final_result));
            }
        }

        Err(codegen_error_at(cast.loc.clone(), format!("Unsupported cast from {} to {}", from_type, to_type)))
    }
}
