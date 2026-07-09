//! 函数调用类型推断

use super::super::analyzer::SemanticAnalyzer;
use super::helpers::semantic_error_at_loc;
use crate::ast::*;
use crate::miette_diagnostic::semantic_error;
use crate::types::Type;

impl SemanticAnalyzer {
    /// 推断函数调用类型
    pub(crate) fn infer_call_type(
        &mut self,
        call: &CallExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 首先处理标识符调用（内置函数、extern函数、方法调用等）
        // 这需要在函数指针检查之前，因为函数指针变量也是标识符
        // 但我们需要先检查是否是已知的函数名
        if let Expr::Identifier(name) = call.callee.as_ref() {
            // 内置输入函数的类型推断
            match name.as_str() {
                "print" | "println" | "eprint" | "eprintln" => {
                    // 推断所有参数类型以触发类型检查（包括访问控制）
                    for arg in &call.args {
                        self.infer_expr_type_collect_errors(arg);
                    }
                    return Ok(Type::Void);
                }
                "exit" => {
                    if call.args.len() != 1 {
                        return Err(semantic_error_at_loc(
                            &call.loc,
                            format!(
                                "Function 'exit' requires 1 argument, but got {}",
                                call.args.len()
                            ),
                        ));
                    }
                    self.infer_expr_type_collect_errors(&call.args[0]);
                    return Ok(Type::Void);
                }
                "readInt" => return Ok(Type::Int32),
                "readLong" => return Ok(Type::Int64),
                "readFloat" => return Ok(Type::Float32),
                "readDouble" => return Ok(Type::Float64),
                "readLine" => return Ok(Type::String),
                "readChar" => return Ok(Type::Char),
                "readBool" => return Ok(Type::Bool),
                // 运行时辅助函数
                "__cay_read_ptr" => {
                    // 检查参数数量
                    if call.args.len() != 1 {
                        return Err(semantic_error_at_loc(
                            &call.loc,
                            format!(
                                "Function '__cay_read_ptr' requires 1 argument, but got {}",
                                call.args.len()
                            ),
                        ));
                    }
                    return Ok(Type::Int64);
                }
                "__cay_ptr_to_string" => {
                    // 检查参数数量
                    if call.args.len() != 1 {
                        return Err(semantic_error_at_loc(
                            &call.loc,
                            format!(
                                "Function '__cay_ptr_to_string' requires 1 argument, but got {}",
                                call.args.len()
                            ),
                        ));
                    }
                    return Ok(Type::String);
                }
                "__cay_write_ptr" => {
                    // 检查参数数量
                    if call.args.len() != 2 {
                        return Err(semantic_error_at_loc(
                            &call.loc,
                            format!(
                                "Function '__cay_write_ptr' requires 2 arguments, but got {}",
                                call.args.len()
                            ),
                        ));
                    }
                    return Ok(Type::Void);
                }
                "__cay_write_int" => {
                    // 检查参数数量
                    if call.args.len() != 2 {
                        return Err(semantic_error_at_loc(
                            &call.loc,
                            format!(
                                "Function '__cay_write_int' requires 2 arguments, but got {}",
                                call.args.len()
                            ),
                        ));
                    }
                    return Ok(Type::Void);
                }
                _ => {}
            }

            // 5.3.0: 支持省略 new 的类实例化 ClassName(args) / ClassName<T>(args)
            // 当标识符是类名且不被值绑定遮蔽时，将函数调用语法解释为对象创建
            if let Some(class_name) = self.try_resolve_class_instantiation(name.as_ref()) {
                let new_expr = NewExpr {
                    class_name,
                    args: call.args.clone(),
                    loc: call.loc.clone(),
                };
                return self.infer_new_type(&new_expr);
            }

            // 5.3.0: 支持命名空间式静态类方法调用 ClassName::staticMethod(args)
            // 当标识符形如 ClassName::methodName 且前缀为类、后缀为静态方法时，
            // 将其重写为 ClassName.staticMethod(args) 进行类型推断
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
                return self.infer_call_type(&member_call);
            }

            // 检查是否是 extern 函数（全局函数）
            // 注意：如果extern函数有别名，只能通过别名调用
            let extern_func_info = if let Some(ref prog) = self.program {
                let mut found_func = None;
                for extern_decl in &prog.extern_declarations {
                    for extern_func in &extern_decl.functions {
                        // 检查是否匹配：有别名的按别名匹配，没别名的按原名匹配
                        let is_match = match &extern_func.alias {
                            Some(alias) => alias == name.as_ref(),
                            None => extern_func.name == name.as_ref(),
                        };

                        if is_match {
                            // 检查参数数量（不包括可变参数）
                            let fixed_param_count =
                                extern_func.params.iter().filter(|p| !p.is_varargs).count();
                            let has_varargs = extern_func.params.iter().any(|p| p.is_varargs);

                            if has_varargs {
                                // 可变参数函数：参数数量 >= 固定参数数量
                                if call.args.len() < fixed_param_count {
                                    return Err(semantic_error_at_loc(
                                        &call.loc,
                                        format!(
                                            "Function '{}' requires at least {} arguments, but got {}",
                                            name,
                                            fixed_param_count,
                                            call.args.len()
                                        ),
                                    ));
                                }
                            } else {
                                // 非可变参数函数：参数数量必须匹配
                                if call.args.len() != extern_func.params.len() {
                                    return Err(semantic_error_at_loc(
                                        &call.loc,
                                        format!(
                                            "Function '{}' requires {} arguments, but got {}",
                                            name,
                                            extern_func.params.len(),
                                            call.args.len()
                                        ),
                                    ));
                                }
                            }

                            found_func =
                                Some((extern_func.return_type.clone(), extern_func.params.clone()));
                            break;
                        }
                    }
                    if found_func.is_some() {
                        break;
                    }
                }
                found_func
            } else {
                None
            };

            // 在可变借用self之前检查extern函数参数类型
            if let Some((return_type, params)) = extern_func_info {
                // 检查参数类型兼容性
                for (i, (arg, param)) in call.args.iter().zip(params.iter()).enumerate() {
                    if param.is_varargs {
                        break; // 可变参数后面不再检查
                    }
                    let arg_type = self.infer_expr_type_internal(arg)?;
                    if !self.types_compatible(&arg_type, &param.param_type) {
                        return Err(semantic_error_at_loc(
                            &call.loc,
                            format!(
                                "Argument {} type mismatch: expected {}, got {}",
                                i + 1,
                                param.param_type,
                                arg_type
                            ),
                        ));
                    }
                }
                return Ok(return_type);
            }

            // 尝试查找当前类的方法（无对象调用）- 支持方法重载、命名参数和继承
            if let Some(ref current_class) = self.current_class.clone() {
                // 收集当前类及其所有父类的候选方法
                let mut candidate_methods: Vec<(Type, Vec<crate::types::ParameterInfo>, bool)> =
                    Vec::new();
                let mut class_to_check = Some(current_class.clone());

                while let Some(class_name) = class_to_check {
                    if let Some(class_info) = self.type_registry.get_class(&class_name) {
                        // 收集当前类中的匹配方法
                        if let Some(methods) = class_info.methods.get(name.as_ref()) {
                            for method in methods.iter() {
                                candidate_methods.push((
                                    method.return_type.clone(),
                                    method.params.clone(),
                                    method.is_static,
                                ));
                            }
                        }
                        // 继续检查父类
                        class_to_check = class_info.parent.clone();
                    } else {
                        break;
                    }
                }

                // 第一步：尝试精确匹配（参数类型完全相同）
                for (return_type, params, is_static) in &candidate_methods {
                    // 检查：静态方法中不能调用实例方法
                    if self.current_method_is_static && !is_static {
                        continue;
                    }
                    if self.check_arguments_exact(&call.args, params) {
                        return Ok(return_type.clone());
                    }
                }

                // 第二步：尝试兼容匹配（允许隐式类型转换）
                for (return_type, params, is_static) in &candidate_methods {
                    // 检查：静态方法中不能调用实例方法
                    if self.current_method_is_static && !is_static {
                        continue;
                    }
                    if let Ok(()) = self.check_arguments_compatible(
                        &call.args,
                        params,
                        call.loc.line,
                        call.loc.column,
                    ) {
                        return Ok(return_type.clone());
                    }
                }
            }

            // 如果找不到任何合适的方法，尝试查找顶层函数
            // 先收集顶层函数信息，避免借用冲突
            let top_level_func_info = if let Some(program) = &self.program {
                program
                    .top_level_functions
                    .iter()
                    .find(|func| func.name == name.as_ref())
                    .map(|func| (func.params.clone(), func.return_type.clone()))
            } else {
                None
            };

            if let Some((params, return_type)) = top_level_func_info {
                // 找到顶层函数，检查参数类型兼容性
                if let Err(msg) = self.check_arguments_compatible(
                    &call.args,
                    &params,
                    call.loc.line,
                    call.loc.column,
                ) {
                    return Err(semantic_error_at_loc(&call.loc, msg));
                }
                return Ok(return_type);
            }
        }

