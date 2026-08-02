use crate::miette_diagnostic::ErrorCodes;
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;

/// null 字面量的内部类型标记名。
/// `<null>` 不是合法的源语言标识符，不会与用户定义的类名冲突。
/// 语义分析阶段用 `Type::Object(NULL_TYPE_NAME)` 表示 null 字面量的类型，
/// 以便与真正的 Object 实例精确区分（修复 null 与 Object 混用导致的类型大洞）。
pub const NULL_TYPE_NAME: &str = "<null>";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum Type {
    Void,
    Int32,
    Int64,
    Float32,
    Float64,
    Bool,
    String,
    Char,
    Object(String),
    Array(Box<Type>),
    Function(Box<FunctionType>),
    Auto, // 自动类型推断占位符
    // 泛型支持
    GenericParam(String),       // 泛型类型参数: T
    Generic(String, Vec<Type>), // 泛型特化: Optional<Int32>, ArrayList<Int32>
    // FFI 类型
    CInt,    // C int (通常为 i32)
    CUInt,   // C unsigned int (通常为 u32)
    CLong,   // C long (平台相关: Windows i32, Linux/macOS i64)
    CULong,  // C unsigned long (平台相关)
    CShort,  // C short (i16)
    CUShort, // C unsigned short (u16)
    CChar,   // C char (i8)
    CUChar,  // C unsigned char (u8)
    CFloat,  // C float (f32)
    CDouble, // C double (f64)
    SizeT,   // size_t (usize, 平台相关)
    SSizeT,  // ssize_t (isize, 平台相关)
    UIntPtr, // uintptr_t (usize)
    IntPtr,  // intptr_t (isize)
    CVoid,   // C void (用于指针)
    CBool,   // C bool (i8, 0 或 1)
    // FFI 指针类型
    Pointer(Box<Type>), // 通用指针类型: Pointer(CVoid) = void*
    // FFI 结构体类型
    Struct(String), // 命名结构体: Struct("SDL_Window")
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct FunctionType {
    pub params: Vec<Type>,
    pub return_type: Box<Type>,
    pub is_static: bool,
    pub is_closure: bool, // 是否是闭包（有捕获变量）
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct TypeParamInfo {
    pub name: String,
    pub bound: Option<String>,      // 类型边界（暂不强制检查）
    pub default_type: Option<Type>, // 默认类型
}

/// 用给定的类型参数映射递归替换类型中的泛型参数。
///
/// 用于将方法签名中的泛型参数（如接口 `Iterator<T>` 的返回类型 `T`）
/// 替换为调用处的具体类型实参（如 `String`）。
pub fn substitute_type_params(ty: &Type, mapping: &HashMap<String, Type>) -> Type {
    match ty {
        Type::GenericParam(name) => mapping.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::Object(name) => mapping.get(name).cloned().unwrap_or_else(|| ty.clone()),
        Type::Array(inner) => Type::Array(Box::new(substitute_type_params(inner, mapping))),
        Type::Pointer(inner) => Type::Pointer(Box::new(substitute_type_params(inner, mapping))),
        Type::Generic(base, args) => Type::Generic(
            base.clone(),
            args.iter()
                .map(|a| substitute_type_params(a, mapping))
                .collect(),
        ),
        Type::Function(func_type) => Type::Function(Box::new(FunctionType {
            return_type: Box::new(substitute_type_params(&func_type.return_type, mapping)),
            params: func_type
                .params
                .iter()
                .map(|p| substitute_type_params(p, mapping))
                .collect(),
            is_static: func_type.is_static,
            is_closure: func_type.is_closure,
        })),
        _ => ty.clone(),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ClassInfo {
    pub name: String,
    pub type_params: Vec<TypeParamInfo>, // 泛型类型参数: <T, U, ...>
    pub methods: HashMap<String, Vec<MethodInfo>>, // 支持方法重载：同名方法可以有多个
    pub fields: HashMap<String, FieldInfo>,
    pub constructors: Vec<ConstructorInfo>, // 构造函数列表
    pub has_destructor: bool,               // 是否有析构函数
    pub parent: Option<String>,
    pub interfaces: Vec<Type>, // 实现的接口列表（支持泛型实参）
    pub is_abstract: bool,     // 是否是抽象类
    pub is_final: bool,        // 是否是final类（禁止继承）
    /// C++ 互操作类：对象无 16 字节头，字段从 offset 0 起，与普通 C++ 类布局一致。
    /// 此类不生成 vtable，instanceof/虚函数派发对其不可用。
    pub is_interop: bool,
    /// @stack_only 类：禁止通过 new 在堆上分配，只能作为局部变量/值类型使用。
    pub is_stack_only: bool,
    pub vtable_layout: Option<VTableLayout>, // vtable 布局信息
}

/// 构造函数信息
#[derive(Debug, Clone, Serialize)]
pub struct ConstructorInfo {
    pub params: Vec<ParameterInfo>,
    pub is_public: bool,
    pub is_private: bool,
    pub is_protected: bool,
    pub loc: crate::miette_diagnostic::SourceLocation,
}

#[derive(Debug, Clone, Serialize)]
pub struct InterfaceInfo {
    pub name: String,
    pub type_params: Vec<TypeParamInfo>, // 泛型类型参数: <T, U, ...>
    pub methods: HashMap<String, MethodInfo>,
}

/// struct 信息 - 值类型，无继承
#[derive(Debug, Clone, Serialize)]
pub struct StructInfo {
    pub name: String,
    pub type_params: Vec<TypeParamInfo>, // 泛型类型参数: <T, U, ...>
    pub fields: HashMap<String, FieldInfo>,
    pub field_order: Vec<String>, // 字段定义顺序，用于 LLVM GEP 索引
    pub methods: HashMap<String, Vec<MethodInfo>>, // 支持方法重载
    pub constructors: Vec<ConstructorInfo>, // 构造函数列表
    pub is_public: bool,
}

impl StructInfo {
    /// 根据方法名和参数类型查找方法（支持可变参数）
    /// 时间复杂度: O(n)，n 为同名方法数量
    pub fn find_method(&self, name: &str, arg_types: &[Type]) -> Option<&MethodInfo> {
        let methods = self.methods.get(name)?;

        // 第一遍：寻找精确匹配
        for m in methods.iter() {
            if Self::match_method_params_exact(&m.params, arg_types) {
                return Some(m);
            }
        }

        // 第二遍：寻找兼容匹配（允许隐式转换）
        methods
            .iter()
            .find(|m| Self::match_method_params(&m.params, arg_types))
    }

    /// 检查类型是否精确匹配（支持泛型参数）
    fn types_match_exact(param_type: &Type, arg_type: &Type) -> bool {
        // 泛型参数类型可以匹配任何类型
        if matches!(
            (param_type, arg_type),
            (Type::GenericParam(_), _) | (_, Type::GenericParam(_))
        ) {
            return true;
        }

        // 处理泛型类型匹配：Generic("Wrapper", [GenericParam("T")]) 应该匹配 Generic("Wrapper", [Int32])
        match (param_type, arg_type) {
            (Type::Generic(param_name, param_args), Type::Generic(arg_name, arg_args)) => {
                if param_name != arg_name || param_args.len() != arg_args.len() {
                    return false;
                }
                // 逐个比较类型参数
                param_args
                    .iter()
                    .zip(arg_args.iter())
                    .all(|(p, a)| Self::types_match_exact(p, a))
            }
            (Type::Generic(param_name, param_args), Type::Object(arg_name)) => {
                // 解析对象类型名中的泛型参数: "Wrapper<int>" -> ("Wrapper", ["int"])
                if let Some(pos) = arg_name.find('<') {
                    let base_name = &arg_name[..pos];
                    if param_name != base_name {
                        return false;
                    }
                    // 解析类型参数
                    let args_str = &arg_name[pos + 1..arg_name.len() - 1];
                    let arg_type_names: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
                    if param_args.len() != arg_type_names.len() {
                        return false;
                    }
                    // 对于参数中的GenericParam，可以匹配任何类型
                    true
                } else {
                    // 没有泛型参数的对象类型，基础类型必须匹配
                    param_name == arg_name && param_args.is_empty()
                }
            }
            _ => param_type == arg_type,
        }
    }

    /// 精确匹配方法参数（不考虑隐式转换，支持非末尾可变参数）
    fn match_method_params_exact(params: &[ParameterInfo], arg_types: &[Type]) -> bool {
        if params.is_empty() {
            return arg_types.is_empty();
        }

        // 找到可变参数的位置（如果有）
        let varargs_idx = params.iter().position(|p| p.is_varargs);

        if let Some(vi) = varargs_idx {
            let fixed_before = vi; // 可变参数之前的固定参数
            let fixed_after = params.len() - vi - 1; // 可变参数之后的固定参数
            let min_args = fixed_before + fixed_after;
            if arg_types.len() < min_args {
                return false;
            }

            // 检查可变参数之前的固定参数（精确匹配，支持泛型参数）
            for i in 0..fixed_before {
                if !Self::types_match_exact(&params[i].param_type, &arg_types[i]) {
                    return false;
                }
            }

            // 检查可变参数（精确匹配元素类型，支持泛型参数）
            let vararg_elem_type = match &params[vi].param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => &params[vi].param_type,
            };
            let varargs_len = arg_types.len() - min_args;
            let varargs_end = fixed_before + varargs_len;

            // 如果恰好一个数组参数且类型匹配，直接接受
            if varargs_len == 1
                && Self::types_match_exact(&params[vi].param_type, &arg_types[fixed_before])
            {
                // 直接传递数组给可变参数
            } else {
                for i in fixed_before..varargs_end {
                    if !Self::types_match_exact(vararg_elem_type, &arg_types[i]) {
                        return false;
                    }
                }
            }

            // 检查可变参数之后的固定参数（精确匹配，支持泛型参数）
            for i in 0..fixed_after {
                if !Self::types_match_exact(
                    &params[vi + 1 + i].param_type,
                    &arg_types[varargs_end + i],
                ) {
                    return false;
                }
            }
            true
        } else {
            // 非可变参数：参数数量必须完全匹配
            if params.len() != arg_types.len() {
                return false;
            }
            params
                .iter()
                .zip(arg_types.iter())
                .all(|(p, a)| Self::types_match_exact(&p.param_type, a))
        }
    }

    /// 匹配方法参数（支持可变参数，支持非末尾可变参数）
    fn match_method_params(params: &[ParameterInfo], arg_types: &[Type]) -> bool {
        if params.is_empty() {
            return arg_types.is_empty();
        }

        // 找到可变参数的位置（如果有）
        let varargs_idx = params.iter().position(|p| p.is_varargs);

        if let Some(vi) = varargs_idx {
            let fixed_before = vi;
            let fixed_after = params.len() - vi - 1;
            let min_args = fixed_before + fixed_after;

            if arg_types.len() < min_args {
                return false;
            }

            // 检查可变参数之前的固定参数
            for i in 0..fixed_before {
                if !Self::types_compatible(&params[i].param_type, &arg_types[i]) {
                    return false;
                }
            }

            // 检查可变参数
            let vararg_elem_type = match &params[vi].param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => &params[vi].param_type,
            };
            let varargs_len = arg_types.len() - min_args;
            let varargs_end = fixed_before + varargs_len;

            if varargs_len == 1
                && Self::types_compatible(&params[vi].param_type, &arg_types[fixed_before])
            {
                // 直接传递数组给可变参数
            } else {
                for i in fixed_before..varargs_end {
                    if !Self::types_compatible(vararg_elem_type, &arg_types[i]) {
                        return false;
                    }
                }
            }

            // 检查可变参数之后的固定参数
            for i in 0..fixed_after {
                if !Self::types_compatible(
                    &params[vi + 1 + i].param_type,
                    &arg_types[varargs_end + i],
                ) {
                    return false;
                }
            }
            true
        } else {
            if params.len() != arg_types.len() {
                return false;
            }
            params
                .iter()
                .zip(arg_types.iter())
                .all(|(p, a)| Self::types_compatible(&p.param_type, a))
        }
    }

    /// 检查类型兼容性（支持基本类型隐式转换）
    fn types_compatible(param_type: &Type, arg_type: &Type) -> bool {
        if param_type == arg_type {
            return true;
        }
        // 泛型参数类型匹配
        if matches!(
            (param_type, arg_type),
            (Type::GenericParam(_), _) | (_, Type::GenericParam(_))
        ) {
            return true;
        }
        // 基本类型隐式转换
        match (param_type, arg_type) {
            (Type::Int64, Type::Int32) => true,
            (Type::Float32, Type::Int32) => true,
            (Type::Float64, Type::Int32) => true,
            (Type::Float64, Type::Int64) => true,
            (Type::Float64, Type::Float32) => true,
            (Type::Float32, Type::Float64) => true,
            _ => false,
        }
    }
}

/// enum 信息 - tagged union / ADT
#[derive(Debug, Clone, Serialize)]
pub struct EnumInfo {
    pub name: String,
    pub type_params: Vec<TypeParamInfo>, // 泛型类型参数
    pub variants: Vec<EnumVariantInfo>,
    pub methods: HashMap<String, Vec<MethodInfo>>, // 支持方法重载
    pub is_public: bool,
}

/// enum variant 信息
#[derive(Debug, Clone, Serialize)]
pub struct EnumVariantInfo {
    pub name: String,
    pub payload_type: Option<Type>, // variant 携带的数据类型
}

impl ClassInfo {
    /// 添加方法到类中（支持重载）
    pub fn add_method(&mut self, method: MethodInfo) {
        self.methods
            .entry(method.name.clone())
            .or_insert_with(Vec::new)
            .push(method);
    }

    /// 更新指定方法的返回类型（用于 fn 自动推断）
    pub fn update_method_return_type(
        &mut self,
        method_name: &str,
        params: &[ParameterInfo],
        new_return_type: Type,
    ) -> bool {
        if let Some(methods) = self.methods.get_mut(method_name) {
            for method in methods.iter_mut() {
                if method.params.len() == params.len() {
                    let params_match = method
                        .params
                        .iter()
                        .zip(params.iter())
                        .all(|(a, b)| a.param_type == b.param_type);
                    if params_match {
                        method.return_type = new_return_type;
                        return true;
                    }
                }
            }
        }
        false
    }

    /// 根据方法名和参数类型查找方法（支持可变参数）
    pub fn find_method(&self, name: &str, arg_types: &[Type]) -> Option<&MethodInfo> {
        let methods = self.methods.get(name)?;

        // 第一遍：寻找精确匹配
        for m in methods.iter() {
            if Self::match_method_params_exact(&m.params, arg_types) {
                return Some(m);
            }
        }

        // 第二遍：寻找兼容匹配（允许隐式转换）
        methods
            .iter()
            .find(|m| Self::match_method_params(&m.params, arg_types))
    }

    /// 检查类型是否精确匹配（支持泛型参数）
    fn types_match_exact(param_type: &Type, arg_type: &Type) -> bool {
        // 泛型参数类型可以匹配任何类型
        if matches!(
            (param_type, arg_type),
            (Type::GenericParam(_), _) | (_, Type::GenericParam(_))
        ) {
            return true;
        }

        // 处理泛型类型匹配：Generic("Wrapper", [GenericParam("T")]) 应该匹配 Object("Wrapper<int>")
        match (param_type, arg_type) {
            (Type::Generic(param_name, param_args), Type::Generic(arg_name, arg_args)) => {
                if param_name != arg_name || param_args.len() != arg_args.len() {
                    return false;
                }
                // 逐个比较类型参数
                param_args
                    .iter()
                    .zip(arg_args.iter())
                    .all(|(p, a)| Self::types_match_exact(p, a))
            }
            (Type::Generic(param_name, param_args), Type::Object(arg_name)) => {
                // 解析对象类型名中的泛型参数: "Wrapper<int>" -> ("Wrapper", ["int"])
                if let Some(pos) = arg_name.find('<') {
                    let base_name = &arg_name[..pos];
                    if param_name != base_name {
                        return false;
                    }
                    // 参数数量必须匹配
                    let args_str = &arg_name[pos + 1..arg_name.len() - 1];
                    let arg_type_names: Vec<&str> = args_str.split(',').map(|s| s.trim()).collect();
                    if param_args.len() != arg_type_names.len() {
                        return false;
                    }
                    // 对于参数中的GenericParam，可以匹配任何类型
                    true
                } else {
                    // 没有泛型参数的对象类型，基础类型必须匹配
                    param_name == arg_name && param_args.is_empty()
                }
            }
            _ => param_type == arg_type,
        }
    }

    /// 精确匹配方法参数（不考虑隐式转换，支持非末尾可变参数）
    fn match_method_params_exact(params: &[ParameterInfo], arg_types: &[Type]) -> bool {
        if params.is_empty() {
            return arg_types.is_empty();
        }

        // 找到可变参数的位置（如果有）
        let varargs_idx = params.iter().position(|p| p.is_varargs);

        if let Some(vi) = varargs_idx {
            let fixed_before = vi; // 可变参数之前的固定参数
            let fixed_after = params.len() - vi - 1; // 可变参数之后的固定参数
            let min_args = fixed_before + fixed_after;
            if arg_types.len() < min_args {
                return false;
            }

            // 检查可变参数之前的固定参数（精确匹配，支持泛型参数）
            for i in 0..fixed_before {
                if !Self::types_match_exact(&params[i].param_type, &arg_types[i]) {
                    return false;
                }
            }

            // 检查可变参数（精确匹配元素类型，支持泛型参数）
            let vararg_elem_type = match &params[vi].param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => &params[vi].param_type,
            };
            let varargs_len = arg_types.len() - min_args;
            let varargs_end = fixed_before + varargs_len;

            // 如果恰好一个数组参数且类型匹配，直接接受
            if varargs_len == 1
                && Self::types_match_exact(&params[vi].param_type, &arg_types[fixed_before])
            {
                // 直接传递数组给可变参数
            } else {
                for i in fixed_before..varargs_end {
                    if !Self::types_match_exact(vararg_elem_type, &arg_types[i]) {
                        return false;
                    }
                }
            }

            // 检查可变参数之后的固定参数（精确匹配，支持泛型参数）
            for i in 0..fixed_after {
                if !Self::types_match_exact(
                    &params[vi + 1 + i].param_type,
                    &arg_types[varargs_end + i],
                ) {
                    return false;
                }
            }
            true
        } else {
            // 非可变参数：参数数量必须完全匹配
            if params.len() != arg_types.len() {
                return false;
            }
            params
                .iter()
                .zip(arg_types.iter())
                .all(|(p, a)| Self::types_match_exact(&p.param_type, a))
        }
    }

    /// 匹配方法参数（支持可变参数，支持非末尾可变参数）
    fn match_method_params(params: &[ParameterInfo], arg_types: &[Type]) -> bool {
        if params.is_empty() {
            return arg_types.is_empty();
        }

        let varargs_idx = params.iter().position(|p| p.is_varargs);

        if let Some(vi) = varargs_idx {
            let fixed_before = vi;
            let fixed_after = params.len() - vi - 1;
            let min_args = fixed_before + fixed_after;
            if arg_types.len() < min_args {
                return false;
            }

            // 检查可变参数之前的固定参数
            for i in 0..fixed_before {
                if !Self::types_match(&params[i].param_type, &arg_types[i]) {
                    return false;
                }
            }

            // 检查可变参数（兼容匹配）
            let vararg_elem_type = match &params[vi].param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => &params[vi].param_type,
            };
            let varargs_len = arg_types.len() - min_args;
            let varargs_end = fixed_before + varargs_len;

            // 如果恰好一个数组参数且类型匹配，直接接受
            if varargs_len == 1
                && Self::types_match(&params[vi].param_type, &arg_types[fixed_before])
            {
                // 直接传递数组给可变参数
            } else {
                for i in fixed_before..varargs_end {
                    if !Self::types_match(vararg_elem_type, &arg_types[i]) {
                        return false;
                    }
                }
            }

            // 检查可变参数之后的固定参数
            for i in 0..fixed_after {
                if !Self::types_match(&params[vi + 1 + i].param_type, &arg_types[varargs_end + i]) {
                    return false;
                }
            }
            true
        } else {
            if params.len() != arg_types.len() {
                return false;
            }
            params
                .iter()
                .zip(arg_types.iter())
                .all(|(p, a)| Self::types_match(&p.param_type, a))
        }
    }

    /// 根据方法名查找第一个匹配的方法（用于无参数的情况）
    pub fn find_method_by_name(&self, name: &str) -> Option<&MethodInfo> {
        self.methods.get(name)?.first()
    }

    /// 检查类型是否匹配（支持基本类型转换）
    fn types_match(param_type: &Type, arg_type: &Type) -> bool {
        if param_type == arg_type {
            return true;
        }
        // 泛型参数类型匹配：GenericParam 可以匹配任何类型
        if matches!(
            (param_type, arg_type),
            (Type::GenericParam(_), _) | (_, Type::GenericParam(_))
        ) {
            return true;
        }
        // 允许 int -> long, int -> float, int -> double 等隐式转换
        // 也允许 double -> float 的显式转换（用于字面量）
        match (param_type, arg_type) {
            (Type::Int64, Type::Int32) => true,
            (Type::Float32, Type::Int32) => true,
            (Type::Float64, Type::Int32) => true,
            (Type::Float64, Type::Int64) => true,
            (Type::Float64, Type::Float32) => true,
            (Type::Float32, Type::Float64) => true, // double -> float 截断转换
            // FFI 类型与内置类型的匹配
            (Type::CInt, Type::Int32) | (Type::Int32, Type::CInt) => true,
            (Type::CUInt, Type::Int32) | (Type::Int32, Type::CUInt) => true,
            (Type::CLong, Type::Int64) | (Type::Int64, Type::CLong) => true,
            (Type::CShort, Type::Int32) | (Type::Int32, Type::CShort) => true,
            (Type::CChar, Type::Int32) | (Type::Int32, Type::CChar) => true,
            (Type::CChar, Type::Char) | (Type::Char, Type::CChar) => true,
            (Type::CFloat, Type::Float32) | (Type::Float32, Type::CFloat) => true,
            (Type::CDouble, Type::Float64) | (Type::Float64, Type::CDouble) => true,
            (Type::CBool, Type::Bool) | (Type::Bool, Type::CBool) => true,
            // size_t 和 ssize_t 与整数类型的匹配
            (Type::SizeT, Type::Int64) | (Type::Int64, Type::SizeT) => true,
            (Type::SizeT, Type::Int32) | (Type::Int32, Type::SizeT) => true,
            (Type::SSizeT, Type::Int64) | (Type::Int64, Type::SSizeT) => true,
            (Type::SSizeT, Type::Int32) | (Type::Int32, Type::SSizeT) => true,
            // 指针类型匹配：允许 null 赋值给任何指针类型
            (Type::Pointer(_), Type::Object(obj_name)) if obj_name == "Object" => true,
            // 数组类型可以匹配指针类型（数组退化为指针）
            (Type::Pointer(_), Type::Array(_)) => true,
            // 函数指针类型匹配：允许将静态方法作为函数指针传递
            (Type::Function(expected), Type::Function(actual)) => {
                // 检查返回类型是否匹配
                if !Self::types_match(&expected.return_type, &actual.return_type) {
                    return false;
                }
                // 检查参数数量和类型是否匹配
                if expected.params.len() != actual.params.len() {
                    return false;
                }
                expected
                    .params
                    .iter()
                    .zip(actual.params.iter())
                    .all(|(e, a)| Self::types_match(e, a))
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct MethodInfo {
    pub name: String,
    pub class_name: String,
    pub type_params: Vec<TypeParamInfo>, // 方法级泛型参数
    pub params: Vec<ParameterInfo>,
    pub return_type: Type,
    pub is_public: bool,
    pub is_private: bool,
    pub is_protected: bool,
    pub is_static: bool,
    pub is_native: bool,
    pub is_abstract: bool,          // 是否是抽象方法（无实现）
    pub is_override: bool,          // 标记是否是重写方法
    pub is_final: bool,             // 是否是final方法（禁止重写）
    pub is_test: bool,              // 是否被 @Test 注解标记
    pub vtable_slot: Option<usize>, // 在 vtable 中的槽位编号（仅虚方法有值）
    pub loc: crate::miette_diagnostic::SourceLocation, // 方法定义的源位置
}

/// VTable 布局信息
#[derive(Debug, Clone, Serialize)]
pub struct VTableLayout {
    /// 类名
    pub class_name: String,
    /// vtable 中的方法槽位列表（方法名 → 槽位编号）
    pub slots: HashMap<String, usize>,
    /// vtable 总大小（槽位数量）
    pub size: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FieldInfo {
    pub name: String,
    pub field_type: Type,
    pub is_public: bool,
    pub is_private: bool,
    pub is_protected: bool,
    pub is_static: bool,
    pub is_final: bool,      // 是否是final字段（编译期常量）
    pub is_const_expr: bool, // 是否是编译期常量（static final且初始化值为常量）
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParameterInfo {
    pub name: String,
    pub param_type: Type,
    pub is_varargs: bool, // 是否为可变参数
}

impl ParameterInfo {
    pub fn new(name: String, param_type: Type) -> Self {
        Self {
            name,
            param_type,
            is_varargs: false,
        }
    }

    pub fn new_varargs(name: String, param_type: Type) -> Self {
        // 可变参数类型在内部表示为数组类型
        Self {
            name,
            param_type: Type::Array(Box::new(param_type)),
            is_varargs: true,
        }
    }
}

impl Type {
    pub fn size_in_bytes(&self) -> usize {
        match self {
            Type::Void => 0,
            Type::Int32 => 4,
            Type::Int64 => 8,
            Type::Float32 => 4,
            Type::Float64 => 8,
            Type::Bool => 1,
            Type::Char => 1,
            Type::String => 8,      // 指针大小
            Type::Object(_) => 8,   // 引用类型
            Type::Array(_) => 8,    // 指针大小
            Type::Function(_) => 8, // 函数指针
            Type::Auto => {
                // auto 应在语义分析阶段被解析为具体类型，不应泄漏到这里。
                // 与 codegen/ir 中对 Auto 的既有处理保持一致（按指针大小回退），
                // 避免对用户代码 panic。
                8
            }
            // 泛型类型 — 大小取决于具体实例化，运行时由单态化版本决定
            Type::GenericParam(_) => 8, // 泛型参数默认指针大小
            Type::Generic(_, _) => 8,   // 泛型对象默认指针大小（引用语义）
            // FFI 类型大小 (平台相关，这里使用常见值)
            Type::CInt => 4,    // C int 通常为 4 字节
            Type::CUInt => 4,   // C unsigned int 通常为 4 字节
            Type::CLong => 8,   // C long: Windows 4, Linux/macOS 8，使用 8 作为保守值
            Type::CULong => 8,  // C unsigned long，与 CLong 同大小
            Type::CShort => 2,  // C short 为 2 字节
            Type::CUShort => 2, // C unsigned short 为 2 字节
            Type::CChar => 1,   // C char 为 1 字节
            Type::CUChar => 1,  // C unsigned char 为 1 字节
            Type::CFloat => 4,  // C float 为 4 字节
            Type::CDouble => 8, // C double 为 8 字节
            Type::SizeT => 8,   // size_t 为指针大小 (64位系统)
            Type::SSizeT => 8,  // ssize_t 为指针大小 (64位系统)
            Type::UIntPtr => 8, // uintptr_t 为指针大小
            Type::IntPtr => 8,  // intptr_t 为指针大小
            Type::CVoid => 0,   // void 无大小
            Type::CBool => 1,   // C bool 通常为 1 字节
            // FFI 指针和结构体
            Type::Pointer(_) => 8, // 指针大小 (64位系统)
            Type::Struct(_) => 8,  // 结构体作为指针传递，实际大小由编译器决定
        }
    }

    /// 检查是否为原始类型（包括内置数值类型和FFI类型）
    /// 时间复杂度: O(1)
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            // 内置数值类型
            Type::Int32 |
            Type::Int64 |
            Type::Float32 |
            Type::Float64 |
            Type::Bool |
            Type::Char |
            // FFI 数值类型
            Type::CInt | Type::CUInt | Type::CLong | Type::CULong |
            Type::CShort | Type::CUShort | Type::CChar | Type::CUChar |
            Type::CFloat | Type::CDouble | Type::SizeT | Type::SSizeT |
            Type::UIntPtr | Type::IntPtr | Type::CVoid | Type::CBool
        )
    }

    pub fn is_reference_type(&self) -> bool {
        matches!(
            self,
            Type::String | Type::Object(_) | Type::Array(_) | Type::Generic(_, _)
        )
    }

    /// 判断是否为 null 字面量的内部标记类型
    pub fn is_null_literal(&self) -> bool {
        matches!(self, Type::Object(name) if name == NULL_TYPE_NAME)
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            // 内置整数类型（char 可参与整数运算）
            Type::Int32 | Type::Int64 | Type::Char |
            // FFI 整数类型
            Type::CInt | Type::CUInt | Type::CLong | Type::CULong |
            Type::CShort | Type::CUShort | Type::CChar | Type::CUChar |
            Type::SizeT | Type::SSizeT | Type::UIntPtr | Type::IntPtr
        )
    }

    /// 检查是否是泛型相关类型
    pub fn is_generic(&self) -> bool {
        matches!(self, Type::GenericParam(_) | Type::Generic(_, _))
    }

    /// 返回源码风格的类型显示名（用于泛型特化名，如 `Point<int>`、`Pair<int, String>`）。
    ///
    /// 与 `Display` 的区别：`String` 显示为 `String` 而非 `string`，
    /// 以与泛型实例化语法中使用的类名保持一致。
    pub fn display_name(&self) -> String {
        match self {
            Type::Void => "void".to_string(),
            Type::Int32 => "int".to_string(),
            Type::Int64 => "long".to_string(),
            Type::Float32 => "float".to_string(),
            Type::Float64 => "double".to_string(),
            Type::Bool => "bool".to_string(),
            Type::String => "String".to_string(),
            Type::Char => "char".to_string(),
            Type::Object(name) => name.clone(),
            Type::Array(inner) => format!("{}[]", inner.display_name()),
            Type::Function(func_type) => {
                let params: Vec<String> = func_type
                    .params
                    .iter()
                    .map(|p| p.display_name())
                    .collect();
                format!(
                    "fn({}) -> {}",
                    params.join(", "),
                    func_type.return_type.display_name()
                )
            }
            Type::Auto => "auto".to_string(),
            Type::GenericParam(name) => name.clone(),
            Type::Generic(name, args) => {
                let args_str: Vec<String> = args.iter().map(|a| a.display_name()).collect();
                format!("{}<{}>", name, args_str.join(", "))
            }
            Type::CInt => "c_int".to_string(),
            Type::CUInt => "c_uint".to_string(),
            Type::CLong => "c_long".to_string(),
            Type::CULong => "c_ulong".to_string(),
            Type::CShort => "c_short".to_string(),
            Type::CUShort => "c_ushort".to_string(),
            Type::CChar => "c_char".to_string(),
            Type::CUChar => "c_uchar".to_string(),
            Type::CFloat => "c_float".to_string(),
            Type::CDouble => "c_double".to_string(),
            Type::SizeT => "size_t".to_string(),
            Type::SSizeT => "ssize_t".to_string(),
            Type::UIntPtr => "uintptr_t".to_string(),
            Type::IntPtr => "intptr_t".to_string(),
            Type::CVoid => "c_void".to_string(),
            Type::CBool => "c_bool".to_string(),
            Type::Pointer(inner) => format!("{}*", inner.display_name()),
            Type::Struct(name) => format!("struct {}", name),
        }
    }
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Type::Void => write!(f, "void"),
            Type::Int32 => write!(f, "int"),
            Type::Int64 => write!(f, "long"),
            Type::Float32 => write!(f, "float"),
            Type::Float64 => write!(f, "double"),
            Type::Bool => write!(f, "bool"),
            Type::String => write!(f, "string"),
            Type::Char => write!(f, "char"),
            Type::Object(name) => {
                // null 字面量的内部标记类型对用户显示为 "null"
                if name == NULL_TYPE_NAME {
                    write!(f, "null")
                } else {
                    write!(f, "{}", name)
                }
            }
            Type::Array(inner) => write!(f, "{}[]", inner),
            Type::Function(func_type) => {
                write!(f, "fn(")?;
                for (i, param) in func_type.params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", param)?;
                }
                write!(f, ") -> {}", func_type.return_type)
            }
            Type::Auto => write!(f, "auto"),
            // 泛型类型
            Type::GenericParam(name) => write!(f, "{}", name),
            Type::Generic(name, args) => {
                write!(f, "{}<", name)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, ">")
            }
            // FFI 类型显示
            Type::CInt => write!(f, "c_int"),
            Type::CUInt => write!(f, "c_uint"),
            Type::CLong => write!(f, "c_long"),
            Type::CULong => write!(f, "c_ulong"),
            Type::CShort => write!(f, "c_short"),
            Type::CUShort => write!(f, "c_ushort"),
            Type::CChar => write!(f, "c_char"),
            Type::CUChar => write!(f, "c_uchar"),
            Type::CFloat => write!(f, "c_float"),
            Type::CDouble => write!(f, "c_double"),
            Type::SizeT => write!(f, "size_t"),
            Type::SSizeT => write!(f, "ssize_t"),
            Type::UIntPtr => write!(f, "uintptr_t"),
            Type::IntPtr => write!(f, "intptr_t"),
            Type::CVoid => write!(f, "c_void"),
            Type::CBool => write!(f, "c_bool"),
            // FFI 指针和结构体
            Type::Pointer(inner) => write!(f, "{}*", inner),
            Type::Struct(name) => write!(f, "struct {}", name),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeRegistry {
    pub classes: HashMap<String, ClassInfo>,
    pub structs: HashMap<String, StructInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub interfaces: HashMap<String, InterfaceInfo>,
    /// 接口方法在对象 vtable 中的全局槽位映射。
    /// key 使用 build_interface_vtable_key(interface, method_sig) 构造。
    pub interface_vtable_slots: HashMap<String, usize>,
    /// 命名空间别名映射: simple_name -> namespace_qualified_name (用于 using 声明)
    pub namespace_aliases: HashMap<String, String>,
    /// 类的命名空间路径: qualified_name -> namespace_path
    pub class_namespace_paths: HashMap<String, Vec<String>>,
    /// @FreeFunction 导出的函数名 -> (类名, 方法信息, 源位置)
    /// 用于检测跨类同名冲突
    pub free_functions:
        HashMap<String, (String, MethodInfo, crate::miette_diagnostic::SourceLocation)>,
    /// 当前命名空间上下文 (由语义分析器在处理每个类时设置)
    pub current_namespace: Vec<String>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            classes: HashMap::new(),
            structs: HashMap::new(),
            enums: HashMap::new(),
            interfaces: HashMap::new(),
            interface_vtable_slots: HashMap::new(),
            namespace_aliases: HashMap::new(),
            class_namespace_paths: HashMap::new(),
            free_functions: HashMap::new(),
            current_namespace: Vec::new(),
        };

        // 注册内置根类 Object（所有类的隐式父类）
        registry.register_builtin_object_class();

        // 注册内置类 String（用于支持 String.valueOf() 等静态方法调用）
        registry.register_builtin_string_class();

        // 注册内置类 Integer（用于支持 Integer.parseInt() 等静态方法调用）
        registry.register_builtin_integer_class();

        registry
    }

    /// 注册内置 Object 根类
    /// 时间复杂度: O(1)，空间复杂度: O(1)
    fn register_builtin_object_class(&mut self) {
        let mut object_class = ClassInfo {
            name: "Object".to_string(),
            type_params: Vec::new(),
            methods: HashMap::new(),
            fields: HashMap::new(),
            constructors: Vec::new(),
            has_destructor: false,
            parent: None,
            interfaces: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_interop: false,
            is_stack_only: false,
            vtable_layout: None,
        };

        // int hashCode()：默认基于对象身份（地址）的哈希码
        object_class.add_method(MethodInfo {
            name: "hashCode".to_string(),
            class_name: "Object".to_string(),
            type_params: Vec::new(),
            params: Vec::new(),
            return_type: Type::Int32,
            is_static: false,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: true,
            is_abstract: false,
            is_final: false,
            is_override: false,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // bool equals(Object other)：默认基于对象身份（地址）的相等性
        object_class.add_method(MethodInfo {
            name: "equals".to_string(),
            class_name: "Object".to_string(),
            type_params: Vec::new(),
            params: vec![ParameterInfo {
                name: "other".to_string(),
                param_type: Type::Object("Object".to_string()),
                is_varargs: false,
            }],
            return_type: Type::Bool,
            is_static: false,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: true,
            is_abstract: false,
            is_final: false,
            is_override: false,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // Object 默认构造函数：无参数，无字段初始化
        object_class.constructors.push(ConstructorInfo {
            params: Vec::new(),
            is_public: true,
            is_private: false,
            is_protected: false,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        self.classes.insert("Object".to_string(), object_class);
    }

    /// 注册内置 String 类
    fn register_builtin_string_class(&mut self) {
        // 创建 String 类信息
        let mut string_class = ClassInfo {
            name: "String".to_string(),
            type_params: Vec::new(),
            methods: HashMap::new(),
            fields: HashMap::new(),
            constructors: Vec::new(),
            has_destructor: false,
            parent: Some("Object".to_string()), // String 继承 Object
            interfaces: Vec::new(),
            is_abstract: false,
            is_final: true, // String 是 final 类，不能被继承
            is_interop: false,
            is_stack_only: false,
            vtable_layout: None,
        };

        // 覆盖 Object.hashCode()：基于字符串内容计算哈希码（native，代码生成器特化处理）
        string_class.add_method(MethodInfo {
            name: "hashCode".to_string(),
            class_name: "String".to_string(),
            type_params: Vec::new(),
            params: Vec::new(),
            return_type: Type::Int32,
            is_static: false,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: true,
            is_abstract: false,
            is_final: true,
            is_override: true,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // 覆盖 Object.equals(Object other)
        string_class.add_method(MethodInfo {
            name: "equals".to_string(),
            class_name: "String".to_string(),
            type_params: Vec::new(),
            params: vec![ParameterInfo {
                name: "other".to_string(),
                param_type: Type::Object("Object".to_string()),
                is_varargs: false,
            }],
            return_type: Type::Bool,
            is_static: false,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: true,
            is_abstract: false,
            is_final: true,
            is_override: true,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // 添加 String.valueOf() 方法（各种重载版本）
        // valueOf(int)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
            type_params: Vec::new(),
            params: vec![ParameterInfo {
                name: "value".to_string(),
                param_type: Type::Int32,
                is_varargs: false,
            }],
            return_type: Type::String,
            is_static: true,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: false,
            is_abstract: false,
            is_final: true,
            is_override: false,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // valueOf(long)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
            type_params: Vec::new(),
            params: vec![ParameterInfo {
                name: "value".to_string(),
                param_type: Type::Int64,
                is_varargs: false,
            }],
            return_type: Type::String,
            is_static: true,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: false,
            is_abstract: false,
            is_final: true,
            is_override: false,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // valueOf(float)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
            type_params: Vec::new(),
            params: vec![ParameterInfo {
                name: "value".to_string(),
                param_type: Type::Float32,
                is_varargs: false,
            }],
            return_type: Type::String,
            is_static: true,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: false,
            is_abstract: false,
            is_final: true,
            is_override: false,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // valueOf(double)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
            type_params: Vec::new(),
            params: vec![ParameterInfo {
                name: "value".to_string(),
                param_type: Type::Float64,
                is_varargs: false,
            }],
            return_type: Type::String,
            is_static: true,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: false,
            is_abstract: false,
            is_final: true,
            is_override: false,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // valueOf(boolean)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
            type_params: Vec::new(),
            params: vec![ParameterInfo {
                name: "value".to_string(),
                param_type: Type::Bool,
                is_varargs: false,
            }],
            return_type: Type::String,
            is_static: true,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: false,
            is_abstract: false,
            is_final: true,
            is_override: false,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // valueOf(char)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
            type_params: Vec::new(),
            params: vec![ParameterInfo {
                name: "value".to_string(),
                param_type: Type::Char,
                is_varargs: false,
            }],
            return_type: Type::String,
            is_static: true,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: false,
            is_abstract: false,
            is_final: true,
            is_override: false,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // valueOf(String) - 返回自身
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
            type_params: Vec::new(),
            params: vec![ParameterInfo {
                name: "value".to_string(),
                param_type: Type::String,
                is_varargs: false,
            }],
            return_type: Type::String,
            is_static: true,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: false,
            is_abstract: false,
            is_final: true,
            is_override: false,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // 注册 String 类
        self.classes.insert("String".to_string(), string_class);
    }

    /// 注册内置 Integer 类
    fn register_builtin_integer_class(&mut self) {
        // 创建 Integer 类信息
        let mut integer_class = ClassInfo {
            name: "Integer".to_string(),
            type_params: Vec::new(),
            methods: HashMap::new(),
            fields: HashMap::new(),
            constructors: Vec::new(),
            has_destructor: false,
            parent: Some("Object".to_string()), // Integer 继承 Object
            interfaces: Vec::new(),
            is_abstract: false,
            is_final: true, // Integer 是 final 类，不能被继承
            is_interop: false,
            is_stack_only: false,
            vtable_layout: None,
        };

        // 添加 Integer.parseInt(String) 方法
        integer_class.add_method(MethodInfo {
            name: "parseInt".to_string(),
            class_name: "Integer".to_string(),
            type_params: Vec::new(),
            params: vec![ParameterInfo {
                name: "s".to_string(),
                param_type: Type::String,
                is_varargs: false,
            }],
            return_type: Type::Int32,
            is_static: true,
            is_public: true,
            is_private: false,
            is_protected: false,
            is_native: false,
            is_abstract: false,
            is_final: true,
            is_override: false,
            is_test: false,
            vtable_slot: None,
            loc: crate::miette_diagnostic::SourceLocation::default(),
        });

        // 注册 Integer 类
        self.classes.insert("Integer".to_string(), integer_class);
    }

    pub fn register_class(
        &mut self,
        class_info: ClassInfo,
        file: Option<String>,
        line: usize,
        column: usize,
    ) -> crate::miette_diagnostic::CayResult<()> {
        let name = class_info.name.clone();
        if self.classes.contains_key(&name) {
            return Err(crate::miette_diagnostic::CayError::DuplicateDefinition {
                error_code: ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
                file,
                line,
                column,
                name: name.clone(),
                suggestion: format!("'{}' 已被定义，请使用不同的名称", name),
            });
        }
        self.classes.insert(name, class_info);
        Ok(())
    }

    pub fn register_interface(
        &mut self,
        interface_info: InterfaceInfo,
        file: Option<String>,
        line: usize,
        column: usize,
    ) -> crate::miette_diagnostic::CayResult<()> {
        let name = interface_info.name.clone();
        if self.interfaces.contains_key(&name) {
            return Err(crate::miette_diagnostic::CayError::DuplicateDefinition {
                error_code: ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
                file,
                line,
                column,
                name: name.clone(),
                suggestion: format!("'{}' 已被定义，请使用不同的名称", name),
            });
        }
        self.interfaces.insert(name, interface_info);
        Ok(())
    }

    pub fn build_method_signature(method_name: &str, params: &[ParameterInfo]) -> String {
        let param_types: Vec<String> = params
            .iter()
            .map(|p| format!("{:?}", p.param_type))
            .collect();
        if param_types.is_empty() {
            method_name.to_string()
        } else {
            format!("{}({})", method_name, param_types.join(","))
        }
    }

    pub fn build_method_signature_from_types(method_name: &str, arg_types: &[Type]) -> String {
        let param_types: Vec<String> = arg_types.iter().map(|t| format!("{:?}", t)).collect();
        if param_types.is_empty() {
            method_name.to_string()
        } else {
            format!("{}({})", method_name, param_types.join(","))
        }
    }

    pub fn build_interface_vtable_key(interface_name: &str, method_sig: &str) -> String {
        format!("$iface${}${}", interface_name, method_sig)
    }

    /// 构造带接口类型实参的 vtable 槽位键。
    ///
    /// 用于区分同一泛型接口的不同实例化（如 `Into<IOError>` 与 `Into<ParseError>`），
    /// 它们各自提供一个仅返回类型不同的 `into()` 重载——vtable 必须为每个实例化
    /// 分配独立槽位，否则动态分派只会命中其一。
    ///
    /// 当 `type_args` 为空时退化为 [`build_interface_vtable_key`] 的形式，保持
    /// 非泛型接口的既有布局与 ABI 不变。
    pub fn build_interface_vtable_key_with_type_args(
        interface_name: &str,
        type_args: &[Type],
        method_sig: &str,
    ) -> String {
        if type_args.is_empty() {
            Self::build_interface_vtable_key(interface_name, method_sig)
        } else {
            let args_str: Vec<String> = type_args.iter().map(|t| t.display_name()).collect();
            // 用 `$` 作为分隔符（与既有键格式一致），格式：
            //   $iface$Into<IOError>$into
            // 去命名空间后的裸名也保留，便于 attach 阶段反查。
            format!(
                "$iface${}<{}>${}",
                interface_name,
                args_str.join(","),
                method_sig
            )
        }
    }

    pub fn interface_vtable_key_method_signature(slot_key: &str) -> Option<&str> {
        let rest = slot_key.strip_prefix("$iface$")?;
        let (_, method_sig) = rest.split_once('$')?;
        Some(method_sig)
    }

    /// 解析接口 vtable 槽位键，提取基础接口名与类型实参列表。
    ///
    /// 槽位键格式（由 [`build_interface_vtable_key_with_type_args`] 构造）：
    /// - 非泛型接口：`$iface$InterfaceName$method_sig`
    /// - 泛型接口实例化：`$iface$InterfaceName<TypeArg1,TypeArg2>$method_sig`
    ///
    /// 返回 `(基础接口名, 类型实参列表)`。非泛型接口返回空向量。
    /// 若键不是接口槽位键（不含 `$iface$` 前缀），返回 `None`。
    ///
    /// 时间复杂度 O(n)，n 为键长度；空间复杂度 O(k)，k 为类型实参数量。
    pub fn parse_interface_slot_key_type_args(slot_key: &str) -> Option<(&str, Vec<Type>)> {
        let rest = slot_key.strip_prefix("$iface$")?;
        // 接口部分可能包含 `<...>`（其内可能有逗号），但类型名不含 `$`，
        // 故按第一个 `$` 拆分即可分离接口部分与方法签名。
        let (interface_part, _method_sig) = rest.split_once('$')?;

        if let Some(pos) = interface_part.find('<') {
            let interface_name = &interface_part[..pos];
            let end = interface_part.rfind('>').unwrap_or(interface_part.len());
            if end <= pos + 1 {
                return Some((interface_name, Vec::new()));
            }
            let args_str = &interface_part[pos + 1..end];
            let type_args: Vec<Type> = args_str
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| Type::Object(s.to_string()))
                .collect();
            Some((interface_name, type_args))
        } else {
            Some((interface_part, Vec::new()))
        }
    }

    /// 查找接口方法在 vtable 中的槽位编号。
    ///
    /// 当 `interface_type_args` 非空时，按泛型接口的特化实例查找独立槽位
    /// （如 `Into<IOError>::into` 与 `Into<ParseError>::into` 各占一槽）；
    /// 为空时退化为按裸接口名查找，保持非泛型接口的既有行为。
    pub fn get_interface_vtable_slot(
        &self,
        interface_name: &str,
        method_name: &str,
        arg_types: &[Type],
        interface_type_args: &[Type],
    ) -> Option<usize> {
        // 接口 vtable 槽位注册时使用的是基础接口名（无泛型实参），
        // 调用处可能传入特化名如 Iterator<String>，需还原为基础名。
        let base_interface_name = interface_name
            .split('<')
            .next()
            .unwrap_or(interface_name)
            .trim_end();
        let method_sig = Self::build_method_signature_from_types(method_name, arg_types);
        let key = if interface_type_args.is_empty() {
            Self::build_interface_vtable_key(base_interface_name, &method_sig)
        } else {
            Self::build_interface_vtable_key_with_type_args(
                base_interface_name,
                interface_type_args,
                &method_sig,
            )
        };
        // 先按特化键查找；若失败，回退到裸键查找（兼容未特化的注册路径）
        if let Some(slot) = self.interface_vtable_slots.get(&key).copied() {
            return Some(slot);
        }
        if !interface_type_args.is_empty() {
            let fallback_key = Self::build_interface_vtable_key(base_interface_name, &method_sig);
            return self.interface_vtable_slots.get(&fallback_key).copied();
        }
        None
    }

    /// 注册 struct（值类型）
    pub fn register_struct(
        &mut self,
        struct_info: StructInfo,
        file: Option<String>,
        line: usize,
        column: usize,
    ) -> crate::miette_diagnostic::CayResult<()> {
        let name = struct_info.name.clone();
        if self.structs.contains_key(&name) || self.classes.contains_key(&name) {
            return Err(crate::miette_diagnostic::CayError::DuplicateDefinition {
                error_code: ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
                file,
                line,
                column,
                name: name.clone(),
                suggestion: format!("'{}' 已被定义为类或 struct，请使用不同的名称", name),
            });
        }
        self.structs.insert(name, struct_info);
        Ok(())
    }

    /// 注册 enum（tagged union / ADT）
    pub fn register_enum(
        &mut self,
        enum_info: EnumInfo,
        file: Option<String>,
        line: usize,
        column: usize,
    ) -> crate::miette_diagnostic::CayResult<()> {
        let name = enum_info.name.clone();
        if self.enums.contains_key(&name)
            || self.classes.contains_key(&name)
            || self.structs.contains_key(&name)
        {
            return Err(crate::miette_diagnostic::CayError::DuplicateDefinition {
                error_code: ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
                file,
                line,
                column,
                name: name.clone(),
                suggestion: format!("'{}' 已被定义为类/struct/enum，请使用不同的名称", name),
            });
        }
        self.enums.insert(name, enum_info);
        Ok(())
    }

    /// 获取 struct 信息
    pub fn get_struct(&self, name: &str) -> Option<&StructInfo> {
        self.structs.get(name)
    }

    /// 获取 enum 信息
    pub fn get_enum(&self, name: &str) -> Option<&EnumInfo> {
        self.enums.get(name)
    }

    /// 注册 @FreeFunction 导出函数
    /// 如果已有同名函数且来自不同的类，返回冲突错误
    pub fn register_free_function(
        &mut self,
        func_name: &str,
        class_name: &str,
        method_info: MethodInfo,
        loc: crate::miette_diagnostic::SourceLocation,
    ) -> crate::miette_diagnostic::CayResult<()> {
        if let Some((existing_class, _, existing_loc)) = self.free_functions.get(func_name) {
            if existing_class != class_name {
                return Err(crate::miette_diagnostic::CayError::DuplicateDefinition {
                    error_code: ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
                    file: loc.file.clone(),
                    line: loc.line,
                    column: loc.column,
                    name: func_name.to_string(),
                    suggestion: format!(
                        "@FreeFunction 函数 '{}' 已在类 '{}' ({}:{}) 中定义，类 '{}' 中的同名 @FreeFunction 方法冲突。请使用不同的函数名。",
                        func_name,
                        existing_class,
                        existing_loc.line,
                        existing_loc.column,
                        class_name
                    ),
                });
            }
        }
        self.free_functions.insert(
            func_name.to_string(),
            (class_name.to_string(), method_info, loc),
        );
        Ok(())
    }

    pub fn get_interface(&self, name: &str) -> Option<&InterfaceInfo> {
        self.interfaces.get(name)
    }

    pub fn interface_exists(&self, name: &str) -> bool {
        self.interfaces.contains_key(name)
    }

    pub fn get_class(&self, name: &str) -> Option<&ClassInfo> {
        // 直接查找（适用于全局类或限定名如 "std::StringBuilder"）
        if let Some(class) = self.classes.get(name) {
            return Some(class);
        }
        // 在当前命名空间上下文中查找（优先于 using 别名，
        // 避免 include 文件内部的类型被外层 using 错误覆盖）
        if !self.current_namespace.is_empty() {
            let qualified = format!("{}::{}", self.current_namespace.join("::"), name);
            if let Some(class) = self.classes.get(&qualified) {
                return Some(class);
            }
        }
        // 尝试命名空间别名（using 声明）
        if let Some(qualified) = self.namespace_aliases.get(name) {
            return self.classes.get(qualified);
        }
        None
    }

    /// 获取类的可变引用
    pub fn get_class_mut(&mut self, name: &str) -> Option<&mut ClassInfo> {
        if self.classes.contains_key(name) {
            return self.classes.get_mut(name);
        }
        // 在当前命名空间上下文中查找（优先于 using 别名）
        if !self.current_namespace.is_empty() {
            let qualified = format!("{}::{}", self.current_namespace.join("::"), name);
            if self.classes.contains_key(&qualified) {
                return self.classes.get_mut(&qualified);
            }
        }
        // 尝试命名空间别名（using 声明）
        if let Some(qualified) = self.namespace_aliases.get(name).cloned() {
            return self.classes.get_mut(&qualified);
        }
        None
    }

    /// 更新指定类方法的返回类型（用于 fn 自动推断）
    pub fn update_method_return_type(
        &mut self,
        class_name: &str,
        method_name: &str,
        params: &[ParameterInfo],
        new_return_type: Type,
    ) -> bool {
        if let Some(class_info) = self.get_class_mut(class_name) {
            class_info.update_method_return_type(method_name, params, new_return_type)
        } else {
            false
        }
    }

    /// 注册命名空间别名（用于 using 声明）
    pub fn add_namespace_alias(&mut self, simple_name: String, qualified_name: String) {
        self.namespace_aliases.insert(simple_name, qualified_name);
    }

    /// 记录类的命名空间路径
    pub fn set_class_namespace(&mut self, qualified_name: &str, namespace_path: Vec<String>) {
        self.class_namespace_paths
            .insert(qualified_name.to_string(), namespace_path);
    }

    /// 在命名空间上下文中解析类名
    /// 1. 先查找 using 别名
    /// 2. 再在当前命名空间上下文中查找 (context_ns::name)
    /// 3. 最后查找简单名（用于全局类）
    pub fn resolve_class(&self, name: &str, context_ns: &[String]) -> Option<&ClassInfo> {
        // 1. using 别名
        if let Some(qualified) = self.namespace_aliases.get(name) {
            if let Some(cls) = self.classes.get(qualified) {
                return Some(cls);
            }
        }
        // 2. 当前命名空间上下文: context_ns::name
        if !context_ns.is_empty() {
            let qualified = format!("{}::{}", context_ns.join("::"), name);
            if let Some(cls) = self.classes.get(&qualified) {
                return Some(cls);
            }
        }
        // 3. 全局简单名（无命名空间的类）
        // 只有该类确实没有命名空间路径时才返回
        if let Some(cls) = self.classes.get(name) {
            // 检查这个类是否有命名空间路径
            // 有关联命名空间的类不应该通过简单名访问（除非通过上面的上下文查找）
            if !self.class_namespace_paths.contains_key(name) {
                return Some(cls);
            }
        }
        None
    }

    /// 获取类的可变引用（带命名空间上下文）
    pub fn resolve_class_mut(
        &mut self,
        name: &str,
        context_ns: &[String],
    ) -> Option<&mut ClassInfo> {
        // 1. using 别名
        if let Some(qualified) = self.namespace_aliases.get(name).cloned() {
            if self.classes.contains_key(&qualified) {
                return self.classes.get_mut(&qualified);
            }
        }
        // 2. 当前命名空间上下文
        if !context_ns.is_empty() {
            let qualified = format!("{}::{}", context_ns.join("::"), name);
            if self.classes.contains_key(&qualified) {
                return self.classes.get_mut(&qualified);
            }
        }
        // 3. 全局简单名
        if self.classes.contains_key(name) && !self.class_namespace_paths.contains_key(name) {
            return self.classes.get_mut(name);
        }
        None
    }

    /// 根据类名和方法名获取方法（获取第一个匹配的方法，用于无参数类型信息的情况，支持继承和接口）
    pub fn get_method(&self, class_name: &str, method_name: &str) -> Option<&MethodInfo> {
        if let Some(class_info) = self.get_class(class_name) {
            if let Some(method) = class_info.find_method_by_name(method_name) {
                return Some(method);
            }
            // 如果在当前类中没找到，递归在父类中查找
            if let Some(ref parent_name) = class_info.parent {
                return self.get_method(parent_name, method_name);
            }
        }
        // 检查接口方法
        if let Some(interface_info) = self.get_interface(class_name) {
            if let Some(method) = interface_info.methods.get(method_name) {
                return Some(method);
            }
        }
        None
    }

    /// 根据类名、方法名和参数类型查找方法（支持重载、继承和接口）
    /// 支持泛型类名，如 "Wrapper<int>" 会被解析为 "Wrapper"
    pub fn find_method(
        &self,
        class_name: &str,
        method_name: &str,
        arg_types: &[Type],
    ) -> Option<&MethodInfo> {
        // 解析泛型类名: "Wrapper<int>" -> "Wrapper"
        // 支持多类型参数: "Pair<int, String>" -> "Pair"
        let base_class_name = if let Some(pos) = class_name.find('<') {
            &class_name[..pos]
        } else {
            class_name
        };

        // 首先在当前类中查找
        if let Some(class_info) = self.get_class(base_class_name) {
            if let Some(method) = self.find_matching_method(class_info, method_name, arg_types) {
                return Some(method);
            }
            // 如果在当前类中没找到，递归在父类中查找
            if let Some(ref parent_name) = class_info.parent {
                return self.find_method(parent_name, method_name, arg_types);
            }
        }
        // 检查接口方法（接口方法没有重载，直接匹配方法名）
        if let Some(interface_info) = self.get_interface(base_class_name) {
            if let Some(method) = interface_info.methods.get(method_name) {
                return Some(method);
            }
        }
        // 检查 struct 方法
        if let Some(struct_info) = self.get_struct(base_class_name) {
            if let Some(method) =
                self.find_matching_struct_method(struct_info, method_name, arg_types)
            {
                return Some(method);
            }
        }
        None
    }

    /// 根据类名、方法名和参数类型查找方法，只在当前类中查找（不递归父类）
    pub fn find_method_in_class(
        &self,
        class_name: &str,
        method_name: &str,
        arg_types: &[Type],
    ) -> Option<&MethodInfo> {
        self.get_class(class_name)
            .and_then(|c| c.find_method(method_name, arg_types))
    }

    /// 检查两个类型是否兼容（考虑命名空间前缀）
    /// 例如 Object("JsonValue") 和 Object("json::JsonValue") 被认为是兼容的
    ///
    /// # 参数顺序
    /// `types_compatible_with_namespace(param_type, arg_type)`：形参（目标）类型在前、
    /// 实参（来源）类型在后。与统一规则源 `types_compatible(from, to)` 的顺序相反。
    /// 本函数只做结构/命名空间层面的判定，数值提升与 FFI 互通规则在
    /// `types_compatible` 中，需要完整规则时请走 `types_compatible`。
    pub fn types_compatible_with_namespace(&self, param_type: &Type, arg_type: &Type) -> bool {
        if param_type == arg_type {
            return true;
        }

        if matches!(
            (param_type, arg_type),
            (Type::GenericParam(_), _) | (_, Type::GenericParam(_))
        ) {
            return true;
        }

        match (param_type, arg_type) {
            (Type::Object(param_name), Type::Object(arg_name)) => {
                self.class_names_compatible(param_name, arg_name)
            }
            (Type::Generic(param_name, param_args), Type::Generic(arg_name, arg_args)) => {
                self.class_names_compatible(param_name, arg_name)
                    && param_args.len() == arg_args.len()
                    && param_args
                        .iter()
                        .zip(arg_args.iter())
                        .all(|(p, a)| self.types_compatible_with_namespace(p, a))
            }
            (Type::Generic(param_name, param_args), Type::Object(arg_name)) => {
                self.class_names_compatible(param_name, arg_name)
                    && Self::generic_arg_count_in_name(arg_name)
                        .map(|count| count == param_args.len())
                        .unwrap_or(true)
            }
            (Type::Object(param_name), Type::Generic(arg_name, _)) => {
                self.class_names_compatible(param_name, arg_name)
            }
            (Type::Array(param_elem), Type::Array(arg_elem)) => {
                self.types_compatible_with_namespace(param_elem, arg_elem)
            }
            (Type::Pointer(param_inner), Type::Pointer(arg_inner)) => {
                matches!(param_inner.as_ref(), Type::CVoid)
                    || matches!(arg_inner.as_ref(), Type::CVoid)
                    || self.types_compatible_with_namespace(param_inner, arg_inner)
            }
            (Type::Function(param_fn), Type::Function(arg_fn)) => {
                param_fn.params.len() == arg_fn.params.len()
                    && self
                        .types_compatible_with_namespace(&param_fn.return_type, &arg_fn.return_type)
                    && param_fn
                        .params
                        .iter()
                        .zip(arg_fn.params.iter())
                        .all(|(p, a)| self.types_compatible_with_namespace(p, a))
            }
            _ => false,
        }
    }

    /// 检查类型兼容性（全编译器唯一的规则源）
    ///
    /// 验证源类型是否可以赋值给目标类型。
    /// 对于引用类型（Object），检查继承关系：子类可以赋值给父类。
    ///
    /// # 参数顺序
    /// `types_compatible(from, to)`：源类型在前、目标类型在后，
    /// 语义为「from 是否可以赋值/隐式转换为 to」。
    /// 注意与 `types_compatible_with_namespace(param, arg)` 的
    /// （形参, 实参）顺序相反，调用处换算为 (to, from)。
    ///
    /// 其它入口（SemanticAnalyzer::types_compatible、is_valid_cast、
    /// types_match_with_namespace）均为本函数的薄封装，不再各自维护规则表。
    pub fn types_compatible(&self, from: &Type, to: &Type) -> bool {
        if from == to {
            return true;
        }

        // 泛型参数类型可以匹配任何类型
        if matches!(to, Type::GenericParam(_)) {
            return true;
        }

        // null 字面量（内部标记类型）可以赋值给任何引用类型（包括 String 和指针），
        // 但不能赋值给值类型（int/bool/struct 等）。
        // 注意：真正的 Object 实例不再享受此待遇，Object 实例赋给 String/无关类会报错。
        if from.is_null_literal() {
            return to.is_reference_type() || matches!(to, Type::Pointer(_));
        }

        if self.types_compatible_with_namespace(to, from) {
            return true;
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
            (Type::Generic(from_name, _), Type::Object(to_name)) => {
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
            (Type::Object(from_name), Type::Generic(to_name, _)) => {
                // Object 与 Generic 混合时，提取基础名后检查子类型关系。
                // 例如 Object("ArrayListIterator<T>") 可赋值给 Generic("Iterator", [T])。
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
                if from_base == to_base {
                    return true;
                }
                self.is_subtype_of(from_base, to_base)
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
                let ret_compatible =
                    self.types_compatible(&from_fn.return_type, &to_fn.return_type);
                // 检查参数数量是否相同
                let params_count_match = from_fn.params.len() == to_fn.params.len();
                // 检查每个参数类型是否兼容（允许协变/逆变）
                let params_compatible =
                    if params_count_match {
                        from_fn.params.iter().zip(to_fn.params.iter()).all(
                            |(from_param, to_param)| {
                                self.types_compatible(from_param, to_param)
                                    || self.types_compatible(to_param, from_param)
                            },
                        )
                    } else {
                        false
                    };
                ret_compatible && params_count_match && params_compatible
            }
            _ => {
                // 兜底：检查接口子类型关系。例如实现类 ArrayListIterator<T>
                // 可以赋值给接口类型 Iterator<T>。
                if let (Some(from_name), Some(to_name)) = (type_base_name(from), type_base_name(to))
                {
                    if self.interface_exists(&to_name) && self.is_subtype_of(&from_name, &to_name)
                    {
                        return true;
                    }
                }
                false
            }
        }
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
            return self.class_exists(subtype) || subtype == "String" || subtype == "Function";
        }

        // 检查 subtype 是否实现了 supertype 接口
        if self.interface_exists(supertype) {
            if let Some(class_info) = self.get_class(subtype) {
                if class_info
                    .interfaces
                    .iter()
                    .any(|i| interface_bare_name(i) == supertype)
                {
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

            if let Some(class_info) = self.get_class(&current) {
                // 检查当前类是否实现了目标接口
                if self.interface_exists(supertype) {
                    if class_info
                        .interfaces
                        .iter()
                        .any(|i| interface_bare_name(i) == supertype)
                    {
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
        let simple1 = name1
            .rfind("::")
            .map(|pos| &name1[pos + 2..])
            .unwrap_or(name1);
        let simple2 = name2
            .rfind("::")
            .map(|pos| &name2[pos + 2..])
            .unwrap_or(name2);

        // 如果简单类名不同，直接返回 false
        if simple1 != simple2 {
            return false;
        }

        // 尝试直接查找两个类名（不依赖 current_namespace）
        let class1 = self.classes.get(name1);
        let class2 = self.classes.get(name2);

        match (class1, class2) {
            (Some(c1), Some(c2)) => {
                // 两个类都找到了，检查它们是否是同一个类（通过名称比较）
                c1.name == c2.name
            }
            (Some(c1), None) => {
                // 只有 name1 找到了，检查 name2 是否是 name1 的简单名形式
                // 例如 name1="json::JsonValue", name2="JsonValue"
                c1.name
                    .rfind("::")
                    .map(|pos| &c1.name[pos + 2..])
                    .unwrap_or(&c1.name)
                    == simple2
            }
            (None, Some(c2)) => {
                // 只有 name2 找到了，检查 name1 是否是 name2 的简单名形式
                c2.name
                    .rfind("::")
                    .map(|pos| &c2.name[pos + 2..])
                    .unwrap_or(&c2.name)
                    == simple1
            }
            (None, None) => {
                // 两个都没直接找到，说明这两个类名可能都不存在
                // 这种情况下不能假设它们是同一个类
                false
            }
        }
    }

    fn find_matching_method<'a>(
        &self,
        class_info: &'a ClassInfo,
        method_name: &str,
        arg_types: &[Type],
    ) -> Option<&'a MethodInfo> {
        let methods = class_info.methods.get(method_name)?;
        self.find_matching_method_in_list(methods, arg_types)
    }

    fn find_matching_struct_method<'a>(
        &self,
        struct_info: &'a StructInfo,
        method_name: &str,
        arg_types: &[Type],
    ) -> Option<&'a MethodInfo> {
        let methods = struct_info.methods.get(method_name)?;
        self.find_matching_method_in_list(methods, arg_types)
    }

    fn find_matching_method_in_list<'a>(
        &self,
        methods: &'a [MethodInfo],
        arg_types: &[Type],
    ) -> Option<&'a MethodInfo> {
        for method in methods {
            if self.match_method_params_exact(&method.params, arg_types) {
                return Some(method);
            }
        }

        methods
            .iter()
            .find(|method| self.match_method_params(&method.params, arg_types))
    }

    fn match_method_params_exact(&self, params: &[ParameterInfo], arg_types: &[Type]) -> bool {
        self.match_method_params_impl(params, arg_types, true)
    }

    fn match_method_params(&self, params: &[ParameterInfo], arg_types: &[Type]) -> bool {
        self.match_method_params_impl(params, arg_types, false)
    }

    fn match_method_params_impl(
        &self,
        params: &[ParameterInfo],
        arg_types: &[Type],
        exact: bool,
    ) -> bool {
        if params.is_empty() {
            return arg_types.is_empty();
        }

        let type_matches = |registry: &TypeRegistry, param: &Type, arg: &Type| {
            if exact {
                registry.types_match_exact_with_namespace(param, arg)
            } else {
                registry.types_match_with_namespace(param, arg)
            }
        };

        let varargs_idx = params.iter().position(|p| p.is_varargs);

        if let Some(vi) = varargs_idx {
            let fixed_before = vi;
            let fixed_after = params.len() - vi - 1;
            let min_args = fixed_before + fixed_after;

            if arg_types.len() < min_args {
                return false;
            }

            for i in 0..fixed_before {
                if !type_matches(self, &params[i].param_type, &arg_types[i]) {
                    return false;
                }
            }

            let vararg_elem_type = match &params[vi].param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => &params[vi].param_type,
            };
            let varargs_len = arg_types.len() - min_args;
            let varargs_end = fixed_before + varargs_len;

            if varargs_len == 1
                && type_matches(self, &params[vi].param_type, &arg_types[fixed_before])
            {
                // 直接传递数组给可变参数
            } else {
                for i in fixed_before..varargs_end {
                    if !type_matches(self, vararg_elem_type, &arg_types[i]) {
                        return false;
                    }
                }
            }

            for i in 0..fixed_after {
                if !type_matches(
                    self,
                    &params[vi + 1 + i].param_type,
                    &arg_types[varargs_end + i],
                ) {
                    return false;
                }
            }

            true
        } else {
            params.len() == arg_types.len()
                && params
                    .iter()
                    .zip(arg_types.iter())
                    .all(|(p, a)| type_matches(self, &p.param_type, a))
        }
    }

    fn types_match_exact_with_namespace(&self, param_type: &Type, arg_type: &Type) -> bool {
        if param_type == arg_type {
            return true;
        }

        if matches!(
            (param_type, arg_type),
            (Type::GenericParam(_), _) | (_, Type::GenericParam(_))
        ) {
            return true;
        }

        match (param_type, arg_type) {
            (Type::Generic(param_name, param_args), Type::Generic(arg_name, arg_args)) => {
                self.class_names_compatible(param_name, arg_name)
                    && param_args.len() == arg_args.len()
                    && param_args
                        .iter()
                        .zip(arg_args.iter())
                        .all(|(p, a)| self.types_match_exact_with_namespace(p, a))
            }
            (Type::Generic(param_name, param_args), Type::Object(arg_name)) => {
                self.class_names_compatible(param_name, arg_name)
                    && Self::generic_arg_count_in_name(arg_name)
                        .map(|count| count == param_args.len())
                        .unwrap_or(true)
            }
            (Type::Object(param_name), Type::Generic(arg_name, _)) => {
                self.class_names_compatible(param_name, arg_name)
            }
            (Type::Object(param_name), Type::Object(arg_name)) => {
                self.class_names_compatible(param_name, arg_name)
            }
            (Type::Array(param_elem), Type::Array(arg_elem)) => {
                self.types_match_exact_with_namespace(param_elem, arg_elem)
            }
            (Type::Pointer(param_inner), Type::Pointer(arg_inner)) => {
                self.types_match_exact_with_namespace(param_inner, arg_inner)
            }
            _ => false,
        }
    }

    /// 方法重载匹配用的兼容判定（薄入口，非独立规则表）
    ///
    /// # 参数顺序
    /// `types_match_with_namespace(param_type, arg_type)`：形参类型在前、实参类型在后。
    /// 委托统一规则源 `types_compatible(from, to)` 时换算为
    /// (from = arg_type, to = param_type)，请勿传反。
    fn types_match_with_namespace(&self, param_type: &Type, arg_type: &Type) -> bool {
        // 统一规则源：实参类型可以赋值给形参类型即可匹配
        if self.types_compatible(arg_type, param_type) {
            return true;
        }

        // 方法匹配特有的附加规则（不属于通用赋值兼容）：
        match (param_type, arg_type) {
            // Object 实例（多为 null 占位类型）可以传给任意指针形参
            (Type::Pointer(_), Type::Object(obj_name)) if obj_name == "Object" => true,
            _ => false,
        }
    }

    fn class_names_compatible(&self, param_name: &str, arg_name: &str) -> bool {
        let param_base = Self::generic_base_name(param_name);
        let arg_base = Self::generic_base_name(arg_name);

        if param_base == arg_base {
            return true;
        }

        let param_class = self.get_class(param_base);
        let arg_class = self.get_class(arg_base);

        match (param_class, arg_class) {
            (Some(p), Some(a)) => p.name == a.name,
            (Some(p), None) => Self::simple_name(&p.name) == Self::simple_name(arg_base),
            (None, Some(a)) => Self::simple_name(param_base) == Self::simple_name(&a.name),
            // 两个类名都无法解析到注册信息时，不能仅凭简单名相同判定兼容：
            // 不同命名空间下的同名类并不是同一个类，此处保守判不兼容。
            (None, None) => false,
        }
    }

    fn simple_name(name: &str) -> &str {
        name.rfind("::").map(|pos| &name[pos + 2..]).unwrap_or(name)
    }

    fn generic_base_name(name: &str) -> &str {
        name.find('<').map(|pos| &name[..pos]).unwrap_or(name)
    }

    fn generic_arg_count_in_name(name: &str) -> Option<usize> {
        let start = name.find('<')?;
        if !name.ends_with('>') {
            return None;
        }

        let args = &name[start + 1..name.len() - 1];
        let mut count = 0usize;
        let mut depth = 0usize;
        let mut has_current = false;

        for ch in args.chars() {
            match ch {
                '<' => {
                    depth += 1;
                    has_current = true;
                }
                '>' => {
                    depth = depth.saturating_sub(1);
                    has_current = true;
                }
                ',' if depth == 0 => {
                    if has_current {
                        count += 1;
                    }
                    has_current = false;
                }
                c if !c.is_whitespace() => has_current = true,
                _ => {}
            }
        }

        if has_current {
            count += 1;
        }

        Some(count)
    }

    /// 根据简单名查找命名空间限定名（如 "HttpHeaders" → "http::HttpHeaders"）
    /// 优先匹配当前命名空间
    pub fn find_qualified_class(&self, simple_name: &str) -> Option<String> {
        // 首先检查 using 别名
        if let Some(qualified) = self.namespace_aliases.get(simple_name) {
            return Some(qualified.clone());
        }
        // 如果有当前命名空间，优先检查
        if !self.current_namespace.is_empty() {
            let preferred = format!("{}::{}", self.current_namespace.join("::"), simple_name);
            if self.classes.contains_key(&preferred) {
                return Some(preferred);
            }
        }
        // 注意：不在全局回退查找其他命名空间中的类
        // 必须通过 using 声明或限定名显式引用
        None
    }

    pub fn class_exists(&self, name: &str) -> bool {
        self.get_class(name).is_some()
    }

    /// 查找实现指定接口且拥有指定方法的类
    /// 用于代码生成阶段：当变量类型是接口时，找到实际实现该方法的类
    pub fn find_implementing_class_for_method(
        &self,
        interface_name: &str,
        method_name: &str,
    ) -> Option<&ClassInfo> {
        // 首先确认这是一个接口
        if !self.interfaces.contains_key(interface_name) {
            return None;
        }
        // 查找实现了该接口且拥有该方法的类。
        // HashMap 遍历顺序不确定，先收集候选再按类名排序，
        // 保证结果确定性；存在多个实现类时按类名字典序取第一个。
        let mut candidates: Vec<&ClassInfo> = self
            .classes
            .values()
            .filter(|class_info| {
                class_info.interfaces.iter().any(|i| {
                    let bare_name = match i {
                        Type::Object(name) | Type::Generic(name, _) => {
                            name.split('<').next().unwrap_or(name)
                        }
                        _ => &format!("{}", i),
                    };
                    bare_name == interface_name
                }) && class_info.methods.contains_key(method_name)
            })
            .collect();
        candidates.sort_by(|a, b| a.name.cmp(&b.name));
        candidates.into_iter().next()
    }

    /// 根据简单名查找枚举的命名空间限定名（如 "JsonType" → "json::JsonType"）
    /// 优先匹配当前命名空间
    pub fn find_qualified_enum(&self, simple_name: &str) -> Option<String> {
        // 如果已经是限定名，直接返回
        if simple_name.contains("::") {
            return self.enums.get(simple_name).map(|_| simple_name.to_string());
        }
        // 如果有当前命名空间，优先检查
        if !self.current_namespace.is_empty() {
            let preferred = format!("{}::{}", self.current_namespace.join("::"), simple_name);
            if self.enums.contains_key(&preferred) {
                return Some(preferred);
            }
        }
        // 回退：遍历查找
        for qname in self.enums.keys() {
            if qname.ends_with(&format!("::{}", simple_name)) {
                return Some(qname.clone());
            }
        }
        None
    }

    /// 获取枚举信息，支持简单名和限定名查找
    pub fn get_enum_by_name(&self, name: &str) -> Option<&EnumInfo> {
        // 首先尝试直接查找
        if let Some(info) = self.enums.get(name) {
            return Some(info);
        }
        // 尝试查找限定名
        if let Some(qualified_name) = self.find_qualified_enum(name) {
            return self.enums.get(&qualified_name);
        }
        None
    }
}

impl Default for TypeRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl InterfaceInfo {
    pub fn new(name: String) -> Self {
        Self {
            name,
            type_params: Vec::new(),
            methods: HashMap::new(),
        }
    }

    pub fn add_method(&mut self, method: MethodInfo) {
        self.methods.insert(method.name.clone(), method);
    }
}


/// 从接口类型中提取基础接口名（如 Iterator<T> -> Iterator）。
fn interface_bare_name(interface_type: &Type) -> &str {
    match interface_type {
        Type::Object(name) | Type::Generic(name, _) => name.split('<').next().unwrap_or(name),
        _ => "",
    }
}

/// 从引用类型中提取基础类型名（如 Iterator<T> -> Iterator）。
fn type_base_name(ty: &Type) -> Option<String> {
    match ty {
        Type::Object(name) | Type::Generic(name, _) => {
            name.split('<').next().map(|s| s.to_string())
        }
        _ => None,
    }
}
