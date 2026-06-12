//! 内联IR集成测试
//!
//! 测试内联IR功能的端到端集成

mod common;
use common::compile_and_run_eol;

// ============================================================
// 基础内联IR测试
// ============================================================

#[test]
fn test_inline_ir_basic() {
    let output = compile_and_run_eol("examples/test_inline_ir_basic.cay")
        .expect("test_inline_ir_basic.cay should compile and run");

    assert!(
        output.contains("10 + 20 = 30"),
        "Addition should work, got: {}",
        output
    );
    assert!(
        output.contains("5 * 6 = 30"),
        "Multiplication should work, got: {}",
        output
    );
    assert!(
        output.contains("30 - 15 = 15"),
        "Subtraction should work, got: {}",
        output
    );
    assert!(
        output.contains("100 / 4 = 25"),
        "Division should work, got: {}",
        output
    );
    assert!(
        output.contains("17 % 5 = 2"),
        "Modulo should work, got: {}",
        output
    );
    assert!(
        output.contains("All basic inline IR tests passed!"),
        "All tests should pass, got: {}",
        output
    );
}

#[test]
fn test_inline_ir_float() {
    let output = compile_and_run_eol("examples/test_inline_ir_float.cay")
        .expect("test_inline_ir_float.cay should compile and run");

    assert!(
        output.contains("3.5 + 2.5 = 6.0") || output.contains("3.5 + 2.5 = 6"),
        "Float addition should work, got: {}",
        output
    );
    assert!(
        output.contains("5.0 - 2.5 = 2.5") || output.contains("5.0 - 2.5 = 2"),
        "Float subtraction should work, got: {}",
        output
    );
    assert!(
        output.contains("2.0 * 3.0 = 6.0") || output.contains("2.0 * 3.0 = 6"),
        "Float multiplication should work, got: {}",
        output
    );
    assert!(
        output.contains("10.0 / 2.0 = 5.0") || output.contains("10.0 / 2.0 = 5"),
        "Float division should work, got: {}",
        output
    );
    assert!(
        output.contains("Float inline IR tests completed!"),
        "Tests should complete, got: {}",
        output
    );
}

#[test]
fn test_inline_ir_bitwise() {
    let output = compile_and_run_eol("examples/test_inline_ir_bitwise.cay")
        .expect("test_inline_ir_bitwise.cay should compile and run");

    assert!(
        output.contains("1100 & 1010 = 8"),
        "Bitwise AND should work, got: {}",
        output
    );
    assert!(
        output.contains("1100 | 1010 = 14"),
        "Bitwise OR should work, got: {}",
        output
    );
    assert!(
        output.contains("1100 ^ 1010 = 6"),
        "Bitwise XOR should work, got: {}",
        output
    );
    assert!(
        output.contains("1 << 4 = 16"),
        "Left shift should work, got: {}",
        output
    );
    assert!(
        output.contains("-16 >> 2 = -4"),
        "Arithmetic right shift should work, got: {}",
        output
    );
    assert!(
        output.contains("16 >>> 2 = 4"),
        "Logical right shift should work, got: {}",
        output
    );
    assert!(
        output.contains("All bitwise tests passed!"),
        "All tests should pass, got: {}",
        output
    );
}

#[test]
fn test_inline_ir_comparison() {
    let output = compile_and_run_eol("examples/test_inline_ir_comparison.cay")
        .expect("test_inline_ir_comparison.cay should compile and run");

    assert!(
        output.contains("5 == 5: true"),
        "Integer equality should work, got: {}",
        output
    );
    assert!(
        output.contains("5 == 3: false"),
        "Integer inequality should work, got: {}",
        output
    );
    assert!(
        output.contains("3 < 5: true"),
        "Integer less than should work, got: {}",
        output
    );
    assert!(
        output.contains("3.5 == 3.5: true"),
        "Float equality should work, got: {}",
        output
    );
    assert!(
        output.contains("2.5 < 3.5: true"),
        "Float less than should work, got: {}",
        output
    );
    assert!(
        output.contains("Comparison tests completed!"),
        "Tests should complete, got: {}",
        output
    );
}

// ============================================================
// 内存操作测试
// ============================================================