        // 支持成员调用: obj.method(...) 或 ClassName.method()（静态方法）
        if let Expr::MemberAccess(member) = call.callee.as_ref() {
            if let Some(return_type) = self.infer_static_or_enum_member_call(member, call)? {
                return Ok(return_type);
            }

            // 推断对象类型
            let obj_type = self.infer_expr_type_internal(&member.object)?;

            // 处理 String 类型方法调用
            if obj_type == Type::String {
                return self.infer_string_method_call(
                    &member.member,
                    &call.args,
                    call.loc.line,
                    call.loc.column,
                );
            }

            // 处理数组类型的 length() 方法调用（作为 .length 属性的语法糖）
            if let Type::Array(_) = &obj_type {
                if member.member == "length" && call.args.is_empty() {
                    return Ok(Type::Int32);
                }
            }

            // 处理基本类型的 toString() 方法调用
            if matches!(
                obj_type,
                Type::Int32 | Type::Int64 | Type::Float32 | Type::Float64 | Type::Bool | Type::Char
            ) {
                if member.member == "toString" && call.args.is_empty() {
                    return Ok(Type::String);
                }
            }

            // 处理类实例方法调用 - 支持方法重载
            // 获取类名（支持 Type::Object 和 Type::Generic）
            // 对于泛型类型如 "Wrapper<int>"，需要解析出基础类名 "Wrapper"
            let class_name_opt = match &obj_type {
                Type::Object(class_name) => {
                    // 解析泛型类名: "Wrapper<int>" -> "Wrapper"
                    if let Some(pos) = class_name.find('<') {
                        Some(class_name[..pos].to_string())
                    } else {
                        Some(class_name.clone())
                    }
                }
                Type::Generic(class_name, _) => Some(class_name.clone()),
                Type::GenericParam(param_name) => {
                    // 泛型类型参数：使用 bound（默认为 Object）查找方法
                    self.current_class_type_params
                        .iter()
                        .find(|p| &p.name == param_name)
                        .and_then(|p| p.bound.clone())
                        .or(Some("Object".to_string()))
                }
                _ => None,
            };

            if let Some(class_name) = class_name_opt {
                // 先推断所有参数类型
                let mut arg_types = Vec::new();
                for arg in &call.args {
                    arg_types.push(self.infer_expr_type_internal(arg)?);
                }

                // 首先检查是否是函数指针字段调用
                // 查找类的字段，看是否是函数指针类型
                if let Some(class_info) = self.type_registry.get_class(&class_name) {
                    if let Some(field_info) = class_info.fields.get(&member.member) {
                        if let Type::Function(func_type) = &field_info.field_type {
                            // 是函数指针字段调用
                            let return_type = *func_type.return_type.clone();
                            let params = func_type.params.clone();
                            // 检查参数数量
                            if call.args.len() != params.len() {
                                return Err(semantic_error_at_loc(
                                    &call.loc,
                                    format!(
                                        "Function pointer field '{}' requires {} arguments, but got {}",
                                        member.member,
                                        params.len(),
                                        call.args.len()
                                    ),
                                ));
                            }
                            // 检查参数类型兼容性（手动检查，因为params是Vec<Type>而不是Vec<ParameterInfo>）
                            for (i, (arg, expected_type)) in
                                call.args.iter().zip(params.iter()).enumerate()
                            {
                                let arg_type = self.infer_expr_type_internal(arg)?;
                                if !self.types_compatible(&arg_type, expected_type) {
                                    return Err(semantic_error_at_loc(
                                        &call.loc,
                                        format!(
                                            "Argument {} type mismatch: expected {}, got {}",
                                            i + 1,
                                            expected_type,
                                            arg_type
                                        ),
                                    ));
                                }
                            }
                            return Ok(return_type);
                        }
                    }
                }

                // 使用参数类型查找匹配的方法
                // 首先尝试直接使用类名查找
                let method_result = if let Some(method_info) =
                    self.type_registry
                        .find_method(&class_name, &member.member, &arg_types)
                {
                    Some((
                        class_name.clone(),
                        method_info.clone(),
                        method_info.return_type.clone(),
                        method_info.params.clone(),
                    ))
                } else {
                    // 如果直接查找失败，尝试查找限定类名
                    if let Some(qualified_name) =
                        self.type_registry.find_qualified_class(&class_name)
                    {
                        self.type_registry
                            .find_method(&qualified_name, &member.member, &arg_types)
                            .map(|m| {
                                (
                                    qualified_name.clone(),
                                    m.clone(),
                                    m.return_type.clone(),
                                    m.params.clone(),
                                )
                            })
                    } else {
                        None
                    }
                };

                if let Some((owner_class, method_info, return_type, params)) = method_result {
                    // 检查方法访问权限
                    super::helpers::check_member_access(
                        &member.member,
                        method_info.is_public,
                        method_info.is_protected,
                        method_info.is_private,
                        &self.current_class,
                        &owner_class,
                        &self.type_registry,
                        &member.loc,
                    )?;
                    // eprintln!("[DEBUG] Found method: {}.{}, params={:?}, return_type={:?}", class_name, member.member, params, return_type);
                    // 检查参数类型兼容性（支持可变参数）
                    if let Err(msg) = self.check_arguments_compatible(
                        &call.args,
                        &params,
                        call.loc.line,
                        call.loc.column,
                    ) {
                        return Err(semantic_error_at_loc(&call.loc, msg));
                    }

                    // 如果对象是泛型类型，替换返回类型中的泛型参数。
                    // 支持 Type::Generic 和 Type::Object("Class<T>") 两种形式，
                    // 后者由 new Class<T>() 产生，需要解析字符串中的类型实参。
                    let scoped_return_type =
                        self.qualify_type_for_class(&return_type, &owner_class);
                    let obj_type_args: Option<Vec<Type>> = match &obj_type {
                        Type::Generic(_, type_args) => Some(type_args.clone()),
                        Type::Object(class_name) => {
                            // 解析 "Container<int>" 中的类型参数
                            if class_name.contains('<') && class_name.ends_with('>') {
                                if let Some(pos) = class_name.find('<') {
                                    let args_str =
                                        &class_name[pos + 1..class_name.len() - 1];
                                    let type_args: Vec<Type> = args_str
                                        .split(',')
                                        .map(|s| s.trim())
                                        .filter(|s| !s.is_empty())
                                        .map(|s| self.parse_type_string(s))
                                        .collect();
                                    if type_args.is_empty() {
                                        None
                                    } else {
                                        Some(type_args)
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        }
                        _ => None,
                    };
                    let final_return_type = if let Some(type_args) = obj_type_args {
                        if let Some(class_info) = self.type_registry.get_class(&owner_class) {
                            self.substitute_type_params(
                                &scoped_return_type,
                                &class_info.type_params,
                                &type_args,
                            )
                        } else {
                            scoped_return_type
                        }
                    } else {
                        scoped_return_type
                    };

                    return Ok(final_return_type);
                } else {
                    return Err(semantic_error_at_loc(
                        &call.loc,
                        self.unknown_method_message(&member.member, &class_name),
                    ));
                }
            }
        }

        // 检查标识符是否是函数指针变量
        if let Expr::Identifier(name) = call.callee.as_ref() {
            // 首先检查是否是函数指针变量 - 先收集类型信息避免借用冲突
            let func_ptr_info = self.symbol_table.lookup(name.as_ref()).and_then(|info| {
                if let Type::Function(func_type) = &info.symbol_type {
                    Some((func_type.params.clone(), *func_type.return_type.clone()))
                } else {
                    None
                }
            });

            if let Some((params, return_type)) = func_ptr_info {
                // 检查参数数量
                let expected_args = params.len();
                let actual_args = call.args.len();
                if actual_args != expected_args {
                    return Err(semantic_error_at_loc(
                        &call.loc,
                        format!(
                            "Function pointer call requires {} arguments, but got {}",
                            expected_args, actual_args
                        ),
                    ));
                }
                // 检查参数类型兼容性
                for (i, (arg, expected_type)) in call.args.iter().zip(params.iter()).enumerate() {
                    let arg_type = self.infer_expr_type_internal(arg)?;
                    if !self.types_compatible(&arg_type, expected_type) {
                        return Err(semantic_error_at_loc(
                            &call.loc,
                            format!(
                                "Argument {} type mismatch: expected {}, got {}",
                                i + 1,
                                expected_type,
                                arg_type
                            ),
                        ));
                    }
                }
                return Ok(return_type);
            }

            // 检查 @FreeFunction 注册的自由函数
            if let Some((_class_name, method_info, _loc)) = self
                .type_registry
                .free_functions
                .get(name.as_ref())
                .cloned()
            {
                // 验证参数（使用 check_arguments_compatible 以支持可变参数）
                if let Err(msg) = self.check_arguments_compatible(
                    &call.args,
                    &method_info.params,
                    call.loc.line,
                    call.loc.column,
                ) {
                    return Err(semantic_error_at_loc(
                        &call.loc,
                        format!("@FreeFunction '{}' {}", name, msg),
                    ));
                }
                return Ok(method_info.return_type.clone());
            }

            // 检查是否存在同名方法（参数不匹配）
            if let Some(ref current_class) = self.current_class {
                if let Some(class_info) = self.type_registry.get_class(current_class) {
                    if class_info.methods.contains_key(name.as_ref()) {
                        return Err(semantic_error_at_loc(
                            &call.loc,
                            format!(
                                "Method '{}' in class '{}' cannot be applied to given types: argument mismatch",
                                name, current_class
                            ),
                        ));
                    }
                }
            }
            return Err(semantic_error_at_loc(
                &call.loc,
                format!("Cannot find method '{}'", name),
            ));
        }

        if let Expr::MemberAccess(member) = call.callee.as_ref() {
            if let Expr::Identifier(class_name) = &*member.object {
                return Err(semantic_error_at_loc(
                    &call.loc,
                    format!(
                        "Method '{}' in class '{}' cannot be applied to given types: argument mismatch",
                        member.member, class_name
                    ),
                ));
            }
            if let Type::Object(class_name) = self.infer_expr_type_internal(&member.object)? {
                return Err(semantic_error_at_loc(
                    &call.loc,
                    format!(
                        "Method '{}' in class '{}' cannot be applied to given types: argument mismatch",
                        member.member, class_name
                    ),
                ));
            }
        }

        // 最后检查是否是函数指针类型调用: fn_ptr(args...)
        // 如果callee不是标识符或标识符不是已知函数名，则尝试作为函数指针处理
        let callee_type = self.infer_expr_type_internal(&call.callee)?;
        if let Type::Function(func_type) = &callee_type {
            // 检查参数数量
            let expected_args = func_type.params.len();
            let actual_args = call.args.len();
            if actual_args != expected_args {
                return Err(semantic_error_at_loc(
                    &call.loc,
                    format!(
                        "Function pointer call requires {} arguments, but got {}",
                        expected_args, actual_args
                    ),
                ));
            }
            // 检查参数类型兼容性
            for (i, (arg, expected_type)) in
                call.args.iter().zip(func_type.params.iter()).enumerate()
            {
                let arg_type = self.infer_expr_type_internal(arg)?;
                if !self.types_compatible(&arg_type, expected_type) {
                    return Err(semantic_error_at_loc(
                        &call.loc,
                        format!(
                            "Argument {} type mismatch: expected {}, got {}",
                            i + 1,
                            expected_type,
                            arg_type
                        ),
                    ));
                }
            }
            return Ok(*func_type.return_type.clone());
        }

        Err(semantic_error_at_loc(
            &call.loc,
            "Cannot resolve method call".to_string(),
        ))
    }

    /// 检查名称是否是 extern 函数（按别名或原名匹配）
    /// 时间复杂度: O(e * f)，e 为 extern 声明块数，f 为每块函数数
    fn is_extern_function_name(&self, name: &str) -> bool {
        if let Some(ref prog) = self.program {
            for extern_decl in &prog.extern_declarations {
                for extern_func in &extern_decl.functions {
                    let is_match = match &extern_func.alias {
                        Some(alias) => alias == name,
                        None => extern_func.name == name,
                    };
                    if is_match {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 5.3.0: 尝试将标识符调用解析为省略 new 的类实例化
    ///
    /// 返回 Some(类名) 当且仅当：
    /// - name 是已注册的类/struct/限定类名
    /// - name 不被局部变量、参数、字段或函数等值绑定遮蔽
    /// - name 不是 extern 函数
    fn try_resolve_class_instantiation(&self, name: &str) -> Option<String> {
        if self.identifier_has_value_binding(name) || self.is_extern_function_name(name) {
            return None;
        }
        let is_class = self.type_registry.class_exists(name)
            || self.type_registry.get_struct(name).is_some()
            || self.type_registry.find_qualified_class(name).is_some();
        if is_class {
            Some(name.to_string())
        } else {
            None
        }
    }

    /// 5.3.0: 尝试将形如 ClassName::methodName 的标识符解析为静态方法调用
    ///
    /// 返回 Some((类前缀, 方法名)) 当且仅当：
    /// - name 包含 '::' 且前后段非空
    /// - name 不被值绑定或 extern 函数遮蔽
    /// - 类前缀解析到一个类，且该类包含同名的静态方法
    fn try_resolve_static_method_call(&self, name: &str) -> Option<(String, String)> {
        if !name.contains("::") {
            return None;
        }
        if self.identifier_has_value_binding(name) || self.is_extern_function_name(name) {
            return None;
        }
        let (class_prefix, method_name) = name.rsplit_once("::")?;
        if class_prefix.is_empty() || method_name.is_empty() {
            return None;
        }
        let class_info = self.type_registry.get_class(class_prefix)?;
        let methods = class_info.methods.get(method_name)?;
        if methods.iter().any(|m| m.is_static) {
            Some((class_prefix.to_string(), method_name.to_string()))
        } else {
            None
        }
    }
}
