//! 语句解析

use crate::ast::*;
use crate::error::cayResult;
use super::Parser;
use super::types::{parse_type, is_primitive_type_token};
use super::expressions::parse_expression;

/// 解析代码块
pub fn parse_block(parser: &mut Parser) -> cayResult<Block> {
    let loc = parser.current_loc();
    parser.consume(&crate::lexer::Token::LBrace, "Expected '{' to start block")?;
    
    let mut statements = Vec::new();
    while !parser.check(&crate::lexer::Token::RBrace) && !parser.is_at_end() {
        statements.push(parse_statement(parser)?);
    }
    
    parser.consume(&crate::lexer::Token::RBrace, "Expected '}' to end block")?;
    
    Ok(Block { statements, loc })
}

/// 解析语句
pub fn parse_statement(parser: &mut Parser) -> cayResult<Stmt> {
    match parser.current_token() {
        crate::lexer::Token::LBrace => Ok(Stmt::Block(parse_block(parser)?)),
        crate::lexer::Token::If => parse_if_statement(parser),
        crate::lexer::Token::While => parse_while_statement(parser),
        crate::lexer::Token::For => parse_for_statement(parser),
        crate::lexer::Token::Do => parse_do_while_statement(parser),
        crate::lexer::Token::Switch => parse_switch_statement(parser),
        crate::lexer::Token::Return => parse_return_statement(parser),
        crate::lexer::Token::Break => {
            let _loc = parser.current_loc();
            parser.advance();
            parser.consume(&crate::lexer::Token::Semicolon, "Expected ';' after break")?;
            Ok(Stmt::Break)
        }
        crate::lexer::Token::Continue => {
            let _loc = parser.current_loc();
            parser.advance();
            parser.consume(&crate::lexer::Token::Semicolon, "Expected ';' after continue")?;
            Ok(Stmt::Continue)
        }
        crate::lexer::Token::Var | crate::lexer::Token::Let | crate::lexer::Token::Auto => {
            // 后置类型声明或自动类型推断
            parse_modern_var_decl(parser)
        }
        _ => {
            // 检查是否是变量声明：支持任意类型标识（类名或原始类型），
            // 但要确保接下来的 token 是变量名（Identifier），以避免将函数调用等标识误判为类型。
            if parser.check(&crate::lexer::Token::Final) {
                // 检查是否是 final var/let/auto 语法
                if parser.check_next(&crate::lexer::Token::Var) ||
                   parser.check_next(&crate::lexer::Token::Let) ||
                   parser.check_next(&crate::lexer::Token::Auto) {
                    return parse_modern_var_decl(parser);
                }
                return parse_var_decl(parser);
            }

            if super::types::is_type_token(parser) {
                // 尝试解析类型（不消耗最终位置）以判断是否紧跟变量名。
                let checkpoint = parser.pos;
                if super::types::parse_type(parser).is_ok() {
                    // 如果解析类型后当前token是标识符，则认为是变量声明
                    if let crate::lexer::Token::Identifier(_) = parser.current_token() {
                        parser.pos = checkpoint; // 回退到类型前位置
                        return parse_var_decl(parser);
                    }
                }
                // 回退到初始位置，继续解析为表达式语句
                parser.pos = checkpoint;
            }

            parse_expression_statement(parser)
        }
    }
}

/// 解析传统变量声明（类型前置）
pub fn parse_var_decl(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    
    let is_final = parser.match_token(&crate::lexer::Token::Final);
    
    let var_type = parse_type(parser)?;
    let name = parser.consume_identifier("Expected variable name")?;
    
    let initializer = if parser.match_token(&crate::lexer::Token::Assign) {
        // 检查是否是数组初始化: {1, 2, 3}
        if parser.check(&crate::lexer::Token::LBrace) {
            Some(parse_array_initializer(parser)?)
        } else {
            Some(parse_expression(parser)?)
        }
    } else {
        None
    };
    
    parser.consume(&crate::lexer::Token::Semicolon, "Expected ';' after variable declaration")?;
    
    Ok(Stmt::VarDecl(VarDecl {
        name,
        var_type,
        initializer,
        is_final,
        loc,
    }))
}

