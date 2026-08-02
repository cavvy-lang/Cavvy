//! 函数调用表达式代码生成 - 实例方法级泛型（method<U>）单态化
//!
//! 实例泛型方法（如 `Result<T, E>::map<U>(fn(T) -> U)`）的方法级类型参数
//! 在调用点从实参（通常是 lambda）推断，并按 (类类型实参, 方法类型实参)
//! 组合发射独立的单态化函数副本——不擦除为 i8*，否则闭包返回类型与
//! 字段布局都会错位。
//!
//! 命名：调用点与定义点共用 `mangle_method_with_type_args`，把方法级类型
//! 实参改编进函数名（如 `Result<int, String>::map<long>`）。
//!
//! 类型推断：`infer_instance_generic_method_return_type` 与发射路径共用同一套
//! 「类级替换 → 方法级类型实参推断」，供 `auto r2 = r.map(...)` 与链式调用
//! `r.map(f).getValue()` 解析外层接收者的具体类型。lambda 实参按单态化计划
//! 中的期望 fn 类型发射签名（`pending_lambda_expected_fn`）。
//!
//! 块体 lambda（`-> { ... }`）参与方法级类型实参推断：扫描块内 return
//! 语句，用与表达式体相同的「按期望形参类型合成 fn 类型」策略推断返回类型，
//! 多个 return 取首个并做数值提升；无 return 则视为 void。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::types::{FunctionType, Type};

/// 一次实例泛型方法调用的单态化计划：
/// 调用点使用特化函数名、具体形参类型与具体返回类型发射调用。
pub(crate) struct MethodGenericPlan {
    pub fn_name: String,
    pub param_types: Vec<Type>,
    pub ret_type: Type,
}

impl IRGenerator {
    /// 若本次实例方法调用的目标是「带方法级类型参数的实例泛型方法」，
    /// 推断方法级类型实参、懒生成特化方法体，并返回单态化调用计划。
    /// 否则返回 None，调用点走常规路径。
    pub(crate) fn prepare_instance_generic_method_call(
        &mut self,
        class_name: &str,
        method_name: &str,
        actual_args: &[Expr],
    ) -> Option<MethodGenericPlan> {
        // 类级类型实参映射（如 Result<int, String> -> {T: int, E: String}）
        // 立即取出所需数据的所有权副本，避免与后续 &mut self 懒生成冲突。
        let (owner_name, class_mapping, methods) = {
            let (owner_name, class_info) = self.lookup_owner_class(class_name)?;
            let methods = class_info.methods.get(method_name).cloned()?;
            let class_mapping = self.build_specialization_mapping(class_name, class_info);
            (owner_name, class_mapping, methods)
        };

        // 同名候选方法
        let has_generic_candidate = methods
            .iter()
            .any(|m| !m.is_static && !m.type_params.is_empty());
        if !has_generic_candidate {
            return None;
        }
        let methods = methods;
        let positional_args: Vec<&Expr> = actual_args
            .iter()
            .filter(|a| !matches!(a, Expr::NamedArg(_)))
            .collect();

        // 非泛型重载优先：若某个不带方法级类型参数的重载在类级替换后
        // 与实参逐一匹配，则让常规路径处理，避免遮蔽。
        let arg_types: Vec<Option<Type>> = positional_args
            .iter()
            .map(|a| self.get_expression_type(a))
            .collect();
        for method in &methods {
            if !method.type_params.is_empty() || method.is_static {
                continue;
            }
            if method.params.len() != positional_args.len() {
                continue;
            }
            let substituted: Vec<Type> = method
                .params
                .iter()
                .map(|p| crate::types::substitute_type_params(&p.param_type, &class_mapping))
                .collect();
            let all_match = substituted
                .iter()
                .zip(arg_types.iter())
                .all(|(p, a)| matches!(a, Some(a) if Self::generic_shadow_types_match(p, a)));
            if all_match {
                return None;
            }
        }

        // 逐泛型候选：类级替换 → 方法级类型实参推断 → 完全特化
        for method in &methods {
            if method.is_static || method.type_params.is_empty() {
                continue;
            }
            if method.params.len() != positional_args.len() {
                continue;
            }
            let class_substituted: Vec<crate::types::ParameterInfo> = method
                .params
                .iter()
                .map(|p| crate::types::ParameterInfo {
                    name: p.name.clone(),
                    param_type: crate::types::substitute_type_params(
                        &p.param_type,
                        &class_mapping,
                    ),
                    is_varargs: p.is_varargs,
                })
                .collect();

            let Some(method_args) =
                self.infer_method_type_args(&class_substituted, &positional_args, &method.type_params)
            else {
                continue;
            };

            // 完整映射 = 类级映射 + 方法级映射
            let mut full_mapping = class_mapping.clone();
            for (param, arg) in method.type_params.iter().zip(method_args.iter()) {
                full_mapping.insert(param.name.clone(), arg.clone());
            }

            let param_types: Vec<Type> = class_substituted
                .iter()
                .map(|p| crate::types::substitute_type_params(&p.param_type, &full_mapping))
                .collect();
            let ret_type = crate::types::substitute_type_params(&method.return_type, &full_mapping);

            let fn_name =
                self.mangle_instance_generic_method(class_name, method_name, &method_args, &param_types);

            // 懒生成特化方法体（幂等）
            self.ensure_instance_method_specialization(
                &owner_name,
                method_name,
                method,
                class_name,
                &full_mapping,
                &fn_name,
            );
            return Some(MethodGenericPlan {
                fn_name,
                param_types,
                ret_type,
            });
        }

        None
    }

