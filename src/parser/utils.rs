//! 解析器辅助方法
//!
//! 提供语法分析器的通用工具函数和增强的错误处理

use super::Parser;
use crate::lexer::{Token, TokenWithLocation};
use crate::miette_diagnostic::{CayError, CayResult, ErrorCodes};

/// 检查是否到达令牌流末尾
pub fn is_at_end(parser: &Parser) -> bool {
    parser.pos >= parser.tokens.len()
}

/// 获取当前令牌
/// 当已到达令牌流末尾时，返回最后一个有效令牌（用于错误报告）
pub fn current_token(parser: &Parser) -> &Token {
    let idx = if parser.pos >= parser.tokens.len() {
        parser.tokens.len().saturating_sub(1)
    } else {
        parser.pos
    };
    &parser.tokens[idx].token
}

/// 获取当前完整位置（包含源文件信息，使用预处理行号）
/// 当已到达令牌流末尾时，返回最后一个有效令牌的位置
pub fn current_full_loc(parser: &Parser) -> crate::miette_diagnostic::SourceLocation {
    let token = if parser.pos >= parser.tokens.len() && !parser.tokens.is_empty() {
        &parser.tokens[parser.tokens.len() - 1]
    } else if parser.pos < parser.tokens.len() {
        &parser.tokens[parser.pos]
    } else {
        // 空令牌流，返回默认位置
        return crate::miette_diagnostic::SourceLocation::default();
    };
    crate::miette_diagnostic::SourceLocation {
        file: token.source_file.clone(),
        line: token.loc.line, // 使用预处理后的行号，让语义分析器来映射
        column: token.loc.column,
    }
}

/// 获取上一个完整位置（包含源文件信息，使用预处理行号）
pub fn previous_full_loc(parser: &Parser) -> crate::miette_diagnostic::SourceLocation {
    let token = if parser.pos > 0 {
        &parser.tokens[parser.pos - 1]
    } else {
        &parser.tokens[0]
    };
    crate::miette_diagnostic::SourceLocation {
        file: token.source_file.clone(),
        line: token.loc.line, // 使用预处理后的行号，让语义分析器来映射
        column: token.loc.column,
    }
}

/// 获取当前位置（向后兼容）
/// 使用预处理后的行号（loc.line），语义分析器负责映射到原始源文件
/// 当已到达令牌流末尾时，返回最后一个有效令牌的位置
pub fn current_loc(parser: &Parser) -> crate::miette_diagnostic::SourceLocation {
    let token = if parser.pos >= parser.tokens.len() && !parser.tokens.is_empty() {
        &parser.tokens[parser.tokens.len() - 1]
    } else if parser.pos < parser.tokens.len() {
        &parser.tokens[parser.pos]
    } else {
        // 空令牌流，返回默认位置
        return crate::miette_diagnostic::SourceLocation::default();
    };
    crate::miette_diagnostic::SourceLocation {
        file: token.source_file.clone(),
        line: token.loc.line,
        column: token.loc.column,
    }
}

/// 获取上一个位置（向后兼容）
/// 使用 source_line（原始源文件行号）而不是 loc.line（预处理后的行号）
pub fn previous_loc(parser: &Parser) -> crate::miette_diagnostic::SourceLocation {
    let token = if parser.pos > 0 {
        &parser.tokens[parser.pos - 1]
    } else {
        &parser.tokens[0]
    };
    crate::miette_diagnostic::SourceLocation {
        file: token.source_file.clone(),
        line: token.source_line.unwrap_or(token.loc.line),
        column: token.loc.column,
    }
}

/// 前进到下一个令牌
pub fn advance(parser: &mut Parser) -> &Token {
    if !is_at_end(parser) {
        parser.pos += 1;
    }
    &parser.tokens[parser.pos - 1].token
}

/// 检查当前令牌是否匹配给定令牌
pub fn check(parser: &Parser, token: &Token) -> bool {
    if is_at_end(parser) {
        false
    } else {
        current_token(parser) == token
    }
}

/// 如果匹配则消耗令牌
pub fn match_token(parser: &mut Parser, token: &Token) -> bool {
    if check(parser, token) {
        advance(parser);
        true
    } else {
        false
    }
}

