//! 类型工具函数

use crate::ast::Expr;
use crate::types::{Type, ParameterInfo};
use crate::error::cayResult;
use super::analyzer::SemanticAnalyzer;

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
            return Err(format!("Missing argument for parameter '{}'", params[i].name));
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
                return Err(format!("Missing argument for parameter '{}' (after varargs, must use named argument)", params[i].name));
            }
        }
    }

    // 检查未使用的位置参数
    if pos_idx < positional.len() {
        return Err(format!("Too many positional arguments ({} extra)", positional.len() - pos_idx));
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
    /// 检查类型兼容性
    ///
    /// 验证源类型是否可以赋值给目标类型。
    /// 对于引用类型（Object），检查继承关系：子类可以赋值给父类。
    pub fn types_compatible(&self, from: &Type, to: &Type) -> bool {
        if from == to {
            return true;
        }

        // 泛型参数类型可以匹配任何类型
        if matches!(to, Type::GenericParam(_)) {
            return true;
        }

        // null 可以赋值给任何引用类型（包括 string 和指针）
        if let Type::Object(obj_name) = from {
            if obj_name == "Object" {
                // null 是 Object 类型，可以赋值给 String 或其他引用类型
                return true;
            }
        }

        // null (Object 类型) 可以赋值给任何指针类型
        if let Type::Object(obj_name) = from {
            if obj_name == "Object" && matches!(to, Type::Pointer(_)) {
                return true;
            }
        }

        // 基本类型之间的兼容
        match (from, to) {
            (Type::Int32, Type::Int64) => true,
            (Type::Int32, Type::Float32) => true,
            (Type::Int32, Type::Float64) => true,
            (Type::Int64, Type::Float64) => true,
            (Type::Float32, Type::Float64) => true,
            (Type::Float64, Type::Float32) => true, // 允许double到float转换（可能有精度损失）
            // 泛型类型兼容性：Type::Generic 和 Type::Object 之间的兼容
            (Type::Generic(from_name, _), Type::Object(to_name)) |
            (Type::Object(to_name), Type::Generic(from_name, _)) => {
                // 解析泛型类名: "Optional<T>" -> "Optional"
                let from_base = if let Some(pos) = from_name.find('<') {
                    &from_name[..pos]
                } else {
                    from_name.as_str()
                };
                let to_base = if let Some(pos) = to_name.find('<') {
                    &to_name[..pos]
                } else {
                    to_name.as_str()
                };
                // 如果基础类名相同，认为是兼容的（泛型类型擦除）
                if from_base == to_base {
                    return true;
                }
                // 否则检查继承关系：from_name 是否是 to_name 的子类
                self.is_subtype_of(from_name, to_name)
            }
            (Type::Generic(from_name, _), Type::Generic(to_name, _)) => {
                // 两个泛型类型：检查基础类名是否相同
                if from_name == to_name {
                    return true;
                }
                self.is_subtype_of(from_name, to_name)
            }
            (Type::Object(from_name), Type::Object(to_name)) => {
                // 解析泛型类名: "Optional<T>" -> "Optional"
                let from_base = if let Some(pos) = from_name.find('<') {
                    &from_name[..pos]
                } else {
                    from_name.as_str()
                };
                let to_base = if let Some(pos) = to_name.find('<') {
                    &to_name[..pos]
                } else {
                    to_name.as_str()
                };
                // 如果基础类名相同，认为是兼容的（泛型类型擦除）
                if from_base == to_base {
                    return true;
                }
                // 检查两个类名是否指向同一个类（处理命名空间前缀）
                // 例如 "JsonValue" 和 "json::JsonValue"
                if self.is_same_class(from_base, to_base) {
                    return true;
                }
                // 否则检查继承关系：from_name 是否是 to_name 的子类
                self.is_subtype_of(from_name, to_name)
            }
            // char 可以赋值给 int (ASCII 码值)
            (Type::Char, Type::Int32) => true,
            (Type::Char, Type::Int64) => true,
            // 数组类型：检查元素类型兼容性
            (Type::Array(from_elem), Type::Array(to_elem)) => {
                self.types_compatible(from_elem, to_elem)
            }
            // FFI 类型与基本类型之间的兼容
            // c_int <-> int
            (Type::CInt, Type::Int32) | (Type::Int32, Type::CInt) => true,
            // c_uint <-> int
            (Type::CUInt, Type::Int32) | (Type::Int32, Type::CUInt) => true,
            // c_long <-> long 和 int
            (Type::CLong, Type::Int64) | (Type::Int64, Type::CLong) => true,
            (Type::CLong, Type::Int32) | (Type::Int32, Type::CLong) => true,
            // c_ulong <-> long/int/c_long
            (Type::CULong, Type::Int64) | (Type::Int64, Type::CULong) => true,
            (Type::CULong, Type::Int32) | (Type::Int32, Type::CULong) => true,
            (Type::CULong, Type::CLong) | (Type::CLong, Type::CULong) => true,
            // c_short <-> int
            (Type::CShort, Type::Int32) | (Type::Int32, Type::CShort) => true,
            // c_char <-> int 或 char
            (Type::CChar, Type::Int32) | (Type::Int32, Type::CChar) => true,
            (Type::CChar, Type::Char) | (Type::Char, Type::CChar) => true,
            // c_float <-> float
            (Type::CFloat, Type::Float32) | (Type::Float32, Type::CFloat) => true,
            // c_double <-> double
            (Type::CDouble, Type::Float64) | (Type::Float64, Type::CDouble) => true,
            // size_t/ssize_t <-> long 和 int
            (Type::SizeT, Type::Int64) | (Type::Int64, Type::SizeT) => true,
            (Type::SizeT, Type::Int32) | (Type::Int32, Type::SizeT) => true,
            (Type::SSizeT, Type::Int64) | (Type::Int64, Type::SSizeT) => true,
            (Type::SSizeT, Type::Int32) | (Type::Int32, Type::SSizeT) => true,
            // uintptr_t/intptr_t <-> long 和 int
            (Type::UIntPtr, Type::Int64) | (Type::Int64, Type::UIntPtr) => true,
            (Type::UIntPtr, Type::Int32) | (Type::Int32, Type::UIntPtr) => true,
            (Type::IntPtr, Type::Int64) | (Type::Int64, Type::IntPtr) => true,
            (Type::IntPtr, Type::Int32) | (Type::Int32, Type::IntPtr) => true,
            // ptr (void*) <-> uintptr_t/intptr_t
            (Type::Pointer(_), Type::UIntPtr) | (Type::UIntPtr, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::IntPtr) | (Type::IntPtr, Type::Pointer(_)) => true,
            // c_bool <-> bool 和 int
            (Type::CBool, Type::Bool) | (Type::Bool, Type::CBool) => true,
            (Type::CBool, Type::Int32) | (Type::Int32, Type::CBool) => true,
            // String -> c_string (c_char*) 自动转换
            (Type::String, Type::Pointer(to_inner)) => {
                if matches!(to_inner.as_ref(), Type::CChar) {
                    return true;
                }
                false
            }
            // 指针类型与 long/int 之间的兼容（用于 FFI）
            (Type::Pointer(_), Type::Int64) | (Type::Int64, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::Int32) | (Type::Int32, Type::Pointer(_)) => true,
            // FFI 整数类型 <-> 指针 (用于 c_long/c_ulong 等作为指针值的场景)
            (Type::Pointer(_), Type::CLong) | (Type::CLong, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CULong) | (Type::CULong, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CInt) | (Type::CInt, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CUInt) | (Type::CUInt, Type::Pointer(_)) => true,
            // ptr (void*) 可以转换为任何其他指针类型（C 语言规则）
            (Type::Pointer(from_inner), Type::Pointer(_)) => {
                if matches!(from_inner.as_ref(), Type::CVoid) {
                    return true;
                }
                false
            }
            // 数组类型可以退化为指针类型（数组作为参数传递时退化为指针）
            (Type::Array(_), Type::Pointer(_)) => true,
            // FFI 类型之间的兼容
            (Type::CInt, Type::CLong) | (Type::CLong, Type::CInt) => true,
            (Type::CInt, Type::CShort) | (Type::CShort, Type::CInt) => true,
            (Type::CInt, Type::CChar) | (Type::CChar, Type::CInt) => true,
            (Type::CFloat, Type::CDouble) | (Type::CDouble, Type::CFloat) => true,
            (Type::SizeT, Type::UIntPtr) | (Type::UIntPtr, Type::SizeT) => true,
            (Type::SSizeT, Type::IntPtr) | (Type::IntPtr, Type::SSizeT) => true,
            (Type::UIntPtr, Type::IntPtr) | (Type::IntPtr, Type::UIntPtr) => true,
            // 函数类型兼容性：函数指针可以赋值给兼容的函数指针类型
            // 顶层函数可以赋值给函数指针类型（当参数和返回类型匹配时）
            (Type::Function(from_fn), Type::Function(to_fn)) => {
                // 检查返回类型是否兼容
                let ret_compatible = Self::types_compatible(self, &from_fn.return_type, &to_fn.return_type);
                // 检查参数数量是否相同
                let params_count_match = from_fn.params.len() == to_fn.params.len();
                // 检查每个参数类型是否兼容（允许协变/逆变）
                let params_compatible = if params_count_match {
                    from_fn.params.iter().zip(to_fn.params.iter())
                        .all(|(from_param, to_param)| {
                            Self::types_compatible(self, from_param, to_param) || 
                            Self::types_compatible(self, to_param, from_param)
                        })
                } else {
                    false
                };
                ret_compatible && params_count_match && params_compatible
            }
            _ => false,
        }
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
        matches!(ty, 
            // 内置数值类型
            Type::Int32 | Type::Int64 | Type::Float32 | Type::Float64 | Type::Char |
            // FFI 数值类型
            Type::CInt | Type::CUInt | Type::CLong |
            Type::CShort | Type::CUShort | Type::CChar | Type::CUChar |
            Type::CFloat | Type::CDouble | Type::SizeT | Type::SSizeT |
            Type::UIntPtr | Type::IntPtr
        )
    }

    /// 检查 subtype 是否是 supertype 的子类型
    ///
    /// 通过递归遍历继承层次结构来确定类型兼容性。
    /// 子类可以赋值给父类（里氏替换原则）。
    ///
    /// # Arguments
    /// * `subtype` - 待检查的子类型名称
    /// * `supertype` - 目标父类型名称
    ///
    /// # Returns
    /// 如果 subtype 是 supertype 的子类型则返回 true
    ///
    /// # Algorithm
    /// 时间复杂度: O(h)，其中 h 是继承链的高度
    /// 空间复杂度: O(1)，迭代实现避免递归栈溢出
    fn is_subtype_of(&self, subtype: &str, supertype: &str) -> bool {
        // 相同类型必然是子类型
        if subtype == supertype {
            return true;
        }
        
        // 特殊处理：所有类都是 Object 的子类型
        if supertype == "Object" {
            // 检查 subtype 是否是一个有效的类名（不是内置类型别名）
            return self.type_registry.class_exists(subtype)
                || subtype == "String"
                || subtype == "Function";
        }
        
        // 检查 subtype 是否实现了 supertype 接口
        if self.type_registry.interface_exists(supertype) {
            if let Some(class_info) = self.type_registry.get_class(subtype) {
                if class_info.interfaces.iter().any(|i| i == supertype) {
                    return true;
                }
            }
        }
        
        // 迭代遍历继承链
        let mut current = subtype.to_string();
        let mut visited = std::collections::HashSet::new();
        
        loop {
            // 防止循环继承导致的无限循环
            if !visited.insert(current.clone()) {
                return false; // 检测到循环继承
            }
            
            if let Some(class_info) = self.type_registry.get_class(&current) {
                // 检查当前类是否实现了目标接口
                if self.type_registry.interface_exists(supertype) {
                    if class_info.interfaces.iter().any(|i| i == supertype) {
                        return true;
                    }
                }
                match &class_info.parent {
                    Some(parent) => {
                        if parent == supertype {
                            return true;
                        }
                        current = parent.clone();
                    }
                    None => return false, // 到达继承链顶端
                }
            } else {
                // 如果不是类，检查内置类型关系
                // String 是 Object 的子类型，但其他内置类型不是
                return (subtype == "String" || subtype == "Function") && supertype == "Object";
            }
        }
    }

    /// 检查两个类名是否指向同一个类
    ///
    /// 处理命名空间前缀的情况，例如 "JsonValue" 和 "json::JsonValue"
    /// 如果两个类名都能解析到同一个类定义，则返回 true
    ///
    /// # Arguments
    /// * `name1` - 第一个类名（可能带命名空间前缀）
    /// * `name2` - 第二个类名（可能带命名空间前缀）
    ///
    /// # Returns
    /// 如果两个类名指向同一个类则返回 true
    fn is_same_class(&self, name1: &str, name2: &str) -> bool {
        // 如果完全相同，直接返回 true
        if name1 == name2 {
            return true;
        }

        // 提取简单类名（去掉命名空间前缀）
        let simple1 = name1.rfind("::").map(|pos| &name1[pos + 2..]).unwrap_or(name1);
        let simple2 = name2.rfind("::").map(|pos| &name2[pos + 2..]).unwrap_or(name2);

        // 如果简单类名不同，直接返回 false
        if simple1 != simple2 {
            return false;
        }

        // 尝试直接查找两个类名（不依赖 current_namespace）
        let class1 = self.type_registry.classes.get(name1);
        let class2 = self.type_registry.classes.get(name2);

        match (class1, class2) {
            (Some(c1), Some(c2)) => {
                // 两个类都找到了，检查它们是否是同一个类（通过名称比较）
                c1.name == c2.name
            }
            (Some(c1), None) => {
                // 只有 name1 找到了，检查 name2 是否是 name1 的简单名形式
                // 例如 name1="json::JsonValue", name2="JsonValue"
                c1.name.rfind("::").map(|pos| &c1.name[pos + 2..]).unwrap_or(&c1.name) == simple2
            }
            (None, Some(c2)) => {
                // 只有 name2 找到了，检查 name1 是否是 name2 的简单名形式
                c2.name.rfind("::").map(|pos| &c2.name[pos + 2..]).unwrap_or(&c2.name) == simple1
            }
            (None, None) => {
                // 两个都没直接找到，说明这两个类名可能都不存在
                // 这种情况下不能假设它们是同一个类
                false
            }
        }
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
        methods.iter().find(|m| {
            self.match_method_params_with_namespace(&m.params, arg_types)
        })
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
        params.iter().zip(arg_types.iter()).all(|(p, a)| {
            self.types_compatible_with_namespace(&p.param_type, a)
        })
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
    fn types_compatible_with_namespace(&self, param_type: &Type, arg_type: &Type) -> bool {
        self.type_registry.types_compatible_with_namespace(param_type, arg_type)
    }

    /// 整数类型提升
    pub fn promote_integer_types(&self, left: &Type, right: &Type) -> Type {
        match (left, right) {
            (Type::Int64, _) | (_, Type::Int64) => Type::Int64,
            _ => Type::Int32,
        }
    }

    /// 检查参数是否与参数定义兼容（支持可变参数和命名参数 name=value）
    pub fn check_arguments_compatible(&mut self, args: &[Expr], params: &[ParameterInfo], _line: usize, _column: usize) -> Result<(), String> {
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
        let has_varargs = if !params.is_empty() { params.iter().any(|p| p.is_varargs) } else { false };

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
                        return Err(format!("Named argument '{}' type mismatch: expected {}, got {}",
                            params[last_idx].name, params[last_idx].param_type, arg_type));
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
                                return Err(format!("Varargs argument type mismatch: expected {}, got {}",
                                    elem_type, arg_type));
                            }
                            arg_for_param[last_idx] = Some(positional[pos_idx]);
                        }
                    } else if remaining_count > 1 {
                        // 多个剩余参数，检查每个元素类型
                        if let Some(ref elem_type) = varargs_elem_type {
                            for j in pos_idx..positional.len() {
                                let arg_type = self.infer_expr_type_collect_errors(positional[j]);
                                if !self.types_compatible(&arg_type, elem_type) {
                                    return Err(format!("Varargs argument {} type mismatch: expected {}, got {}",
                                        j + 1, elem_type, arg_type));
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
                return Err(format!("Expected {} arguments, got {}", params.len(), positional.len()));
            }

            // 第三步：检查是否有必需的参数未提供
            for i in 0..fixed_count {
                if arg_for_param[i].is_none() {
                    return Err(format!("Missing argument for parameter '{}'", params[i].name));
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
                        return Err(format!("Argument {} type mismatch: expected {}, got {}",
                            i + 1, param.param_type, arg_type));
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
                return Err(format!("Expected at least {} arguments, got {}", last_idx, args.len()));
            }

            // 检查固定参数
            for i in 0..last_idx {
                let arg_type = self.infer_expr_type_collect_errors(&args[i]);
                if !self.types_compatible(&arg_type, &params[i].param_type) {
                    return Err(format!("Argument {} type mismatch: expected {}, got {}",
                        i + 1, params[i].param_type, arg_type));
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
                    return Err(format!("Varargs argument {} type mismatch: expected {}, got {}",
                        i + 1, vararg_element_type, arg_type));
                }
            }
        } else {
            // 非可变参数：参数数量必须完全匹配
            if params.len() != args.len() {
                return Err(format!("Expected {} arguments, got {}", params.len(), args.len()));
            }

            for (i, (arg, param)) in args.iter().zip(params.iter()).enumerate() {
                let arg_type = self.infer_expr_type_collect_errors(arg);
                if !self.types_compatible(&arg_type, &param.param_type) {
                    return Err(format!("Argument {} type mismatch: expected {}, got {}",
                        i + 1, param.param_type, arg_type));
                }
            }
        }

        Ok(())
    }

    /// 推断 String 方法调用的返回类型
    pub fn infer_string_method_call(&mut self, method_name: &str, args: &[Expr], line: usize, column: usize) -> cayResult<Type> {
        match method_name {
            "length" => {
                if !args.is_empty() {
                    return Err(self.report_error(line, column, "String.length() takes no arguments".to_string()));
                }
                Ok(Type::Int32)
            }
            "substring" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.report_error(line, column, "String.substring() takes 1 or 2 arguments".to_string()));
                }
                // 检查参数类型
                for (i, arg) in args.iter().enumerate() {
                    let arg_type = self.infer_expr_type_collect_errors(arg);
                    if !arg_type.is_integer() {
                        return Err(self.report_error(line, column, format!("Argument {} of substring() must be integer, got {}", i + 1, arg_type)));
                    }
                }
                Ok(Type::String)
            }
            "indexOf" => {
                if args.len() < 1 || args.len() > 2 {
                    return Err(self.report_error(line, column, "String.indexOf() takes 1 or 2 arguments".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("First argument of indexOf() must be string, got {}", arg_type)));
                }
                if args.len() == 2 {
                    let start_type = self.infer_expr_type_collect_errors(&args[1]);
                    if !start_type.is_integer() {
                        return Err(self.report_error(line, column, format!("Second argument of indexOf() must be integer, got {}", start_type)));
                    }
                }
                Ok(Type::Int32)
            }
            "lastIndexOf" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.lastIndexOf() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of lastIndexOf() must be string, got {}", arg_type)));
                }
                Ok(Type::Int32)
            }
            "charAt" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.charAt() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if !arg_type.is_integer() {
                    return Err(self.report_error(line, column, format!("Argument of charAt() must be integer, got {}", arg_type)));
                }
                Ok(Type::Char)
            }
            "replace" => {
                if args.len() != 2 {
                    return Err(self.report_error(line, column, "String.replace() takes 2 arguments".to_string()));
                }
                for (i, arg) in args.iter().enumerate() {
                    let arg_type = self.infer_expr_type_collect_errors(arg);
                    if arg_type != Type::String {
                        return Err(self.report_error(line, column, format!("Argument {} of replace() must be string, got {}", i + 1, arg_type)));
                    }
                }
                Ok(Type::String)
            }
            "isEmpty" => {
                if !args.is_empty() {
                    return Err(self.report_error(line, column, "String.isEmpty() takes no arguments".to_string()));
                }
                Ok(Type::Bool)
            }
            "equals" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.equals() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of equals() must be string, got {}", arg_type)));
                }
                Ok(Type::Bool)
            }
            "equalsIgnoreCase" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.equalsIgnoreCase() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of equalsIgnoreCase() must be string, got {}", arg_type)));
                }
                Ok(Type::Bool)
            }
            "c_str" => {
                if !args.is_empty() {
                    return Err(self.report_error(line, column, "String.c_str() takes no arguments".to_string()));
                }
                Ok(Type::Pointer(Box::new(Type::CChar)))  // 返回 c_char* 指针类型，与 codegen 中的 i8* 一致
            }
            "startsWith" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.startsWith() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of startsWith() must be string, got {}", arg_type)));
                }
                Ok(Type::Bool)
            }
            "endsWith" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.endsWith() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of endsWith() must be string, got {}", arg_type)));
                }
                Ok(Type::Bool)
            }
            "trim" => {
                if !args.is_empty() {
                    return Err(self.report_error(line, column, "String.trim() takes no arguments".to_string()));
                }
                Ok(Type::String)
            }
            "toLowerCase" => {
                if !args.is_empty() {
                    return Err(self.report_error(line, column, "String.toLowerCase() takes no arguments".to_string()));
                }
                Ok(Type::String)
            }
            "toUpperCase" => {
                if !args.is_empty() {
                    return Err(self.report_error(line, column, "String.toUpperCase() takes no arguments".to_string()));
                }
                Ok(Type::String)
            }
            "contains" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.contains() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of contains() must be string, got {}", arg_type)));
                }
                Ok(Type::Bool)
            }
            "compareTo" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.compareTo() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of compareTo() must be string, got {}", arg_type)));
                }
                Ok(Type::Int32)
            }
            _ => Err(self.report_error(line, column, format!("Unknown String method '{}'", method_name))),
        }
    }
}
