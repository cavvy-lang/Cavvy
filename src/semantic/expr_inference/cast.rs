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
    pub(crate) fn infer_cast_type(
        &mut self,
        cast: &CastExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
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

    /// 检查类型转换是否合法（薄入口，非独立规则表）
    ///
    /// # Arguments
    /// * `from` - 源类型
    /// * `to` - 目标类型
    ///
    /// # 参数顺序
    /// `is_valid_cast(from, to)`：源类型在前、目标类型在后，
    /// 与统一规则源 `types_compatible(from, to)` 一致。
    ///
    /// 先复用统一兼容规则（赋值兼容的转换必然可以作为显式转换），
    /// 再叠加仅显式转换允许的附加规则：缩窄数值转换、数值/char/bool 转 string、
    /// String 与 c_char*/c_char 双向互转、向下转型（父类 -> 子类）、
    /// 以及赋值兼容中未覆盖的 FFI 互转（如 c_int <-> long、c_float <-> double）。
    ///
    /// # Returns
    /// 如果转换合法返回 true
    fn is_valid_cast(&self, from: &Type, to: &Type) -> bool {
        use crate::types::Type;

        // 统一规则源：赋值兼容的类型对必然允许显式转换
        // （含相同类型、数值提升链、FFI 整型互通、null -> 引用/指针、
        // 继承/实现方向、void* 与任意指针双向、指针与整数互转等）
        if self.types_compatible(from, to) {
            return true;
        }

        // 以下为仅显式转换允许的附加规则（统一规则未覆盖的部分）
        match (from, to) {
            // 数值类型之间的缩窄转换（提升链方向已由统一规则覆盖）
            (
                Type::Int32 | Type::Int64 | Type::Float32 | Type::Float64,
                Type::Int32 | Type::Int64 | Type::Float32 | Type::Float64,
            ) => true,

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
            // （String -> c_char* 方向已由统一规则覆盖，这里补反向和 c_char 标量形式）
            (Type::String, Type::CChar) | (Type::CChar, Type::String) => true,
            (Type::Pointer(inner), Type::String) if matches!(inner.as_ref(), Type::CChar) => true,

            // 统一规则未覆盖的 FFI 互转（仅显式转换允许；与基本类型互通部分已由统一规则覆盖）
            // c_uchar <-> int
            (Type::CUChar, Type::Int32) | (Type::Int32, Type::CUChar) => true,
            // c_ushort <-> int
            (Type::CUShort, Type::Int32) | (Type::Int32, Type::CUShort) => true,
            // c_int/c_uint <-> long
            (Type::CInt, Type::Int64) | (Type::Int64, Type::CInt) => true,
            (Type::CUInt, Type::Int64) | (Type::Int64, Type::CUInt) => true,
            // c_ulong <-> c_uint
            (Type::CULong, Type::CUInt) | (Type::CUInt, Type::CULong) => true,
            // c_float <-> double
            (Type::CFloat, Type::Float64) | (Type::Float64, Type::CFloat) => true,
            // c_double <-> float
            (Type::CDouble, Type::Float32) | (Type::Float32, Type::CDouble) => true,

            // 引用类型之间的转换：需要继承关系
            (Type::Object(from_name), Type::Object(to_name)) => {
                // 向上转型和 null 字面量已由统一规则覆盖，这里补向下转型（双向检查）
                self.is_related_type(from_name, to_name)
            }

            // 数组类型之间的转换：元素类型兼容（递归走显式转换规则，允许元素缩窄）
            (Type::Array(from_elem), Type::Array(to_elem)) => {
                self.is_valid_cast(from_elem, to_elem)
            }

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
