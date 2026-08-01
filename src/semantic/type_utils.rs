//! 类型工具函数

use super::analyzer::SemanticAnalyzer;
use crate::ast::Expr;
use crate::miette_diagnostic::CayResult;
use crate::types::{ParameterInfo, Type};

/// 命名参数解析结果
pub struct ResolvedArgs {
    /// 按形参顺序重新排列后的实参引用列表
    pub args: Vec<Expr>,
    /// 可变参数在 params 中的索引（如果有的话）
    pub varargs_index: Option<usize>,
}

/// 将混合位置参数和命名参数（name=value）的实参列表，按形参顺序重排。
/// 位置参数从左到右填满固定形参，可变参数吃掉所有剩余位置参数，
/// 命名参数按其名称匹配到对应形参。
pub fn resolve_call_args(args: &[Expr], params: &[ParameterInfo]) -> Result<ResolvedArgs, String> {
    use std::collections::HashMap;

    // 分离命名参数和位置参数
    let mut named: HashMap<String, &Expr> = HashMap::new();
    let mut positional: Vec<&Expr> = Vec::new();
    let mut has_named = false;

    for arg in args {
        if let Expr::NamedArg(n) = arg {
            if named.contains_key(&n.name) {
                return Err(format!("Duplicate named argument '{}'", n.name));
            }
            named.insert(n.name.clone(), &n.value);
            has_named = true;
        } else {
            positional.push(arg);
        }
    }

    // 没有命名参数时直接返回原参数
    if !has_named {
        let varargs_idx = params.iter().position(|p| p.is_varargs);
        return Ok(ResolvedArgs {
            args: args.to_vec(),
            varargs_index: varargs_idx,
        });
    }

    // 验证命名参数名称合法性
    for name in named.keys() {
        if !params.iter().any(|p| &p.name == name) {
            return Err(format!("Unknown named argument '{}'", name));
        }
    }

    // 找到可变参数的位置
    let varargs_idx = params.iter().position(|p| p.is_varargs);

    // 构建按形参顺序的结果
    let mut result: Vec<Expr> = Vec::new();
    let mut pos_idx = 0;
    let fixed_count = match varargs_idx {
        Some(vi) => vi,
        None => params.len(),
    };

    // 第一步：填充可变参数之前的固定参数
    for i in 0..fixed_count {
        if let Some(val) = named.get(&params[i].name) {
            result.push((*val).clone());
        } else if pos_idx < positional.len() {
            result.push(positional[pos_idx].clone());
            pos_idx += 1;
        } else {
            return Err(format!(
                "Missing argument for parameter '{}'",
                params[i].name
            ));
        }
    }

    // 第二步：可变参数（如果在中间，可变参数之后的由命名参数填充）
    if let Some(vi) = varargs_idx {
        // 检查可变参数是否被命名参数覆盖
        if let Some(val) = named.get(&params[vi].name) {
            // 命名参数直接指定可变参数的值（如传递整个数组）
            result.push((*val).clone());
        } else {
            // 剩余位置参数全部归可变参数
            while pos_idx < positional.len() {
                result.push(positional[pos_idx].clone());
                pos_idx += 1;
            }
        }

        // 第三步：可变参数之后的固定参数（只能通过命名参数填充）
        for i in (vi + 1)..params.len() {
            if let Some(val) = named.get(&params[i].name) {
                result.push((*val).clone());
            } else {
                return Err(format!(
                    "Missing argument for parameter '{}' (after varargs, must use named argument)",
                    params[i].name
                ));
            }
        }
    }

    // 检查未使用的位置参数
    if pos_idx < positional.len() {
        return Err(format!(
            "Too many positional arguments ({} extra)",
            positional.len() - pos_idx
        ));
    }

    // 检查未匹配的命名参数
    for name in named.keys() {
        if !params.iter().any(|p| &p.name == name) {
            return Err(format!("Unknown named argument '{}'", name));
        }
    }

    Ok(ResolvedArgs {
        args: result,
        varargs_index: varargs_idx,
    })
}

