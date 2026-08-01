//! 递归深度上限回归测试
//!
//! 病态嵌套输入（上万层括号、超长一元运算符链等）曾会让递归下降解析器和
//! 语义分析的表达式类型推断栈溢出直接崩溃。现在应在超过深度上限时返回
//! 诊断错误而不是崩溃。这些测试在独立线程中运行，断言得到 Err 而非进程崩溃。

use cavvy::ast::{Expr, LiteralExpr, LiteralValue, UnaryExpr, UnaryOp};
use cavvy::miette_diagnostic::{SourceLocation, get_error_message};
use cavvy::semantic::SemanticAnalyzer;
use cavvy::types::Type;

/// 在独立线程中运行闭包（解析/语义分析崩溃会表现为线程 panic 或进程 abort）
fn run_in_thread<F, T>(f: F) -> T
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    std::thread::Builder::new()
        // 8MB 栈：足够构造/释放深层测试数据，又足以让未加保护的递归溢出
        .stack_size(8 * 1024 * 1024)
        .spawn(f)
        .expect("spawn test thread")
        .join()
        .expect("compiler must not crash on deeply nested input")
}

/// 生成包含有 count 层括号嵌套表达式的源码
fn nested_parens_source(count: usize) -> String {
    format!(
        "class DepthProbe {{\n    int f() {{\n        int x = {}1{};\n        return x;\n    }}\n}}",
        "(".repeat(count),
        ")".repeat(count)
    )
}

#[test]
fn parser_deeply_nested_parens_returns_error_not_crash() {
    let result = run_in_thread(|| {
        let source = nested_parens_source(100_000);
        let tokens = cavvy::lexer::lex(&source).expect("source should lex");
        cavvy::parser::parse(tokens)
    });
    let err = result.expect_err("病态嵌套输入应返回解析错误");
    let msg = get_error_message(&err);
    assert!(
        msg.contains("嵌套过深"),
        "错误消息应指出嵌套过深，实际: {}",
        msg
    );
}

#[test]
fn parser_deep_unary_chain_returns_error_not_crash() {
    let result = run_in_thread(|| {
        // 10 万个前置换作符链：parse_unary 直接自递归
        let source = format!(
            "class DepthProbe {{\n    bool f() {{\n        bool b = {}true;\n        return b;\n    }}\n}}",
            "!".repeat(100_000)
        );
        let tokens = cavvy::lexer::lex(&source).expect("source should lex");
        cavvy::parser::parse(tokens)
    });
    let err = result.expect_err("超长一元运算符链应返回解析错误");
    let msg = get_error_message(&err);
    assert!(
        msg.contains("嵌套过深"),
        "错误消息应指出嵌套过深，实际: {}",
        msg
    );
}

#[test]
fn parser_deep_ternary_chain_returns_error_not_crash() {
    let result = run_in_thread(|| {
        // 三元运算符右递归：parse_ternary 直接自递归
        let source = format!(
            "class DepthProbe {{\n    int f() {{\n        int x = {}0;\n        return x;\n    }}\n}}",
            "true ? 1 : ".repeat(100_000)
        );
        let tokens = cavvy::lexer::lex(&source).expect("source should lex");
        cavvy::parser::parse(tokens)
    });
    let err = result.expect_err("超长三元链应返回解析错误");
    let msg = get_error_message(&err);
    assert!(
        msg.contains("嵌套过深"),
        "错误消息应指出嵌套过深，实际: {}",
        msg
    );
}

#[test]
fn parser_reasonable_nesting_still_parses() {
    // 正常代码的嵌套深度远低于上限（实测全部语料库最大计数仅 11），
    // 50 层括号嵌套必须仍然可以解析
    let source = nested_parens_source(50);
    let tokens = cavvy::lexer::lex(&source).expect("source should lex");
    cavvy::parser::parse(tokens).expect("合理深度的嵌套表达式应正常解析");
}

#[test]
fn semantic_deep_expr_inference_returns_error_not_crash() {
    // 以编程方式构造 2000 层嵌套的一元表达式（远超 256 层上限），
    // 直接驱动类型推断（绕过解析器上限）
    let result = run_in_thread(|| {
        let mut expr = Expr::Literal(LiteralExpr {
            value: LiteralValue::Bool(true),
            loc: SourceLocation::default(),
        });
        for _ in 0..2000 {
            expr = Expr::Unary(UnaryExpr {
                op: UnaryOp::Not,
                operand: Box::new(expr),
                loc: SourceLocation::default(),
            });
        }

        let mut analyzer = SemanticAnalyzer::new();
        analyzer.infer_expr_type_collect_errors(&expr)
    });
    // 未触发深度保护时，2000 层 !true 推断结果应为 Bool；
    // 触发深度上限后错误被收集并回退为默认类型 Int32。
    // 关键断言是上面没有栈溢出崩溃，这里再确认保护确实生效
    assert_eq!(
        result,
        Type::Int32,
        "深度上限生效时应收集错误并回退为默认类型"
    );
}
