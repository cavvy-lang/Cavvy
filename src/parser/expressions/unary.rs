//! 一元表达式解析
//!
//! 处理一元运算符（-、!、~）和类型转换表达式。

use super::super::Parser;
use super::super::types::{is_type_token, parse_type};
use super::postfix::parse_postfix;
use crate::ast::*;
use crate::miette_diagnostic::CayResult;

/// 解析一元表达式（包括类型转换）
pub fn parse_unary(parser: &mut Parser) -> CayResult<Expr> {
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

    // 前置自增 ++i
    if parser.match_token(&crate::lexer::Token::Inc) {
        let operand = parse_unary(parser)?;
        return Ok(Expr::Unary(UnaryExpr {
            op: UnaryOp::PreInc,
            operand: Box::new(operand),
            loc,
        }));
    }

    // 前置自减 --i
    if parser.match_token(&crate::lexer::Token::Dec) {
        let operand = parse_unary(parser)?;
        return Ok(Expr::Unary(UnaryExpr {
            op: UnaryOp::PreDec,
            operand: Box::new(operand),
            loc,
        }));
    }

    // 取地址 &variable
    if parser.match_token(&crate::lexer::Token::Ampersand) {
        let operand = parse_unary(parser)?;
        return Ok(Expr::Unary(UnaryExpr {
            op: UnaryOp::AddressOf,
            operand: Box::new(operand),
            loc,
        }));
    }

    // 解引用 *pointer
    if parser.match_token(&crate::lexer::Token::Star) {
        let operand = parse_unary(parser)?;
        return Ok(Expr::Unary(UnaryExpr {
            op: UnaryOp::Deref,
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
                    // 期望 RParen，且 ')' 之后的 token 必须能开始一个一元表达式，
                    // 否则这只是普通括号表达式（如 int c = (a) + b;），回退按括号表达式解析
                    if parser.check(&crate::lexer::Token::RParen) {
                        parser.advance();
                        if can_start_unary_expr(parser) {
                            // 成功解析类型转换，解析后面的表达式
                            let expr = parse_unary(parser)?;
                            return Ok(Expr::Cast(CastExpr {
                                expr: Box::new(expr),
                                target_type,
                                loc,
                            }));
                        }
                    }
                    // 没有 RParen，或 ')' 后无法开始一元表达式，回退
                    parser.pos = checkpoint;
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

/// 判断当前 token 是否能开始一个一元表达式
/// 用于类型转换预读：区分 (type) expr 与普通括号表达式 (expr)
fn can_start_unary_expr(parser: &Parser) -> bool {
    use crate::lexer::Token;
    matches!(
        parser.current_token(),
        // 一元运算符
        Token::Minus
            | Token::Bang
            | Token::Tilde
            | Token::Inc
            | Token::Dec
            | Token::Ampersand
            | Token::Star
            // 括号/字面量/标识符等 primary 表达式起始
            | Token::LParen
            | Token::Identifier(_)
            | Token::IntegerLiteral(_)
            | Token::FloatLiteral(_)
            | Token::StringLiteral(_)
            | Token::CharLiteral(_)
            | Token::True
            | Token::False
            | Token::Null
            | Token::New
            | Token::This
            | Token::Super
    )
}

#[cfg(test)]
mod tests {
    /// 回归测试：(a) + b 是普通括号表达式，不得误判为类型转换
    /// （修复前预读看到 '(' + 标识符 + ')' 就无条件按 cast 提交，导致无法解析）
    #[test]
    fn paren_expr_not_misread_as_cast() {
        let source = r#"
            class CastParenProbe {
                int f(int a, int b) {
                    int c = (a) + b;
                    return c;
                }
            }
        "#;

        let tokens = crate::lexer::lex(source).expect("source should lex");
        crate::parser::parse_with_source(tokens, source.to_string())
            .expect("parenthesized expression should parse");
    }

    /// 回归测试：真正的类型转换 (long) a 仍然按 cast 解析
    #[test]
    fn real_cast_still_parses_as_cast() {
        let source = r#"
            class CastProbe {
                long f(int a) {
                    return (long) a;
                }
            }
        "#;

        let tokens = crate::lexer::lex(source).expect("source should lex");
        let ast = crate::parser::parse_with_source(tokens, source.to_string())
            .expect("cast expression should parse");

        let method = match &ast.classes[0].members[0] {
            crate::ast::ClassMember::Method(method) => method,
            _ => panic!("expected method"),
        };
        let body = method.body.as_ref().expect("method should have body");
        match &body.statements[0] {
            crate::ast::Stmt::Return(Some(crate::ast::Expr::Cast(_))) => {}
            other => panic!("expected return of cast expression, got {:?}", other),
        }
    }
}
