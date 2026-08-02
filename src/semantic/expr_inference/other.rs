//! 其他表达式类型推断

use super::super::analyzer::SemanticAnalyzer;
use super::helpers::semantic_error_at_loc;
use crate::ast::*;
use crate::miette_diagnostic::{ErrorCodes, semantic_error_with_file};
use crate::types::{ParameterInfo, Type};
use std::collections::{HashMap, HashSet};

/// 检查类型中是否包含指定的泛型参数名集合中的任意一个。
fn type_contains_generic_params(ty: &Type, names: &HashSet<String>) -> bool {
    match ty {
        Type::GenericParam(name) => names.contains(name),
        Type::Array(inner) | Type::Pointer(inner) => type_contains_generic_params(inner, names),
        Type::Generic(_, args) => {
            args.iter().any(|a| type_contains_generic_params(a, names))
        }
        Type::Function(ft) => {
            type_contains_generic_params(&ft.return_type, names)
                || ft.params
                    .iter()
                    .any(|p| type_contains_generic_params(p, names))
        }
        _ => false,
    }
}

impl SemanticAnalyzer {
    /// 判断使用默认类型参数的构造函数是否发生“致命”错配：
    /// 实参 arity 与某个使用默认类型参数的构造函数相同，且在该构造函数中
    /// 某个直接使用默认类型参数的位置，实参是一个泛型类型实例（Type::Generic），
    /// 却与替换后的默认类型不兼容。这通常意味着调用者把本应用于默认类型参数
    /// 的位置传成了其他泛型类型实参，必须报错而不是回退到其他构造函数。
    fn is_default_param_fatal_mismatch(
        &mut self,
        constructor: &crate::types::ConstructorInfo,
        substituted_params: &[ParameterInfo],
        args: &[Expr],
        defaulted_param_names: &HashSet<String>,
    ) -> bool {
        if substituted_params.len() != args.len() {
            return false;
        }
        for ((original, substituted), arg) in constructor
            .params
            .iter()
            .zip(substituted_params.iter())
            .zip(args.iter())
        {
            if let Type::GenericParam(name) = &original.param_type {
                if defaulted_param_names.contains(name) {
                    let arg_type = self.infer_expr_type_collect_errors(arg);
                    if !self.types_compatible(&arg_type, &substituted.param_type)
                        && matches!(arg_type, Type::Generic(_, _))
                    {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 根据类/struct 的类型形参与调用处提供的类型实参，替换构造函数形参中的泛型参数。
    /// 未显式提供的类型实参使用类型形参的默认值填充。
    fn substitute_constructor_params(
        &self,
        constructor: &crate::types::ConstructorInfo,
        type_params: &[crate::types::TypeParamInfo],
        type_args: &[Type],
    ) -> Vec<ParameterInfo> {
        let mut complete_args: Vec<Type> = type_args.to_vec();
        for (idx, param) in type_params.iter().enumerate() {
            if complete_args.get(idx).is_none() {
                complete_args.push(
                    param
                        .default_type
                        .clone()
                        .unwrap_or(Type::GenericParam(param.name.clone())),
                );
            }
        }
        let mapping: HashMap<String, Type> = type_params
            .iter()
            .zip(complete_args.iter())
            .map(|(p, t)| (p.name.clone(), t.clone()))
            .collect();
        constructor
            .params
            .iter()
            .map(|p| ParameterInfo {
                name: p.name.clone(),
                param_type: crate::types::substitute_type_params(&p.param_type, &mapping),
                is_varargs: p.is_varargs,
            })
            .collect()
    }

    /// 推断 new 表达式类型
    pub(crate) fn infer_new_type(
        &mut self,
        new_expr: &NewExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 解析泛型类名，并递归解析嵌套泛型实参。
        // 例如 Wrapper<Pair<int, String>> -> ("Wrapper", Some([Generic("Pair", [Int32, String])]))
        let (base_class_name, parsed_type_args_opt) =
            self.split_generic_type_name(&new_expr.class_name);
        let parsed_type_args = parsed_type_args_opt.unwrap_or_default();

        // 先严格推断构造参数，避免非法表达式漏到代码生成阶段。
        for arg in &new_expr.args {
            self.infer_expr_type_internal(arg)?;
        }

        // 辅助闭包：构造返回类型。有显式类型实参时返回 Type::Generic，
        // 否则返回 Type::Object，保持与非泛型构造的一致性。
        let make_return_type = |base: &str, args: &[Type]| -> Type {
            if args.is_empty() {
                Type::Object(base.to_string())
            } else {
                Type::Generic(base.to_string(), args.to_vec())
            }
        };

        // 检查基础类是否存在
        if let Some(class_info) = self.type_registry.get_class(&base_class_name).cloned() {
            // 检查是否是抽象类
            if class_info.is_abstract {
                return Err(semantic_error_at_loc(
                    &new_expr.loc,
                    format!("Cannot instantiate abstract class '{}'", base_class_name),
                ));
            }

            if class_info.constructors.is_empty() {
                if !new_expr.args.is_empty() {
                    return Err(semantic_error_at_loc(
                        &new_expr.loc,
                        format!(
                            "Constructor '{}' cannot be applied to given types: expected 0 arguments, got {}",
                            base_class_name,
                            new_expr.args.len()
                        ),
                    ));
                }
            } else {
                // 收集被默认填充的泛型形参名（用户未显式提供的类型实参）。
                // 这些默认类型参数必须参与构造函数重载解析：如果存在使用它们的
                // 构造函数且实参 arity 匹配，即使类型不匹配，也不得默默回退到
                // 使用其他类型参数的构造函数，否则会把 allocator 等默认参数错配
                // 解释为其他泛型参数，导致运行时错误（如 #issue-vec-sigsegv）。
                let defaulted_param_names: HashSet<String> = class_info.type_params
                    [parsed_type_args.len()..]
                    .iter()
                    .filter_map(|p| p.default_type.as_ref().map(|_| p.name.clone()))
                    .collect();

                let mut matched_constructor = None;
                let mut mismatch_detail = None;
                let mut has_arity_match_error = false;
                let mut default_param_fatal_mismatch: Option<String> = None;

                // 第一遍：优先匹配使用默认类型参数的构造函数。
                for constructor in &class_info.constructors {
                    let uses_defaulted_param = constructor
                        .params
                        .iter()
                        .any(|p| type_contains_generic_params(&p.param_type, &defaulted_param_names));
                    if !uses_defaulted_param {
                        continue;
                    }

                    let substituted_params = self.substitute_constructor_params(
                        constructor,
                        &class_info.type_params,
                        &parsed_type_args,
                    );
                    let arity_matches = substituted_params.len() == new_expr.args.len();
                    match self.check_arguments_compatible(
                        &new_expr.args,
                        &substituted_params,
                        new_expr.loc.line,
                        new_expr.loc.column,
                    ) {
                        Ok(()) => {
                            matched_constructor = Some(constructor.clone());
                            break;
                        }
                        Err(msg) => {
                            if arity_matches {
                                if default_param_fatal_mismatch.is_none()
                                    && self.is_default_param_fatal_mismatch(
                                        constructor,
                                        &substituted_params,
                                        &new_expr.args,
                                        &defaulted_param_names,
                                    )
                                {
                                    default_param_fatal_mismatch = Some(msg.clone());
                                }
                                if !has_arity_match_error {
                                    mismatch_detail = Some(msg);
                                    has_arity_match_error = true;
                                }
                            } else if mismatch_detail.is_none() && !has_arity_match_error {
                                mismatch_detail = Some(msg);
                            }
                        }
                    }
                }

                // 第二遍：只有默认类型参数构造函数没有发生致命错配时，
                // 才回退到普通构造函数。
                if matched_constructor.is_none() && default_param_fatal_mismatch.is_none() {
                    for constructor in &class_info.constructors {
                        let uses_defaulted_param = constructor
                            .params
                            .iter()
                            .any(|p| type_contains_generic_params(&p.param_type, &defaulted_param_names));
                        if uses_defaulted_param {
                            continue;
                        }

                        let substituted_params = self.substitute_constructor_params(
                            constructor,
                            &class_info.type_params,
                            &parsed_type_args,
                        );
                        let arity_matches = substituted_params.len() == new_expr.args.len();
                        match self.check_arguments_compatible(
                            &new_expr.args,
                            &substituted_params,
                            new_expr.loc.line,
                            new_expr.loc.column,
                        ) {
                            Ok(()) => {
                                matched_constructor = Some(constructor.clone());
                                break;
                            }
                            Err(msg) => {
                                if arity_matches {
                                    if !has_arity_match_error {
                                        mismatch_detail = Some(msg);
                                        has_arity_match_error = true;
                                    }
                                } else if mismatch_detail.is_none() && !has_arity_match_error {
                                    mismatch_detail = Some(msg);
                                }
                            }
                        }
                    }
                }

                if let Some(msg) = default_param_fatal_mismatch {
                    return Err(semantic_error_at_loc(
                        &new_expr.loc,
                        format!(
                            "Constructor '{}' cannot be applied to given types: {}",
                            base_class_name, msg
                        ),
                    ));
                }

                let Some(constructor) = matched_constructor else {
                    return Err(semantic_error_at_loc(
                        &new_expr.loc,
                        format!(
                            "Constructor '{}' cannot be applied to given types: {}",
                            base_class_name,
                            mismatch_detail.unwrap_or_else(|| "argument mismatch".to_string())
                        ),
                    ));
                };

                // 当在当前类/struct 内部 new 本类时，target_class 需与 current_class 对齐
                // （current_class 为限定名而 base_class_name 可能为非限定名）。
                let target_class = self
                    .current_class
                    .as_ref()
                    .filter(|c| **c == base_class_name || c.ends_with(&format!("::{}", base_class_name)))
                    .cloned()
                    .unwrap_or_else(|| base_class_name.clone());
                super::helpers::check_member_access(
                    &base_class_name,
                    constructor.is_public,
                    constructor.is_protected,
                    constructor.is_private,
                    &self.current_class,
                    &target_class,
                    &self.type_registry,
                    &new_expr.loc,
                )?;
            }

            // 如果类有泛型参数，验证显式提供的类型实参是否合法。
            // 未提供实参时保持原有行为，由后续单态化或默认类型处理。
            if !class_info.type_params.is_empty() {
                for ty in &parsed_type_args {
                    if !self.is_valid_type_arg(ty) {
                        return Err(semantic_error_at_loc(
                            &new_expr.loc,
                            format!(
                                "Invalid type argument '{}' for class '{}'",
                                ty.display_name(),
                                base_class_name
                            ),
                        ));
                    }
                }
            }

            Ok(make_return_type(&base_class_name, &parsed_type_args))
        } else if self
            .current_class_type_params
            .iter()
            .any(|p| &p.name == &base_class_name)
        {
            // new A() where A is a type parameter of the current class.
            // The concrete type will be substituted during monomorphization.
            Ok(make_return_type(&base_class_name, &parsed_type_args))
        } else if let Some(struct_info) = self.type_registry.get_struct(&base_class_name).cloned() {
            // struct 是值类型，校验构造函数并检查泛型参数。
            if struct_info.constructors.is_empty() {
                if !new_expr.args.is_empty() {
                    return Err(semantic_error_at_loc(
                        &new_expr.loc,
                        format!(
                            "Constructor '{}' cannot be applied to given types: expected 0 arguments, got {}",
                            base_class_name,
                            new_expr.args.len()
                        ),
                    ));
                }
            } else {
                let defaulted_param_names: HashSet<String> = struct_info.type_params
                    [parsed_type_args.len()..]
                    .iter()
                    .filter_map(|p| p.default_type.as_ref().map(|_| p.name.clone()))
                    .collect();

                let mut matched_constructor = None;
                let mut mismatch_detail = None;
                let mut has_arity_match_error = false;
                let mut default_param_fatal_mismatch: Option<String> = None;

                for constructor in &struct_info.constructors {
                    let uses_defaulted_param = constructor
                        .params
                        .iter()
                        .any(|p| type_contains_generic_params(&p.param_type, &defaulted_param_names));
                    if !uses_defaulted_param {
                        continue;
                    }

                    let substituted_params = self.substitute_constructor_params(
                        constructor,
                        &struct_info.type_params,
                        &parsed_type_args,
                    );
                    let arity_matches = substituted_params.len() == new_expr.args.len();
                    match self.check_arguments_compatible(
                        &new_expr.args,
                        &substituted_params,
                        new_expr.loc.line,
                        new_expr.loc.column,
                    ) {
                        Ok(()) => {
                            matched_constructor = Some(constructor.clone());
                            break;
                        }
                        Err(msg) => {
                            if arity_matches {
                                if default_param_fatal_mismatch.is_none()
                                    && self.is_default_param_fatal_mismatch(
                                        constructor,
                                        &substituted_params,
                                        &new_expr.args,
                                        &defaulted_param_names,
                                    )
                                {
                                    default_param_fatal_mismatch = Some(msg.clone());
                                }
                                if !has_arity_match_error {
                                    mismatch_detail = Some(msg);
                                    has_arity_match_error = true;
                                }
                            } else if mismatch_detail.is_none() && !has_arity_match_error {
                                mismatch_detail = Some(msg);
                            }
                        }
                    }
                }

                if matched_constructor.is_none() && default_param_fatal_mismatch.is_none() {
                    for constructor in &struct_info.constructors {
                        let uses_defaulted_param = constructor
                            .params
                            .iter()
                            .any(|p| type_contains_generic_params(&p.param_type, &defaulted_param_names));
                        if uses_defaulted_param {
                            continue;
                        }

                        let substituted_params = self.substitute_constructor_params(
                            constructor,
                            &struct_info.type_params,
                            &parsed_type_args,
                        );
                        let arity_matches = substituted_params.len() == new_expr.args.len();
                        match self.check_arguments_compatible(
                            &new_expr.args,
                            &substituted_params,
                            new_expr.loc.line,
                            new_expr.loc.column,
                        ) {
                            Ok(()) => {
                                matched_constructor = Some(constructor.clone());
                                break;
                            }
                            Err(msg) => {
                                if arity_matches {
                                    if !has_arity_match_error {
                                        mismatch_detail = Some(msg);
                                        has_arity_match_error = true;
                                    }
                                } else if mismatch_detail.is_none() && !has_arity_match_error {
                                    mismatch_detail = Some(msg);
                                }
                            }
                        }
                    }
                }

                if let Some(msg) = default_param_fatal_mismatch {
                    return Err(semantic_error_at_loc(
                        &new_expr.loc,
                        format!(
                            "Constructor '{}' cannot be applied to given types: {}",
                            base_class_name, msg
                        ),
                    ));
                }

                let Some(constructor) = matched_constructor else {
                    return Err(semantic_error_at_loc(
                        &new_expr.loc,
                        format!(
                            "Constructor '{}' cannot be applied to given types: {}",
                            base_class_name,
                            mismatch_detail.unwrap_or_else(|| "argument mismatch".to_string())
                        ),
                    ));
                };

                // 当在当前类/struct 内部 new 本类时，target_class 需与 current_class 对齐
                // （current_class 为限定名而 base_class_name 可能为非限定名）。
                let target_class = self
                    .current_class
                    .as_ref()
                    .filter(|c| **c == base_class_name || c.ends_with(&format!("::{}", base_class_name)))
                    .cloned()
                    .unwrap_or_else(|| base_class_name.clone());
                super::helpers::check_member_access(
                    &base_class_name,
                    constructor.is_public,
                    constructor.is_protected,
                    constructor.is_private,
                    &self.current_class,
                    &target_class,
                    &self.type_registry,
                    &new_expr.loc,
                )?;
            }

            // 如果 struct 有泛型参数，验证显式提供的类型实参是否合法。
            if !struct_info.type_params.is_empty() {
                for ty in &parsed_type_args {
                    if !self.is_valid_type_arg(ty) {
                        return Err(semantic_error_at_loc(
                            &new_expr.loc,
                            format!(
                                "Invalid type argument '{}' for struct '{}'",
                                ty.display_name(),
                                base_class_name
                            ),
                        ));
                    }
                }
            }

            Ok(make_return_type(&base_class_name, &parsed_type_args))
        } else {
            Err(semantic_error_at_loc(
                &new_expr.loc,
                format!("Unknown class or struct: {}", base_class_name),
            ))
        }
    }

    /// 检查给定的类型实参是否是有效类型。
    ///
    /// 有效类型包括：基本类型、已注册的类/结构体/枚举、当前类的泛型参数、
    /// 以及由上述类型递归构成的泛型类型（支持嵌套泛型）。
    fn is_valid_type_arg(&self, ty: &Type) -> bool {
        match ty {
            Type::Void
            | Type::Int32
            | Type::Int64
            | Type::Float32
            | Type::Float64
            | Type::Bool
            | Type::String
            | Type::Char
            | Type::CInt
            | Type::CUInt
            | Type::CLong
            | Type::CULong
            | Type::CShort
            | Type::CUShort
            | Type::CChar
            | Type::CUChar
            | Type::CFloat
            | Type::CDouble
            | Type::SizeT
            | Type::SSizeT
            | Type::UIntPtr
            | Type::IntPtr
            | Type::CVoid
            | Type::CBool => true,
            Type::Object(name) | Type::Struct(name) => {
                self.type_registry.class_exists(name)
                    || self.type_registry.get_struct(name).is_some()
                    || self.type_registry.get_enum_by_name(name).is_some()
                    || self.current_class_type_params.iter().any(|p| &p.name == name)
            }
            Type::GenericParam(name) => self
                .current_class_type_params
                .iter()
                .any(|p| &p.name == name),
            Type::Generic(base, args) => {
                let base_valid = self.type_registry.class_exists(base)
                    || self.type_registry.get_struct(base).is_some()
                    || self.type_registry.get_enum_by_name(base).is_some();
                base_valid && args.iter().all(|a| self.is_valid_type_arg(a))
            }
            Type::Array(inner) | Type::Pointer(inner) => self.is_valid_type_arg(inner),
            _ => false,
        }
    }

    /// 推断赋值表达式类型
    pub(crate) fn infer_assignment_type(
        &mut self,
        assign: &AssignmentExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 检查是否是 final 变量重新赋值
        if let Expr::Identifier(name) = &assign.target.as_ref() {
            if let Some(info) = self.symbol_table.lookup(name.as_ref()) {
                if info.is_final {
                    return Err(semantic_error_at_loc(
                        &assign.loc,
                        format!("Cannot assign a value to final variable '{}'", name),
                    ));
                }
            }
        }

        let target_type = self.infer_expr_type_internal(&assign.target)?;
        let value_type = self.infer_expr_type_internal(&assign.value)?;

        if self.types_compatible(&value_type, &target_type) {
            Ok(target_type)
        } else {
            Err(semantic_error_with_file(
                ErrorCodes::SEMANTIC_INVALID_OPERATION,
                assign.loc.file.clone(),
                assign.loc.line,
                assign.loc.column,
                format!("Cannot assign {} to {}", value_type, target_type),
            ))
        }
    }

    /// 推断方法引用表达式类型
    pub(crate) fn infer_method_ref_type(
        &mut self,
        method_ref: &MethodRefExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 方法引用: ClassName::methodName 或 obj::methodName
        // 返回函数类型，包含参数类型和返回类型信息

        if let Some(ref class_name) = method_ref.class_name {
            // 检查类是否存在
            if !self.type_registry.class_exists(class_name) {
                return Err(semantic_error_at_loc(
                    &method_ref.loc,
                    format!("Unknown class: {}", class_name),
                ));
            }
            // 获取方法信息
            if let Some(class_info) = self.type_registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(&method_ref.method_name) {
                    if let Some(method_info) = methods.first() {
                        // 构建函数类型
                        let param_types: Vec<Type> = method_info
                            .params
                            .iter()
                            .map(|p| p.param_type.clone())
                            .collect();
                        let return_type = Box::new(method_info.return_type.clone());

                        return Ok(Type::Function(Box::new(crate::types::FunctionType {
                            params: param_types,
                            return_type,
                            is_static: method_info.is_static,
                            is_closure: false,
                        })));
                    }
                } else {
                    return Err(semantic_error_at_loc(
                        &method_ref.loc,
                        format!(
                            "Unknown method '{}' for class {}",
                            method_ref.method_name, class_name
                        ),
                    ));
                }
            }
        } else if let Some(object) = method_ref.object.as_ref() {
            // 实例方法引用: obj::methodName
            let obj_type = self.infer_expr_type_internal(object)?;
            if let Type::Object(class_name) = obj_type {
                if let Some(class_info) = self.type_registry.get_class(&class_name) {
                    if let Some(methods) = class_info.methods.get(&method_ref.method_name) {
                        if let Some(method_info) = methods.first() {
                            let param_types: Vec<Type> = method_info
                                .params
                                .iter()
                                .map(|p| p.param_type.clone())
                                .collect();
                            let return_type = Box::new(method_info.return_type.clone());

                            return Ok(Type::Function(Box::new(crate::types::FunctionType {
                                params: param_types,
                                return_type,
                                is_static: false,
                                is_closure: false,
                            })));
                        }
                    }
                }
            }
        }

        // 无法确定具体函数类型，返回通用 Function 类型
        Ok(Type::Object("Function".to_string()))
    }

    /// 推断三元运算符表达式类型
    pub(crate) fn infer_ternary_type(
        &mut self,
        ternary: &TernaryExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 推断条件表达式类型
        let cond_type = self.infer_expr_type_internal(&ternary.condition)?;

        // 条件必须是布尔类型
        if cond_type != Type::Bool {
            return Err(semantic_error_at_loc(
                &ternary.loc,
                format!(
                    "Ternary operator condition must be boolean, got {}",
                    cond_type
                ),
            ));
        }

        // 推断两个分支的类型
        let true_type = self.infer_expr_type_internal(&ternary.true_branch)?;
        let false_type = self.infer_expr_type_internal(&ternary.false_branch)?;

        // 两个分支类型必须兼容
        if true_type == false_type {
            Ok(true_type)
        } else if Self::is_numeric_type_helper(&true_type)
            && Self::is_numeric_type_helper(&false_type)
        {
            // 数值类型进行类型提升
            Ok(self.promote_types(&true_type, &false_type))
        } else if true_type.is_null_literal() && false_type.is_reference_type() {
            // null 与引用类型合并：结果为非 null 侧的引用类型
            Ok(false_type)
        } else if false_type.is_null_literal() && true_type.is_reference_type() {
            Ok(true_type)
        } else {
            Err(semantic_error_at_loc(
                &ternary.loc,
                format!(
                    "Ternary operator branches must have compatible types, got {} and {}",
                    true_type, false_type
                ),
            ))
        }
    }

    /// 6.2.x: 推断 if 表达式类型
    ///
    /// `if (cond) { stmts; tail } else { stmts; tail }`。
    /// 分支语句在新作用域中检查（局部变量对 tail 可见），
    /// 两分支 tail 类型相同则取之，皆为数值则类型提升，否则报错。
    pub(crate) fn infer_if_type(
        &mut self,
        if_expr: &crate::ast::IfExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 推断条件表达式类型
        let cond_type = self.infer_expr_type_internal(&if_expr.condition)?;
        if cond_type != Type::Bool {
            return Err(semantic_error_at_loc(
                &if_expr.loc,
                format!(
                    "if expression condition must be boolean, got {}",
                    cond_type
                ),
            ));
        }

        let expected_return = self.current_return_type.clone();
        let infer_branch = |analyzer: &mut Self,
                            branch: &crate::ast::Block|
         -> crate::miette_diagnostic::CayResult<Type> {
            analyzer.symbol_table.enter_scope();
            let result = (|| {
                for stmt in &branch.statements {
                    analyzer.type_check_statement(stmt, expected_return.as_ref())?;
                }
                let tail = branch.tail_expr.as_ref().ok_or_else(|| {
                    semantic_error_at_loc(&branch.loc, "if expression branch must end with an expression (without semicolon)".to_string())
                })?;
                analyzer.infer_expr_type_internal(tail)
            })();
            analyzer.symbol_table.exit_scope();
            result
        };

        let then_type = infer_branch(self, &if_expr.then_branch)?;
        let else_type = infer_branch(self, &if_expr.else_branch)?;

        // 两个分支类型必须兼容（与三元运算符一致的合并规则）
        if then_type == else_type {
            Ok(then_type)
        } else if Self::is_numeric_type_helper(&then_type)
            && Self::is_numeric_type_helper(&else_type)
        {
            Ok(self.promote_types(&then_type, &else_type))
        } else if then_type.is_null_literal() && else_type.is_reference_type() {
            // null 与引用类型合并：结果为非 null 侧的引用类型
            Ok(else_type)
        } else if else_type.is_null_literal() && then_type.is_reference_type() {
            Ok(then_type)
        } else {
            Err(semantic_error_at_loc(
                &if_expr.loc,
                format!(
                    "if expression branches must have compatible types, got {} and {}",
                    then_type, else_type
                ),
            ))
        }
    }

    /// 6.1.0: 推断 ? 运算符表达式类型
    ///
    /// `expr?` 支持两种操作数类型：
    /// - `Result<T, E>`：当前函数返回类型也必须为 `Result<T2, E2>`，要求 T==T2 且
    ///   （E==E2 或 E 实现 `Into<E2>`，codegen 在 err 分支插入 `e.into()`）。
    /// - `Optional<T>`：当前函数返回类型必须为 `Optional<U>`，要求 T==U；
    ///   空值分支构造 `Optional<U>.empty()` 提前返回（U==T 已由 value 类型检查保证）。
    ///
    /// 推断结果类型为操作数中包裹的值类型（T）。
    pub(crate) fn infer_try_type(
        &mut self,
        try_expr: &crate::ast::TryExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        use crate::types::Type;

        let expr_type = self.infer_expr_type_internal(&try_expr.expr)?;

        // 解析基础类名与类型实参：支持 Type::Generic 与 Type::Object 两种表示，
        // 同时支持裸名（如 "Result"/"Optional"）和限定名（如 "std::Result"）。
        let (base_name, type_args) = match &expr_type {
            Type::Generic(name, args) => (name.clone(), args.clone()),
            Type::Object(name) => {
                if let Some(pos) = name.find('<') {
                    let base = name[..pos].to_string();
                    let end = name.len().saturating_sub(1);
                    let args_str = if end > pos + 1 {
                        &name[pos + 1..end]
                    } else {
                        ""
                    };
                    let args: Vec<Type> = self
                        .split_type_arguments(args_str)
                        .iter()
                        .map(|s| Type::Object(s.clone()))
                        .collect();
                    (base, args)
                } else {
                    (name.clone(), Vec::new())
                }
            }
            _ => (String::new(), Vec::new()),
        };

        let is_result = base_name == "Result" || base_name == "std::Result";
        let is_optional = base_name == "Optional" || base_name == "std::Optional";

        if !is_result && !is_optional {
            return Err(semantic_error_at_loc(
                &try_expr.loc,
                format!(
                    "The '?' operator can only be used on Result<T, E> or Optional<T>, got {}",
                    expr_type
                ),
            ));
        }

        if is_result && type_args.len() != 2 {
            return Err(semantic_error_at_loc(
                &try_expr.loc,
                format!(
                    "Result<T, E> requires 2 type arguments, got {}",
                    type_args.len()
                ),
            ));
        }
        if is_optional && type_args.len() != 1 {
            return Err(semantic_error_at_loc(
                &try_expr.loc,
                format!(
                    "Optional<T> requires 1 type argument, got {}",
                    type_args.len()
                ),
            ));
        }

        let value_type = type_args[0].clone();
        let error_type = if is_result {
            Some(type_args[1].clone())
        } else {
            None
        };

        // 检查当前函数返回类型是否兼容
        let return_type = self.current_return_type.clone().ok_or_else(|| {
            semantic_error_at_loc(
                &try_expr.loc,
                "The '?' operator can only be used inside a function with a return type".to_string(),
            )
        })?;

        let (ret_base, ret_args) = match &return_type {
            Type::Generic(name, args) => (name.clone(), args.clone()),
            Type::Object(name) => {
                if let Some(pos) = name.find('<') {
                    let base = name[..pos].to_string();
                    let end = name.len().saturating_sub(1);
                    let args_str = if end > pos + 1 {
                        &name[pos + 1..end]
                    } else {
                        ""
                    };
                    let args: Vec<Type> = self
                        .split_type_arguments(args_str)
                        .iter()
                        .map(|s| Type::Object(s.clone()))
                        .collect();
                    (base, args)
                } else {
                    (name.clone(), Vec::new())
                }
            }
            _ => (String::new(), Vec::new()),
        };

        let ret_is_result = ret_base == "Result" || ret_base == "std::Result";
        let ret_is_optional = ret_base == "Optional" || ret_base == "std::Optional";

        // 操作数类型与函数返回类型必须是同一种「结果/可选」类型。
        if is_result
            && (!ret_is_result || ret_args.len() != 2)
        {
            return Err(semantic_error_at_loc(
                &try_expr.loc,
                format!(
                    "The '?' operator on Result<T, E> requires the enclosing function to return Result<T, E>, got {}",
                    return_type
                ),
            ));
        }
        if is_optional
            && (!ret_is_optional || ret_args.len() != 1)
        {
            return Err(semantic_error_at_loc(
                &try_expr.loc,
                format!(
                    "The '?' operator on Optional<T> requires the enclosing function to return Optional<U>, got {}",
                    return_type
                ),
            ));
        }

        let ret_value_type = ret_args[0].clone();

        if ret_value_type != value_type {
            // T ≠ T2/U：value 类型必须严格相等，与原 Result 路径行为对齐。
            // 若未来放宽到 T: Into<U>，可在此处扩展（codegen 在 ok 分支插入 t.into()）。
            return Err(semantic_error_at_loc(
                &try_expr.loc,
                format!(
                    "The '?' operator value type {} does not match function return value type {}",
                    value_type, ret_value_type
                ),
            ));
        }

        // Result 路径：错误类型必须严格匹配或实现 Into<E2>。
        if let Some(error_type) = error_type {
            let ret_error_type = ret_args[1].clone();
            if ret_error_type != error_type {
                // E ≠ E2：E 实现 Into<E2> 时允许，codegen 在 err 分支插入 e.into()
                // （ROADMAP 6.1.x：return Result::err(e.into())）
                if !self.class_implements_into(&error_type, &ret_error_type) {
                    return Err(semantic_error_at_loc(
                        &try_expr.loc,
                        format!(
                            "The '?' operator error type {} does not match function return error type {}, and {} does not implement Into<{}>",
                            error_type, ret_error_type, error_type, ret_error_type
                        ),
                    ));
                }
            }
        }
        // Optional 路径：没有「错误类型」需要转换，空值分支直接构造
        // Optional<U>.empty() 返回（U == T 已由上面的 value 类型检查保证）。

        Ok(value_type)
    }

    /// 检查错误类型 `error_type` 是否实现了 `Into<target>`（沿父类链查找）。
    ///
    /// 不能复用 `is_subtype_of`——它只比裸接口名，会把 Into<A>/Into<B> 视为同一接口；
    /// 这里比对完整类型实参。未解析的 GenericParam 一律返回 false（上层报错）。
    pub(crate) fn class_implements_into(&self, error_type: &Type, target: &Type) -> bool {
        let class_name = match error_type {
            Type::Object(name) | Type::Generic(name, _) => {
                name.split('<').next().unwrap_or(name).to_string()
            }
            _ => return false,
        };

        let mut current = Some(class_name);
        let mut visited = std::collections::HashSet::new();
        while let Some(name) = current {
            if !visited.insert(name.clone()) {
                break;
            }
            let class_info = match self.type_registry.get_class(&name) {
                Some(info) => info,
                None => return false,
            };
            for iface in &class_info.interfaces {
                let bare = match iface {
                    Type::Object(n) | Type::Generic(n, _) => n.split('<').next().unwrap_or(n),
                    _ => continue,
                };
                if bare != "Into" && bare != "std::Into" {
                    continue;
                }
                if let Type::Generic(_, args) = iface {
                    if args.len() == 1 && &args[0] == target {
                        return true;
                    }
                }
            }
            current = class_info.parent.clone();
        }
        false
    }

    /// 推断 instanceof 表达式类型
    pub(crate) fn infer_instanceof_type(
        &mut self,
        instanceof: &InstanceOfExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 检查表达式类型
        let expr_type = self.infer_expr_type_internal(&instanceof.expr)?;

        // 检查目标类型是否存在（类或接口）
        match &instanceof.target_type {
            Type::Object(class_name) => {
                if !self.type_registry.class_exists(class_name)
                    && !self.type_registry.interface_exists(class_name)
                {
                    return Err(semantic_error_at_loc(
                        &instanceof.loc,
                        format!("Unknown type in instanceof: {}", class_name),
                    ));
                }
            }
            _ => {
                // instanceof 只能用于引用类型
                return Err(semantic_error_at_loc(
                    &instanceof.loc,
                    format!(
                        "instanceof can only be used with reference types, got {}",
                        instanceof.target_type
                    ),
                ));
            }
        }

        // instanceof 返回布尔类型
        Ok(Type::Bool)
    }

    /// 辅助方法：检查类型是否为数值类型
    pub(crate) fn is_numeric_type_helper(ty: &Type) -> bool {
        matches!(
            ty,
            // 内置数值类型
            Type::Int32 | Type::Int64 | Type::Float32 | Type::Float64 | Type::Char |
            // FFI 数值类型
            Type::CInt | Type::CUInt | Type::CLong | Type::CULong |
            Type::CShort | Type::CUShort | Type::CChar | Type::CUChar |
            Type::CFloat | Type::CDouble | Type::SizeT | Type::SSizeT |
            Type::UIntPtr | Type::IntPtr
        )
    }

    /// 替换类型中的泛型参数为实际类型
    ///
    /// 例如：将 GenericParam("T") 替换为 Int32
    pub(crate) fn substitute_type_params(
        &self,
        ty: &Type,
        type_params: &[crate::types::TypeParamInfo],
        type_args: &[Type],
    ) -> Type {
        match ty {
            Type::GenericParam(name) => {
                // 查找泛型参数在列表中的位置
                if let Some(idx) = type_params.iter().position(|p| &p.name == name) {
                    if idx < type_args.len() {
                        return type_args[idx].clone();
                    }
                }
                ty.clone()
            }
            Type::Array(elem) => Type::Array(Box::new(self.substitute_type_params(
                elem,
                type_params,
                type_args,
            ))),
            Type::Generic(name, args) => {
                let new_args = args
                    .iter()
                    .map(|arg| self.substitute_type_params(arg, type_params, type_args))
                    .collect();
                Type::Generic(name.clone(), new_args)
            }
            Type::Function(func_type) => {
                let new_params = func_type
                    .params
                    .iter()
                    .map(|p| self.substitute_type_params(p, type_params, type_args))
                    .collect();
                let new_return =
                    self.substitute_type_params(&func_type.return_type, type_params, type_args);
                Type::Function(Box::new(crate::types::FunctionType {
                    params: new_params,
                    return_type: Box::new(new_return),
                    is_static: func_type.is_static,
                    is_closure: func_type.is_closure,
                }))
            }
            Type::Pointer(inner) => Type::Pointer(Box::new(self.substitute_type_params(
                inner,
                type_params,
                type_args,
            ))),
            _ => ty.clone(),
        }
    }

    /// 将类型字符串解析为 Type
    /// 用于解析泛型类型参数，如 "int", "String", "long" 等
    pub(crate) fn parse_type_string(&self, type_str: &str) -> Type {
        let type_str = type_str.trim();
        if let Some(pos) = type_str.find('<') {
            if type_str.ends_with('>') {
                let base = type_str[..pos].trim();
                let args_str = &type_str[pos + 1..type_str.len() - 1];
                let args = self
                    .split_type_arguments(args_str)
                    .into_iter()
                    .map(|arg| self.parse_type_string(&arg))
                    .collect();
                return Type::Generic(base.to_string(), args);
            }
        }

        match type_str {
            "int" => Type::Int32,
            "long" => Type::Int64,
            "float" => Type::Float32,
            "double" => Type::Float64,
            "boolean" | "bool" => Type::Bool,
            "char" => Type::Char,
            "String" | "string" => Type::String,
            "void" => Type::Void,
            // 检查是否是已注册的类或结构体
            name => {
                if self.type_registry.class_exists(name)
                    || self.type_registry.get_struct(name).is_some()
                {
                    Type::Object(name.to_string())
                } else {
                    // 未知类型，返回 Object 作为占位符
                    Type::Object(name.to_string())
                }
            }
        }
    }
}
