//! 标识符类型推断

use super::super::analyzer::SemanticAnalyzer;
use super::helpers::semantic_error_at_loc;
use crate::ast::*;
use crate::miette_diagnostic::undefined_identifier_error_with_file;
use crate::types::Type;

impl SemanticAnalyzer {
    /// 推断表达式类型（带错误收集）
    /// 这个版本会收集错误到 self.errors 而不是直接返回 Err
    pub fn infer_expr_type_collect_errors(&mut self, expr: &Expr) -> Type {
        match self.infer_expr_type_internal(expr) {
            Ok(ty) => ty,
            Err(e) => {
                // 将错误转换为 SemanticErrorInfo 并收集。
                // 即使错误没有位置信息也必须上报（用 (0,0) 占位），不能静默丢弃。
                let message = crate::miette_diagnostic::get_error_message(&e);
                let file = crate::miette_diagnostic::get_error_file(&e);
                let (line, column) =
                    crate::miette_diagnostic::get_error_location(&e).unwrap_or((0, 0));
                self.errors
                    .push(self.create_error_info_with_file(file, line, column, message));
                Type::Int32 // 返回默认类型继续分析
            }
        }
    }

    /// 推断表达式类型（内部实现）
    ///
    /// 表达式树的所有递归推断都经过本入口，因此在这里做递归深度计数：
    /// 病态嵌套表达式（如上万层括号）超过上限时返回语义错误而不是栈溢出。
    /// 正常路径只有一次 usize 自增与比较，开销可忽略。
    pub(crate) fn infer_expr_type_internal(
        &mut self,
        expr: &Expr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        if self.infer_expr_depth >= super::super::analyzer::MAX_INFER_EXPR_DEPTH {
            let loc = self.get_expr_source_location(expr);
            return Err(semantic_error_at_loc(
                &loc,
                format!(
                    "expression nesting too deep (exceeded {} levels); \
                     split the expression into smaller statements",
                    super::super::analyzer::MAX_INFER_EXPR_DEPTH
                ),
            ));
        }
        self.infer_expr_depth += 1;
        let result = self.infer_expr_type_inner(expr);
        self.infer_expr_depth -= 1;
        result
    }

