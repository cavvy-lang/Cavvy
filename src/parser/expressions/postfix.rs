//! 后缀表达式解析
//!
//! 处理函数调用、成员访问、数组索引、后缀自增自减等后缀表达式。
//! 支持泛型静态方法调用: Type<T>.method()

use super::super::Parser;
use super::assignment::parse_expression;
use super::primary::parse_primary;
use crate::ast::*;
use crate::miette_diagnostic::CayResult;

/// 解析后缀表达式
pub fn parse_postfix(parser: &mut Parser) -> CayResult<Expr> {
    let mut expr = parse_primary(parser)?;

    loop {
        let loc = parser.current_loc();

        // 检查是否是泛型参数: Type<T> 或 Type<T, U>
        // 这用于支持 FileResult<File>.ok(file) 语法
        // 也用于支持省略 new 的泛型构造: Box<int>(42)
        if parser.check(&crate::lexer::Token::Lt) {
            // 向前看，检查是否是泛型参数列表
            let checkpoint = parser.pos;
            let type_args = crate::parser::classes::parse_generic_type_args(parser);

            if let Ok(type_args) = type_args {
                // 成功解析泛型参数，检查后面是否有 '.'、'::' 或 '('
                if parser.check(&crate::lexer::Token::Dot)
                    || parser.check(&crate::lexer::Token::DoubleColon)
                {
                    // 这是泛型静态方法调用: Type<T>.method() 或 Type<T>::method()
                    // 将标识符和泛型参数组合成新的标识符
                    if let Expr::Identifier(ident) = &expr {
                        let generic_name = format!(
                            "{}<{}>",
                            ident.name,
                            type_args
                                .iter()
                                .map(|t| t.display_name())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        expr = Expr::Identifier(IdentifierExpr {
                            name: generic_name,
                            loc: loc.clone(),
                        });
                        continue; // 继续循环，处理成员访问运算符
                    }
                } else if parser.check(&crate::lexer::Token::LParen) {
                    // 这是省略 new 的泛型对象创建: Type<T>(args)
                    // 等价于 new Type<T>(args)
                    if let Expr::Identifier(ident) = &expr {
                        parser.advance(); // 消费 '('
                        let args = parse_arguments(parser)?;
                        parser.consume(
                            &crate::lexer::Token::RParen,
                            "期望 ')'\n提示: 泛型构造参数列表应以 ')' 结束，例如: Box<int>(42)",
                        )?;
                        let class_name = format!(
                            "{}<{}>",
                            ident.name,
                            type_args
                                .iter()
                                .map(|t| t.display_name())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        expr = Expr::New(NewExpr {
                            class_name,
                            args,
                            loc: loc.clone(),
                        });
                        continue; // 继续循环，处理后续后缀
                    }
                }
            }

            // 不是泛型静态方法调用或泛型构造，回退
            parser.pos = checkpoint;
        }

        if parser.match_token(&crate::lexer::Token::LParen) {
            // 函数调用
            let args = parse_arguments(parser)?;
            parser.consume(
                &crate::lexer::Token::RParen,
                "期望 ')'\n提示: 函数调用参数列表应以 ')' 结束",
            )?;
            expr = Expr::Call(CallExpr {
                callee: Box::new(expr),
                args,
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Dot)
            || parser.match_token(&crate::lexer::Token::DoubleColon)
        {
            // 成员访问
            let member = parser.consume_identifier(
                "期望成员名\n提示: 成员访问运算符后应跟成员名，例如: obj.field 或 Type::method()",
            )?;
            expr = Expr::MemberAccess(MemberAccessExpr {
                object: Box::new(expr),
                member,
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::LBracket) {
            // 数组索引访问: arr[index]
            let index = parse_expression(parser)?;
            parser.consume(
                &crate::lexer::Token::RBracket,
                "期望 ']'\n提示: 数组索引应以 ']' 结束，例如: arr[0]",
            )?;
            expr = Expr::ArrayAccess(ArrayAccessExpr {
                array: Box::new(expr),
                index: Box::new(index),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Inc) {
            // 后缀自增: i++
            expr = Expr::Unary(UnaryExpr {
                op: UnaryOp::PostInc,
                operand: Box::new(expr),
                loc,
            });
        } else if parser.match_token(&crate::lexer::Token::Dec) {
            // 后缀自减: i--
            expr = Expr::Unary(UnaryExpr {
                op: UnaryOp::PostDec,
                operand: Box::new(expr),
                loc,
            });
        } else if parser.check(&crate::lexer::Token::Question) && !is_ternary_question(parser) {
            // 6.1.0: ? 运算符: expr?
            // 仅当此 '?' 不是三元运算符 '?:' 的一部分时才作为 try 解析
            parser.advance(); // 消费 '?'
            expr = Expr::Try(TryExpr {
                expr: Box::new(expr),
                loc,
            });
        } else {
            break;
        }
    }

    Ok(expr)
}

/// 6.1.0: 判断当前位置的 '?' 是否属于三元运算符 `?:`
///
/// 通过向前看进行括号深度计数：若在相同括号深度内存在与当前 '?' 匹配的 ':'，
/// 则为三元运算符；否则为 try 运算符 `expr?`。
///
/// 时间复杂度: O(k)，k 为向前看令牌数
/// 空间复杂度: O(1)
fn is_ternary_question(parser: &Parser) -> bool {
    let mut pos = parser.pos + 1; // 跳过当前 '?'
    let mut bracket_depth = 0i32;
    let mut qcount = 1i32; // 当前待匹配的 '?'

    while pos < parser.tokens.len() {
        let token = &parser.tokens[pos].token;
        match token {
            crate::lexer::Token::LParen
            | crate::lexer::Token::LBracket
            | crate::lexer::Token::LBrace => bracket_depth += 1,
            crate::lexer::Token::RParen
            | crate::lexer::Token::RBracket
            | crate::lexer::Token::RBrace => {
                bracket_depth -= 1;
                if bracket_depth < 0 {
                    // 遇到当前表达式外的闭合括号，说明当前 '?' 没有匹配 ':'
                    return false;
                }
                // 同层闭合括号意味着表达式结束
                if bracket_depth == 0 {
                    return false;
                }
            }
            crate::lexer::Token::Question if bracket_depth == 0 => qcount += 1,
            crate::lexer::Token::Colon if bracket_depth == 0 => {
                qcount -= 1;
                if qcount == 0 {
                    return true;
                }
            }
            crate::lexer::Token::Semicolon | crate::lexer::Token::Comma
                if bracket_depth == 0 =>
            {
                return false;
            }
            _ => {}
        }
        pos += 1;
    }

    false
}

/// 解析参数列表（支持命名参数 name=value）
pub fn parse_arguments(parser: &mut Parser) -> CayResult<Vec<Expr>> {
    let mut args = Vec::new();

    if !parser.check(&crate::lexer::Token::RParen) {
        loop {
            let arg = parse_expression(parser)?;
            // 检查是否是命名参数: ident = value
            if let Expr::Assignment(ref assign) = arg {
                if let Expr::Identifier(ref ident) = *assign.target {
                    if assign.op == AssignOp::Assign {
                        // 转换为命名参数
                        args.push(Expr::NamedArg(NamedArgExpr {
                            name: ident.name.clone(),
                            value: assign.value.clone(),
                            loc: assign.loc.clone(),
                        }));
                        if !parser.match_token(&crate::lexer::Token::Comma) {
                            break;
                        }
                        continue;
                    }
                }
            }
            args.push(arg);
            if !parser.match_token(&crate::lexer::Token::Comma) {
                break;
            }
        }
    }

    Ok(args)
}
