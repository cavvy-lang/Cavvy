//! 函数调用表达式代码生成 - 特殊内置调用
//!
//! 处理 print、read、String.valueOf、Integer.parseInt、函数指针调用等特殊调用。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};

impl IRGenerator {
    /// 生成 __cay_read_ptr 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 i64
    pub fn generate_read_ptr_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "__cay_read_ptr requires 1 argument".to_string(),
            ));
        }

        // 生成参数
        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);

        // 转换参数类型为 i64，并提取值部分
        let llvm_arg_full = self.convert_arg_type(&arg_type, &arg_val, "i64");
        let llvm_arg_val = llvm_arg_full.split_whitespace().last().unwrap_or(&arg_val);

        // 调用运行时函数
        let temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i64 @__cay_read_ptr(i64 {})",
            temp, llvm_arg_val
        ));

        Ok(format!("i64 {}", temp))
    }

    /// 生成 __cay_ptr_to_string 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 i8* (String)
    pub fn generate_ptr_to_string_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "__cay_ptr_to_string requires 1 argument".to_string(),
            ));
        }

        // 生成参数
        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);

        // 转换参数类型为 i64，并提取值部分
        let llvm_arg_full = self.convert_arg_type(&arg_type, &arg_val, "i64");
        let llvm_arg_val = llvm_arg_full.split_whitespace().last().unwrap_or(&arg_val);

        // 调用运行时函数
        let temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i8* @__cay_ptr_to_string(i64 {})",
            temp, llvm_arg_val
        ));

        Ok(format!("i8* {}", temp))
    }

    /// 生成 __cay_write_ptr 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 void
    pub fn generate_write_ptr_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.len() != 2 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "__cay_write_ptr requires 2 arguments".to_string(),
            ));
        }

        // 生成参数
        let arg1_result = self.generate_expression(&args[0])?;
        let arg2_result = self.generate_expression(&args[1])?;

        let (arg1_type, arg1_val) = self.parse_typed_value(&arg1_result);
        let (arg2_type, arg2_val) = self.parse_typed_value(&arg2_result);

        // 转换参数类型，并提取值部分
        let llvm_arg1_full = self.convert_arg_type(&arg1_type, &arg1_val, "i64");
        let llvm_arg1_val = llvm_arg1_full
            .split_whitespace()
            .last()
            .unwrap_or(&arg1_val);
        let llvm_arg2_full = self.convert_arg_type(&arg2_type, &arg2_val, "i64");
        let llvm_arg2_val = llvm_arg2_full
            .split_whitespace()
            .last()
            .unwrap_or(&arg2_val);

        // 调用运行时函数
        self.emit_line(&format!(
            "  call void @__cay_write_ptr(i64 {}, i64 {})",
            llvm_arg1_val, llvm_arg2_val
        ));

        Ok("void %dummy".to_string())
    }

    /// 生成 __cay_write_int 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 void
    pub fn generate_write_int_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.len() != 2 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "__cay_write_int requires 2 arguments".to_string(),
            ));
        }

        // 生成参数
        let arg1_result = self.generate_expression(&args[0])?;
        let arg2_result = self.generate_expression(&args[1])?;

        let (arg1_type, arg1_val) = self.parse_typed_value(&arg1_result);
        let (arg2_type, arg2_val) = self.parse_typed_value(&arg2_result);

        // 转换参数类型，并提取值部分
        let llvm_arg1_full = self.convert_arg_type(&arg1_type, &arg1_val, "i64");
        let llvm_arg1_val = llvm_arg1_full
            .split_whitespace()
            .last()
            .unwrap_or(&arg1_val);
        let llvm_arg2_full = self.convert_arg_type(&arg2_type, &arg2_val, "i32");
        let llvm_arg2_val = llvm_arg2_full
            .split_whitespace()
            .last()
            .unwrap_or(&arg2_val);

        // 调用运行时函数
        self.emit_line(&format!(
            "  call void @__cay_write_int(i64 {}, i32 {})",
            llvm_arg1_val, llvm_arg2_val
        ));

        Ok("void %dummy".to_string())
    }

    /// 生成 __cay_read_int 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 i32
    pub fn generate_cay_read_int_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "__cay_read_int requires 1 argument".to_string(),
            ));
        }

        // 生成参数
        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);

        // 转换参数类型为 i64
        let llvm_arg_full = self.convert_arg_type(&arg_type, &arg_val, "i64");
        let llvm_arg_val = llvm_arg_full.split_whitespace().last().unwrap_or(&arg_val);

        // 生成临时变量存储结果
        let temp = self.new_temp();

        // 调用运行时函数
        self.emit_line(&format!(
            "  {} = call i32 @__cay_read_int(i64 {})",
            temp, llvm_arg_val
        ));

        Ok(format!("i32 {}", temp))
    }

    /// 生成 __cay_array_base 内建函数调用
    /// 返回数组数据指针前 8 字节的分配基址（i64），用于析构时 deallocate。
    pub fn generate_array_base_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "__cay_array_base requires 1 argument".to_string(),
            ));
        }

        // 生成数组表达式
        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);

        // 数组类型都是指针；先统一转成 i8*
        let arr_i8 = if arg_type == "i8*" {
            arg_val.to_string()
        } else if arg_type.ends_with("*") {
            let temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast {} {} to i8*",
                temp, arg_type, arg_val
            ));
            temp
        } else {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!(
                    "__cay_array_base expects an array, got {}",
                    arg_type
                ),
            ));
        };

        // 数据指针 - 8 字节 = 分配基址
        let base_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 -8",
            base_ptr, arr_i8
        )
        );

        // 转成 i64 返回
        let base_i64 = self.new_temp();
        self.emit_line(&format!(
            "  {} = ptrtoint i8* {} to i64",
            base_i64, base_ptr
        )
        );

        Ok(format!("i64 {}", base_i64))
    }

    /// 生成 String.valueOf() 静态方法调用
    /// 支持多种类型：int, long, float, double, bool, char
    pub fn generate_string_valueof_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "String.valueOf() takes exactly 1 argument".to_string(),
            ));
        }

        // 生成参数
        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);

        let temp = self.new_temp();

        // 根据参数类型选择对应的转换函数
        match arg_type.as_str() {
            "i32" => {
                // int -> String
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_int_to_string(i32 {})",
                    temp, arg_val
                ));
            }
            "i64" => {
                // long -> String
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_long_to_string(i64 {})",
                    temp, arg_val
                ));
            }
            "float" => {
                // float -> String
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_float_to_string(float {})",
                    temp, arg_val
                ));
            }
            "double" => {
                // double -> String
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_double_to_string(double {})",
                    temp, arg_val
                ));
            }
            "i1" => {
                // bool -> String
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_bool_to_string(i1 {})",
                    temp, arg_val
                ));
            }
            "i8" => {
                // char -> String
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_char_to_string(i8 {})",
                    temp, arg_val
                ));
            }
            "i8*" => {
                // 可能是装箱的泛型值，尝试推断实际类型并解箱
                // 首先检查参数表达式是否是方法调用（如 Box<T>.get()）
                if let Expr::Call(call_expr) = &args[0] {
                    if let Expr::MemberAccess(member) = &*call_expr.callee {
                        // 尝试推断方法返回的实际类型
                        if let Some(obj_type) = self.get_expression_type(&member.object) {
                            match obj_type {
                                crate::types::Type::Generic(class_name, type_args) => {
                                    // 泛型类型，从方法签名推断返回类型
                                    if let Some(ref registry) = self.type_registry {
                                        if let Some(class_info) = registry.get_class(&class_name) {
                                            // 查找方法信息（methods是HashMap<String, Vec<MethodInfo>>）
                                            if let Some(method_overloads) =
                                                class_info.methods.get(&member.member)
                                            {
                                                // 使用第一个重载（无参方法通常只有一个重载）
                                                if let Some(method_info) = method_overloads.first()
                                                {
                                                    // 根据返回类型推断
                                                    let return_type_sig = match &method_info
                                                        .return_type
                                                    {
                                                        crate::types::Type::GenericParam(
                                                            param_name,
                                                        ) => {
                                                            // 返回类型是泛型参数（如 T 或 V）
                                                            // 找到对应的类型参数索引
                                                            if let Some(idx) = class_info
                                                                .type_params
                                                                .iter()
                                                                .position(|p| &p.name == param_name)
                                                            {
                                                                if let Some(type_arg) =
                                                                    type_args.get(idx)
                                                                {
                                                                    self.type_to_signature(type_arg)
                                                                } else {
                                                                    "o".to_string() // 默认对象类型
                                                                }
                                                            } else {
                                                                "o".to_string()
                                                            }
                                                        }
                                                        other => self.type_to_signature(other),
                                                    };
                                                    return self
                                                        .generate_string_valueof_with_unbox(
                                                            &arg_val,
                                                            &return_type_sig,
                                                            temp,
                                                        );
                                                }
                                            }
                                        }
                                    }
                                    // 无法从方法签名推断，回退到第一个类型参数
                                    if let Some(first_arg) = type_args.first() {
                                        let type_sig = self.type_to_signature(first_arg);
                                        return self.generate_string_valueof_with_unbox(
                                            &arg_val, &type_sig, temp,
                                        );
                                    }
                                }
                                crate::types::Type::Object(class_name) => {
                                    // 检查是否是泛型类实例（如 Box<int>）
                                    if let Some(ref registry) = self.type_registry {
                                        if let Some(class_info) = registry.get_class(&class_name) {
                                            // 获取泛型参数（如 int）
                                            if !class_info.type_params.is_empty() {
                                                // 这是一个泛型类，尝试从类名推断实际类型
                                                // 例如：Box<int> 的类名可能是 "Box<int>" 或 "Box_T_"
                                                // 我们需要找到实际的类型参数
                                                let actual_type =
                                                    self.infer_generic_arg_type(&class_name);
                                                return self.generate_string_valueof_with_unbox(
                                                    &arg_val,
                                                    &actual_type,
                                                    temp,
                                                );
                                            }
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }

                // 无法推断类型，假设是 String -> String (直接返回)
                return Ok(format!("i8* {}", arg_val));
            }
            _ => {
                return Err(codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    loc.clone(),
                    format!("String.valueOf() does not support type: {}", arg_type),
                ));
            }
        }

        Ok(format!("i8* {}", temp))
    }

    /// 辅助函数：从泛型类名推断实际类型参数
    fn infer_generic_arg_type(&self, class_name: &str) -> String {
        // 尝试从类名解析泛型参数，如 "Box<int>" -> "i"
        // 或者从类型注册表查找
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                // 如果有类型参数，返回第一个
                if let Some(first_param) = class_info.type_params.first() {
                    // 根据参数名推断类型签名
                    return match first_param.name.as_str() {
                        "int" => "i".to_string(),
                        "long" => "l".to_string(),
                        "float" => "f".to_string(),
                        "double" => "d".to_string(),
                        "boolean" => "b".to_string(),
                        "char" => "c".to_string(),
                        "String" => "s".to_string(),
                        _ => "o".to_string(), // 默认对象类型
                    };
                }
            }
        }
        "i".to_string() // 默认 int
    }

    /// 辅助函数：生成带解箱的 String.valueOf 调用
    fn generate_string_valueof_with_unbox(
        &mut self,
        arg_val: &str,
        type_sig: &str,
        temp: String,
    ) -> CayResult<String> {
        // 根据类型签名解箱并转换为字符串
        match type_sig {
            "i" => {
                // i8* -> i32 -> String
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, arg_val));
                let trunc_val = self.new_temp();
                self.emit_line(&format!("  {} = trunc i64 {} to i32", trunc_val, int_val));
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_int_to_string(i32 {})",
                    temp, trunc_val
                ));
            }
            "l" => {
                // i8* -> i64 -> String
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, arg_val));
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_long_to_string(i64 {})",
                    temp, int_val
                ));
            }
            "f" => {
                // i8* -> double -> float -> String
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, arg_val));
                let double_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i64 {} to double",
                    double_val, int_val
                ));
                let float_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = fptrunc double {} to float",
                    float_val, double_val
                ));
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_float_to_string(float {})",
                    temp, float_val
                ));
            }
            "d" => {
                // i8* -> double -> String
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, arg_val));
                let double_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i64 {} to double",
                    double_val, int_val
                ));
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_double_to_string(double {})",
                    temp, double_val
                ));
            }
            "b" => {
                // i8* -> i8 -> i1 -> String
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, arg_val));
                let trunc_i8 = self.new_temp();
                self.emit_line(&format!("  {} = trunc i64 {} to i8", trunc_i8, int_val));
                let trunc_i1 = self.new_temp();
                self.emit_line(&format!("  {} = trunc i8 {} to i1", trunc_i1, trunc_i8));
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_bool_to_string(i1 {})",
                    temp, trunc_i1
                ));
            }
            "c" => {
                // i8* -> i8 -> String
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, arg_val));
                let trunc_val = self.new_temp();
                self.emit_line(&format!("  {} = trunc i64 {} to i8", trunc_val, int_val));
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_char_to_string(i8 {})",
                    temp, trunc_val
                ));
            }
            "s" | _ => {
                // String 类型，直接使用
                return Ok(format!("i8* {}", arg_val));
            }
        }

        Ok(format!("i8* {}", temp))
    }

    /// 生成 Integer.parseInt() 静态方法调用
    /// 将 String 转换为 int
    pub fn generate_integer_parseint_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "Integer.parseInt() takes exactly 1 argument".to_string(),
            ));
        }

        // 生成参数（String）
        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);

        // 检查参数类型是否为 String (i8*)
        if arg_type != "i8*" {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("Integer.parseInt() expects String, got {}", arg_type),
            ));
        }

        let temp = self.new_temp();

        // 调用 atoi 函数将字符串转换为整数
        self.emit_line(&format!("  {} = call i32 @atoi(i8* {})", temp, arg_val));

        Ok(format!("i32 {}", temp))
    }

    /// 生成函数指针调用
    ///
    /// # Arguments
    /// * `var_name` - 函数指针变量名
    /// * `args` - 参数表达式列表
    /// * `func_type` - 函数指针类型
    /// * `loc` - 源码位置
    pub fn generate_function_pointer_call(
        &mut self,
        var_name: &str,
        args: &[Expr],
        func_type: &crate::types::Type,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        use crate::types::{FunctionType, Type};

        // 获取函数类型信息
        let (param_types, ret_type) = if let Type::Function(func) = func_type {
            (func.params.clone(), *func.return_type.clone())
        } else {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("Variable '{}' is not a function pointer", var_name),
            ));
        };

        // 检查参数数量
        if args.len() != param_types.len() {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
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

        // 获取函数指针变量
        let llvm_name = self.scope_manager.get_llvm_name(var_name).ok_or_else(|| {
            codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("Undefined function pointer variable: {}", var_name),
            )
        })?;

        // 检查是否是闭包（有捕获变量）- 使用类型中的 is_closure 标志
        let is_closure = if let Type::Function(func) = func_type {
            func.is_closure
        } else {
            false
        };

        let func_ptr_temp;
        if is_closure {
            // 闭包调用：从结构体中解包函数指针和环境指针
            // 结构体: { i8* func_ptr, i8* env_ptr }
            let struct_ptr_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = load i8*, i8** %{}, align 8",
                struct_ptr_temp, llvm_name
            ));

            // 加载函数指针（偏移0）
            let func_ptr_slot = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast i8* {} to i8**",
                func_ptr_slot, struct_ptr_temp
            ));
            func_ptr_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = load i8*, i8** {}, align 8",
                func_ptr_temp, func_ptr_slot
            ));

            // 加载环境指针（偏移8）
            let env_ptr_slot_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr i8, i8* {}, i64 8",
                env_ptr_slot_temp, struct_ptr_temp
            ));
            let env_ptr_slot_cast = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast i8* {} to i8**",
                env_ptr_slot_cast, env_ptr_slot_temp
            ));
            let env_ptr_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = load i8*, i8** {}, align 8",
                env_ptr_temp, env_ptr_slot_cast
            ));

            // 将环境指针作为最后一个参数
            arg_values.push(format!("i8* {}", env_ptr_temp));
        } else {
            // 普通函数指针调用
            let fp_temp = self.new_temp();
            let func_ptr_type = self.type_to_llvm(func_type);
            self.emit_line(&format!(
                "  {} = load {}, {}* %{}, align 8",
                fp_temp, func_ptr_type, func_ptr_type, llvm_name
            ));
            func_ptr_temp = fp_temp;
        }

        // 生成调用
        let llvm_ret_type = self.type_to_llvm(&ret_type);
        if llvm_ret_type == "void" {
            self.emit_line(&format!(
                "  call void {}({})",
                func_ptr_temp,
                arg_values.join(", ")
            ));
            Ok("void %dummy".to_string())
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = call {} {}({})",
                temp,
                llvm_ret_type,
                func_ptr_temp,
                arg_values.join(", ")
            ));
            Ok(format!("{} {}", llvm_ret_type, temp))
        }
    }
}
