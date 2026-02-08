//! 类相关解析

use crate::ast::*;
use crate::types::{Type, ParameterInfo};
use crate::error::EolResult;
use crate::lexer::Token;
use crate::error::SourceLocation;
use super::Parser;
use super::types::{parse_type, is_type_token};
use super::expressions::parse_expression;
use super::statements::parse_block;

/// 解析类声明
pub fn parse_class(parser: &mut Parser) -> EolResult<ClassDecl> {
    let loc = parser.current_loc();
    let modifiers = parse_modifiers(parser)?;
    
    parser.consume(&Token::Class, "Expected 'class' keyword")?;
    
    let name = parser.consume_identifier("Expected class name")?;
    
    let parent = if parser.match_token(&Token::Colon) {
        Some(parser.consume_identifier("Expected parent class name")?)
    } else {
        None
    };
    
    parser.consume(&Token::LBrace, "Expected '{' after class declaration")?;
    
    let mut members = Vec::new();
    while !parser.check(&Token::RBrace) && !parser.is_at_end() {
        members.push(parse_class_member(parser)?);
    }
    
    parser.consume(&Token::RBrace, "Expected '}' after class body")?;
    
    Ok(ClassDecl {
        name,
        modifiers,
        parent,
        members,
        loc,
    })
}

/// 解析类成员（字段或方法）
pub fn parse_class_member(parser: &mut Parser) -> EolResult<ClassMember> {
    // 向前看判断是字段或方法
    let checkpoint = parser.pos;
    let _modifiers = parse_modifiers(parser)?;
    
    // 如果是void，一定是方法返回类型
    if parser.check(&Token::Void) {
        parser.pos = checkpoint;
        return Ok(ClassMember::Method(parse_method(parser)?));
    }
    
    // 如果是类型关键字，可能是字段或方法
    if is_type_token(parser) {
        // 读取类型
        let _type = parse_type(parser)?;
        let _name = parser.consume_identifier("Expected member name")?;
        
        if parser.check(&Token::LParen) {
            // 是方法
            parser.pos = checkpoint;
            Ok(ClassMember::Method(parse_method(parser)?))
        } else {
            // 是字段
            parser.pos = checkpoint;
            Ok(ClassMember::Field(parse_field(parser)?))
        }
    } else {
        Err(parser.error("Expected field or method declaration"))
    }
}

/// 解析字段声明
pub fn parse_field(parser: &mut Parser) -> EolResult<FieldDecl> {
    let loc = parser.current_loc();
    let modifiers = parse_modifiers(parser)?;
    let field_type = parse_type(parser)?;
    let name = parser.consume_identifier("Expected field name")?;
    
    let initializer = if parser.match_token(&Token::Assign) {
        Some(parse_expression(parser)?)
    } else {
        None
    };
    
    parser.consume(&Token::Semicolon, "Expected ';' after field declaration")?;
    
    Ok(FieldDecl {
        name,
        field_type,
        modifiers,
        initializer,
        loc,
    })
}

/// 解析方法声明
pub fn parse_method(parser: &mut Parser) -> EolResult<MethodDecl> {
    let loc = parser.current_loc();
    let modifiers = parse_modifiers(parser)?;
    
    let return_type = if parser.check(&Token::Void) {
        parser.advance();
        Type::Void
    } else {
        parse_type(parser)?
    };
    
    let name = parser.consume_identifier("Expected method name")?;
    
    parser.consume(&Token::LParen, "Expected '(' after method name")?;
    let params = parse_parameters(parser)?;
    parser.consume(&Token::RParen, "Expected ')' after parameters")?;
    
    // 检查是否是native方法
    let is_native = modifiers.contains(&Modifier::Native);
    
    let body = if is_native {
        parser.consume(&Token::Semicolon, "Expected ';' after native method declaration")?;
        None
    } else {
        Some(parse_block(parser)?)
    };
    
    Ok(MethodDecl {
        name,
        modifiers,
        return_type,
        params,
        body,
        loc,
    })
}

/// 解析修饰符列表
pub fn parse_modifiers(parser: &mut Parser) -> EolResult<Vec<Modifier>> {
    let mut modifiers = Vec::new();
    
    loop {
        match parser.current_token() {
            Token::Public => {
                modifiers.push(Modifier::Public);
                parser.advance();
            }
            Token::Private => {
                modifiers.push(Modifier::Private);
                parser.advance();
            }
            Token::Protected => {
                modifiers.push(Modifier::Protected);
                parser.advance();
            }
            Token::Static => {
                modifiers.push(Modifier::Static);
                parser.advance();
            }
            Token::Final => {
                modifiers.push(Modifier::Final);
                parser.advance();
            }
            Token::Abstract => {
                modifiers.push(Modifier::Abstract);
                parser.advance();
            }
            Token::Native => {
                modifiers.push(Modifier::Native);
                parser.advance();
            }
            _ => break,
        }
    }
    
    Ok(modifiers)
}

/// 解析参数列表（支持可变参数）
pub fn parse_parameters(parser: &mut Parser) -> EolResult<Vec<ParameterInfo>> {
    let mut params = Vec::new();

    if !parser.check(&Token::RParen) {
        loop {
            // 检查是否是可变参数类型（type...）
            let param_type = parse_type(parser)?;

            // 检查是否有 ... 标记
            let is_varargs = parser.match_token(&Token::DotDotDot);

            let name = parser.consume_identifier("Expected parameter name")?;

            if is_varargs {
                params.push(ParameterInfo::new_varargs(name, param_type));
                // 可变参数必须是最后一个参数
                if parser.match_token(&Token::Comma) {
                    return Err(parser.error("Varargs parameter must be the last parameter"));
                }
                break;
            } else {
                params.push(ParameterInfo::new(name, param_type));
            }

            if !parser.match_token(&Token::Comma) {
                break;
            }
        }
    }

    Ok(params)
}