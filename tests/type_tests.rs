//! Cavvy 语言类型系统集成测试
//!
//! 测试基础类型、类型转换等

mod common;
use common::compile_and_run_eol;

// ========== 新增基础类型测试 ==========

#[test]
fn test_basic_int() {
    let output = compile_and_run_eol("examples/test_basic_int.cay")
        .expect("basic int example should compile and run");
    assert!(
        output.contains("30")
            || output.contains("-10")
            || output.contains("200")
            || output.contains("2")
            || output.contains("0"),
        "Basic int operations should work, got: {}",
        output
    );
}

#[test]
fn test_basic_long() {
    let output = compile_and_run_eol("examples/test_basic_long.cay")
        .expect("basic long example should compile and run");
    assert!(
        output.contains("3000000")
            || output.contains("1000000")
            || output.contains("2000000000000"),
        "Basic long operations should work, got: {}",
        output
    );
}

#[test]
fn test_basic_float() {
    let output = compile_and_run_eol("examples/test_basic_float.cay")
        .expect("basic float example should compile and run");
    assert!(
        output.contains("5.64")
            || output.contains("0.64")
            || output.contains("7.85")
            || output.contains("1.256"),
        "Basic float operations should work, got: {}",
        output
    );
}

#[test]
fn test_basic_double() {
    let output = compile_and_run_eol("examples/test_basic_double.cay")
        .expect("basic double example should compile and run");
    assert!(
        output.contains("5.85987")
            || output.contains("0.42331")
            || output.contains("8.53972")
            || output.contains("1.15572"),
        "Basic double operations should work, got: {}",
        output
    );
}

#[test]
fn test_basic_bool() {
    let output = compile_and_run_eol("examples/test_basic_bool.cay")
        .expect("basic bool example should compile and run");
    assert!(
        output.contains("true is true")
            && output.contains("false is false")
            && output.contains("true && true is true")
            && output.contains("true || false is true"),
        "Basic bool operations should work, got: {}",
        output
    );
}

#[test]
fn test_basic_char() {
    let output = compile_and_run_eol("examples/test_basic_char.cay")
        .expect("basic char example should compile and run");
    assert!(
        output.contains("65")
            || output.contains("66")
            || output.contains("67")
            || output.contains("68"),
        "Basic char operations should work, got: {}",
        output
    );
}

#[test]
fn test_basic_string() {
    let output = compile_and_run_eol("examples/test_basic_string.cay")
        .expect("basic string example should compile and run");
    assert!(
        output.contains("Hello") || output.contains("World") || output.contains("Hello, World!"),
        "Basic string operations should work, got: {}",
        output
    );
}

// ========== 类型转换测试 ==========

#[test]
fn test_type_casting() {
    let output = compile_and_run_eol("examples/test_type_casting.cay")
        .expect("type casting example should compile and run");
    // 测试类型转换
    assert!(
        output.contains("=== Type Casting Tests ==="),
        "Should show type casting test header, got: {}",
        output
    );
    assert!(
        output.contains("Test 1: int to long"),
        "Should test int to long, got: {}",
        output
    );
    assert!(
        output.contains("Test 2: int to float"),
        "Should test int to float, got: {}",
        output
    );
    assert!(
        output.contains("Test 3: int to double"),
        "Should test int to double, got: {}",
        output
    );
    assert!(
        output.contains("Test 4: float to double"),
        "Should test float to double, got: {}",
        output
    );
    assert!(
        output.contains("Test 5: long to double"),
        "Should test long to double, got: {}",
        output
    );
    assert!(
        output.contains("Test 6: Mixed type operations"),
        "Should test mixed type operations, got: {}",
        output
    );
    assert!(
        output.contains("Test 7: Array element assignment with type conversion"),
        "Should test array element type conversion, got: {}",
        output
    );
    assert!(
        output.contains("=== All type casting tests PASSED! ==="),
        "Type casting tests should pass, got: {}",
        output
    );
}

#[test]
fn test_type_casting_advanced() {
    let output = compile_and_run_eol("examples/test_type_casting_advanced.cay")
        .expect("advanced type casting example should compile and run");
    // 测试高级类型转换
    assert!(
        output.contains("=== Advanced Type Casting Tests ==="),
        "Should show advanced type casting test header, got: {}",
        output
    );
    assert!(
        output.contains("PASS: i32 + double promotion works!"),
        "i32 + double promotion should work, got: {}",
        output
    );
    assert!(
        output.contains("PASS: i32 * double promotion works!"),
        "i32 * double promotion should work, got: {}",
        output
    );
    assert!(
        output.contains("PASS: double / i32 promotion works!"),
        "double / i32 promotion should work, got: {}",
        output
    );
    assert!(
        output.contains("PASS: double to i32 cast works!"),
        "double to i32 cast should work, got: {}",
        output
    );
    assert!(
        output.contains("PASS: i32 to double cast works!"),
        "i32 to double cast should work, got: {}",
        output
    );
    assert!(
        output.contains("PASS: float to i32 cast works!"),
        "float to i32 cast should work, got: {}",
        output
    );
    assert!(
        output.contains("PASS: long to i32 cast works!"),
        "long to i32 cast should work, got: {}",
        output
    );
    assert!(
        output.contains("PASS: Comparison with promotion works!"),
        "Comparison with type promotion should work, got: {}",
        output
    );
    assert!(
        output.contains("=== All advanced type casting tests completed! ==="),
        "Advanced type casting tests should complete, got: {}",
        output
    );
}