    /// 推断实例泛型方法调用的具体返回类型（纯推断，不触发懒生成）。
    ///
    /// 与发射路径 prepare_instance_generic_method_call 共用同一套
    /// 「类级替换 → 方法级类型实参推断 → 完整替换」，保证类型结论一致。
    /// 供 `auto` 变量声明与链式调用（`r.map<U>(f).then()`）的接收者
    /// 类型推断使用：此时方法级类型参数只在调用点可推断。
    pub(crate) fn infer_instance_generic_method_return_type(
        &self,
        class_name: &str,
        method_name: &str,
        actual_args: &[Expr],
    ) -> Option<Type> {
        let (_owner_name, class_info) = self.lookup_owner_class(class_name)?;
        let methods = class_info.methods.get(method_name).cloned()?;
        let class_mapping = self.build_specialization_mapping(class_name, class_info);
        let positional_args: Vec<&Expr> = actual_args
            .iter()
            .filter(|a| !matches!(a, Expr::NamedArg(_)))
            .collect();

        for method in &methods {
            if method.is_static || method.type_params.is_empty() {
                continue;
            }
            if method.params.len() != positional_args.len() {
                continue;
            }
            let class_substituted: Vec<crate::types::ParameterInfo> = method
                .params
                .iter()
                .map(|p| crate::types::ParameterInfo {
                    name: p.name.clone(),
                    param_type: crate::types::substitute_type_params(
                        &p.param_type,
                        &class_mapping,
                    ),
                    is_varargs: p.is_varargs,
                })
                .collect();

            let Some(method_args) =
                self.infer_method_type_args(&class_substituted, &positional_args, &method.type_params)
            else {
                continue;
            };

            let mut full_mapping = class_mapping.clone();
            for (param, arg) in method.type_params.iter().zip(method_args.iter()) {
                full_mapping.insert(param.name.clone(), arg.clone());
            }
            return Some(crate::types::substitute_type_params(
                &method.return_type,
                &full_mapping,
            ));
        }

        None
    }

