//! 语句解析

use super::Parser;
use super::expressions::parse_expression;
use super::types::{is_primitive_type_token, parse_type};
use crate::ast::*;
use crate::miette_diagnostic::cayResult;
use crate::types::Type;

/// 给语句添加标签
fn add_label_to_stmt(stmt: Stmt, label: String) -> Stmt {
    match stmt {
        Stmt::For(mut for_stmt) => {
            for_stmt.label = Some(label);
            Stmt::For(for_stmt)
        }
        Stmt::While(mut while_stmt) => {
            while_stmt.label = Some(label);
            Stmt::While(while_stmt)
        }
        Stmt::DoWhile(mut do_while_stmt) => {
            do_while_stmt.label = Some(label);
            Stmt::DoWhile(do_while_stmt)
        }
        _ => stmt, // 非循环语句不支持标签，保持原样
    }
}

/// 解析代码块
pub fn parse_block(parser: &mut Parser) -> cayResult<Block> {
    let loc = parser.current_loc();
    parser.consume(
        &crate::lexer::Token::LBrace,
        "期望 '{'\n提示: 代码块以 '{' 开始，例如: { ... }",
    )?;

    let mut statements = Vec::new();
    while !parser.check(&crate::lexer::Token::RBrace) && !parser.is_at_end() {
        // 跳过换行符（支持一行内多个语句）
        while parser.check(&crate::lexer::Token::Newline) {
            parser.advance();
        }

        // 再次检查是否到达代码块结束
        if parser.check(&crate::lexer::Token::RBrace) || parser.is_at_end() {
            break;
        }

        statements.push(parse_statement(parser)?);
    }

    parser.consume(
        &crate::lexer::Token::RBrace,
        "期望 '}'\n提示: 代码块以 '}' 结束",
    )?;

    Ok(Block { statements, loc })
}