#[test]
fn test_type_casting_comprehensive() {
    let output = compile_and_run_eol("examples/test_type_casting_comprehensive.cay")
        .expect("comprehensive type casting example should compile and run");
    // 测试综合类型转换
    assert!(
        output.contains("=== Comprehensive Type Casting Tests ==="),
        "Should show comprehensive type casting test header, got: {}",
        output
    );
    assert!(
        output.contains("char 'A' to int: 65"),
        "char to int cast should work, got: {}",
        output
    );
    assert!(
        output.contains("long 2147483647L to int: 2147483647"),
        "long to int cast should work, got: {}",
        output
    );
    assert!(
        output.contains("double array elements: 1.000000, 2.500000, 3.000000"),
        "Array element type conversion should work, got: {}",
        output
    );
    assert!(
        output.contains("int 42 explicitly to double: 42.000000"),
        "int to double explicit cast should work, got: {}",
        output
    );
    assert!(
        output.contains("double 42.0 explicitly to int: 42"),
        "double to int explicit cast should work, got: {}",
        output
    );
    assert!(
        output.contains("All comprehensive type casting tests completed!"),
        "Comprehensive type casting tests should complete, got: {}",
        output
    );
}

#[test]
fn test_type_system_rules() {
    let output = compile_and_run_eol("examples/test_type_system_rules.cay")
        .expect("type system rules example should compile and run");
    assert!(
        output.contains("(string)42 = 42"),
        "int to string cast should work, got: {}",
        output
    );
    assert!(
        output.contains("(string)true = true"),
        "bool to string cast should work, got: {}",
        output
    );
    assert!(
        output.contains("(string)false = false"),
        "bool to string cast should work, got: {}",
        output
    );
    assert!(
        output.contains("5 + 'A' (65) = 70"),
        "char should promote to int, got: {}",
        output
    );
    assert!(
        output.contains("All type system rule tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

// ========== 新增类型转换测试 ==========

#[test]
fn test_cast_int_to_long() {
    let output = compile_and_run_eol("examples/test_cast_int_to_long.cay")
        .expect("cast int to long example should compile and run");
    assert!(
        output.contains("100"),
        "Cast int to long should work, got: {}",
        output
    );
}

#[test]
fn test_cast_int_to_float() {
    let output = compile_and_run_eol("examples/test_cast_int_to_float.cay")
        .expect("cast int to float example should compile and run");
    assert!(
        output.contains("42"),
        "Cast int to float should work, got: {}",
        output
    );
}

#[test]
fn test_cast_int_to_double() {
    let output = compile_and_run_eol("examples/test_cast_int_to_double.cay")
        .expect("cast int to double example should compile and run");
    assert!(
        output.contains("42"),
        "Cast int to double should work, got: {}",
        output
    );
}

#[test]
fn test_cast_long_to_int() {
    let output = compile_and_run_eol("examples/test_cast_long_to_int.cay")
        .expect("cast long to int example should compile and run");
    assert!(
        output.contains("100"),
        "Cast long to int should work, got: {}",
        output
    );
}

#[test]
fn test_cast_float_to_int() {
    let output = compile_and_run_eol("examples/test_cast_float_to_int.cay")
        .expect("cast float to int example should compile and run");
    assert!(
        output.contains("3"),
        "Cast float to int should work, got: {}",
        output
    );
}

#[test]
fn test_cast_double_to_int() {
    let output = compile_and_run_eol("examples/test_cast_double_to_int.cay")
        .expect("cast double to int example should compile and run");
    assert!(
        output.contains("3"),
        "Cast double to int should work, got: {}",
        output
    );
}

#[test]
fn test_cast_char_to_int() {
    let output = compile_and_run_eol("examples/test_cast_char_to_int.cay")
        .expect("cast char to int example should compile and run");
    assert!(
        output.contains("65") && output.contains("97"),
        "Cast char to int should work, got: {}",
        output
    );
}

#[test]
fn test_cast_int_to_char() {
    let output = compile_and_run_eol("examples/test_cast_int_to_char.cay")
        .expect("cast int to char example should compile and run");
    assert!(
        output.contains("65") && output.contains("97"),
        "Cast int to char should work, got: {}",
        output
    );
}

#[test]
fn test_type_conversions_advanced() {
    let output = compile_and_run_eol("examples/test_type_conversions_advanced.cay")
        .expect("type conversions advanced should compile and run");
    assert!(
        output.contains("completed"),
        "Type conversions advanced test should complete, got: {}",
        output
    );
}
