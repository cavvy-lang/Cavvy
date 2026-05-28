//! 函数调用表达式代码生成
//!
//! 处理函数调用、内置函数（print/read）、String 方法调用和可变参数。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error_at};
use crate::semantic::resolve_call_args;

impl IRGenerator {
    /// 生成函数调用表达式代码
    ///
    /// # Arguments
    /// * `call` - 函数调用表达式
    pub fn generate_call_expression(&mut self, call: &CallExpr) -> cayResult<String> {
        // 处理 print 和 println 函数
        if let Expr::Identifier(name) = call.callee.as_ref() {
            match name.as_str() {
                "print" => return self.generate_print_call(&call.args, false, &call.loc),
                "println" => return self.generate_print_call(&call.args, true, &call.loc),
                "readInt" => return self.generate_read_int_call(&call.args, &call.loc),
                "readLong" => return self.generate_read_long_call(&call.args, &call.loc),
                "readFloat" => return self.generate_read_float_call(&call.args, &call.loc),
                "readDouble" => return self.generate_read_double_call(&call.args, &call.loc),
                "readLine" => return self.generate_read_line_call(&call.args, &call.loc),
                "readChar" => return self.generate_read_char_call(&call.args, &call.loc),
                // 运行时辅助函数
                "__cay_read_ptr" => return self.generate_read_ptr_call(&call.args, &call.loc),
                "__cay_ptr_to_string" => return self.generate_ptr_to_string_call(&call.args, &call.loc),
                "__cay_write_ptr" => return self.generate_write_ptr_call(&call.args, &call.loc),
                "__cay_write_int" => return self.generate_write_int_call(&call.args, &call.loc),
                "__cay_read_int" => return self.generate_cay_read_int_call(&call.args, &call.loc),
                _ => {}
            }
        }

        // 处理 String 方法调用: str.method(args)
        if let Expr::MemberAccess(member) = call.callee.as_ref() {
            // 检查是否是 String 方法调用
            if let Some(method_result) = self.try_generate_string_method_call(member, &call.args)? {
                return Ok(method_result);
            }

            // 处理数组的 length() 方法调用（作为 length 属性的语法糖）
            if member.member == "length" && call.args.is_empty() {
                // 检查对象是否是数组类型
                if let Some(var_type) = self.get_expression_type(&member.object) {
                    if matches!(var_type, crate::types::Type::Array(_)) {
                        // 将 length() 转换为 length 属性访问
                        return self.generate_array_length_access(&member.object);
                    }
                }
            }

            // 处理 String.valueOf() 静态方法
            if let Expr::Identifier(class_name) = member.object.as_ref() {
                if class_name == "String" && member.member == "valueOf" {
                    return self.generate_string_valueof_call(&call.args, &call.loc);
                }
            }

            // 处理 Integer.parseInt() 静态方法
            if let Expr::Identifier(class_name) = member.object.as_ref() {
                if class_name == "Integer" && member.member == "parseInt" {
                    return self.generate_integer_parseint_call(&call.args, &call.loc);
                }
            }
        }

        // 处理 extern 函数调用
        if let Expr::Identifier(name) = call.callee.as_ref() {
            let func_name = name.as_ref();
            if self.is_extern_function(func_name) {
                return self.generate_extern_function_call(func_name, &call.args, &call.loc);
            }
        }

        // 处理普通函数调用（支持方法重载和可变参数）
        // 先确定方法信息（类名和方法名）
        // 对于实例方法调用，还需要保存对象表达式以获取 this 指针
        // is_static_call 表示是否是类名.方法名() 形式的静态方法调用
        let (class_name, method_name, obj_expr, is_static_call) = match call.callee.as_ref() {
            Expr::Identifier(name) => {
                let name_str = name.as_ref();
                // 检查是否是全局 extern 函数
                if let Some(_extern_func) = self.get_extern_function(name_str) {
                    return self.generate_extern_function_call(name_str, &call.args, &call.loc);
                }
                // 检查是否是函数指针变量
                if let Some(var_type) = self.get_variable_type(name_str) {
                    if matches!(var_type, crate::types::Type::Function(_)) {
                        return self.generate_function_pointer_call(name_str, &call.args, &var_type, &call.loc);
                    }
                }
                // 检查是否是 @FreeFunction 导出的自由函数
                let free_fn_info = self.type_registry.as_ref()
                    .and_then(|r| r.free_functions.get(name_str))
                    .map(|(class_name, method_info, _)| (class_name.clone(), method_info.name.clone()));
                if let Some((owner_class, method_name)) = free_fn_info {
                    // 将 @FreeFunction 调用转为对应类的静态方法调用
                    // 使用注册时的方法名（而非调用时的限定名）
                    (owner_class, method_name, None, true)
                } else if self.is_top_level_function(name_str) {
                    // 顶层函数没有类名前缀
                    (String::new(), name_str.to_string(), None, false)
                } else if !self.current_class.is_empty() {
                    (self.current_class.clone(), name_str.to_string(), None, false)
                } else {
                    (String::new(), name_str.to_string(), None, false)
                }
            }
            Expr::MemberAccess(member) => {
                // 检查 object 是否是标识符（类名或变量名）
                match member.object.as_ref() {
                    Expr::Identifier(obj_name) => {
                        let obj_name_str = obj_name.as_ref();
                        // 特殊处理 super 标识符
                        if obj_name_str == "super" {
                            // super.methodName() 调用父类的方法
                            let parent_class = self.get_parent_class(&self.current_class)
                                .unwrap_or_else(|| self.current_class.clone());
                            (parent_class, member.member.clone(), Some(member.object.clone()), false)
                        } else if obj_name_str == "this" {
                            // this.methodName() - 首先检查是否是函数指针字段
                            if let Some(field_type) = self.get_field_type(&self.current_class, &member.member) {
                                if matches!(field_type, crate::types::Type::Function(_)) {
                                    // 是函数指针字段调用
                                    return self.generate_member_func_ptr_call(member, &call.args, &field_type, &call.loc);
                                }
                            }
                            // 不是函数指针字段，按普通方法处理
                            (self.current_class.clone(), member.member.clone(), Some(member.object.clone()), false)
                        } else {
                            // 检查是否是 enum 构造函数调用: EnumName.VariantName(args)
                            if let Some(ref registry) = self.type_registry {
                                if let Some(enum_info) = registry.get_enum(obj_name_str) {
                                    if let Some(idx) = enum_info.variants.iter().position(|v| v.name == member.member) {
                                        let has_payload = enum_info.variants[idx].payload_type.is_some();
                                        let payload_val = if has_payload {
                                            let val = self.generate_expression(&call.args[0])?;
                                            let (pl_type, pl_val) = self.parse_typed_value(&val);
                                            if pl_type == "i32" {
                                                let ext = self.new_temp();
                                                self.emit_line(&format!("  {} = sext i32 {} to i64", ext, pl_val));
                                                ext
                                            } else if pl_type == "i8*" || pl_type.ends_with('*') {
                                                let ptr_to_i64 = self.new_temp();
                                                self.emit_line(&format!("  {} = ptrtoint {} {} to i64", ptr_to_i64, pl_type, pl_val));
                                                ptr_to_i64
                                            } else {
                                                pl_val.to_string()
                                            }
                                        } else {
                                            "0".to_string()
                                        };
                                        // 构造 struct { i32 discriminant, i64 payload }
                                        let struct_val = self.new_temp();
                                        self.emit_line(&format!("  {} = insertvalue {{ i32, i64 }} undef, i32 {}, 0", struct_val, idx));
                                        let struct_val2 = self.new_temp();
                                        self.emit_line(&format!("  {} = insertvalue {{ i32, i64 }} {}, i64 {}, 1", struct_val2, struct_val, payload_val));
                                        return Ok(format!("{{ i32, i64 }} {}", struct_val2));
                                    }
                                }
                            }
                            // 首先检查是否是已知的类名
                            let (class_name, is_class) = if let Some(ref registry) = self.type_registry {
                                if registry.class_exists(obj_name_str) {
                                    (obj_name_str.to_string(), true)
                                } else {
                                    // 不是类名，尝试从变量映射获取
                                    let result = self.var_class_map.get(obj_name_str)
                                        .cloned()
                                        .unwrap_or_else(|| obj_name_str.to_string());
                                    (result, false)
                                }
                            } else {
                                let result = self.var_class_map.get(obj_name_str)
                                    .cloned()
                                    .unwrap_or_else(|| obj_name_str.to_string());
                                (result, false)
                            };
                            // 如果是类名.方法名() 形式，标记为静态方法调用
                            (class_name, member.member.clone(), Some(member.object.clone()), is_class)
                        }
                    }
                    _ => {
                        // object 不是标识符，可能是其他表达式（如 new 表达式）
                        // 尝试从表达式推断类型
                        if let Some(obj_type) = self.get_expression_type(&member.object) {
                            match obj_type {
                                crate::types::Type::Object(class_name) => {
                                    // 首先检查是否是函数指针字段
                                    if let Some(field_type) = self.get_field_type(&class_name, &member.member) {
                                        if matches!(field_type, crate::types::Type::Function(_)) {
                                            // 是函数指针字段调用，生成函数指针调用代码
                                            return self.generate_member_func_ptr_call(member, &call.args, &field_type, &call.loc);
                                        }
                                    }
                                    // 不是函数指针字段，按普通方法处理
                                    (class_name, member.member.clone(), Some(member.object.clone()), false)
                                }
                                _ => {
                                    return Err(codegen_error_at(member.loc.clone(), format!(
                                        "Cannot call method '{}' on non-class type",
                                        member.member
                                    )));
                                }
                            }
                        } else {
                            return Err(codegen_error_at(member.loc.clone(), format!(
                                "Cannot determine type for method call '{}'",
                                member.member
                            )));
                        }
                    }
                }
            }
            _ => return Err(codegen_error_at(call.loc.clone(), "Invalid function call".to_string())),
        };

        // 检查是否有命名参数需要重排
        let has_named_args = call.args.iter().any(|a| matches!(a, Expr::NamedArg(_)));
        let resolved_args: Vec<Expr>;
        let actual_args: &[Expr] = if has_named_args {
            // 获取方法形参以进行重排
            let params = self.get_method_params(&class_name, &method_name)
                .ok_or_else(|| codegen_error_at(call.loc.clone(), 
                    format!("Cannot resolve parameters for '{}' to process named arguments", method_name)))?;
            let resolved = resolve_call_args(&call.args, &params)
                .map_err(|msg| codegen_error_at(call.loc.clone(), msg))?;
            resolved_args = resolved.args;
            &resolved_args
        } else {
            &call.args
        };

        // 检查是否是可变参数方法
        let is_varargs_method = self.is_varargs_method(&class_name, &method_name);

        // 生成参数表达式
        let mut arg_results = Vec::new();
        for arg in actual_args {
            arg_results.push(self.generate_expression(arg)?);
        }

        // 处理可变参数：将多余参数打包成数组
        let (processed_args, has_varargs_array) = if is_varargs_method {
            let packed = self.pack_varargs_args(&class_name, &method_name, &arg_results)?;
            // 如果原始参数多于固定参数数量，说明创建了数组
            let (fixed_count, _) = self.get_varargs_info(&class_name, &method_name);
            let has_array = arg_results.len() > fixed_count;
            (packed, has_array)
        } else {
            (arg_results, false)
        };

        // 检查是否是实例方法（需要传递 this）
        // 如果是 Class.method() 形式的静态方法调用，即使存在同名实例方法，也不传递 this
        let is_instance_method = if is_static_call {
            false
        } else {
            self.is_instance_method(&class_name, &method_name)
        };
        
        // 为实例方法添加 this 参数
        let mut final_args = Vec::new();
        
        if is_instance_method {
            // 获取 this 指针
            if let Some(obj) = &obj_expr {
                // 检查是否是 super 标识符
                if let Expr::Identifier(name) = obj.as_ref() {
                    if name.as_ref() == "super" {
                        // super.methodName() 使用 this 指针
                        if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                            let this_temp = self.new_temp();
                            self.emit_line(&format!("  {} = load i8*, i8** %{}, align 8",
                                this_temp, this_llvm_name));
                            final_args.push(format!("i8* {}", this_temp));
                        } else {
                            final_args.push("i8* null".to_string());
                        }
                    } else {
                        // 通过对象表达式获取 this 指针（如 obj1.getId()）
                        let obj_result = self.generate_expression(obj)?;
                        let (_, obj_val) = self.parse_typed_value(&obj_result);
                        final_args.push(format!("i8* {}", obj_val));
                    }
                } else {
                    // 通过对象表达式获取 this 指针（如 obj1.getId()）
                    let obj_result = self.generate_expression(obj)?;
                    let (_, obj_val) = self.parse_typed_value(&obj_result);
                    final_args.push(format!("i8* {}", obj_val));
                }
            } else if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                // 通过当前方法的 this 获取（如在实例方法中调用其他实例方法）
                let this_temp = self.new_temp();
                self.emit_line(&format!("  {} = load i8*, i8** %{}, align 8",
                    this_temp, this_llvm_name));
                final_args.push(format!("i8* {}", this_temp));
            } else {
                // 在静态方法中调用实例方法且没有对象表达式，使用 null 作为 this
                final_args.push("i8* null".to_string());
            }
        }
        
        // 获取方法的参数类型信息以进行必要的类型转换
        let param_types = self.get_method_param_types(&class_name, &method_name, &processed_args, has_varargs_array);
        
        // 添加其他参数（根据需要进行类型转换）
        for (idx, arg_str) in processed_args.iter().enumerate() {
            let (arg_type, arg_val) = self.parse_typed_value(arg_str);
            
            // 检查是否需要类型转换
            if idx < param_types.len() {
                let param_llvm_type = self.type_to_llvm(&param_types[idx]);
                let converted_arg = self.convert_arg_type(&arg_type, &arg_val, &param_llvm_type);
                final_args.push(converted_arg);
            } else {
                final_args.push(arg_str.clone());
            }
        }

        // 生成函数名 - 使用类型注册表获取方法定义的参数类型
        // 注意：函数名不包含 this 参数，this 只在 IR 调用时传递
        let fn_name = self.generate_function_name(&class_name, &method_name, &processed_args, has_varargs_array);

        // 获取方法的返回类型
        let ret_type = self.get_method_return_type(&class_name, &method_name, &processed_args, has_varargs_array);
        let llvm_ret_type = self.type_to_llvm(&ret_type);
        
        if llvm_ret_type == "void" {
            // void 方法调用不需要命名结果
            self.emit_line(&format!("  call void @{}({})",
                fn_name, final_args.join(", ")));
            Ok("void %dummy".to_string())
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = call {} @{}({})",
                temp, llvm_ret_type, fn_name, final_args.join(", ")));
            Ok(format!("{} {}", llvm_ret_type, temp))
        }
    }

    /// 生成函数名 - 优先使用类型注册表中方法定义的参数类型，支持继承
    fn generate_function_name(&self, class_name: &str, method_name: &str, processed_args: &[String], has_varargs_array: bool) -> String {
        let llvm_class = self.get_qualified_class_name(class_name);
        // 特殊处理运行时 native 方法：直接返回运行时函数名
        if method_name == "__cay_buffer_to_string" {
            return "__cay_buffer_to_string".to_string();
        }

        // 获取实际参数的类型签名
        // 找到可变参数在形参列表中的位置（用于正确标记数组参数）
        let varargs_param_index = self.get_varargs_index(class_name, method_name);
        let arg_types: Vec<String> = processed_args.iter()
            .enumerate()
            .map(|(idx, r)| {
                let (ty, _) = self.parse_typed_value(r);
                let is_varargs_array = has_varargs_array && Some(idx) == varargs_param_index;
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
            let llvm_current = self.get_qualified_class_name(&current_class_name);
            loop {
                if let Some(class_info) = registry.get_class(&current_class_name) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        let arg_count = processed_args.len();
                        
                        // 首先尝试找到参数类型完全匹配的方法
                        for method in methods {
                            let param_count = method.params.len();
                            let is_varargs = method.params.iter().any(|p| p.is_varargs);
                            let fixed_count = method.params.iter().position(|p| p.is_varargs).unwrap_or(param_count);
                            
                            if is_varargs {
                                // 可变参数方法
                                if arg_count >= fixed_count {
                                    // 检查固定参数类型是否匹配
                                    let method_sig = self.build_function_name_from_method(&current_class_name, method_name, &method.params, has_varargs_array);
                                    let expected_sig = format!("{}.__{}_{}", llvm_current, method_name, arg_types.join("_"));
                                    if method_sig == expected_sig {
                                        return method_sig;
                                    }
                                }
                            } else if param_count == arg_count {
                                // 非可变参数方法：检查参数类型是否匹配
                                let method_sig = self.build_function_name_from_method(&current_class_name, method_name, &method.params, has_varargs_array);
                                let expected_sig = format!("{}.__{}_{}", llvm_current, method_name, arg_types.join("_"));
                                if method_sig == expected_sig {
                                    return method_sig;
                                }
                            }
                        }
                        
                        // 如果没有找到类型完全匹配的方法，回退到参数数量匹配
                        for method in methods {
                            let param_count = method.params.len();
                            let is_varargs = method.params.iter().any(|p| p.is_varargs);
                            let fixed_count = method.params.iter().position(|p| p.is_varargs).unwrap_or(param_count);
                            
                            if is_varargs {
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
        // 顶层函数（class_name 为空）使用 __toplevel_ 前缀
        if class_name.is_empty() {
            // 顶层函数命名：__toplevel_func_name
            format!("__toplevel_{}", method_name)
        } else if arg_types.is_empty() {
            format!("{}.{}", llvm_class, method_name)
        } else {
            format!("{}.__{}_{}", llvm_class, method_name, arg_types.join("_"))
        }
    }

    /// 根据方法定义的参数类型构建函数名
    /// 从方法信息构建函数名
    /// 
    /// # Arguments
    /// * `class_name` - 类名
    /// * `method_name` - 方法名
    /// * `params` - 参数信息列表
    /// * `has_varargs_array` - 是否有可变参数数组
    pub fn build_function_name_from_method(&self, class_name: &str, method_name: &str, params: &[crate::types::ParameterInfo], has_varargs_array: bool) -> String {
        let llvm_cls = self.get_qualified_class_name(class_name);
        if params.is_empty() {
            return format!("{}.{}", llvm_cls, method_name);
        }

        let param_types: Vec<String> = params.iter()
            .map(|p| {
                let is_param_varargs = has_varargs_array && p.is_varargs;
                let resolved_type = self.resolve_type(&p.param_type);
                self.param_type_to_signature(&resolved_type, is_param_varargs)
            })
            .collect();

        format!("{}.__{}_{}", llvm_cls, method_name, param_types.join("_"))
    }

    /// 将参数类型转换为签名
    fn param_type_to_signature(&self, ty: &crate::types::Type, is_varargs_array: bool) -> String {
        if is_varargs_array {
            // 可变参数数组：提取元素类型并生成签名
            return self.varargs_element_type_to_signature(ty);
        }

        match ty {
            crate::types::Type::Void => "v".to_string(),
            crate::types::Type::Int32 => "i".to_string(),
            crate::types::Type::Int64 => "l".to_string(),
            crate::types::Type::Float32 => "f".to_string(),
            crate::types::Type::Float64 => "d".to_string(),
            crate::types::Type::Bool => "b".to_string(),
            crate::types::Type::String => "s".to_string(),
            crate::types::Type::Char => "c".to_string(),
            crate::types::Type::Object(name) => format!("o{}", name),
            crate::types::Type::Array(inner) => format!("a{}", self.param_type_to_signature(inner, false)),
            // FFI 类型
            crate::types::Type::CInt => "ci".to_string(),
            crate::types::Type::CUInt => "cu".to_string(),
            crate::types::Type::CLong => "cl".to_string(),
            crate::types::Type::CULong => "cul".to_string(),
            crate::types::Type::CShort => "cs".to_string(),
            crate::types::Type::CUShort => "cus".to_string(),
            crate::types::Type::CChar => "cc".to_string(),
            crate::types::Type::CUChar => "cuc".to_string(),
            crate::types::Type::CFloat => "cf".to_string(),
            crate::types::Type::CDouble => "cd".to_string(),
            crate::types::Type::SizeT => "sz".to_string(),
            crate::types::Type::SSizeT => "ssz".to_string(),
            crate::types::Type::UIntPtr => "uptr".to_string(),
            crate::types::Type::IntPtr => "iptr".to_string(),
            crate::types::Type::CVoid => "cv".to_string(),
            crate::types::Type::CBool => "cb".to_string(),
            crate::types::Type::Pointer(inner) => format!("p{}", self.param_type_to_signature(inner, false)),
            // 函数指针类型
            crate::types::Type::Function(func_type) => {
                // 生成函数指针签名: fn_<return>_<param1>_<param2>_...
                let mut sig = "fn".to_string();
                sig.push_str(&self.param_type_to_signature(&func_type.return_type, false));
                for param in &func_type.params {
                    sig.push_str("_");
                    sig.push_str(&self.param_type_to_signature(param, false));
                }
                sig
            }
            _ => "x".to_string(),
        }
    }

    /// 将可变参数数组的元素类型转换为签名
    /// 可变参数类型是 Array(ElementType)，需要提取元素类型
    fn varargs_element_type_to_signature(&self, ty: &crate::types::Type) -> String {
        use crate::types::Type;
        match ty {
            Type::Array(elem) => {
                match elem.as_ref() {
                    Type::Int32 => "ai".to_string(),
                    Type::Int64 => "al".to_string(),
                    Type::Float32 => "af".to_string(),
                    Type::Float64 => "ad".to_string(),
                    Type::Bool => "ab".to_string(),
                    Type::String => "as".to_string(),
                    Type::Char => "ac".to_string(),
                    Type::Object(name) => format!("ao{}", name),
                    _ => "ax".to_string(),
                }
            }
            _ => self.param_type_to_signature(ty, false), // 如果不是数组类型，回退到普通签名
        }
    }

    /// 获取方法的返回类型
    fn get_method_return_type(&self, class_name: &str, method_name: &str, processed_args: &[String], has_varargs_array: bool) -> crate::types::Type {
        // 获取实际参数的类型签名
        let varargs_param_index = self.get_varargs_index(class_name, method_name);
        let arg_types: Vec<String> = processed_args.iter()
            .enumerate()
            .map(|(idx, r)| {
                let (ty, _) = self.parse_typed_value(r);
                let is_varargs_array = has_varargs_array && Some(idx) == varargs_param_index;
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
                    let llvm_class = self.get_qualified_class_name(class_name);
                    
                    // 首先尝试找到参数类型完全匹配的方法
                    for method in methods {
                        let param_count = method.params.len();
                        let is_varargs = method.params.iter().any(|p| p.is_varargs);
                        let fixed_count = method.params.iter().position(|p| p.is_varargs).unwrap_or(param_count);
                        
                        if is_varargs {
                            if arg_count >= fixed_count {
                                let method_sig = self.build_function_name_from_method(class_name, method_name, &method.params, has_varargs_array);
                                let expected_sig = format!("{}.__{}_{}", llvm_class, method_name, arg_types.join("_"));
                                if method_sig == expected_sig {
                                    return method.return_type.clone();
                                }
                            }
                        } else if param_count == arg_count {
                            let method_sig = self.build_function_name_from_method(class_name, method_name, &method.params, has_varargs_array);
                            let expected_sig = format!("{}.__{}_{}", llvm_class, method_name, arg_types.join("_"));
                            if method_sig == expected_sig {
                                return method.return_type.clone();
                            }
                        }
                    }
                    
                    // 如果没有找到类型完全匹配的方法，回退到参数数量匹配
                    for method in methods {
                        let param_count = method.params.len();
                        let is_varargs = method.params.iter().any(|p| p.is_varargs);
                        let fixed_count = method.params.iter().position(|p| p.is_varargs).unwrap_or(param_count);
                        
                        if is_varargs {
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

    /// 获取方法的形参列表
    fn get_method_params(&self, class_name: &str, method_name: &str) -> Option<Vec<crate::types::ParameterInfo>> {
        if let Some(ref registry) = self.type_registry {
            let mut current = class_name.to_string();
            loop {
                if let Some(class_info) = registry.get_class(&current) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        // 返回第一个匹配的方法
                        return methods.first().map(|m| m.params.clone());
                    }
                    if let Some(ref parent) = class_info.parent {
                        current = parent.clone();
                        continue;
                    }
                }
                break;
            }
        }
        None
    }

    /// 获取方法形参个数
    fn get_method_param_count(&self, class_name: &str, method_name: &str) -> usize {
        self.get_method_params(class_name, method_name)
            .map(|p| p.len())
            .unwrap_or(0)
    }

    /// 获取可变参数在形参列表中的索引
    fn get_varargs_index(&self, class_name: &str, method_name: &str) -> Option<usize> {
        self.get_method_params(class_name, method_name)
            .and_then(|params| params.iter().position(|p| p.is_varargs))
    }

    /// 检查方法是否是可变参数方法
    /// 查询类型注册表来确定方法是否真的是可变参数方法
    fn is_varargs_method(&self, class_name: &str, method_name: &str) -> bool {
        // 查询类型注册表
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    // 检查是否有任何方法是可变参数的（扫描所有参数，不假设可变参数在最后）
                    for method in methods {
                        if method.params.iter().any(|p| p.is_varargs) {
                            return true;
                        }
                    }
                }
            }
        }
        // 默认返回false，避免将普通方法误认为可变参数方法
        false
    }

    /// 检查方法是否是实例方法（非静态方法）- 支持继承
    fn is_instance_method(&self, class_name: &str, method_name: &str) -> bool {
        // 查询类型注册表，支持继承查找
        if let Some(ref registry) = self.type_registry {
            // 先查 struct
            if let Some(struct_info) = registry.get_struct(class_name) {
                if let Some(methods) = struct_info.methods.get(method_name) {
                    for method in methods {
                        if !method.is_static {
                            return true;
                        }
                    }
                }
                return false;
            }
            
            let mut current_class_name = class_name.to_string();
            loop {
                if let Some(class_info) = registry.get_class(&current_class_name) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        // 检查是否有任何方法是实例方法（非静态）
                        for method in methods {
                            if !method.is_static {
                                return true;
                            }
                        }
                    }
                    // 在当前类没找到，查找父类
                    if let Some(ref parent_name) = class_info.parent {
                        current_class_name = parent_name.clone();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        // 默认返回false
        false
    }

    /// 将可变参数打包成数组（支持非末尾可变参数）
    fn pack_varargs_args(&mut self, class_name: &str, method_name: &str, arg_results: &[String]) -> cayResult<Vec<String>> {
        // 从类型注册表获取可变参数位置和元素类型
        let (varargs_index, varargs_elem_type) = self.get_varargs_info(class_name, method_name);
        let fixed_param_count = varargs_index;

        // 获取总形参个数以确定可变参数之后还有多少参数
        let total_param_count = self.get_method_param_count(class_name, method_name);
        let after_varargs_count = total_param_count.saturating_sub(varargs_index + 1);

        // 可变参数的实参个数 = 总实参 - 固定参数 - 可变参数之后的参数
        let varargs_min_count = if after_varargs_count > 0 { after_varargs_count } else { 0 };
        if arg_results.len() <= fixed_param_count + varargs_min_count {
            // 参数数量不足，不需要打包
            return Ok(arg_results.to_vec());
        }

        // 分割：固定参数 | 可变参数 | 之后参数
        let fixed_args = &arg_results[..fixed_param_count];
        let varargs_end = arg_results.len() - varargs_min_count;
        let varargs = &arg_results[fixed_param_count..varargs_end];
        let after_args = &arg_results[varargs_end..];

        // 检查是否只有一个参数且是数组类型（直接传递数组给可变参数）
        if varargs.len() == 1 {
            let (arg_type, arg_val) = self.parse_typed_value(&varargs[0]);
            // 检查参数类型是否是数组指针（以*结尾但不是i8*）
            if arg_type.ends_with("*") && arg_type != "i8*" {
                // 直接将数组指针作为可变参数传递
                let mut result = fixed_args.to_vec();
                result.push(format!("i8* {}", arg_val));
                return Ok(result);
            }
        }

        // 创建数组来存储可变参数
        let array_size = varargs.len();
        let raw_ptr = self.new_temp();
        let array_ptr = self.new_temp();

        // 根据元素类型确定 LLVM 类型和大小
        let (llvm_elem_type, elem_size) = match varargs_elem_type {
            crate::types::Type::Int32 => ("i32", 4),
            crate::types::Type::Int64 => ("i64", 8),
            crate::types::Type::Float32 => ("float", 4),
            crate::types::Type::Float64 => ("double", 8),
            crate::types::Type::String => ("i8", 8), // String 是指针类型
            crate::types::Type::Char => ("i8", 1),
            crate::types::Type::Bool => ("i8", 1),
            _ => ("i32", 4), // 默认使用 i32
        };

        // 分配数组内存：8字节（长度+padding）+ 元素数据
        let header_size = 8;
        let data_size = array_size * elem_size;
        let total_size = header_size + data_size;
        self.emit_line(&format!("  {} = call i8* @calloc(i64 1, i64 {})", raw_ptr, total_size));

        // 存储长度信息到前4字节
        let len_ptr_i8 = self.new_temp();
        let len_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 0", len_ptr_i8, raw_ptr));
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", len_ptr, len_ptr_i8));
        self.emit_line(&format!("  store i32 {}, i32* {}, align 4", array_size, len_ptr));

        // 计算数组元素起始地址（跳过8字节头部）
        self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 {}", array_ptr, raw_ptr, header_size));

        // 将可变参数存入数组
        for (i, arg_str) in varargs.iter().enumerate() {
            let (arg_type, arg_val) = self.parse_typed_value(arg_str);
            let elem_ptr_i8 = self.new_temp();
            let offset = i * elem_size;

            // 计算元素地址 (i8*)
            self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 {}", elem_ptr_i8, array_ptr, offset));

            // 根据元素类型进行存储
            self.store_vararg_element(&elem_ptr_i8, &arg_type, &arg_val, llvm_elem_type);
        }

        // 构建结果：固定参数 + 数组指针 + 之后参数
        let mut result = fixed_args.to_vec();
        result.push(format!("i8* {}", array_ptr));
        result.extend(after_args.iter().cloned());

        Ok(result)
    }

    /// 获取可变参数方法的固定参数数量和元素类型
    /// 返回 (varargs_param_index, element_type)，如果未找到可变参数则返回 (0, Int32)
    fn get_varargs_info(&self, class_name: &str, method_name: &str) -> (usize, crate::types::Type) {
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    for method in methods {
                        // 扫描所有参数，找到可变参数
                        for (i, param) in method.params.iter().enumerate() {
                            if param.is_varargs {
                                let fixed_count = i; // 可变参数之前的固定参数数量
                                let elem_type = match &param.param_type {
                                    crate::types::Type::Array(elem) => elem.as_ref().clone(),
                                    _ => param.param_type.clone(),
                                };
                                return (fixed_count, elem_type);
                            }
                        }
                    }
                }
            }
        }
        // 默认值：没有固定参数，元素类型为 Int32
        (0, crate::types::Type::Int32)
    }

    /// 存储可变参数元素到数组
    fn store_vararg_element(&mut self, elem_ptr_i8: &str, arg_type: &str, arg_val: &str, llvm_elem_type: &str) {
        match llvm_elem_type {
            "i32" => {
                let elem_ptr = self.new_temp();
                self.emit_line(&format!("  {} = bitcast i8* {} to i32*", elem_ptr, elem_ptr_i8));
                if arg_type == "i64" {
                    let truncated = self.new_temp();
                    self.emit_line(&format!("  {} = trunc i64 {} to i32", truncated, arg_val));
                    self.emit_line(&format!("  store i32 {}, i32* {}, align 4", truncated, elem_ptr));
                } else if arg_type == "i32" {
                    self.emit_line(&format!("  store i32 {}, i32* {}, align 4", arg_val, elem_ptr));
                }
            }
            "i64" => {
                let elem_ptr = self.new_temp();
                self.emit_line(&format!("  {} = bitcast i8* {} to i64*", elem_ptr, elem_ptr_i8));
                if arg_type == "i32" {
                    let extended = self.new_temp();
                    self.emit_line(&format!("  {} = sext i32 {} to i64", extended, arg_val));
                    self.emit_line(&format!("  store i64 {}, i64* {}, align 8", extended, elem_ptr));
                } else {
                    self.emit_line(&format!("  store i64 {}, i64* {}, align 8", arg_val, elem_ptr));
                }
            }
            "float" => {
                let elem_ptr = self.new_temp();
                self.emit_line(&format!("  {} = bitcast i8* {} to float*", elem_ptr, elem_ptr_i8));
                // 如果参数是 double 类型，需要转换为 float
                if arg_type == "double" {
                    let converted = self.new_temp();
                    self.emit_line(&format!("  {} = fptrunc double {} to float", converted, arg_val));
                    self.emit_line(&format!("  store float {}, float* {}, align 4", converted, elem_ptr));
                } else {
                    self.emit_line(&format!("  store float {}, float* {}, align 4", arg_val, elem_ptr));
                }
            }
            "double" => {
                let elem_ptr = self.new_temp();
                self.emit_line(&format!("  {} = bitcast i8* {} to double*", elem_ptr, elem_ptr_i8));
                self.emit_line(&format!("  store double {}, double* {}, align 8", arg_val, elem_ptr));
            }
            "i8" => {
                // 用于 String (i8*), char, bool
                let elem_ptr = self.new_temp();
                self.emit_line(&format!("  {} = bitcast i8* {} to i8**", elem_ptr, elem_ptr_i8));
                self.emit_line(&format!("  store i8* {}, i8** {}, align 8", arg_val, elem_ptr));
            }
            _ => {
                // 默认处理为 i32
                let elem_ptr = self.new_temp();
                self.emit_line(&format!("  {} = bitcast i8* {} to i32*", elem_ptr, elem_ptr_i8));
                self.emit_line(&format!("  store i32 {}, i32* {}, align 4", arg_val, elem_ptr));
            }
        }
    }

    /// 获取方法的参数类型列表
    fn get_method_param_types(&self, class_name: &str, method_name: &str, processed_args: &[String], has_varargs_array: bool) -> Vec<crate::types::Type> {
        if let Some(ref registry) = self.type_registry {
            // 在类及其父类中查找方法
            let mut current_class_name = class_name.to_string();
            loop {
                if let Some(class_info) = registry.get_class(&current_class_name) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        let arg_count = processed_args.len();
                        
                        for method in methods {
                            let param_count = method.params.len();
                            let is_varargs = method.params.iter().any(|p| p.is_varargs);
                            let varargs_idx = method.params.iter().position(|p| p.is_varargs);
                            
                            if is_varargs {
                                let fixed_count = varargs_idx.unwrap_or(param_count);
                                if arg_count >= fixed_count {
                                    // 返回所有非可变参数的参数类型（可变参数之前的+之后的）
                                    return method.params.iter()
                                        .enumerate()
                                        .filter(|(i, p)| !p.is_varargs)
                                        .map(|(_, p)| p.param_type.clone())
                                        .collect();
                                }
                            } else if param_count == arg_count {
                                return method.params.iter()
                                    .map(|p| p.param_type.clone())
                                    .collect();
                            }
                        }
                    }
                    
                    // 在父类中查找
                    if let Some(parent) = &class_info.parent {
                        current_class_name = parent.clone();
                        continue;
                    }
                }
                break;
            }
        }
        Vec::new()
    }

    /// 转换参数类型以匹配形参类型
    fn convert_arg_type(&mut self, arg_type: &str, arg_val: &str, param_llvm_type: &str) -> String {
        // 如果类型已经匹配，直接返回
        if arg_type == param_llvm_type {
            return format!("{} {}", arg_type, arg_val);
        }

        // double -> float 转换
        if arg_type == "double" && param_llvm_type == "float" {
            let converted = self.new_temp();
            self.emit_line(&format!("  {} = fptrunc double {} to float", converted, arg_val));
            return format!("float {}", converted);
        }

        // float -> double 转换
        if arg_type == "float" && param_llvm_type == "double" {
            let converted = self.new_temp();
            self.emit_line(&format!("  {} = fpext float {} to double", converted, arg_val));
            return format!("double {}", converted);
        }

        // i32 -> i64 转换
        if arg_type == "i32" && param_llvm_type == "i64" {
            let converted = self.new_temp();
            self.emit_line(&format!("  {} = sext i32 {} to i64", converted, arg_val));
            return format!("i64 {}", converted);
        }

        // i64 -> i32 截断
        if arg_type == "i64" && param_llvm_type == "i32" {
            let converted = self.new_temp();
            self.emit_line(&format!("  {} = trunc i64 {} to i32", converted, arg_val));
            return format!("i32 {}", converted);
        }

        // 指针 -> i64 转换 (ptrtoint)
        if arg_type.ends_with("*") && param_llvm_type == "i64" {
            let converted = self.new_temp();
            self.emit_line(&format!("  {} = ptrtoint {} {} to i64", converted, arg_type, arg_val));
            return format!("i64 {}", converted);
        }

        // i64 -> 指针 转换 (inttoptr)
        if arg_type == "i64" && param_llvm_type.ends_with("*") {
            let converted = self.new_temp();
            self.emit_line(&format!("  {} = inttoptr i64 {} to {}", converted, arg_val, param_llvm_type));
            return format!("{} {}", param_llvm_type, converted);
        }

        // 默认：不进行转换
        format!("{} {}", arg_type, arg_val)
    }

    /// 生成 extern 函数调用
    ///
    /// # Arguments
    /// * `func_name` - extern 函数名称
    /// * `args` - 函数参数
    /// * `loc` - 源码位置
    fn generate_extern_function_call(&mut self, func_name: &str, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
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
                None => return Err(codegen_error_at(loc.clone(), format!("Extern function '{}' not found", func_name))),
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
        let mut processed_args = Vec::new();
        for (idx, arg_str) in arg_results.iter().enumerate() {
            let (arg_type, arg_val) = self.parse_typed_value(arg_str);
            
            // 获取参数的期望类型（从 extern 函数声明中）
            if idx < extern_func.params.len() {
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

        // 检查是否是可变参数函数
        let is_varargs = extern_func.params.iter().any(|p| p.is_varargs);

        // 直接调用 extern 函数（不创建包装函数）
        if llvm_ret_type == "void" {
            if is_varargs {
                // 可变参数函数需要显式类型签名
                let param_types: Vec<String> = extern_func.params.iter()
                    .filter(|p| !p.is_varargs)
                    .map(|p| self.type_to_llvm(&p.param_type))
                    .collect();
                let type_sig = if param_types.is_empty() {
                    "(...)".to_string()
                } else {
                    format!("({}, ...)", param_types.join(", "))
                };
                self.emit_line(&format!("  call void {} @{}({})",
                    type_sig, llvm_func_name, processed_args.join(", ")));
            } else {
                self.emit_line(&format!("  call void @{}({})",
                    llvm_func_name, processed_args.join(", ")));
            }
            Ok("void %dummy".to_string())
        } else {
            let temp = self.new_temp();
            if is_varargs {
                // 可变参数函数需要显式类型签名
                let param_types: Vec<String> = extern_func.params.iter()
                    .filter(|p| !p.is_varargs)
                    .map(|p| self.type_to_llvm(&p.param_type))
                    .collect();
                let type_sig = if param_types.is_empty() {
                    format!("{} (...)", llvm_ret_type)
                } else {
                    format!("{} ({}, ...)", llvm_ret_type, param_types.join(", "))
                };
                self.emit_line(&format!("  {} = call {} @{}({})",
                    temp, type_sig, llvm_func_name, processed_args.join(", ")));
            } else {
                self.emit_line(&format!("  {} = call {} @{}({})",
                    temp, llvm_ret_type, llvm_func_name, processed_args.join(", ")));
            }
            Ok(format!("{} {}", llvm_ret_type, temp))
        }
    }

    /// 生成 __cay_buffer_to_string 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 i8* (String)
    fn generate_buffer_to_string_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        if args.len() != 2 {
            return Err(codegen_error_at(loc.clone(), "__cay_buffer_to_string requires 2 arguments".to_string()));
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
        self.emit_line(&format!("  {} = call i8* @__cay_buffer_to_string(i64 {}, i32 {})",
            temp, llvm_arg1, llvm_arg2));

        Ok(format!("i8* {}", temp))
    }
    
    /// 生成 __cay_read_ptr 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 i64
    fn generate_read_ptr_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(loc.clone(), "__cay_read_ptr requires 1 argument".to_string()));
        }

        // 生成参数
        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);
        
        // 转换参数类型为 i64，并提取值部分
        let llvm_arg_full = self.convert_arg_type(&arg_type, &arg_val, "i64");
        let llvm_arg_val = llvm_arg_full.split_whitespace().last().unwrap_or(&arg_val);

        // 调用运行时函数
        let temp = self.new_temp();
        self.emit_line(&format!("  {} = call i64 @__cay_read_ptr(i64 {})",
            temp, llvm_arg_val));

        Ok(format!("i64 {}", temp))
    }
    
    /// 生成 __cay_ptr_to_string 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 i8* (String)
    fn generate_ptr_to_string_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(loc.clone(), "__cay_ptr_to_string requires 1 argument".to_string()));
        }

        // 生成参数
        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);
        
        // 转换参数类型为 i64，并提取值部分
        let llvm_arg_full = self.convert_arg_type(&arg_type, &arg_val, "i64");
        let llvm_arg_val = llvm_arg_full.split_whitespace().last().unwrap_or(&arg_val);

        // 调用运行时函数
        let temp = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @__cay_ptr_to_string(i64 {})",
            temp, llvm_arg_val));

        Ok(format!("i8* {}", temp))
    }
    
    /// 生成 __cay_write_ptr 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 void
    fn generate_write_ptr_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        if args.len() != 2 {
            return Err(codegen_error_at(loc.clone(), "__cay_write_ptr requires 2 arguments".to_string()));
        }

        // 生成参数
        let arg1_result = self.generate_expression(&args[0])?;
        let arg2_result = self.generate_expression(&args[1])?;

        let (arg1_type, arg1_val) = self.parse_typed_value(&arg1_result);
        let (arg2_type, arg2_val) = self.parse_typed_value(&arg2_result);

        // 转换参数类型，并提取值部分
        let llvm_arg1_full = self.convert_arg_type(&arg1_type, &arg1_val, "i64");
        let llvm_arg1_val = llvm_arg1_full.split_whitespace().last().unwrap_or(&arg1_val);
        let llvm_arg2_full = self.convert_arg_type(&arg2_type, &arg2_val, "i64");
        let llvm_arg2_val = llvm_arg2_full.split_whitespace().last().unwrap_or(&arg2_val);

        // 调用运行时函数
        self.emit_line(&format!("  call void @__cay_write_ptr(i64 {}, i64 {})",
            llvm_arg1_val, llvm_arg2_val));

        Ok("void %dummy".to_string())
    }
    
    /// 生成 __cay_write_int 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 void
    fn generate_write_int_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        if args.len() != 2 {
            return Err(codegen_error_at(loc.clone(), "__cay_write_int requires 2 arguments".to_string()));
        }

        // 生成参数
        let arg1_result = self.generate_expression(&args[0])?;
        let arg2_result = self.generate_expression(&args[1])?;

        let (arg1_type, arg1_val) = self.parse_typed_value(&arg1_result);
        let (arg2_type, arg2_val) = self.parse_typed_value(&arg2_result);

        // 转换参数类型，并提取值部分
        let llvm_arg1_full = self.convert_arg_type(&arg1_type, &arg1_val, "i64");
        let llvm_arg1_val = llvm_arg1_full.split_whitespace().last().unwrap_or(&arg1_val);
        let llvm_arg2_full = self.convert_arg_type(&arg2_type, &arg2_val, "i32");
        let llvm_arg2_val = llvm_arg2_full.split_whitespace().last().unwrap_or(&arg2_val);

        // 调用运行时函数
        self.emit_line(&format!("  call void @__cay_write_int(i64 {}, i32 {})",
            llvm_arg1_val, llvm_arg2_val));

        Ok("void %dummy".to_string())
    }

    /// 生成 __cay_read_int 运行时函数调用
    /// 这个函数在运行时模块中已经定义，返回 i32
    fn generate_cay_read_int_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(loc.clone(), "__cay_read_int requires 1 argument".to_string()));
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
        self.emit_line(&format!("  {} = call i32 @__cay_read_int(i64 {})",
            temp, llvm_arg_val));

        Ok(format!("i32 {}", temp))
    }

    /// 生成 String.valueOf() 静态方法调用
    /// 支持多种类型：int, long, float, double, bool, char
    fn generate_string_valueof_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(loc.clone(), "String.valueOf() takes exactly 1 argument".to_string()));
        }

        // 生成参数
        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);

        let temp = self.new_temp();

        // 根据参数类型选择对应的转换函数
        match arg_type.as_str() {
            "i32" => {
                // int -> String
                self.emit_line(&format!("  {} = call i8* @__cay_int_to_string(i32 {})",
                    temp, arg_val));
            }
            "i64" => {
                // long -> String
                self.emit_line(&format!("  {} = call i8* @__cay_long_to_string(i64 {})",
                    temp, arg_val));
            }
            "float" => {
                // float -> String
                self.emit_line(&format!("  {} = call i8* @__cay_float_to_string(float {})",
                    temp, arg_val));
            }
            "double" => {
                // double -> String
                self.emit_line(&format!("  {} = call i8* @__cay_double_to_string(double {})",
                    temp, arg_val));
            }
            "i1" => {
                // bool -> String
                self.emit_line(&format!("  {} = call i8* @__cay_bool_to_string(i1 {})",
                    temp, arg_val));
            }
            "i8" => {
                // char -> String
                self.emit_line(&format!("  {} = call i8* @__cay_char_to_string(i8 {})",
                    temp, arg_val));
            }
            "i8*" => {
                // String -> String (直接返回)
                return Ok(format!("i8* {}", arg_val));
            }
            _ => {
                return Err(codegen_error_at(loc.clone(), format!("String.valueOf() does not support type: {}", arg_type)));
            }
        }

        Ok(format!("i8* {}", temp))
    }

    /// 生成 Integer.parseInt() 静态方法调用
    /// 将 String 转换为 int
    fn generate_integer_parseint_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(loc.clone(), "Integer.parseInt() takes exactly 1 argument".to_string()));
        }

        // 生成参数（String）
        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);

        // 检查参数类型是否为 String (i8*)
        if arg_type != "i8*" {
            return Err(codegen_error_at(loc.clone(), format!("Integer.parseInt() expects String, got {}", arg_type)));
        }

        let temp = self.new_temp();

        // 调用 atoi 函数将字符串转换为整数
        self.emit_line(&format!("  {} = call i32 @atoi(i8* {})",
            temp, arg_val));

        Ok(format!("i32 {}", temp))
    }

    /// 生成函数指针调用
    /// 
    /// # Arguments
    /// * `var_name` - 函数指针变量名
    /// * `args` - 参数表达式列表
    /// * `func_type` - 函数指针类型
    /// * `loc` - 源码位置
    fn generate_function_pointer_call(&mut self, var_name: &str, args: &[Expr], func_type: &crate::types::Type, loc: &crate::error::SourceLocation) -> cayResult<String> {
        use crate::types::{Type, FunctionType};
        
        // 获取函数类型信息
        let (param_types, ret_type) = if let Type::Function(func) = func_type {
            (func.params.clone(), *func.return_type.clone())
        } else {
            return Err(codegen_error_at(loc.clone(), format!("Variable '{}' is not a function pointer", var_name)));
        };
        
        // 检查参数数量
        if args.len() != param_types.len() {
            return Err(codegen_error_at(loc.clone(), format!(
                "Function pointer call requires {} arguments, but got {}",
                param_types.len(), args.len()
            )));
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
        let llvm_name = self.scope_manager.get_llvm_name(var_name)
            .ok_or_else(|| codegen_error_at(loc.clone(), format!("Undefined function pointer variable: {}", var_name)))?;
        
        // 加载函数指针
        let func_ptr_temp = self.new_temp();
        let func_ptr_type = self.type_to_llvm(func_type);
        self.emit_line(&format!("  {} = load {}, {}* %{}, align 8",
            func_ptr_temp, func_ptr_type, func_ptr_type, llvm_name));
        
        // 生成调用
        let llvm_ret_type = self.type_to_llvm(&ret_type);
        if llvm_ret_type == "void" {
            self.emit_line(&format!("  call void {}({})",
                func_ptr_temp, arg_values.join(", ")));
            Ok("void %dummy".to_string())
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = call {} {}({})",
                temp, llvm_ret_type, func_ptr_temp, arg_values.join(", ")));
            Ok(format!("{} {}", llvm_ret_type, temp))
        }
    }

    /// 获取类的字段类型
    fn get_field_type(&self, class_name: &str, field_name: &str) -> Option<crate::types::Type> {
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(field_info) = class_info.fields.get(field_name) {
                    return Some(field_info.field_type.clone());
                }
            }
        }
        None
    }

    /// 生成成员函数指针字段调用
    fn generate_member_func_ptr_call(&mut self, member: &crate::ast::MemberAccessExpr, args: &[Expr], func_type: &crate::types::Type, loc: &crate::error::SourceLocation) -> cayResult<String> {
        use crate::types::{Type, FunctionType};
        use crate::ast::Expr;

        // 获取函数类型信息
        let (param_types, ret_type) = if let Type::Function(func) = func_type {
            (func.params.clone(), *func.return_type.clone())
        } else {
            return Err(codegen_error_at(loc.clone(), format!("Field '{}' is not a function pointer", member.member)));
        };

        // 检查参数数量
        if args.len() != param_types.len() {
            return Err(codegen_error_at(loc.clone(), format!(
                "Function pointer call requires {} arguments, but got {}",
                param_types.len(), args.len()
            )));
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

        // 获取字段偏移量并加载函数指针
        // 确定类名：如果是this，使用current_class；否则从表达式推断
        let class_name = if let Expr::Identifier(obj_name) = member.object.as_ref() {
            if obj_name.as_ref() == "this" {
                self.current_class.clone()
            } else if let Some(obj_type) = self.get_expression_type(&member.object) {
                if let Type::Object(name) = obj_type {
                    name
                } else {
                    return Err(codegen_error_at(loc.clone(), "Object is not a class instance".to_string()));
                }
            } else {
                return Err(codegen_error_at(loc.clone(), "Cannot determine object type".to_string()));
            }
        } else if let Some(obj_type) = self.get_expression_type(&member.object) {
            if let Type::Object(name) = obj_type {
                name
            } else {
                return Err(codegen_error_at(loc.clone(), "Object is not a class instance".to_string()));
            }
        } else {
            return Err(codegen_error_at(loc.clone(), "Cannot determine object type".to_string()));
        };

        // 获取字段信息（使用类布局信息获取偏移量）
        let field_offset = if let Some(field_info) = self.get_instance_field(&class_name, &member.member) {
            field_info.offset
        } else {
            return Err(codegen_error_at(loc.clone(), format!("Field '{}' not found in class '{}'", member.member, class_name)));
        };

        // 计算字段地址
        let field_ptr_i8 = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 {}",
            field_ptr_i8, obj_val, field_offset));

        // 加载函数指针
        let func_ptr_type = self.type_to_llvm(func_type);
        let func_ptr_temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to {}*",
            func_ptr_temp, field_ptr_i8, func_ptr_type));

        let loaded_func_ptr = self.new_temp();
        self.emit_line(&format!("  {} = load {}, {}* {}, align 8",
            loaded_func_ptr, func_ptr_type, func_ptr_type, func_ptr_temp));

        // 生成调用
        let llvm_ret_type = self.type_to_llvm(&ret_type);
        if llvm_ret_type == "void" {
            self.emit_line(&format!("  call void {}({})",
                loaded_func_ptr, arg_values.join(", ")));
            Ok("void %dummy".to_string())
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = call {} {}({})",
                temp, llvm_ret_type, loaded_func_ptr, arg_values.join(", ")));
            Ok(format!("{} {}", llvm_ret_type, temp))
        }
    }
}
