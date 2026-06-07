//! Math标准库测试
//!
//! 测试Math.cay标准库的所有功能，包括数学函数、随机数生成和向量操作

mod common;
use common::compile_and_run_eol;

/// 测试Math库综合功能
#[test]
fn test_math_library() {
    let output = compile_and_run_eol("examples/test_math_library.cay")
        .expect("Math library test should compile and run");
    assert!(
        output.contains("=== Math Library Test Suite ==="),
        "Should show test suite header, got: {}",
        output
    );
    assert!(
        output.contains("All Math Tests Passed!"),
        "All Math tests should pass, got: {}",
        output
    );
}

/// 测试Random库
#[test]
fn test_random_library() {
    let output = compile_and_run_eol("examples/test_random.cay")
        .expect("Random library test should compile and run");
    assert!(
        output.contains("=== Random Library Test Suite ==="),
        "Should show test suite header, got: {}",
        output
    );
    assert!(
        output.contains("All Random Tests Passed!"),
        "All Random tests should pass, got: {}",
        output
    );
}

/// 测试Vector2
#[test]
fn test_vector2_library() {
    let output = compile_and_run_eol("examples/test_vector2.cay")
        .expect("Vector2 test should compile and run");
    assert!(
        output.contains("=== Vector2 Test Suite ==="),
        "Should show test suite header, got: {}",
        output
    );
    assert!(
        output.contains("All Vector2 Tests Passed!"),
        "All Vector2 tests should pass, got: {}",
        output
    );
}

/// 测试Vector3
#[test]
fn test_vector3_library() {
    let output = compile_and_run_eol("examples/test_vector3.cay")
        .expect("Vector3 test should compile and run");
    assert!(
        output.contains("=== Vector3 Test Suite ==="),
        "Should show test suite header, got: {}",
        output
    );
    assert!(
        output.contains("All Vector3 Tests Passed!"),
        "All Vector3 tests should pass, got: {}",
        output
    );
}

/// 测试extern别名功能
#[test]
fn test_extern_alias_feature() {
    let output = compile_and_run_eol("examples/test_extern_alias.cay")
        .expect("Extern alias test should compile and run");
    assert!(
        output.contains("=== Extern Alias Test Suite ==="),
        "Should show test suite header, got: {}",
        output
    );
    assert!(
        output.contains("All Extern Alias Tests Passed!"),
        "All extern alias tests should pass, got: {}",
        output
    );
    assert!(
        output.contains("Original function names (sin, cos, sqrt, etc.) are NOT accessible"),
        "Should note that original names are not accessible, got: {}",
        output
    );
}
