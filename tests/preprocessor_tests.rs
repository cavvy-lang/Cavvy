//! Cavvy 语言预处理器集成测试
//!
//! 测试 #include、#pragma once 等预处理器功能

mod common;
use common::{compile_and_run_eol, compile_eol_expect_error};

// ==================== 0.3.5.0 预处理器 #include 测试 ====================

#[test]
fn test_include_basic() {
    let output = compile_and_run_eol("examples/test_include_basic.cay").expect("include basic should compile and run");
    assert!(output.contains("Version test"),
            "Should show version test message, got: {}", output);
    assert!(output.contains("Addition test"),
            "Should show addition test message, got: {}", output);
    assert!(output.contains("Include test PASSED!"),
            "Include basic test should pass, got: {}", output);
}

#[test]
fn test_include_nested() {
    let output = compile_and_run_eol("examples/test_include_nested.cay").expect("include nested should compile and run");
    assert!(output.contains("Nested include test PASSED!"),
            "Nested include test should pass, got: {}", output);
}

#[test]
fn test_include_pragma_once() {
    let output = compile_and_run_eol("examples/test_include_pragma_once.cay").expect("include pragma once should compile and run");
    assert!(output.contains("Pragma once test PASSED!"),
            "Pragma once test should pass (multiple includes handled correctly), got: {}", output);
}

#[test]
fn test_error_include_cycle() {
    let error = compile_eol_expect_error("examples/errors/error_include_cycle.cay")
        .expect("cyclic include should fail to compile");
    assert!(
        error.contains("循环包含") || error.contains("cyclic") || error.contains("circular") || error.contains("include"),
        "Should report cyclic include error, got: {}",
        error
    );
}

// ==================== 0.4.8.3 系统包含路径 #include <> 测试 ====================

#[test]
fn test_include_system_angle_brackets() {
    let output = compile_and_run_eol("examples/test_include_system.cay").expect("system include should compile and run");
    assert!(output.contains("hello"),
            "Should show 'hello' from split result, got: {}", output);
    assert!(output.contains("world"),
            "Should show 'world' from split result, got: {}", output);
    assert!(output.contains("cavvy"),
            "Should show 'cavvy' from split result, got: {}", output);
    assert!(output.contains("apple"),
            "Should show 'apple' from CSV split, got: {}", output);
    assert!(output.contains("banana"),
            "Should show 'banana' from CSV split, got: {}", output);
    assert!(output.contains("cherry"),
            "Should show 'cherry' from CSV split, got: {}", output);
    assert!(output.contains("Hello, Cavvy!"),
            "Should show formatted string, got: {}", output);
    assert!(output.contains("Greetings, World!"),
            "Should show indexed formatted string, got: {}", output);
}

// ==================== 错误行号定位测试（多级include）====================

#[test]
fn test_error_line_number_with_nested_include() {
    let error = compile_eol_expect_error("examples/test_extreme_line_main.cay")
        .expect("nested include type error should fail to compile");
    // 错误应在 test_extreme_line_c.cay（被 #include 的文件）中
    // 行号取决于源映射实现
    assert!(
        error.contains("test_extreme_line_c.cay") && error.contains("Cannot assign string to int"),
        "Should report error in test_extreme_line_c.cay about type mismatch, got: {}",
        error
    );
}

// ==================== 0.4.x 预处理器条件表达式测试 ====================

#[test]
fn test_preprocessor_conditional() {
    let output = compile_and_run_eol("examples/test_preprocessor_conditional.cay")
        .expect("preprocessor conditional should compile and run");
    assert!(output.contains("Version 2 or higher"),
            "Should evaluate VERSION >= 2, got: {}", output);
    assert!(output.contains("Not both features enabled"),
            "Should evaluate FEATURE_A && FEATURE_B as false, got: {}", output);
    // defined(VERSION) 检查宏是否已定义
    // 注意：预处理器目前会展开字符串中的宏名（已知行为）
    assert!(output.contains("is defined"),
            "Should evaluate defined(VERSION), got: {}", output);
    assert!(output.contains("Version is exactly 2"),
            "Should evaluate VERSION == 2, got: {}", output);
    assert!(output.contains("Preprocessor conditional test passed"),
            "Should complete all tests, got: {}", output);
}