/// 解析语句
pub fn parse_statement(parser: &mut Parser) -> cayResult<Stmt> {
    // 检查是否是标签语句: label:
    if let crate::lexer::Token::Identifier(label_name) = parser.current_token().clone() {
        // 向前看检查是否是冒号
        let checkpoint = parser.pos;
        parser.advance(); // 跳过标识符

        if parser.check(&crate::lexer::Token::Colon) {
            parser.advance(); // 跳过冒号

            // 解析带标签的语句
            let stmt = parse_statement(parser)?;

            // 给语句添加标签
            return Ok(add_label_to_stmt(stmt, label_name));
        } else {
            // 不是标签，回退
            parser.pos = checkpoint;
        }
    }

    match parser.current_token() {
        crate::lexer::Token::LBrace => Ok(Stmt::Block(parse_block(parser)?)),
        crate::lexer::Token::If => parse_if_statement(parser),
        crate::lexer::Token::While => parse_while_statement(parser),
        crate::lexer::Token::For => parse_for_statement(parser),
        crate::lexer::Token::Do => parse_do_while_statement(parser),
        crate::lexer::Token::Switch => parse_switch_statement(parser),
        crate::lexer::Token::Scope => parse_scope_statement(parser),
        crate::lexer::Token::Return => parse_return_statement(parser),
        crate::lexer::Token::Break => {
            let loc = parser.current_loc();
            parser.advance();

            // 检查是否有标签
            let label =
                if let crate::lexer::Token::Identifier(name) = parser.current_token().clone() {
                    parser.advance();
                    Some(name)
                } else {
                    None
                };

            parser.consume(
                &crate::lexer::Token::Semicolon,
                "期望 ';'\n提示: break 语句应以 ';' 结束",
            )?;
            Ok(Stmt::Break(label, loc))
        }
        crate::lexer::Token::Continue => {
            let loc = parser.current_loc();
            parser.advance();

            // 检查是否有标签
            let label =
                if let crate::lexer::Token::Identifier(name) = parser.current_token().clone() {
                    parser.advance();
                    Some(name)
                } else {
                    None
                };

            parser.consume(
                &crate::lexer::Token::Semicolon,
                "期望 ';'\n提示: continue 语句应以 ';' 结束",
            )?;
            Ok(Stmt::Continue(label, loc))
        }
        crate::lexer::Token::InlineIr => parse_inline_ir_statement(parser),
        _ => {
            // 检查是否是变量声明：支持任意类型标识（类名或原始类型），
            // 但要确保接下来的 token 是变量名（Identifier），以避免将函数调用等标识误判为类型。
            if parser.check(&crate::lexer::Token::Final) {
                return parse_var_decl(parser);
            }

            // 检查是否是 var/let/auto 后置类型声明
            if parser.check(&crate::lexer::Token::Var)
                || parser.check(&crate::lexer::Token::Let)
                || parser.check(&crate::lexer::Token::Auto)
            {
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

/// 解析变量声明
/// 支持以下语法：
/// - 传统语法: int x = 10;
/// - 多变量声明: int a = 10, b = 20, c;
/// - final 修饰: final int x = 10;
/// - var 后置类型: var x: int = 10;
/// - let 后置类型: let x: int = 10;
/// - auto 类型推断: auto a = 42;
pub fn parse_var_decl(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();

    let is_final = parser.match_token(&crate::lexer::Token::Final);

    // 检查是否是 var/let/auto 语法
    let var_type = if parser.check(&crate::lexer::Token::Var)
        || parser.check(&crate::lexer::Token::Let)
        || parser.check(&crate::lexer::Token::Auto)
    {
        // var/let/auto 语法: var name: type = value; 或 auto name = value;
        parser.advance(); // 消费 var/let/auto

        // 解析变量名
        let name = parser.consume_identifier(
            "期望变量名\n提示: var/let/auto 后应跟变量名，例如: var x: int = 10;",
        )?;

        // 检查是否有类型注解 (: type)
        let explicit_type = if parser.match_token(&crate::lexer::Token::Colon) {
            Some(parse_type(parser)?)
        } else {
            None
        };

        // 检查是否有初始化表达式
        let initializer = if parser.match_token(&crate::lexer::Token::Assign) {
            Some(parse_expression(parser)?)
        } else {
            None
        };

        parser.consume(
            &crate::lexer::Token::Semicolon,
            "期望 ';'\n提示: 变量声明应以 ';' 结束，例如: var x: int = 10;",
        )?;

        // 确定变量类型
        let var_type = match explicit_type {
            Some(t) => t,
            None => {
                // 如果没有显式类型，需要根据初始化表达式推断
                // 使用 Auto 类型作为占位符，由语义分析阶段推断
                crate::types::Type::Auto
            }
        };

        return Ok(Stmt::VarDecl(VarDecl {
            name,
            var_type,
            initializer,
            is_final,
            loc,
        }));
    } else {
        // 传统语法: type name = value;
        parse_type(parser)?
    };

    // 解析第一个变量
    let name = parser.consume_identifier("期望变量名\n提示: 类型后应跟变量名，例如: int count;")?;

    let initializer = if parser.match_token(&crate::lexer::Token::Assign) {
        Some(parse_expression(parser)?)
    } else {
        None
    };

    // 检查是否有多变量声明 (逗号分隔)
    let mut var_decls = vec![VarDecl {
        name,
        var_type: var_type.clone(),
        initializer,
        is_final,
        loc: loc.clone(),
    }];

    while parser.match_token(&crate::lexer::Token::Comma) {
        // 解析下一个变量名
        let next_name = parser
            .consume_identifier("期望变量名\n提示: 逗号后应跟变量名，例如: int a = 10, b, c;")?;

        // 检查是否有初始化表达式
        let next_initializer = if parser.match_token(&crate::lexer::Token::Assign) {
            Some(parse_expression(parser)?)
        } else {
            None
        };

        var_decls.push(VarDecl {
            name: next_name,
            var_type: var_type.clone(),
            initializer: next_initializer,
            is_final,
            loc: parser.current_loc(),
        });
    }

    parser.consume(
        &crate::lexer::Token::Semicolon,
        "期望 ';'\n提示: 变量声明应以 ';' 结束，例如: int count = 0;",
    )?;

    // 如果只有一个变量，直接返回
    if var_decls.len() == 1 {
        return Ok(Stmt::VarDecl(var_decls.into_iter().next().unwrap()));
    }

    // 多个变量，返回一个 Block 包含所有声明
    let statements: Vec<Stmt> = var_decls.into_iter().map(Stmt::VarDecl).collect();
    Ok(Stmt::Block(Block { statements, loc }))
}

/// 解析 if 语句
pub fn parse_if_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume 'if'

    parser.consume(
        &crate::lexer::Token::LParen,
        "期望 '('\n提示: if 后应跟 '(' 开始条件表达式，例如: if (x > 0) { ... }",
    )?;
    let condition = parse_expression(parser)?;
    parser.consume(
        &crate::lexer::Token::RParen,
        "期望 ')'\n提示: 条件表达式应以 ')' 结束，例如: if (x > 0) { ... }",
    )?;

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

    parser.consume(
        &crate::lexer::Token::LParen,
        "期望 '('\n提示: while 后应跟 '(' 开始条件表达式，例如: while (x > 0) { ... }",
    )?;
    let condition = parse_expression(parser)?;
    parser.consume(
        &crate::lexer::Token::RParen,
        "期望 ')'\n提示: 条件表达式应以 ')' 结束，例如: while (x > 0) { ... }",
    )?;

    let body = Box::new(parse_statement(parser)?);

    Ok(Stmt::While(WhileStmt {
        condition,
        body,
        label: None,
        loc,
    }))
}

/// 解析 for 语句
///
/// 支持两种形式：
/// - 传统 for 循环: `for (init; cond; update) body`
/// - 增强 for 循环: `for (type var : iterable) body`
pub fn parse_for_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume 'for'

    parser.consume(
        &crate::lexer::Token::LParen,
        "期望 '('\n提示: for 后应跟 '(' 开始循环头，例如: for (int i = 0; i < 10; i++) { ... }",
    )?;

    // 预读判断是否为增强 for 循环: `type identifier : expression`
    let saved_pos = parser.pos;
    let enhanced_decl: Option<(Type, String)> = (|| -> cayResult<_> {
        let var_type = super::types::parse_type(parser)?;
        let var_name = parser.consume_identifier(
            "期望变量名\n提示: 增强 for 循环应为 for (type var : iterable) { ... }",
        )?;
        parser.consume(
            &crate::lexer::Token::Colon,
            "期望 ':'\n提示: 增强 for 循环应为 for (type var : iterable) { ... }",
        )?;
        Ok((var_type, var_name))
    })()
    .ok();
    if enhanced_decl.is_none() {
        parser.pos = saved_pos;
    }

    if let Some((var_type, var_name)) = enhanced_decl {
        let iterable = parse_expression(parser)?;
        parser.consume(
            &crate::lexer::Token::RParen,
            "期望 ')'\n提示: 增强 for 循环头应以 ')' 结束，例如: for (int x : arr) { ... }",
        )?;
        let body = Box::new(parse_statement(parser)?);
        return Ok(Stmt::ForEach(ForEachStmt {
            var_type,
            var_name,
            iterable,
            body,
            label: None,
            loc,
        }));
    }

    let init = if parser.check(&crate::lexer::Token::Semicolon) {
        parser.advance(); // consume ';'
        None
    } else {
        Some(Box::new(parse_statement(parser)?))
    };

    let condition = if parser.check(&crate::lexer::Token::Semicolon) {
        None
    } else {
        Some(parse_expression(parser)?)
    };
    parser.consume(&crate::lexer::Token::Semicolon, "期望 ';'\n提示: for 循环的条件部分应以 ';' 结束，例如: for (int i = 0; i < 10; i++) { ... }")?;

    let update = if parser.check(&crate::lexer::Token::RParen) {
        None
    } else {
        Some(parse_expression(parser)?)
    };

    parser.consume(
        &crate::lexer::Token::RParen,
        "期望 ')'\n提示: for 循环头应以 ')' 结束，例如: for (int i = 0; i < 10; i++) { ... }",
    )?;

    let body = Box::new(parse_statement(parser)?);

    Ok(Stmt::For(ForStmt {
        init,
        condition,
        update,
        body,
        label: None,
        loc,
    }))
}

/// 解析 do-while 语句
pub fn parse_do_while_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume 'do'

    let body = Box::new(parse_statement(parser)?);

    parser.consume(
        &crate::lexer::Token::While,
        "期望 'while'\n提示: do 语句后应跟 while，例如: do { ... } while (condition);",
    )?;
    parser.consume(
        &crate::lexer::Token::LParen,
        "期望 '('\n提示: while 后应跟 '(' 开始条件表达式，例如: while (x > 0)",
    )?;
    let condition = parse_expression(parser)?;
    parser.consume(
        &crate::lexer::Token::RParen,
        "期望 ')'\n提示: 条件表达式应以 ')' 结束",
    )?;
    parser.consume(
        &crate::lexer::Token::Semicolon,
        "期望 ';'\n提示: do-while 语句应以 ';' 结束",
    )?;

    Ok(Stmt::DoWhile(DoWhileStmt {
        condition,
        body,
        label: None,
        loc,
    }))
}

/// 解析 switch 语句
pub fn parse_switch_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume 'switch'

    parser.consume(
        &crate::lexer::Token::LParen,
        "期望 '('\n提示: switch 后应跟 '(' 开始表达式，例如: switch (x) { ... }",
    )?;
    let expr = parse_expression(parser)?;
    parser.consume(
        &crate::lexer::Token::RParen,
        "期望 ')'\n提示: 表达式应以 ')' 结束，例如: switch (x) { ... }",
    )?;

    parser.consume(
        &crate::lexer::Token::LBrace,
        "期望 '{'\n提示: switch 体以 '{' 开始，例如: switch (x) { case 1: ... }",
    )?;

    let mut cases = Vec::new();
    let mut default = None;

    while !parser.check(&crate::lexer::Token::RBrace) && !parser.is_at_end() {
        if parser.match_token(&crate::lexer::Token::Case) {
            // 解析 case 值（支持整数、字符常量和 enum variant）
            let case_value = match *parser.current_token() {
                crate::lexer::Token::IntegerLiteral(Some((v, _))) => {
                    let val = v; // v 是 i64
                    parser.advance();
                    CaseValue::Integer(val)
                }
                crate::lexer::Token::CharLiteral(Some(c)) => {
                    let val = c as i64; // 字符转换为整数
                    parser.advance();
                    CaseValue::Integer(val)
                }
                crate::lexer::Token::Identifier(ref enum_name) => {
                    // 可能是 enum variant: Color.Red 或带解构: Result.Ok(int val)
                    let enum_name = enum_name.clone();
                    parser.advance();

                    // 检查是否是 MemberAccess: EnumName.VariantName
                    if parser.check(&crate::lexer::Token::Dot) {
                        parser.advance(); // consume '.'

                        if let crate::lexer::Token::Identifier(variant_name) =
                            parser.current_token().clone()
                        {
                            parser.advance();
                            CaseValue::EnumVariant {
                                enum_name,
                                variant_name,
                            }
                        } else {
                            return Err(parser.error(&format!(
                                "期望 enum variant 名称\n提示: case 标签格式为 'EnumName.VariantName'"
                            )));
                        }
                    } else {
                        // 单独的标识符，可能是常量（未来扩展）
                        return Err(parser.error(&format!(
                            "case 标签不支持单独的标识符 '{}'\n提示: 应使用整数常量（如 case 1:）或 enum variant（如 case Color.Red:）",
                            enum_name
                        )));
                    }
                }
                _ => {
                    let current_token = parser.current_token();
                    let (token_desc, suggestion) = match current_token {
                        crate::lexer::Token::StringLiteral(Some(s)) => (
                            format!("字符串(\"{}\")", s),
                            "case 标签不支持字符串。可能的问题:\n    - 应使用整数常量，如: case 1:\n    - 如果需要字符串匹配，考虑使用 if-else 链".to_string()
                        ),
                        crate::lexer::Token::FloatLiteral(Some((val, _))) => (
                            format!("浮点数({})", val),
                            "case 标签必须是整数常量，不能使用浮点数。可能的问题:\n    - 应使用整数，如: case 1: 而不是 case 1.0:".to_string()
                        ),
                        crate::lexer::Token::True | crate::lexer::Token::False => (
                            "布尔值".to_string(),
                            "case 标签必须是整数常量。可能的问题:\n    - 应使用整数，如: case 1: 表示 true, case 0: 表示 false".to_string()
                        ),
                        crate::lexer::Token::Colon => (
                            "冒号(:)".to_string(),
                            "case 标签缺少值。可能的问题:\n    - case 后缺少整数常量，如: case : 应该是 case 1:".to_string()
                        ),
                        crate::lexer::Token::Semicolon => (
                            "分号(;)".to_string(),
                            "case 标签格式错误。可能的问题:\n    - case 后缺少值和冒号，如: case ; 应该是 case 1: ...;".to_string()
                        ),
                        crate::lexer::Token::Case => (
                            "关键字(case)".to_string(),
                            "case 标签重复或缺少值。可能的问题:\n    - 两个 case 之间缺少值，如: case case 1: 应该是 case 0: case 1:".to_string()
                        ),
                        crate::lexer::Token::Default => (
                            "关键字(default)".to_string(),
                            "default 标签位置错误。可能的问题:\n    - case 和 default 不能在同一位置\n    - default 应该单独使用: default:".to_string()
                        ),
                        _ => {
                            let token_name = super::utils::get_token_name(current_token);
                            (
                                token_name.clone(),
                                format!("case 标签必须是整数常量或 enum variant。可能的问题:\n    - 使用了不合法的值\n    - 应使用整数常量，如: case 1:\n    - 或使用 enum variant，如: case Color.Red:")
                            )
                        }
                    };
                    return Err(parser.error(&format!(
                        "期望整数常量或 enum variant，但遇到了 {}\n提示: {}",
                        token_desc, suggestion
                    )));
                }
            };

            // 检查是否有枚举 variant 解构绑定: case EnumName.Variant(Type var_name):
            let mut payload_binding: Option<PayloadBinding> = None;
            if matches!(case_value, CaseValue::EnumVariant { .. })
                && parser.check(&crate::lexer::Token::LParen)
            {
                parser.advance(); // consume '('
                let var_type = super::types::parse_type(parser)?;
                if let crate::lexer::Token::Identifier(var_name) = parser.current_token().clone() {
                    parser.advance();
                    parser.consume(&crate::lexer::Token::RParen, "期望 ')'\n提示: enum variant 解构模式应以 ')' 结束，例如: case Result.Ok(int val):")?;
                    payload_binding = Some(PayloadBinding { var_type, var_name });
                } else {
                    return Err(parser.error("期望变量名\n提示: enum variant 解构格式为 'case EnumName.Variant(Type varName):'"));
                }
            }

            parser.consume(
                &crate::lexer::Token::Colon,
                "期望 ':'\n提示: case 值后应跟 ':'，例如: case 1:",
            )?;

            // 解析 case 体（直到遇到另一个 case、default 或 }）
            let mut body = Vec::new();
            while !parser.check(&crate::lexer::Token::Case)
                && !parser.check(&crate::lexer::Token::Default)
                && !parser.check(&crate::lexer::Token::RBrace)
                && !parser.is_at_end()
            {
                body.push(parse_statement(parser)?);
            }

            cases.push(Case {
                value: case_value,
                body,
                payload_binding,
                loc: parser.current_loc(),
            });
        } else if parser.match_token(&crate::lexer::Token::Default) {
            parser.consume(
                &crate::lexer::Token::Colon,
                "期望 ':'\n提示: default 后应跟 ':'，例如: default:",
            )?;

            // 解析 default 体
            let mut body = Vec::new();
            while !parser.check(&crate::lexer::Token::Case)
                && !parser.check(&crate::lexer::Token::Default)
                && !parser.check(&crate::lexer::Token::RBrace)
                && !parser.is_at_end()
            {
                body.push(parse_statement(parser)?);
            }

            default = Some(body);
        } else {
            let current_token = parser.current_token();
            let (token_desc, suggestion) = match current_token {
                crate::lexer::Token::RBrace => (
                    "右花括号(})".to_string(),
                    "switch 体为空或提前结束。可能的问题:\n    - switch 语句缺少 case 或 default 分支\n    - 在添加分支前关闭了 switch 体".to_string()
                ),
                crate::lexer::Token::Semicolon => (
                    "分号(;)".to_string(),
                    "switch 体内不能直接放置分号。可能的问题:\n    - 多余的空语句\n    - 语句位置错误，应在 case 标签后".to_string()
                ),
                crate::lexer::Token::Identifier(name) => (
                    format!("标识符('{}')", name),
                    format!("switch 体内只能包含 case 或 default 标签。可能的问题:\n    - 缺少 case 关键字，如: {}: 应该是 case 1:\n    - 语句位置错误，应在 case 标签后", name)
                ),
                crate::lexer::Token::IntegerLiteral(Some((val, _))) => (
                    format!("整数({})", val),
                    format!("switch 体内只能包含 case 或 default 标签。可能的问题:\n    - 缺少 case 关键字，如: {}: 应该是 case {}:", val, val)
                ),
                crate::lexer::Token::If | crate::lexer::Token::While |
                crate::lexer::Token::For | crate::lexer::Token::Return => {
                    let kw = format!("{:?}", current_token).to_lowercase();
                    (
                        format!("关键字({})", kw),
                        format!("{} 语句必须在 case 或 default 标签后。可能的问题:\n    - 缺少 case 标签\n    - 语句缩进错误", kw)
                    )
                }
                crate::lexer::Token::LBrace => (
                    "左花括号({)".to_string(),
                    "switch 体内不能直接嵌套代码块。可能的问题:\n    - 代码块应在 case 标签后\n    - 考虑使用 case 1: {{ ... }} 语法".to_string()
                ),
                crate::lexer::Token::Colon => (
                    "冒号(:)".to_string(),
                    "冒号位置错误。可能的问题:\n    - case 标签缺少值，如: case : 应该是 case 1:\n    - 多余的冒号".to_string()
                ),
                _ => {
                    let token_name = super::utils::get_token_name(current_token);
                    (
                        token_name.clone(),
                        format!("switch 体内只能包含 case 或 default 标签。可能的问题:\n    - 语句位置错误\n    - 缺少 case/default 关键字")
                    )
                }
            };
            return Err(parser.error(&format!(
                "期望 'case' 或 'default'，但遇到了 {}\n提示: {}",
                token_desc, suggestion
            )));
        }
    }

    parser.consume(
        &crate::lexer::Token::RBrace,
        "期望 '}'\n提示: switch 体以 '}' 结束",
    )?;

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

    parser.consume(
        &crate::lexer::Token::Semicolon,
        "期望 ';'\n提示: return 语句应以 ';' 结束，例如: return 0;",
    )?;

    Ok(Stmt::Return(value))
}

