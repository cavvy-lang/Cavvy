//! Lambda表达式解析
//!
//! 处理Lambda表达式: (params) -> { body } 或 (params) -> expr

use crate::ast::*;
use crate::error::EolResult;
use super::super::Parser;
use super::super::types::{parse_type, is_type_token};
use super::super::statements::parse_statement;
use super::assignment::parse_expression;

/// 尝试解析 Lambda 表达式
/// 假设已经消耗了 '('，需要解析参数列表和 -> 箭头
pub fn try_parse_lambda(parser: &mut Parser, loc: crate::error::SourceLocation) -> EolResult<Expr> {
    // 解析 Lambda 参数列表: (param1, param2, ...) 或 (int x, int y) 或 ()
    let mut params = Vec::new();

    if !parser.check(&crate::lexer::Token::RParen) {
        loop {
            // 尝试解析参数（可能有类型注解）
            let param = parse_lambda_param(parser)?;
            params.push(param);

            if !parser.match_token(&crate::lexer::Token::Comma) {
                break;
            }
        }
    }

    // 期望 ')'
    if !parser.check(&crate::lexer::Token::RParen) {
        return Err(parser.error("Expected ')' after lambda parameters"));
    }
    parser.advance(); // 跳过 ')'

    // 期望 '->'
    if !parser.check(&crate::lexer::Token::Arrow) {
        return Err(parser.error("Expected '->' after lambda parameters"));
    }
    parser.advance(); // 跳过 '->'

    // 解析 Lambda 体：可以是表达式或语句块
    let body = if parser.check(&crate::lexer::Token::LBrace) {
        // 语句块: { ... }
        parser.advance(); // 跳过 '{'
        let block = parse_lambda_block(parser)?;
        LambdaBody::Block(block)
    } else {
        // 单表达式
        let expr = parse_expression(parser)?;
        LambdaBody::Expr(Box::new(expr))
    };

    Ok(Expr::Lambda(LambdaExpr {
        params,
        body,
        loc,
    }))
}

/// 解析 Lambda 参数
fn parse_lambda_param(parser: &mut Parser) -> EolResult<LambdaParam> {
    // 检查是否有类型注解（可选）
    let checkpoint = parser.pos;

    // 尝试解析类型
    let type_result = if is_type_token(parser) {
        let ty = parse_type(parser)?;
        // 类型后面必须跟着标识符
        if let crate::lexer::Token::Identifier(name) = parser.current_token() {
            let name = name.clone();
            parser.advance();
            Ok(LambdaParam {
                name,
                param_type: Some(ty),
            })
        } else {
            // 类型后面没有标识符，回退
            parser.pos = checkpoint;
            Err(parser.error("Expected parameter name after type"))
        }
    } else {
        Err(parser.error("Expected type or parameter name"))
    };

    if let Ok(param) = type_result {
        return Ok(param);
    }

    // 没有类型注解，只有参数名
    if let crate::lexer::Token::Identifier(name) = parser.current_token() {
        let name = name.clone();
        parser.advance();
        Ok(LambdaParam {
            name,
            param_type: None,
        })
    } else {
        Err(parser.error("Expected parameter name"))
    }
}

/// 解析 Lambda 语句块
fn parse_lambda_block(parser: &mut Parser) -> EolResult<Block> {
    let mut statements = Vec::new();

    while !parser.check(&crate::lexer::Token::RBrace) {
        let stmt = parse_statement(parser)?;
        statements.push(stmt);
    }

    parser.consume(&crate::lexer::Token::RBrace, "Expected '}' after lambda block")?;

    Ok(Block {
        statements,
        loc: crate::error::SourceLocation { line: 0, column: 0 },
    })
}
