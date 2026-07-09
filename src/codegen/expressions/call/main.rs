//! 函数调用表达式代码生成 - 主入口
//!
//! 处理函数调用表达式的主分发逻辑。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};
use crate::semantic::resolve_call_args;

impl IRGenerator {
    /// 生成函数调用表达式代码
    ///
    /// # Arguments
    /// * `call` - 函数调用表达式
    pub fn generate_call_expression(&mut self, call: &CallExpr) -> CayResult<String> {
        // 捕获变量声明为泛型静态工厂调用（如 `Optional<int> x = Optional.of(42)`）设置的
        // 期望类型，供下方将静态方法调用特化到具体单态化版本时推断类型参数。
        // 在生成任何子表达式（可能清除该字段）之前读取。
        let expected_static_type = self.pending_new_expected_type.clone();
        // 进入本次调用前的泛型类型参数映射快照。下方静态工厂特化与接收者泛型安装
        // 都会修改该映射，调用结束时恢复到此快照，避免类型参数泄漏到后续代码。
        let entry_generic_args = self.generic_type_args.clone();
        // 处理 print、println、eprint、eprintln 函数
        if let Expr::Identifier(name) = call.callee.as_ref() {
            match name.as_str() {
                "print" => {
                    return self.generate_print_call(
                        &call.args,
                        false,
                        crate::codegen::expressions::PrintStream::Stdout,
                        &call.loc,
                    )
                }
                "println" => {
                    return self.generate_print_call(
                        &call.args,
                        true,
                        crate::codegen::expressions::PrintStream::Stdout,
                        &call.loc,
                    )
                }
                "eprint" => {
                    return self.generate_print_call(
                        &call.args,
                        false,
                        crate::codegen::expressions::PrintStream::Stderr,
                        &call.loc,
                    )
                }
                "eprintln" => {
                    return self.generate_print_call(
                        &call.args,
                        true,
                        crate::codegen::expressions::PrintStream::Stderr,
                        &call.loc,
                    )
                }
                "exit" => return self.generate_exit_call(&call.args, &call.loc),
                "readInt" => return self.generate_read_int_call(&call.args, &call.loc),
                "readLong" => return self.generate_read_long_call(&call.args, &call.loc),
                "readFloat" => return self.generate_read_float_call(&call.args, &call.loc),
                "readDouble" => return self.generate_read_double_call(&call.args, &call.loc),
                "readLine" => return self.generate_read_line_call(&call.args, &call.loc),
                "readChar" => return self.generate_read_char_call(&call.args, &call.loc),
                // 运行时辅助函数
                "__cay_read_ptr" => return self.generate_read_ptr_call(&call.args, &call.loc),
                "__cay_ptr_to_string" => {
                    return self.generate_ptr_to_string_call(&call.args, &call.loc);
                }
                "__cay_write_ptr" => return self.generate_write_ptr_call(&call.args, &call.loc),
                "__cay_write_int" => return self.generate_write_int_call(&call.args, &call.loc),
                "__cay_read_int" => return self.generate_cay_read_int_call(&call.args, &call.loc),
                _ => {}
            }

            // 5.3.0: 支持省略 new 的类实例化 ClassName(args) / ClassName<T>(args)
            // 当标识符是类名且不被局部变量/函数遮蔽时，生成 new 表达式代码
            if let Some(class_name) = self.try_resolve_class_instantiation(name.as_ref()) {
                return self.generate_new_expression(&NewExpr {
                    class_name,
                    args: call.args.clone(),
                    loc: call.loc.clone(),
                });
            }

            // 5.3.0: 支持命名空间式静态类方法调用 ClassName::staticMethod(args)
            // 当标识符形如 ClassName::methodName 且前缀为类、后缀为静态方法时，
            // 将其重写为 ClassName.staticMethod(args) 进行代码生成
            if let Some((class_name, method_name)) =
                self.try_resolve_static_method_call(name.as_ref())
            {
                let member_call = CallExpr {
                    callee: Box::new(Expr::MemberAccess(MemberAccessExpr {
                        object: Box::new(Expr::Identifier(IdentifierExpr {
                            name: class_name,
                            loc: call.callee.location().clone(),
                        })),
                        member: method_name,
                        loc: call.callee.location().clone(),
                    })),
                    args: call.args.clone(),
                    loc: call.loc.clone(),
                };
                return self.generate_call_expression(&member_call);
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

            // 处理基本类型的 toString() 方法调用
            if member.member == "toString" && call.args.is_empty() {
                if let Some(obj_type) = self.get_expression_type(&member.object) {
                    let obj_val = self.generate_expression(&member.object)?;
                    let (_, val) = self.parse_typed_value(&obj_val);
                    let temp = self.new_temp();
                    match obj_type {
                        crate::types::Type::Int32 => {
                            self.emit_line(&format!(
                                "  {} = call i8* @__cay_int_to_string(i32 {})",
                                temp, val
                            ));
                            return Ok(format!("i8* {}", temp));
                        }
                        crate::types::Type::Int64 => {
                            self.emit_line(&format!(
                                "  {} = call i8* @__cay_long_to_string(i64 {})",
                                temp, val
                            ));
                            return Ok(format!("i8* {}", temp));
                        }
                        crate::types::Type::Float32 => {
                            self.emit_line(&format!(
                                "  {} = call i8* @__cay_float_to_string(float {})",
                                temp, val
                            ));
                            return Ok(format!("i8* {}", temp));
                        }
                        crate::types::Type::Float64 => {
                            self.emit_line(&format!(
                                "  {} = call i8* @__cay_double_to_string(double {})",
                                temp, val
                            ));
                            return Ok(format!("i8* {}", temp));
                        }
                        crate::types::Type::Bool => {
                            self.emit_line(&format!(
                                "  {} = call i8* @__cay_bool_to_string(i1 {})",
                                temp, val
                            ));
                            return Ok(format!("i8* {}", temp));
                        }
                        crate::types::Type::Char => {
                            self.emit_line(&format!(
                                "  {} = call i8* @__cay_char_to_string(i8 {})",
                                temp, val
                            ));
                            return Ok(format!("i8* {}", temp));
                        }
                        _ => {}
                    }
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
                        return self.generate_function_pointer_call(
                            name_str, &call.args, &var_type, &call.loc,
                        );
                    }
                }
                // 检查是否是 @FreeFunction 导出的自由函数
                let free_fn_info = self
                    .type_registry
                    .as_ref()
                    .and_then(|r| r.free_functions.get(name_str))
                    .map(|(class_name, method_info, _)| {
                        (class_name.clone(), method_info.name.clone())
                    });
                if let Some((owner_class, method_name)) = free_fn_info {
                    // 将 @FreeFunction 调用转为对应类的静态方法调用
                    // 使用注册时的方法名（而非调用时的限定名）
                    (owner_class, method_name, None, true)
                } else if self.is_top_level_function(name_str) {
                    // 顶层函数没有类名前缀
                    (String::new(), name_str.to_string(), None, false)
                } else if !self.current_class.is_empty() {
                    // 泛型特化方法体内使用完整特化类名，确保隐式静态/实例调用链接到
                    // 已单态化的特化版本，而非类型擦除的基础模板。
                    let class_name = self
                        .current_class_specialized
                        .clone()
                        .unwrap_or_else(|| self.current_class.clone());
                    (class_name, name_str.to_string(), None, false)
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
                            let parent_class = self
                                .get_parent_class(&self.current_class)
                                .unwrap_or_else(|| self.current_class.clone());
                            (
                                parent_class,
                                member.member.clone(),
                                Some(member.object.clone()),
                                false,
                            )
                        } else if obj_name_str == "this" {
                            // this.methodName() - 首先检查是否是函数指针字段
                            let class_name = self
                                .current_class_specialized
                                .clone()
                                .unwrap_or_else(|| self.current_class.clone());
                            if let Some(field_type) =
                                self.get_field_type(&class_name, &member.member)
                            {
                                if matches!(field_type, crate::types::Type::Function(_)) {
                                    // 是函数指针字段调用
                                    return self.generate_member_func_ptr_call(
                                        member,
                                        &call.args,
                                        &field_type,
                                        &call.loc,
                                    );
                                }
                            }
                            // 不是函数指针字段，按普通方法处理
                            (
                                class_name,
                                member.member.clone(),
                                Some(member.object.clone()),
                                false,
                            )
                        } else {
                            // 检查是否是 enum 构造函数调用: EnumName.VariantName(args)
                            if let Some(ref registry) = self.type_registry {
                                if let Some(enum_info) = registry.get_enum(obj_name_str) {
                                    if let Some(idx) = enum_info
                                        .variants
                                        .iter()
                                        .position(|v| v.name == member.member)
                                    {
                                        let has_payload =
                                            enum_info.variants[idx].payload_type.is_some();
                                        let payload_val = if has_payload {
                                            let val = self.generate_expression(&call.args[0])?;
                                            let (pl_type, pl_val) = self.parse_typed_value(&val);
                                            if pl_type == "i32" {
                                                let ext = self.new_temp();
                                                self.emit_line(&format!(
                                                    "  {} = sext i32 {} to i64",
                                                    ext, pl_val
                                                ));
                                                ext
                                            } else if pl_type == "i8*" || pl_type.ends_with('*') {
                                                let ptr_to_i64 = self.new_temp();
                                                self.emit_line(&format!(
                                                    "  {} = ptrtoint {} {} to i64",
                                                    ptr_to_i64, pl_type, pl_val
                                                ));
                                                ptr_to_i64
                                            } else {
                                                pl_val.to_string()
                                            }
                                        } else {
                                            "0".to_string()
                                        };
                                        // 构造 struct { i32 discriminant, i64 payload }
                                        let struct_val = self.new_temp();
                                        self.emit_line(&format!(
                                            "  {} = insertvalue {{ i32, i64 }} undef, i32 {}, 0",
                                            struct_val, idx
                                        ));
                                        let struct_val2 = self.new_temp();
                                        self.emit_line(&format!(
                                            "  {} = insertvalue {{ i32, i64 }} {}, i64 {}, 1",
                                            struct_val2, struct_val, payload_val
                                        ));
                                        return Ok(format!("{{ i32, i64 }} {}", struct_val2));
                                    }
                                }
                            }
                            // 首先检查是否是已知的类名
                            // 对于泛型类型如 FileResult<File>，需要提取基础类名 FileResult 进行检查
                            let base_obj_name = if let Some(lt_pos) = obj_name_str.find('<') {
                                &obj_name_str[..lt_pos]
                            } else {
                                obj_name_str
                            };
                            let (class_name, is_class) =
                                if let Some(ref registry) = self.type_registry {
                                    if registry.class_exists(base_obj_name)
                                        || registry.find_qualified_class(base_obj_name).is_some()
                                        || registry.get_struct(base_obj_name).is_some()
                                        || registry.get_enum_by_name(base_obj_name).is_some()
                                    {
                                        // 是类名，保留原始泛型类型名（如 FileResult<File>）
                                        (obj_name_str.to_string(), true)
                                    } else {
                                        // 不是类名，尝试从变量映射获取
                                        let result = self
                                            .var_class_map
                                            .get(obj_name_str)
                                            .cloned()
                                            .unwrap_or_else(|| obj_name_str.to_string());
                                        (result, false)
                                    }
                                } else {
                                    let result = self
                                        .var_class_map
                                        .get(obj_name_str)
                                        .cloned()
                                        .unwrap_or_else(|| obj_name_str.to_string());
                                    (result, false)
                                };

                            // 如果不是类名（是变量），检查是否是函数指针字段调用
                            if !is_class {
                                if let Some(field_type) =
                                    self.get_field_type(&class_name, &member.member)
                                {
                                    if matches!(field_type, crate::types::Type::Function(_)) {
                                        // 是函数指针字段调用，生成函数指针调用代码
                                        return self.generate_member_func_ptr_call(
                                            member,
                                            &call.args,
                                            &field_type,
                                            &call.loc,
                                        );
                                    }
                                }
                            }

                            // 如果是类名.方法名() 形式，标记为静态方法调用
                            (
                                class_name,
                                member.member.clone(),
                                Some(member.object.clone()),
                                is_class,
                            )
                        }
                    }
                    _ => {
                        // object 不是标识符，可能是其他表达式（如 new 表达式）
                        // 尝试从表达式推断类型
                        if let Some(obj_type) = self.get_expression_type(&member.object) {
                            match obj_type {
                                crate::types::Type::Object(class_name) => {
                                    // 首先检查是否是函数指针字段
                                    if let Some(field_type) =
                                        self.get_field_type(&class_name, &member.member)
                                    {
                                        if matches!(field_type, crate::types::Type::Function(_)) {
                                            // 是函数指针字段调用，生成函数指针调用代码
                                            return self.generate_member_func_ptr_call(
                                                member,
                                                &call.args,
                                                &field_type,
                                                &call.loc,
                                            );
                                        }
                                    }
                                    // 不是函数指针字段，按普通方法处理
                                    (
                                        class_name,
                                        member.member.clone(),
                                        Some(member.object.clone()),
                                        false,
                                    )
                                }
                                crate::types::Type::Generic(class_name, type_args) => {
                                    // 对于泛型类型（如 vector<Student>），处理其方法调用
                                    // 建立类型参数映射，支持泛型特化
                                    if let Some(ref registry) = self.type_registry {
                                        if let Some(class_info) = registry.get_class(&class_name) {
                                            if !class_info.type_params.is_empty()
                                                && !type_args.is_empty()
                                            {
                                                for (idx, param) in
                                                    class_info.type_params.iter().enumerate()
                                                {
                                                    let resolved_arg = type_args
                                                        .get(idx)
                                                        .cloned()
                                                        .or_else(|| param.default_type.clone())
                                                        .unwrap_or_else(|| {
                                                            crate::types::Type::GenericParam(
                                                                param.name.clone(),
                                                            )
                                                        });
                                                    self.generic_type_args
                                                        .insert(param.name.clone(), resolved_arg);
                                                }
                                            }
                                        }
                                    }
                                    // 构建完整特化类名（如 Box<int>）用于方法查找与生成
                                    let type_args_str: Vec<String> =
                                        type_args.iter().map(|t| format!("{}", t)).collect();
                                    let specialized_class_name =
                                        format!("{}<{}>", class_name, type_args_str.join(", "));
                                    // 首先检查是否是函数指针字段
                                    if let Some(field_type) =
                                        self.get_field_type(&class_name, &member.member)
                                    {
                                        if matches!(field_type, crate::types::Type::Function(_)) {
                                            // 是函数指针字段调用，生成函数指针调用代码
                                            return self.generate_member_func_ptr_call(
                                                member,
                                                &call.args,
                                                &field_type,
                                                &call.loc,
                                            );
                                        }
                                    }
                                    // 不是函数指针字段，按普通方法处理
                                    (
                                        specialized_class_name,
                                        member.member.clone(),
                                        Some(member.object.clone()),
                                        false,
                                    )
                                }
                                _ => {
                                    return Err(codegen_error_at(
                                        ErrorCodes::CODEGEN_INVALID_OPERATION,
                                        member.loc.clone(),
                                        format!(
                                            "Cannot call method '{}' on non-class type",
                                            member.member
                                        ),
                                    ));
                                }
                            }
                        } else {
                            return Err(codegen_error_at(
                                ErrorCodes::CODEGEN_INVALID_OPERATION,
                                member.loc.clone(),
                                format!(
                                    "Cannot determine type for method call '{}'",
                                    member.member
                                ),
                            ));
                        }
                    }
                }
            }
            _ => {
                return Err(codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    call.loc.clone(),
                    "Invalid function call".to_string(),
                ));
            }
        };

        // 泛型静态工厂调用的单态化：形如 `Optional<int> x = Optional.of(42)`，
        // 调用点的 class_name 只有裸类名 "Optional"，若不特化则解析到未定义的
        // 类型擦除基础模板 Optional_T_.of。此处依据变量声明的期望类型 Optional<int>
        // 推断类型参数（T=int），将 class_name 特化为 "Optional<int>" 并安装类型映射，
        // 使调用解析到单态化版本 Optional_int_.of。
        let mut class_name = class_name;
        // 若 class_name 含未解析的泛型参数（如 WeakPtr<Tracked> 方法体内出现
        // `Optional<Rc<T>>.of(...)`），用当前 generic_type_args 替换，避免链接到
        // 未定义的类型擦除基础模板。
        if class_name.contains('<') {
            class_name = crate::codegen::specialization::substitute_type_args_in_class_name(
                &class_name,
                &self.generic_type_args,
            );
        }

        if is_static_call && !class_name.contains('<') {
            if let Some(crate::types::Type::Generic(exp_base, exp_args)) = &expected_static_type {
                if !exp_args.is_empty() {
                    let exp_base_bare = exp_base.rsplit("::").next().unwrap_or(exp_base);
                    let cn_bare = class_name
                        .rsplit("::")
                        .next()
                        .unwrap_or(&class_name)
                        .to_string();
                    if exp_base_bare == cn_bare {
                        let type_params = self
                            .type_registry
                            .as_ref()
                            .and_then(|r| r.get_class(&cn_bare))
                            .map(|c| c.type_params.clone());
                        if let Some(params) = type_params {
                            if !params.is_empty() {
                                for (idx, param) in params.iter().enumerate() {
                                    let resolved_arg = exp_args
                                        .get(idx)
                                        .cloned()
                                        .or_else(|| param.default_type.clone())
                                        .unwrap_or_else(|| {
                                            crate::types::Type::GenericParam(param.name.clone())
                                        });
                                    self.generic_type_args
                                        .insert(param.name.clone(), resolved_arg);
                                }
                                let args_str: Vec<String> =
                                    exp_args.iter().map(|t| format!("{}", t)).collect();
                                class_name = format!("{}<{}>", cn_bare, args_str.join(", "));
                            }
                        }
                    }
                }
            }
        }

        // 安装接收者的具体泛型类型参数映射，使方法签名中的泛型参数（如参数/返回
        // 类型 T）能在参数类型解析与转换之前就解析为具体类型。静态泛型工厂调用的
        // 映射已在上文按期望类型安装。调用结束后恢复到进入本函数时的快照。
        self.install_receiver_generic_args(&obj_expr);

        // 检查是否有命名参数需要重排
        let has_named_args = call.args.iter().any(|a| matches!(a, Expr::NamedArg(_)));
        let resolved_args: Vec<Expr>;
        let actual_args: &[Expr] = if has_named_args {
            // 获取方法形参以进行重排
            let params = self
                .get_method_params(&class_name, &method_name)
                .ok_or_else(|| {
                    codegen_error_at(
                        ErrorCodes::CODEGEN_INVALID_OPERATION,
                        call.loc.clone(),
                        format!(
                            "Cannot resolve parameters for '{}' to process named arguments",
                            method_name
                        ),
                    )
                })?;
            let resolved = resolve_call_args(&call.args, &params).map_err(|msg| {
                codegen_error_at(ErrorCodes::CODEGEN_INVALID_OPERATION, call.loc.clone(), msg)
            })?;
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

        // 判断目标类型是否是 struct，决定 this 指针类型
        let is_struct_target = self.is_struct_type(&class_name);
        let this_llvm_type = if is_struct_target {
            let base_name = if let Some(pos) = class_name.find('<') {
                &class_name[..pos]
            } else {
                &class_name
            };
            format!("%struct.{}*", base_name)
        } else {
            "i8*".to_string()
        };

        // 为实例方法添加 this 参数
        let mut final_args = Vec::new();
        // 缓存 obj_expr 的求值结果，避免链式调用时重复生成内层链式表达式。
        // 下方 final_args 构建和 resolved_this_val 计算都需要 obj 的值，
        // 若 obj_expr 是带副作用的链式调用（如 sb.append("a").append("b")），
        // 重复求值会导致重复执行，缓存一次即可。
        let mut cached_obj_val: Option<String> = None;

        if is_instance_method {
            // 获取 this 指针
            if let Some(obj) = &obj_expr {
                // 检查是否是 super 标识符
                if let Expr::Identifier(name) = obj.as_ref() {
                    if name.as_ref() == "super" {
                        // super.methodName() 使用 this 指针
                        if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                            let this_temp = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = load {}, {}* %{}, align 8",
                                this_temp, this_llvm_type, this_llvm_type, this_llvm_name
                            ));
                            final_args.push(format!("{} {}", this_llvm_type, this_temp));
                        } else {
                            final_args.push(format!("{} null", this_llvm_type));
                        }
                    } else {
                        // 通过对象表达式获取 this 指针（如 obj1.getId()）
                        let obj_result = self.generate_expression(obj)?;
                        let (obj_type, obj_val) = self.parse_typed_value(&obj_result);
                        cached_obj_val = Some(obj_val.clone());
                        if is_struct_target && obj_type.starts_with("%struct.") {
                            final_args.push(format!("{} {}", obj_type, obj_val));
                        } else {
                            final_args.push(format!("{} {}", this_llvm_type, obj_val));
                        }
                    }
                } else {
                    // 通过对象表达式获取 this 指针（链式调用等）
                    let obj_result = self.generate_expression(obj)?;
                    let (obj_type, obj_val) = self.parse_typed_value(&obj_result);
                    cached_obj_val = Some(obj_val.clone());
                    if is_struct_target && obj_type.starts_with("%struct.") {
                        final_args.push(format!("{} {}", obj_type, obj_val));
                    } else {
                        final_args.push(format!("{} {}", this_llvm_type, obj_val));
                    }
                }
            } else if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                // 通过当前方法的 this 获取（如在实例方法中调用其他实例方法）
                let this_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = load {}, {}* %{}, align 8",
                    this_temp, this_llvm_type, this_llvm_type, this_llvm_name
                ));
                final_args.push(format!("{} {}", this_llvm_type, this_temp));
            } else {
                // 在静态方法中调用实例方法且没有对象表达式，使用 null 作为 this
                final_args.push(format!("{} null", this_llvm_type));
            }
        }

        // 获取方法的参数类型信息以进行必要的类型转换
        let param_types = self.get_method_param_types(
            &class_name,
            &method_name,
            &processed_args,
            has_varargs_array,
        );

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
        let fn_name = self.generate_function_name(
            &class_name,
            &method_name,
            &processed_args,
            has_varargs_array,
        );

        // 获取方法的返回类型
        let ret_type = self.get_method_return_type(
            &class_name,
            &method_name,
            &processed_args,
            has_varargs_array,
        );
        let llvm_ret_type = self.type_to_llvm(&ret_type);

        // 预先计算 this 指针值（用于 vtable 分派和直接调用都可能需要）
        // 对于非 super、非标识符的 obj_expr（链式调用），使用 final_args 构建
        // 阶段已缓存的求值结果，避免重复生成带副作用的链式表达式代码。
        let resolved_this_val = if is_static_call {
            None
        } else if let Some(obj) = &obj_expr {
            if let Expr::Identifier(name) = obj.as_ref() {
                if name.as_ref() == "super" {
                    if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                        let this_temp = self.new_temp();
                        self.emit_line(&format!(
                            "  {} = load i8*, i8** %{}, align 8",
                            this_temp, this_llvm_name
                        ));
                        Some(this_temp)
                    } else {
                        None
                    }
                } else if let Some(ref cached_val) = cached_obj_val {
                    Some(cached_val.clone())
                } else {
                    None
                }
            } else if let Some(ref cached_val) = cached_obj_val {
                Some(cached_val.clone())
            } else {
                None
            }
        } else if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
            let this_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = load i8*, i8** %{}, align 8",
                this_temp, this_llvm_name
            ));
            Some(this_temp)
        } else {
            None
        };

        // 检查是否需要 vtable 间接调用
        // 条件：是实例方法，有可用的 this 指针，类有 vtable 布局，且方法不是 private
        // private 方法不需要动态分派，直接调用即可
        let is_private = self.is_private_method(&class_name, &method_name);
        let is_interface_dispatch = !is_static_call && self.is_interface_type(&class_name);
        let has_dispatch_slot = if is_interface_dispatch {
            self.interface_has_vtable_slot(&class_name, &method_name, &param_types)
        } else {
            self.class_has_vtable(&class_name)
        };
        let needs_vtable_dispatch =
            is_instance_method && resolved_this_val.is_some() && has_dispatch_slot && !is_private;

        let call_result = if needs_vtable_dispatch {
            let this_val = resolved_this_val.unwrap();

            // 计算 vtable 指针位置（this + 8）
            let vtable_ptr_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr i8, i8* {}, i64 8",
                vtable_ptr_temp, this_val
            ));

            // 加载 vtable 指针
            let vtable_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = load i8*, i8* {}",
                vtable_temp, vtable_ptr_temp
            ));

            let slot = if is_interface_dispatch {
                self.get_interface_vtable_slot(&class_name, &method_name, &param_types)
            } else {
                self.get_vtable_slot(&class_name, &method_name, &param_types)
            };

            // 将 vtable 指针转换为 i8** 数组（每个元素是 i8*）
            let vtable_array_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast i8* {} to i8**",
                vtable_array_temp, vtable_temp
            ));

            // 从 vtable 加载函数指针（slot * 8 偏移）
            let slot_offset_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr i8*, i8** {}, i64 {}",
                slot_offset_temp, vtable_array_temp, slot
            ));
            let fn_ptr_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = load i8*, i8** {}",
                fn_ptr_temp, slot_offset_temp
            ));

            // 将 i8* 转换为正确的函数指针类型
            let fn_type = self.build_function_type_string(
                &ret_type,
                &processed_args,
                &class_name,
                &method_name,
            );
            let fn_ptr_cast_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast i8* {} to {}",
                fn_ptr_cast_temp, fn_ptr_temp, fn_type
            ));

            // 间接调用
            if llvm_ret_type == "void" {
                self.emit_line(&format!(
                    "  call void {}({})",
                    fn_ptr_cast_temp,
                    final_args.join(", ")
                ));
                Ok("void %dummy".to_string())
            } else {
                let temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = call {} {}({})",
                    temp,
                    llvm_ret_type,
                    fn_ptr_cast_temp,
                    final_args.join(", ")
                ));
                Ok(format!("{} {}", llvm_ret_type, temp))
            }
        } else {
            // 直接调用
            if llvm_ret_type == "void" {
                self.emit_line(&format!(
                    "  call void @{}({})",
                    fn_name,
                    final_args.join(", ")
                ));
                Ok("void %dummy".to_string())
            } else {
                let temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = call {} @{}({})",
                    temp,
                    llvm_ret_type,
                    fn_name,
                    final_args.join(", ")
                ));
                Ok(format!("{} {}", llvm_ret_type, temp))
            }
        };

        // 恢复调用前的泛型类型参数映射
        self.generic_type_args = entry_generic_args;
        call_result
    }
}
