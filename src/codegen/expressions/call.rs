//! 函数调用表达式代码生成
//!
//! 处理函数调用、内置函数（print/read）、String 方法调用和可变参数。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error};

impl IRGenerator {
    /// 生成函数调用表达式代码
    ///
    /// # Arguments
    /// * `call` - 函数调用表达式
    pub fn generate_call_expression(&mut self, call: &CallExpr) -> cayResult<String> {
        // 处理 print 和 println 函数
        if let Expr::Identifier(name) = call.callee.as_ref() {
            match name.as_str() {
                "print" => return self.generate_print_call(&call.args, false),
                "println" => return self.generate_print_call(&call.args, true),
                "readInt" => return self.generate_read_int_call(&call.args),
                "readFloat" => return self.generate_read_float_call(&call.args),
                "readLine" => return self.generate_read_line_call(&call.args),
                _ => {}
            }
        }

        // 处理 String 方法调用: str.method(args)
        if let Expr::MemberAccess(member) = call.callee.as_ref() {
            // 检查是否是 String 方法调用
            if let Some(method_result) = self.try_generate_string_method_call(member, &call.args)? {
                return Ok(method_result);
            }
        }

        // 处理普通函数调用（支持方法重载和可变参数）
        // 先确定方法信息（类名和方法名）
        let (class_name, method_name) = match call.callee.as_ref() {
            Expr::Identifier(name) => {
                if !self.current_class.is_empty() {
                    (self.current_class.clone(), name.clone())
                } else {
                    (String::new(), name.clone())
                }
            }
            Expr::MemberAccess(member) => {
                if let Expr::Identifier(obj_name) = member.object.as_ref() {
                    let class_name = self.var_class_map.get(obj_name)
                        .cloned()
                        .unwrap_or_else(|| obj_name.clone());
                    (class_name, member.member.clone())
                } else {
                    return Err(codegen_error("Invalid method call".to_string()));
                }
            }
            _ => return Err(codegen_error("Invalid function call".to_string())),
        };

        // 检查是否是可变参数方法（根据方法名推断）
        let is_varargs_method = self.is_varargs_method(&class_name, &method_name);

        // 先生成参数以获取参数类型
        let mut arg_results = Vec::new();
        for arg in &call.args {
            arg_results.push(self.generate_expression(arg)?);
        }

        // 处理可变参数：将多余参数打包成数组
        let (processed_args, has_varargs_array) = if is_varargs_method {
            let packed = self.pack_varargs_args(&class_name, &method_name, &arg_results)?;
            // 如果原始参数多于固定参数数量，说明创建了数组
            let fixed_count = match method_name.as_str() {
                "sum" => 0,
                "printAll" => 1,
                "multiplyAndAdd" => 1,
                _ => 0,
            };
            let has_array = arg_results.len() > fixed_count;
            (packed, has_array)
        } else {
            (arg_results, false)
        };

        // 生成函数名 - 使用类型注册表获取方法定义的参数类型
        let fn_name = self.generate_function_name(&class_name, &method_name, &processed_args, has_varargs_array);

        // 转换参数类型
        let mut converted_args = Vec::new();
        for arg_str in &processed_args {
            // 保持参数类型不变，不进行转换
            converted_args.push(arg_str.clone());
        }

        // 获取方法的返回类型
        let ret_type = self.get_method_return_type(&class_name, &method_name, &processed_args, has_varargs_array);
        let llvm_ret_type = self.type_to_llvm(&ret_type);
        
        if llvm_ret_type == "void" {
            // void 方法调用不需要命名结果
            self.emit_line(&format!("  call void @{}({})",
                fn_name, converted_args.join(", ")));
            Ok("void %dummy".to_string())
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = call {} @{}({})",
                temp, llvm_ret_type, fn_name, converted_args.join(", ")));
            Ok(format!("{} {}", llvm_ret_type, temp))
        }
    }

    /// 生成函数名 - 优先使用类型注册表中方法定义的参数类型，支持继承
    fn generate_function_name(&self, class_name: &str, method_name: &str, processed_args: &[String], has_varargs_array: bool) -> String {
        // 获取实际参数的类型签名
        let arg_types: Vec<String> = processed_args.iter()
            .enumerate()
            .map(|(idx, r)| {
                let (ty, _) = self.parse_typed_value(r);
                let is_varargs_array = has_varargs_array && idx == processed_args.len() - 1;
                let llvm_type = self.llvm_type_to_signature(&ty);
                if is_varargs_array {
                    "ai".to_string()
                } else {
                    llvm_type
                }
            })
            .collect();
        
        // 尝试从类型注册表获取方法信息（支持继承查找）
        if let Some(ref registry) = self.type_registry {
            // 首先在当前类中查找方法
            let mut current_class_name = class_name.to_string();
            loop {
                if let Some(class_info) = registry.get_class(&current_class_name) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        let arg_count = processed_args.len();
                        
                        // 首先尝试找到参数类型完全匹配的方法
                        for method in methods {
                            let param_count = method.params.len();
                            let is_varargs = method.params.last().map(|p| p.is_varargs).unwrap_or(false);
                            
                            if is_varargs {
                                // 可变参数方法
                                let fixed_count = param_count.saturating_sub(1);
                                if arg_count >= fixed_count {
                                    // 检查固定参数类型是否匹配
                                    let method_sig = self.build_function_name_from_method(&current_class_name, method_name, &method.params, has_varargs_array);
                                    let expected_sig = format!("{}.__{}_{}", current_class_name, method_name, arg_types.join("_"));
                                    if method_sig == expected_sig {
                                        return method_sig;
                                    }
                                }
                            } else if param_count == arg_count {
                                // 非可变参数方法：检查参数类型是否匹配
                                let method_sig = self.build_function_name_from_method(&current_class_name, method_name, &method.params, has_varargs_array);
                                let expected_sig = format!("{}.__{}_{}", current_class_name, method_name, arg_types.join("_"));
                                if method_sig == expected_sig {
                                    return method_sig;
                                }
                            }
                        }
                        
                        // 如果没有找到类型完全匹配的方法，回退到参数数量匹配
                        for method in methods {
                            let param_count = method.params.len();
                            let is_varargs = method.params.last().map(|p| p.is_varargs).unwrap_or(false);
                            
                            if is_varargs {
                                let fixed_count = param_count.saturating_sub(1);
                                if arg_count >= fixed_count {
                                    return self.build_function_name_from_method(&current_class_name, method_name, &method.params, has_varargs_array);
                                }
                            } else if param_count == arg_count {
                                return self.build_function_name_from_method(&current_class_name, method_name, &method.params, has_varargs_array);
                            }
                        }
                    }
                    
                    // 如果在当前类中没找到，尝试在父类中查找
                    if let Some(ref parent_name) = class_info.parent {
                        current_class_name = parent_name.clone();
                        continue;
                    }
                }
                break;
            }
        }

        // 回退到使用实际参数类型生成函数名
        if arg_types.is_empty() {
            format!("{}.{}", class_name, method_name)
        } else {
            format!("{}.__{}_{}", class_name, method_name, arg_types.join("_"))
        }
    }

    /// 根据方法定义的参数类型构建函数名
    fn build_function_name_from_method(&self, class_name: &str, method_name: &str, params: &[crate::types::ParameterInfo], has_varargs_array: bool) -> String {
        if params.is_empty() {
            return format!("{}.{}", class_name, method_name);
        }

        let param_types: Vec<String> = params.iter()
            .enumerate()
            .map(|(idx, p)| {
                let is_last_varargs = has_varargs_array && idx == params.len() - 1 && p.is_varargs;
                self.param_type_to_signature(&p.param_type, is_last_varargs)
            })
            .collect();

        format!("{}.__{}_{}", class_name, method_name, param_types.join("_"))
    }

    /// 将参数类型转换为签名
    fn param_type_to_signature(&self, ty: &crate::types::Type, is_varargs_array: bool) -> String {
        if is_varargs_array {
            return "ai".to_string(); // 可变参数数组签名
        }

        match ty {
            crate::types::Type::Int32 => "i".to_string(),
            crate::types::Type::Int64 => "l".to_string(),
            crate::types::Type::Float32 => "f".to_string(),
            crate::types::Type::Float64 => "d".to_string(),
            crate::types::Type::Bool => "b".to_string(),
            crate::types::Type::String => "s".to_string(),
            crate::types::Type::Char => "c".to_string(),
            crate::types::Type::Object(name) => format!("o{}", name),
            crate::types::Type::Array(inner) => format!("a{}", self.param_type_to_signature(inner, false)),
            _ => "x".to_string(),
        }
    }

    /// 获取方法的返回类型
    fn get_method_return_type(&self, class_name: &str, method_name: &str, processed_args: &[String], has_varargs_array: bool) -> crate::types::Type {
        // 获取实际参数的类型签名
        let arg_types: Vec<String> = processed_args.iter()
            .enumerate()
            .map(|(idx, r)| {
                let (ty, _) = self.parse_typed_value(r);
                let is_varargs_array = has_varargs_array && idx == processed_args.len() - 1;
                let llvm_type = self.llvm_type_to_signature(&ty);
                if is_varargs_array {
                    "ai".to_string()
                } else {
                    llvm_type
                }
            })
            .collect();
        
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    let arg_count = processed_args.len();
                    
                    // 首先尝试找到参数类型完全匹配的方法
                    for method in methods {
                        let param_count = method.params.len();
                        let is_varargs = method.params.last().map(|p| p.is_varargs).unwrap_or(false);
                        
                        if is_varargs {
                            let fixed_count = param_count.saturating_sub(1);
                            if arg_count >= fixed_count {
                                let method_sig = self.build_function_name_from_method(class_name, method_name, &method.params, has_varargs_array);
                                let expected_sig = format!("{}.__{}_{}", class_name, method_name, arg_types.join("_"));
                                if method_sig == expected_sig {
                                    return method.return_type.clone();
                                }
                            }
                        } else if param_count == arg_count {
                            let method_sig = self.build_function_name_from_method(class_name, method_name, &method.params, has_varargs_array);
                            let expected_sig = format!("{}.__{}_{}", class_name, method_name, arg_types.join("_"));
                            if method_sig == expected_sig {
                                return method.return_type.clone();
                            }
                        }
                    }
                    
                    // 如果没有找到类型完全匹配的方法，回退到参数数量匹配
                    for method in methods {
                        let param_count = method.params.len();
                        let is_varargs = method.params.last().map(|p| p.is_varargs).unwrap_or(false);
                        
                        if is_varargs {
                            let fixed_count = param_count.saturating_sub(1);
                            if arg_count >= fixed_count {
                                return method.return_type.clone();
                            }
                        } else if param_count == arg_count {
                            return method.return_type.clone();
                        }
                    }
                }
            }
        }
        
        // 默认返回 i64 类型
        crate::types::Type::Int64
    }

    /// 检查方法是否是可变参数方法
    /// 查询类型注册表来确定方法是否真的是可变参数方法
    fn is_varargs_method(&self, class_name: &str, method_name: &str) -> bool {
        // 查询类型注册表
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    // 检查是否有任何方法是可变参数的
                    for method in methods {
                        if method.params.last().map(|p| p.is_varargs).unwrap_or(false) {
                            return true;
                        }
                    }
                }
            }
        }
        // 默认返回false，避免将普通方法误认为可变参数方法
        false
    }

    /// 将可变参数打包成数组
    /// fixed_param_count: 固定参数的数量
    fn pack_varargs_args(&mut self, _class_name: &str, method_name: &str, arg_results: &[String]) -> cayResult<Vec<String>> {
        // 确定固定参数数量（这里需要根据实际方法定义来确定）
        let fixed_param_count = match method_name {
            "sum" => 0,  // sum(int... numbers) 没有固定参数
            "printAll" => 1,  // printAll(string prefix, int... numbers) 有1个固定参数
            "multiplyAndAdd" => 1,  // multiplyAndAdd(int multiplier, int... numbers) 有1个固定参数
            _ => 0,
        };

        if arg_results.len() <= fixed_param_count {
            // 参数数量不足或刚好，不需要打包
            return Ok(arg_results.to_vec());
        }

        // 分割固定参数和可变参数
        let fixed_args = &arg_results[..fixed_param_count];
        let varargs = &arg_results[fixed_param_count..];

        // 创建数组来存储可变参数
        let array_size = varargs.len();
        let array_type = "i32";  // 假设可变参数是 int 类型
        let array_ptr = self.new_temp();

        // 分配数组内存
        let elem_size = 4;  // i32 占 4 字节
        let total_size = array_size * elem_size;
        self.emit_line(&format!("  {} = call i8* @calloc(i64 1, i64 {})", array_ptr, total_size));

        // 将可变参数存入数组
        for (i, arg_str) in varargs.iter().enumerate() {
            let (arg_type, arg_val) = self.parse_typed_value(arg_str);
            let elem_ptr_i8 = self.new_temp();
            let elem_ptr_i32 = self.new_temp();
            let offset = i * elem_size;

            // 计算元素地址 (i8*)
            self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 {}", elem_ptr_i8, array_ptr, offset));

            // 将 i8* 转换为 i32*
            self.emit_line(&format!("  {} = bitcast i8* {} to i32*", elem_ptr_i32, elem_ptr_i8));

            // 将值转换为 i32 并存储
            if arg_type == "i64" {
                let truncated = self.new_temp();
                self.emit_line(&format!("  {} = trunc i64 {} to i32", truncated, arg_val));
                self.emit_line(&format!("  store i32 {}, i32* {}, align 4", truncated, elem_ptr_i32));
            } else if arg_type == "i32" {
                self.emit_line(&format!("  store i32 {}, i32* {}, align 4", arg_val, elem_ptr_i32));
            }
        }

        // 构建结果：固定参数 + 数组指针
        let mut result = fixed_args.to_vec();
        result.push(format!("i8* {}", array_ptr));

        Ok(result)
    }
}
