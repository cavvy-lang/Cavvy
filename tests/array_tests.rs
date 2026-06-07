//! Cavvy 语言数组功能集成测试
//!
//! 测试数组的各种功能：一维、多维、初始化、长度等

mod common;
use common::compile_and_run_eol;

// ========== 0.3.3.0 Array Features Tests ==========

#[test]
fn test_array_simple() {
    let output = compile_and_run_eol("examples/test_array_simple.cay")
        .expect("array simple example should compile and run");
    // 数组简单测试应该输出 arr[0] = 10
    assert!(
        output.contains("arr[0] = 10"),
        "Array simple test should output 'arr[0] = 10', got: {}",
        output
    );
}

#[test]
fn test_array_complex() {
    let output = compile_and_run_eol("examples/test_array.cay")
        .expect("array example should compile and run");
    // 数组复杂测试应该输出数组相关的内容
    assert!(
        output.contains("数组")
            || output.contains("arr[")
            || output.contains("sum")
            || output.contains("array"),
        "Array test should output array-related content, got: {}",
        output
    );
}

#[test]
fn test_array_init() {
    let output = compile_and_run_eol("examples/test_array_init.cay")
        .expect("array init example should compile and run");
    assert!(
        output.contains("arr1[0] = 10: PASS"),
        "Array init test should pass for arr1[0], got: {}",
        output
    );
    assert!(
        output.contains("arr1[4] = 50: PASS"),
        "Array init test should pass for arr1[4], got: {}",
        output
    );
    assert!(
        output.contains("arr1[2] = 100: PASS"),
        "Array init test should pass for arr1[2], got: {}",
        output
    );
    assert!(
        output.contains("All array init tests passed!"),
        "Array init test should complete, got: {}",
        output
    );
}

#[test]
fn test_array_length() {
    let output = compile_and_run_eol("examples/test_array_length.cay")
        .expect("array length example should compile and run");
    assert!(
        output.contains("arr1.length = 5: PASS"),
        "Array length test should pass for arr1, got: {}",
        output
    );
    assert!(
        output.contains("arr2.length = 10: PASS"),
        "Array length test should pass for arr2, got: {}",
        output
    );
    assert!(
        output.contains("Sum using length = 15: PASS"),
        "Array length test should pass for sum, got: {}",
        output
    );
    assert!(
        output.contains("All length tests passed!"),
        "Array length test should complete, got: {}",
        output
    );
}

#[test]
fn test_multidim_array() {
    let output = compile_and_run_eol("examples/test_multidim_array.cay")
        .expect("multidim array example should compile and run");
    assert!(
        output.contains("matrix[0][0] = 1: PASS"),
        "Multidim array test should pass for [0][0], got: {}",
        output
    );
    assert!(
        output.contains("matrix[0][1] = 2: PASS"),
        "Multidim array test should pass for [0][1], got: {}",
        output
    );
    assert!(
        output.contains("matrix[1][0] = 3: PASS"),
        "Multidim array test should pass for [1][0], got: {}",
        output
    );
    assert!(
        output.contains("matrix[2][3] = 4: PASS"),
        "Multidim array test should pass for [2][3], got: {}",
        output
    );
    assert!(
        output.contains("All multidim array tests passed!"),
        "Multidim array test should complete, got: {}",
        output
    );
}

#[test]
fn test_array_loop() {
    let output = compile_and_run_eol("examples/test_array_loop.cay")
        .expect("array loop example should compile and run");
    assert!(
        output.contains("Sum = 75: PASS"),
        "Array loop test should pass for sum, got: {}",
        output
    );
    assert!(
        output.contains("Product = 375000: PASS"),
        "Array loop test should pass for product, got: {}",
        output
    );
    assert!(
        output.contains("Max = 25: PASS"),
        "Array loop test should pass for max, got: {}",
        output
    );
    assert!(
        output.contains("All array loop tests passed!"),
        "Array loop test should complete, got: {}",
        output
    );
}

#[test]
fn test_array_types() {
    let output = compile_and_run_eol("examples/test_array_types.cay")
        .expect("array types example should compile and run");
    assert!(
        output.contains("long[]: PASS"),
        "Array types test should pass for long[], got: {}",
        output
    );
    assert!(
        output.contains("float[]: PASS"),
        "Array types test should pass for float[], got: {}",
        output
    );
    assert!(
        output.contains("double[]: PASS"),
        "Array types test should pass for double[], got: {}",
        output
    );
    assert!(
        output.contains("char[]: PASS"),
        "Array types test should pass for char[], got: {}",
        output
    );
    assert!(
        output.contains("bool[]: PASS"),
        "Array types test should pass for bool[], got: {}",
        output
    );
    assert!(
        output.contains("All array type tests passed!"),
        "Array types test should complete, got: {}",
        output
    );
}

