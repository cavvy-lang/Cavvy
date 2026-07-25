//! Cavvy 语言静态字段功能集成测试
//!
//! 测试静态字段、零初始化数组、静态数组等

mod common;
use common::compile_and_run_eol;

// ========== 0.3.4.0 Static Fields & Calloc Tests ==========

#[test]
fn test_static_fields() {
    let output = compile_and_run_eol("examples/test_static_fields.cay")
        .expect("static fields example should compile and run");
    // 测试静态字段初始值为0
    assert!(
        output.contains("Initial count:") && output.contains("0"),
        "Static fields should be zero-initialized, got: {}",
        output
    );
    assert!(
        output.contains("Initial total:") && output.contains("0"),
        "Static fields should be zero-initialized, got: {}",
        output
    );
    // 测试增量后的值
    assert!(
        output.contains("After 3 increments:"),
        "Should show after increments message, got: {}",
        output
    );
}

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
fn test_static_string_field() {
    let output = compile_and_run_eol("examples/test_static_string_field.cay")
        .expect("static string field example should compile and run");
    assert!(
        output.contains("cavvy"),
        "static string field should be initialized to \"cavvy\", got: {}",
        output
    );
    assert!(
        output.contains("hello world"),
        "second static string field should be initialized, got: {}",
        output
    );
    assert!(
        output.contains("100"),
        "static int field should still be initialized, got: {}",
        output
    );
    assert!(
        output.contains("changed"),
        "static string field reassignment should work, got: {}",
        output
    );
    assert!(
        !output.contains("(null)"),
        "static string field must not read as null, got: {}",
        output
    );
    assert!(
        output.contains("done"),
        "test should run to completion, got: {}",
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