#[test]
fn test_inline_ir_memory() {
    let output = compile_and_run_eol("examples/test_inline_ir_memory.cay")
        .expect("test_inline_ir_memory.cay should compile and run");

    assert!(
        output.contains("Stack alloc test: 42"),
        "Stack allocation should work, got: {}",
        output
    );
    assert!(
        output.contains("arr[0] = 10"),
        "Array access should work, got: {}",
        output
    );
    assert!(
        output.contains("arr[2] = 30"),
        "Array access at index 2 should work, got: {}",
        output
    );
    assert!(
        output.contains("Multi store test: 30"),
        "Multiple store operations should work, got: {}",
        output
    );
    assert!(
        output.contains("All memory tests passed!"),
        "All tests should pass, got: {}",
        output
    );
}

// ============================================================
// 类型转换测试
// ============================================================

#[test]
fn test_inline_ir_typecast() {
    let output = compile_and_run_eol("examples/test_inline_ir_typecast.cay")
        .expect("test_inline_ir_typecast.cay should compile and run");

    assert!(
        output.contains("Sext(-100) = -100"),
        "Sign extension should work, got: {}",
        output
    );
    assert!(
        output.contains("Zext(100) = 100"),
        "Zero extension should work, got: {}",
        output
    );
    assert!(
        output.contains("Sitofp(42) = 42.0") || output.contains("Sitofp(42) = 42"),
        "Integer to float conversion should work, got: {}",
        output
    );
    assert!(
        output.contains("Fptosi(3.7) = 3"),
        "Float to integer conversion should work, got: {}",
        output
    );
    assert!(
        output.contains("Typecast tests completed!"),
        "Tests should complete, got: {}",
        output
    );
}

// ============================================================
// 复杂表达式测试
// ============================================================

#[test]
fn test_inline_ir_complex() {
    let output = compile_and_run_eol("examples/test_inline_ir_complex.cay")
        .expect("test_inline_ir_complex.cay should compile and run");

    assert!(
        output.contains("(2+3)*(5-1) = 20"),
        "Complex expression should work, got: {}",
        output
    );
    assert!(
        output.contains("2*2^2 + 3*2 + 4 = 18"),
        "Polynomial calculation should work, got: {}",
        output
    );
    assert!(
        output.contains("Abs(-42) = 42"),
        "Absolute value should work for negative, got: {}",
        output
    );
    assert!(
        output.contains("Abs(42) = 42"),
        "Absolute value should work for positive, got: {}",
        output
    );
    assert!(
        output.contains("Max(10, 5) = 10"),
        "Max function should work, got: {}",
        output
    );
    assert!(
        output.contains("Min(10, 5) = 5"),
        "Min function should work, got: {}",
        output
    );
    assert!(
        output.contains("All complex tests passed!"),
        "All tests should pass, got: {}",
        output
    );
}

// ============================================================
// 数学函数测试
// ============================================================

#[test]
fn test_inline_ir_math() {
    let output = compile_and_run_eol("examples/test_inline_ir_math.cay")
        .expect("test_inline_ir_math.cay should compile and run");

    assert!(
        output.contains("Square(5) = 25"),
        "Square function should work, got: {}",
        output
    );
    assert!(
        output.contains("Cube(3) = 27"),
        "Cube function should work, got: {}",
        output
    );
    assert!(
        output.contains("2^0 = 1"),
        "Power of 2 (0) should work, got: {}",
        output
    );
    assert!(
        output.contains("2^4 = 16"),
        "Power of 2 (4) should work, got: {}",
        output
    );
    assert!(
        output.contains("2^8 = 256"),
        "Power of 2 (8) should work, got: {}",
        output
    );
    assert!(
        output.contains("IsPowerOfTwo(1): true"),
        "IsPowerOfTwo(1) should be true, got: {}",
        output
    );
    assert!(
        output.contains("IsPowerOfTwo(2): true"),
        "IsPowerOfTwo(2) should be true, got: {}",
        output
    );
    assert!(
        output.contains("IsPowerOfTwo(4): true"),
        "IsPowerOfTwo(4) should be true, got: {}",
        output
    );
    assert!(
        output.contains("IsPowerOfTwo(3): false"),
        "IsPowerOfTwo(3) should be false, got: {}",
        output
    );
    assert!(
        output.contains("All math tests passed!"),
        "All tests should pass, got: {}",
        output
    );
}

#[test]
fn test_inline_ir_private_method() {
    let output = compile_and_run_eol("examples/test_inline_ir_private_method.cay")
        .expect("test_inline_ir_private_method.cay should compile and run");

    assert!(
        output.contains("private inline ir = 43"),
        "Private methods should allow inline IR blocks, got: {}",
        output
    );
}