/// 解析表达式语句
pub fn parse_expression_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let expr = parse_expression(parser)?;
    parser.consume(
        &crate::lexer::Token::Semicolon,
        "期望 ';'\n提示: 表达式语句应以 ';' 结束，例如: x = 10;",
    )?;
    Ok(Stmt::Expr(expr))
}

/// 0.5.0.0: 解析 scope 语句
/// scope 语句创建一个栈作用域，内部声明的变量在 scope 结束时自动释放
///
/// 语法: scope { statements... }
///
/// 示例:
///   scope {
///       int x = 10;
///       println("x = " + x);
///   } // x 在这里自动释放
pub fn parse_scope_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume 'scope'

    // 解析 scope 体（代码块）
    let body = parse_block(parser)?;

    Ok(Stmt::Scope(ScopeStmt { body, loc }))
}

/// 解析内联IR语句
///
/// 语法: __ir { raw_llvm_ir_lines... }
///
/// 示例:
///   __ir {
///       %result = add i32 %a, %b
///       store i32 %result, i32* %ptr
///   }
pub fn parse_inline_ir_statement(parser: &mut Parser) -> cayResult<Stmt> {
    let loc = parser.current_loc();
    parser.advance(); // consume '__ir'

    // 期望 {
    parser.consume(
        &crate::lexer::Token::LBrace,
        "期望 '{'\n提示: __ir 后应跟 '{' 开始 IR 块",
    )?;

    // 从 token 流解析内联 IR（基于 token，不受预处理器行号映射影响）
    let raw_lines = parse_inline_ir_from_tokens(parser)?;

    Ok(Stmt::InlineIr(InlineIrStmt { raw_lines, loc }))
}