/// 消耗指定令牌，否则报错（增强版，带详细错误信息）
pub fn consume<'a>(parser: &'a mut Parser, token: &Token, message: &str) -> CayResult<&'a Token> {
    if check(parser, token) {
        Ok(advance(parser))
    } else {
        // 如果期望分号但没找到，使用上一个token的位置
        let loc = if message.contains("';'") {
            previous_full_loc(parser)
        } else {
            current_full_loc(parser)
        };

        // 创建详细的错误信息
        let (error_code, detailed_message, suggestion) = match token {
            Token::Semicolon => (
                ErrorCodes::PARSER_EXPECTED_SEMICOLON,
                format!(
                    "期望分号 ';'，但找到 '{}'",
                    get_token_name(current_token(parser))
                ),
                "在语句末尾添加分号 ';'".to_string(),
            ),
            Token::LBrace => (
                ErrorCodes::PARSER_EXPECTED_BRACE,
                format!(
                    "期望左大括号 '{{'，但找到 '{}'",
                    get_token_name(current_token(parser))
                ),
                "在代码块开始处添加 '{{'".to_string(),
            ),
            Token::RBrace => (
                ErrorCodes::PARSER_EXPECTED_BRACE,
                format!(
                    "期望右大括号 '}}'，但找到 '{}'",
                    get_token_name(current_token(parser))
                ),
                "在代码块结束处添加 '}}'".to_string(),
            ),
            Token::LParen => (
                ErrorCodes::PARSER_EXPECTED_PAREN,
                format!(
                    "期望左括号 '('，但找到 '{}'",
                    get_token_name(current_token(parser))
                ),
                "在表达式或参数列表开始处添加 '('".to_string(),
            ),
            Token::RParen => (
                ErrorCodes::PARSER_EXPECTED_PAREN,
                format!(
                    "期望右括号 ')'，但找到 '{}'",
                    get_token_name(current_token(parser))
                ),
                "在表达式或参数列表结束处添加 ')'".to_string(),
            ),
            _ => (
                ErrorCodes::PARSER_UNEXPECTED_TOKEN,
                message.to_string(),
                format!("期望 '{}'", get_token_name(token)),
            ),
        };

        // 添加到诊断收集器
        let error = CayError::Parser {
            error_code,
            file: loc.file.clone(),
            line: loc.line,
            column: loc.column,
            message: detailed_message.clone(),
            suggestion,
        };

        parser.diagnostics.push(error.clone());

        Err(error)
    }
}

/// 消耗标识符
pub fn consume_identifier(parser: &mut Parser, message: &str) -> CayResult<String> {
    if let Token::Identifier(name) = current_token(parser) {
        let name = name.clone();
        advance(parser);
        Ok(name)
    } else {
        let loc = current_full_loc(parser);
        let actual = get_token_name(current_token(parser));
        let detailed_message = format!("期望标识符，但找到 '{}'", actual);

        let error = CayError::Parser {
            error_code: ErrorCodes::PARSER_EXPECTED_IDENTIFIER,
            file: loc.file.clone(),
            line: loc.line,
            column: loc.column,
            message: detailed_message.clone(),
            suggestion: "使用有效的标识符名称（以字母或下划线开头）".to_string(),
        };

        parser.diagnostics.push(error.clone());

        Err(error)
    }
}

/// 创建错误
pub fn error(parser: &Parser, message: &str) -> CayError {
    let loc = current_full_loc(parser);

    CayError::Parser {
        error_code: ErrorCodes::PARSER_UNEXPECTED_TOKEN,
        file: loc.file.clone(),
        line: loc.line,
        column: loc.column,
        message: message.to_string(),
        suggestion: ErrorCodes::get_suggestion(ErrorCodes::PARSER_UNEXPECTED_TOKEN).to_string(),
    }
}

