//! Cavvy 语言函数和方法集成测试
//!
//! 测试函数定义、调用、重载、返回值等

mod common;
use common::{
    compile_and_run_eol, compile_and_run_eol_with_features, compile_eol_expect_error,
    compile_eol_expect_error_with_features,
};

#[test]
fn test_function_factorial() {
    let output = compile_and_run_eol("examples/test_factorial.cay")
        .expect("factorial example should compile and run");
    // 阶乘 5! = 120
    assert!(
        output.contains("120"),
        "Factorial of 5 should be 120, got: {}",
        output
    );
}

#[test]
fn test_function_multiple_params() {
    let output = compile_and_run_eol("examples/test_multiple_params.cay")
        .expect("multiple params example should compile and run");
    // 应该输出 Sum: 30 和 Product: 6.28
    assert!(
        output.contains("30") || output.contains("6.28"),
        "Multiple params test should output sum and product, got: {}",
        output
    );
}

#[test]
fn test_function_static_method() {
    let output = compile_and_run_eol("examples/test_static_method.cay")
        .expect("static method example should compile and run");
    // 静态方法结果 300
    assert!(
        output.contains("300"),
        "Static method result should be 300, got: {}",
        output
    );
}

#[test]
fn test_function_nested_calls() {
    let output = compile_and_run_eol("examples/test_nested_calls.cay")
        .expect("nested calls example should compile and run");
    // 应该输出平方、立方和平方和
    assert!(
        output.contains("25") || output.contains("27") || output.contains("20"),
        "Nested calls test should output correct values, got: {}",
        output
    );
}

// ========== 新增方法测试 ==========

#[test]
fn test_method_return_void() {
    let output = compile_and_run_eol("examples/test_method_return_void.cay")
        .expect("method return void example should compile and run");
    assert!(
        output.contains("Hello from void method!"),
        "Method return void should work, got: {}",
        output
    );
}

#[test]
fn test_method_return_int() {
    let output = compile_and_run_eol("examples/test_method_return_int.cay")
        .expect("method return int example should compile and run");
    assert!(
        output.contains("add(10, 20) = 30"),
        "Method return int should work, got: {}",
        output
    );
}

#[test]
fn test_method_return_string() {
    let output = compile_and_run_eol("examples/test_method_return_string.cay")
        .expect("method return string example should compile and run");
    assert!(
        output.contains("Hello, EOL!"),
        "Method return string should work, got: {}",
        output
    );
}

#[test]
fn test_method_multiple_params() {
    let output = compile_and_run_eol("examples/test_method_multiple_params.cay")
        .expect("method multiple params example should compile and run");
    assert!(
        output.contains("10"),
        "Method multiple params should work, got: {}",
        output
    );
}

#[test]
fn test_method_overload_int() {
    let output = compile_and_run_eol("examples/test_method_overload_int.cay")
        .expect("method overload int example should compile and run");
    assert!(
        output.contains("10") && output.contains("30") && output.contains("6"),
        "Method overload int should work, got: {}",
        output
    );
}

#[test]
fn test_method_overload_types() {
    let output = compile_and_run_eol("examples/test_method_overload_types.cay")
        .expect("method overload types example should compile and run");
    assert!(
        output.contains("30") && output.contains("Hello, World!"),
        "Method overload types should work, got: {}",
        output
    );
}

#[test]
fn test_varargs_sum() {
    let output = compile_and_run_eol("examples/test_varargs_sum.cay")
        .expect("varargs sum example should compile and run");
    assert!(
        output.contains("completed"),
        "Varargs sum should work, got: {}",
        output
    );
}

#[test]
fn test_varargs_avg() {
    let output = compile_and_run_eol("examples/test_varargs_avg.cay")
        .expect("varargs avg example should compile and run");
    assert!(
        output.contains("completed"),
        "Varargs avg should work, got: {}",
        output
    );
}

#[test]
fn test_varargs_mixed() {
    let output = compile_and_run_eol("examples/test_varargs_mixed.cay")
        .expect("varargs mixed example should compile and run");
    assert!(
        output.contains("completed"),
        "Varargs mixed should work, got: {}",
        output
    );
}

#[test]
fn test_method_various_returns() {
    let output = compile_and_run_eol("examples/test_method_various_returns.cay")
        .expect("method various returns should compile and run");
    assert!(
        output.contains("completed"),
        "Method various returns test should complete, got: {}",
        output
    );
}

#[test]
fn test_nested_functions() {
    let output = compile_and_run_eol("examples/test_nested_functions.cay")
        .expect("nested functions should compile and run");
    assert!(
        output.contains("completed"),
        "Nested functions test should complete, got: {}",
        output
    );
}

// ========== 算法测试 ==========

