//! 类型转换推断

use super::super::analyzer::SemanticAnalyzer;
use super::helpers::semantic_error_at_loc;
use crate::ast::*;
use crate::types::Type;

impl SemanticAnalyzer {
    /// 推断类型转换表达式类型
    ///
    /// 验证类型转换的合法性并返回目标类型。
    /// 支持的转换类型：
    /// - 数值类型之间的转换（int <-> float，精度可能损失）
    /// - 引用类型之间的转换（继承层次结构内）
    /// - char 与 int 之间的转换
    ///
    /// # Arguments
    /// * `cast` - 类型转换表达式
    ///
    /// # Returns
    /// 成功时返回目标类型，失败时返回语义错误
    ///
    /// # Type Conversion Rules
    /// 1. 相同类型：允许（无实际效果）
    /// 2. 数值类型之间：允许（可能精度损失）
    /// 3. 引用类型之间：仅当存在继承关系时允许
    /// 4. char <-> int：允许
    /// 5. 数组类型之间：仅当元素类型兼容时允许
    /// 6. 其他组合：非法转换
    pub(crate) fn infer_cast_type(&mut self, cast: &CastExpr) -> crate::error::cayResult<Type> {
        let source_type = self.infer_expr_type_internal(&cast.expr)?;
        let target_type = &cast.target_type;

        // 相同类型，无需转换
        if source_type == *target_type {
            return Ok(target_type.clone());
        }

        // 检查转换是否合法
        if self.is_valid_cast(&source_type, target_type) {
            Ok(target_type.clone())
        } else {
            Err(semantic_error_at_loc(
                &cast.loc,
                format!("Invalid cast from {} to {}", source_type, target_type),
            ))
        }
    }

    /// 检查类型转换是否合法
    ///
    /// # Arguments
    /// * `from` - 源类型
    /// * `to` - 目标类型
    ///
    /// # Returns
    /// 如果转换合法返回 true
    fn is_valid_cast(&self, from: &Type, to: &Type) -> bool {
        use crate::types::Type;

        match (from, to) {
            // 相同类型
            (a, b) if a == b => true,

            // 数值类型之间的转换（所有组合都允许，可能精度损失）
            (Type::Int32, Type::Int64)
            | (Type::Int32, Type::Float32)
            | (Type::Int32, Type::Float64)
            | (Type::Int64, Type::Int32)
            | (Type::Int64, Type::Float32)
            | (Type::Int64, Type::Float64)
            | (Type::Float32, Type::Int32)
            | (Type::Float32, Type::Int64)
            | (Type::Float32, Type::Float64)
            | (Type::Float64, Type::Int32)
            | (Type::Float64, Type::Int64)
            | (Type::Float64, Type::Float32) => true,

            // char 与数值类型之间的转换
            (Type::Char, Type::Int32)
            | (Type::Char, Type::Int64)
            | (Type::Char, Type::CInt)
            | (Type::Int32, Type::Char)
            | (Type::Int64, Type::Char)
            | (Type::CInt, Type::Char) => true,

            // 任何基本类型都可以转换为 string
            (Type::Int32, Type::String)
            | (Type::Int64, Type::String)
            | (Type::Float32, Type::String)
            | (Type::Float64, Type::String)
            | (Type::Char, Type::String)
            | (Type::Bool, Type::String) => true,

            // String 与 c_string (c_char*) 之间的转换（两者在底层都是 i8*）
            (Type::String, Type::CChar) | (Type::CChar, Type::String) => true,
            // String 与 c_char* (Pointer(CChar)) 之间的转换
            (Type::String, Type::Pointer(inner)) if matches!(inner.as_ref(), Type::CChar) => true,
            (Type::Pointer(inner), Type::String) if matches!(inner.as_ref(), Type::CChar) => true,

            // c_void* 与任意指针类型之间的转换（C风格）
            (Type::Pointer(from_inner), Type::Pointer(to_inner))
                if matches!(from_inner.as_ref(), Type::CVoid)
                    || matches!(to_inner.as_ref(), Type::CVoid) =>
            {
                true
            }

            // FFI 类型与基本类型之间的转换
            // c_int <-> int
            (Type::CInt, Type::Int32) | (Type::Int32, Type::CInt) => true,
            // c_long <-> long
            (Type::CLong, Type::Int64) | (Type::Int64, Type::CLong) => true,
            // c_short <-> int (16位到32位)
            (Type::CShort, Type::Int32) | (Type::Int32, Type::CShort) => true,
            // c_char/c_uchar <-> int
            (Type::CChar, Type::Int32) | (Type::Int32, Type::CChar) => true,
            (Type::CUChar, Type::Int32) | (Type::Int32, Type::CUChar) => true,
            (Type::CChar, Type::Char) | (Type::Char, Type::CChar) => true,
            // c_short/c_ushort <-> int
            (Type::CShort, Type::Int32) | (Type::Int32, Type::CShort) => true,
            (Type::CUShort, Type::Int32) | (Type::Int32, Type::CUShort) => true,
            // c_int/c_uint <-> int/long
            (Type::CInt, Type::Int32) | (Type::Int32, Type::CInt) => true,
            (Type::CUInt, Type::Int32) | (Type::Int32, Type::CUInt) => true,
            (Type::CInt, Type::Int64) | (Type::Int64, Type::CInt) => true,
            (Type::CUInt, Type::Int64) | (Type::Int64, Type::CUInt) => true,
            // c_long <-> int/long
            (Type::CLong, Type::Int32) | (Type::Int32, Type::CLong) => true,
            (Type::CLong, Type::Int64) | (Type::Int64, Type::CLong) => true,
            // c_ulong <-> int/long/c_long
            (Type::CULong, Type::Int32) | (Type::Int32, Type::CULong) => true,
            (Type::CULong, Type::Int64) | (Type::Int64, Type::CULong) => true,
            (Type::CULong, Type::CLong) | (Type::CLong, Type::CULong) => true,
            (Type::CULong, Type::CUInt) | (Type::CUInt, Type::CULong) => true,
            // c_float <-> float/double
            (Type::CFloat, Type::Float32) | (Type::Float32, Type::CFloat) => true,
            (Type::CFloat, Type::Float64) | (Type::Float64, Type::CFloat) => true,
            // c_double <-> float/double
            (Type::CDouble, Type::Float64) | (Type::Float64, Type::CDouble) => true,
            (Type::CDouble, Type::Float32) | (Type::Float32, Type::CDouble) => true,
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
            // ptr <-> uintptr_t/intptr_t (指针与整数类型转换)
            (Type::Pointer(_), Type::UIntPtr) | (Type::UIntPtr, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::IntPtr) | (Type::IntPtr, Type::Pointer(_)) => true,
            // ptr <-> long/int (指针与基本整数类型转换，用于 FFI 中 & 和 c_str 等返回 IntPtr 的场景)
            (Type::Pointer(_), Type::Int64) | (Type::Int64, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::Int32) | (Type::Int32, Type::Pointer(_)) => true,
            // FFI 整数类型 <-> 指针 (用于 c_long/c_ulong 等作为指针值的场景)
            (Type::Pointer(_), Type::CLong) | (Type::CLong, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CULong) | (Type::CULong, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CInt) | (Type::CInt, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CUInt) | (Type::CUInt, Type::Pointer(_)) => true,
            // c_bool <-> bool 和 int
            (Type::CBool, Type::Bool) | (Type::Bool, Type::CBool) => true,
            (Type::CBool, Type::Int32) | (Type::Int32, Type::CBool) => true,

            // FFI 类型之间的转换
            (Type::CInt, Type::CLong) | (Type::CLong, Type::CInt) => true,
            (Type::CInt, Type::CShort) | (Type::CShort, Type::CInt) => true,
            (Type::CInt, Type::CChar) | (Type::CChar, Type::CInt) => true,
            (Type::CFloat, Type::CDouble) | (Type::CDouble, Type::CFloat) => true,
            (Type::SizeT, Type::UIntPtr) | (Type::UIntPtr, Type::SizeT) => true,
            (Type::SSizeT, Type::IntPtr) | (Type::IntPtr, Type::SSizeT) => true,
            (Type::UIntPtr, Type::IntPtr) | (Type::IntPtr, Type::UIntPtr) => true,

            // 引用类型之间的转换：需要继承关系
            (Type::Object(from_name), Type::Object(to_name)) => {
                // 检查是否存在继承关系（双向）
                self.is_related_type(from_name, to_name)
            }

            // 数组类型之间的转换：元素类型兼容
            (Type::Array(from_elem), Type::Array(to_elem)) => {
                self.is_valid_cast(from_elem, to_elem)
            }

            // null 可以转换为任何引用类型
            (Type::Object(obj_name), Type::Object(_)) if obj_name == "Object" => true,

            // 其他组合都不合法
            _ => false,
        }
    }

