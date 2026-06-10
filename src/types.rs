use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionType {
    pub params: Vec<Type>,
    pub return_type: Box<Type>,
    pub is_static: bool,
    pub is_closure: bool, // 是否是闭包（有捕获变量）
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub type_params: Vec<String>, // 泛型类型参数: <T, U, ...>
    pub methods: HashMap<String, Vec<MethodInfo>>, // 支持方法重载：同名方法可以有多个
    pub fields: HashMap<String, FieldInfo>,
    pub constructors: Vec<ConstructorInfo>, // 构造函数列表
    pub has_destructor: bool,               // 是否有析构函数
    pub parent: Option<String>,
    pub interfaces: Vec<String>,             // 实现的接口列表
    pub is_abstract: bool,                   // 是否是抽象类
    pub is_final: bool,                      // 是否是final类（禁止继承）
    pub vtable_layout: Option<VTableLayout>, // vtable 布局信息
}

/// 构造函数信息
#[derive(Debug, Clone)]
pub struct ConstructorInfo {
    pub params: Vec<ParameterInfo>,
    pub is_public: bool,
    pub is_private: bool,
    pub is_protected: bool,
}

#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    pub name: String,
    pub methods: HashMap<String, MethodInfo>,
}

/// struct 信息 - 值类型，无继承
#[derive(Debug, Clone)]
pub struct StructInfo {
    pub name: String,
    pub fields: HashMap<String, FieldInfo>,
    pub methods: HashMap<String, Vec<MethodInfo>>, // 支持方法重载
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
#[derive(Debug, Clone)]
pub struct EnumInfo {
    pub name: String,
    pub type_params: Vec<String>, // 泛型类型参数
    pub variants: Vec<EnumVariantInfo>,
    pub methods: HashMap<String, Vec<MethodInfo>>, // 支持方法重载
    pub is_public: bool,
}

/// enum variant 信息
#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct MethodInfo {
    pub name: String,
    pub class_name: String,
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
}

/// VTable 布局信息
#[derive(Debug, Clone)]
pub struct VTableLayout {
    /// 类名
    pub class_name: String,
    /// vtable 中的方法槽位列表（方法名 → 槽位编号）
    pub slots: HashMap<String, usize>,
    /// vtable 总大小（槽位数量）
    pub size: usize,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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
                unreachable!("Cannot get size of auto type - type inference not completed")
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

    pub fn is_integer(&self) -> bool {
        matches!(self, Type::Int32 | Type::Int64 | Type::CULong)
    }

    /// 检查是否是泛型相关类型
    pub fn is_generic(&self) -> bool {
        matches!(self, Type::GenericParam(_) | Type::Generic(_, _))
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
            Type::Object(name) => write!(f, "{}", name),
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

#[derive(Debug, Clone)]
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
    pub free_functions: HashMap<String, (String, MethodInfo, crate::error::SourceLocation)>,
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

        // 注册内置类 String（用于支持 String.valueOf() 等静态方法调用）
        registry.register_builtin_string_class();

        // 注册内置类 Integer（用于支持 Integer.parseInt() 等静态方法调用）
        registry.register_builtin_integer_class();

        registry
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
            parent: None,
            interfaces: Vec::new(),
            is_abstract: false,
            is_final: true, // String 是 final 类，不能被继承
            vtable_layout: None,
        };

