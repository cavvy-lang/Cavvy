//! 函数调用类型推断

use super::super::analyzer::SemanticAnalyzer;
use super::helpers::semantic_error_at_loc;
use crate::ast::*;
use crate::miette_diagnostic::semantic_error;
use crate::types::Type;

impl SemanticAnalyzer {
    /// 推断函数调用类型
    ///
    /// 主函数只做流程编排：按 callee 形态依次尝试各解析阶段，
    /// 任一阶段命中即返回，全部未命中时按函数指针兜底。
    pub(crate) fn infer_call_type(
        &mut self,
        call: &CallExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 阶段一：标识符调用（内置函数、类实例化、静态方法重写、extern、当前类方法、顶层函数）
        // 这需要在函数指针检查之前，因为函数指针变量也是标识符
        // 但我们需要先检查是否是已知的函数名
        if let Expr::Identifier(name) = call.callee.as_ref() {
            if let Some(result) = self.try_infer_identifier_call(call, name) {
                return result;
            }
        }

        // 阶段二：成员调用 obj.method(...) 或 ClassName.method()（静态方法）
        if let Expr::MemberAccess(member) = call.callee.as_ref() {
            if let Some(return_type) = self.try_infer_member_call(member, call)? {
                return Ok(return_type);
            }
        }

        // 阶段三：标识符兜底（函数指针变量、@FreeFunction、未找到方法的错误）
        if let Expr::Identifier(name) = call.callee.as_ref() {
            return self.infer_identifier_call_fallback(call, name);
        }

        // 阶段四：成员调用的参数不匹配错误
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

        // 阶段五：函数指针兜底 fn_ptr(args...)
        // 如果callee不是标识符或标识符不是已知函数名，则尝试作为函数指针处理
        self.infer_function_pointer_fallback(call)
    }

    /// 阶段一：尝试将标识符调用解析为已知函数/方法
    ///
    /// 依次尝试：内置函数、省略 new 的类实例化、静态方法重写、
    /// extern 函数、当前类方法（含继承与重载）、顶层函数。
    /// 返回 None 表示均不命中，由后续阶段继续处理。
    fn try_infer_identifier_call(
        &mut self,
        call: &CallExpr,
        name: &IdentifierExpr,
    ) -> Option<crate::miette_diagnostic::CayResult<Type>> {
        // 内置输入函数的类型推断
        if let Some(result) = self.try_infer_builtin_call(call, name) {
            return Some(result);
        }

        // 5.3.0: 支持省略 new 的类实例化 ClassName(args) / ClassName<T>(args)
        // 当标识符是类名且不被值绑定遮蔽时，将函数调用语法解释为对象创建
        if let Some(class_name) = self.try_resolve_class_instantiation(name.as_ref()) {
            let new_expr = NewExpr {
                class_name,
                args: call.args.clone(),
                loc: call.loc.clone(),
            };
            return Some(self.infer_new_type(&new_expr));
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
            return Some(self.infer_call_type(&member_call));
        }

        // 检查是否是 extern 函数（全局函数）
        // 注意：如果extern函数有别名，只能通过别名调用
        if let Some(result) = self.try_infer_extern_call(call, name) {
            return Some(result);
        }

        // 尝试查找当前类的方法（无对象调用）- 支持方法重载、命名参数和继承
        if let Some(result) = self.try_infer_current_class_method_call(call, name) {
            return Some(result);
        }

        // 如果找不到任何合适的方法，尝试查找顶层函数
        self.try_infer_top_level_call(call, name)
    }