    /// 推断表达式类型（内部实现的主体）
    pub(crate) fn infer_expr_type_inner(
        &mut self,
        expr: &Expr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        match expr {
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::Int32(_) => Ok(Type::Int32),
                LiteralValue::Int64(_) => Ok(Type::Int64),
                LiteralValue::Float32(_) => Ok(Type::Float32),
                LiteralValue::Float64(_) => Ok(Type::Float64),
                LiteralValue::String(_) => Ok(Type::String),
                LiteralValue::Bool(_) => Ok(Type::Bool),
                LiteralValue::Char(_) => Ok(Type::Char),
                LiteralValue::Null => Ok(Type::Object(crate::types::NULL_TYPE_NAME.to_string())),
            },
            Expr::Identifier(ident) => {
                let name = &ident.name;
                let loc = &ident.loc;

                // 处理 this 标识符
                if name == "this" {
                    // 检查是否在静态上下文中访问 this
                    if self.current_method_is_static {
                        return Err(semantic_error_at_loc(
                            loc,
                            "non-static variable this cannot be referenced from a static context"
                                .to_string(),
                        ));
                    }
                    // 返回当前类类型
                    if let Some(current_class_name) = &self.current_class {
                        return Ok(Type::Object(current_class_name.clone()));
                    }
                    return Err(semantic_error_at_loc(
                        loc,
                        "this can only be used inside a class".to_string(),
                    ));
                }

                // 处理 super 标识符
                if name == "super" {
                    // 检查是否在静态上下文中访问 super
                    if self.current_method_is_static {
                        return Err(semantic_error_at_loc(
                            loc,
                            "non-static variable super cannot be referenced from a static context"
                                .to_string(),
                        ));
                    }
                    // 返回父类类型
                    if let Some(current_class_name) = &self.current_class {
                        if let Some(class_info) = self.type_registry.get_class(current_class_name) {
                            if let Some(parent_name) = &class_info.parent {
                                return Ok(Type::Object(parent_name.clone()));
                            }
                        }
                    }
                    return Err(semantic_error_at_loc(
                        loc,
                        "super can only be used in a class that extends another class".to_string(),
                    ));
                }

                // 首先检查本地符号表（参数、局部变量优先于类字段）
                if let Some(info) = self.symbol_table.lookup(name) {
                    return Ok(info.symbol_type.clone());
                }

                // 检查是否是当前类的字段（包括静态和非静态）
                if let Some(current_class_name) = &self.current_class {
                    if let Some(class_info) = self.type_registry.get_class(current_class_name) {
                        if let Some(field_info) = class_info.fields.get(name) {
                            if field_info.is_static {
                                return Ok(field_info.field_type.clone());
                            } else if self.current_method_is_static {
                                // 静态方法中不能访问非静态字段
                                return Err(semantic_error_at_loc(
                                    loc,
                                    format!(
                                        "non-static variable {} cannot be referenced from a static context",
                                        name
                                    ),
                                ));
                            }
                            // 非静态方法中返回字段类型
                            return Ok(field_info.field_type.clone());
                        }
                        // 检查父类的字段（继承）
                        if let Some(parent_name) = &class_info.parent {
                            if let Some(parent_info) = self.type_registry.get_class(parent_name) {
                                if let Some(field_info) = parent_info.fields.get(name) {
                                    if field_info.is_static {
                                        return Ok(field_info.field_type.clone());
                                    } else if self.current_method_is_static {
                                        return Err(semantic_error_at_loc(
                                            loc,
                                            format!(
                                                "non-static variable {} cannot be referenced from a static context",
                                                name
                                            ),
                                        ));
                                    }
                                    return Ok(field_info.field_type.clone());
                                }
                            }
                        }
                    }
                }

                if self.type_registry.class_exists(name)
                    || self.type_registry.get_struct(name).is_some()
                    || self.type_registry.get_enum(name).is_some()
                {
                    // 标识符是类名，返回类类型（用于静态成员访问）
                    Ok(Type::Object(name.clone()))
                } else {
                    Err(undefined_identifier_error_with_file(
                        loc.file.clone(),
                        loc.line,
                        loc.column,
                        name,
                    ))
                }
            }
            Expr::Binary(bin) => self.infer_binary_type(bin),
            Expr::Unary(unary) => self.infer_unary_type(unary),
            Expr::Call(call) => self.infer_call_type(call),
            Expr::MemberAccess(member) => self.infer_member_access_type(member),
            Expr::New(new_expr) => self.infer_new_type(new_expr),
            Expr::Assignment(assign) => self.infer_assignment_type(assign),
            Expr::Cast(cast) => self.infer_cast_type(cast),
            Expr::ArrayCreation(arr) => self.infer_array_creation_type(arr),
            Expr::ArrayInit(init) => self.infer_array_init_type(init),
            Expr::ArrayAccess(arr) => self.infer_array_access_type(arr),
            Expr::MethodRef(method_ref) => self.infer_method_ref_type(method_ref),
            Expr::Lambda(lambda) => self.infer_lambda_type(lambda),
            Expr::Ternary(ternary) => self.infer_ternary_type(ternary),
            Expr::If(if_expr) => self.infer_if_type(if_expr),
            Expr::Try(try_expr) => self.infer_try_type(try_expr),
            Expr::InstanceOf(instanceof) => self.infer_instanceof_type(instanceof),
            Expr::Alloc(_) => Ok(Type::Int64), // 0.5.0.0: alloc 返回 long (指针)
            Expr::Dealloc(_) => Ok(Type::Void), // 0.5.0.0: dealloc 返回 void
            Expr::AllocArray(alloc_array) => self.infer_alloc_array_type(alloc_array), // 0.5.2.x
            Expr::NamedArg(named) => self.infer_expr_type_internal(&named.value), // 命名参数返回其值的类型
        }
    }

    pub(crate) fn identifier_has_value_binding(&self, name: &str) -> bool {
        if self.symbol_table.lookup(name).is_some() {
            return true;
        }

        let mut class_to_check = self.current_class.clone();
        // 防止循环继承导致死循环
        let mut visited = std::collections::HashSet::new();
        while let Some(class_name) = class_to_check {
            if !visited.insert(class_name.clone()) {
                break;
            }
            if let Some(class_info) = self.type_registry.get_class(&class_name) {
                if class_info.fields.contains_key(name) {
                    return true;
                }
                class_to_check = class_info.parent.clone();
            } else {
                break;
            }
        }

        false
    }

    pub(crate) fn split_type_arguments(&self, args_str: &str) -> Vec<String> {
        let mut args = Vec::new();
        let mut depth = 0usize;
        let mut start = 0usize;

        for (idx, ch) in args_str.char_indices() {
            match ch {
                '<' => depth += 1,
                '>' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    let arg = args_str[start..idx].trim();
                    if !arg.is_empty() {
                        args.push(arg.to_string());
                    }
                    start = idx + ch.len_utf8();
                }
                _ => {}
            }
        }

        let arg = args_str[start..].trim();
        if !arg.is_empty() {
            args.push(arg.to_string());
        }

        args
    }

    pub(crate) fn split_generic_type_name(&self, name: &str) -> (String, Option<Vec<Type>>) {
        let Some(pos) = name.find('<') else {
            return (name.to_string(), None);
        };

        if !name.ends_with('>') {
            return (name.to_string(), None);
        }

        let base = name[..pos].trim().to_string();
        let args_str = &name[pos + 1..name.len() - 1];
        let type_args: Vec<Type> = self
            .split_type_arguments(args_str)
            .into_iter()
            .map(|arg| self.parse_type_string(&arg))
            .collect();

        if type_args.is_empty() {
            (base, None)
        } else {
            (base, Some(type_args))
        }
    }

    pub(crate) fn specialize_method_info(
        &self,
        method_info: &crate::types::MethodInfo,
        type_params: &[crate::types::TypeParamInfo],
        type_args: Option<&[Type]>,
    ) -> crate::types::MethodInfo {
        let Some(type_args) = type_args else {
            return method_info.clone();
        };

        if type_params.is_empty() || type_args.is_empty() {
            return method_info.clone();
        }

        let mut specialized = method_info.clone();
        for param in &mut specialized.params {
            param.param_type =
                self.substitute_type_params(&param.param_type, type_params, type_args);
        }
        specialized.return_type =
            self.substitute_type_params(&specialized.return_type, type_params, type_args);
        specialized
    }

    /// 从调用实参推断泛型类型实参。
    /// 用于静态方法调用没有显式类型实参的场景，例如 `Container.make(42)` 推断出 `T = int`。
    pub(crate) fn infer_type_args_from_arguments(
        &mut self,
        method_params: &[crate::types::ParameterInfo],
        call_args: &[Expr],
        type_params: &[crate::types::TypeParamInfo],
    ) -> Option<Vec<Type>> {
        let mut inferred: Vec<Option<Type>> = vec![None; type_params.len()];

        // 仅使用位置参数进行推断
        let positional_args: Vec<&Expr> = call_args
            .iter()
            .filter(|a| !matches!(a, Expr::NamedArg(_)))
            .collect();

        for (param, arg) in method_params.iter().zip(positional_args.iter()) {
            let arg_type = self.infer_expr_type_internal(arg).ok()?;
            self.infer_generic_substitution(
                &param.param_type,
                &arg_type,
                type_params,
                &mut inferred,
            )?;
        }

        // 未推断出的类型参数使用默认值
        for (idx, param) in type_params.iter().enumerate() {
            if inferred[idx].is_none() {
                inferred[idx] = param.default_type.clone();
            }
        }

        if inferred.iter().all(|t| t.is_some()) {
            Some(inferred.into_iter().map(|t| t.unwrap()).collect())
        } else {
            None
        }
    }

    /// 递归比较形参类型与实参类型，收集泛型参数映射。
    fn infer_generic_substitution(
        &self,
        param_type: &Type,
        arg_type: &Type,
        type_params: &[crate::types::TypeParamInfo],
        inferred: &mut [Option<Type>],
    ) -> Option<()> {
        // 将裸 Object("T") 与 GenericParam("T") 统一视为类型参数占位符。
        let param_name = match param_type {
            Type::GenericParam(name) => Some(name.as_str()),
            Type::Object(name) if type_params.iter().any(|p| &p.name == name) => {
                Some(name.as_str())
            }
            _ => None,
        };
        if let Some(name) = param_name {
            if let Some(idx) = type_params.iter().position(|p| p.name == name) {
                if inferred[idx].is_none() {
                    inferred[idx] = Some(arg_type.clone());
                }
            }
            return Some(());
        }

        match (param_type, arg_type) {
            (Type::Generic(p_base, p_args), Type::Generic(a_base, a_args))
                if p_base == a_base && p_args.len() == a_args.len() =>
            {
                for (p, a) in p_args.iter().zip(a_args.iter()) {
                    self.infer_generic_substitution(p, a, type_params, inferred)?;
                }
            }
            (Type::Array(p_inner), Type::Array(a_inner)) => {
                self.infer_generic_substitution(p_inner, a_inner, type_params, inferred)?;
            }
            (Type::Pointer(p_inner), Type::Pointer(a_inner)) => {
                self.infer_generic_substitution(p_inner, a_inner, type_params, inferred)?;
            }
            (Type::Function(p_ft), Type::Function(a_ft))
                if p_ft.params.len() == a_ft.params.len() =>
            {
                self.infer_generic_substitution(
                    &p_ft.return_type,
                    &a_ft.return_type,
                    type_params,
                    inferred,
                )?;
                for (p, a) in p_ft.params.iter().zip(a_ft.params.iter()) {
                    self.infer_generic_substitution(p, a, type_params, inferred)?;
                }
            }
            _ => {}
        }
        Some(())
    }

    pub(crate) fn qualify_type_for_class(&self, ty: &Type, owner_class: &str) -> Type {
        match ty {
            Type::Object(name) => {
                Type::Object(self.qualify_class_name_for_owner(name, owner_class))
            }
            Type::Generic(name, args) => Type::Generic(
                self.qualify_class_name_for_owner(name, owner_class),
                args.iter()
                    .map(|arg| self.qualify_type_for_class(arg, owner_class))
                    .collect(),
            ),
            Type::Array(elem) => {
                Type::Array(Box::new(self.qualify_type_for_class(elem, owner_class)))
            }
            Type::Pointer(inner) => {
                Type::Pointer(Box::new(self.qualify_type_for_class(inner, owner_class)))
            }
            Type::Function(func_type) => {
                let params = func_type
                    .params
                    .iter()
                    .map(|param| self.qualify_type_for_class(param, owner_class))
                    .collect();
                let return_type = self.qualify_type_for_class(&func_type.return_type, owner_class);
                Type::Function(Box::new(crate::types::FunctionType {
                    params,
                    return_type: Box::new(return_type),
                    is_static: func_type.is_static,
                    is_closure: func_type.is_closure,
                }))
            }
            _ => ty.clone(),
        }
    }

    pub(crate) fn qualify_class_name_for_owner(&self, name: &str, owner_class: &str) -> String {
        let Some(ns_end) = owner_class.rfind("::") else {
            return name.to_string();
        };

        let base_end = name.find('<').unwrap_or(name.len());
        let base_name = &name[..base_end];
        if base_name.contains("::") {
            return name.to_string();
        }

        let namespace = &owner_class[..ns_end];
        let qualified = format!("{}::{}", namespace, base_name);
        if self.type_registry.classes.contains_key(&qualified) {
            format!("{}{}", qualified, &name[base_end..])
        } else {
            name.to_string()
        }
    }

    pub(crate) fn collect_static_method_candidates(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> (bool, Vec<(String, crate::types::MethodInfo)>) {
        let mut has_instance_method = false;
        let mut candidate_methods = Vec::new();
        let mut class_to_check = Some(class_name.to_string());

        while let Some(name) = class_to_check {
            if let Some(class_info) = self.type_registry.get_class(&name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    for method in methods {
                        if method.is_static {
                            candidate_methods.push((class_info.name.clone(), method.clone()));
                        } else {
                            has_instance_method = true;
                        }
                    }
                }
                class_to_check = class_info.parent.clone();
            } else {
                break;
            }
        }

        if let Some(struct_info) = self.type_registry.get_struct(class_name) {
            if let Some(methods) = struct_info.methods.get(method_name) {
                for method in methods {
                    if method.is_static {
                        candidate_methods.push((struct_info.name.clone(), method.clone()));
                    } else {
                        has_instance_method = true;
                    }
                }
            }
        }

        (has_instance_method, candidate_methods)
    }

    pub(crate) fn suggest_method_name(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<String> {
        let mut best_name: Option<String> = None;
        let mut best_distance = usize::MAX;
        let target = method_name.to_ascii_lowercase();
        let threshold = (method_name.chars().count() / 3).max(2);
        let mut class_to_check = Some(class_name.to_string());

        while let Some(name) = class_to_check {
            if let Some(class_info) = self.type_registry.get_class(&name) {
                for candidate in class_info.methods.keys() {
                    let distance =
                        super::helpers::edit_distance(&target, &candidate.to_ascii_lowercase());
                    if distance < best_distance {
                        best_distance = distance;
                        best_name = Some(candidate.clone());
                    }
                }
                class_to_check = class_info.parent.clone();
            } else {
                break;
            }
        }

        if let Some(struct_info) = self.type_registry.get_struct(class_name) {
            for candidate in struct_info.methods.keys() {
                let distance =
                    super::helpers::edit_distance(&target, &candidate.to_ascii_lowercase());
                if distance < best_distance {
                    best_distance = distance;
                    best_name = Some(candidate.clone());
                }
            }
        }

        if best_distance <= threshold {
            best_name
        } else {
            None
        }
    }

    pub(crate) fn unknown_method_message(&self, method_name: &str, class_name: &str) -> String {
        if let Some(suggestion) = self.suggest_method_name(class_name, method_name) {
            format!(
                "Unknown method '{}' for class {}. Did you mean '{}'?",
                method_name, class_name, suggestion
            )
        } else {
            format!("Unknown method '{}' for class {}", method_name, class_name)
        }
    }

    pub(crate) fn unknown_static_member_message(
        &self,
        member_name: &str,
        class_name: &str,
    ) -> String {
        if let Some(suggestion) = self.suggest_method_name(class_name, member_name) {
            format!(
                "Unknown static member '{}' for class {}. Did you mean '{}()'?",
                member_name, class_name, suggestion
            )
        } else {
            format!(
                "Unknown static member '{}' for class {}",
                member_name, class_name
            )
        }
    }

    pub(crate) fn infer_static_or_enum_member_call(
        &mut self,
        member: &MemberAccessExpr,
        call: &CallExpr,
    ) -> crate::miette_diagnostic::CayResult<Option<Type>> {
        let raw_class_name = match &*member.object {
            Expr::Identifier(class_name) => class_name.as_ref().to_string(),
            _ => return Ok(None),
        };

        if self.identifier_has_value_binding(&raw_class_name) {
            return Ok(None);
        }

        let (class_name, type_args) = self.split_generic_type_name(&raw_class_name);

        if let Some(enum_info) = self.type_registry.get_enum_by_name(&class_name).cloned() {
            if let Some(variant) = enum_info.variants.iter().find(|v| v.name == member.member) {
                // 解析泛型 enum 的类型实参：显式提供或通过 payload 实参推断。
                let resolved_type_args: Option<Vec<Type>> = if enum_info.type_params.is_empty() {
                    None
                } else if let Some(ref explicit_args) = type_args {
                    Some(explicit_args.clone())
                } else if call.args.len() == 1 {
                    if let Some(ref payload_type) = variant.payload_type {
                        let fake_param = crate::types::ParameterInfo {
                            name: "value".to_string(),
                            param_type: payload_type.clone(),
                            is_varargs: false,
                        };
                        self.infer_type_args_from_arguments(
                            &[fake_param],
                            &call.args,
                            &enum_info.type_params,
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };

                if !enum_info.type_params.is_empty() && resolved_type_args.is_none() {
                    let param_names: Vec<String> = enum_info
                        .type_params
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();
                    return Err(semantic_error_at_loc(
                        &call.loc,
                        format!(
                            "Generic enum '{}<{}>' requires explicit type arguments, e.g. {}<{}>.{}(...)",
                            enum_info.name,
                            param_names.join(", "),
                            enum_info.name,
                            param_names.join(", "),
                            member.member
                        ),
                    ));
                }

                let payload_type_opt = variant.payload_type.clone().map(|pt| {
                    if let Some(ref args) = resolved_type_args {
                        self.substitute_type_params(&pt, &enum_info.type_params, args)
                    } else {
                        pt
                    }
                });
                match &payload_type_opt {
                    Some(expected_payload_type) => {
                        if call.args.len() != 1 {
                            return Err(semantic_error_at_loc(
                                &call.loc,
                                format!(
                                    "Enum variant '{}.{}' with payload expects 1 argument, but got {}",
                                    enum_info.name,
                                    member.member,
                                    call.args.len()
                                ),
                            ));
                        }
                        let arg_type = self.infer_expr_type_internal(&call.args[0])?;
                        if !self.types_compatible(&arg_type, expected_payload_type) {
                            return Err(semantic_error_at_loc(
                                &call.loc,
                                format!(
                                    "Enum variant '{}.{}' payload type mismatch: expected {}, got {}",
                                    enum_info.name, member.member, expected_payload_type, arg_type
                                ),
                            ));
                        }
                    }
                    None => {
                        if !call.args.is_empty() {
                            return Err(semantic_error_at_loc(
                                &call.loc,
                                format!(
                                    "Enum variant '{}.{}' has no payload, but got {} argument(s)",
                                    enum_info.name,
                                    member.member,
                                    call.args.len()
                                ),
                            ));
                        }
                    }
                }
                let return_type = if enum_info.type_params.is_empty() {
                    Type::Object(enum_info.name.clone())
                } else if let Some(ref args) = resolved_type_args {
                    Type::Generic(enum_info.name.clone(), args.clone())
                } else {
                    Type::Object(enum_info.name.clone())
                };
                return Ok(Some(return_type));
            }

            return Err(semantic_error_at_loc(
                &call.loc,
                format!(
                    "Unknown variant '{}' for enum {}",
                    member.member, enum_info.name
                ),
            ));
        }

        let resolved_class_name = if let Some(class_info) =
            self.type_registry.get_class(&class_name)
        {
            Some(class_info.name.clone())
        } else if let Some(qualified_name) = self.type_registry.find_qualified_class(&class_name) {
            Some(qualified_name)
        } else if self.type_registry.get_struct(&class_name).is_some() {
            Some(class_name.clone())
        } else {
            None
        };

        let Some(resolved_class_name) = resolved_class_name else {
            return Ok(None);
        };

        if let Some(class_info) = self.type_registry.get_class(&resolved_class_name) {
            if let Some(field_info) = class_info.fields.get(&member.member) {
                if field_info.is_static {
                    return Ok(None);
                }
            }
        }

        let (has_instance_method, candidate_methods) =
            self.collect_static_method_candidates(&resolved_class_name, &member.member);
        let candidate_methods: Vec<_> = candidate_methods
            .into_iter()
            .map(|(owner_class, method_info)| {
                let owner_type_params = self
                    .type_registry
                    .get_class(&owner_class)
                    .map(|ci| ci.type_params.clone())
                    .unwrap_or_default();
                let mut all_type_params = owner_type_params.clone();
                all_type_params.extend(method_info.type_params.clone());
                // 没有显式类型实参且类有泛型参数时，尝试从调用实参推断。
                let inferred_type_args: Option<Vec<Type>> = if type_args.is_some()
                    && method_info.type_params.is_empty()
                {
                    type_args.clone()
                } else {
                    self.infer_type_args_from_arguments(
                        &method_info.params,
                        &call.args,
                        &all_type_params,
                    )
                };
                (
                    owner_class,
                    self.specialize_method_info(
                        &method_info,
                        &all_type_params,
                        inferred_type_args.as_deref(),
                    ),
                )
            })
            .collect();

        for (owner_class, method_info) in &candidate_methods {
            if self.check_arguments_exact(&call.args, &method_info.params) {
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

        let mut mismatch_detail = None;
        for (owner_class, method_info) in &candidate_methods {
            match self.check_arguments_compatible(
                &call.args,
                &method_info.params,
                call.loc.line,
                call.loc.column,
            ) {
                Ok(()) => {
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
                Err(msg) => {
                    if mismatch_detail.is_none() {
                        mismatch_detail = Some(msg);
                    }
                }
            }
        }

        if !candidate_methods.is_empty() {
            let detail = mismatch_detail.unwrap_or_else(|| "argument mismatch".to_string());
            return Err(semantic_error_at_loc(
                &call.loc,
                format!(
                    "Method '{}' in class '{}' cannot be applied to given types: {}",
                    member.member, resolved_class_name, detail
                ),
            ));
        }

        if has_instance_method {
            return Err(semantic_error_at_loc(
                &call.loc,
                format!(
                    "Non-static method '{}' in class '{}' cannot be referenced from a static context",
                    member.member, resolved_class_name
                ),
            ));
        }

        Err(semantic_error_at_loc(
            &call.loc,
            self.unknown_method_message(&member.member, &resolved_class_name),
        ))
    }
}