/// 解析现代变量声明（var/let/auto + 后置类型）
/// 支持语法：
/// - var x: int = 10;      // var 声明，类型后置
/// - let y: String = "a";  // let 声明，类型后置
/// - auto z = 10;          // 自动类型推断
/// - final var x: int = 10; // final 修饰
pub fn parse_modern_var_decl(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    
    // 检查是否有 final 修饰符（final var x: int = 10）
    let is_final = parser.match_token(&crate::lexer::Token::Final);
    
    // 获取声明关键字（var/let/auto）
    let keyword = parser.current_token().clone();
    parser.advance(); // consume var/let/auto
    
    let name = parser.consume_identifier("Expected variable name after var/let/auto")?;
    
    // 解析可选的类型注解（: Type）
    let var_type = if parser.match_token(&crate::lexer::Token::Colon) {
        // 有类型注解：var x: int
        parse_type(parser)?
    } else {
        // 无类型注解，必须是 auto 或必须有初始化器
        match keyword {
            crate::lexer::Token::Auto => crate::types::Type::Auto,
            crate::lexer::Token::Var | crate::lexer::Token::Let => {
                // var/let 必须有类型注解（暂时如此，之后可以实现类型推断）
                return Err(parser.error("var/let declaration requires type annotation (: Type) or use 'auto' for type inference"));
            }
            _ => unreachable!()
        }
    };
    
    // 解析初始化器
    let initializer = if parser.match_token(&crate::lexer::Token::Assign) {
        // 检查是否是数组初始化: {1, 2, 3}
        if parser.check(&crate::lexer::Token::LBrace) {
            Some(parse_array_initializer(parser)?)
        } else {
            Some(parse_expression(parser)?)
        }
    } else {
        None
    };
    
    parser.consume(&crate::lexer::Token::Semicolon, "Expected ';' after variable declaration")?;
    
    Ok(Stmt::VarDecl(VarDecl {
        name,
        var_type,
        initializer,
        is_final,
        loc,
    }))
}

/// 解析数组初始化表达式: {1, 2, 3}
fn parse_array_initializer(parser: &mut Parser) -> cayResult<Expr> {
    let loc = parser.current_loc();
    parser.consume(&crate::lexer::Token::LBrace, "Expected '{' to start array initializer")?;
    
    let mut elements = Vec::new();
    
    // 解析元素列表
    if !parser.check(&crate::lexer::Token::RBrace) {
        loop {
            // 递归解析，支持嵌套数组初始化
            if parser.check(&crate::lexer::Token::LBrace) {
                elements.push(parse_array_initializer(parser)?);
            } else {
                elements.push(parse_expression(parser)?);
            }
            
            if !parser.match_token(&crate::lexer::Token::Comma) {
                break;
            }
        }
    }
    
    parser.consume(&crate::lexer::Token::RBrace, "Expected '}' to end array initializer")?;
    
    Ok(Expr::ArrayInit(ArrayInitExpr { elements, loc }))
}

/// 解析 if 语句
pub fn parse_if_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume 'if'
    
    parser.consume(&crate::lexer::Token::LParen, "Expected '(' after 'if'")?;
    let condition = parse_expression(parser)?;
    parser.consume(&crate::lexer::Token::RParen, "Expected ')' after if condition")?;
    
    let then_branch = Box::new(parse_statement(parser)?);
    let else_branch = if parser.match_token(&crate::lexer::Token::Else) {
        Some(Box::new(parse_statement(parser)?))
    } else {
        None
    };
    
    Ok(Stmt::If(IfStmt {
        condition,
        then_branch,
        else_branch,
        loc,
    }))
}

/// 解析 while 语句
pub fn parse_while_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume 'while'
    
    parser.consume(&crate::lexer::Token::LParen, "Expected '(' after 'while'")?;
    let condition = parse_expression(parser)?;
    parser.consume(&crate::lexer::Token::RParen, "Expected ')' after while condition")?;
    
    let body = Box::new(parse_statement(parser)?);
    
    Ok(Stmt::While(WhileStmt {
        condition,
        body,
        loc,
    }))
}

