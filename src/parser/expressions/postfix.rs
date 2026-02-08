//! 后缀表达式解析
//!
//! 处理函数调用、成员访问、数组索引等后缀表达式。

use crate::ast::*;
use crate::error::EolResult;
use super::super::Parser;
use super::primary::parse_primary;
use super::assignment::parse_expression;

/// 解析后缀表达式
pub fn parse_postfix(parser: &mut Parser) -> EolResult<Expr> {
    let mut expr = parse_primary(parser)?;

    loop {
        let loc = parser.current_loc();
        if parser.match_token(&crate::lexer::Token::LParen) {
            // 函数调用
            let args = parse_arguments(parser)?;
            parser.consume(&crate::lexer::Token::RParen, "Expected ')' after arguments")?;
            expr = Expr::Call(CallExpr {
                callee: Box::new(expr),
                args,
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Dot) {
            // 成员访问
            let member = parser.consume_identifier("Expected member name after '.'")?;
            expr = Expr::MemberAccess(MemberAccessExpr {
                object: Box::new(expr),
                member,
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::LBracket) {
            // 数组索引访问: arr[index]
            let index = parse_expression(parser)?;
            parser.consume(&crate::lexer::Token::RBracket, "Expected ']' after index")?;
            expr = Expr::ArrayAccess(ArrayAccessExpr {
                array: Box::new(expr),
                index: Box::new(index),
                loc,
            });
        } else {
            break;
        }
    }

    Ok(expr)
}

/// 解析参数列表
pub fn parse_arguments(parser: &mut Parser) -> EolResult<Vec<Expr>> {
    let mut args = Vec::new();

    if !parser.check(&crate::lexer::Token::RParen) {
        loop {
            args.push(parse_expression(parser)?);
            if !parser.match_token(&crate::lexer::Token::Comma) {
                break;
            }
        }
    }

    Ok(args)
}
