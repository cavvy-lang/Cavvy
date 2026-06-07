//! Cavvy 语言高级功能集成测试
//!
//! 测试高级算法、数据结构、数学运算等

mod common;
use common::compile_and_run_eol;

#[test]
fn test_algorithm_sorting() {
    let output = compile_and_run_eol("examples/test_algorithm_sorting.cay")
        .expect("algorithm sorting should compile and run");
    assert!(
        output.contains("PASSED"),
        "Algorithm sorting test should pass, got: {}",
        output
    );
}

#[test]
fn test_algorithm_search() {
    let output = compile_and_run_eol("examples/test_algorithm_search.cay")
        .expect("algorithm search should compile and run");
    assert!(
        output.contains("PASSED"),
        "Algorithm search test should pass, got: {}",
        output
    );
}

#[test]
fn test_math_operations() {
    let output = compile_and_run_eol("examples/test_math_operations.cay")
        .expect("math operations should compile and run");
    assert!(
        output.contains("completed"),
        "Math operations test should complete, got: {}",
        output
    );
}

#[test]
fn test_static_variables() {
    let output = compile_and_run_eol("examples/test_static_variables.cay")
        .expect("static variables should compile and run");
    assert!(
        output.contains("PASSED"),
        "Static variables test should pass, got: {}",
        output
    );
}

#[test]
fn test_data_structures() {
    let output = compile_and_run_eol("examples/test_data_structures.cay")
        .expect("data structures should compile and run");
    assert!(
        output.contains("completed"),
        "Data structures test should complete, got: {}",
        output
    );
}

#[test]
fn test_pointer_simulation() {
    let output = compile_and_run_eol("examples/test_pointer_simulation.cay")
        .expect("pointer simulation should compile and run");
    assert!(
        output.contains("PASSED"),
        "Pointer simulation test should pass, got: {}",
        output
    );
}

#[test]
fn test_number_theory() {
    let output = compile_and_run_eol("examples/test_number_theory.cay")
        .expect("number theory should compile and run");
    assert!(
        output.contains("completed"),
        "Number theory test should complete, got: {}",
        output
    );
}

#[test]
fn test_floating_point_advanced() {
    let output = compile_and_run_eol("examples/test_floating_point_advanced.cay")
        .expect("floating point advanced should compile and run");
    assert!(
        output.contains("completed"),
        "Floating point advanced test should complete, got: {}",
        output
    );
}

#[test]
fn test_game_of_life() {
    let output = compile_and_run_eol("examples/test_game_of_life.cay")
        .expect("game of life should compile and run");
    assert!(
        output.contains("completed"),
        "Game of life test should complete, got: {}",
        output
    );
}

#[test]
fn test_prime_sieve() {
    let output = compile_and_run_eol("examples/test_prime_sieve.cay")
        .expect("prime sieve should compile and run");
    assert!(
        output.contains("PASSED"),
        "Prime sieve test should pass, got: {}",
        output
    );
}

#[test]
fn test_matrix_determinant() {
    let output = compile_and_run_eol("examples/test_matrix_determinant.cay")
        .expect("matrix determinant should compile and run");
    assert!(
        output.contains("completed"),
        "Matrix determinant test should complete, got: {}",
        output
    );
}

#[test]
fn test_histogram() {
    let output = compile_and_run_eol("examples/test_histogram.cay")
        .expect("histogram should compile and run");
    assert!(
        output.contains("PASSED"),
        "Histogram test should pass, got: {}",
        output
    );
}

#[test]
fn test_fibonacci_large() {
    let output = compile_and_run_eol("examples/test_fibonacci_large.cay")
        .expect("fibonacci large should compile and run");
    assert!(
        output.contains("completed"),
        "Fibonacci large test should complete, got: {}",
        output
    );
}

#[test]
fn test_permutations() {
    let output = compile_and_run_eol("examples/test_permutations.cay")
        .expect("permutations should compile and run");
    assert!(
        output.contains("PASSED"),
        "Permutations test should pass, got: {}",
        output
    );
}

#[test]
fn test_combinations() {
    let output = compile_and_run_eol("examples/test_combinations.cay")
        .expect("combinations should compile and run");
    assert!(
        output.contains("completed"),
        "Combinations test should complete, got: {}",
        output
    );
}

#[test]
fn test_roman_numerals() {
    let output = compile_and_run_eol("examples/test_roman_numerals.cay")
        .expect("roman numerals should compile and run");
    assert!(
        output.contains("PASSED"),
        "Roman numerals test should pass, got: {}",
        output
    );
}

#[test]
fn test_base_conversion() {
    let output = compile_and_run_eol("examples/test_base_conversion.cay")
        .expect("base conversion should compile and run");
    assert!(
        output.contains("PASSED"),
        "Base conversion test should pass, got: {}",
        output
    );
}

#[test]
fn test_memoization() {
    let output = compile_and_run_eol("examples/test_memoization.cay")
        .expect("memoization example should compile and run");
    // 测试斐波那契数列
    assert!(
        output.contains("Fibonacci numbers:"),
        "Should show fibonacci header, got: {}",
        output
    );
    assert!(
        output.contains("F(0) = 0"),
        "F(0) should be 0, got: {}",
        output
    );
    assert!(
        output.contains("F(1) = 1"),
        "F(1) should be 1, got: {}",
        output
    );
    assert!(
        output.contains("F(10) = 55"),
        "F(10) should be 55, got: {}",
        output
    );
    assert!(
        output.contains("F(20) = 6765"),
        "F(20) should be 6765, got: {}",
        output
    );
    assert!(
        output.contains("F(40) =") && output.contains("102334155"),
        "F(40) should be 102334155, got: {}",
        output
    );
}

#[test]
fn test_scope_isolation() {
    let output = compile_and_run_eol("examples/test_scope_isolation.cay")
        .expect("scope isolation example should compile and run");
    // 测试作用域隔离
    assert!(
        output.contains("Before if: x =") && output.contains("100"),
        "Should show initial x value, got: {}",
        output
    );
    assert!(
        output.contains("In if branch: newVal =") && output.contains("10"),
        "Should show if branch newVal, got: {}",
        output
    );
    assert!(
        output.contains("After modify in if: newVal =") && output.contains("15"),
        "Should show modified newVal in if branch, got: {}",
        output
    );
    assert!(
        output.contains("After if: x =") && output.contains("100"),
        "x should remain unchanged after if block, got: {}",
        output
    );
    assert!(
        output.contains("Scope isolation test PASSED!"),
        "Scope isolation test should pass, got: {}",
        output
    );
}

#[test]
fn test_class_naming() {
    let output = compile_and_run_eol("examples/test_class_naming.cay")
        .expect("class naming example should compile and run");
    // 测试类名规范
    assert!(
        output.contains("Class naming test:"),
        "Should show class naming test header, got: {}",
        output
    );
    assert!(
        output.contains("Filename: test_class_naming.cay"),
        "Should show filename, got: {}",
        output
    );
    assert!(
        output.contains("Class name: TestClassNaming"),
        "Should show class name, got: {}",
        output
    );
    assert!(
        output.contains("Naming convention test PASSED!"),
        "Naming convention test should pass, got: {}",
        output
    );
}