/// 解析 for 语句
pub fn parse_for_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume 'for'
    
    parser.consume(&crate::lexer::Token::LParen, "Expected '(' after 'for'")?;
    
    let init = if parser.check(&crate::lexer::Token::Semicolon) {
        None
    } else {
        Some(Box::new(parse_statement(parser)?))
    };
    
    let condition = if parser.check(&crate::lexer::Token::Semicolon) {
        None
    } else {
        Some(parse_expression(parser)?)
    };
    parser.consume(&crate::lexer::Token::Semicolon, "Expected ';' after for condition")?;
    
    let update = if parser.check(&crate::lexer::Token::RParen) {
        None
    } else {
        Some(parse_expression(parser)?)
    };
    
    parser.consume(&crate::lexer::Token::RParen, "Expected ')' after for clauses")?;
    
    let body = Box::new(parse_statement(parser)?);
    
    Ok(Stmt::For(ForStmt {
        init,
        condition,
        update,
        body,
        loc,
    }))
}

/// 解析 do-while 语句
pub fn parse_do_while_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume 'do'
    
    let body = Box::new(parse_statement(parser)?);
    
    parser.consume(&crate::lexer::Token::While, "Expected 'while' after 'do'")?;
    parser.consume(&crate::lexer::Token::LParen, "Expected '(' after 'while'")?;
    let condition = parse_expression(parser)?;
    parser.consume(&crate::lexer::Token::RParen, "Expected ')' after condition")?;
    parser.consume(&crate::lexer::Token::Semicolon, "Expected ';' after do-while")?;
    
    Ok(Stmt::DoWhile(DoWhileStmt {
        condition,
        body,
        loc,
    }))
}

/// 解析 switch 语句
pub fn parse_switch_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume 'switch'
    
    parser.consume(&crate::lexer::Token::LParen, "Expected '(' after 'switch'")?;
    let expr = parse_expression(parser)?;
    parser.consume(&crate::lexer::Token::RParen, "Expected ')' after switch expression")?;
    
    parser.consume(&crate::lexer::Token::LBrace, "Expected '{' to start switch body")?;
    
    let mut cases = Vec::new();
    let mut default = None;
    
    while !parser.check(&crate::lexer::Token::RBrace) && !parser.is_at_end() {
        if parser.match_token(&crate::lexer::Token::Case) {
            // 解析 case 值
            let value = match *parser.current_token() {
                crate::lexer::Token::IntegerLiteral(Some((v, _))) => {
                    let val = v;  // v 是 i64
                    parser.advance();
                    val
                }
                _ => return Err(parser.error("Expected integer literal in case")),
            };
            parser.consume(&crate::lexer::Token::Colon, "Expected ':' after case value")?;
            
            // 解析 case 体（直到遇到另一个 case、default 或 }）
            let mut body = Vec::new();
            while !parser.check(&crate::lexer::Token::Case) && !parser.check(&crate::lexer::Token::Default)
                && !parser.check(&crate::lexer::Token::RBrace) && !parser.is_at_end() {
                body.push(parse_statement(parser)?);
            }
            
            cases.push(Case { value, body });
        } else if parser.match_token(&crate::lexer::Token::Default) {
            parser.consume(&crate::lexer::Token::Colon, "Expected ':' after 'default'")?;
            
            // 解析 default 体
            let mut body = Vec::new();
            while !parser.check(&crate::lexer::Token::Case) && !parser.check(&crate::lexer::Token::Default)
                && !parser.check(&crate::lexer::Token::RBrace) && !parser.is_at_end() {
                body.push(parse_statement(parser)?);
            }
            
            default = Some(body);
        } else {
            return Err(parser.error("Expected 'case' or 'default' in switch"));
        }
    }
    
    parser.consume(&crate::lexer::Token::RBrace, "Expected '}' to end switch body")?;
    
    Ok(Stmt::Switch(SwitchStmt {
        expr,
        cases,
        default,
        loc,
    }))
}

/// 解析 return 语句
pub fn parse_return_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let _loc = parser.current_loc();
    parser.advance(); // consume 'return'
    
    let value = if !parser.check(&crate::lexer::Token::Semicolon) {
        Some(parse_expression(parser)?)
    } else {
        None
    };
    
    parser.consume(&crate::lexer::Token::Semicolon, "Expected ';' after return")?;
    
    Ok(Stmt::Return(value))
}

/// 解析表达式语句
pub fn parse_expression_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let expr = parse_expression(parser)?;
    parser.consume(&crate::lexer::Token::Semicolon, "Expected ';' after expression")?;
    Ok(Stmt::Expr(expr))
}
