//! Cavvy 语言预处理器集成测试
//!
//! 测试 #include、#pragma once 等预处理器功能

mod common;
use common::{compile_and_run_eol, compile_eol_expect_error};

// ==================== 0.3.5.0 预处理器 #include 测试 ====================

#[test]
fn test_include_basic() {
    let output = compile_and_run_eol("examples/test_include_basic.cay")
        .expect("include basic should compile and run");
    assert!(
        output.contains("Version test"),
        "Should show version test message, got: {}",
        output
    );
    assert!(
        output.contains("Addition test"),
        "Should show addition test message, got: {}",
        output
    );
    assert!(
        output.contains("Include test PASSED!"),
        "Include basic test should pass, got: {}",
        output
    );
}

#[test]
fn test_include_nested() {
    let output = compile_and_run_eol("examples/test_include_nested.cay")
        .expect("include nested should compile and run");
    assert!(
        output.contains("Nested include test PASSED!"),
        "Nested include test should pass, got: {}",
        output
    );
}

#[test]
fn test_include_pragma_once() {
    let output = compile_and_run_eol("examples/test_include_pragma_once.cay")
        .expect("include pragma once should compile and run");
    assert!(
        output.contains("Pragma once test PASSED!"),
        "Pragma once test should pass (multiple includes handled correctly), got: {}",
        output
    );
}

#[test]
fn test_error_include_cycle() {
    let error = compile_eol_expect_error("examples/errors/error_include_cycle.cay")
        .expect("cyclic include should fail to compile");
    assert!(
        error.contains("循环包含")
            || error.contains("cyclic")
            || error.contains("circular")
            || error.contains("include"),
        "Should report cyclic include error, got: {}",
        error
    );
}

// ==================== 0.4.8.3 系统包含路径 #include <> 测试 ====================

#[test]
fn test_include_system_angle_brackets() {
    let output = compile_and_run_eol("examples/test_include_system.cay")
        .expect("system include should compile and run");
    assert!(
        output.contains("hello"),
        "Should show 'hello' from split result, got: {}",
        output
    );
    assert!(
        output.contains("world"),
        "Should show 'world' from split result, got: {}",
        output
    );
    assert!(
        output.contains("cavvy"),
        "Should show 'cavvy' from split result, got: {}",
        output
    );
    assert!(
        output.contains("apple"),
        "Should show 'apple' from CSV split, got: {}",
        output
    );
    assert!(
        output.contains("banana"),
        "Should show 'banana' from CSV split, got: {}",
        output
    );
    assert!(
        output.contains("cherry"),
        "Should show 'cherry' from CSV split, got: {}",
        output
    );
    assert!(
        output.contains("Hello, Cavvy!"),
        "Should show formatted string, got: {}",
        output
    );
    assert!(
        output.contains("Greetings, World!"),
        "Should show indexed formatted string, got: {}",
        output
    );
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
    assert!(
        output.contains("Version 2 or higher"),
        "Should evaluate VERSION >= 2, got: {}",
        output
    );
    assert!(
        output.contains("Not both features enabled"),
        "Should evaluate FEATURE_A && FEATURE_B as false, got: {}",
        output
    );
    // defined(VERSION) 检查宏是否已定义
    // 注意：预处理器目前会展开字符串中的宏名（已知行为）
    assert!(
        output.contains("is defined"),
        "Should evaluate defined(VERSION), got: {}",
        output
    );
    assert!(
        output.contains("Version is exactly 2"),
        "Should evaluate VERSION == 2, got: {}",
        output
    );
    assert!(
        output.contains("Preprocessor conditional test passed"),
        "Should complete all tests, got: {}",
        output
    );
}

// ==================== #link 指令测试 ====================

#[test]
#[cfg(target_os = "windows")]
fn test_link_directive_basic() {
    let output = compile_and_run_eol("examples/test_link_directive.cay")
        .expect("link directive basic should compile and run");
    assert!(
        output.contains("#link directive test passed!"),
        "Should show link directive test passed, got: {}",
        output
    );
    assert!(
        output.contains("ws2_32"),
        "Should mention ws2_32 library, got: {}",
        output
    );
}

#[test]
#[cfg(target_os = "windows")]
fn test_link_directive_with_include() {
    let output = compile_and_run_eol("examples/test_link_directive_include.cay")
        .expect("link directive with include should compile and run");
    assert!(
        output.contains("#link with #include test passed!"),
        "Should show link with include test passed, got: {}",
        output
    );
    assert!(
        output.contains("gdi32") && output.contains("user32"),
        "Should mention both gdi32 and user32 libraries, got: {}",
        output
    );
}

#[test]
fn test_error_link_invalid_syntax() {
    let error = compile_eol_expect_error("examples/errors/error_link_invalid_syntax.cay")
        .expect("invalid link syntax should fail to compile");
    assert!(
        error.contains("无效的 #link 语法") || error.contains("#link"),
        "Should report invalid #link syntax error, got: {}",
        error
    );
}

#[test]
fn test_error_link_empty_name() {
    let error = compile_eol_expect_error("examples/errors/error_link_empty_name.cay")
        .expect("empty link name should fail to compile");
    assert!(
        error.contains("库名称不能为空") || error.contains("#link"),
        "Should report empty library name error, got: {}",
        error
    );
}

// ==================== #include_c 测试 ====================

#[test]
fn test_include_c_wrapper_mapping() {
    let output = compile_and_run_eol("examples/demo_include_c.cay")
        .expect("include_c wrapper-mapping demo should compile and run");
    assert!(
        output.contains("#include_c wrapper-mapping test passed!"),
        "Should show wrapper-mapping test passed, got: {}",
        output
    );
    assert!(
        output.contains("value=42"),
        "Should show printf-formatted value, got: {}",
        output
    );
}

#[test]
fn test_include_c_real_header_fallback() {
    let output = compile_and_run_eol("examples/demo_include_c_user.cay")
        .expect("include_c real-header fallback demo should compile and run");
    assert!(
        output.contains("abs(-7)=7"),
        "Should call abs() extracted from real header, got: {}",
        output
    );
    assert!(
        output.contains("rand() in range: true"),
        "Should call rand() extracted from real header, got: {}",
        output
    );
}

#[test]
fn test_error_include_c_missing() {
    let error = compile_eol_expect_error("examples/errors/error_include_c_missing.cay")
        .expect("missing header should fail to compile");
    assert!(
        error.contains("E1007") || error.contains("#include_c"),
        "Should report include_c missing header error, got: {}",
        error
    );
}
