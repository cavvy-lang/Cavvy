//! 函数调用表达式代码生成 - extern 函数调用
//!
//! 处理 extern 函数调用和运行时辅助函数调用。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, codegen_error_at, ErrorCodes};

impl IRGenerator {
    /// 生成 extern 函数调用
    ///
    /// # Arguments
    /// * `func_name` - extern 函数名称
    /// * `args` - 函数参数
    /// * `loc` - 源码位置
    pub fn generate_extern_function_call(
        &mut self,
        func_name: &str,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // 特殊处理运行时函数 __cay_buffer_to_string
        // 这个函数在运行时模块中已经定义，不需要从 extern 声明中查找
        if func_name == "__cay_buffer_to_string" {
            return self.generate_buffer_to_string_call(args, loc);
        }

        // 特殊处理指针操作运行时函数
        match func_name {
            "__cay_read_ptr" => return self.generate_read_ptr_call(args, loc),
            "__cay_ptr_to_string" => return self.generate_ptr_to_string_call(args, loc),
            "__cay_write_ptr" => return self.generate_write_ptr_call(args, loc),
            "__cay_write_int" => return self.generate_write_int_call(args, loc),
            _ => {}
        }

        // 获取 extern 函数信息（克隆以避免借用问题）
        let extern_func = {
            let found = self.get_extern_function(func_name);
            match found {
                Some(f) => f.clone(),
                None => {
                    return Err(codegen_error_at(ErrorCodes::CODEGEN_INVALID_OPERATION, 
                        loc.clone(),
                        format!("Extern function '{}' not found", func_name),
                    ));
                }
            }
        };

        // 使用实际的C函数名（而非别名）生成call指令
        let llvm_func_name = &extern_func.name;

        // 生成参数
        let mut arg_results = Vec::new();
        for arg in args {
            arg_results.push(self.generate_expression(arg)?);
        }

        // 获取参数类型和值
        let is_varargs = extern_func.params.iter().any(|p| p.is_varargs);
        let varargs_index = extern_func
            .params
            .iter()
            .position(|p| p.is_varargs)
            .unwrap_or(extern_func.params.len());
        let mut processed_args = Vec::new();
        for (idx, arg_str) in arg_results.iter().enumerate() {
            let (arg_type, arg_val) = self.parse_typed_value(arg_str);

            // 获取参数的期望类型（从 extern 函数声明中）
            if is_varargs && idx >= varargs_index {
                let cay_type = match &args[idx] {
                    Expr::Cast(cast) => Some(cast.target_type.clone()),
                    expr => self.get_expression_type(expr),
                };
                processed_args.push(self.promote_c_vararg_arg(
                    &arg_type,
                    &arg_val,
                    cay_type.as_ref(),
                ));
            } else if idx < extern_func.params.len() {
                let param_type = &extern_func.params[idx].param_type;
                let llvm_param_type = self.type_to_llvm(param_type);

                // 进行类型转换
                let converted_arg = self.convert_arg_type(&arg_type, &arg_val, &llvm_param_type);
                processed_args.push(converted_arg);
            } else {
                // 如果参数数量超过声明中的数量，直接传递
                processed_args.push(arg_str.clone());
            }
        }

        // 获取返回类型
        let llvm_ret_type = self.type_to_llvm(&extern_func.return_type);

        // 直接调用 extern 函数（不创建包装函数）
        if llvm_ret_type == "void" {
            if is_varargs {
                // 可变参数函数需要显式类型签名
                let param_types: Vec<String> = extern_func
                    .params
                    .iter()
                    .filter(|p| !p.is_varargs)
                    .map(|p| self.type_to_llvm(&p.param_type))
                    .collect();
                let type_sig = if param_types.is_empty() {
                    "(...)".to_string()
                } else {
                    format!("({}, ...)", param_types.join(", "))
                };
                self.emit_line(&format!(
                    "  call void {} @{}({})",
                    type_sig,
                    llvm_func_name,
                    processed_args.join(", ")
                ));
            } else {
                self.emit_line(&format!(
                    "  call void @{}({})",
                    llvm_func_name,
                    processed_args.join(", ")
                ));
            }
            Ok("void %dummy".to_string())
        } else {
            let temp = self.new_temp();
            if is_varargs {
                // 可变参数函数需要显式类型签名
                let param_types: Vec<String> = extern_func
                    .params
                    .iter()
                    .filter(|p| !p.is_varargs)
                    .map(|p| self.type_to_llvm(&p.param_type))
                    .collect();
                let type_sig = if param_types.is_empty() {
                    format!("{} (...)", llvm_ret_type)
                } else {
                    format!("{} ({}, ...)", llvm_ret_type, param_types.join(", "))
                };
                self.emit_line(&format!(
                    "  {} = call {} @{}({})",
                    temp,
                    type_sig,
                    llvm_func_name,
                    processed_args.join(", ")
                ));
            } else {
                self.emit_line(&format!(
                    "  {} = call {} @{}({})",
                    temp,
                    llvm_ret_type,
                    llvm_func_name,
                    processed_args.join(", ")
                ));
            }
            Ok(format!("{} {}", llvm_ret_type, temp))
        }
    }

    /// 生成 __cay_buffer_to_string 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 i8* (String)
    pub fn generate_buffer_to_string_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.len() != 2 {
            return Err(codegen_error_at(ErrorCodes::CODEGEN_INVALID_OPERATION, 
                loc.clone(),
                "__cay_buffer_to_string requires 2 arguments".to_string(),
            ));
        }

        // 生成参数
        let arg1_result = self.generate_expression(&args[0])?;
        let arg2_result = self.generate_expression(&args[1])?;

        let (arg1_type, arg1_val) = self.parse_typed_value(&arg1_result);
        let (arg2_type, arg2_val) = self.parse_typed_value(&arg2_result);

        // 转换参数类型
        let llvm_arg1 = self.convert_arg_type(&arg1_type, &arg1_val, "i64");
        let llvm_arg2 = self.convert_arg_type(&arg2_type, &arg2_val, "i32");

        // 调用运行时函数
        let temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i8* @__cay_buffer_to_string(i64 {}, i32 {})",
            temp, llvm_arg1, llvm_arg2
        ));

        Ok(format!("i8* {}", temp))
    }
}
