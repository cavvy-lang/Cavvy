//! 赋值表达式解析
//!
//! 处理赋值表达式和作为入口点的表达式解析。

use crate::ast::*;
use crate::error::EolResult;
use super::super::Parser;
use super::binary::parse_or;

/// 解析表达式（入口点）
pub fn parse_expression(parser: &mut Parser) -> EolResult<Expr> {
    parse_assignment(parser)
}

/// 解析赋值表达式
pub fn parse_assignment(parser: &mut Parser) -> EolResult<Expr> {
    let loc = parser.current_loc();
    let expr = parse_or(parser)?;

    if let Some(op) = match_assignment_op(parser) {
        let value = parse_assignment(parser)?;
        return Ok(Expr::Assignment(AssignmentExpr {
            target: Box::new(expr),
            value: Box::new(value),
            op,
            loc,
        }));
    }

    Ok(expr)
}

/// 匹配赋值操作符
pub fn match_assignment_op(parser: &mut Parser) -> Option<AssignOp> {
    if parser.check(&crate::lexer::Token::Assign) {
        parser.advance();
        Some(AssignOp::Assign)
    } else if parser.check(&crate::lexer::Token::AddAssign) {
        parser.advance();
        Some(AssignOp::AddAssign)
    } else if parser.check(&crate::lexer::Token::SubAssign) {
        parser.advance();
        Some(AssignOp::SubAssign)
    } else if parser.check(&crate::lexer::Token::MulAssign) {
        parser.advance();
        Some(AssignOp::MulAssign)
    } else if parser.check(&crate::lexer::Token::DivAssign) {
        parser.advance();
        Some(AssignOp::DivAssign)
    } else if parser.check(&crate::lexer::Token::ModAssign) {
        parser.advance();
        Some(AssignOp::ModAssign)
    } else {
        None
    }
}