/// 跳过内联IR块中的所有token，直到匹配的RBrace
/// 注意：调用此函数时，当前token应该是块内的第一个token（LBrace已经被consume消耗）
/// 从token流解析内联IR
fn parse_inline_ir_from_tokens(parser: &mut Parser) -> cayResult<Vec<String>> {
    let mut raw_lines: Vec<String> = Vec::new();
    let mut current_line: Vec<String> = Vec::new();
    let mut brace_depth = 1;
    let mut current_line_num = parser.current_loc().line;

    while !parser.is_at_end() && brace_depth > 0 {
        let token_line = parser.current_loc().line;

        // 如果行号变化，保存当前行并开始新行
        if token_line != current_line_num && !current_line.is_empty() {
            let line = join_ir_tokens(&current_line);
            if !line.trim().is_empty() {
                raw_lines.push(line);
            }
            current_line.clear();
            current_line_num = token_line;
        }

        match parser.current_token() {
            crate::lexer::Token::LBrace => {
                brace_depth += 1;
                if brace_depth > 1 {
                    current_line.push("{".to_string());
                }
                parser.advance();
            }
            crate::lexer::Token::RBrace => {
                brace_depth -= 1;
                if brace_depth > 0 {
                    current_line.push("}".to_string());
                }
                parser.advance();
            }
            crate::lexer::Token::Percent => {
                parser.advance();
                let reg_name = match parser.current_token() {
                    crate::lexer::Token::Identifier(s) => {
                        let name = s.clone();
                        parser.advance();
                        name
                    }
                    crate::lexer::Token::IntegerLiteral(opt) => {
                        let val = match opt {
                            Some((v, _)) => v.to_string(),
                            None => "0".to_string(),
                        };
                        parser.advance();
                        val
                    }
                    // 处理关键字作为寄存器名（IR块中允许使用关键字作为标识符）
                    crate::lexer::Token::Final => {
                        parser.advance();
                        "final".to_string()
                    }
                    crate::lexer::Token::Class => {
                        parser.advance();
                        "class".to_string()
                    }
                    crate::lexer::Token::Void => {
                        parser.advance();
                        "void".to_string()
                    }
                    crate::lexer::Token::Int => {
                        parser.advance();
                        "i32".to_string()
                    }
                    crate::lexer::Token::Long => {
                        parser.advance();
                        "i64".to_string()
                    }
                    crate::lexer::Token::Float => {
                        parser.advance();
                        "float".to_string()
                    }
                    crate::lexer::Token::Double => {
                        parser.advance();
                        "double".to_string()
                    }
                    crate::lexer::Token::Bool => {
                        parser.advance();
                        "bool".to_string()
                    }
                    crate::lexer::Token::String => {
                        parser.advance();
                        "string".to_string()
                    }
                    crate::lexer::Token::Char => {
                        parser.advance();
                        "char".to_string()
                    }
                    crate::lexer::Token::True => {
                        parser.advance();
                        "true".to_string()
                    }
                    crate::lexer::Token::False => {
                        parser.advance();
                        "false".to_string()
                    }
                    crate::lexer::Token::Null => {
                        parser.advance();
                        "null".to_string()
                    }
                    crate::lexer::Token::If => {
                        parser.advance();
                        "if".to_string()
                    }
                    crate::lexer::Token::Else => {
                        parser.advance();
                        "else".to_string()
                    }
                    crate::lexer::Token::While => {
                        parser.advance();
                        "while".to_string()
                    }
                    crate::lexer::Token::For => {
                        parser.advance();
                        "for".to_string()
                    }
                    crate::lexer::Token::Return => {
                        parser.advance();
                        "return".to_string()
                    }
                    crate::lexer::Token::Break => {
                        parser.advance();
                        "break".to_string()
                    }
                    crate::lexer::Token::Continue => {
                        parser.advance();
                        "continue".to_string()
                    }
                    crate::lexer::Token::New => {
                        parser.advance();
                        "new".to_string()
                    }
                    crate::lexer::Token::This => {
                        parser.advance();
                        "this".to_string()
                    }
                    crate::lexer::Token::Super => {
                        parser.advance();
                        "super".to_string()
                    }
                    crate::lexer::Token::Public => {
                        parser.advance();
                        "public".to_string()
                    }
                    crate::lexer::Token::Private => {
                        parser.advance();
                        "private".to_string()
                    }
                    crate::lexer::Token::Protected => {
                        parser.advance();
                        "protected".to_string()
                    }
                    crate::lexer::Token::Static => {
                        parser.advance();
                        "static".to_string()
                    }
                    crate::lexer::Token::Abstract => {
                        parser.advance();
                        "abstract".to_string()
                    }
                    crate::lexer::Token::Native => {
                        parser.advance();
                        "native".to_string()
                    }
                    crate::lexer::Token::Extends => {
                        parser.advance();
                        "extends".to_string()
                    }
                    crate::lexer::Token::Implements => {
                        parser.advance();
                        "implements".to_string()
                    }
                    crate::lexer::Token::Interface => {
                        parser.advance();
                        "interface".to_string()
                    }
                    crate::lexer::Token::InstanceOf => {
                        parser.advance();
                        "instanceof".to_string()
                    }
                    crate::lexer::Token::Var => {
                        parser.advance();
                        "var".to_string()
                    }
                    crate::lexer::Token::Let => {
                        parser.advance();
                        "let".to_string()
                    }
                    crate::lexer::Token::Auto => {
                        parser.advance();
                        "auto".to_string()
                    }
                    crate::lexer::Token::Extern => {
                        parser.advance();
                        "extern".to_string()
                    }
                    crate::lexer::Token::Scope => {
                        parser.advance();
                        "scope".to_string()
                    }
                    crate::lexer::Token::Do => {
                        parser.advance();
                        "do".to_string()
                    }
                    crate::lexer::Token::Switch => {
                        parser.advance();
                        "switch".to_string()
                    }
                    crate::lexer::Token::Case => {
                        parser.advance();
                        "case".to_string()
                    }
                    crate::lexer::Token::Default => {
                        parser.advance();
                        "default".to_string()
                    }
                    _ => {
                        current_line.push("%".to_string());
                        continue;
                    }
                };
                current_line.push(format!("%{}", reg_name));
            }
            crate::lexer::Token::Identifier(s) => {
                current_line.push(s.clone());
                parser.advance();
            }
            crate::lexer::Token::IntegerLiteral(opt) => {
                let val = match opt {
                    Some((v, _)) => v.to_string(),
                    None => "0".to_string(),
                };
                current_line.push(val);
                parser.advance();
            }
            crate::lexer::Token::Comma => {
                current_line.push(",".to_string());
                parser.advance();
            }
            crate::lexer::Token::Dot => {
                current_line.push(".".to_string());
                parser.advance();
            }
            crate::lexer::Token::Star => {
                current_line.push("*".to_string());
                parser.advance();
            }
            crate::lexer::Token::At => {
                current_line.push("@".to_string());
                parser.advance();
            }
            crate::lexer::Token::LParen => {
                current_line.push("(".to_string());
                parser.advance();
            }
            crate::lexer::Token::RParen => {
                current_line.push(")".to_string());
                parser.advance();
            }
            crate::lexer::Token::Assign => {
                current_line.push("=".to_string());
                parser.advance();
            }
            crate::lexer::Token::Float => {
                current_line.push("float".to_string());
                parser.advance();
            }
            crate::lexer::Token::Double => {
                current_line.push("double".to_string());
                parser.advance();
            }
            // LLVM IR 类型关键词（Cavvy 关键词，但在 IR 中是普通标识符）
            crate::lexer::Token::Void => {
                current_line.push("void".to_string());
                parser.advance();
            }
            crate::lexer::Token::Int => {
                current_line.push("i32".to_string());
                parser.advance();
            }
            crate::lexer::Token::Long => {
                current_line.push("i64".to_string());
                parser.advance();
            }
            crate::lexer::Token::LBracket => {
                current_line.push("[".to_string());
                parser.advance();
            }
            crate::lexer::Token::RBracket => {
                current_line.push("]".to_string());
                parser.advance();
            }
            crate::lexer::Token::Lt => {
                current_line.push("<".to_string());
                parser.advance();
            }
            crate::lexer::Token::Gt => {
                current_line.push(">".to_string());
                parser.advance();
            }
            crate::lexer::Token::Bool => {
                current_line.push("bool".to_string());
                parser.advance();
            }
            crate::lexer::Token::True => {
                current_line.push("true".to_string());
                parser.advance();
            }
            crate::lexer::Token::False => {
                current_line.push("false".to_string());
                parser.advance();
            }
            crate::lexer::Token::Semicolon => {
                // 跳过注释：分号表示注释开始，跳过直到行末
                let semicolon_line = parser.current_loc().line;
                parser.advance();
                // 跳过注释内容（所有token直到行号变化或RBrace）
                while !parser.is_at_end() {
                    let next_token_line = parser.current_loc().line;
                    match parser.current_token() {
                        crate::lexer::Token::RBrace => break,
                        _ => {
                            // 如果行号变化，说明注释结束
                            if next_token_line != semicolon_line {
                                break;
                            }
                            parser.advance();
                        }
                    }
                }
                // 如果当前行不为空，保存当前行
                if !current_line.is_empty() {
                    let line = join_ir_tokens(&current_line);
                    if !line.trim().is_empty() {
                        raw_lines.push(line);
                    }
                    current_line.clear();
                }
                // 更新current_line_num为注释后的token行号
                // 这样后续的IR指令会在同一行被处理
                current_line_num = parser.current_loc().line;
            }
            crate::lexer::Token::Newline => {
                // Newline token被lexer跳过，这里不需要处理
                parser.advance();
            }
            _ => {
                parser.advance();
            }
        }
    }

    // 处理最后一行
    if !current_line.is_empty() {
        let line = join_ir_tokens(&current_line);
        if !line.trim().is_empty() {
            raw_lines.push(line);
        }
    }

    Ok(raw_lines)
}

