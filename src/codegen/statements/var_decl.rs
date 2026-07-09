//! 变量声明代码生成
//!
//! 处理变量声明和初始化的代码生成。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, semantic_error_with_file};
use crate::types::Type;

impl IRGenerator {
    /// 从表达式推断类型
    fn infer_type_from_expr(&self, expr: &Expr) -> Option<Type> {
        match expr {
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::Int32(_) => Some(Type::Int32),
                LiteralValue::Int64(_) => Some(Type::Int64),
                LiteralValue::Float32(_) => Some(Type::Float32),
                LiteralValue::Float64(_) => Some(Type::Float64),
                LiteralValue::String(_) => Some(Type::String),
                LiteralValue::Bool(_) => Some(Type::Bool),
                LiteralValue::Char(_) => Some(Type::Char),
                LiteralValue::Null => Some(Type::Object("Object".to_string())),
            },
            Expr::Identifier(name) => {
                // 从变量类型映射中查找
                self.var_types
                    .get(name.as_ref())
                    .and_then(|llvm_type| self.llvm_type_to_cay_type(llvm_type))
            }
            Expr::Binary(bin) => {
                // 对于二元表达式，尝试推断结果类型
                self.infer_type_from_expr(&bin.left)
            }
            Expr::Unary(unary) => self.infer_type_from_expr(&unary.operand),
            Expr::Call(call) => {
                // 对于函数调用，尝试从类型注册表获取返回类型
                self.infer_call_return_type(call)
            }
            Expr::MemberAccess(member) => {
                // 对于方法调用如 obj.method()，尝试推断返回类型
                if let Expr::Identifier(obj_name) = &*member.object {
                    // 获取对象类型
                    if let Some(class_name) = self.var_class_map.get(obj_name.as_ref()) {
                        return self.infer_method_return_type(class_name, &member.member);
                    }
                }
                None
            }
            Expr::New(new_expr) => {
                // new 表达式返回对象类型
                Some(Type::Object(new_expr.class_name.clone()))
            }
            Expr::Lambda(lambda) => {
                // Lambda 表达式推断为函数指针类型
                let param_types: Vec<Type> = lambda
                    .params
                    .iter()
                    .map(|p| p.param_type.clone().unwrap_or(Type::Int32))
                    .collect();
                let return_type = match &lambda.body {
                    LambdaBody::Expr(expr) => {
                        self.infer_type_from_expr(expr).unwrap_or(Type::Int32)
                    }
                    LambdaBody::Block(block) => {
                        // 查找块中的 return 语句
                        let mut ret_type = Type::Void;
                        for stmt in &block.statements {
                            if let Stmt::Return(Some(ret_expr)) = stmt {
                                ret_type =
                                    self.infer_type_from_expr(ret_expr).unwrap_or(Type::Int32);
                                break;
                            }
                        }
                        ret_type
                    }
                };
                // Lambda 总是使用闭包格式（打包结构体），所以 is_closure 总是 true
                Some(Type::Function(Box::new(crate::types::FunctionType {
                    params: param_types,
                    return_type: Box::new(return_type),
                    is_static: true,
                    is_closure: true,
                })))
            }
            _ => None, // 无法推断，返回 None
        }
    }

    /// 推断函数调用的返回类型
    pub fn infer_call_return_type(&self, call: &CallExpr) -> Option<Type> {
        // 处理内置函数
        if let Expr::Identifier(name) = call.callee.as_ref() {
            match name.as_str() {
                "print" | "println" | "eprint" | "eprintln" | "exit" => return Some(Type::Void),
                "readInt" => return Some(Type::Int32),
                "readLong" => return Some(Type::Int64),
                "readFloat" => return Some(Type::Float32),
                "readDouble" => return Some(Type::Float64),
                "readLine" => return Some(Type::String),
                "readChar" => return Some(Type::Char),
                "readBool" => return Some(Type::Bool),
                _ => {}
            }

            // 5.3.0: 省略 new 的类实例化类型推断
            if let Some(class_name) = self.try_resolve_class_instantiation(name.as_ref()) {
                return Some(Type::Object(class_name));
            }

            // 5.3.0: 命名空间式静态方法调用类型推断
            if let Some(return_type) =
                self.infer_static_method_call_return_type(name.as_ref(), &call.args)
            {
                return Some(return_type);
            }
        }

        // 尝试从类型注册表获取（支持方法重载）
        if let Some(ref registry) = self.type_registry {
            if let Expr::Identifier(name) = call.callee.as_ref() {
                // 尝试在当前类中查找
                if !self.current_class.is_empty() {
                    if let Some(method_info) =
                        registry.get_method(&self.current_class, name.as_ref())
                    {
                        return Some(method_info.return_type.clone());
                    }
                }
            } else if let Expr::MemberAccess(member) = call.callee.as_ref() {
                // 推断实参类型
                let mut arg_types: Vec<Type> = Vec::new();
                let mut arg_types_resolved = true;
                for arg in &call.args {
                    if let Some(t) = self.get_expression_type(arg) {
                        arg_types.push(t);
                    } else {
                        arg_types_resolved = false;
                        break;
                    }
                }
                let arg_types_slice: Option<&[Type]> = if arg_types_resolved {
                    Some(&arg_types)
                } else {
                    None
                };

                // obj.method() 形式
                if let Expr::Identifier(obj_name) = &*member.object {
                    let obj_name_str = obj_name.as_ref();
                    // 处理 this.method() 调用
                    if obj_name_str == "this" && !self.current_class.is_empty() {
                        let found = if let Some(types) = arg_types_slice {
                            registry.find_method(&self.current_class, &member.member, types)
                        } else {
                            None
                        }
                        .or_else(|| registry.get_method(&self.current_class, &member.member));
                        if let Some(method_info) = found {
                            return Some(method_info.return_type.clone());
                        }
                        // 简单名找不到，用限定名重试（处理 namespace 内的类）
                        if let Some(qname) = registry.find_qualified_class(&self.current_class) {
                            let found_q = if let Some(types) = arg_types_slice {
                                registry.find_method(&qname, &member.member, types)
                            } else {
                                None
                            }
                            .or_else(|| registry.get_method(&qname, &member.member));
                            if let Some(method_info) = found_q {
                                return Some(method_info.return_type.clone());
                            }
                        }
                    }
                    // 首先检查是否是已知的类名（静态方法调用）
                    if registry.class_exists(obj_name_str) {
                        // 类名.方法名() 形式，如 Vector2.right() 或 Container.make(42)
                        let found = if let Some(types) = arg_types_slice {
                            registry.find_method(obj_name_str, &member.member, types)
                        } else {
                            None
                        }
                        .or_else(|| registry.get_method(obj_name_str, &member.member));
                        if let Some(method_info) = found {
                            // 对泛型静态工厂方法，尝试从调用实参推断类型参数。
                            // 例如 Container.make(42) 推断出 T = int，使返回类型
                            // 变为 Container<int> 而非 Container<GenericParam("T")>。
                            if let Some(class_info) = registry.get_class(obj_name_str) {
                                if !class_info.type_params.is_empty() {
                                    if let Some(inferred) = self
                                        .infer_type_args_from_call_args_codegen(
                                            &method_info.params,
                                            &call.args,
                                            &class_info.type_params,
                                        )
                                    {
                                        let mapping: std::collections::HashMap<String, Type> =
                                            class_info
                                                .type_params
                                                .iter()
                                                .zip(inferred.iter())
                                                .map(|(p, t)| (p.name.clone(), t.clone()))
                                                .collect();
                                        return Some(crate::types::substitute_type_params(
                                            &method_info.return_type,
                                            &mapping,
                                        ));
                                    }
                                }
                            }
                            return Some(method_info.return_type.clone());
                        }
                    }
                    // 否则尝试从变量映射获取
                    // 首先尝试从 var_cay_types 获取完整类型信息（支持泛型）
                    let (class_name, type_args) = if let Some(cay_type) =
                        self.var_cay_types.get(obj_name_str)
                    {
                        match cay_type {
                            crate::types::Type::Object(name) => (name.clone(), Vec::new()),
                            crate::types::Type::Generic(name, args) => (name.clone(), args.clone()),
                            _ => {
                                // 回退到 var_class_map
                                if let Some(name) = self.var_class_map.get(obj_name_str) {
                                    (name.clone(), Vec::new())
                                } else {
                                    return None;
                                }
                            }
                        }
                    } else if let Some(name) = self.var_class_map.get(obj_name_str) {
                        (name.clone(), Vec::new())
                    } else {
                        return None;
                    };

                    let found = if let Some(types) = arg_types_slice {
                        registry.find_method(&class_name, &member.member, types)
                    } else {
                        None
                    }
                    .or_else(|| registry.get_method(&class_name, &member.member));
                    if let Some(method_info) = found {
                        // 如果返回类型是泛型参数，需要根据调用上下文替换为实际类型
                        return self.resolve_generic_return_type(
                            &method_info.return_type,
                            registry,
                            &class_name,
                            &type_args,
                        );
                    }
                    // 检查是否是 String 类型变量
                    if let Some(var_cay_type) = self.var_cay_types.get(obj_name_str) {
                        if let crate::types::Type::String = var_cay_type {
                            // String 类型特殊处理
                            if member.member == "length"
                                || member.member == "indexOf"
                                || member.member == "lastIndexOf"
                                || member.member == "compareTo"
                            {
                                return Some(crate::types::Type::Int32);
                            } else if member.member == "substring"
                                || member.member == "toString"
                                || member.member == "replace"
                                || member.member == "toLowerCase"
                                || member.member == "toUpperCase"
                            {
                                return Some(crate::types::Type::String);
                            } else if member.member == "equals"
                                || member.member == "isEmpty"
                                || member.member == "startsWith"
                                || member.member == "endsWith"
                                || member.member == "contains"
                                || member.member == "equalsIgnoreCase"
                            {
                                return Some(crate::types::Type::Bool);
                            } else if member.member == "charAt" {
                                return Some(crate::types::Type::Char);
                            }
                        }
                    }
                } else {
                    // 处理链式调用：obj 不是 Identifier，递归推断其类型
                    if let Some(obj_type) = self.get_expression_type(&member.object) {
                        let (class_name, type_args) = match &obj_type {
                            crate::types::Type::Object(name) => (name.clone(), Vec::new()),
                            crate::types::Type::Generic(name, args) => (name.clone(), args.clone()),
                            crate::types::Type::String => {
                                // String 类型特殊处理
                                if member.member == "length"
                                    || member.member == "indexOf"
                                    || member.member == "lastIndexOf"
                                    || member.member == "compareTo"
                                {
                                    return Some(crate::types::Type::Int32);
                                } else if member.member == "substring"
                                    || member.member == "toString"
                                    || member.member == "replace"
                                    || member.member == "toLowerCase"
                                    || member.member == "toUpperCase"
                                {
                                    return Some(crate::types::Type::String);
                                } else if member.member == "equals"
                                    || member.member == "isEmpty"
                                    || member.member == "startsWith"
                                    || member.member == "endsWith"
                                    || member.member == "contains"
                                    || member.member == "equalsIgnoreCase"
                                {
                                    return Some(crate::types::Type::Bool);
                                } else if member.member == "charAt" {
                                    return Some(crate::types::Type::Char);
                                }
                                return None;
                            }
                            _ => return None,
                        };

                        let found = if let Some(types) = arg_types_slice {
                            registry.find_method(&class_name, &member.member, types)
                        } else {
                            None
                        }
                        .or_else(|| registry.get_method(&class_name, &member.member));

                        if let Some(method_info) = found {
                            // 如果返回类型是泛型参数，需要根据调用上下文替换为实际类型
                            return self.resolve_generic_return_type(
                                &method_info.return_type,
                                registry,
                                &class_name,
                                &type_args,
                            );
                        }
                    }
                }
            }
        }

        // 无法推断
        None
    }

    /// 从调用实参推断泛型类型实参（codegen 阶段辅助函数）。
    /// 用于静态方法调用没有显式类型实参的场景，例如 Container.make(42) 推断出 T = int。
    fn infer_type_args_from_call_args_codegen(
        &self,
        method_params: &[crate::types::ParameterInfo],
        call_args: &[Expr],
        type_params: &[crate::types::TypeParamInfo],
    ) -> Option<Vec<Type>> {
        let mut inferred: Vec<Option<Type>> = vec![None; type_params.len()];

        let positional_args: Vec<&Expr> = call_args
            .iter()
            .filter(|a| !matches!(a, Expr::NamedArg(_)))
            .collect();

        for (param, arg) in method_params.iter().zip(positional_args.iter()) {
            let arg_type = self.get_expression_type(arg)?;
            self.infer_generic_substitution_codegen(
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

    /// 递归比较形参类型与实参类型，收集泛型参数映射（codegen 阶段）。
    fn infer_generic_substitution_codegen(
        &self,
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
                    self.infer_generic_substitution_codegen(p, a, type_params, inferred)?;
                }
            }
            (Type::Array(p_inner), Type::Array(a_inner)) => {
                self.infer_generic_substitution_codegen(p_inner, a_inner, type_params, inferred)?;
            }
            (Type::Pointer(p_inner), Type::Pointer(a_inner)) => {
                self.infer_generic_substitution_codegen(p_inner, a_inner, type_params, inferred)?;
            }
            (Type::Function(p_ft), Type::Function(a_ft))
                if p_ft.params.len() == a_ft.params.len() =>
            {
                self.infer_generic_substitution_codegen(
                    &p_ft.return_type,
                    &a_ft.return_type,
                    type_params,
                    inferred,
                )?;
                for (p, a) in p_ft.params.iter().zip(a_ft.params.iter()) {
                    self.infer_generic_substitution_codegen(p, a, type_params, inferred)?;
                }
            }
            _ => {}
        }
        Some(())
    }

    /// 解析泛型返回类型
    /// 如果返回类型包含泛型参数，则根据调用类的类型实参递归替换为实际类型。
    fn resolve_generic_return_type(
        &self,
        return_type: &Type,
        registry: &crate::types::TypeRegistry,
        class_name: &str,
        type_args: &[Type],
    ) -> Option<Type> {
        let base_class_name = if let Some(pos) = class_name.find('<') {
            &class_name[..pos]
        } else {
            class_name
        };

        // 获取定义该方法的类或接口的类型参数名
        let type_params = registry
            .get_class(base_class_name)
            .map(|c| c.type_params.clone())
            .or_else(|| {
                registry
                    .get_interface(base_class_name)
                    .map(|i| i.type_params.clone())
            })
            .unwrap_or_default();

        if type_params.is_empty() || type_args.is_empty() {
            return Some(return_type.clone());
        }

        let mapping: std::collections::HashMap<String, Type> = type_params
            .iter()
            .zip(type_args.iter())
            .map(|(param, arg)| (param.name.clone(), arg.clone()))
            .collect();

        Some(crate::types::substitute_type_params(return_type, &mapping))
    }

    /// 推断方法的返回类型
    fn infer_method_return_type(&self, class_name: &str, method_name: &str) -> Option<Type> {
        if let Some(ref registry) = self.type_registry {
            if let Some(method_info) = registry.get_method(class_name, method_name) {
                return Some(method_info.return_type.clone());
            }
        }
        None
    }

    /// 将 LLVM 类型转换为 Cayvy 类型
    pub(crate) fn llvm_type_to_cay_type(&self, llvm_type: &str) -> Option<Type> {
        match llvm_type {
            "i32" => Some(Type::Int32),
            "i64" => Some(Type::Int64),
            "float" => Some(Type::Float32),
            "double" => Some(Type::Float64),
            "i1" => Some(Type::Bool),
            "i8" => Some(Type::Char),
            "i8*" => Some(Type::String),
            "void" => Some(Type::Void),
            _ => {
                // 检查是否是对象指针类型
                if llvm_type.starts_with("%") && llvm_type.ends_with("*") {
                    let class_name = llvm_type.trim_start_matches('%').trim_end_matches('*');
                    Some(Type::Object(class_name.to_string()))
                } else {
                    None
                }
            }
        }
    }

    /// 生成变量声明代码
    pub fn generate_var_decl(&mut self, var: &VarDecl) -> CayResult<()> {
        // 处理 auto 类型推断
        let actual_type = if var.var_type == Type::Auto {
            // 从初始化器推断类型
            if let Some(init) = &var.initializer {
                self.infer_type_from_expr(init).unwrap_or(Type::Int32)
            } else {
                return Err(semantic_error_with_file(
                    ErrorCodes::SEMANTIC_INVALID_OPERATION,
                    var.loc.file.clone(),
                    var.loc.line,
                    var.loc.column,
                    "'auto' variable declaration requires an initializer".to_string(),
                ));
            }
        } else {
            // 单态化上下文下，将泛型参数替换为实际类型
            self.resolve_type_arg_concrete(&var.var_type)
        };

        let var_type = self.type_to_llvm(&actual_type);
        let align = self.get_type_align(&var_type); // 获取对齐

        // 使用作用域管理器生成唯一的 LLVM 变量名
        let llvm_name = self.scope_manager.declare_var(&var.name, &var_type);

        self.emit_line(&format!(
            "  %{} = alloca {}, align {}",
            llvm_name, var_type, align
        ));
        // 同时存储到旧系统以保持兼容性
        self.var_types.insert(var.name.clone(), var_type.clone());
        // 存储Cavvy类型信息，用于准确的类型推断
        self.var_cay_types
            .insert(var.name.clone(), actual_type.clone());
        // 如果变量类型是对象，记录其类名以便后续方法调用解析
        match &actual_type {
            Type::Object(class_name) => {
                self.var_class_map
                    .insert(var.name.clone(), class_name.clone());
            }
            Type::Generic(class_name, type_args) => {
                // 编译期单态化：将类型参数经 generic_type_args 替换为具体类型，
                // 若全部具体则记录特化类名（如 "Optional<double>"），使字段访问
                // 解析到特化布局而非基础布局；否则退回基础类名。
                let resolved: Vec<Type> = type_args
                    .iter()
                    .map(|t| self.resolve_type_arg_concrete(t))
                    .collect();
                let all_concrete =
                    !resolved.is_empty() && resolved.iter().all(|t| self.type_arg_is_concrete(t));
                if all_concrete {
                    let args_str: Vec<String> = resolved.iter().map(|t| format!("{}", t)).collect();
                    self.var_class_map.insert(
                        var.name.clone(),
                        format!("{}<{}>", class_name, args_str.join(", ")),
                    );
                } else {
                    self.var_class_map
                        .insert(var.name.clone(), class_name.clone());
                }
            }
            _ => {}
        }

        // ROADMAP 5.3.x 自动 RAII：若局部变量类型是带析构函数的类，
        // 登记为析构候选，作用域退出时由 generate_block 自动调用 __dtor。
        // （参数与 this 不走此路径——它们由 declare_var_with_flag 登记，is_parameter=true）
        self.register_dtor_candidate_if_applicable(&var.name, &llvm_name, &actual_type);

        if let Some(init) = var.initializer.as_ref() {
            // 特殊处理数组初始化，传递目标类型信息
            if let Expr::ArrayInit(array_init) = init {
                let value = self.generate_array_init_with_type(array_init, &actual_type)?;
                self.emit_line(&format!("  store {}, {}* %{}", value, var_type, llvm_name));
            } else if let Expr::Lambda(lambda_expr) = init {
                // Lambda 表达式特殊处理：传递变量名以支持闭包捕获
                let value = self.generate_lambda(lambda_expr, Some(&var.name))?;
                let (value_type, val) = self.parse_typed_value(&value);
                if value_type != var_type {
                    let temp = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = bitcast {} {} to {}",
                        temp, value_type, val, var_type
                    ));
                    self.emit_line(&format!(
                        "  store {} {}, {}* %{}, align {}",
                        var_type, temp, var_type, llvm_name, align
                    ));
                } else {
                    self.emit_line(&format!("  store {}, {}* %{}", value, var_type, llvm_name));
                }
            } else {
                // 对于泛型类型变量的初始化，传递期望目标类型以便单态化：
                // - `Box<int> b = new Box(42)`：将 new 解析到 Box<int> 特化版本；
                // - `Optional<int> o = Optional.of(42)`：将静态工厂调用解析到
                //   Optional<int> 单态化版本（而非类型擦除的基础模板）。
                let is_generic_init = matches!(actual_type, Type::Generic(_, _))
                    && matches!(init, Expr::New(_) | Expr::Call(_));
                if is_generic_init {
                    self.pending_new_expected_type = Some(actual_type.clone());
                }
                let value = self.generate_expression(init)?;
                self.pending_new_expected_type = None;
                let (value_type, val) = self.parse_typed_value(&value);

                // 如果值类型与变量类型不匹配，需要转换
                if value_type != var_type {
                    let temp = self.new_temp();

                    // null 赋值给指针类型（int 0 转换为指针）- 必须最先检查
                    if (val == "0" || val == "null") && var_type.ends_with("*") {
                        // null 可以直接存储到指针类型
                        self.emit_line(&format!(
                            "  store {} null, {}* %{}, align {}",
                            var_type, var_type, llvm_name, align
                        ));
                    }
                    // 浮点类型转换
                    else if value_type == "double" && var_type == "float" {
                        // double -> float 转换
                        self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                        let align = self.get_type_align("float");
                        self.emit_line(&format!(
                            "  store float {}, float* %{}, align {}",
                            temp, llvm_name, align
                        ));
                    } else if value_type == "float" && var_type == "double" {
                        // float -> double 转换
                        self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                        let align = self.get_type_align("double");
                        self.emit_line(&format!(
                            "  store double {}, double* %{}, align {}",
                            temp, llvm_name, align
                        ));
                    }
                    // 指针类型转换 (bitcast)
                    else if value_type.ends_with("*") && var_type.ends_with("*") {
                        self.emit_line(&format!(
                            "  {} = bitcast {} {} to {}",
                            temp, value_type, val, var_type
                        ));
                        self.emit_line(&format!(
                            "  store {} {}, {}* %{}, align {}",
                            var_type, temp, var_type, llvm_name, align
                        ));
                    }
                    // 整数类型转换
                    else if value_type.starts_with("i")
                        && var_type.starts_with("i")
                        && !value_type.ends_with("*")
                        && !var_type.ends_with("*")
                    {
                        let from_bits: u32 =
                            value_type.trim_start_matches('i').parse().unwrap_or(64);
                        let to_bits: u32 = var_type.trim_start_matches('i').parse().unwrap_or(64);

                        if to_bits > from_bits {
                            // 符号扩展
                            self.emit_line(&format!(
                                "  {} = sext {} {} to {}",
                                temp, value_type, val, var_type
                            ));
                        } else {
                            // 截断
                            self.emit_line(&format!(
                                "  {} = trunc {} {} to {}",
                                temp, value_type, val, var_type
                            ));
                        }
                        self.emit_line(&format!(
                            "  store {} {}, {}* %{}, align {}",
                            var_type, temp, var_type, llvm_name, align
                        ));
                    }
                    // 整数到浮点数转换
                    else if value_type.starts_with("i")
                        && !value_type.ends_with("*")
                        && (var_type == "float" || var_type == "double")
                    {
                        self.emit_line(&format!(
                            "  {} = sitofp {} {} to {}",
                            temp, value_type, val, var_type
                        ));
                        self.emit_line(&format!(
                            "  store {} {}, {}* %{}, align {}",
                            var_type, temp, var_type, llvm_name, align
                        ));
                    }
                    // 浮点数到整数转换
                    else if (value_type == "float" || value_type == "double")
                        && var_type.starts_with("i")
                    {
                        self.emit_line(&format!(
                            "  {} = fptosi {} {} to {}",
                            temp, value_type, val, var_type
                        ));
                        self.emit_line(&format!(
                            "  store {} {}, {}* %{}, align {}",
                            var_type, temp, var_type, llvm_name, align
                        ));
                    }
                    // 指针到整数转换 (ptrtoint)
                    else if value_type.ends_with("*")
                        && var_type.starts_with("i")
                        && !var_type.ends_with("*")
                    {
                        self.emit_line(&format!(
                            "  {} = ptrtoint {} {} to {}",
                            temp, value_type, val, var_type
                        ));
                        self.emit_line(&format!(
                            "  store {} {}, {}* %{}, align {}",
                            var_type, temp, var_type, llvm_name, align
                        ));
                    }
                    // 整数到指针转换 (inttoptr)
                    else if var_type.ends_with("*")
                        && value_type.starts_with("i")
                        && !value_type.ends_with("*")
                    {
                        // LLVM 不支持 void*，使用 i8* 代替
                        let llvm_var_type: &str = if var_type == "void*" {
                            "i8*"
                        } else {
                            &var_type
                        };
                        self.emit_line(&format!(
                            "  {} = inttoptr {} {} to {}",
                            temp, value_type, val, llvm_var_type
                        ));
                        self.emit_line(&format!(
                            "  store {} {}, {}* %{}, align {}",
                            var_type, temp, var_type, llvm_name, align
                        ));
                    }
                    // i8* 解箱转换（用于泛型类型返回值）
                    // 对于泛型类如 Box<T>.get() 返回 i8*，需要解箱为具体值类型
                    else if value_type == "i8*" {
                        // i8* -> i1 (bool)：指针转整数，截断到 i1
                        if var_type == "i1" {
                            let int_val = self.new_temp();
                            self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                            let trunc_i8 = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = trunc i64 {} to i8",
                                trunc_i8, int_val
                            ));
                            self.emit_line(&format!("  {} = trunc i8 {} to i1", temp, trunc_i8));
                            self.emit_line(&format!(
                                "  store i1 {}, i1* %{}, align {}",
                                temp, llvm_name, align
                            ));
                        }
                        // i8* -> i8 (char)：指针转整数，截断到 i8
                        else if var_type == "i8" {
                            let int_val = self.new_temp();
                            self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                            self.emit_line(&format!("  {} = trunc i64 {} to i8", temp, int_val));
                            self.emit_line(&format!(
                                "  store i8 {}, i8* %{}, align {}",
                                temp, llvm_name, align
                            ));
                        }
                        // i8* -> i16：指针转整数，截断到 i16
                        else if var_type == "i16" {
                            let int_val = self.new_temp();
                            self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                            self.emit_line(&format!("  {} = trunc i64 {} to i16", temp, int_val));
                            self.emit_line(&format!(
                                "  store i16 {}, i16* %{}, align {}",
                                temp, llvm_name, align
                            ));
                        }
                        // i8* -> i32：指针转整数，截断到 i32
                        else if var_type == "i32" {
                            let int_val = self.new_temp();
                            self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                            self.emit_line(&format!("  {} = trunc i64 {} to i32", temp, int_val));
                            self.emit_line(&format!(
                                "  store i32 {}, i32* %{}, align {}",
                                temp, llvm_name, align
                            ));
                        }
                        // i8* -> i64：指针转整数
                        else if var_type == "i64" {
                            self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", temp, val));
                            self.emit_line(&format!(
                                "  store i64 {}, i64* %{}, align {}",
                                temp, llvm_name, align
                            ));
                        }
                        // i8* -> float：指针转整数，bitcast 到 float
                        else if var_type == "float" {
                            let int_val = self.new_temp();
                            self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                            let double_val = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = bitcast i64 {} to double",
                                double_val, int_val
                            ));
                            self.emit_line(&format!(
                                "  {} = fptrunc double {} to float",
                                temp, double_val
                            ));
                            self.emit_line(&format!(
                                "  store float {}, float* %{}, align {}",
                                temp, llvm_name, align
                            ));
                        }
                        // i8* -> double：指针转整数，bitcast 到 double
                        else if var_type == "double" {
                            let int_val = self.new_temp();
                            self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                            self.emit_line(&format!(
                                "  {} = bitcast i64 {} to double",
                                temp, int_val
                            ));
                            self.emit_line(&format!(
                                "  store double {}, double* %{}, align {}",
                                temp, llvm_name, align
                            ));
                        }
                        // i8* -> 其他指针类型：bitcast
                        else if var_type.ends_with("*") {
                            self.emit_line(&format!(
                                "  {} = bitcast i8* {} to {}",
                                temp, val, var_type
                            ));
                            self.emit_line(&format!(
                                "  store {} {}, {}* %{}, align {}",
                                var_type, temp, var_type, llvm_name, align
                            ));
                        } else {
                            // 类型不兼容，报错
                            return Err(crate::miette_diagnostic::codegen_error_at(
                                ErrorCodes::CODEGEN_INVALID_OPERATION,
                                var.loc.clone(),
                                format!(
                                    "Cannot unbox i8* to {} in variable initialization '{}'",
                                    var_type, var.name
                                ),
                            ));
                        }
                    } else {
                        // 类型不兼容，报错
                        return Err(crate::miette_diagnostic::codegen_error_at(
                            ErrorCodes::CODEGEN_INVALID_OPERATION,
                            var.loc.clone(),
                            format!(
                                "Cannot convert {} to {} in variable initialization '{}'",
                                value_type, var_type, var.name
                            ),
                        ));
                    }
                } else {
                    // 类型匹配，直接存储
                    // 检查是否是 struct 类型赋值（需要深拷贝而非指针复制）
                    if let Some(struct_name) = self.extract_struct_name_from_ptr_type(&var_type) {
                        // struct 深拷贝：分配新栈空间并通过 llvm.memcpy 复制内容
                        let src_ptr = val;
                        let new_struct = self.new_temp();
                        let llvm_struct_type = format!("%struct.{}", struct_name);
                        self.emit_line(&format!("  {} = alloca {}", new_struct, llvm_struct_type));
                        self.emit_struct_memcpy(&new_struct, &src_ptr, &struct_name);
                        self.emit_line(&format!(
                            "  store {}* {}, {}** %{}, align {}",
                            llvm_struct_type, new_struct, llvm_struct_type, llvm_name, align
                        ));
                    } else {
                        self.emit_line(&format!("  store {}, {}* %{}", value, var_type, llvm_name));
                    }
                }
            }
        }

        Ok(())
    }
}
