//! Cavvy 语言 6.2.x 新语法集成测试
//!
//! 覆盖：
//! - if 表达式: `let x = if (cond) { a } else { b }`
//! - 函数体尾表达式省略 return: `pub fn add(int a, int b) { a+b }`
//! - 参数后置类型: `pub fn add(a: i32, b: i32) -> i32 { a+b }`

mod common;
use common::{assert_output_contains, compile_and_run_eol_with_features, compile_eol_expect_error};

// ==================== if 表达式 ====================

#[test]
fn test_if_expression() {
    let output = compile_and_run_eol_with_features(
        "examples/test_if_expression.cay",
        &["-F=top_level_function"],
    )
    .expect("test_if_expression should compile and run");
    assert_output_contains(
        &output,
        &["100", "small", "21", "22", "9", "7", "done"],
        "test_if_expression",
    );
}

// ==================== 函数体尾表达式 + 参数后置类型 ====================

#[test]
fn test_expr_body_fn_and_postfix_params() {
    let output = compile_and_run_eol_with_features(
        "examples/test_expr_body_fn.cay",
        &["-F=top_level_function"],
    )
    .expect("test_expr_body_fn should compile and run");
    assert_output_contains(
        &output,
        &["7", "21", "3", "hi", "15", "2", "done"],
        "test_expr_body_fn",
    );
}

// ==================== 错误路径 ====================

#[test]
fn test_if_expression_missing_else_error() {
    let error = compile_eol_expect_error("examples/errors/error_if_expr_missing_else.cay")
        .expect("missing else should fail compilation");
    assert!(
        error.contains("if 表达式必须带 else 分支"),
        "should report missing else, got: {}",
        error
    );
}

#[test]
fn test_if_expression_missing_tail_error() {
    let error = compile_eol_expect_error("examples/errors/error_if_expr_missing_tail.cay")
        .expect("missing tail should fail compilation");
    assert!(
        error.contains("分支必须以表达式结尾"),
        "should report missing tail expression, got: {}",
        error
    );
}

#[test]
fn test_if_expression_type_mismatch_error() {
    let error = compile_eol_expect_error("examples/errors/error_if_expr_type_mismatch.cay")
        .expect("incompatible branch types should fail compilation");
    assert!(
        error.contains("branches must have compatible types"),
        "should report incompatible branch types, got: {}",
        error
    );
}