        // 添加 String.valueOf() 方法（各种重载版本）
        // valueOf(int)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
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
        });

        // valueOf(long)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
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
        });

        // valueOf(float)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
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
        });

        // valueOf(double)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
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
        });

        // valueOf(boolean)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
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
        });

        // valueOf(char)
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
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
        });

        // valueOf(String) - 返回自身
        string_class.add_method(MethodInfo {
            name: "valueOf".to_string(),
            class_name: "String".to_string(),
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
            parent: None,
            interfaces: Vec::new(),
            is_abstract: false,
            is_final: true, // Integer 是 final 类，不能被继承
            vtable_layout: None,
        };

        // 添加 Integer.parseInt(String) 方法
        integer_class.add_method(MethodInfo {
            name: "parseInt".to_string(),
            class_name: "Integer".to_string(),
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
    ) -> crate::error::cayResult<()> {
        let name = class_info.name.clone();
        if self.classes.contains_key(&name) {
            return Err(crate::error::cayError::DuplicateDefinition {
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
    ) -> crate::error::cayResult<()> {
        let name = interface_info.name.clone();
        if self.interfaces.contains_key(&name) {
            return Err(crate::error::cayError::DuplicateDefinition {
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

    pub fn interface_vtable_key_method_signature(slot_key: &str) -> Option<&str> {
        let rest = slot_key.strip_prefix("$iface$")?;
        let (_, method_sig) = rest.split_once('$')?;
        Some(method_sig)
    }

    pub fn get_interface_vtable_slot(
        &self,
        interface_name: &str,
        method_name: &str,
        arg_types: &[Type],
    ) -> Option<usize> {
        let method_sig = Self::build_method_signature_from_types(method_name, arg_types);
        let key = Self::build_interface_vtable_key(interface_name, &method_sig);
        self.interface_vtable_slots.get(&key).copied()
    }

    /// 注册 struct（值类型）
    pub fn register_struct(
        &mut self,
        struct_info: StructInfo,
        file: Option<String>,
        line: usize,
        column: usize,
    ) -> crate::error::cayResult<()> {
        let name = struct_info.name.clone();
        if self.structs.contains_key(&name) || self.classes.contains_key(&name) {
            return Err(crate::error::cayError::DuplicateDefinition {
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
    ) -> crate::error::cayResult<()> {
        let name = enum_info.name.clone();
        if self.enums.contains_key(&name)
            || self.classes.contains_key(&name)
            || self.structs.contains_key(&name)
        {
            return Err(crate::error::cayError::DuplicateDefinition {
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
        loc: crate::error::SourceLocation,
    ) -> crate::error::cayResult<()> {
        if let Some((existing_class, _, existing_loc)) = self.free_functions.get(func_name) {
            if existing_class != class_name {
                return Err(crate::error::cayError::DuplicateDefinition {
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
        // 尝试命名空间别名（using 声明）
        if let Some(qualified) = self.namespace_aliases.get(name) {
            return self.classes.get(qualified);
        }
        // 尝试当前命名空间上下文
        if !self.current_namespace.is_empty() {
            let qualified = format!("{}::{}", self.current_namespace.join("::"), name);
            if let Some(class) = self.classes.get(&qualified) {
                return Some(class);
            }
        }
        None
    }

    /// 获取类的可变引用
    pub fn get_class_mut(&mut self, name: &str) -> Option<&mut ClassInfo> {
        if self.classes.contains_key(name) {
            return self.classes.get_mut(name);
        }
        if let Some(qualified) = self.namespace_aliases.get(name).cloned() {
            return self.classes.get_mut(&qualified);
        }
        if !self.current_namespace.is_empty() {
            let qualified = format!("{}::{}", self.current_namespace.join("::"), name);
            return self.classes.get_mut(&qualified);
        }
        None
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

    fn types_match_with_namespace(&self, param_type: &Type, arg_type: &Type) -> bool {
        if self.types_match_exact_with_namespace(param_type, arg_type) {
            return true;
        }

        match (param_type, arg_type) {
            (Type::Int64, Type::Int32) => true,
            (Type::Float32, Type::Int32) => true,
            (Type::Float64, Type::Int32) => true,
            (Type::Float64, Type::Int64) => true,
            (Type::Float64, Type::Float32) => true,
            (Type::Float32, Type::Float64) => true,
            (Type::CInt, Type::Int32) | (Type::Int32, Type::CInt) => true,
            (Type::CUInt, Type::Int32) | (Type::Int32, Type::CUInt) => true,
            (Type::CLong, Type::Int64) | (Type::Int64, Type::CLong) => true,
            (Type::CShort, Type::Int32) | (Type::Int32, Type::CShort) => true,
            (Type::CChar, Type::Int32) | (Type::Int32, Type::CChar) => true,
            (Type::CChar, Type::Char) | (Type::Char, Type::CChar) => true,
            (Type::CFloat, Type::Float32) | (Type::Float32, Type::CFloat) => true,
            (Type::CDouble, Type::Float64) | (Type::Float64, Type::CDouble) => true,
            (Type::CBool, Type::Bool) | (Type::Bool, Type::CBool) => true,
            (Type::SizeT, Type::Int64) | (Type::Int64, Type::SizeT) => true,
            (Type::SizeT, Type::Int32) | (Type::Int32, Type::SizeT) => true,
            (Type::SSizeT, Type::Int64) | (Type::Int64, Type::SSizeT) => true,
            (Type::SSizeT, Type::Int32) | (Type::Int32, Type::SSizeT) => true,
            (Type::Pointer(_), Type::Object(obj_name)) if obj_name == "Object" => true,
            (Type::Pointer(_), Type::Array(_)) => true,
            (Type::Function(expected), Type::Function(actual)) => {
                expected.params.len() == actual.params.len()
                    && self.types_match_with_namespace(&expected.return_type, &actual.return_type)
                    && expected
                        .params
                        .iter()
                        .zip(actual.params.iter())
                        .all(|(e, a)| self.types_match_with_namespace(e, a))
            }
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
            (None, None) => Self::simple_name(param_base) == Self::simple_name(arg_base),
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
        // 查找实现了该接口且拥有该方法的类
        for (_, class_info) in &self.classes {
            if class_info.interfaces.iter().any(|i| i == interface_name) {
                if class_info.methods.contains_key(method_name) {
                    return Some(class_info);
                }
            }
        }
        None
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
            methods: HashMap::new(),
        }
    }

    pub fn add_method(&mut self, method: MethodInfo) {
        self.methods.insert(method.name.clone(), method);
    }
}