impl SemanticAnalyzer {
    /// 检查类型兼容性（薄入口，非独立规则表）
    ///
    /// 验证源类型是否可以赋值给目标类型。
    /// 对于引用类型（Object），检查继承关系：子类可以赋值给父类。
    ///
    /// # 参数顺序
    /// `types_compatible(from, to)`：源类型在前、目标类型在后，
    /// 语义为「from 是否可以赋值/隐式转换为 to」。
    ///
    /// 规则本体统一在 `TypeRegistry::types_compatible`，此处仅作转发。
    pub fn types_compatible(&self, from: &Type, to: &Type) -> bool {
        self.type_registry.types_compatible(from, to)
    }

    /// 类型提升规则
    pub fn promote_types(&self, left: &Type, right: &Type) -> Type {
        match (left, right) {
            (Type::Float64, _) | (_, Type::Float64) => Type::Float64,
            (Type::Float32, _) | (_, Type::Float32) => Type::Float32,
            (Type::Int64, _) | (_, Type::Int64) => Type::Int64,
            // char 类型在算术运算中提升为 int32
            (Type::Char, Type::Char) => Type::Int32,
            (Type::Char, Type::Int32) | (Type::Int32, Type::Char) => Type::Int32,
            (Type::Int32, Type::Int32) => Type::Int32,
            _ => left.clone(),
        }
    }

    /// 检查类型是否为数值类型
    /// 检查类型是否为数值类型
    /// 时间复杂度: O(1)
    pub fn is_numeric_type(ty: &Type) -> bool {
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

    /// 查找方法（考虑命名空间前缀）
    ///
    /// 这个方法与 ClassInfo::find_method 类似，但在比较参数类型时会考虑命名空间前缀。
    /// 例如，Object("JsonValue") 和 Object("json::JsonValue") 被认为是兼容的。
    ///
    /// # Arguments
    /// * `class_info` - 类信息
    /// * `method_name` - 方法名
    /// * `arg_types` - 实参类型列表
    ///
    /// # Returns
    /// 如果找到匹配的方法，返回方法信息
    pub fn find_method_with_namespace<'a>(
        &self,
        class_info: &'a crate::types::ClassInfo,
        method_name: &str,
        arg_types: &[Type],
    ) -> Option<&'a crate::types::MethodInfo> {
        use crate::types::ParameterInfo;

        let methods = class_info.methods.get(method_name)?;

        // 第一遍：寻找精确匹配
        for m in methods.iter() {
            if self.match_method_params_exact_with_namespace(&m.params, arg_types) {
                return Some(m);
            }
        }

