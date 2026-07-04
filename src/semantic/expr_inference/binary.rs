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

                // 字符串连接：支持 String + String 和 String + char
                if left_type == Type::String && right_type == Type::String {
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
                if left_type.is_primitive() && right_type.is_primitive() {
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
            BinaryOp::Eq
            | BinaryOp::Ne
            | BinaryOp::Lt
            | BinaryOp::Le
            | BinaryOp::Gt
            | BinaryOp::Ge => Ok(Type::Bool),
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
