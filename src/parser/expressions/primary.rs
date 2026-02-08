//! 基本表达式解析
//!
//! 处理字面量、标识符、括号表达式、new表达式等基本表达式。

use crate::ast::*;
use crate::types::Type;
use crate::error::EolResult;
use super::super::Parser;
use super::super::types::is_type_token;
use super::lambda::try_parse_lambda;
use super::assignment::parse_expression;

/// 解析基本表达式
pub fn parse_primary(parser: &mut Parser) -> EolResult<Expr> {
    let loc = parser.current_loc();

    let token = parser.current_token().clone();
    match token {
        crate::lexer::Token::IntegerLiteral(Some((val, suffix))) => {
            parser.advance();
            let lit = match suffix {
                Some('L') | Some('l') => LiteralValue::Int64(val),
                None => {
                    // 默认整数字面量类型为 int32，但如果值超出范围，则视为 int64？
                    if val >= i32::MIN as i64 && val <= i32::MAX as i64 {
                        LiteralValue::Int32(val as i32)
                    } else {
                        LiteralValue::Int64(val)
                    }
                }
                _ => unreachable!(),
            };
            Ok(Expr::Literal(lit))
        }
        crate::lexer::Token::FloatLiteral(Some((val, suffix))) => {
            parser.advance();
            let lit = match suffix {
                Some('f') | Some('F') => LiteralValue::Float32(val as f32),
                Some('d') | Some('D') | None => LiteralValue::Float64(val),
                _ => unreachable!(),
            };
            Ok(Expr::Literal(lit))
        }
        crate::lexer::Token::StringLiteral(s) => {
            parser.advance();
            Ok(Expr::Literal(LiteralValue::String(s)))
        }
        crate::lexer::Token::CharLiteral(Some(c)) => {
            parser.advance();
            Ok(Expr::Literal(LiteralValue::Char(c)))
        }
        crate::lexer::Token::True => {
            parser.advance();
            Ok(Expr::Literal(LiteralValue::Bool(true)))
        }
        crate::lexer::Token::False => {
            parser.advance();
            Ok(Expr::Literal(LiteralValue::Bool(false)))
        }
        crate::lexer::Token::Null => {
            parser.advance();
            Ok(Expr::Literal(LiteralValue::Null))
        }
        crate::lexer::Token::Identifier(name) => {
            let name = name.clone();
            parser.advance();

            // 检查是否是方法引用: ClassName::methodName
            if parser.match_token(&crate::lexer::Token::DoubleColon) {
                let method_name = parser.consume_identifier("Expected method name after '::'")?;
                return Ok(Expr::MethodRef(MethodRefExpr {
                    class_name: Some(name),
                    object: None,
                    method_name,
                    loc,
                }));
            }

            Ok(Expr::Identifier(name))
        }
        crate::lexer::Token::New => {
            parser.advance();
            parse_new_expression(parser, loc)
        }
        crate::lexer::Token::LParen => {
            // 检查是否是 Lambda 表达式: (params) -> { body }
            // 需要向前看，检查是否有 -> 箭头
            let checkpoint = parser.pos;
            parser.advance(); // 跳过 '('

            // 尝试解析 Lambda 参数列表
            if let Ok(lambda_expr) = try_parse_lambda(parser, loc.clone()) {
                return Ok(lambda_expr);
            }

            // 不是 Lambda，回退并解析普通括号表达式
            parser.pos = checkpoint;
            parser.advance(); // 跳过 '('
            let expr = parse_expression(parser)?;
            parser.consume(&crate::lexer::Token::RParen, "Expected ')' after expression")?;
            Ok(expr)
        }
        _ => Err(parser.error("Expected expression")),
    }
}

/// 解析 new 表达式（支持类创建和多维数组创建）
pub fn parse_new_expression(parser: &mut Parser, loc: crate::error::SourceLocation) -> EolResult<Expr> {
    // 首先尝试解析类型
    if is_type_token(parser) {
        // 解析基本类型或类名（不包含数组维度）
        let base_element_type = parse_base_type(parser)?;

        // 如果接下来是 '[' 则为数组创建: new Type[size] 或 new Type[size1][size2]...
        if parser.check(&crate::lexer::Token::LBracket) {
            let mut sizes = Vec::new();

            // 解析所有维度: [size1][size2]...
            while parser.match_token(&crate::lexer::Token::LBracket) {
                let size = parse_expression(parser)?;
                parser.consume(&crate::lexer::Token::RBracket, "Expected ']' after array size")?;
                sizes.push(size);
            }

            // 构建多维元素类型: base_type[][]...
            let mut element_type = base_element_type;
            for _ in 1..sizes.len() {
                element_type = Type::Array(Box::new(element_type));
            }

            // 检查是否有 () 零初始化后缀
            let zero_init = parser.match_token(&crate::lexer::Token::LParen)
                && parser.match_token(&crate::lexer::Token::RParen);

            return Ok(Expr::ArrayCreation(ArrayCreationExpr {
                element_type,
                sizes,
                zero_init,
                loc,
            }));
        }

        // 如果接下来是 '(' 则为对象创建: new ClassName(...)
        if parser.match_token(&crate::lexer::Token::LParen) {
            // element_type should be Type::Object(name)
            match base_element_type {
                crate::types::Type::Object(name) => {
                    let args = parse_arguments(parser)?;
                    parser.consume(&crate::lexer::Token::RParen, "Expected ')' after arguments")?;
                    return Ok(Expr::New(NewExpr { class_name: name, args, loc }));
                }
                _ => {
                    return Err(parser.error("Only object types can be constructed with 'new Type()'"));
                }
            }
        }

        // 否则既不是数组也不是对象构造，报错
        return Err(parser.error("Expected '[' for array creation or '(' for object creation after type"));
    }

    // 普通类创建: new ClassName()
    let class_name = parser.consume_identifier("Expected class name or type after 'new'")?;
    parser.consume(&crate::lexer::Token::LParen, "Expected '(' after class name")?;
    let args = parse_arguments(parser)?;
    parser.consume(&crate::lexer::Token::RParen, "Expected ')' after arguments")?;
    Ok(Expr::New(NewExpr {
        class_name,
        args,
        loc,
    }))
}

/// 解析基本类型（不包含数组维度）
pub fn parse_base_type(parser: &mut Parser) -> EolResult<Type> {
    match parser.current_token() {
        crate::lexer::Token::Int => { parser.advance(); Ok(Type::Int32) }
        crate::lexer::Token::Long => { parser.advance(); Ok(Type::Int64) }
        crate::lexer::Token::Float => { parser.advance(); Ok(Type::Float32) }
        crate::lexer::Token::Double => { parser.advance(); Ok(Type::Float64) }
        crate::lexer::Token::Bool => { parser.advance(); Ok(Type::Bool) }
        crate::lexer::Token::String => { parser.advance(); Ok(Type::String) }
        crate::lexer::Token::Char => { parser.advance(); Ok(Type::Char) }
        crate::lexer::Token::Identifier(name) => {
            let name = name.clone();
            parser.advance();
            Ok(Type::Object(name))
        }
        _ => Err(parser.error("Expected type")),
    }
}

/// 解析参数列表
fn parse_arguments(parser: &mut Parser) -> EolResult<Vec<Expr>> {
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
