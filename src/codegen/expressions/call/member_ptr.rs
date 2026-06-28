//! 函数调用表达式代码生成 - 成员函数指针调用
//!
//! 处理成员函数指针字段的调用生成。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::error::{cayResult, codegen_error_at};

impl IRGenerator {
    /// 生成成员函数指针字段调用
    pub fn generate_member_func_ptr_call(
        &mut self,
        member: &crate::ast::MemberAccessExpr,
        args: &[Expr],
        func_type: &crate::types::Type,
        loc: &crate::error::SourceLocation,
    ) -> cayResult<String> {
        use crate::ast::Expr;
        use crate::types::{FunctionType, Type};

        // 获取函数类型信息
        let (param_types, ret_type, is_static) = if let Type::Function(func) = func_type {
            (
                func.params.clone(),
                *func.return_type.clone(),
                func.is_static,
            )
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!("Field '{}' is not a function pointer", member.member),
            ));
        };

        // 检查参数数量
        if args.len() != param_types.len() {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Function pointer call requires {} arguments, but got {}",
                    param_types.len(),
                    args.len()
                ),
            ));
        }

        // 生成参数
        let mut arg_values = Vec::new();
        for (i, arg) in args.iter().enumerate() {
            let arg_result = self.generate_expression(arg)?;
            let (arg_type, arg_val) = self.parse_typed_value(&arg_result);
            let param_llvm_type = self.type_to_llvm(&param_types[i]);
            let converted_arg = self.convert_arg_type(&arg_type, &arg_val, &param_llvm_type);
            arg_values.push(converted_arg);
        }

        // 生成对象表达式以获取this指针
        let obj_result = self.generate_expression(&member.object)?;
        let (_, obj_val) = self.parse_typed_value(&obj_result);

        // 确定类名：如果是this，使用current_class；否则从表达式推断
        let class_name = if let Expr::Identifier(obj_name) = member.object.as_ref() {
            if obj_name.as_ref() == "this" {
                self.current_class.clone()
            } else if let Some(obj_type) = self.get_expression_type(&member.object) {
                if let Type::Object(name) = obj_type {
                    name
                } else {
                    return Err(codegen_error_at(
                        loc.clone(),
                        "Object is not a class instance".to_string(),
                    ));
                }
            } else {
                return Err(codegen_error_at(
                    loc.clone(),
                    "Cannot determine object type".to_string(),
                ));
            }
        } else if let Some(obj_type) = self.get_expression_type(&member.object) {
            if let Type::Object(name) = obj_type {
                name
            } else {
                return Err(codegen_error_at(
                    loc.clone(),
                    "Object is not a class instance".to_string(),
                ));
            }
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                "Cannot determine object type".to_string(),
            ));
        };

        // 获取字段信息（使用类布局信息获取偏移量）
        let field_offset =
            if let Some(field_info) = self.get_instance_field(&class_name, &member.member) {
                field_info.offset
            } else {
                return Err(codegen_error_at(
                    loc.clone(),
                    format!(
                        "Field '{}' not found in class '{}'",
                        member.member, class_name
                    ),
                ));
            };

        // 计算字段地址
        let field_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            field_ptr_i8, obj_val, field_offset
        ));

        // 加载函数指针
        // 注意：函数指针字段存储的是闭包结构体指针（i8*），而不是直接的函数指针
        // 闭包结构体布局：[函数指针: i8*][环境指针: i8*]
        // 所以我们需要从闭包结构体中加载函数指针

        let closure_ptr_slot = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8**",
            closure_ptr_slot, field_ptr_i8
        ));

        let closure_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8** {}, align 8",
            closure_ptr_i8, closure_ptr_slot
        ));

        // 从闭包结构体中加载函数指针（偏移 0）
        let func_ptr_slot = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8**",
            func_ptr_slot, closure_ptr_i8
        ));

        let loaded_func_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8** {}, align 8",
            loaded_func_ptr_i8, func_ptr_slot
        ));

        // 将函数指针转换为正确的类型（包含this指针）
        let loaded_func_ptr = self.new_temp();
        // 构建包含this的函数类型: ret_type (i8*, param_types...)
        let llvm_ret_type = self.type_to_llvm(&ret_type);
        let param_llvm_types: Vec<String> =
            param_types.iter().map(|t| self.type_to_llvm(t)).collect();
        let func_type_with_this = if llvm_ret_type == "void" {
            if param_llvm_types.is_empty() {
                "void (i8*)*".to_string()
            } else {
                format!("void (i8*, {})*", param_llvm_types.join(", "))
            }
        } else {
            if param_llvm_types.is_empty() {
                format!("{} (i8*)*", llvm_ret_type)
            } else {
                format!("{} (i8*, {})*", llvm_ret_type, param_llvm_types.join(", "))
            }
        };
        // 根据是否是静态方法决定是否传递this指针
        let (func_type_str, final_args) = if is_static {
            // 静态方法：不传递this指针
            let func_type_str = if llvm_ret_type == "void" {
                if param_llvm_types.is_empty() {
                    "void ()*".to_string()
                } else {
                    format!("void ({})*", param_llvm_types.join(", "))
                }
            } else {
                if param_llvm_types.is_empty() {
                    format!("{} ()*", llvm_ret_type)
                } else {
                    format!("{} ({})*", llvm_ret_type, param_llvm_types.join(", "))
                }
            };
            (func_type_str, arg_values)
        } else {
            // 实例方法：传递this指针作为第一个参数
            let func_type_str = if llvm_ret_type == "void" {
                if param_llvm_types.is_empty() {
                    "void (i8*)*".to_string()
                } else {
                    format!("void (i8*, {})*", param_llvm_types.join(", "))
                }
            } else {
                if param_llvm_types.is_empty() {
                    format!("{} (i8*)*", llvm_ret_type)
                } else {
                    format!("{} (i8*, {})*", llvm_ret_type, param_llvm_types.join(", "))
                }
            };
            let mut args = vec![format!("i8* {}", obj_val)];
            args.extend(arg_values);
            (func_type_str, args)
        };

        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}",
            loaded_func_ptr, loaded_func_ptr_i8, func_type_str
        ));

        // 生成调用
        if llvm_ret_type == "void" {
            self.emit_line(&format!(
                "  call void {}({})",
                loaded_func_ptr,
                final_args.join(", ")
            ));
            Ok("void %dummy".to_string())
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = call {} {}({})",
                temp,
                llvm_ret_type,
                loaded_func_ptr,
                final_args.join(", ")
            ));
            Ok(format!("{} {}", llvm_ret_type, temp))
        }
    }
}
