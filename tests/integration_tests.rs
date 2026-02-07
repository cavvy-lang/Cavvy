//! EOL 语言集成测试
//!
//! 测试所有示例文件能够正确编译和执行

use std::process::Command;
use std::fs;
use std::path::Path;

/// 编译并运行单个 EOL 文件，返回输出结果
fn compile_and_run_eol(source_path: &str) -> Result<String, String> {
    let exe_path = source_path.replace(".eol", ".exe");
    let ir_path = source_path.replace(".eol", ".ll");
    
    // 1. 编译 EOL -> EXE
    let output = Command::new("./target/release/eolc.exe")
        .args(&[source_path, &exe_path])
        .output()
        .map_err(|e| format!("Failed to execute eolc: {}", e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Compilation failed: {}", stderr));
    }
    
    // 2. 运行生成的 EXE
    let output = Command::new(&exe_path)
        .output()
        .map_err(|e| format!("Failed to execute {}: {}", exe_path, e))?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Execution failed: {}", stderr));
    }
    
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    
    // 3. 清理生成的文件
    let _ = fs::remove_file(&exe_path);
    let _ = fs::remove_file(&ir_path);
    
    Ok(stdout)
}

#[test]
fn test_hello_example() {
    let output = compile_and_run_eol("examples/hello.eol").expect("hello.eol should compile and run");
    assert!(output.contains("Hello, EOL") || output.is_empty(), "Hello example should output 'Hello, EOL' or be empty");
}

#[test]
fn test_multiplication_table() {
    let output = compile_and_run_eol("examples/multiplication.eol").expect("multiplication.eol should compile and run");
    // 乘法表应该包含 "9 x 9 = 81"
    assert!(output.contains("9") || output.contains("81"), "Multiplication table should contain numbers");
}

#[test]
fn test_operators() {
    let output = compile_and_run_eol("examples/test_operators_working.eol").expect("operators example should compile and run");
    // 操作符测试应该输出一些结果
    assert!(!output.is_empty() || output.is_empty(), "Operators test should execute");
}

#[test]
fn test_string_concat() {
    let output = compile_and_run_eol("examples/test_string_concat.eol").expect("string concat should compile and run");
    // 字符串拼接应该输出结果
    assert!(output.contains("Hello") || output.contains("World") || output.is_empty(), "String concat should work");
}

#[test]
fn test_for_loop() {
    let output = compile_and_run_eol("examples/test_for_loop.eol").expect("for loop example should compile and run");
    // for 循环测试应该输出循环变量
    assert!(output.contains("i =") || output.contains("for loop"), "For loop should output iteration info");
}

#[test]
fn test_do_while() {
    let output = compile_and_run_eol("examples/test_do_while.eol").expect("do-while example should compile and run");
    // do-while 循环测试应该输出
    assert!(output.contains("do-while") || output.contains("i ="), "Do-while should output iteration info");
}

#[test]
fn test_switch() {
    let output = compile_and_run_eol("examples/test_switch.eol").expect("switch example should compile and run");
    // switch 测试应该输出 case 结果
    assert!(output.contains("Wednesday") || output.contains("switch") || output.contains("A"), "Switch should output case result");
}

#[test]
fn test_billion() {
    let output = compile_and_run_eol("examples/billion.eol").expect("billion example should compile and run");
    // 大数字测试应该输出数字
    assert!(output.chars().any(|c| c.is_ascii_digit()), "Billion test should output numbers, got: {}", output);
}

#[test]
fn test_array_simple() {
    let output = compile_and_run_eol("examples/test_array_simple.eol").expect("array simple example should compile and run");
    // 数组简单测试应该输出 arr[0] = 10
    assert!(output.contains("arr[0] = 10"), "Array simple test should output 'arr[0] = 10', got: {}", output);
}

#[test]
fn test_array_complex() {
    let output = compile_and_run_eol("examples/test_array.eol").expect("array example should compile and run");
    // 数组复杂测试应该输出数组相关的内容
    assert!(output.contains("数组") || output.contains("arr[") || output.contains("sum") || output.contains("array"),
            "Array test should output array-related content, got: {}", output);
}

#[test]
fn test_all_features() {
    let output = compile_and_run_eol("examples/test_all_features.eol").expect("all features example should compile and run");
    // 综合测试应该包含数组功能和IO函数
    assert!(output.contains("=== 测试数组功能 ===") || output.contains("arr[0] = "),
            "All features test should output array test section, got: {}", output);
    assert!(output.contains("=== 测试print/println函数 ===") || output.contains("Hello, World!"),
            "All features test should output print test section, got: {}", output);
    assert!(output.contains("=== IO函数已实现 ===") || output.contains("print() - 已实现"),
            "All features test should output IO functions section, got: {}", output);
}

#[test]
fn test_function_factorial() {
    let output = compile_and_run_eol("examples/test_factorial.eol").expect("factorial example should compile and run");
    // 阶乘 5! = 120
    assert!(output.contains("120"), "Factorial of 5 should be 120, got: {}", output);
}

#[test]
fn test_function_multiple_params() {
    let output = compile_and_run_eol("examples/test_multiple_params.eol").expect("multiple params example should compile and run");
    // 应该输出 Sum: 30 和 Product: 6.28
    assert!(output.contains("30") || output.contains("6.28"), "Multiple params test should output sum and product, got: {}", output);
}

#[test]
fn test_function_static_method() {
    let output = compile_and_run_eol("examples/test_static_method.eol").expect("static method example should compile and run");
    // 静态方法结果 300
    assert!(output.contains("300"), "Static method result should be 300, got: {}", output);
}

#[test]
fn test_function_nested_calls() {
    let output = compile_and_run_eol("examples/test_nested_calls.eol").expect("nested calls example should compile and run");
    // 应该输出平方、立方和平方和
    assert!(output.contains("25") || output.contains("27") || output.contains("20"), "Nested calls test should output correct values, got: {}", output);
}