#[test]
fn test_array_033() {
    let output = compile_and_run_eol("examples/test_array_033.cay")
        .expect("array 0.3.3 example should compile and run");
    assert!(
        output.contains("arr1[0] is correct"),
        "Array 0.3.3 test should pass for arr1[0], got: {}",
        output
    );
    assert!(
        output.contains("arr1[4] is correct"),
        "Array 0.3.3 test should pass for arr1[4], got: {}",
        output
    );
    assert!(
        output.contains("arr1.length is correct"),
        "Array 0.3.3 test should pass for arr1.length, got: {}",
        output
    );
    assert!(
        output.contains("arr2.length is correct"),
        "Array 0.3.3 test should pass for arr2.length, got: {}",
        output
    );
    assert!(
        output.contains("Sum is correct: 150"),
        "Array 0.3.3 test should pass for sum, got: {}",
        output
    );
    assert!(
        output.contains("All tests passed!"),
        "Array 0.3.3 test should complete, got: {}",
        output
    );
}

// ========== 0.3.4.0 Static Fields & Calloc Tests ==========

#[test]
fn test_zero_init_array() {
    let output = compile_and_run_eol("examples/test_zero_init_array.cay")
        .expect("zero init array example should compile and run");
    // 测试数组零初始化
    assert!(
        output.contains("Zero-initialized int array:"),
        "Should show int array message, got: {}",
        output
    );
    assert!(
        output.contains("Zero-initialized long array:"),
        "Should show long array message, got: {}",
        output
    );
    assert!(
        output.contains("Array without () (still zero-init):"),
        "Should show array without parens message, got: {}",
        output
    );
    // 验证零初始化测试通过
    assert!(
        output.contains("Zero initialization test PASSED!"),
        "Zero initialization test should pass, got: {}",
        output
    );
}

#[test]
fn test_static_array() {
    let output = compile_and_run_eol("examples/test_static_array.cay")
        .expect("static array example should compile and run");
    // 测试静态数组
    assert!(
        output.contains("Initial zero vector:"),
        "Should show initial vector message, got: {}",
        output
    );
    assert!(
        output.contains("After setting values:"),
        "Should show after setting values message, got: {}",
        output
    );
    // 检查和是否为60 (10+20+30)
    assert!(
        output.contains("Sum:") && output.contains("60"),
        "Sum should be 60, got: {}",
        output
    );
}

