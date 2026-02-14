//! 表达式代码生成工具函数
//!
//! 提供类型提升、左值信息获取等通用工具函数。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error};

impl IRGenerator {
    /// 提升整数操作数到相同类型
    ///
    /// # Arguments
    /// * `left_type` - 左操作数类型
    /// * `left_val` - 左操作数值
    /// * `right_type` - 右操作数类型
    /// * `right_val` - 右操作数值
    ///
    /// # Returns
    /// (目标类型, 提升后的左值, 提升后的右值)
    pub fn promote_integer_operands(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str) -> (String, String, String) {
        // 检查是否为指针类型（如 i8*），指针类型不参与整数提升
        let left_is_ptr = left_type.ends_with('*');
        let right_is_ptr = right_type.ends_with('*');
        
        if left_is_ptr || right_is_ptr {
            // 指针类型不应该调用此函数，返回原值以避免错误
            return (left_type.to_string(), left_val.to_string(), right_val.to_string());
        }
        
        // char (i8) 类型在算术运算中需要提升到 i32
        let target_type = if left_type == "i8" || right_type == "i8" {
            "i32"
        } else if left_type == right_type {
            return (left_type.to_string(), left_val.to_string(), right_val.to_string());
        } else {
            // 确定提升后的类型（选择位数更大的类型）
            let left_bits: u32 = left_type.trim_start_matches('i').parse().unwrap_or(64);
            let right_bits: u32 = right_type.trim_start_matches('i').parse().unwrap_or(64);
            if left_bits >= right_bits { left_type } else { right_type }
        };
        
        // 提升左操作数
        let promoted_left = if left_type != target_type {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to {}", temp, left_type, left_val, target_type));
            temp
        } else {
            left_val.to_string()
        };
        
        // 提升右操作数
        let promoted_right = if right_type != target_type {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to {}", temp, right_type, right_val, target_type));
            temp
        } else {
            right_val.to_string()
        };
        
        (target_type.to_string(), promoted_left, promoted_right)
    }
    
    /// 提升浮点操作数到相同类型
    ///
    /// # Arguments
    /// * `left_type` - 左操作数类型
    /// * `left_val` - 左操作数值
    /// * `right_type` - 右操作数类型
    /// * `right_val` - 右操作数值
    ///
    /// # Returns
    /// (目标类型, 提升后的左值, 提升后的右值)
    pub fn promote_float_operands(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str) -> (String, String, String) {
        if left_type == right_type {
            return (left_type.to_string(), left_val.to_string(), right_val.to_string());
        }

        // 确定提升后的类型（选择精度更高的类型：double > float）
        if left_type == "double" || right_type == "double" {
            let promoted_type = "double".to_string();
            let mut promoted_left = left_val.to_string();
            let mut promoted_right = right_val.to_string();

            if left_type == "float" {
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = fpext float {} to double", temp, left_val));
                promoted_left = temp;
            }

            if right_type == "float" {
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = fpext float {} to double", temp, right_val));
                promoted_right = temp;
            }

            (promoted_type, promoted_left, promoted_right)
        } else {
            // 两者都是float，无需提升
            (left_type.to_string(), left_val.to_string(), right_val.to_string())
        }
    }

    /// 处理整数和浮点数的混合运算，将整数转换为浮点数
    ///
    /// # Arguments
    /// * `left_type` - 左操作数类型
    /// * `left_val` - 左操作数值
    /// * `right_type` - 右操作数类型
    /// * `right_val` - 右操作数值
    ///
    /// # Returns
    /// Some((目标类型, 提升后的左值, 提升后的右值)) 如果是混合类型
    /// None 如果不是混合类型
    pub fn promote_mixed_operands(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str) -> Option<(String, String, String)> {
        // 检查是否是混合类型（整数 + 浮点数）
        let left_is_int = left_type.starts_with("i") && !left_type.ends_with("*");
        let right_is_int = right_type.starts_with("i") && !right_type.ends_with("*");
        let left_is_float = left_type == "float" || left_type == "double";
        let right_is_float = right_type == "float" || right_type == "double";

        if left_is_int && right_is_float {
            // 整数 + 浮点数：将整数转换为浮点数
            let promoted_type = if right_type == "double" { "double" } else { "float" };
            let converted_left = self.new_temp();
            if promoted_type == "double" {
                self.emit_line(&format!("  {} = sitofp {} {} to double", converted_left, left_type, left_val));
            } else {
                self.emit_line(&format!("  {} = sitofp {} {} to float", converted_left, left_type, left_val));
            }
            Some((promoted_type.to_string(), converted_left, right_val.to_string()))
        } else if left_is_float && right_is_int {
            // 浮点数 + 整数：将整数转换为浮点数
            let promoted_type = if left_type == "double" { "double" } else { "float" };
            let converted_right = self.new_temp();
            if promoted_type == "double" {
                self.emit_line(&format!("  {} = sitofp {} {} to double", converted_right, right_type, right_val));
            } else {
                self.emit_line(&format!("  {} = sitofp {} {} to float", converted_right, right_type, right_val));
            }
            Some((promoted_type.to_string(), left_val.to_string(), converted_right))
        } else {
            None
        }
    }

