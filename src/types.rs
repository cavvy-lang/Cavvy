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
    pub parent: Option<String>,
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
        self.methods.get(name)?.iter().find(|m| {
            Self::match_method_params(&m.params, arg_types)
        })
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
            // 可变参数类型是 Array(ElementType)，需要匹配 ElementType
            let vararg_element_type = match &params[last_idx].param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => &params[last_idx].param_type,
            };
            // 所有剩余参数必须匹配可变参数的元素类型
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
        match (param_type, arg_type) {
            (Type::Int64, Type::Int32) => true,
            (Type::Float32, Type::Int32) => true,
            (Type::Float64, Type::Int32) => true,
            (Type::Float64, Type::Int64) => true,
            (Type::Float64, Type::Float32) => true,
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
    pub is_static: bool,
    pub is_native: bool,
}

#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub name: String,
    pub field_type: Type,
    pub is_public: bool,
    pub is_static: bool,
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
        }
    }

    pub fn is_primitive(&self) -> bool {
        matches!(self, 
            Type::Int32 | 
            Type::Int64 | 
            Type::Float32 | 
            Type::Float64 | 
            Type::Bool | 
            Type::Char
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
        }
    }
}

pub struct TypeRegistry {
    pub classes: HashMap<String, ClassInfo>,
}

impl TypeRegistry {
    pub fn new() -> Self {
        Self {
            classes: HashMap::new(),
        }
    }

    pub fn register_class(&mut self, class_info: ClassInfo) -> crate::error::EolResult<()> {
        let name = class_info.name.clone();
        if self.classes.contains_key(&name) {
            return Err(crate::error::semantic_error(
                0, 0,
                format!("Class '{}' already defined", name)
            ));
        }
        self.classes.insert(name, class_info);
        Ok(())
    }

    pub fn get_class(&self, name: &str) -> Option<&ClassInfo> {
        self.classes.get(name)
    }

    /// 根据类名和方法名获取方法（获取第一个匹配的方法，用于无参数类型信息的情况）
    pub fn get_method(&self, class_name: &str, method_name: &str) -> Option<&MethodInfo> {
        self.classes.get(class_name)
            .and_then(|c| c.find_method_by_name(method_name))
    }

    /// 根据类名、方法名和参数类型查找方法（支持重载）
    pub fn find_method(&self, class_name: &str, method_name: &str, arg_types: &[Type]) -> Option<&MethodInfo> {
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