    /// 查找方法的属主类（去掉类名中的泛型实参，支持继承暂按本类处理）。
    fn lookup_owner_class(
        &self,
        class_name: &str,
    ) -> Option<(String, &crate::types::ClassInfo)> {
        let base = class_name.split('<').next().unwrap_or(class_name);
        let registry = self.type_registry.as_ref()?;
        let class_info = registry
            .get_class(base)
            .or_else(|| {
                let bare = base.rsplit("::").next().unwrap_or(base);
                registry.get_class(bare)
            })
            .or_else(|| {
                registry
                    .find_qualified_class(base)
                    .and_then(|q| registry.get_class(&q))
            })?;
        Some((class_info.name.clone(), class_info))
    }

    /// 非泛型重载遮蔽检查用的宽松类型相等：函数类型按结构比较。
    fn generic_shadow_types_match(param: &Type, arg: &Type) -> bool {
        match (param, arg) {
            (Type::Function(_), Type::Function(_)) => true,
            _ => param == arg,
        }
    }

    /// 从调用实参推断方法级泛型类型实参（codegen 阶段）。
    ///
    /// 与 var_decl.rs 的 infer_type_args_from_call_args_codegen 同构，
    /// 但实参类型通过 `infer_generic_call_arg_type` 获取，支持 lambda 实参
    /// （get_expression_type 对 lambda 返回 None）。
    fn infer_method_type_args(
        &self,
        method_params: &[crate::types::ParameterInfo],
        call_args: &[&Expr],
        type_params: &[crate::types::TypeParamInfo],
    ) -> Option<Vec<Type>> {
        let mut inferred: Vec<Option<Type>> = vec![None; type_params.len()];

        for (param, arg) in method_params.iter().zip(call_args.iter()) {
            let arg_type = self.infer_generic_call_arg_type(arg, &param.param_type)?;
            Self::collect_generic_substitution(
                &param.param_type,
                &arg_type,
                type_params,
                &mut inferred,
            )?;
        }

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

    /// 递归比较形参类型与实参类型，收集方法级泛型参数映射。
    /// 返回类型优先于参数（与语义阶段 infer_generic_substitution 一致）。
    fn collect_generic_substitution(
        param_type: &Type,
        arg_type: &Type,
        type_params: &[crate::types::TypeParamInfo],
        inferred: &mut [Option<Type>],
    ) -> Option<()> {
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
                    Self::collect_generic_substitution(p, a, type_params, inferred)?;
                }
            }
            (Type::Array(p_inner), Type::Array(a_inner)) => {
                Self::collect_generic_substitution(p_inner, a_inner, type_params, inferred)?;
            }
            (Type::Pointer(p_inner), Type::Pointer(a_inner)) => {
                Self::collect_generic_substitution(p_inner, a_inner, type_params, inferred)?;
            }
            (Type::Function(p_ft), Type::Function(a_ft))
                if p_ft.params.len() == a_ft.params.len() =>
            {
                Self::collect_generic_substitution(
                    &p_ft.return_type,
                    &a_ft.return_type,
                    type_params,
                    inferred,
                )?;
                for (p, a) in p_ft.params.iter().zip(a_ft.params.iter()) {
                    Self::collect_generic_substitution(p, a, type_params, inferred)?;
                }
            }
            _ => {}
        }
        Some(())
    }

    /// 获取泛型方法调用实参的类型。lambda 实参按期望函数类型合成 fn 类型，
    /// 其余实参走 get_expression_type。
    fn infer_generic_call_arg_type(&self, arg: &Expr, expected_param: &Type) -> Option<Type> {
        if let Expr::Lambda(lambda) = arg {
            if let Type::Function(expected_ft) = expected_param {
                return self.lambda_fn_type_for_expected(lambda, expected_ft);
            }
            return None;
        }
        self.get_expression_type(arg)
    }

    /// 按期望函数类型合成 lambda 的 fn 类型：
    /// 形参取显式注解（缺省用期望形参类型，最终回退 int，与语义阶段一致），
    /// 返回类型从 lambda 体推断。
    fn lambda_fn_type_for_expected(
        &self,
        lambda: &LambdaExpr,
        expected_ft: &FunctionType,
    ) -> Option<Type> {
        if lambda.params.len() != expected_ft.params.len() {
            return None;
        }
        let params: Vec<Type> = lambda
            .params
            .iter()
            .enumerate()
            .map(|(i, p)| {
                p.param_type
                    .clone()
                    .or_else(|| expected_ft.params.get(i).cloned())
                    .unwrap_or(Type::Int32)
            })
            .collect();
        let return_type = match &lambda.body {
            LambdaBody::Expr(expr) => {
                self.infer_lambda_body_expr_type(expr, &lambda.params, &params)?
            }
            // 块体 lambda：扫描 return 语句推断返回类型。
            // 与表达式体共用「按期望形参类型替换裸标识符」的推断路径，
            // 多个 return 取首个非空推断并做数值提升；无 return 视为 void。
            // 语义阶段会拦截 return 类型不一致的错误，这里只负责推断。
            LambdaBody::Block(block) => self.infer_lambda_block_return_type(
                block,
                &lambda.params,
                &params,
            )?,
        };
        Some(Type::Function(Box::new(FunctionType {
            params,
            return_type: Box::new(return_type),
            is_static: false,
            is_closure: true,
        })))
    }

    /// 推断 lambda 单表达式体的类型（仅覆盖推断方法级类型实参所需的常见形态）。
    fn infer_lambda_body_expr_type(
        &self,
        expr: &Expr,
        lambda_params: &[LambdaParam],
        param_types: &[Type],
    ) -> Option<Type> {
        match expr {
            Expr::Cast(cast) => Some(cast.target_type.clone()),
            Expr::Literal(_) => self.get_expression_type(expr),
            Expr::Identifier(name) => lambda_params
                .iter()
                .position(|p| p.name == name.as_ref())
                .map(|idx| param_types[idx].clone())
                .or_else(|| self.get_expression_type(expr)),
            Expr::Binary(binary) => {
                let left =
                    self.infer_lambda_body_expr_type(&binary.left, lambda_params, param_types)?;
                let right =
                    self.infer_lambda_body_expr_type(&binary.right, lambda_params, param_types)?;
                // 算术提升：double > float > 左操作数
                if matches!(left, Type::Float64) || matches!(right, Type::Float64) {
                    Some(Type::Float64)
                } else if matches!(left, Type::Float32) || matches!(right, Type::Float32) {
                    Some(Type::Float32)
                } else {
                    Some(left)
                }
            }
            Expr::Unary(unary) => {
                self.infer_lambda_body_expr_type(&unary.operand, lambda_params, param_types)
            }
            _ => self.get_expression_type(expr),
        }
    }

    /// 推断 lambda 块体的返回类型。
    ///
    /// 扫描块中所有 `return expr;` 语句，对每个返回表达式按
    /// `infer_lambda_body_expr_type` 推断（与表达式体共用同一套策略，
    /// 让 `return x + 1;` 与 `x + 1` 得到一致的类型结论）。
    ///
    /// 多个 return 的处理：
    /// - 首个 return 确定初始类型；
    /// - 后续 return 与之做数值提升（同为数值时取较宽类型），类型不一致时
    ///   保留首个——语义阶段已校验一致性，这里只负责推断，错误由语义阶段报。
    /// - 无 return 语句视为 `void`。
    ///
    /// 嵌套块（Stmt::Block）递归扫描，确保 `if (...) { return x; }`
    /// 这类控制流内的 return 也参与推断。
    fn infer_lambda_block_return_type(
        &self,
        block: &Block,
        lambda_params: &[LambdaParam],
        param_types: &[Type],
    ) -> Option<Type> {
        let mut inferred: Option<Type> = None;

        for stmt in &block.statements {
            self.collect_block_return_types(stmt, lambda_params, param_types, &mut inferred);
        }
        // tail_expr 也是隐式 return
        if let Some(tail) = &block.tail_expr {
            if let Some(ty) =
                self.infer_lambda_body_expr_type(tail, lambda_params, param_types)
            {
                self.merge_inferred_return(&mut inferred, ty);
            }
        }
        Some(inferred.unwrap_or(Type::Void))
    }

    /// 递归从语句中收集 return 表达式的推断类型。
    /// 嵌套 Block / If / While / For / Do-While 都递归扫描。
    fn collect_block_return_types(
        &self,
        stmt: &Stmt,
        lambda_params: &[LambdaParam],
        param_types: &[Type],
        inferred: &mut Option<Type>,
    ) {
        match stmt {
            Stmt::Return(Some(ret_expr)) => {
                if let Some(ty) =
                    self.infer_lambda_body_expr_type(ret_expr, lambda_params, param_types)
                {
                    self.merge_inferred_return(inferred, ty);
                }
            }
            Stmt::Block(block) => {
                for s in &block.statements {
                    self.collect_block_return_types(s, lambda_params, param_types, inferred);
                }
                if let Some(tail) = &block.tail_expr {
                    if let Some(ty) = self
                        .infer_lambda_body_expr_type(tail, lambda_params, param_types)
                    {
                        self.merge_inferred_return(inferred, ty);
                    }
                }
            }
            Stmt::If(if_stmt) => {
                self.collect_block_return_types(
                    &if_stmt.then_branch,
                    lambda_params,
                    param_types,
                    inferred,
                );
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.collect_block_return_types(
                        else_branch,
                        lambda_params,
                        param_types,
                        inferred,
                    );
                }
            }
            Stmt::While(w) => {
                self.collect_block_return_types(
                    &w.body,
                    lambda_params,
                    param_types,
                    inferred,
                );
            }
            Stmt::DoWhile(dw) => {
                self.collect_block_return_types(
                    &dw.body,
                    lambda_params,
                    param_types,
                    inferred,
                );
            }
            Stmt::For(f) => {
                self.collect_block_return_types(
                    &f.body,
                    lambda_params,
                    param_types,
                    inferred,
                );
            }
            Stmt::ForEach(fe) => {
                self.collect_block_return_types(
                    &fe.body,
                    lambda_params,
                    param_types,
                    inferred,
                );
            }
            Stmt::Switch(sw) => {
                for case in &sw.cases {
                    for s in &case.body {
                        self.collect_block_return_types(
                            s,
                            lambda_params,
                            param_types,
                            inferred,
                        );
                    }
                }
                if let Some(default) = &sw.default {
                    for s in default {
                        self.collect_block_return_types(
                            s,
                            lambda_params,
                            param_types,
                            inferred,
                        );
                    }
                }
            }
            _ => {}
        }
    }

    /// 合并新推断的返回类型到已有结论中。
    /// - 首次设置直接采纳；
    /// - 数值类型做提升；
    /// - 类型不同且非数值时保留首个（语义阶段会报错）。
    fn merge_inferred_return(&self, inferred: &mut Option<Type>, new_ty: Type) {
        match inferred {
            None => *inferred = Some(new_ty),
            Some(existing) => {
                // 数值类型提升
                if Self::is_numeric_type_for_inference(existing)
                    && Self::is_numeric_type_for_inference(&new_ty)
                {
                    if matches!(new_ty, Type::Float64)
                        || matches!(existing, Type::Float64)
                    {
                        *existing = Type::Float64;
                    } else if matches!(new_ty, Type::Float32)
                        || matches!(existing, Type::Float32)
                    {
                        *existing = Type::Float32;
                    } else if matches!(new_ty, Type::Int64)
                        || matches!(existing, Type::Int64)
                    {
                        *existing = Type::Int64;
                    }
                    // 否则保留原 Int32
                }
                // 类型不一致或不可提升：保留首个，语义阶段会拦截
            }
        }
    }

    /// 数值类型判定（用于 return 类型合并时的提升决策）。
    fn is_numeric_type_for_inference(t: &Type) -> bool {
        matches!(
            t,
            Type::Int32
                | Type::Int64
                | Type::Float32
                | Type::Float64
                | Type::Char
                | Type::Bool
        )
    }

    /// 生成实例泛型方法特化副本的函数名。
    /// 调用点与定义点（懒生成）共用此函数，保证名字一致。
    fn mangle_instance_generic_method(
        &self,
        class_name: &str,
        method_name: &str,
        method_type_args: &[Type],
        param_types: &[Type],
    ) -> String {
        let mut mangler = crate::codegen::itanium_mangle::ItaniumMangler::new(
            self.type_registry.as_ref(),
            &self.class_namespaces,
            self.is_windows_target(),
        );
        mangler.mangle_method_with_type_args(class_name, method_name, method_type_args, param_types)
    }

    /// 懒生成实例泛型方法的特化副本（幂等：generated_methods 去重）。
    ///
    /// 方法体的形参/返回类型按完整映射（类级 + 方法级）替换为具体类型，
    /// 并以完整映射安装 generic_type_args，使体内的 GenericParam("U")
    /// 经 type_to_llvm 解析为具体 LLVM 类型（如 i64），而非回退 i8*。
    fn ensure_instance_method_specialization(
        &mut self,
        owner_name: &str,
        method_name: &str,
        method_info: &crate::types::MethodInfo,
        specialized_class_name: &str,
        full_mapping: &std::collections::HashMap<String, Type>,
        fn_name: &str,
    ) {
        if self.generated_methods.contains(fn_name) {
            return;
        }

        // 定位属主类 AST 中的方法定义（含方法体）
        let bare = owner_name.rsplit("::").next().unwrap_or(owner_name);
        let Some(class_decl) = self
            .classes_cache
            .get(owner_name)
            .or_else(|| self.classes_cache.get(bare))
            .cloned()
        else {
            return;
        };
        let method_decl = class_decl.members.iter().find_map(|m| {
            if let ClassMember::Method(method) = m {
                if method.name == method_name
                    && method.params.len() == method_info.params.len()
                    && !method.type_params.is_empty()
                {
                    return Some(method.clone());
                }
            }
            None
        });
        let Some(mut method_decl) = method_decl else {
            return;
        };

        // 形参与返回类型替换为具体类型（方法体 AST 中的类型引用经
        // generic_type_args 在生成时解析，无需改写方法体）
        method_decl.return_type =
            crate::types::substitute_type_params(&method_decl.return_type, full_mapping);
        method_decl.params = method_decl
            .params
            .iter()
            .map(|p| crate::types::ParameterInfo {
                name: p.name.clone(),
                param_type: crate::types::substitute_type_params(&p.param_type, full_mapping),
                is_varargs: p.is_varargs,
            })
            .collect();

        let mapping = full_mapping.clone();
        let fn_name_owned = fn_name.to_string();
        let class_name_owned = specialized_class_name.to_string();
        self.with_deferred_codegen(move |s| {
            s.generic_type_args = mapping;
            let _ = s.generate_method_with_name(&class_name_owned, &method_decl, fn_name_owned);
        });
    }

    /// 方法是否声明了方法级类型参数（用于禁用 vtable 动态分派：
    /// 泛型方法不入 vtable，必须直接调用其单态化副本）。
    pub(crate) fn method_has_method_level_type_params(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> bool {
        let base = class_name.split('<').next().unwrap_or(class_name);
        self.type_registry
            .as_ref()
            .and_then(|r| r.get_class(base))
            .and_then(|c| c.methods.get(method_name))
            .is_some_and(|methods| methods.iter().any(|m| !m.type_params.is_empty()))
    }
}
