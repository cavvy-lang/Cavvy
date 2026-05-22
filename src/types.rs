use std::fmt;
use std::collections::HashMap;

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
    Auto,  // 自动类型推断占位符
    // FFI 类型
    CInt,       // C int (通常为 i32)
    CUInt,      // C unsigned int (通常为 u32)
    CLong,      // C long (平台相关: Windows i32, Linux/macOS i64)
    CShort,     // C short (i16)
    CUShort,    // C unsigned short (u16)
    CChar,      // C char (i8)
    CUChar,     // C unsigned char (u8)
    CFloat,     // C float (f32)
    CDouble,    // C double (f64)
    SizeT,      // size_t (usize, 平台相关)
    SSizeT,     // ssize_t (isize, 平台相关)
    UIntPtr,    // uintptr_t (usize)
    IntPtr,     // intptr_t (isize)
    CVoid,      // C void (用于指针)
    CBool,      // C bool (i8, 0 或 1)
    // FFI 指针类型
    Pointer(Box<Type>),  // 通用指针类型: Pointer(CVoid) = void*
    // FFI 结构体类型
    Struct(String),      // 命名结构体: Struct("SDL_Window")
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionType {
    pub params: Vec<Type>,
    pub return_type: Box<Type>,
    pub is_static: bool,
}

