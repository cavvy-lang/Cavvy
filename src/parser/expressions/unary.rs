//! 一元表达式解析
//!
//! 处理一元运算符（-、!、~）和类型转换表达式。

use crate::ast::*;
use crate::error::EolResult;
use super::super::Parser;
use super::super::types::{parse_type, is_type_token};
use super::postfix::parse_postfix;

/// 解析一元表达式（包括类型转换）
pub fn parse_unary(parser: &mut Parser) -> EolResult<Expr> {
    let loc = parser.current_loc();

    if parser.match_token(&crate::lexer::Token::Minus) {
        let operand = parse_unary(parser)?;
        return Ok(Expr::Unary(UnaryExpr {
            op: UnaryOp::Neg,
            operand: Box::new(operand),
            loc,
        }));
    }

    if parser.match_token(&crate::lexer::Token::Bang) {
        let operand = parse_unary(parser)?;
        return Ok(Expr::Unary(UnaryExpr {
            op: UnaryOp::Not,
            operand: Box::new(operand),
            loc,
        }));
    }

    if parser.match_token(&crate::lexer::Token::Tilde) {
        let operand = parse_unary(parser)?;
        return Ok(Expr::Unary(UnaryExpr {
            op: UnaryOp::BitNot,
            operand: Box::new(operand),
            loc,
        }));
    }

    // 尝试解析类型转换 (type) expr
    if parser.check(&crate::lexer::Token::LParen) {
        let checkpoint = parser.pos;
        let loc = parser.current_loc();

        // 尝试解析 ( type )
        parser.advance(); // 跳过 LParen

        // 检查是否是类型关键字
        if is_type_token(parser) {
            // 解析类型
            match parse_type(parser) {
                Ok(target_type) => {
                    // 期望 RParen
                    if parser.check(&crate::lexer::Token::RParen) {
                        parser.advance();
                        // 成功解析类型转换，解析后面的表达式
                        let expr = parse_unary(parser)?;
                        return Ok(Expr::Cast(CastExpr {
                            expr: Box::new(expr),
                            target_type,
                            loc,
                        }));
                    } else {
                        // 没有 RParen，回退
                        parser.pos = checkpoint;
                    }
                }
                Err(_) => {
                    // 解析类型失败，回退
                    parser.pos = checkpoint;
                }
            }
        } else {
            // 不是类型，回退
            parser.pos = checkpoint;
        }
    }

    parse_postfix(parser)
}