    /// 内置函数（print/exit/panic/read*/__cay_* 等）的类型推断
    fn try_infer_builtin_call(
        &mut self,
        call: &CallExpr,
        name: &IdentifierExpr,
    ) -> Option<crate::miette_diagnostic::CayResult<Type>> {
        match name.as_str() {
            "print" | "println" | "eprint" | "eprintln" => {
                // 推断所有参数类型以触发类型检查（包括访问控制）
                for arg in &call.args {
                    self.infer_expr_type_collect_errors(arg);
                }
                Some(Ok(Type::Void))
            }
            "exit" => {
                if call.args.len() != 1 {
                    return Some(Err(semantic_error_at_loc(
                        &call.loc,
                        format!(
                            "Function 'exit' requires 1 argument, but got {}",
                            call.args.len()
                        ),
                    )));
                }
                self.infer_expr_type_collect_errors(&call.args[0]);
                Some(Ok(Type::Void))
            }
            // 6.1.0: panic/abort 内置函数
            "panic" | "abort" => {
                if call.args.len() != 1 {
                    return Some(Err(semantic_error_at_loc(
                        &call.loc,
                        format!(
                            "Function '{}' requires 1 argument, but got {}",
                            name, call.args.len()
                        ),
                    )));
                }
                self.infer_expr_type_collect_errors(&call.args[0]);
                Some(Ok(Type::Void))
            }
            "readInt" => Some(Ok(Type::Int32)),
            "readLong" => Some(Ok(Type::Int64)),
            "readFloat" => Some(Ok(Type::Float32)),
            "readDouble" => Some(Ok(Type::Float64)),
            "readLine" => Some(Ok(Type::String)),
            "readChar" => Some(Ok(Type::Char)),
            "readBool" => Some(Ok(Type::Bool)),
            // 运行时辅助函数
            "__cay_read_ptr" => {
                // 检查参数数量
                if call.args.len() != 1 {
                    return Some(Err(semantic_error_at_loc(
                        &call.loc,
                        format!(
                            "Function '__cay_read_ptr' requires 1 argument, but got {}",
                            call.args.len()
                        ),
                    )));
                }
                Some(Ok(Type::Int64))
            }
            "__cay_ptr_to_string" => {
                // 检查参数数量
                if call.args.len() != 1 {
                    return Some(Err(semantic_error_at_loc(
                        &call.loc,
                        format!(
                            "Function '__cay_ptr_to_string' requires 1 argument, but got {}",
                            call.args.len()
                        ),
                    )));
                }
                Some(Ok(Type::String))
            }
            "__cay_write_ptr" => {
                // 检查参数数量
                if call.args.len() != 2 {
                    return Some(Err(semantic_error_at_loc(
                        &call.loc,
                        format!(
                            "Function '__cay_write_ptr' requires 2 arguments, but got {}",
                            call.args.len()
                        ),
                    )));
                }
                Some(Ok(Type::Void))
            }
            "__cay_write_int" => {
                // 检查参数数量
                if call.args.len() != 2 {
                    return Some(Err(semantic_error_at_loc(
                        &call.loc,
                        format!(
                            "Function '__cay_write_int' requires 2 arguments, but got {}",
                            call.args.len()
                        ),
                    )));
                }
                Some(Ok(Type::Void))
            }
            "__cay_array_base" => {
                // 检查参数数量
                if call.args.len() != 1 {
                    return Some(Err(semantic_error_at_loc(
                        &call.loc,
                        format!(
                            "Function '__cay_array_base' requires 1 argument, but got {}",
                            call.args.len()
                        ),
                    )));
                }
                self.infer_expr_type_collect_errors(&call.args[0]);
                Some(Ok(Type::Int64))
            }
            _ => None,
        }
    }