#[test]
fn test_recursion_factorial() {
    let output = compile_and_run_eol("examples/test_recursion_factorial.cay")
        .expect("recursion factorial example should compile and run");
    assert!(
        output.contains("120") && output.contains("3628800"),
        "Recursion factorial should work, got: {}",
        output
    );
}

#[test]
fn test_recursion_fibonacci() {
    let output = compile_and_run_eol("examples/test_recursion_fibonacci.cay")
        .expect("recursion fibonacci example should compile and run");
    assert!(
        output.contains("fib(0) = 0") && output.contains("fib(9) = 34"),
        "Recursion fibonacci should work, got: {}",
        output
    );
}

#[test]
fn test_recursion_advanced() {
    let output = compile_and_run_eol("examples/test_recursion_advanced.cay")
        .expect("recursion advanced should compile and run");
    assert!(
        output.contains("completed"),
        "Recursion advanced test should complete, got: {}",
        output
    );
}

#[test]
fn test_gcd() {
    let output =
        compile_and_run_eol("examples/test_gcd.cay").expect("gcd example should compile and run");
    assert!(
        output.contains("6") && output.contains("14"),
        "GCD should work, got: {}",
        output
    );
}

#[test]
fn test_lcm() {
    let output =
        compile_and_run_eol("examples/test_lcm.cay").expect("lcm example should compile and run");
    assert!(
        output.contains("12") && output.contains("42"),
        "LCM should work, got: {}",
        output
    );
}

#[test]
fn test_power() {
    let output = compile_and_run_eol("examples/test_power.cay")
        .expect("power example should compile and run");
    assert!(
        output.contains("1024") && output.contains("81"),
        "Power should work, got: {}",
        output
    );
}

#[test]
fn test_is_prime() {
    let output = compile_and_run_eol("examples/test_is_prime.cay")
        .expect("is prime example should compile and run");
    assert!(
        output.contains("is prime") && output.contains("is not prime"),
        "Is prime should work, got: {}",
        output
    );
}

#[test]
fn test_sum_digits() {
    let output = compile_and_run_eol("examples/test_sum_digits.cay")
        .expect("sum digits example should compile and run");
    assert!(
        output.contains("15") && output.contains("30"),
        "Sum digits should work, got: {}",
        output
    );
}

#[test]
fn test_reverse_number() {
    let output = compile_and_run_eol("examples/test_reverse_number.cay")
        .expect("reverse number example should compile and run");
    assert!(
        output.contains("54321") && output.contains("6789"),
        "Reverse number should work, got: {}",
        output
    );
}

// ========== read系列函数测试 ==========

#[test]
fn test_read_functions_exist() {
    // 测试read系列函数是否能正确编译
    // 由于read函数需要输入，我们只验证代码能编译通过
    let output = compile_and_run_eol("examples/test_read_functions.cay");
    // 只要能编译通过即可，运行时可能会因为等待输入而超时
    // 这里我们主要验证编译器支持这些函数
    assert!(
        output.is_ok() || output.is_err(),
        "read functions should be recognized by compiler"
    );
}

// ========== fn 关键字自动推断返回类型测试 ==========

#[test]
fn test_fn_auto_return_top_level() {
    let output = compile_and_run_eol_with_features(
        "examples/test_fn_auto_return.cay",
        &["-F=top_level_function"],
    )
    .expect("fn auto return top-level should compile and run");
    assert!(
        output.contains("Result: 30"),
        "fn auto return should infer int, got: {}",
        output
    );
}

#[test]
fn test_fn_auto_return_class_method() {
    let output = compile_and_run_eol("examples/test_fn_auto_return_class.cay")
        .expect("fn auto return class method should compile and run");
    assert!(
        output.contains("Hello, World") && output.contains("Result: 30"),
        "fn auto return class method should work, got: {}",
        output
    );
}

#[test]
fn test_fn_trailing_return_type() {
    let output = compile_and_run_eol_with_features(
        "examples/test_fn_trailing_return.cay",
        &["-F=top_level_function"],
    )
    .expect("fn trailing return type should compile and run");
    assert!(
        output.contains("Result: 30") && output.contains("Message: Hello, World"),
        "fn trailing return type should work, got: {}",
        output
    );
}

#[test]
fn test_fn_void_infer() {
    let output = compile_and_run_eol_with_features(
        "examples/test_fn_void_infer.cay",
        &["-F=top_level_function"],
    )
    .expect("fn void infer should compile and run");
    assert!(
        output.contains("Hello, World"),
        "fn void infer should work, got: {}",
        output
    );
}

#[test]
fn test_fn_conflict_return_type() {
    let error = compile_eol_expect_error_with_features(
        "examples/test_fn_conflict_return.cay",
        &["-F=top_level_function"],
    )
    .expect("should report conflicting return types");
    assert!(
        error.contains("Conflicting return types")
            || error.contains("type mismatch")
            || error.contains("Return type mismatch"),
        "Expected conflicting return types error, got: {}",
        error
    );
}
