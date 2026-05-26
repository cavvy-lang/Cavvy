//! 类型转换和类型系统支持
use crate::codegen::context::IRGenerator;
use crate::types::Type;

impl IRGenerator {
    /// 解析类型（包括类型别名）
    pub fn resolve_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Object(name) => {
                // 检查是否是类型别名
                if let Some(aliased_type) = self.type_aliases.get(name) {
                    aliased_type.clone()
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone()
        }
    }

    /// 将 cay 类型转换为 LLVM IR 类型
    pub fn type_to_llvm(&self, ty: &Type) -> String {
        // 首先解析类型别名
        let resolved_ty = self.resolve_type(ty);
        
        match &resolved_ty {
            Type::Void => "void".to_string(),
            Type::Int32 => "i32".to_string(),
            Type::Int64 => "i64".to_string(),
            Type::Float32 => "float".to_string(),
            Type::Float64 => "double".to_string(),
            Type::Bool => "i1".to_string(),
            Type::String => "i8*".to_string(),
            Type::Char => "i8".to_string(),
            Type::Object(_) => "i8*".to_string(),
            Type::Array(inner) => {
                // LLVM 不允许 void*，使用 i8* 代替
                if matches!(inner.as_ref(), Type::CVoid) {
                    "i8*".to_string()
                } else {
                    format!("{}*", self.type_to_llvm(inner))
                }
            },
            Type::Function(_) => "i8*".to_string(),
            Type::Auto => panic!("Type::Auto should have been resolved before code generation"),
            // FFI 类型映射
            Type::CInt => "i32".to_string(),      // C int 通常为 32 位
            Type::CUInt => "i32".to_string(),     // C unsigned int 通常为 32 位
            Type::CLong => self.c_long_llvm(),    // 平台相关
            Type::CULong => self.c_long_llvm(),   // 同 CLong
            Type::CShort => "i16".to_string(),    // C short 为 16 位
            Type::CUShort => "i16".to_string(),   // C unsigned short 为 16 位
            Type::CChar => "i8".to_string(),      // C char 为 8 位
            Type::CUChar => "i8".to_string(),     // C unsigned char 为 8 位
            Type::CFloat => "float".to_string(),  // C float 为 32 位
            Type::CDouble => "double".to_string(), // C double 为 64 位
            Type::SizeT => "i64".to_string(),     // size_t 在 64 位系统为 64 位
            Type::SSizeT => "i64".to_string(),    // ssize_t 在 64 位系统为 64 位
            Type::UIntPtr => "i64".to_string(),   // uintptr_t 在 64 位系统为 64 位
            Type::IntPtr => "i64".to_string(),    // intptr_t 在 64 位系统为 64 位
            Type::CVoid => "void".to_string(),    // C void
            Type::CBool => "i8".to_string(),      // C bool 通常为 8 位
            // FFI 指针和结构体
            Type::Pointer(inner) => {
                // LLVM 不允许 void*，使用 i8* 代替
                if matches!(inner.as_ref(), Type::CVoid) {
                    "i8*".to_string()
                } else {
                    format!("{}*", self.type_to_llvm(inner))
                }
            },
            Type::Struct(name) => format!("%struct.{}", name),                // 命名结构体
            // 泛型类型 - 默认为 i8* 指针
            Type::GenericParam(_) => "i8*".to_string(),
            Type::Generic(_, _) => "i8*".to_string(),
        }
    }

    /// 获取 C long 类型的 LLVM 表示（平台相关）
    fn c_long_llvm(&self) -> String {
        // Windows: long 是 32 位
        // Linux/macOS: long 是 64 位
        if self.is_windows_target() {
            "i32".to_string()
        } else {
            "i64".to_string()
        }
    }

    /// 解析类型化的值，返回 (类型, 值)
    pub fn parse_typed_value(&self, typed_val: &str) -> (String, String) {
        let parts: Vec<&str> = typed_val.splitn(2, ' ').collect();
        if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            ("i64".to_string(), typed_val.to_string())
        }
    }

    /// 判断是否为整数类型
    pub fn is_integer_type(&self, ty: &str) -> bool {
        ty.starts_with("i") && !ty.ends_with("*")
    }

    /// 判断是否为浮点类型
    pub fn is_float_type(&self, ty: &str) -> bool {
        ty == "float" || ty == "double"
    }

    /// 判断是否为布尔类型
    pub fn is_bool_type(&self, ty: &str) -> bool {
        ty == "i1"
    }

    /// 判断是否为字符串类型
    pub fn is_string_type(&self, ty: &str) -> bool {
        ty == "i8*"
    }
}
