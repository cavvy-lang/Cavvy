//! Cavvy 语言 var/let 和 auto 功能集成测试
//!
//! 测试 var/let 后置类型声明和 auto 自动类型推断

mod common;
use common::compile_and_run_eol;

// ==================== 0.4.3.0 var/let 后置类型声明和 auto 自动类型推断测试 ====================

#[test]
fn test_var_let_decl() {
    let output = compile_and_run_eol("examples/test_var_let_decl.cay")
        .expect("var/let decl should compile and run");
    assert!(
        output.contains("Testing var/let declarations:"),
        "var/let decl test should start, got: {}",
        output
    );
    assert!(
        output.contains("10")
            && output.contains("20")
            && output.contains("100")
            && output.contains("200"),
        "var/let declarations should output correct values, got: {}",
        output
    );
    assert!(
        output.contains("330"),
        "sum (10+20+100+200=330) should be calculated, got: {}",
        output
    );
}

#[test]
fn test_auto_inference() {
    let output = compile_and_run_eol("examples/test_auto_inference.cay")
        .expect("auto inference should compile and run");
    assert!(
        output.contains("Testing auto type inference:"),
        "auto inference test should start, got: {}",
        output
    );
    assert!(
        output.contains("42"),
        "auto a = 42 should output 42, got: {}",
        output
    );
    assert!(
        output.contains("true"), // true is printed as true
        "auto flag = true should output true, got: {}",
        output
    );
    assert!(
        output.contains("88"), // 'X' is ASCII 88
        "auto c = 'X' should output 88, got: {}",
        output
    );
    assert!(
        output.contains("50"),
        "auto result = a + 8 should output 50, got: {}",
        output
    );
}

#[test]
fn test_0_4_3_features() {
    let output = compile_and_run_eol("examples/test_0_4_3_features.cay")
        .expect("0.4.3 features should compile and run");
    assert!(
        output.contains("=== Cavvy 0.4.3.0 Feature Test ==="),
        "0.4.3 features test should start, got: {}",
        output
    );
    assert!(
        output.contains("5"),
        "var count: int = 5 should output 5, got: {}",
        output
    );
    assert!(
        output.contains("1001"),
        "let id: int = 1001 should output 1001, got: {}",
        output
    );
    assert!(
        output.contains("123"),
        "auto num = 123 should output 123, got: {}",
        output
    );
    assert!(
        output.contains("10"), // sum = 1+2+3+4 = 10
        "mixed usage sum should output 10, got: {}",
        output
    );
    assert!(
        output.contains("60"), // result = (10+20)*2 = 60
        "expression result should output 60, got: {}",
        output
    );
    assert!(
        output.contains("=== All Tests Passed! ==="),
        "All 0.4.3.x features tests should pass, got: {}",
        output
    );
}

#[test]
fn test_0_4_3_basic() {
    let output = compile_and_run_eol("examples/test_0_4_3_basic.cay")
        .expect("0.4.3 basic should compile and run");
    assert!(
        output.contains("10"),
        "Should output 10 (var x), got: {}",
        output
    );
    assert!(
        output.contains("20"),
        "Should output 20 (let y), got: {}",
        output
    );
    assert!(
        output.contains("42"),
        "Should output 42 (auto a), got: {}",
        output
    );
    assert!(
        output.contains("72"),
        "Should output 72 (sum), got: {}",
        output
    );
    assert!(
        output.contains("9999"),
        "Should output 9999 (final marker), got: {}",
        output
    );
}

#[test]
fn test_0_4_3_simple() {
    let output = compile_and_run_eol("examples/test_0_4_3_simple.cay")
        .expect("0.4.3 simple should compile and run");
    assert!(
        output.contains("10"),
        "Should output 10 (var x), got: {}",
        output
    );
    assert!(
        output.contains("20"),
        "Should output 20 (let y), got: {}",
        output
    );
    assert!(
        output.contains("30"),
        "Should output 30 (final var z), got: {}",
        output
    );
    assert!(
        output.contains("42"),
        "Should output 42 (auto a), got: {}",
        output
    );
    assert!(
        output.contains("72"), // 10+20+42=72
        "Should output 72 (sum), got: {}",
        output
    );
    assert!(
        output.contains("9999"),
        "Should output 9999 (final marker), got: {}",
        output
    );
}