/// 创建详细的语法错误
pub fn create_parser_error(
    parser: &mut Parser,
    error_code: &'static str,
    message: impl Into<String>,
) -> CayError {
    let loc = current_full_loc(parser);
    let message = message.into();

    let error = CayError::Parser {
        error_code,
        file: loc.file.clone(),
        line: loc.line,
        column: loc.column,
        message: message.clone(),
        suggestion: ErrorCodes::get_suggestion(error_code).to_string(),
    };

    parser.diagnostics.push(error.clone());
    error
}

/// 检查下一个令牌是否匹配给定令牌
pub fn check_next(parser: &Parser, token: &Token) -> bool {
    if parser.pos + 1 >= parser.tokens.len() - 1 {
        false
    } else {
        &parser.tokens[parser.pos + 1].token == token
    }
}

/// 获取令牌的友好名称
pub fn get_token_name(token: &Token) -> String {
    match token {
        Token::Identifier(s) => format!("标识符 '{}'", s),
        Token::IntegerLiteral(_) => "整数".to_string(),
        Token::FloatLiteral(_) => "浮点数".to_string(),
        Token::StringLiteral(_) => "字符串".to_string(),
        Token::CharLiteral(_) => "字符".to_string(),
        Token::Public => "public".to_string(),
        Token::Private => "private".to_string(),
        Token::Protected => "protected".to_string(),
        Token::Static => "static".to_string(),
        Token::Final => "final".to_string(),
        Token::Abstract => "abstract".to_string(),
        Token::Class => "class".to_string(),
        Token::Struct => "struct".to_string(),
        Token::Enum => "enum".to_string(),
        Token::Interface => "interface".to_string(),
        Token::Void => "void".to_string(),
        Token::Int => "int".to_string(),
        Token::Long => "long".to_string(),
        Token::Float => "float".to_string(),
        Token::Double => "double".to_string(),
        Token::Bool => "bool".to_string(),
        Token::String => "String".to_string(),
        Token::Char => "char".to_string(),
        Token::If => "if".to_string(),
        Token::Else => "else".to_string(),
        Token::While => "while".to_string(),
        Token::For => "for".to_string(),
        Token::Do => "do".to_string(),
        Token::Switch => "switch".to_string(),
        Token::Case => "case".to_string(),
        Token::Default => "default".to_string(),
        Token::Return => "return".to_string(),
        Token::Break => "break".to_string(),
        Token::Continue => "continue".to_string(),
        Token::Scope => "scope".to_string(),
        Token::New => "new".to_string(),
        Token::This => "this".to_string(),
        Token::Super => "super".to_string(),
        Token::Extends => "extends".to_string(),
        Token::Implements => "implements".to_string(),
        Token::InstanceOf => "instanceof".to_string(),
        Token::Var => "var".to_string(),
        Token::Let => "let".to_string(),
        Token::Auto => "auto".to_string(),
        Token::Extern => "extern".to_string(),
        Token::Namespace => "namespace".to_string(),
        Token::Using => "using".to_string(),
        Token::Specialize => "specialize".to_string(),
        Token::True => "true".to_string(),
        Token::False => "false".to_string(),
        Token::Null => "null".to_string(),
        Token::AtMain => "@main".to_string(),
        Token::AtOverride => "@Override".to_string(),
        Token::AtTest => "@Test".to_string(),
        Token::AtFreeFunction => "@FreeFunction".to_string(),
        Token::AtStackOnly => "@stack_only".to_string(),
        Token::At => "@".to_string(),
        Token::LParen => "(".to_string(),
        Token::RParen => ")".to_string(),
        Token::LBrace => "{".to_string(),
        Token::RBrace => "}".to_string(),
        Token::LBracket => "[".to_string(),
        Token::RBracket => "]".to_string(),
        Token::Semicolon => ";".to_string(),
        Token::Comma => ",".to_string(),
        Token::Dot => ".".to_string(),
        Token::DotDotDot => "...".to_string(),
        Token::Colon => ":".to_string(),
        Token::DoubleColon => "::".to_string(),
        Token::Arrow => "->".to_string(),
        Token::Question => "?".to_string(),
        Token::Plus => "+".to_string(),
        Token::Minus => "-".to_string(),
        Token::Star => "*".to_string(),
        Token::Slash => "/".to_string(),
        Token::Percent => "%".to_string(),
        Token::EqEq => "==".to_string(),
        Token::NotEq => "!=".to_string(),
        Token::Lt => "<".to_string(),
        Token::Le => "<=".to_string(),
        Token::Gt => ">".to_string(),
        Token::Ge => ">=".to_string(),
        Token::AndAnd => "&&".to_string(),
        Token::OrOr => "||".to_string(),
        Token::Bang => "!".to_string(),
        Token::Ampersand => "&".to_string(),
        Token::Pipe => "|".to_string(),
        Token::Caret => "^".to_string(),
        Token::Tilde => "~".to_string(),
        Token::Shl => "<<".to_string(),
        Token::Shr => ">>".to_string(),
        Token::UnsignedShr => ">>>".to_string(),
        Token::Assign => "=".to_string(),
        Token::AddAssign => "+=".to_string(),
        Token::SubAssign => "-=".to_string(),
        Token::MulAssign => "*=".to_string(),
        Token::DivAssign => "/=".to_string(),
        Token::ModAssign => "%=".to_string(),
        Token::Inc => "++".to_string(),
        Token::Dec => "--".to_string(),
        Token::Newline => "换行".to_string(),
        Token::BlockComment(_) => "块注释".to_string(),
        Token::CInt => "c_int".to_string(),
        Token::CUInt => "c_uint".to_string(),
        Token::CLong => "c_long".to_string(),
        Token::CULong => "c_ulong".to_string(),
        Token::CShort => "c_short".to_string(),
        Token::CUShort => "c_ushort".to_string(),
        Token::CChar => "c_char".to_string(),
        Token::CUChar => "c_uchar".to_string(),
        Token::CFloat => "c_float".to_string(),
        Token::CDouble => "c_double".to_string(),
        Token::SizeT => "size_t".to_string(),
        Token::SSizeT => "ssize_t".to_string(),
        Token::UIntPtr => "uintptr_t".to_string(),
        Token::IntPtr => "intptr_t".to_string(),
        Token::CVoid => "c_void".to_string(),
        Token::CBool => "c_bool".to_string(),
        Token::CString => "c_string".to_string(),
        Token::CInt64 => "c_int64_t".to_string(),
        Token::CUInt64 => "c_uint64_t".to_string(),
        Token::Cdecl => "cdecl".to_string(),
        Token::Stdcall => "stdcall".to_string(),
        Token::Fastcall => "fastcall".to_string(),
        Token::Sysv64 => "sysv64".to_string(),
        Token::Win64 => "win64".to_string(),
        Token::Native => "native".to_string(),
        Token::InlineIr => "__ir".to_string(),
        Token::Alias => "alias".to_string(),
        Token::Fn => "fn".to_string(),
        Token::As => "as".to_string(),
        Token::Interop => "interop".to_string(),
    }
}