/// 智能拼接内联 IR 的 token，LLVM IR 类型如 i8* 和函数调用 @malloc() 不需要空格
fn join_ir_tokens(tokens: &[String]) -> String {
    let mut result = String::new();
    for (i, token) in tokens.iter().enumerate() {
        if i > 0 {
            let prev = &tokens[i - 1];
            // 不需要在前面加空格的情况:
            // - 当前是 ) , ( *  时
            // - 前一个是 @ (  时
            let no_space = token == ")"
                || token == ","
                || token == "."
                || token == "("
                || prev == "@"
                || prev == "."
                || prev == "("
                || (*token == "*" && is_llvm_type_token(prev));
            if !no_space {
                result.push(' ');
            }
        }
        result.push_str(token);
    }
    result
}

/// 判断 token 是否是 LLVM 类型名（后面跟 * 不需要空格）
fn is_llvm_type_token(token: &str) -> bool {
    matches!(
        token,
        "i1" | "i8" | "i16" | "i32" | "i64" | "float" | "double" | "void" | "%struct" | "label"
    )
}

#[cfg(test)]
mod tests {
    use crate::ast::{ClassMember, Stmt};

    #[test]
    fn inline_ir_preserves_intrinsic_names_and_bool_literals() {
        let source = r#"
            class InlineIrParserProbe {
                void copy(ptr dest, ptr src) {
                    __ir {
                        call void @llvm.memcpy.p0i8.p0i8.i64(i8* %dest, i8* %src, i64 8, i1 false)
                    }
                }
            }
        "#;

        let tokens = crate::lexer::lex(source).expect("source should lex");
        let ast = crate::parser::parse_with_source(tokens, source.to_string())
            .expect("source should parse");

        let method = match &ast.classes[0].members[0] {
            ClassMember::Method(method) => method,
            _ => panic!("expected method"),
        };
        let body = method.body.as_ref().expect("method should have body");
        let inline_ir = match &body.statements[0] {
            Stmt::InlineIr(inline_ir) => inline_ir,
            _ => panic!("expected inline IR statement"),
        };

        assert_eq!(
            inline_ir.raw_lines[0],
            "call void @llvm.memcpy.p0i8.p0i8.i64(i8* %dest, i8* %src, i64 8, i1 false)"
        );
    }
}
