//! 二元表达式类型推断

use super::super::analyzer::SemanticAnalyzer;
use super::helpers::semantic_error_at_loc;
use crate::ast::*;
use crate::types::Type;

impl SemanticAnalyzer {
    /// 推断二元表达式类型
    pub(crate) fn infer_binary_type(
        &mut self,
        bin: &BinaryExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        let left_type = self.infer_expr_type_internal(&bin.left)?;
        let right_type = self.infer_expr_type_internal(&bin.right)?;

        match bin.op {
            BinaryOp::Add => {
                let right_is_literal = matches!(bin.right.as_ref(), Expr::Literal(_));
                let left_is_literal = matches!(bin.left.as_ref(), Expr::Literal(_));

                // 泛型模板体中的同类型运算延迟到单态化后确定具体指令。
                // 例如 Helper<T>::add(T, T) 在 Helper<int> 特化中生成整数加法。
                if left_type == right_type && matches!(left_type, Type::GenericParam(_)) {
                    Ok(left_type)
                }
                // 字符串连接：支持 String + String 和 String + char
                else if left_type == Type::String && right_type == Type::String {
                    Ok(Type::String)
                } else if left_type == Type::String && right_type == Type::Char {
                    // String + char = String
                    Ok(Type::String)
                } else if left_type == Type::Char && right_type == Type::String {
                    // char + String = String
                    Ok(Type::String)
                } else if left_type == Type::String
                    && Self::is_numeric_type_helper(&right_type)
                    && !right_is_literal
                {
                    Ok(Type::String)
                } else if Self::is_numeric_type_helper(&left_type)
                    && !left_is_literal
                    && right_type == Type::String
                {
                    Ok(Type::String)
                }
                // 数值加法：两个操作数都必须是基本数值类型
                else if left_type.is_primitive() && right_type.is_primitive() {
                    // 类型提升
                    Ok(self.promote_types(&left_type, &right_type))
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        format!(
                            "Cannot add {} and {}: addition requires both operands to be numeric or both to be strings",
                            left_type, right_type
                        ),
                    ))
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                if left_type == right_type && matches!(left_type, Type::GenericParam(_)) {
                    Ok(left_type)
                } else if left_type.is_primitive() && right_type.is_primitive() {
                    // 检查除零和模零（仅当右操作数是字面量0时）
                    if matches!(bin.op, BinaryOp::Div | BinaryOp::Mod) {
                        if let Expr::Literal(lit_expr) = bin.right.as_ref() {
                            if let LiteralValue::Int32(0) = lit_expr.value {
                                return Err(semantic_error_at_loc(
                                    &bin.loc,
                                    "/ by zero".to_string(),
                                ));
                            }
                            if let LiteralValue::Int64(0) = lit_expr.value {
                                return Err(semantic_error_at_loc(
                                    &bin.loc,
                                    "/ by zero".to_string(),
                                ));
                            }
                        }
                    }
                    // 类型提升
                    Ok(self.promote_types(&left_type, &right_type))
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        format!(
                            "Cannot apply {:?} to {} and {}: operator requires numeric operands",
                            bin.op, left_type, right_type
                        ),
                    ))
                }
            }
            BinaryOp::Eq | BinaryOp::Ne => {
                // 相等性比较规则：
                // - 数值类型之间可以比较（含 char 与 FFI 数值类型）
                // - 同类型可以比较（含 bool、String、对象、数组、指针、函数指针）
                // - null 字面量可以与任何引用类型/指针类型比较
                // - 引用类型之间存在继承/实现关系（任一方向可赋值）时可以比较
                // - 泛型模板体内的类型参数延迟到单态化后检查
                let both_numeric =
                    Self::is_numeric_type_helper(&left_type) && Self::is_numeric_type_helper(&right_type);
                let involves_generic_param = matches!(left_type, Type::GenericParam(_))
                    || matches!(right_type, Type::GenericParam(_));
                let null_vs_reference = (left_type.is_null_literal()
                    && (right_type.is_reference_type() || matches!(right_type, Type::Pointer(_))))
                    || (right_type.is_null_literal()
                        && (left_type.is_reference_type() || matches!(left_type, Type::Pointer(_))));
                let both_reference_like = (left_type.is_reference_type()
                    || matches!(left_type, Type::Pointer(_)))
                    && (right_type.is_reference_type() || matches!(right_type, Type::Pointer(_)));
                let related = both_reference_like
                    && (self.types_compatible(&left_type, &right_type)
                        || self.types_compatible(&right_type, &left_type));

                if both_numeric
                    || left_type == right_type
                    || involves_generic_param
                    || null_vs_reference
                    || related
                {
                    Ok(Type::Bool)
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        format!(
                            "Cannot compare {} and {} with {:?}: incomparable types",
                            left_type, right_type, bin.op
                        ),
                    ))
                }
            }
            BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                // 关系比较规则：
                // - 数值类型之间可以比较
                // - String 与 String 可以比较
                // - 泛型模板体内的类型参数延迟到单态化后检查
                // - bool、对象（无重载时）等不支持关系比较
                let both_numeric =
                    Self::is_numeric_type_helper(&left_type) && Self::is_numeric_type_helper(&right_type);
                let both_string = left_type == Type::String && right_type == Type::String;
                let involves_generic_param = matches!(left_type, Type::GenericParam(_))
                    || matches!(right_type, Type::GenericParam(_));

                if both_numeric || both_string || involves_generic_param {
                    Ok(Type::Bool)
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        format!(
                            "Cannot compare {} and {} with {:?}: operator requires numeric or string operands",
                            left_type, right_type, bin.op
                        ),
                    ))
                }
            }
            BinaryOp::And | BinaryOp::Or => {
                if left_type == Type::Bool && right_type == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        "Logical operators require boolean operands",
                    ))
                }
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                if left_type.is_integer() && right_type.is_integer() {
                    Ok(self.promote_integer_types(&left_type, &right_type))
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        format!(
                            "Bitwise operator {:?} requires integer operands, got {} and {}",
                            bin.op, left_type, right_type
                        ),
                    ))
                }
            }
            BinaryOp::Shl | BinaryOp::Shr | BinaryOp::UnsignedShr => {
                if left_type.is_integer() && right_type.is_integer() {
                    // 移位运算符的结果类型与左操作数相同（经过整数提升）
                    Ok(self.promote_integer_types(&left_type, &right_type))
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        format!(
                            "Shift operator {:?} requires integer operands, got {} and {}",
                            bin.op, left_type, right_type
                        ),
                    ))
                }
            }
            _ => Ok(left_type),
        }
    }
}