        // 第二遍：寻找兼容匹配（允许隐式转换）
        methods
            .iter()
            .find(|m| self.match_method_params_with_namespace(&m.params, arg_types))
    }

    /// 精确匹配方法参数（考虑命名空间前缀）
    fn match_method_params_exact_with_namespace(
        &self,
        params: &[crate::types::ParameterInfo],
        arg_types: &[Type],
    ) -> bool {
        if params.len() != arg_types.len() {
            return false;
        }
        params
            .iter()
            .zip(arg_types.iter())
            .all(|(p, a)| self.types_compatible_with_namespace(&p.param_type, a))
    }

    /// 兼容匹配方法参数（考虑命名空间前缀）
    fn match_method_params_with_namespace(
        &self,
        params: &[crate::types::ParameterInfo],
        arg_types: &[Type],
    ) -> bool {
        if params.len() != arg_types.len() {
            return false;
        }
        params.iter().zip(arg_types.iter()).all(|(p, a)| {
            // 首先尝试使用 types_compatible_with_namespace
            if self.types_compatible_with_namespace(&p.param_type, a) {
                return true;
            }
            // 然后尝试使用基本的类型兼容性检查
            self.types_compatible(a, &p.param_type)
        })
    }

    /// 检查两个类型是否兼容（考虑命名空间前缀）
    /// 这是 TypeRegistry::types_compatible_with_namespace 的包装
    ///
    /// # 参数顺序
    /// `types_compatible_with_namespace(param_type, arg_type)`：形参（目标）类型在前、
    /// 实参（来源）类型在后，与 `types_compatible(from, to)` 顺序相反，注意勿传反。
    fn types_compatible_with_namespace(&self, param_type: &Type, arg_type: &Type) -> bool {
        self.type_registry
            .types_compatible_with_namespace(param_type, arg_type)
    }

    /// 整数类型提升
    pub fn promote_integer_types(&self, left: &Type, right: &Type) -> Type {
        match (left, right) {
            (Type::Int64, _) | (_, Type::Int64) => Type::Int64,
            _ => Type::Int32,
        }
    }

    /// 检查参数是否与参数定义精确匹配（参数类型完全相同，不支持隐式转换）
    /// 用于方法重载解析时优先选择精确匹配
    pub fn check_arguments_exact(&mut self, args: &[Expr], params: &[ParameterInfo]) -> bool {
        // 分离命名参数和位置参数
        let mut named: std::collections::HashMap<String, &Expr> = std::collections::HashMap::new();
        let mut positional: Vec<&Expr> = Vec::new();
        let mut has_named = false;

        for arg in args {
            if let Expr::NamedArg(n) = arg {
                named.insert(n.name.clone(), &n.value);
                has_named = true;
            } else {
                positional.push(arg);
            }
        }

        // 构建按形参顺序的实参列表
        let mut ordered_args: Vec<&Expr> = Vec::new();
        let mut pos_idx = 0;

        for param in params {
            if let Some(val) = named.get(&param.name) {
                ordered_args.push(val);
            } else if pos_idx < positional.len() {
                ordered_args.push(positional[pos_idx]);
                pos_idx += 1;
            } else {
                return false; // 缺少参数
            }
        }

        // 检查是否有未使用的位置参数
        if pos_idx < positional.len() {
            return false;
        }

        // 检查参数数量
        if ordered_args.len() != params.len() {
            return false;
        }

        // 检查每个参数类型是否精确匹配
        for (arg, param) in ordered_args.iter().zip(params.iter()) {
            let arg_type = self.infer_expr_type_collect_errors(arg);
            // 精确匹配：类型必须完全相同
            if arg_type != param.param_type {
                return false;
            }
        }

        true
    }

    /// 检查参数是否与参数定义兼容（支持可变参数和命名参数 name=value）
    pub fn check_arguments_compatible(
        &mut self,
        args: &[Expr],
        params: &[ParameterInfo],
        _line: usize,
        _column: usize,
    ) -> Result<(), String> {
        if params.is_empty() {
            if args.is_empty() {
                return Ok(());
            } else {
                return Err(format!("Expected 0 arguments, got {}", args.len()));
            }
        }

        // === 预处理：分离位置参数和命名参数 ===
        let mut named: std::collections::HashMap<String, &Expr> = std::collections::HashMap::new();
        let mut positional: Vec<&Expr> = Vec::new();
        let mut has_explicit_named = false;

        for arg in args {
            if let Expr::NamedArg(n) = arg {
                if named.contains_key(&n.name) {
                    return Err(format!("Duplicate named argument '{}'", n.name));
                }
                named.insert(n.name.clone(), &n.value);
                has_explicit_named = true;
            } else {
                positional.push(arg);
            }
        }

        // 验证命名参数的名称是否合法
        for name in named.keys() {
            if !params.iter().any(|p| &p.name == name) {
                return Err(format!("Unknown named argument '{}'", name));
            }
        }

        let last_idx = params.len() - 1;
        let has_varargs = if !params.is_empty() {
            params.iter().any(|p| p.is_varargs)
        } else {
            false
        };

        // 如果有命名参数，我们需要重新排列参数以匹配形参顺序
        if has_explicit_named {
            let fixed_count = if has_varargs { last_idx } else { params.len() };
            let varargs_elem_type = if has_varargs {
                match &params[last_idx].param_type {
                    Type::Array(elem) => Some(elem.as_ref().clone()),
                    _ => Some(params[last_idx].param_type.clone()),
                }
            } else {
                None
            };

            // 构建每个形参对应的实参
            let mut arg_for_param: Vec<Option<&Expr>> = vec![None; params.len()];
            let mut pos_idx = 0;

            // 第一步：填充固定（非可变）参数
            for i in 0..fixed_count {
                if let Some(val) = named.get(&params[i].name) {
                    // 命名参数显式指定
                    arg_for_param[i] = Some(val);
                } else if pos_idx < positional.len() {
                    // 使用位置参数
                    arg_for_param[i] = Some(positional[pos_idx]);
                    pos_idx += 1;
                }
                // 否则保持 None（后续会报参数不足错误）
            }

            // 第二步：可变参数获取所有剩余位置参数
            if has_varargs {
                // 可变参数也可以被命名参数覆盖
                if let Some(val) = named.get(&params[last_idx].name) {
                    // 命名参数传入整个数组
                    let arg_type = self.infer_expr_type_collect_errors(val);
                    if !self.types_compatible(&arg_type, &params[last_idx].param_type) {
                        return Err(format!(
                            "Named argument '{}' type mismatch: expected {}, got {}",
                            params[last_idx].name, params[last_idx].param_type, arg_type
                        ));
                    }
                    arg_for_param[last_idx] = Some(val);
                } else {
                    // 检查剩余位置参数
                    let remaining_count = positional.len() - pos_idx;
                    if remaining_count == 1 {
                        // 只有一个剩余参数，检查是否是数组类型
                        let arg_type = self.infer_expr_type_collect_errors(positional[pos_idx]);
                        if self.types_compatible(&arg_type, &params[last_idx].param_type) {
                            // 直接传递数组
                            arg_for_param[last_idx] = Some(positional[pos_idx]);
                        } else if let Some(ref elem_type) = varargs_elem_type {
                            // 单个元素
                            if !self.types_compatible(&arg_type, elem_type) {
                                return Err(format!(
                                    "Varargs argument type mismatch: expected {}, got {}",
                                    elem_type, arg_type
                                ));
                            }
                            arg_for_param[last_idx] = Some(positional[pos_idx]);
                        }
                    } else if remaining_count > 1 {
                        // 多个剩余参数，检查每个元素类型
                        if let Some(ref elem_type) = varargs_elem_type {
                            for j in pos_idx..positional.len() {
                                let arg_type = self.infer_expr_type_collect_errors(positional[j]);
                                if !self.types_compatible(&arg_type, elem_type) {
                                    return Err(format!(
                                        "Varargs argument {} type mismatch: expected {}, got {}",
                                        j + 1,
                                        elem_type,
                                        arg_type
                                    ));
                                }
                            }
                        }
                    }
                    // 标记可变参数有值（即使是零个）
                    if pos_idx < positional.len() {
                        arg_for_param[last_idx] = Some(positional[pos_idx]);
                    }
                }
            } else if pos_idx < positional.len() {
                // 非可变参数函数：有未使用的位置参数
                return Err(format!(
                    "Expected {} arguments, got {}",
                    params.len(),
                    positional.len()
                ));
            }

            // 第三步：检查是否有必需的参数未提供
            for i in 0..fixed_count {
                if arg_for_param[i].is_none() {
                    return Err(format!(
                        "Missing argument for parameter '{}'",
                        params[i].name
                    ));
                }
            }

            // 第四步：对所有已匹配的参数进行类型检查
            for (i, param) in params.iter().enumerate() {
                if let Some(arg) = arg_for_param[i] {
                    let arg_type = self.infer_expr_type_collect_errors(arg);
                    if param.is_varargs {
                        // 可变参数的检查已经在上面完成了，这里跳过
                        continue;
                    }
                    if !self.types_compatible(&arg_type, &param.param_type) {
                        return Err(format!(
                            "Argument {} type mismatch: expected {}, got {}",
                            i + 1,
                            param.param_type,
                            arg_type
                        ));
                    }
                }
            }

            return Ok(());
        }

        // === 原有逻辑：没有命名参数时的处理 ===

        // 检查最后一个参数是否是可变参数
        if has_varargs {
            // 可变参数：至少需要 params.len() - 1 个参数
            if args.len() < last_idx {
                return Err(format!(
                    "Expected at least {} arguments, got {}",
                    last_idx,
                    args.len()
                ));
            }

            // 检查固定参数
            for i in 0..last_idx {
                let arg_type = self.infer_expr_type_collect_errors(&args[i]);
                if !self.types_compatible(&arg_type, &params[i].param_type) {
                    return Err(format!(
                        "Argument {} type mismatch: expected {}, got {}",
                        i + 1,
                        params[i].param_type,
                        arg_type
                    ));
                }
            }

            // 检查可变参数
            let vararg_param_type = &params[last_idx].param_type;
            let vararg_element_type = match vararg_param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => vararg_param_type,
            };

            // 如果只有一个参数且类型匹配数组类型，直接接受
            if args.len() == last_idx + 1 {
                let arg_type = self.infer_expr_type_collect_errors(&args[last_idx]);
                if self.types_compatible(&arg_type, vararg_param_type) {
                    return Ok(());
                }
            }

            // 否则，按元素类型检查每个参数
            for i in last_idx..args.len() {
                let arg_type = self.infer_expr_type_collect_errors(&args[i]);
                if !self.types_compatible(&arg_type, vararg_element_type) {
                    return Err(format!(
                        "Varargs argument {} type mismatch: expected {}, got {}",
                        i + 1,
                        vararg_element_type,
                        arg_type
                    ));
                }
            }
        } else {
            // 非可变参数：参数数量必须完全匹配
            if params.len() != args.len() {
                return Err(format!(
                    "Expected {} arguments, got {}",
                    params.len(),
                    args.len()
                ));
            }

            for (i, (arg, param)) in args.iter().zip(params.iter()).enumerate() {
                let arg_type = self.infer_expr_type_collect_errors(arg);
                if !self.types_compatible(&arg_type, &param.param_type) {
                    return Err(format!(
                        "Argument {} type mismatch: expected {}, got {}",
                        i + 1,
                        param.param_type,
                        arg_type
                    ));
                }
            }
        }

        Ok(())
    }

    /// 推断 String 方法调用的返回类型
    pub fn infer_string_method_call(
        &mut self,
        method_name: &str,
        args: &[Expr],
        line: usize,
        column: usize,
    ) -> CayResult<Type> {
        match method_name {
            "length" => {
                if !args.is_empty() {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.length() takes no arguments".to_string(),
                    ));
                }
                Ok(Type::Int32)
            }
            "substring" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.substring() takes 1 or 2 arguments".to_string(),
                    ));
                }
                // 检查参数类型
                for (i, arg) in args.iter().enumerate() {
                    let arg_type = self.infer_expr_type_collect_errors(arg);
                    if !arg_type.is_integer() {
                        return Err(self.report_error(
                            line,
                            column,
                            format!(
                                "Argument {} of substring() must be integer, got {}",
                                i + 1,
                                arg_type
                            ),
                        ));
                    }
                }
                Ok(Type::String)
            }
            "indexOf" => {
                if args.len() < 1 || args.len() > 2 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.indexOf() takes 1 or 2 arguments".to_string(),
                    ));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(
                        line,
                        column,
                        format!(
                            "First argument of indexOf() must be string, got {}",
                            arg_type
                        ),
                    ));
                }
                if args.len() == 2 {
                    let start_type = self.infer_expr_type_collect_errors(&args[1]);
                    if !start_type.is_integer() {
                        return Err(self.report_error(
                            line,
                            column,
                            format!(
                                "Second argument of indexOf() must be integer, got {}",
                                start_type
                            ),
                        ));
                    }
                }
                Ok(Type::Int32)
            }
            "lastIndexOf" => {
                if args.len() != 1 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.lastIndexOf() takes 1 argument".to_string(),
                    ));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(
                        line,
                        column,
                        format!("Argument of lastIndexOf() must be string, got {}", arg_type),
                    ));
                }
                Ok(Type::Int32)
            }
            "charAt" => {
                if args.len() != 1 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.charAt() takes 1 argument".to_string(),
                    ));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if !arg_type.is_integer() {
                    return Err(self.report_error(
                        line,
                        column,
                        format!("Argument of charAt() must be integer, got {}", arg_type),
                    ));
                }
                Ok(Type::Char)
            }
            "replace" => {
                if args.len() != 2 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.replace() takes 2 arguments".to_string(),
                    ));
                }
                for (i, arg) in args.iter().enumerate() {
                    let arg_type = self.infer_expr_type_collect_errors(arg);
                    if arg_type != Type::String {
                        return Err(self.report_error(
                            line,
                            column,
                            format!(
                                "Argument {} of replace() must be string, got {}",
                                i + 1,
                                arg_type
                            ),
                        ));
                    }
                }
                Ok(Type::String)
            }
            "isEmpty" => {
                if !args.is_empty() {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.isEmpty() takes no arguments".to_string(),
                    ));
                }
                Ok(Type::Bool)
            }
            "equals" => {
                if args.len() != 1 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.equals() takes 1 argument".to_string(),
                    ));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(
                        line,
                        column,
                        format!("Argument of equals() must be string, got {}", arg_type),
                    ));
                }
                Ok(Type::Bool)
            }
            "equalsIgnoreCase" => {
                if args.len() != 1 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.equalsIgnoreCase() takes 1 argument".to_string(),
                    ));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(
                        line,
                        column,
                        format!(
                            "Argument of equalsIgnoreCase() must be string, got {}",
                            arg_type
                        ),
                    ));
                }
                Ok(Type::Bool)
            }
            "c_str" => {
                if !args.is_empty() {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.c_str() takes no arguments".to_string(),
                    ));
                }
                Ok(Type::Pointer(Box::new(Type::CChar))) // 返回 c_char* 指针类型，与 codegen 中的 i8* 一致
            }
            "startsWith" => {
                if args.len() != 1 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.startsWith() takes 1 argument".to_string(),
                    ));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(
                        line,
                        column,
                        format!("Argument of startsWith() must be string, got {}", arg_type),
                    ));
                }
                Ok(Type::Bool)
            }
            "endsWith" => {
                if args.len() != 1 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.endsWith() takes 1 argument".to_string(),
                    ));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(
                        line,
                        column,
                        format!("Argument of endsWith() must be string, got {}", arg_type),
                    ));
                }
                Ok(Type::Bool)
            }
            "trim" => {
                if !args.is_empty() {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.trim() takes no arguments".to_string(),
                    ));
                }
                Ok(Type::String)
            }
            "toLowerCase" => {
                if !args.is_empty() {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.toLowerCase() takes no arguments".to_string(),
                    ));
                }
                Ok(Type::String)
            }
            "toUpperCase" => {
                if !args.is_empty() {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.toUpperCase() takes no arguments".to_string(),
                    ));
                }
                Ok(Type::String)
            }
            "contains" => {
                if args.len() != 1 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.contains() takes 1 argument".to_string(),
                    ));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(
                        line,
                        column,
                        format!("Argument of contains() must be string, got {}", arg_type),
                    ));
                }
                Ok(Type::Bool)
            }
            "compareTo" => {
                if args.len() != 1 {
                    return Err(self.report_error(
                        line,
                        column,
                        "String.compareTo() takes 1 argument".to_string(),
                    ));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(
                        line,
                        column,
                        format!("Argument of compareTo() must be string, got {}", arg_type),
                    ));
                }
                Ok(Type::Int32)
            }
            _ => Err(self.report_error(
                line,
                column,
                format!("Unknown String method '{}'", method_name),
            )),
        }
    }
}