    /// 尝试将标识符调用解析为 extern 函数（全局函数）
    ///
    /// 注意：如果extern函数有别名，只能通过别名调用
    fn try_infer_extern_call(
        &mut self,
        call: &CallExpr,
        name: &IdentifierExpr,
    ) -> Option<crate::miette_diagnostic::CayResult<Type>> {
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
                                return Some(Err(semantic_error_at_loc(
                                    &call.loc,
                                    format!(
                                        "Function '{}' requires at least {} arguments, but got {}",
                                        name,
                                        fixed_param_count,
                                        call.args.len()
                                    ),
                                )));
                            }
                        } else {
                            // 非可变参数函数：参数数量必须匹配
                            if call.args.len() != extern_func.params.len() {
                                return Some(Err(semantic_error_at_loc(
                                    &call.loc,
                                    format!(
                                        "Function '{}' requires {} arguments, but got {}",
                                        name,
                                        extern_func.params.len(),
                                        call.args.len()
                                    ),
                                )));
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
        let (return_type, params) = extern_func_info?;
        // 检查参数类型兼容性
        for (i, (arg, param)) in call.args.iter().zip(params.iter()).enumerate() {
            if param.is_varargs {
                break; // 可变参数后面不再检查
            }
            let arg_type = match self.infer_expr_type_internal(arg) {
                Ok(arg_type) => arg_type,
                Err(e) => return Some(Err(e)),
            };
            if !self.types_compatible(&arg_type, &param.param_type) {
                return Some(Err(semantic_error_at_loc(
                    &call.loc,
                    format!(
                        "Argument {} type mismatch: expected {}, got {}",
                        i + 1,
                        param.param_type,
                        arg_type
                    ),
                )));
            }
        }
        Some(Ok(return_type))
    }

    /// 尝试查找当前类的方法（无对象调用）- 支持方法重载、命名参数和继承
    fn try_infer_current_class_method_call(
        &mut self,
        call: &CallExpr,
        name: &IdentifierExpr,
    ) -> Option<crate::miette_diagnostic::CayResult<Type>> {
        let current_class = self.current_class.clone()?;

        // 收集当前类及其所有父类的候选方法
        let mut candidate_methods: Vec<(Type, Vec<crate::types::ParameterInfo>, bool)> =
            Vec::new();
        let mut class_to_check = Some(current_class);

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
        // 试探期间使用错误缓冲：失败候选产生的错误会被丢弃，避免级联误报；
        // 只有最终选定的候选产生的错误（如实参内部的未定义标识符）才保留。
        let mut selected_return_type: Option<Type> = None;
        for (return_type, params, is_static) in &candidate_methods {
            // 检查：静态方法中不能调用实例方法
            if self.current_method_is_static && !is_static {
                continue;
            }
            let error_checkpoint = self.errors.len();
            if self.check_arguments_exact(&call.args, params) {
                selected_return_type = Some(return_type.clone());
                break;
            }
            // 试探失败：回滚该候选产生的错误
            self.errors.truncate(error_checkpoint);
        }

        // 第二步：尝试兼容匹配（允许隐式类型转换）
        if selected_return_type.is_none() {
            for (return_type, params, is_static) in &candidate_methods {
                // 检查：静态方法中不能调用实例方法
                if self.current_method_is_static && !is_static {
                    continue;
                }
                let error_checkpoint = self.errors.len();
                if let Ok(()) = self.check_arguments_compatible(
                    &call.args,
                    params,
                    call.loc.line,
                    call.loc.column,
                ) {
                    selected_return_type = Some(return_type.clone());
                    break;
                }
                // 试探失败：回滚该候选产生的错误
                self.errors.truncate(error_checkpoint);
            }
        }

        selected_return_type.map(Ok)
    }

    /// 尝试将标识符调用解析为顶层函数
    fn try_infer_top_level_call(
        &mut self,
        call: &CallExpr,
        name: &IdentifierExpr,
    ) -> Option<crate::miette_diagnostic::CayResult<Type>> {
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

        let (params, return_type) = top_level_func_info?;
        // 找到顶层函数，检查参数类型兼容性
        if let Err(msg) = self.check_arguments_compatible(
            &call.args,
            &params,
            call.loc.line,
            call.loc.column,
        ) {
            return Some(Err(semantic_error_at_loc(&call.loc, msg)));
        }
        Some(Ok(return_type))
    }

    /// 阶段二：成员调用 obj.method(...) 或 ClassName.method()（静态方法）
    ///
    /// 返回 Ok(None) 表示对象类型不支持方法调用，由后续阶段继续处理。
    fn try_infer_member_call(
        &mut self,
        member: &MemberAccessExpr,
        call: &CallExpr,
    ) -> crate::miette_diagnostic::CayResult<Option<Type>> {
        if let Some(return_type) = self.infer_static_or_enum_member_call(member, call)? {
            return Ok(Some(return_type));
        }

        // 推断对象类型
        let obj_type = self.infer_expr_type_internal(&member.object)?;

        // 处理 String 类型方法调用
        if obj_type == Type::String {
            return self
                .infer_string_method_call(
                    &member.member,
                    &call.args,
                    call.loc.line,
                    call.loc.column,
                )
                .map(Some);
        }

        // 处理数组类型的 length() 方法调用（作为 .length 属性的语法糖）
        if let Type::Array(_) = &obj_type {
            if member.member == "length" && call.args.is_empty() {
                return Ok(Some(Type::Int32));
            }
        }

        // 处理基本类型的 toString() 方法调用
        if matches!(
            obj_type,
            Type::Int32 | Type::Int64 | Type::Float32 | Type::Float64 | Type::Bool | Type::Char
        ) {
            if member.member == "toString" && call.args.is_empty() {
                return Ok(Some(Type::String));
            }
        }

        // 处理类实例方法调用 - 支持方法重载
        let class_name = match self.method_receiver_class_name(&obj_type) {
            Some(class_name) => class_name,
            None => return Ok(None),
        };

        // 先推断所有参数类型
        let mut arg_types = Vec::new();
        for arg in &call.args {
            arg_types.push(self.infer_expr_type_internal(arg)?);
        }

        // 首先检查是否是函数指针字段调用
        if let Some(return_type) = self.try_infer_func_ptr_field_call(call, member, &class_name)? {
            return Ok(Some(return_type));
        }

        // 使用参数类型查找匹配的方法
        // 首先尝试直接使用类名查找，失败后尝试查找限定类名
        let method_result = self.lookup_instance_method(&class_name, &member.member, &arg_types);

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
            // 检查参数类型兼容性（支持可变参数）
            if let Err(msg) = self.check_arguments_compatible(
                &call.args,
                &params,
                call.loc.line,
                call.loc.column,
            ) {
                return Err(semantic_error_at_loc(&call.loc, msg));
            }

            // 仅返回类型不同的重载集合（实现多个泛型接口实例化，如
            // Into<IOError>/Into<ParseError> 各提供一个 into()）：普通调用
            // 无法按返回类型分派，报歧义错误。`?` 运算符的 e.into() 转换
            // 不经过此路径（codegen 按目标错误类型静态分派），不受影响。
            if let Some(class_info) = self.type_registry.get_class(&owner_class) {
                if let Some(overloads) = class_info.methods.get(&member.member) {
                    let has_return_only_overload = overloads.iter().any(|m| {
                        m.params.len() == params.len()
                            && m
                                .params
                                .iter()
                                .zip(params.iter())
                                .all(|(a, b)| a.param_type == b.param_type)
                            && m.return_type != return_type
                    });
                    if has_return_only_overload {
                        return Err(semantic_error_at_loc(
                            &call.loc,
                            format!(
                                "对方法 '{}' 的调用有歧义：类 '{}' 存在多个仅返回类型不同的重载（分别实现不同的泛型接口实例化）\n提示: 普通调用无法按返回类型分派；该方法应由 `?` 运算符的错误转换间接调用",
                                member.member, owner_class
                            ),
                        ));
                    }
                }
            }

            // 如果对象是泛型类型，替换返回类型中的泛型参数
            let final_return_type =
                self.substitute_method_return_type(&return_type, &obj_type, &owner_class);

            Ok(Some(final_return_type))
        } else if let Some(return_type) =
            self.try_infer_instance_generic_method_call(member, call, &class_name, &obj_type)?
        {
            // 常规重载解析失败（声明形参中的方法级类型参数如 fn(T)->U 永远无法
            // 与 lambda 实参的具体 fn(int)->long 匹配），尝试方法级泛型推断。
            Ok(Some(return_type))
        } else {
            Err(semantic_error_at_loc(
                &call.loc,
                self.unknown_method_message(&member.member, &class_name),
            ))
        }
    }

    /// 尝试将成员调用解析为「带方法级类型参数的实例泛型方法」调用。
    ///
    /// 仅在常规非泛型重载解析失败后调用，因此永远不会遮蔽更匹配的非泛型重载。
    /// 对同名且声明了方法级类型参数（`method<U>(...)`）的候选：
    /// 1. 用接收者的类级类型实参替换签名中的类级类型参数；
    /// 2. 从调用实参（如 lambda 的 fn(int)->long）推断方法级类型实参（U=long）；
    /// 3. 用推断结果特化签名后做常规实参检查，返回特化后的返回类型。
    ///
    /// 返回 Ok(None) 表示没有可推断的泛型方法候选，由调用方报 unknown method。
    fn try_infer_instance_generic_method_call(
        &mut self,
        member: &MemberAccessExpr,
        call: &CallExpr,
        class_name: &str,
        obj_type: &Type,
    ) -> crate::miette_diagnostic::CayResult<Option<Type>> {
        // 收集类层次中同名且带方法级类型参数的实例方法候选
        let mut candidates: Vec<(String, crate::types::MethodInfo, Vec<crate::types::TypeParamInfo>)> =
            Vec::new();
        let mut class_to_check = Some(class_name.to_string());
        while let Some(name) = class_to_check {
            let resolved = self
                .type_registry
                .get_class(&name)
                .map(|c| c.name.clone())
                .or_else(|| self.type_registry.find_qualified_class(&name));
            let Some(resolved) = resolved else { break };
            let Some(class_info) = self.type_registry.get_class(&resolved) else {
                break;
            };
            if let Some(methods) = class_info.methods.get(&member.member) {
                for method in methods {
                    if !method.is_static && !method.type_params.is_empty() {
                        candidates.push((
                            class_info.name.clone(),
                            method.clone(),
                            class_info.type_params.clone(),
                        ));
                    }
                }
            }
            class_to_check = class_info.parent.clone();
        }
        if candidates.is_empty() {
            return Ok(None);
        }

        // 接收者的类级类型实参（如 Result<int, String> -> [int, String]）
        let receiver_args = self.receiver_class_type_args(obj_type);

        // 逐候选：先替换类级类型参数，再推断方法级类型实参并特化签名
        let mut specialized_candidates = Vec::new();
        for (owner_class, method_info, class_type_params) in &candidates {
            let mut signature = method_info.clone();
            if !class_type_params.is_empty() {
                if let Some(args) = &receiver_args {
                    signature.params = signature
                        .params
                        .iter()
                        .map(|p| crate::types::ParameterInfo {
                            name: p.name.clone(),
                            param_type: self.substitute_type_params(
                                &p.param_type,
                                class_type_params,
                                args,
                            ),
                            is_varargs: p.is_varargs,
                        })
                        .collect();
                    signature.return_type = self.substitute_type_params(
                        &signature.return_type,
                        class_type_params,
                        args,
                    );
                }
            }
            let Some(method_args) = self.infer_type_args_from_arguments(
                &signature.params,
                &call.args,
                &method_info.type_params,
            ) else {
                continue;
            };
            let specialized = self.specialize_method_info(
                &signature,
                &method_info.type_params,
                Some(&method_args),
            );
            specialized_candidates.push((owner_class.clone(), specialized));
        }

        // 精确匹配优先，其次兼容匹配（与静态路径的选择顺序一致）
        let mut mismatch_detail = None;
        for exact in [true, false] {
            for (owner_class, method_info) in &specialized_candidates {
                let matches = if exact {
                    self.check_arguments_exact(&call.args, &method_info.params)
                } else {
                    match self.check_arguments_compatible(
                        &call.args,
                        &method_info.params,
                        call.loc.line,
                        call.loc.column,
                    ) {
                        Ok(()) => true,
                        Err(msg) => {
                            if mismatch_detail.is_none() {
                                mismatch_detail = Some(msg);
                            }
                            false
                        }
                    }
                };
                if matches {
                    super::helpers::check_member_access(
                        &member.member,
                        method_info.is_public,
                        method_info.is_protected,
                        method_info.is_private,
                        &self.current_class,
                        owner_class,
                        &self.type_registry,
                        &member.loc,
                    )?;
                    return Ok(Some(
                        self.qualify_type_for_class(&method_info.return_type, owner_class),
                    ));
                }
            }
        }

        // 存在泛型方法候选但实参不匹配：报参数不匹配而非 unknown method
        if !specialized_candidates.is_empty() {
            let detail = mismatch_detail.unwrap_or_else(|| "argument mismatch".to_string());
            return Err(semantic_error_at_loc(
                &call.loc,
                format!(
                    "Method '{}' in class '{}' cannot be applied to given types: {}",
                    member.member, class_name, detail
                ),
            ));
        }

        // 存在泛型方法候选但方法级类型实参推断失败（如 r.map(42)）：
        // 报推断失败，比 unknown method 更准确。
        Err(semantic_error_at_loc(
            &call.loc,
            format!(
                "Cannot infer type arguments for generic method '{}' in class '{}' from the given arguments",
                member.member, class_name
            ),
        ))
    }

    /// 获取方法接收者的类名（支持 Type::Object 和 Type::Generic）
    ///
    /// 对于泛型类型如 "Wrapper<int>"，需要解析出基础类名 "Wrapper"
    fn method_receiver_class_name(&self, obj_type: &Type) -> Option<String> {
        match obj_type {
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
        }
    }

    /// 尝试将成员调用解析为函数指针字段调用
    ///
    /// 查找类的字段，看是否是函数指针类型
    fn try_infer_func_ptr_field_call(
        &mut self,
        call: &CallExpr,
        member: &MemberAccessExpr,
        class_name: &str,
    ) -> crate::miette_diagnostic::CayResult<Option<Type>> {
        if let Some(class_info) = self.type_registry.get_class(class_name) {
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
                    return Ok(Some(return_type));
                }
            }
        }
        Ok(None)
    }

    /// 按参数类型在类中查找实例方法
    ///
    /// 首先尝试直接使用类名查找，失败后尝试查找限定类名。
    /// 返回 (拥有者类名, 方法信息, 返回类型, 参数列表)。
    fn lookup_instance_method(
        &self,
        class_name: &str,
        member_name: &str,
        arg_types: &[Type],
    ) -> Option<(
        String,
        crate::types::MethodInfo,
        Type,
        Vec<crate::types::ParameterInfo>,
    )> {
        if let Some(method_info) =
            self.type_registry
                .find_method(class_name, member_name, arg_types)
        {
            Some((
                class_name.to_string(),
                method_info.clone(),
                method_info.return_type.clone(),
                method_info.params.clone(),
            ))
        } else if let Some(qualified_name) =
            self.type_registry.find_qualified_class(class_name)
        {
            self.type_registry
                .find_method(&qualified_name, member_name, arg_types)
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
    }

    /// 提取接收者表达式的类级类型实参。
    ///
    /// 支持 Type::Generic 和 Type::Object("Class<T>") 两种形式，
    /// 后者由 new Class<T>() 产生，需要解析字符串中的类型实参。
    pub(crate) fn receiver_class_type_args(&self, obj_type: &Type) -> Option<Vec<Type>> {
        match obj_type {
            Type::Generic(_, type_args) => Some(type_args.clone()),
            Type::Object(class_name) => {
                // 解析 "Container<int>" 中的类型参数
                if class_name.contains('<') && class_name.ends_with('>') {
                    if let Some(pos) = class_name.find('<') {
                        let args_str = &class_name[pos + 1..class_name.len() - 1];
                        // 使用 split_type_arguments 以正确处理嵌套泛型实参。
                        let type_args: Vec<Type> = self
                            .split_type_arguments(args_str)
                            .into_iter()
                            .filter(|s| !s.trim().is_empty())
                            .map(|s| self.parse_type_string(&s))
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
        }
    }

    /// 如果对象是泛型类型，替换方法返回类型中的泛型参数。
    ///
    /// 支持 Type::Generic 和 Type::Object("Class<T>") 两种形式，
    /// 后者由 new Class<T>() 产生，需要解析字符串中的类型实参。
    fn substitute_method_return_type(
        &self,
        return_type: &Type,
        obj_type: &Type,
        owner_class: &str,
    ) -> Type {
        let scoped_return_type = self.qualify_type_for_class(return_type, owner_class);
        let obj_type_args = self.receiver_class_type_args(obj_type);
        if let Some(type_args) = obj_type_args {
            if let Some(class_info) = self.type_registry.get_class(owner_class) {
                self.substitute_type_params(
                    &scoped_return_type,
                    &class_info.type_params,
                    &type_args,
                )
            } else if let Some(struct_info) = self.type_registry.get_struct(owner_class) {
                // 对泛型 struct 实例（如 Point<int>）替换返回类型中的类型参数。
                self.substitute_type_params(
                    &scoped_return_type,
                    &struct_info.type_params,
                    &type_args,
                )
            } else {
                scoped_return_type
            }
        } else {
            scoped_return_type
        }
    }

    /// 阶段三：标识符兜底
    ///
    /// 依次检查函数指针变量、@FreeFunction 注册的自由函数，
    /// 均不命中时报告未找到方法的错误。
    fn infer_identifier_call_fallback(
        &mut self,
        call: &CallExpr,
        name: &IdentifierExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
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
        Err(semantic_error_at_loc(
            &call.loc,
            format!("Cannot find method '{}'", name),
        ))
    }

    /// 阶段五：函数指针兜底
    ///
    /// 推断 callee 类型，若是函数指针类型则按函数指针调用处理，
    /// 否则报告无法解析调用。
    fn infer_function_pointer_fallback(
        &mut self,
        call: &CallExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
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
