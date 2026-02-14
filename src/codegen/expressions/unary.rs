//! 一元表达式代码生成
//!
//! 处理取负、逻辑非、位取反和自增/自减操作。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error};

impl IRGenerator {
    /// 生成一元表达式代码
    ///
    /// # Arguments
    /// * `unary` - 一元表达式
    pub fn generate_unary_expression(&mut self, unary: &UnaryExpr) -> cayResult<String> {
        let operand = self.generate_expression(&unary.operand)?;
        let (op_type, op_val) = self.parse_typed_value(&operand);
        let temp = self.new_temp();
        
        match unary.op {
            UnaryOp::Neg => {
                if op_type.starts_with("i") {
                    self.emit_line(&format!("  {} = sub {} 0, {}",
                        temp, op_type, op_val));
                } else {
                    self.emit_line(&format!("  {} = fneg {} {}",
                        temp, op_type, op_val));
                }
            }
            UnaryOp::Not => {
                self.emit_line(&format!("  {} = xor {} {}, 1",
                    temp, op_type, op_val));
                return Ok(format!("i1 {}", temp));
            }
            UnaryOp::BitNot => {
                // 位取反：xor 操作数与 -1
                if op_type.starts_with("i") {
                    self.emit_line(&format!("  {} = xor {} {}, -1",
                        temp, op_type, op_val));
                } else {
                    // 浮点数不支持位取反，但类型系统应该已经阻止了这种情况
                    return Err(codegen_error("Bitwise NOT not supported for floating point".to_string()));
                }
            }
            UnaryOp::PreInc | UnaryOp::PostInc | UnaryOp::PreDec | UnaryOp::PostDec => {
                return self.generate_inc_dec(unary, op_type, op_val);
            }
        }
        
        Ok(format!("{} {}", op_type, temp))
    }

    /// 生成自增/自减表达式代码
    ///
    /// # Arguments
    /// * `unary` - 一元表达式（必须是自增/自减操作）
    /// * `op_type` - 操作数类型
    /// * `op_val` - 操作数值
    fn generate_inc_dec(&mut self, unary: &UnaryExpr, _op_type: String, _op_val: String) -> cayResult<String> {
        // 自增/自减操作：需要先获取变量地址，加载值，计算，存储
        let is_inc = unary.op == UnaryOp::PreInc || unary.op == UnaryOp::PostInc;
        let is_pre = unary.op == UnaryOp::PreInc || unary.op == UnaryOp::PreDec;
        
        // 获取正确的变量类型和指针
        let (llvm_type, llvm_ptr) = self.get_lvalue_info(&unary.operand)?;
        
        // 加载当前值
        let load_temp = self.new_temp();
        self.emit_line(&format!("  {} = load {}, {}* {}, align {}",
            load_temp, llvm_type, llvm_type, llvm_ptr, self.get_type_align(&llvm_type)));
        
        // 计算新值
        let new_temp = self.new_temp();
        let one = if llvm_type == "float" || llvm_type == "double" { "1.0" } else { "1" };
        if llvm_type == "float" || llvm_type == "double" {
            if is_inc {
                self.emit_line(&format!("  {} = fadd {} {}, {}",
                    new_temp, llvm_type, load_temp, one));
            } else {
                self.emit_line(&format!("  {} = fsub {} {}, {}",
                    new_temp, llvm_type, load_temp, one));
            }
        } else {
            if is_inc {
                self.emit_line(&format!("  {} = add {} {}, {}",
                    new_temp, llvm_type, load_temp, one));
            } else {
                self.emit_line(&format!("  {} = sub {} {}, {}",
                    new_temp, llvm_type, load_temp, one));
            }
        }
        
        // 存储新值
        self.emit_line(&format!("  store {} {}, {}* {}, align {}",
            llvm_type, new_temp, llvm_type, llvm_ptr, self.get_type_align(&llvm_type)));
        
        // 前置返回新值，后缀返回旧值
        if is_pre {
            Ok(format!("{} {}", llvm_type, new_temp))
        } else {
            Ok(format!("{} {}", llvm_type, load_temp))
        }
    }
}
