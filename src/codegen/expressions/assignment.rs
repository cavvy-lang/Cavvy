//! 赋值表达式代码生成
//!
//! 处理变量赋值、数组元素赋值和静态字段赋值。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error};

impl IRGenerator {
    /// 生成赋值表达式代码
    ///
    /// # Arguments
    /// * `assign` - 赋值表达式
    pub fn generate_assignment(&mut self, assign: &AssignmentExpr) -> cayResult<String> {
        let value = self.generate_expression(&assign.value)?;
        let (value_type, val) = self.parse_typed_value(&value);
        
        match assign.target.as_ref() {
            Expr::MemberAccess(member) => {
                self.generate_member_assignment(member, &value_type, &val, &value)
            }
            Expr::Identifier(name) => {
                self.generate_variable_assignment(name, &value_type, &val, &value)
            }
            Expr::ArrayAccess(arr_access) => {
                self.generate_array_assignment(arr_access, &value_type, &val, &value)
            }
            _ => Err(codegen_error("Invalid assignment target".to_string()))
        }
    }

    /// 生成成员赋值（静态字段赋值）
    fn generate_member_assignment(&mut self, member: &MemberAccessExpr, value_type: &str, val: &str, value: &str) -> cayResult<String> {
        // 检查是否是静态字段赋值: ClassName.fieldName = value
        if let Expr::Identifier(class_name) = &*member.object {
            let static_key = format!("{}.{}", class_name, member.member);
            if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                // 静态字段赋值
                let align = self.get_type_align(&field_info.llvm_type);
                
                // 如果值类型与字段类型不匹配，需要转换
                if value_type != field_info.llvm_type {
                    let temp = self.new_temp();
                    // 类型转换逻辑（简化版）
                    if value_type.starts_with("i") && field_info.llvm_type.starts_with("i") {
                        let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                        let to_bits: u32 = field_info.llvm_type.trim_start_matches('i').parse().unwrap_or(64);
                        if to_bits > from_bits {
                            self.emit_line(&format!("  {} = sext {} {} to {}",
                                temp, value_type, val, field_info.llvm_type));
                        } else {
                            self.emit_line(&format!("  {} = trunc {} {} to {}",
                                temp, value_type, val, field_info.llvm_type));
                        }
                        self.emit_line(&format!("  store {} {}, {}* {}, align {}", 
                            field_info.llvm_type, temp, field_info.llvm_type, field_info.name, align));
                        return Ok(format!("{} {}", field_info.llvm_type, temp));
                    }
                }
                
                // 类型匹配，直接存储
                self.emit_line(&format!("  store {} {}, {}* {}, align {}", 
                    value_type, val, field_info.llvm_type, field_info.name, align));
                return Ok(value.to_string());
            }
        }
        Err(codegen_error("Invalid member access assignment target".to_string()))
    }

    /// 生成变量赋值
    fn generate_variable_assignment(&mut self, name: &str, value_type: &str, val: &str, value: &str) -> cayResult<String> {
        // 优先使用作用域管理器获取变量类型和 LLVM 名称
        let (var_type, llvm_name) = if let Some(scope_type) = self.scope_manager.get_var_type(name) {
            let llvm_name = self.scope_manager.get_llvm_name(name).unwrap_or_else(|| name.to_string());
            (scope_type, llvm_name)
        } else {
            // 检查是否是当前类的静态字段
            if !self.current_class.is_empty() {
                let static_key = format!("{}.{}", self.current_class, name);
                if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                    let align = self.get_type_align(&field_info.llvm_type);
                    self.emit_line(&format!("  store {} {}, {}* {}, align {}",
                        field_info.llvm_type, val, field_info.llvm_type, field_info.name, align));
                    return Ok(value.to_string());
                }
            }
            // 回退到旧系统
            let var_type = self.var_types.get(name)
                .ok_or_else(|| codegen_error(format!("Variable '{}' not found", name)))?
                .clone();
            (var_type, name.to_string())
        };

        // 如果值类型与变量类型不匹配，需要转换
        if value_type != var_type {
            return self.generate_assignment_with_conversion(&var_type, &llvm_name, value_type, val);
        }

        // 类型匹配，直接存储
        let align = self.get_type_align(&var_type);
        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, val, var_type, llvm_name, align));
        Ok(value.to_string())
    }

    /// 生成数组元素赋值
    fn generate_array_assignment(&mut self, arr_access: &ArrayAccessExpr, value_type: &str, val: &str, value: &str) -> cayResult<String> {
        // 获取数组元素指针
        let (elem_type, elem_ptr, _) = self.get_array_element_ptr(arr_access)?;

        // 如果值类型与元素类型不匹配，需要转换
        if value_type != elem_type {
            return self.generate_array_assignment_with_conversion(&elem_type, &elem_ptr, value_type, val, value);
        }

        // 类型匹配，直接存储到数组元素
        let align = self.get_type_align(&elem_type);
        self.emit_line(&format!("  store {} {}, {}* {}, align {}", elem_type, val, elem_type, elem_ptr, align));
        Ok(value.to_string())
    }

    /// 生成带类型转换的变量赋值
    fn generate_assignment_with_conversion(&mut self, var_type: &str, llvm_name: &str, value_type: &str, val: &str) -> cayResult<String> {
        let temp = self.new_temp();

        // 浮点类型转换
        if value_type == "double" && var_type == "float" {
            // double -> float 转换
            self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
            let align = self.get_type_align("float");
            self.emit_line(&format!("  store float {}, float* %{}, align {}", temp, llvm_name, align));
            return Ok(format!("float {}", temp));
        } else if value_type == "float" && var_type == "double" {
            // float -> double 转换
            self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
            let align = self.get_type_align("double");
            self.emit_line(&format!("  store double {}, double* %{}, align {}", temp, llvm_name, align));
            return Ok(format!("double {}", temp));
        }
        // 整数到浮点数转换
        else if value_type.starts_with("i") && (var_type == "float" || var_type == "double") {
            // 整数 -> 浮点数转换
            self.emit_line(&format!("  {} = sitofp {} {} to {}", temp, value_type, val, var_type));
            let align = self.get_type_align(var_type);
            self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
            return Ok(format!("{} {}", var_type, temp));
        }
        // 整数类型转换
        else if value_type.starts_with("i") && var_type.starts_with("i") {
            let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
            let to_bits: u32 = var_type.trim_start_matches('i').parse().unwrap_or(64);

            if to_bits > from_bits {
                // 符号扩展
                self.emit_line(&format!("  {} = sext {} {} to {}",
                    temp, value_type, val, var_type));
            } else {
                // 截断
                self.emit_line(&format!("  {} = trunc {} {} to {}",
                    temp, value_type, val, var_type));
            }
            let align = self.get_type_align(var_type);
            self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
            return Ok(format!("{} {}", var_type, temp));
        }

        // 默认情况：直接存储
        let align = self.get_type_align(var_type);
        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, val, var_type, llvm_name, align));
        Ok(format!("{} {}", var_type, val))
    }

    /// 生成带类型转换的数组元素赋值
    fn generate_array_assignment_with_conversion(&mut self, elem_type: &str, elem_ptr: &str, value_type: &str, val: &str, value: &str) -> cayResult<String> {
        let temp = self.new_temp();

        // 浮点类型转换
        if value_type == "double" && elem_type == "float" {
            // double -> float 转换
            self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
            let align = self.get_type_align(elem_type);
            self.emit_line(&format!("  store float {}, {}* {}, align {}", temp, elem_type, elem_ptr, align));
            return Ok(format!("float {}", temp));
        } else if value_type == "float" && elem_type == "double" {
            // float -> double 转换
            self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
            let align = self.get_type_align(elem_type);
            self.emit_line(&format!("  store double {}, {}* {}, align {}", temp, elem_type, elem_ptr, align));
            return Ok(format!("double {}", temp));
        }
        // 整数到浮点数转换
        else if value_type.starts_with("i") && (elem_type == "float" || elem_type == "double") {
            // 整数 -> 浮点数转换
            self.emit_line(&format!("  {} = sitofp {} {} to {}", temp, value_type, val, elem_type));
            let align = self.get_type_align(elem_type);
            self.emit_line(&format!("  store {} {}, {}* {}, align {}", elem_type, temp, elem_type, elem_ptr, align));
            return Ok(format!("{} {}", elem_type, temp));
        }
        // 整数类型转换
        else if value_type.starts_with("i") && elem_type.starts_with("i") {
            let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
            let to_bits: u32 = elem_type.trim_start_matches('i').parse().unwrap_or(64);

            if to_bits > from_bits {
                // 符号扩展
                self.emit_line(&format!("  {} = sext {} {} to {}",
                    temp, value_type, val, elem_type));
            } else {
                // 截断
                self.emit_line(&format!("  {} = trunc {} {} to {}",
                    temp, value_type, val, elem_type));
            }
            let align = self.get_type_align(elem_type);
            self.emit_line(&format!("  store {} {}, {}* {}, align {}", elem_type, temp, elem_type, elem_ptr, align));
            return Ok(format!("{} {}", elem_type, temp));
        }

        // 默认情况：直接存储
        let align = self.get_type_align(elem_type);
        self.emit_line(&format!("  store {} {}, {}* {}, align {}", elem_type, val, elem_type, elem_ptr, align));
        Ok(value.to_string())
    }
}
