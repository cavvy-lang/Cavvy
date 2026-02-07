//! 类型解析

use crate::types::Type;
use crate::error::EolResult;
use super::Parser;

/// 解析类型（支持多维数组）
pub fn parse_type(parser: &mut Parser) -> EolResult<Type> {
    let base_type = match parser.current_token() {
        crate::lexer::Token::Int => { parser.advance(); Type::Int32 }
        crate::lexer::Token::Long => { parser.advance(); Type::Int64 }
        crate::lexer::Token::Float => { parser.advance(); Type::Float32 }
        crate::lexer::Token::Double => { parser.advance(); Type::Float64 }
        crate::lexer::Token::Bool => { parser.advance(); Type::Bool }
        crate::lexer::Token::String => { parser.advance(); Type::String }
        crate::lexer::Token::Char => { parser.advance(); Type::Char }
        crate::lexer::Token::Identifier(name) => {
            let name = name.clone();
            parser.advance();
            Type::Object(name)
        }
        _ => return Err(parser.error("Expected type")),
    };
    
    // 检查多维数组类型 Type[][]...
    let mut result_type = base_type;
    while parser.match_token(&crate::lexer::Token::LBracket) {
        parser.consume(&crate::lexer::Token::RBracket, "Expected ']' after '['")?;
        result_type = Type::Array(Box::new(result_type));
    }
    
    Ok(result_type)
}

/// 检查当前token是否是类型token
pub fn is_type_token(parser: &Parser) -> bool {
    matches!(parser.current_token(),
        crate::lexer::Token::Int | crate::lexer::Token::Long | crate::lexer::Token::Float |
        crate::lexer::Token::Double | crate::lexer::Token::Bool | crate::lexer::Token::String |
        crate::lexer::Token::Char | crate::lexer::Token::Identifier(_)
    )
}

/// 检查当前token是否是原始类型token
pub fn is_primitive_type_token(parser: &Parser) -> bool {
    matches!(parser.current_token(),
        crate::lexer::Token::Int | crate::lexer::Token::Long | crate::lexer::Token::Float |
        crate::lexer::Token::Double | crate::lexer::Token::Bool | crate::lexer::Token::String |
        crate::lexer::Token::Char
    )
}