    /// 获取左值的类型和 LLVM 指针表示
    ///
    /// # Arguments
    /// * `expr` - 表达式
    ///
    /// # Returns
    /// (类型字符串, 指针字符串)
    pub fn get_lvalue_info(&mut self, expr: &Expr) -> cayResult<(String, String)> {
        match expr {
            Expr::Identifier(name) => {
                // 优先使用作用域管理器获取变量类型
                let (var_type, llvm_name) = if let Some(scope_type) = self.scope_manager.get_var_type(name) {
                    let llvm_name = self.scope_manager.get_llvm_name(name).unwrap_or_else(|| name.clone());
                    (scope_type, llvm_name)
                } else {
                    // 检查是否是当前类的静态字段
                    if !self.current_class.is_empty() {
                        let static_key = format!("{}.{}", self.current_class, name);
                        if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                            return Ok((field_info.llvm_type, field_info.name));
                        }
                    }
                    // 回退到旧系统
                    let var_type = self.var_types.get(name)
                        .ok_or_else(|| codegen_error(format!("Variable '{}' not found", name)))?
                        .clone();
                    (var_type, name.clone())
                };
                Ok((var_type, format!("%{}", llvm_name)))
            }
            Expr::ArrayAccess(arr) => {
                let (elem_type, elem_ptr, _) = self.get_array_element_ptr(arr)?;
                Ok((elem_type, elem_ptr))
            }
            _ => Err(codegen_error("Invalid lvalue expression".to_string()))
        }
    }

    /// 生成运行时除零检查代码
    ///
    /// # Arguments
    /// * `val_type` - 除数类型
    /// * `val` - 除数值
    pub fn generate_division_by_zero_check(&mut self, val_type: &str, val: &str) -> cayResult<()> {
        // 创建标签
        let error_label = self.new_label("div.error");
        let continue_label = self.new_label("div.cont");

        // 检查除数是否为零
        let is_zero = self.new_temp();
        self.emit_line(&format!("  {} = icmp eq {} {}, 0", is_zero, val_type, val));
        self.emit_line(&format!("  br i1 {}, label %{}, label %{}", is_zero, error_label, continue_label));

        // 错误处理块
        self.emit_line(&format!("{}:", error_label));
        // 输出错误信息到 stderr
        let error_msg = self.get_or_create_string_constant("Error: Division by zero\n");
        self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {})", error_msg));
        // 调用 exit 退出程序
        self.emit_line("  call void @exit(i32 1)");
        self.emit_line("  unreachable");

        // 正常继续块
        self.emit_line(&format!("{}:", continue_label));

        Ok(())
    }

    /// 将 LLVM 类型转换为方法签名
    pub fn llvm_type_to_signature(&self, llvm_type: &str) -> String {
        match llvm_type {
            "i32" => "i".to_string(),
            "i64" => "l".to_string(),
            "float" => "f".to_string(),
            "double" => "d".to_string(),
            "i1" => "b".to_string(),
            "i8*" => "s".to_string(),
            "i8" => "c".to_string(),
            t if t.ends_with("*") => "o".to_string(), // 对象/数组指针
            _ => "x".to_string(), // 未知类型
        }
    }

    /// 将 LLVM 类型转换为方法签名（支持可变参数数组类型）
    pub fn llvm_type_to_signature_with_varargs(&self, llvm_type: &str, is_varargs_array: bool) -> String {
        if is_varargs_array {
            // 可变参数数组使用 ai 签名（array of int）
            "ai".to_string()
        } else {
            self.llvm_type_to_signature(llvm_type)
        }
    }

}
