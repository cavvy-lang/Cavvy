//! 类型转换和类型系统支持
use crate::codegen::context::IRGenerator;
use crate::types::Type;

impl IRGenerator {
    /// 将 cay 类型转换为 LLVM IR 类型
    pub fn type_to_llvm(&self, ty: &Type) -> String {
        match ty {
            Type::Void => "void".to_string(),
            Type::Int32 => "i32".to_string(),
            Type::Int64 => "i64".to_string(),
            Type::Float32 => "float".to_string(),
            Type::Float64 => "double".to_string(),
            Type::Bool => "i1".to_string(),
            Type::String => "i8*".to_string(),
            Type::Char => "i8".to_string(),
            Type::Object(_) => "i8*".to_string(),
            Type::Array(inner) => format!("{}*", self.type_to_llvm(inner)),
            Type::Function(_) => "i8*".to_string(),
            Type::Auto => panic!("Type::Auto should have been resolved before code generation"),
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
