//! Return语句代码生成
//!
//! 处理return语句的代码生成。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成return语句代码
    pub fn generate_return_statement(&mut self, expr: &Option<Expr>) -> cayResult<()> {
        if let Some(e) = expr.as_ref() {
            let value = self.generate_expression(e)?;
            let (value_type, val) = self.parse_typed_value(&value);
            let ret_type = self.current_return_type.clone();

            // 如果返回类型是 void，但表达式非空，这是错误（但由语义分析处理）
            if ret_type == "void" {
                self.emit_line("  ret void");
            } else if value_type != ret_type {
                // 需要类型转换
                let temp = self.new_temp();

                // 特殊处理：null 值（i64 0）转换为指针类型
                if value == "i64 0" && ret_type.ends_with("*") {
                    self.emit_line(&format!("  ret {} null", ret_type));
                    return Ok(());
                }

                // 浮点类型转换
                if value_type == "double" && ret_type == "float" {
                    // double -> float 转换
                    self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                    let align = self.get_type_align("float");
                    self.emit_line(&format!("  ret float {}", temp));
                } else if value_type == "float" && ret_type == "double" {
                    // float -> double 转换
                    self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                    let align = self.get_type_align("double");
                    self.emit_line(&format!("  ret double {}", temp));
                }
                // 指针到整数转换 (ptrtoint)
                else if value_type.ends_with("*") && ret_type.starts_with("i") && !ret_type.ends_with("*") {
                    self.emit_line(&format!("  {} = ptrtoint {} {} to {}",
                        temp, value_type, val, ret_type));
                    self.emit_line(&format!("  ret {} {}", ret_type, temp));
                }
                // 整数到指针转换 (inttoptr)
                else if value_type.starts_with("i") && !value_type.ends_with("*") && ret_type.ends_with("*") {
                    self.emit_line(&format!("  {} = inttoptr {} {} to {}",
                        temp, value_type, val, ret_type));
                    self.emit_line(&format!("  ret {} {}", ret_type, temp));
                }
                // 整数类型转换
                else if value_type.starts_with("i") && ret_type.starts_with("i")
                    && !value_type.ends_with("*") && !ret_type.ends_with("*") {
                    let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                    let to_bits: u32 = ret_type.trim_start_matches('i').parse().unwrap_or(64);

                    if to_bits > from_bits {
                        // 符号扩展
                        self.emit_line(&format!("  {} = sext {} {} to {}",
                            temp, value_type, val, ret_type));
                    } else {
                        // 截断
                        self.emit_line(&format!("  {} = trunc {} {} to {}",
                            temp, value_type, val, ret_type));
                    }
                    self.emit_line(&format!("  ret {} {}", ret_type, temp));
                }
                // 整数到浮点数转换
                else if value_type.starts_with("i") && !value_type.ends_with("*")
                    && (ret_type == "float" || ret_type == "double") {
                    self.emit_line(&format!("  {} = sitofp {} {} to {}",
                        temp, value_type, val, ret_type));
                    self.emit_line(&format!("  ret {} {}", ret_type, temp));
                }
                // 浮点数到整数转换
                else if (value_type == "float" || value_type == "double") && ret_type.starts_with("i") {
                    self.emit_line(&format!("  {} = fptosi {} {} to {}",
                        temp, value_type, val, ret_type));
                    self.emit_line(&format!("  ret {} {}", ret_type, temp));
                }
                else {
                    // 类型不兼容，直接返回（可能会出错）
                    self.emit_line(&format!("  ret {}", value));
                }
            } else {
                // 类型匹配，直接返回
                self.emit_line(&format!("  ret {}", value));
            }
        } else {
            self.emit_line("  ret void");
        }

        Ok(())
    }
}
