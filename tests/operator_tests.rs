//! Cavvy 语言运算符集成测试
//!
//! 测试各种运算符：算术、比较、逻辑、位运算、赋值、自增自减等

mod common;
use common::compile_and_run_eol;

// ========== EBNF 综合测试 ==========

#[test]
fn test_assignment_operators() {
    let output = compile_and_run_eol("examples/test_assignment_operators.cay")
        .expect("assignment operators example should compile and run");
    assert!(
        output.contains("10 += 5 = 15"),
        "+= operator should work, got: {}",
        output
    );
    assert!(
        output.contains("10 -= 5 = 5"),
        "-= operator should work, got: {}",
        output
    );
    assert!(
        output.contains("10 *= 5 = 50"),
        "*= operator should work, got: {}",
        output
    );
    assert!(
        output.contains("10 /= 5 = 2"),
        "/= operator should work, got: {}",
        output
    );
    assert!(
        output.contains("10 %= 5 = 0"),
        "%= operator should work, got: {}",
        output
    );
    assert!(
        output.contains("All assignment operator tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

#[test]
fn test_bitwise_operators() {
    let output = compile_and_run_eol("examples/test_bitwise_operators.cay")
        .expect("bitwise operators example should compile and run");
    assert!(
        output.contains("a & b = 12"),
        "Bitwise AND should work, got: {}",
        output
    );
    assert!(
        output.contains("a | b = 61"),
        "Bitwise OR should work, got: {}",
        output
    );
    assert!(
        output.contains("a ^ b = 49"),
        "Bitwise XOR should work, got: {}",
        output
    );
    assert!(
        output.contains("a << 2 = 240"),
        "Left shift should work, got: {}",
        output
    );
    assert!(
        output.contains("a >> 2 = 15"),
        "Right shift should work, got: {}",
        output
    );
    assert!(
        output.contains("All bitwise operator tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

#[test]
fn test_logical_operators() {
    let output = compile_and_run_eol("examples/test_logical_operators.cay")
        .expect("logical operators example should compile and run");
    assert!(
        output.contains("true && true = true"),
        "Logical AND should work, got: {}",
        output
    );
    assert!(
        output.contains("true || false = true"),
        "Logical OR should work, got: {}",
        output
    );
    assert!(
        output.contains("!true = false"),
        "Logical NOT should work, got: {}",
        output
    );
    assert!(
        output.contains("All logical operator tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

#[test]
fn test_comparison_operators() {
    let output = compile_and_run_eol("examples/test_comparison_operators.cay")
        .expect("comparison operators example should compile and run");
    assert!(
        output.contains("a == c: true"),
        "== operator should work, got: {}",
        output
    );
    assert!(
        output.contains("a != b: true"),
        "!= operator should work, got: {}",
        output
    );
    assert!(
        output.contains("a < b: true"),
        "< operator should work, got: {}",
        output
    );
    assert!(
        output.contains("a <= b: true"),
        "<= operator should work, got: {}",
        output
    );
    assert!(
        output.contains("b > a: true"),
        "> operator should work, got: {}",
        output
    );
    assert!(
        output.contains("b >= a: true"),
        ">= operator should work, got: {}",
        output
    );
    assert!(
        output.contains("All comparison operator tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

#[test]
fn test_increment_decrement() {
    let output = compile_and_run_eol("examples/test_increment_decrement.cay")
        .expect("increment/decrement example should compile and run");
    assert!(
        output.contains("expected: a=6, b=6") || output.contains("a = 6, b = 6"),
        "Prefix ++ should work, got: {}",
        output
    );
    assert!(
        output.contains("expected: a=6, b=5") || output.contains("a = 6, b = 5"),
        "Postfix ++ should work, got: {}",
        output
    );
    assert!(
        output.contains("All increment/decrement tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

// ========== 新增运算符测试 ==========

#[test]
fn test_arith_add() {
    let output = compile_and_run_eol("examples/test_arith_add.cay")
        .expect("arith add example should compile and run");
    assert!(
        output.contains("30") && output.contains("4"),
        "Arithmetic add should work, got: {}",
        output
    );
}

#[test]
fn test_arith_sub() {
    let output = compile_and_run_eol("examples/test_arith_sub.cay")
        .expect("arith sub example should compile and run");
    assert!(
        output.contains("20") && output.contains("3"),
        "Arithmetic sub should work, got: {}",
        output
    );
}

#[test]
fn test_arith_mul() {
    let output = compile_and_run_eol("examples/test_arith_mul.cay")
        .expect("arith mul example should compile and run");
    assert!(
        output.contains("42") && output.contains("10"),
        "Arithmetic mul should work, got: {}",
        output
    );
}

#[test]
fn test_arith_div() {
    let output = compile_and_run_eol("examples/test_arith_div.cay")
        .expect("arith div example should compile and run");
    assert!(
        output.contains("5") && output.contains("2.5"),
        "Arithmetic div should work, got: {}",
        output
    );
}

#[test]
fn test_arith_mod() {
    let output = compile_and_run_eol("examples/test_arith_mod.cay")
        .expect("arith mod example should compile and run");
    assert!(
        output.contains("2") && output.contains("0"),
        "Arithmetic mod should work, got: {}",
        output
    );
}

#[test]
fn test_comp_eq() {
    let output = compile_and_run_eol("examples/test_comp_eq.cay")
        .expect("comp eq example should compile and run");
    assert!(
        output.contains("a == b is true") && output.contains("a == c is false"),
        "Comparison eq should work, got: {}",
        output
    );
}

#[test]
fn test_comp_ne() {
    let output = compile_and_run_eol("examples/test_comp_ne.cay")
        .expect("comp ne example should compile and run");
    assert!(
        output.contains("a != b is true") && output.contains("a != c is false"),
        "Comparison ne should work, got: {}",
        output
    );
}

#[test]
fn test_comp_lt() {
    let output = compile_and_run_eol("examples/test_comp_lt.cay")
        .expect("comp lt example should compile and run");
    assert!(
        output.contains("a < b is true") && output.contains("b < a is false"),
        "Comparison lt should work, got: {}",
        output
    );
}

#[test]
fn test_comp_gt() {
    let output = compile_and_run_eol("examples/test_comp_gt.cay")
        .expect("comp gt example should compile and run");
    assert!(
        output.contains("a > b is true") && output.contains("b > a is false"),
        "Comparison gt should work, got: {}",
        output
    );
}

#[test]
fn test_comp_le() {
    let output = compile_and_run_eol("examples/test_comp_le.cay")
        .expect("comp le example should compile and run");
    assert!(
        output.contains("a <= b is true")
            && output.contains("a <= c is true")
            && output.contains("c <= a is false"),
        "Comparison le should work, got: {}",
        output
    );
}

#[test]
fn test_comp_ge() {
    let output = compile_and_run_eol("examples/test_comp_ge.cay")
        .expect("comp ge example should compile and run");
    assert!(
        output.contains("a >= b is true")
            && output.contains("a >= c is true")
            && output.contains("c >= a is false"),
        "Comparison ge should work, got: {}",
        output
    );
}

#[test]
fn test_logical_and() {
    let output = compile_and_run_eol("examples/test_logical_and.cay")
        .expect("logical and example should compile and run");
    assert!(
        output.contains("true && true is true") && output.contains("true && false is false"),
        "Logical AND should work, got: {}",
        output
    );
}

#[test]
fn test_logical_or() {
    let output = compile_and_run_eol("examples/test_logical_or.cay")
        .expect("logical or example should compile and run");
    assert!(
        output.contains("true || true is true") && output.contains("false || false is false"),
        "Logical OR should work, got: {}",
        output
    );
}

#[test]
fn test_logical_not() {
    let output = compile_and_run_eol("examples/test_logical_not.cay")
        .expect("logical not example should compile and run");
    assert!(
        output.contains("!true is false - correct!")
            && output.contains("!false is true - correct!"),
        "Logical NOT should work, got: {}",
        output
    );
}

#[test]
fn test_bitwise_and() {
    let output = compile_and_run_eol("examples/test_bitwise_and.cay")
        .expect("bitwise and example should compile and run");
    assert!(
        output.contains("8"),
        "Bitwise AND should work, got: {}",
        output
    );
}

#[test]
fn test_bitwise_or() {
    let output = compile_and_run_eol("examples/test_bitwise_or.cay")
        .expect("bitwise or example should compile and run");
    assert!(
        output.contains("14"),
        "Bitwise OR should work, got: {}",
        output
    );
}

#[test]
fn test_bitwise_xor() {
    let output = compile_and_run_eol("examples/test_bitwise_xor.cay")
        .expect("bitwise xor example should compile and run");
    assert!(
        output.contains("6"),
        "Bitwise XOR should work, got: {}",
        output
    );
}

#[test]
fn test_bitwise_not() {
    let output = compile_and_run_eol("examples/test_bitwise_not.cay")
        .expect("bitwise not example should compile and run");
    assert!(
        output.contains("-16"),
        "Bitwise NOT should work, got: {}",
        output
    );
}

#[test]
fn test_shift_left() {
    let output = compile_and_run_eol("examples/test_shift_left.cay")
        .expect("shift left example should compile and run");
    assert!(
        output.contains("2") && output.contains("4") && output.contains("8"),
        "Shift left should work, got: {}",
        output
    );
}

#[test]
fn test_shift_right() {
    let output = compile_and_run_eol("examples/test_shift_right.cay")
        .expect("shift right example should compile and run");
    assert!(
        output.contains("4") && output.contains("2") && output.contains("1"),
        "Shift right should work, got: {}",
        output
    );
}

#[test]
fn test_pre_increment() {
    let output = compile_and_run_eol("examples/test_pre_increment.cay")
        .expect("pre increment example should compile and run");
    assert!(
        output.contains("a = 6") && output.contains("c = 11"),
        "Pre-increment should work, got: {}",
        output
    );
}

#[test]
fn test_post_increment() {
    let output = compile_and_run_eol("examples/test_post_increment.cay")
        .expect("post increment example should compile and run");
    assert!(
        output.contains("a before: 5") && output.contains("a after increment: 6"),
        "Post-increment should work, got: {}",
        output
    );
}

#[test]
fn test_pre_decrement() {
    let output = compile_and_run_eol("examples/test_pre_decrement.cay")
        .expect("pre decrement example should compile and run");
    assert!(
        output.contains("a - 1 = 4") && output.contains("c - 1 = 9"),
        "Pre-decrement should work, got: {}",
        output
    );
}

#[test]
fn test_post_decrement() {
    let output = compile_and_run_eol("examples/test_post_decrement.cay")
        .expect("post decrement example should compile and run");
    assert!(
        output.contains("a before: 5") && output.contains("a after decrement: 4"),
        "Post-decrement should work, got: {}",
        output
    );
}

#[test]
fn test_final_int() {
    let output = compile_and_run_eol("examples/test_final_int.cay")
        .expect("final int example should compile and run");
    assert!(
        output.contains("100") && output.contains("0"),
        "Final int should work, got: {}",
        output
    );
}

#[test]
fn test_final_string() {
    let output = compile_and_run_eol("examples/test_final_string.cay")
        .expect("final string example should compile and run");
    assert!(
        output.contains("Hello") && output.contains("EOL"),
        "Final string should work, got: {}",
        output
    );
}

#[test]
fn test_bitwise_advanced() {
    let output = compile_and_run_eol("examples/test_bitwise_advanced.cay")
        .expect("bitwise advanced should compile and run");
    assert!(
        output.contains("completed"),
        "Bitwise advanced test should complete, got: {}",
        output
    );
}

#[test]
fn test_expression_complex() {
    let output = compile_and_run_eol("examples/test_expression_complex.cay")
        .expect("expression complex should compile and run");
    assert!(
        output.contains("completed"),
        "Expression complex test should complete, got: {}",
        output
    );
}
