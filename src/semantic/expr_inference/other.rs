//! 其他表达式类型推断

use super::super::analyzer::SemanticAnalyzer;
use super::helpers::semantic_error_at_loc;
use crate::ast::*;
use crate::miette_diagnostic::{ErrorCodes, semantic_error_with_file};
use crate::types::Type;

impl SemanticAnalyzer {
    /// 推断 new 表达式类型
    pub(crate) fn infer_new_type(
        &mut self,
        new_expr: &NewExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 解析泛型类名: "Optional<T>" -> ("Optional", Some(["T"]))
        // 支持多类型参数: "Pair<K, V>" -> ("Pair", Some(["K", "V"]))
        let (base_class_name, type_params) = if let Some(pos) = new_expr.class_name.find('<') {
            let base = &new_expr.class_name[..pos];
            let param_start = pos + 1;
            let param_end = new_expr.class_name.len().saturating_sub(1);
            let params_str = if param_end > param_start {
                &new_expr.class_name[param_start..param_end]
            } else {
                ""
            };
            // 解析多个类型参数，用逗号分隔
            let params: Vec<String> = params_str
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            (
                base.to_string(),
                if params.is_empty() {
                    None
                } else {
                    Some(params)
                },
            )
        } else {
            (new_expr.class_name.clone(), None)
        };

        // 先严格推断构造参数，避免非法表达式漏到代码生成阶段。
        for arg in &new_expr.args {
            self.infer_expr_type_internal(arg)?;
        }

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
                let mut matched_constructor = None;
                let mut mismatch_detail = None;
                for constructor in &class_info.constructors {
                    match self.check_arguments_compatible(
                        &new_expr.args,
                        &constructor.params,
                        new_expr.loc.line,
                        new_expr.loc.column,
                    ) {
                        Ok(()) => {
                            matched_constructor = Some(constructor.clone());
                            break;
                        }
                        Err(msg) => {
                            if mismatch_detail.is_none() {
                                mismatch_detail = Some(msg);
                            }
                        }
                    }
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

                super::helpers::check_member_access(
                    &base_class_name,
                    constructor.is_public,
                    constructor.is_protected,
                    constructor.is_private,
                    &self.current_class,
                    &base_class_name,
                    &self.type_registry,
                    &new_expr.loc,
                )?;
            }

            // 如果类有泛型参数，验证类型参数是否合法
            if !class_info.type_params.is_empty() {
                if let Some(ref params) = type_params {
                    // 检查每个类型参数是否合法
                    for param in params {
                        let is_valid_param = class_info.type_params.iter().any(|p| &p.name == param)
                            // 允许使用当前类的泛型类型参数（如 HashMap<K,V,A> 中创建 ArrayList<K>）
                            || self.current_class_type_params.iter().any(|p| &p.name == param)
                            || self.type_registry.class_exists(param)
                            || self.type_registry.get_struct(param).is_some()
                            || matches!(
                                param.as_str(),
                                "int" | "long" | "float" | "double" | "bool" | "boolean" | "char" | "String" | "string"
                            );

                        if !is_valid_param {
                            return Err(semantic_error_at_loc(
                                &new_expr.loc,
                                format!(
                                    "Unknown type parameter '{}' for class '{}'",
                                    param, base_class_name
                                ),
                            ));
                        }
                    }
                }
                // 返回泛型类型
                Ok(Type::Object(new_expr.class_name.clone()))
            } else {
                // 非泛型类
                Ok(Type::Object(base_class_name))
            }
        } else if self
            .current_class_type_params
            .iter()
            .any(|p| &p.name == &base_class_name)
        {
            // new A() where A is a type parameter of the current class.
            // The concrete type will be substituted during monomorphization.
            Ok(Type::Object(new_expr.class_name.clone()))
        } else if self.type_registry.get_struct(&base_class_name).is_some() {
            // struct 是值类型，用 Object 包装
            Ok(Type::Object(new_expr.class_name.clone()))
        } else {
            Err(semantic_error_at_loc(
                &new_expr.loc,
                format!("Unknown class or struct: {}", base_class_name),
            ))
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

    /// 6.1.0: 推断 ? 运算符表达式类型
    ///
    /// `expr?` 要求 expr 的类型为 Result<T, E>，且当前函数返回类型也必须为
    /// Result<T, E]（相同 T, E）。推断结果类型为 T。
    pub(crate) fn infer_try_type(
        &mut self,
        try_expr: &crate::ast::TryExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        use crate::types::Type;

        let expr_type = self.infer_expr_type_internal(&try_expr.expr)?;

        // 解析 Result<T, E>：支持 Type::Generic 与 Type::Object 两种表示，
        // 同时支持裸名 "Result" 和限定名 "std::Result"。
        let (base_name, type_args) = match &expr_type {
            Type::Generic(name, args) => (name.clone(), args.clone()),
            Type::Object(name) => {
                // 尝试从 Object("std::Result<int, String>") 中解析基础名和参数
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
        if !is_result || type_args.len() != 2 {
            return Err(semantic_error_at_loc(
                &try_expr.loc,
                format!(
                    "The '?' operator can only be used on Result<T, E>, got {}",
                    expr_type
                ),
            ));
        }

        let value_type = type_args[0].clone();
        let error_type = type_args[1].clone();

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
        if !ret_is_result || ret_args.len() != 2 {
            return Err(semantic_error_at_loc(
                &try_expr.loc,
                format!(
                    "The '?' operator requires the enclosing function to return Result<T, E>, got {}",
                    return_type
                ),
            ));
        }

        if ret_args[0] != value_type {
            return Err(semantic_error_at_loc(
                &try_expr.loc,
                format!(
                    "The '?' operator value type {} does not match function return value type {}",
                    value_type, ret_args[0]
                ),
            ));
        }

        if ret_args[1] != error_type {
            return Err(semantic_error_at_loc(
                &try_expr.loc,
                format!(
                    "The '?' operator error type {} does not match function return error type {}",
                    error_type, ret_args[1]
                ),
            ));
        }

        Ok(value_type)
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
            Type::CInt | Type::CUInt | Type::CLong |
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
