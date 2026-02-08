//! 二元表达式解析
//!
//! 处理所有二元运算符表达式，包括逻辑、位运算、算术、比较等。

use crate::ast::*;
use crate::error::EolResult;
use super::super::Parser;
use super::unary::parse_unary;

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
