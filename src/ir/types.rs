//! IR 类型系统
//!
//! 定义 IR 层面的类型表示，独立于 AST 类型但完全覆盖所有 Cavvy 类型。

use std::fmt;

/// IR 类型 - 后端无关的类型表示
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum IrType {
    /// 空类型
    Void,
    /// 1位布尔
    I1,
    /// 8位整数 (char, byte)
    I8,
    /// 16位整数
    I16,
    /// 32位整数 (int)
    I32,
    /// 64位整数 (long)
    I64,
    /// 32位浮点 (float)
    F32,
    /// 64位浮点 (double)
    F64,
    /// 指针类型 (i8* 通用指针)
    Pointer(Box<IrType>),
    /// 数组类型 [N x T]
    Array(Box<IrType>, usize),
    /// 结构体类型 (命名字段)
    Struct {
        name: String,
        fields: Vec<(String, IrType)>,
    },
    /// 函数类型
    Function {
        params: Vec<IrType>,
        return_type: Box<IrType>,
    },
    /// 元数据/标签类型 (用于 block 地址等)
    Label,
    /// 原始 LLVM IR 类型字符串 (用于内联 IR)
    Raw(String),
}

impl IrType {
    /// 是否是整数类型
    pub fn is_integer(&self) -> bool {
        matches!(self, IrType::I1 | IrType::I8 | IrType::I16 | IrType::I32 | IrType::I64)
    }

    /// 是否是浮点类型
    pub fn is_float(&self) -> bool {
        matches!(self, IrType::F32 | IrType::F64)
    }

    /// 是否是指针类型
    pub fn is_pointer(&self) -> bool {
        matches!(self, IrType::Pointer(_))
    }

    /// 获取类型的位宽（整数和浮点）
    pub fn bit_width(&self) -> Option<u32> {
        match self {
            IrType::I1 => Some(1),
            IrType::I8 => Some(8),
            IrType::I16 => Some(16),
            IrType::I32 => Some(32),
            IrType::I64 => Some(64),
            IrType::F32 => Some(32),
            IrType::F64 => Some(64),
            _ => None,
        }
    }

    /// 获取类型大小（字节）
    pub fn size_bytes(&self) -> usize {
        match self {
            IrType::Void => 0,
            IrType::I1 | IrType::I8 => 1,
            IrType::I16 => 2,
            IrType::I32 | IrType::F32 => 4,
            IrType::I64 | IrType::F64 | IrType::Pointer(_) => 8,
            IrType::Array(elem, count) => elem.size_bytes() * count,
            IrType::Struct { fields, .. } => {
                let mut total = 0;
                for (_, ty) in fields {
                    let align = ty.alignment();
                    total = (total + align - 1) & !(align - 1);
                    total += ty.size_bytes();
                }
                // 最终对齐到 8 字节
                (total + 7) & !7
            }
            IrType::Function { .. } => 8, // 函数指针
            IrType::Label => 0,
            IrType::Raw(_) => 8, // 假设指针大小
        }
    }

    /// 获取类型对齐（字节）
    pub fn alignment(&self) -> usize {
        match self {
            IrType::I1 | IrType::I8 => 1,
            IrType::I16 => 2,
            IrType::I32 | IrType::F32 => 4,
            IrType::I64 | IrType::F64 | IrType::Pointer(_) | IrType::Function { .. } => 8,
            IrType::Array(elem, _) => elem.alignment(),
            IrType::Struct { .. } => 8,
            IrType::Void | IrType::Label => 1,
            IrType::Raw(_) => 8,
        }
    }

    /// 转换为 LLVM IR 类型字符串
    pub fn to_llvm_str(&self) -> String {
        match self {
            IrType::Void => "void".to_string(),
            IrType::I1 => "i1".to_string(),
            IrType::I8 => "i8".to_string(),
            IrType::I16 => "i16".to_string(),
            IrType::I32 => "i32".to_string(),
            IrType::I64 => "i64".to_string(),
            IrType::F32 => "float".to_string(),
            IrType::F64 => "double".to_string(),
            IrType::Pointer(inner) => format!("{}*", inner.to_llvm_str()),
            IrType::Array(inner, count) => format!("[{} x {}]", count, inner.to_llvm_str()),
            IrType::Struct { name, .. } => format!("%{}", name),
            IrType::Function { params, return_type } => {
                let param_strs: Vec<String> = params.iter().map(|p| p.to_llvm_str()).collect();
                format!("{} ({})*", return_type.to_llvm_str(), param_strs.join(", "))
            }
            IrType::Label => "label".to_string(),
            IrType::Raw(s) => s.clone(),
        }
    }
}

impl fmt::Display for IrType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_llvm_str())
    }
}

/// 从 Cavvy 类型转换为 IR 类型
impl From<&crate::types::Type> for IrType {
    fn from(ty: &crate::types::Type) -> Self {
        use crate::types::Type;
        match ty {
            Type::Void => IrType::Void,
            Type::Int32 => IrType::I32,
            Type::Int64 => IrType::I64,
            Type::Float32 => IrType::F32,
            Type::Float64 => IrType::F64,
            Type::Bool => IrType::I1,
            Type::String => IrType::Pointer(Box::new(IrType::I8)),
            Type::Char => IrType::I8,
            Type::Object(name) => IrType::Struct {
                name: format!("class.{}", name),
                fields: Vec::new(),
            },
            Type::Array(elem) => IrType::Pointer(Box::new(IrType::from(elem.as_ref()))),
            Type::Function(ft) => IrType::Function {
                params: ft.params.iter().map(|p| IrType::from(p)).collect(),
                return_type: Box::new(IrType::from(ft.return_type.as_ref())),
            },
            Type::Auto => IrType::I32, // 默认回退
            // FFI 类型
            Type::CInt | Type::CUInt => IrType::I32,
            Type::CLong | Type::CULong | Type::SizeT | Type::SSizeT | Type::UIntPtr | Type::IntPtr => IrType::I64,
            Type::CShort | Type::CUShort => IrType::I16,
            Type::CChar | Type::CUChar | Type::CBool => IrType::I8,
            Type::CFloat => IrType::F32,
            Type::CDouble => IrType::F64,
            Type::CVoid => IrType::Void,
            Type::Pointer(inner) => IrType::Pointer(Box::new(IrType::from(inner.as_ref()))),
            Type::Struct(name) => IrType::Struct {
                name: format!("struct.{}", name),
                fields: Vec::new(),
            },
            // 泛型类型 - 默认回退为指针
            Type::GenericParam(_) => IrType::Pointer(Box::new(IrType::I8)),
            Type::Generic(_, _) => IrType::Pointer(Box::new(IrType::I8)),
        }
    }
}