/// 检查令牌是否是类型关键字
pub fn is_type_token(parser: &Parser) -> bool {
    matches!(
        current_token(parser),
        Token::Int
            | Token::Long
            | Token::Float
            | Token::Double
            | Token::Bool
            | Token::String
            | Token::Char
            | Token::Void
            | Token::Auto
            | Token::Var
            | Token::Let
            | Token::CInt
            | Token::CLong
            | Token::CShort
            | Token::CChar
            | Token::CFloat
            | Token::CDouble
            | Token::SizeT
            | Token::SSizeT
            | Token::UIntPtr
            | Token::IntPtr
            | Token::CVoid
            | Token::CBool
            | Token::CInt64
            | Token::CUInt64
            | Token::Identifier(_)
    )
}

/// 同步到下一个语句边界（用于错误恢复）
pub fn synchronize(parser: &mut Parser) {
    advance(parser);

    while !is_at_end(parser) {
        if matches!(current_token(parser), Token::Semicolon) {
            advance(parser);
            return;
        }

        match current_token(parser) {
            Token::Class
            | Token::Struct
            | Token::Enum
            | Token::Interface
            | Token::Public
            | Token::Private
            | Token::Protected
            | Token::If
            | Token::While
            | Token::For
            | Token::Return => {
                return;
            }
            _ => {
                advance(parser);
            }
        }
    }
}