    /// 检查两个类型是否存在继承关系（双向）
    ///
    /// 用于类型转换检查，允许向上转型（子类->父类）和向下转型（父类->子类）
    fn is_related_type(&self, type_a: &str, type_b: &str) -> bool {
        // 相同类型
        if type_a == type_b {
            return true;
        }

        // 检查 type_a 是否是 type_b 的子类型
        if self.is_subtype_of_by_name(type_a, type_b) {
            return true;
        }

        // 检查 type_b 是否是 type_a 的子类型
        if self.is_subtype_of_by_name(type_b, type_a) {
            return true;
        }

        false
    }

    /// 通过类型名称检查子类型关系
    ///
    /// 辅助函数，用于检查一个类型是否是另一个类型的子类型
    fn is_subtype_of_by_name(&self, subtype: &str, supertype: &str) -> bool {
        // 相同类型
        if subtype == supertype {
            return true;
        }

        // 所有类都是 Object 的子类型
        if supertype == "Object" {
            return self.type_registry.class_exists(subtype)
                || subtype == "String"
                || subtype == "Function";
        }

        // 迭代遍历继承链
        let mut current = subtype.to_string();
        let mut visited = std::collections::HashSet::new();

        loop {
            // 防止循环继承导致的无限循环
            if !visited.insert(current.clone()) {
                return false;
            }

            if let Some(class_info) = self.type_registry.get_class(&current) {
                match &class_info.parent {
                    Some(parent) => {
                        if parent == supertype {
                            return true;
                        }
                        current = parent.clone();
                    }
                    None => return false,
                }
            } else {
                // 内置类型检查
                return (subtype == "String" || subtype == "Function") && supertype == "Object";
            }
        }
    }
}
