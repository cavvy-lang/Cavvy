//! Cavvy 语言控制流集成测试
//!
//! 测试 if、switch、for、while、do-while、break、continue 等控制流

mod common;
use common::compile_and_run_eol;

#[test]
fn test_for_loop() {
    let output = compile_and_run_eol("examples/test_for_loop.cay")
        .expect("for loop example should compile and run");
    // for 循环测试应该输出循环变量
    assert!(
        output.contains("i =") || output.contains("for loop"),
        "For loop should output iteration info"
    );
}

#[test]
fn test_do_while() {
    let output = compile_and_run_eol("examples/test_do_while.cay")
        .expect("do-while example should compile and run");
    // do-while 循环测试应该输出
    assert!(
        output.contains("do-while") || output.contains("i ="),
        "Do-while should output iteration info"
    );
}

#[test]
fn test_switch() {
    let output = compile_and_run_eol("examples/test_switch.cay")
        .expect("switch example should compile and run");
    // switch 测试应该输出 case 结果
    assert!(
        output.contains("Wednesday") || output.contains("switch") || output.contains("A"),
        "Switch should output case result"
    );
}

#[test]
fn test_break_continue() {
    let output = compile_and_run_eol("examples/test_break_continue.cay")
        .expect("break/continue example should compile and run");
    assert!(
        output.contains("stopped at 5") || output.contains("Break in for loop"),
        "Break should work, got: {}",
        output
    );
    assert!(
        output.contains("All break and continue tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

#[test]
fn test_break_in_loop() {
    let output = compile_and_run_eol("examples/test_break_in_loop.cay")
        .expect("break in loop example should compile and run");
    assert!(
        output.contains("Breaking at i = 5"),
        "Break in loop should work, got: {}",
        output
    );
}

#[test]
fn test_continue_in_loop() {
    let output = compile_and_run_eol("examples/test_continue_in_loop.cay")
        .expect("continue in loop example should compile and run");
    assert!(
        output.contains("Skipping i = 2"),
        "Continue in loop should work, got: {}",
        output
    );
}

// ========== 新增控制流测试 ==========

#[test]
fn test_if_basic() {
    let output = compile_and_run_eol("examples/test_if_basic.cay")
        .expect("if basic example should compile and run");
    assert!(
        output.contains("greater") || output.contains("not less"),
        "Basic if should work, got: {}",
        output
    );
}

#[test]
fn test_if_else_if() {
    let output = compile_and_run_eol("examples/test_if_else_if.cay")
        .expect("if-else-if example should compile and run");
    assert!(
        output.contains("Grade B"),
        "If-else-if should work, got: {}",
        output
    );
}

#[test]
fn test_nested_if() {
    let output = compile_and_run_eol("examples/test_nested_if.cay")
        .expect("nested if example should compile and run");
    assert!(
        output.contains("a > 5 and b > 15"),
        "Nested if should work, got: {}",
        output
    );
}

#[test]
fn test_while_basic() {
    let output = compile_and_run_eol("examples/test_while_basic.cay")
        .expect("while basic example should compile and run");
    assert!(
        output.contains("i = 0") || output.contains("i = 4"),
        "Basic while should work, got: {}",
        output
    );
}

#[test]
fn test_while_nested() {
    let output = compile_and_run_eol("examples/test_while_nested.cay")
        .expect("while nested example should compile and run");
    assert!(
        output.contains("i=1") || output.contains("j=1"),
        "Nested while should work, got: {}",
        output
    );
}

#[test]
fn test_for_basic() {
    let output = compile_and_run_eol("examples/test_for_basic.cay")
        .expect("for basic example should compile and run");
    assert!(
        output.contains("i = 0") || output.contains("i = 4"),
        "Basic for should work, got: {}",
        output
    );
}

#[test]
fn test_for_nested() {
    let output = compile_and_run_eol("examples/test_for_nested.cay")
        .expect("for nested example should compile and run");
    assert!(
        output.contains("i=1") || output.contains("j=1"),
        "Nested for should work, got: {}",
        output
    );
}

#[test]
fn test_do_while_basic() {
    let output = compile_and_run_eol("examples/test_do_while_basic.cay")
        .expect("do-while basic example should compile and run");
    assert!(
        output.contains("i = 0") || output.contains("i = 4"),
        "Basic do-while should work, got: {}",
        output
    );
}

#[test]
fn test_switch_basic() {
    let output = compile_and_run_eol("examples/test_switch_basic.cay")
        .expect("switch basic example should compile and run");
    assert!(
        output.contains("Wednesday"),
        "Basic switch should work, got: {}",
        output
    );
}

#[test]
fn test_switch_fallthrough() {
    let output = compile_and_run_eol("examples/test_switch_fallthrough.cay")
        .expect("switch fallthrough example should compile and run");
    assert!(
        output.contains("Good") || output.contains("Excellent"),
        "Switch fallthrough should work, got: {}",
        output
    );
}

#[test]
fn test_switch_advanced() {
    let output = compile_and_run_eol("examples/test_switch_advanced.cay")
        .expect("advanced switch example should compile and run");
    assert!(
        output.contains("Day of week"),
        "Switch should work, got: {}",
        output
    );
    assert!(
        output.contains("All advanced switch tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

#[test]
fn test_control_flow_complex() {
    let output = compile_and_run_eol("examples/test_control_flow_complex.cay")
        .expect("control flow complex should compile and run");
    assert!(
        output.contains("completed"),
        "Control flow complex test should complete, got: {}",
        output
    );
}

#[test]
fn test_for_empty_init() {
    let output = compile_and_run_eol("examples/test_for_empty_init.cay")
        .expect("for empty init example should compile and run");
    assert!(
        output.contains("All for-loop empty init tests passed!"),
        "For loop with empty init should work, got: {}",
        output
    );
}

#[test]
fn test_switch_char() {
    let output = compile_and_run_eol("examples/test_switch_char.cay")
        .expect("switch char example should compile and run");
    assert!(
        output.contains("Test1: plus"),
        "Switch char case '+' should work, got: {}",
        output
    );
    assert!(
        output.contains("Test2 code: 65"),
        "Switch char case 'A' should work, got: {}",
        output
    );
    assert!(
        output.contains("Test3: newline"),
        "Switch char case '\\n' should work, got: {}",
        output
    );
    assert!(
        output.contains("Test4 fallthrough: XYZ"),
        "Switch char fallthrough should work, got: {}",
        output
    );
    assert!(
        output.contains("All switch char tests passed!"),
        "Switch char test should complete, got: {}",
        output
    );
}

#[test]
fn test_for_init_reuse_regression() {
    let output = compile_and_run_eol("examples/test_for_init_reuse_regression.cay")
        .expect("for init reuse regression should compile and run");
    assert!(
        output.contains("count=3"),
        "For loop should execute 3 times, got: {}",
        output
    );
    assert!(
        output.contains("final_totalTime=15"),
        "totalTime += should accumulate to 15 (3*5), got: {}",
        output
    );
}
