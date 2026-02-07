//! 表达式解析

use crate::ast::*;
use crate::error::EolResult;
use crate::types::Type;
use super::Parser;
use super::types::{parse_type, is_type_token};

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

/// 解析逻辑或表达式
pub fn parse_or(parser: &mut Parser) -> EolResult<Expr> {
    let mut left = parse_and(parser)?;
    
    while parser.match_token(&crate::lexer::Token::OrOr) {
        let loc = parser.current_loc();
        let right = parse_and(parser)?;
        left = Expr::Binary(BinaryExpr {
            left: Box::new(left),
            op: BinaryOp::Or,
            right: Box::new(right),
            loc,
        });
    }
    
    Ok(left)
}

/// 解析逻辑与表达式
pub fn parse_and(parser: &mut Parser) -> EolResult<Expr> {
    let mut left = parse_bitwise_or(parser)?;
    
    while parser.match_token(&crate::lexer::Token::AndAnd) {
        let loc = parser.current_loc();
        let right = parse_bitwise_or(parser)?;
        left = Expr::Binary(BinaryExpr {
            left: Box::new(left),
            op: BinaryOp::And,
            right: Box::new(right),
            loc,
        });
    }
    
    Ok(left)
}

/// 解析按位或表达式
pub fn parse_bitwise_or(parser: &mut Parser) -> EolResult<Expr> {
    let mut left = parse_bitwise_xor(parser)?;
    
    while parser.match_token(&crate::lexer::Token::Pipe) {
        let loc = parser.current_loc();
        let right = parse_bitwise_xor(parser)?;
        left = Expr::Binary(BinaryExpr {
            left: Box::new(left),
            op: BinaryOp::BitOr,
            right: Box::new(right),
            loc,
        });
    }
    
    Ok(left)
}

/// 解析按位异或表达式
pub fn parse_bitwise_xor(parser: &mut Parser) -> EolResult<Expr> {
    let mut left = parse_bitwise_and(parser)?;
    
    while parser.match_token(&crate::lexer::Token::Caret) {
        let loc = parser.current_loc();
        let right = parse_bitwise_and(parser)?;
        left = Expr::Binary(BinaryExpr {
            left: Box::new(left),
            op: BinaryOp::BitXor,
            right: Box::new(right),
            loc,
        });
    }
    
    Ok(left)
}

/// 解析按位与表达式
pub fn parse_bitwise_and(parser: &mut Parser) -> EolResult<Expr> {
    let mut left = parse_equality(parser)?;
    
    while parser.match_token(&crate::lexer::Token::Ampersand) {
        let loc = parser.current_loc();
        let right = parse_equality(parser)?;
        left = Expr::Binary(BinaryExpr {
            left: Box::new(left),
            op: BinaryOp::BitAnd,
            right: Box::new(right),
            loc,
        });
    }
    
    Ok(left)
}

/// 解析相等性表达式
pub fn parse_equality(parser: &mut Parser) -> EolResult<Expr> {
    let mut left = parse_comparison(parser)?;
    
    loop {
        let loc = parser.current_loc();
        if parser.match_token(&crate::lexer::Token::EqEq) {
            let right = parse_comparison(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Eq,
                right: Box::new(right),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::NotEq) {
            let right = parse_comparison(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Ne,
                right: Box::new(right),
                loc,
            });
        } else {
            break;
        }
    }
    
    Ok(left)
}

/// 解析比较表达式
pub fn parse_comparison(parser: &mut Parser) -> EolResult<Expr> {
    let mut left = parse_shift(parser)?;
    
    loop {
        let loc = parser.current_loc();
        if parser.match_token(&crate::lexer::Token::Lt) {
            let right = parse_shift(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Lt,
                right: Box::new(right),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Le) {
            let right = parse_shift(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Le,
                right: Box::new(right),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Gt) {
            let right = parse_shift(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Gt,
                right: Box::new(right),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Ge) {
            let right = parse_shift(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Ge,
                right: Box::new(right),
                loc,
            });
        } else {
            break;
        }
    }
    
    Ok(left)
}

/// 解析移位表达式
pub fn parse_shift(parser: &mut Parser) -> EolResult<Expr> {
    let mut left = parse_term(parser)?;
    
    loop {
        let loc = parser.current_loc();
        if parser.match_token(&crate::lexer::Token::Shl) {
            let right = parse_term(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Shl,
                right: Box::new(right),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Shr) {
            let right = parse_term(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Shr,
                right: Box::new(right),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::UnsignedShr) {
            let right = parse_term(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::UnsignedShr,
                right: Box::new(right),
                loc,
            });
        } else {
            break;
        }
    }
    
    Ok(left)
}

/// 解析加减表达式
pub fn parse_term(parser: &mut Parser) -> EolResult<Expr> {
    let mut left = parse_factor(parser)?;
    
    loop {
        let loc = parser.current_loc();
        if parser.match_token(&crate::lexer::Token::Plus) {
            let right = parse_factor(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Add,
                right: Box::new(right),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Minus) {
            let right = parse_factor(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Sub,
                right: Box::new(right),
                loc,
            });
        } else {
            break;
        }
    }
    
    Ok(left)
}

/// 解析乘除模表达式
pub fn parse_factor(parser: &mut Parser) -> EolResult<Expr> {
    let mut left = parse_unary(parser)?;
    
    loop {
        let loc = parser.current_loc();
        if parser.match_token(&crate::lexer::Token::Star) {
            let right = parse_unary(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Mul,
                right: Box::new(right),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Slash) {
            let right = parse_unary(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Div,
                right: Box::new(right),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Percent) {
            let right = parse_unary(parser)?;
            left = Expr::Binary(BinaryExpr {
                left: Box::new(left),
                op: BinaryOp::Mod,
                right: Box::new(right),
                loc,
            });
        } else {
            break;
        }
    }
    
    Ok(left)
}

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
            Ok(Expr::Identifier(name))
        }
        crate::lexer::Token::New => {
            parser.advance();
            parse_new_expression(parser, loc)
        }
        crate::lexer::Token::LParen => {
            parser.advance();
            let expr = parse_expression(parser)?;
            parser.consume(&crate::lexer::Token::RParen, "Expected ')' after expression")?;
            Ok(expr)
        }
        _ => Err(parser.error("Expected expression")),
    }
}

/// 解析 new 表达式（支持类创建和数组创建）
fn parse_new_expression(parser: &mut Parser, loc: crate::error::SourceLocation) -> EolResult<Expr> {
    // 首先尝试解析类型
    if is_type_token(parser) {
        // 解析基本类型（不包含数组维度）
        let element_type = parse_base_type(parser)?;
        
        // 检查是否是数组创建: new Type[size]
        if parser.match_token(&crate::lexer::Token::LBracket) {
            // 解析数组大小表达式
            let size = parse_expression(parser)?;
            parser.consume(&crate::lexer::Token::RBracket, "Expected ']' after array size")?;
            return Ok(Expr::ArrayCreation(ArrayCreationExpr {
                element_type,
                size: Box::new(size),
                loc,
            }));
        }
        
        // 不是数组，回退作为普通类创建（但类型已经消耗了，需要特殊处理）
        // 这里 Type 不能作为类名，所以我们需要报错
        return Err(parser.error("Expected '[' after type in array creation, or use 'new ClassName()' for object creation"));
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
fn parse_base_type(parser: &mut Parser) -> EolResult<Type> {
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
