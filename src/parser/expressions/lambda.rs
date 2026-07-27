//! Lambda表达式解析
//!
//! 处理Lambda表达式: (params) -> { body } 或 (params) -> expr

use super::super::Parser;
use super::super::statements::parse_statement;
use super::super::types::{is_type_token, parse_type};
use super::assignment::parse_expression;
use crate::ast::*;
use crate::miette_diagnostic::CayResult;

/// 尝试解析 Lambda 表达式
/// 假设已经消耗了 '('，需要解析参数列表和 -> 箭头
pub fn try_parse_lambda(
    parser: &mut Parser,
    loc: crate::miette_diagnostic::SourceLocation,
) -> CayResult<Expr> {
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
        let current_token = super::super::utils::get_token_name(parser.current_token());
        return Err(parser.error(&format!(
            "期望 ')'，但遇到了 {}\n提示: Lambda 参数列表应以 ')' 结束，例如: (x, y) -> x + y",
            current_token
        )));
    }
    parser.advance(); // 跳过 ')'

    // 期望 '->'
    if !parser.check(&crate::lexer::Token::Arrow) {
        let current_token = super::super::utils::get_token_name(parser.current_token());
        return Err(parser.error(&format!(
            "期望 '->'，但遇到了 {}\n提示: Lambda 表达式格式为 (params) -> expr 或 (params) -> {{ body }}",
            current_token
        )));
    }
    parser.advance(); // 跳过 '->'

    // 解析 Lambda 体：可以是表达式或语句块
    let body = if parser.check(&crate::lexer::Token::LBrace) {
        // 语句块: { ... }
        let block_loc = parser.current_loc();
        parser.advance(); // 跳过 '{'
        let mut block = parse_lambda_block(parser, block_loc)?;
        // 6.2.x: 块尾表达式提升为隐式 return
        super::super::statements::promote_tail_to_return(&mut block);
        LambdaBody::Block(block)
    } else {
        // 单表达式
        let expr = parse_expression(parser)?;
        LambdaBody::Expr(Box::new(expr))
    };

    Ok(Expr::Lambda(LambdaExpr { params, body, loc }))
}

/// 解析 Lambda 参数
fn parse_lambda_param(parser: &mut Parser) -> CayResult<LambdaParam> {
    // 6.2.x: 参数后置类型 name: Type（预读：标识符后跟冒号）
    // 与函数参数一致，例如: (x: i32, y: i32) -> x + y
    if let crate::lexer::Token::Identifier(param_name) = parser.current_token().clone() {
        let saved_pos = parser.pos;
        parser.advance(); // 消费标识符
        if parser.check(&crate::lexer::Token::Colon) {
            parser.advance(); // 消费冒号
            let param_type = parser.parse_type_or_fn_ptr()?;
            return Ok(LambdaParam {
                name: param_name,
                param_type: Some(param_type),
            });
        }
        // 不是后置类型格式，回退到类型前置/裸参数名解析
        parser.pos = saved_pos;
    }

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
            Err(parser.error("期望参数名\n提示: 类型后应跟参数名，例如: (int x, int y) -> x + y"))
        }
    } else {
        let current_token = super::super::utils::get_token_name(parser.current_token());
        Err(parser.error(&format!(
            "期望类型或参数名，但遇到了 {}\n\
            提示: Lambda 参数可以是:\
            - 带类型: (int x, int y) -> ...\n\
            - 无类型: (x, y) -> ...",
            current_token
        )))
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
        let current_token = super::super::utils::get_token_name(parser.current_token());
        Err(parser.error(&format!(
            "期望参数名，但遇到了 {}\n提示: Lambda 参数应为标识符，例如: (x, y) -> x + y",
            current_token
        )))
    }
}

/// 解析 Lambda 语句块
/// loc 为 '{' 的真实 token 位置（此前硬编码为 line 0/column 0，会误导错误报告）
fn parse_lambda_block(
    parser: &mut Parser,
    loc: crate::miette_diagnostic::SourceLocation,
) -> CayResult<Block> {
    let mut statements = Vec::new();
    let mut tail_expr = None;

    while !parser.check(&crate::lexer::Token::RBrace) {
        // 6.2.x 块尾表达式：(x) -> { x * 2 } 省略 return
        if let Some(expr) = super::super::statements::try_parse_block_tail(parser)? {
            tail_expr = Some(Box::new(expr));
            break;
        }
        let stmt = parse_statement(parser)?;
        statements.push(stmt);
    }

    parser.consume(
        &crate::lexer::Token::RBrace,
        "期望 '}'\n提示: Lambda 代码块应以 '}' 结束",
    )?;

    Ok(Block {
        statements,
        tail_expr,
        loc,
    })
}
