//! 一元表达式代码生成
//!
//! 处理取负、逻辑非、位取反和自增/自减操作。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{cayResult, codegen_error_at};

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
                    self.emit_line(&format!("  {} = sub {} 0, {}", temp, op_type, op_val));
                } else {
                    self.emit_line(&format!("  {} = fneg {} {}", temp, op_type, op_val));
                }
            }
            UnaryOp::Not => {
                // 逻辑非操作：先将操作数转换为 i1（布尔值），然后取反
                let bool_val = if op_type == "i1" {
                    op_val.to_string()
                } else {
                    let cmp_temp = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = icmp ne {} {}, 0",
                        cmp_temp, op_type, op_val
                    ));
                    cmp_temp
                };
                self.emit_line(&format!("  {} = xor i1 {}, true", temp, bool_val));
                return Ok(format!("i1 {}", temp));
            }
            UnaryOp::BitNot => {
                // 位取反：xor 操作数与 -1
                if op_type.starts_with("i") {
                    self.emit_line(&format!("  {} = xor {} {}, -1", temp, op_type, op_val));
                } else {
                    // 浮点数不支持位取反，但类型系统应该已经阻止了这种情况
                    return Err(codegen_error_at(
                        unary.loc.clone(),
                        "Bitwise NOT not supported for floating point".to_string(),
                    ));
                }
            }
            UnaryOp::PreInc | UnaryOp::PostInc | UnaryOp::PreDec | UnaryOp::PostDec => {
                return self.generate_inc_dec(unary, op_type, op_val);
            }
            UnaryOp::AddressOf => {
                // 取地址操作：获取操作数的地址（指针）
                return self.generate_address_of(unary);
            }
            UnaryOp::Deref => {
                // 解引用操作：加载指针指向的值
                return self.generate_deref(unary, op_type, op_val);
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
    fn generate_inc_dec(
        &mut self,
        unary: &UnaryExpr,
        _op_type: String,
        _op_val: String,
    ) -> cayResult<String> {
        // 自增/自减操作：需要先获取变量地址，加载值，计算，存储
        let is_inc = unary.op == UnaryOp::PreInc || unary.op == UnaryOp::PostInc;
        let is_pre = unary.op == UnaryOp::PreInc || unary.op == UnaryOp::PreDec;

        // 获取正确的变量类型和指针
        let (llvm_type, llvm_ptr) = self.get_lvalue_info(&unary.operand)?;

        // 加载当前值
        let load_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load {}, {}* {}, align {}",
            load_temp,
            llvm_type,
            llvm_type,
            llvm_ptr,
            self.get_type_align(&llvm_type)
        ));

        // 计算新值
        let new_temp = self.new_temp();
        let one = if llvm_type == "float" || llvm_type == "double" {
            "1.0"
        } else {
            "1"
        };
        if llvm_type == "float" || llvm_type == "double" {
            if is_inc {
                self.emit_line(&format!(
                    "  {} = fadd {} {}, {}",
                    new_temp, llvm_type, load_temp, one
                ));
            } else {
                self.emit_line(&format!(
                    "  {} = fsub {} {}, {}",
                    new_temp, llvm_type, load_temp, one
                ));
            }
        } else {
            if is_inc {
                self.emit_line(&format!(
                    "  {} = add {} {}, {}",
                    new_temp, llvm_type, load_temp, one
                ));
            } else {
                self.emit_line(&format!(
                    "  {} = sub {} {}, {}",
                    new_temp, llvm_type, load_temp, one
                ));
            }
        }

        // 存储新值
        self.emit_line(&format!(
            "  store {} {}, {}* {}, align {}",
            llvm_type,
            new_temp,
            llvm_type,
            llvm_ptr,
            self.get_type_align(&llvm_type)
        ));

        // 前置返回新值，后缀返回旧值
        if is_pre {
            Ok(format!("{} {}", llvm_type, new_temp))
        } else {
            Ok(format!("{} {}", llvm_type, load_temp))
        }
    }

    /// 生成取地址表达式代码 (&variable)
    ///
    /// # Arguments
    /// * `unary` - 一元表达式（必须是AddressOf操作）
    fn generate_address_of(&mut self, unary: &UnaryExpr) -> cayResult<String> {
        // 获取操作数的左值信息（类型和指针）
        let (llvm_type, llvm_ptr) = self.get_lvalue_info(&unary.operand)?;

        // Cavvy 对象有 16 字节的对象头（type_id: i32 + padding: i32 + vtable_ptr: i64）
        // 当获取对象地址用于 FFI 调用时，需要跳过对象头，指向字段数据
        if llvm_type == "i8*" {
            // llvm_ptr 是 i8** (alloca of object pointer)
            // 加载对象指针，然后跳过16字节头部
            let obj_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = load {}, {}* {}, align {}",
                obj_ptr,
                llvm_type,
                llvm_type,
                llvm_ptr,
                self.get_type_align(&llvm_type)
            ));
            let data_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr i8, i8* {}, i64 16",
                data_ptr, obj_ptr
            ));
            Ok(format!("i8* {}", data_ptr))
        } else {
            // 对于非对象类型（i32, float, double等），直接返回栈上地址
            // llvm_ptr 是 llvm_type* 类型的 alloca 指针
            // 注意：不能 load 值，否则得到的将是变量的值而非地址
            // 例如 &i 应返回 i32* %i_s3，而非 i32 42
            Ok(format!("{}* {}", llvm_type, llvm_ptr))
        }
    }

    /// 生成解引用表达式代码 (*pointer)
    ///
    /// # Arguments
    /// * `unary` - 一元表达式（必须是Deref操作）
    /// * `op_type` - 操作数类型（应该是指针类型）
    /// * `op_val` - 操作数值（应该是指针值）
    fn generate_deref(
        &mut self,
        unary: &UnaryExpr,
        op_type: String,
        op_val: String,
    ) -> cayResult<String> {
        // 解析指针类型，获取指向的类型
        // op_type 应该是 "i32*" 或 "i64*" 等格式
        if !op_type.ends_with('*') {
            return Err(codegen_error_at(
                unary.loc.clone(),
                format!("Cannot dereference non-pointer type: {}", op_type),
            ));
        }

        // 提取指向的类型（去掉末尾的*）
        let elem_type = op_type[..op_type.len() - 1].to_string();

        // 加载指针指向的值
        let temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load {}, {} {}, align {}",
            temp,
            elem_type,
            op_type,
            op_val,
            self.get_type_align(&elem_type)
        ));

        Ok(format!("{} {}", elem_type, temp))
    }
}
