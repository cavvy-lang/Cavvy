//! 赋值表达式解析
//!
//! 处理赋值表达式和作为入口点的表达式解析。

use super::super::Parser;
use super::binary::parse_or;
use crate::ast::*;
use crate::miette_diagnostic::CayResult;

/// 解析表达式（入口点）
pub fn parse_expression(parser: &mut Parser) -> CayResult<Expr> {
    parse_assignment(parser)
}

/// 解析赋值表达式
pub fn parse_assignment(parser: &mut Parser) -> CayResult<Expr> {
    let loc = parser.current_loc();
    // 先尝试解析三元运算符，它的优先级低于赋值
    let expr = parse_ternary(parser)?;

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

/// 解析三元运算符表达式: condition ? true_expr : false_expr
///
/// 所有表达式递归循环（括号嵌套、调用参数、数组元素、赋值/三元右递归等）
/// 都会经过本层，因此在这里做递归深度计数，防止病态嵌套输入导致栈溢出。
fn parse_ternary(parser: &mut Parser) -> CayResult<Expr> {
    parser.enter_expr_recursion()?;
    let result = parse_ternary_inner(parser);
    parser.leave_expr_recursion();
    result
}

/// 解析三元运算符表达式（内部实现）
fn parse_ternary_inner(parser: &mut Parser) -> CayResult<Expr> {
    let loc = parser.current_loc();
    let condition = parse_or(parser)?;

    // 检查是否有 ? 标记
    if parser.match_token(&crate::lexer::Token::Question) {
        let true_branch = Box::new(parse_or(parser)?);
        parser.consume(
            &crate::lexer::Token::Colon,
            "期望 ':'\n提示: 三元运算符格式为 condition ? true_expr : false_expr",
        )?;
        let false_branch = Box::new(parse_ternary(parser)?); // 右结合

        return Ok(Expr::Ternary(TernaryExpr {
            condition: Box::new(condition),
            true_branch,
            false_branch,
            loc,
        }));
    }

    Ok(condition)
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