#[derive(Debug, Clone)]
pub struct ClassInfo {
    pub name: String,
    pub methods: HashMap<String, Vec<MethodInfo>>,  // 支持方法重载：同名方法可以有多个
    pub fields: HashMap<String, FieldInfo>,
    pub constructors: Vec<ConstructorInfo>,  // 构造函数列表
    pub has_destructor: bool,  // 是否有析构函数
    pub parent: Option<String>,
    pub interfaces: Vec<String>,  // 实现的接口列表
    pub is_abstract: bool,  // 是否是抽象类
    pub is_final: bool,  // 是否是final类（禁止继承）
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
        methods.iter().find(|m| {
            Self::match_method_params(&m.params, arg_types)
        })
    }
    
    /// 精确匹配方法参数（不考虑隐式转换）
    fn match_method_params_exact(params: &[ParameterInfo], arg_types: &[Type]) -> bool {
        if params.is_empty() {
            return arg_types.is_empty();
        }

        // 检查最后一个参数是否是可变参数
        let last_idx = params.len() - 1;
        if params[last_idx].is_varargs {
            // 可变参数：至少需要 params.len() - 1 个参数
            if arg_types.len() < last_idx {
                return false;
            }
            // 检查固定参数（精确匹配）
            for i in 0..last_idx {
                if params[i].param_type != arg_types[i] {
                    return false;
                }
            }

            // 检查可变参数（精确匹配）
            let vararg_param_type = &params[last_idx].param_type;
            let vararg_element_type = match vararg_param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => vararg_param_type,
            };

            // 如果只有一个参数且类型匹配数组类型，直接接受
            if arg_types.len() == last_idx + 1 {
                if *vararg_param_type == arg_types[last_idx] {
                    return true;
                }
            }

            // 按元素类型检查每个参数（精确匹配）
            for i in last_idx..arg_types.len() {
                if *vararg_element_type != arg_types[i] {
                    return false;
                }
            }
            true
        } else {
            // 非可变参数：参数数量必须完全匹配
            if params.len() != arg_types.len() {
                return false;
            }
            // 精确匹配：类型必须完全相同
            params.iter().zip(arg_types.iter()).all(|(p, a)| {
                p.param_type == *a
            })
        }
    }

    /// 匹配方法参数（支持可变参数）
    fn match_method_params(params: &[ParameterInfo], arg_types: &[Type]) -> bool {
        if params.is_empty() {
            return arg_types.is_empty();
        }

        // 检查最后一个参数是否是可变参数
        let last_idx = params.len() - 1;
        if params[last_idx].is_varargs {
            // 可变参数：至少需要 params.len() - 1 个参数
            if arg_types.len() < last_idx {
                return false;
            }
            // 检查固定参数
            for i in 0..last_idx {
                if !Self::types_match(&params[i].param_type, &arg_types[i]) {
                    return false;
                }
            }

            // 检查可变参数
            let vararg_param_type = &params[last_idx].param_type;
            let vararg_element_type = match vararg_param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => vararg_param_type,
            };

            // 如果只有一个参数且类型匹配数组类型，直接接受（传递数组给可变参数）
            if arg_types.len() == last_idx + 1 {
                if Self::types_match(vararg_param_type, &arg_types[last_idx]) {
                    return true;
                }
            }

            // 否则，按元素类型检查每个参数
            for i in last_idx..arg_types.len() {
                if !Self::types_match(vararg_element_type, &arg_types[i]) {
                    return false;
                }
            }
            true
        } else {
            // 非可变参数：参数数量必须完全匹配
            if params.len() != arg_types.len() {
                return false;
            }
            params.iter().zip(arg_types.iter()).all(|(p, a)| {
                Self::types_match(&p.param_type, a)
            })
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
        // 允许 int -> long, int -> float, int -> double 等隐式转换
        // 也允许 double -> float 的显式转换（用于字面量）
        match (param_type, arg_type) {
            (Type::Int64, Type::Int32) => true,
            (Type::Float32, Type::Int32) => true,
            (Type::Float64, Type::Int32) => true,
            (Type::Float64, Type::Int64) => true,
            (Type::Float64, Type::Float32) => true,
            (Type::Float32, Type::Float64) => true,  // double -> float 截断转换
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
                expected.params.iter().zip(actual.params.iter()).all(|(e, a)| {
                    Self::types_match(e, a)
                })
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
    pub is_override: bool,  // 标记是否是重写方法
    pub is_final: bool,  // 是否是final方法（禁止重写）
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub field_type: Type,
    pub is_public: bool,
    pub is_private: bool,
    pub is_protected: bool,
    pub is_static: bool,
    pub is_final: bool,  // 是否是final字段（编译期常量）
    pub is_const_expr: bool,  // 是否是编译期常量（static final且初始化值为常量）
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterInfo {
    pub name: String,
    pub param_type: Type,
    pub is_varargs: bool,  // 是否为可变参数
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
            Type::String => 8, // 指针大小
            Type::Object(_) => 8, // 引用类型
            Type::Array(_) => 8, // 指针大小
            Type::Function(_) => 8, // 函数指针
            Type::Auto => panic!("Cannot get size of auto type - type inference not completed"),
            // FFI 类型大小 (平台相关，这里使用常见值)
            Type::CInt => 4,       // C int 通常为 4 字节
            Type::CUInt => 4,      // C unsigned int 通常为 4 字节
            Type::CLong => 8,      // C long: Windows 4, Linux/macOS 8，使用 8 作为保守值
            Type::CShort => 2,     // C short 为 2 字节
            Type::CUShort => 2,    // C unsigned short 为 2 字节
            Type::CChar => 1,      // C char 为 1 字节
            Type::CUChar => 1,     // C unsigned char 为 1 字节
            Type::CFloat => 4,     // C float 为 4 字节
            Type::CDouble => 8,    // C double 为 8 字节
            Type::SizeT => 8,      // size_t 为指针大小 (64位系统)
            Type::SSizeT => 8,     // ssize_t 为指针大小 (64位系统)
            Type::UIntPtr => 8,    // uintptr_t 为指针大小
            Type::IntPtr => 8,     // intptr_t 为指针大小
            Type::CVoid => 0,      // void 无大小
            Type::CBool => 1,      // C bool 通常为 1 字节
            // FFI 指针和结构体
            Type::Pointer(_) => 8, // 指针大小 (64位系统)
            Type::Struct(_) => 8,  // 结构体作为指针传递，实际大小由编译器决定
        }
    }

    /// 检查是否为原始类型（包括内置数值类型和FFI类型）
    /// 时间复杂度: O(1)
    pub fn is_primitive(&self) -> bool {
        matches!(self, 
            // 内置数值类型
            Type::Int32 | 
            Type::Int64 | 
            Type::Float32 | 
            Type::Float64 | 
            Type::Bool | 
            Type::Char |
            // FFI 数值类型
            Type::CInt | Type::CUInt | Type::CLong |
            Type::CShort | Type::CUShort | Type::CChar | Type::CUChar |
            Type::CFloat | Type::CDouble | Type::SizeT | Type::SSizeT |
            Type::UIntPtr | Type::IntPtr | Type::CVoid | Type::CBool
        )
    }

    pub fn is_reference_type(&self) -> bool {
        matches!(self, Type::String | Type::Object(_) | Type::Array(_))
    }

    pub fn is_integer(&self) -> bool {
        matches!(self, Type::Int32 | Type::Int64)
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
            // FFI 类型显示
            Type::CInt => write!(f, "c_int"),
            Type::CUInt => write!(f, "c_uint"),
            Type::CLong => write!(f, "c_long"),
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
    pub interfaces: HashMap<String, InterfaceInfo>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            classes: HashMap::new(),
            interfaces: HashMap::new(),
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
            methods: HashMap::new(),
            fields: HashMap::new(),
            constructors: Vec::new(),
            has_destructor: false,
            parent: None,
            interfaces: Vec::new(),
            is_abstract: false,
            is_final: true,  // String 是 final 类，不能被继承
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
            is_final: true,
            is_override: false,
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
            is_final: true,
            is_override: false,
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
            is_final: true,
            is_override: false,
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
            is_final: true,
            is_override: false,
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
            is_final: true,
            is_override: false,
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
            is_final: true,
            is_override: false,
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
            is_final: true,
            is_override: false,
        });

        // 注册 String 类
        self.classes.insert("String".to_string(), string_class);
    }

    /// 注册内置 Integer 类
    fn register_builtin_integer_class(&mut self) {
        // 创建 Integer 类信息
        let mut integer_class = ClassInfo {
            name: "Integer".to_string(),
            methods: HashMap::new(),
            fields: HashMap::new(),
            constructors: Vec::new(),
            has_destructor: false,
            parent: None,
            interfaces: Vec::new(),
            is_abstract: false,
            is_final: true,  // Integer 是 final 类，不能被继承
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
            is_final: true,
            is_override: false,
        });

        // 注册 Integer 类
        self.classes.insert("Integer".to_string(), integer_class);
    }

    pub fn register_class(&mut self, class_info: ClassInfo, file: Option<String>, line: usize, column: usize) -> crate::error::cayResult<()> {
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

    pub fn register_interface(&mut self, interface_info: InterfaceInfo, file: Option<String>, line: usize, column: usize) -> crate::error::cayResult<()> {
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

    pub fn get_interface(&self, name: &str) -> Option<&InterfaceInfo> {
        self.interfaces.get(name)
    }

    pub fn interface_exists(&self, name: &str) -> bool {
        self.interfaces.contains_key(name)
    }

    pub fn get_class(&self, name: &str) -> Option<&ClassInfo> {
        self.classes.get(name)
    }

    /// 获取类的可变引用
    pub fn get_class_mut(&mut self, name: &str) -> Option<&mut ClassInfo> {
        self.classes.get_mut(name)
    }

    /// 根据类名和方法名获取方法（获取第一个匹配的方法，用于无参数类型信息的情况，支持继承）
    pub fn get_method(&self, class_name: &str, method_name: &str) -> Option<&MethodInfo> {
        if let Some(class_info) = self.classes.get(class_name) {
            if let Some(method) = class_info.find_method_by_name(method_name) {
                return Some(method);
            }
            // 如果在当前类中没找到，递归在父类中查找
            if let Some(ref parent_name) = class_info.parent {
                return self.get_method(parent_name, method_name);
            }
        }
        None
    }

    /// 根据类名、方法名和参数类型查找方法（支持重载和继承）
    pub fn find_method(&self, class_name: &str, method_name: &str, arg_types: &[Type]) -> Option<&MethodInfo> {
        // 首先在当前类中查找
        if let Some(class_info) = self.classes.get(class_name) {
            if let Some(method) = class_info.find_method(method_name, arg_types) {
                return Some(method);
            }
            // 如果在当前类中没找到，递归在父类中查找
            if let Some(ref parent_name) = class_info.parent {
                return self.find_method(parent_name, method_name, arg_types);
            }
        }
        None
    }

    /// 根据类名、方法名和参数类型查找方法，只在当前类中查找（不递归父类）
    pub fn find_method_in_class(&self, class_name: &str, method_name: &str, arg_types: &[Type]) -> Option<&MethodInfo> {
        self.classes.get(class_name)
            .and_then(|c| c.find_method(method_name, arg_types))
    }

    pub fn class_exists(&self, name: &str) -> bool {
        self.classes.contains_key(name)
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