#[test]
fn test_array_initializer() {
    let output = compile_and_run_eol("examples/test_array_initializer.cay")
        .expect("array initializer example should compile and run");
    assert!(
        output.contains("arr1[0] = 10"),
        "Array initializer should work for int[], got: {}",
        output
    );
    assert!(
        output.contains("arr1[2] = 30"),
        "Array element access should work, got: {}",
        output
    );
    assert!(
        output.contains("arr1.length = 5"),
        "Array length should work, got: {}",
        output
    );
    assert!(
        output.contains("All array initializer tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

// ========== 新增数组测试 ==========

#[test]
fn test_array_1d() {
    let output = compile_and_run_eol("examples/test_array_1d.cay")
        .expect("array 1d example should compile and run");
    assert!(
        output.contains("arr[0] = 10") && output.contains("arr[4] = 50"),
        "1D array should work, got: {}",
        output
    );
}

#[test]
fn test_array_2d() {
    let output = compile_and_run_eol("examples/test_array_2d.cay")
        .expect("array 2d example should compile and run");
    assert!(
        output.contains("matrix[0][0] = 1") && output.contains("matrix[2][2] = 9"),
        "2D array should work, got: {}",
        output
    );
}

#[test]
fn test_array_init_inline() {
    let output = compile_and_run_eol("examples/test_array_init_inline.cay")
        .expect("array init inline example should compile and run");
    assert!(
        output.contains("arr[0] = 1") && output.contains("arr[4] = 5"),
        "Array inline init should work, got: {}",
        output
    );
}

#[test]
fn test_array_sum() {
    let output = compile_and_run_eol("examples/test_array_sum.cay")
        .expect("array sum example should compile and run");
    assert!(
        output.contains("Sum = 150"),
        "Array sum should work, got: {}",
        output
    );
}

#[test]
fn test_array_find_max() {
    let output = compile_and_run_eol("examples/test_array_find_max.cay")
        .expect("array find max example should compile and run");
    assert!(
        output.contains("Max = 42"),
        "Array find max should work, got: {}",
        output
    );
}

#[test]
fn test_array_reverse() {
    let output = compile_and_run_eol("examples/test_array_reverse.cay")
        .expect("array reverse example should compile and run");
    assert!(
        output.contains("Original: 1, 2, 3, 4, 5") && output.contains("Reversed: 5, 4, 3, 2, 1"),
        "Array reverse should work, got: {}",
        output
    );
}

// ========== 新增大型功能测试 ==========

#[test]
fn test_array_matrix_multiply() {
    let output = compile_and_run_eol("examples/test_array_matrix_multiply.cay")
        .expect("array matrix multiply should compile and run");
    assert!(
        output.contains("PASSED"),
        "Array matrix multiply test should pass, got: {}",
        output
    );
}

#[test]
fn test_array_transpose() {
    let output = compile_and_run_eol("examples/test_array_transpose.cay")
        .expect("array transpose should compile and run");
    assert!(
        output.contains("PASSED"),
        "Array transpose test should pass, got: {}",
        output
    );
}

#[test]
fn test_array_large_1d() {
    let output = compile_and_run_eol("examples/test_array_large_1d.cay")
        .expect("array large 1d should compile and run");
    assert!(
        output.contains("PASSED"),
        "Array large 1D test should pass, got: {}",
        output
    );
}

#[test]
fn test_array_3d() {
    let output =
        compile_and_run_eol("examples/test_array_3d.cay").expect("array 3d should compile and run");
    assert!(
        output.contains("completed"),
        "Array 3D test should complete, got: {}",
        output
    );
}

#[test]
fn test_array_jagged() {
    let output = compile_and_run_eol("examples/test_array_jagged.cay")
        .expect("array jagged should compile and run");
    assert!(
        output.contains("completed"),
        "Array jagged test should complete, got: {}",
        output
    );
}

#[test]
fn test_calloc_integration() {
    let output = compile_and_run_eol("examples/test_calloc_integration.cay")
        .expect("calloc integration example should compile and run");
    // 测试初始统计值
    assert!(
        output.contains("Initial Statistics (should be all 0)"),
        "Should show initial stats message, got: {}",
        output
    );
    // 测试添加值后的统计
    assert!(
        output.contains("Adding values: 10, 20, 30, 40, 50"),
        "Should show adding values message, got: {}",
        output
    );
    assert!(
        output.contains("Count:") && output.contains("5"),
        "Count should be 5, got: {}",
        output
    );
    assert!(
        output.contains("Sum:") && output.contains("150"),
        "Sum should be 150, got: {}",
        output
    );
    assert!(
        output.contains("Average:") && output.contains("30"),
        "Average should be 30, got: {}",
        output
    );
    // 测试数组零初始化（添加了3个零，count变为8）
    assert!(
        output.contains("After adding 3 zeros:"),
        "Should show after adding zeros message, got: {}",
        output
    );
    assert!(
        output.contains("Count:") && output.contains("8"),
        "Count should be 8 after adding zeros, got: {}",
        output
    );
}

#[test]
fn test_edge_cases() {
    let output = compile_and_run_eol("examples/test_edge_cases.cay")
        .expect("edge cases example should compile and run");
    // 测试边界情况
    assert!(
        output.contains("=== Edge Case Tests ==="),
        "Should show edge case test header, got: {}",
        output
    );
    assert!(
        output.contains("Test 1: Empty array"),
        "Should test empty array, got: {}",
        output
    );
    assert!(
        output.contains("Empty array created, length =") && output.contains("0"),
        "Empty array should have length 0, got: {}",
        output
    );
    assert!(
        output.contains("Test 2: Single element array"),
        "Should test single element array, got: {}",
        output
    );
    assert!(
        output.contains("Single element:") && output.contains("99"),
        "Single element should be 99, got: {}",
        output
    );
    assert!(
        output.contains("Test 3: Deep recursion"),
        "Should test deep recursion, got: {}",
        output
    );
    assert!(
        output.contains("Test 4: Fibonacci recursion"),
        "Should test fibonacci recursion, got: {}",
        output
    );
    assert!(
        output.contains("fib(0) = 0"),
        "fib(0) should be 0, got: {}",
        output
    );
    assert!(
        output.contains("fib(10) = 55"),
        "fib(10) should be 55, got: {}",
        output
    );
    assert!(
        output.contains("Test 5: Large array"),
        "Should test large array, got: {}",
        output
    );
    assert!(
        output.contains("Test 6: Negative numbers"),
        "Should test negative numbers, got: {}",
        output
    );
    assert!(
        output.contains("Absolute value:") && output.contains("100"),
        "Absolute value should be 100, got: {}",
        output
    );
    assert!(
        output.contains("Test 7: Zero values"),
        "Should test zero values, got: {}",
        output
    );
    assert!(
        output.contains("Test 8: Large numbers"),
        "Should test large numbers, got: {}",
        output
    );
    assert!(
        output.contains("=== All edge case tests PASSED! ==="),
        "Edge case tests should pass, got: {}",
        output
    );
}
