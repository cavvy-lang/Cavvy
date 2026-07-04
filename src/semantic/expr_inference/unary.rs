//! 一元表达式类型推断

use super::super::analyzer::SemanticAnalyzer;
use super::helpers::semantic_error_at_loc;
use crate::ast::*;
use crate::types::Type;

impl SemanticAnalyzer {
    /// 推断一元表达式类型
    pub(crate) fn infer_unary_type(
        &mut self,
        unary: &UnaryExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        let operand_type = self.infer_expr_type_internal(&unary.operand)?;
        match unary.op {
            UnaryOp::Neg => {
                // 特殊处理：-2147483648 (i32::MIN)
                // 正数 2147483648 超出 i32::MAX，被解析为 Int64，
                // 但取反后 -2147483648 = i32::MIN，应该被视为 Int32
                if let Expr::Literal(lit) = unary.operand.as_ref() {
                    if let LiteralValue::Int64(val) = lit.value {
                        if val == -(i32::MIN as i64) {
                            return Ok(Type::Int32);
                        }
                    }
                }
                Ok(operand_type)
            }
            UnaryOp::Not => {
                if operand_type == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(semantic_error_at_loc(
                        &unary.loc,
                        "Cannot apply '!' to non-boolean",
                    ))
                }
            }
            UnaryOp::BitNot => Ok(operand_type),
            UnaryOp::AddressOf => {
                // &操作符返回指向操作数的指针类型
                // 使用 Type::Pointer 包装操作数类型
                Ok(Type::Pointer(Box::new(operand_type)))
            }
            UnaryOp::Deref => {
                // *操作符解引用指针，返回指针指向的类型
                // 根据操作数类型推断解引用返回类型
                match &operand_type {
                    Type::Pointer(elem_type) => {
                        // 指针类型解引用返回元素类型
                        Ok((**elem_type).clone())
                    }
                    Type::Array(elem_type) => {
                        // 数组类型解引用返回元素类型
                        Ok((**elem_type).clone())
                    }
                    _ => {
                        // 对于其他类型，报错
                        Err(semantic_error_at_loc(
                            &unary.loc,
                            format!("Cannot dereference non-pointer type '{}'", operand_type),
                        ))
                    }
                }
            }
            _ => Ok(operand_type),
        }
    }
}
