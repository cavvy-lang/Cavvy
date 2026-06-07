//! Cavvy 语言文档代码示例集成测试
//!
//! 验证 docs/ 目录下所有文档中的代码示例都可以正确编译和运行

mod common;
use common::{compile_and_run_eol, compile_and_run_eol_with_features};

// ========== Quick Start 文档测试 ==========

#[test]
fn test_doc_quickstart_hello_class() {
    let output = compile_and_run_eol("test_docs/quickstart_hello_class.cay")
        .expect("quickstart_hello_class.cay should compile and run");
    assert!(
        output.contains("Hello, World!"),
        "Should output 'Hello, World!', got: {}",
        output
    );
}

#[test]
fn test_doc_quickstart_hello_toplevel() {
    let output = compile_and_run_eol("test_docs/quickstart_hello_toplevel.cay")
        .expect("quickstart_hello_toplevel.cay should compile and run");
    assert!(
        output.contains("Hello from top-level main!"),
        "Should output 'Hello from top-level main!', got: {}",
        output
    );
}

#[test]
fn test_doc_quickstart_hello_args() {
    let output = compile_and_run_eol("test_docs/quickstart_hello_args.cay")
        .expect("quickstart_hello_args.cay should compile and run");
    assert!(
        output.contains("Arguments count:"),
        "Should output arguments info, got: {}",
        output
    );
}

#[test]
fn test_doc_quickstart_variables() {
    let output = compile_and_run_eol("test_docs/quickstart_variables.cay")
        .expect("quickstart_variables.cay should compile and run");
    assert!(
        output.contains("x = 10"),
        "Should output 'x = 10', got: {}",
        output
    );
}

#[test]
fn test_doc_quickstart_control_flow() {
    let output = compile_and_run_eol("test_docs/quickstart_control_flow.cay")
        .expect("quickstart_control_flow.cay should compile and run");
    assert!(
        output.contains("B"),
        "Should output 'B' for score 85, got: {}",
        output
    );
    assert!(
        output.contains("Wednesday"),
        "Should output 'Wednesday' for day 3, got: {}",
        output
    );
}

#[test]
fn test_doc_quickstart_array() {
    let output = compile_and_run_eol("test_docs/quickstart_array.cay")
        .expect("quickstart_array.cay should compile and run");
    assert!(
        output.contains("Array length: 5"),
        "Should output array length, got: {}",
        output
    );
    assert!(
        output.contains("values[0] = 1"),
        "Should output array values, got: {}",
        output
    );
}

#[test]
fn test_doc_quickstart_class() {
    let output = compile_and_run_eol("test_docs/quickstart_class.cay")
        .expect("quickstart_class.cay should compile and run");
    assert!(
        output.contains("Hello, I'm Alice"),
        "Should output greeting, got: {}",
        output
    );
    assert!(
        output.contains("Name: Alice"),
        "Should output name, got: {}",
        output
    );
}

#[test]
fn test_doc_quickstart_inheritance() {
    let output = compile_and_run_eol("test_docs/quickstart_inheritance.cay")
        .expect("quickstart_inheritance.cay should compile and run");
    assert!(
        output.contains("Buddy barks"),
        "Should output 'Buddy barks', got: {}",
        output
    );
}

// ========== Language Guide 文档测试 ==========

#[test]
fn test_doc_language_type_cast() {
    let output = compile_and_run_eol("test_docs/language_type_cast.cay")
        .expect("language_type_cast.cay should compile and run");
    assert!(
        output.contains("Type casting test passed"),
        "Should pass type casting test, got: {}",
        output
    );
}

#[test]
fn test_doc_language_operators() {
    let output = compile_and_run_eol("test_docs/language_operators.cay")
        .expect("language_operators.cay should compile and run");
    assert!(
        output.contains("Operators test passed"),
        "Should pass operators test, got: {}",
        output
    );
    assert!(
        output.contains("a + b = 13"),
        "Should output addition result, got: {}",
        output
    );
}

#[test]
fn test_doc_language_control_flow() {
    let output = compile_and_run_eol("test_docs/language_control_flow.cay")
        .expect("language_control_flow.cay should compile and run");
    assert!(
        output.contains("Control flow test passed"),
        "Should pass control flow test, got: {}",
        output
    );
    assert!(
        output.contains("Wednesday"),
        "Should output Wednesday, got: {}",
        output
    );
}

#[test]
fn test_doc_language_functions() {
    let output = compile_and_run_eol("test_docs/language_functions.cay")
        .expect("language_functions.cay should compile and run");
    assert!(
        output.contains("add(1, 2) = 3"),
        "Should output add result, got: {}",
        output
    );
    assert!(
        output.contains("factorial(5) = 120"),
        "Should output factorial result, got: {}",
        output
    );
}

#[test]
fn test_doc_language_oop() {
    let output = compile_and_run_eol("test_docs/language_oop.cay")
        .expect("language_oop.cay should compile and run");
    assert!(
        output.contains("I'm Alice"),
        "Should output Alice's greeting, got: {}",
        output
    );
    assert!(
        output.contains("Total persons: 2"),
        "Should output person count, got: {}",
        output
    );
    assert!(
        output.contains("Circle area:"),
        "Should output circle area, got: {}",
        output
    );
}

#[test]
fn test_doc_language_arrays() {
    let output = compile_and_run_eol("test_docs/language_arrays.cay")
        .expect("language_arrays.cay should compile and run");
    assert!(
        output.contains("Arrays test passed"),
        "Should pass arrays test, got: {}",
        output
    );
    assert!(
        output.contains("values[0] = 1"),
        "Should output array values, got: {}",
        output
    );
}

// ========== FFI Guide 文档测试 ==========

#[test]
fn test_doc_ffi_basic() {
    let output = compile_and_run_eol("test_docs/ffi_basic.cay")
        .expect("ffi_basic.cay should compile and run");
    assert!(
        output.contains("sqrt(2.0) = 1.414"),
        "Should output sqrt result, got: {}",
        output
    );
    assert!(
        output.contains("Current time:"),
        "Should output current time, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_memory() {
    let output = compile_and_run_eol("test_docs/ffi_memory.cay")
        .expect("ffi_memory.cay should compile and run");
    assert!(
        output.contains("buffer[0] = 42"),
        "Should output buffer value, got: {}",
        output
    );
    assert!(
        output.contains("Memory test passed"),
        "Should pass memory test, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_stdio() {
    let output = compile_and_run_eol("test_docs/ffi_stdio.cay")
        .expect("ffi_stdio.cay should compile and run");
    assert!(
        output.contains("File content: Hello, World!"),
        "Should output file content, got: {}",
        output
    );
    assert!(
        output.contains("File I/O test passed"),
        "Should pass file I/O test, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_string() {
    let output = compile_and_run_eol("test_docs/ffi_string.cay")
        .expect("ffi_string.cay should compile and run");
    assert!(
        output.contains("strlen(\"Hello\") = 5"),
        "Should output strlen result, got: {}",
        output
    );
    assert!(
        output.contains("String test passed"),
        "Should pass string test, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_math() {
    let output =
        compile_and_run_eol("test_docs/ffi_math.cay").expect("ffi_math.cay should compile and run");
    assert!(
        output.contains("sqrt(2) = 1.414"),
        "Should output sqrt result, got: {}",
        output
    );
    assert!(
        output.contains("pow(2, 10) = 1024"),
        "Should output pow result, got: {}",
        output
    );
    assert!(
        output.contains("Math test passed"),
        "Should pass math test, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_time() {
    let output =
        compile_and_run_eol("test_docs/ffi_time.cay").expect("ffi_time.cay should compile and run");
    assert!(
        output.contains("Current time:"),
        "Should output current time, got: {}",
        output
    );
    assert!(
        output.contains("Time test passed"),
        "Should pass time test, got: {}",
        output
    );
}

#[test]
fn test_doc_multi_statement() {
    let output = compile_and_run_eol("test_docs/multi_statement.cay")
        .expect("multi_statement.cay should compile and run");
    assert!(
        output.contains("a = 10, b = 20, c = 30"),
        "Should output variable values, got: {}",
        output
    );
    assert!(
        output.contains("x = 10"),
        "Should output x value, got: {}",
        output
    );
    assert!(
        output.contains("Multi-statement test passed"),
        "Should pass multi-statement test, got: {}",
        output
    );
}

// ========== 新增文档测试 ==========

#[test]
fn test_doc_quickstart_snippets() {
    let output = compile_and_run_eol("test_docs/quickstart_snippets.cay")
        .expect("quickstart_snippets.cay should compile and run");
    assert!(
        output.contains("x = 10"),
        "Should output x value, got: {}",
        output
    );
    assert!(
        output.contains("B"),
        "Should output grade B, got: {}",
        output
    );
}

#[test]
fn test_doc_quickstart_class_test() {
    let output = compile_and_run_eol("test_docs/quickstart_class_test.cay")
        .expect("quickstart_class_test.cay should compile and run");
    assert!(
        output.contains("Hello, I'm Alice"),
        "Should output greeting, got: {}",
        output
    );
}

#[test]
fn test_doc_quickstart_inheritance_test() {
    let output = compile_and_run_eol("test_docs/quickstart_inheritance_test.cay")
        .expect("quickstart_inheritance_test.cay should compile and run");
    assert!(
        output.contains("Buddy barks"),
        "Should output 'Buddy barks', got: {}",
        output
    );
}

#[test]
fn test_doc_language_guide_snippets() {
    let output = compile_and_run_eol("test_docs/language_guide_snippets.cay")
        .expect("language_guide_snippets.cay should compile and run");
    assert!(
        output.contains("String value: 42"),
        "Should output string value, got: {}",
        output
    );
    assert!(
        output.contains("Parsed int: 123"),
        "Should output parsed int, got: {}",
        output
    );
}

#[test]
fn test_doc_language_guide_scope_test() {
    let output = compile_and_run_eol("test_docs/language_guide_scope_test.cay")
        .expect("language_guide_scope_test.cay should compile and run");
    // 输出顺序: 10 (classVar), 20 (methodVar), 30 (blockVar)
    // 注意：println 输出格式可能包含额外字符
    assert!(
        output.contains("10"),
        "Should output classVar (10), got: {}",
        output
    );
}

#[test]
fn test_doc_language_guide_program_structure() {
    let output = compile_and_run_eol("test_docs/language_guide_program_structure.cay")
        .expect("language_guide_program_structure.cay should compile and run");
    assert!(
        output.contains("42"),
        "Should output value 42, got: {}",
        output
    );
}

#[test]
fn test_doc_language_guide_functions() {
    let output = compile_and_run_eol("test_docs/language_guide_functions.cay")
        .expect("language_guide_functions.cay should compile and run");
    assert!(
        output.contains("Hello, Cavvy"),
        "Should output greeting, got: {}",
        output
    );
    assert!(
        output.contains("factorial(5) = 120"),
        "Should output factorial result, got: {}",
        output
    );
}

#[test]
fn test_doc_language_guide_oop() {
    let output = compile_and_run_eol("test_docs/language_guide_oop.cay")
        .expect("language_guide_oop.cay should compile and run");
    assert!(
        output.contains("Hello, I'm Alice"),
        "Should output greeting, got: {}",
        output
    );
    assert!(
        output.contains("Total persons: 2"),
        "Should output person count, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_basic_test() {
    let output = compile_and_run_eol("test_docs/ffi_basic_test.cay")
        .expect("ffi_basic_test.cay should compile and run");
    assert!(
        output.contains("Hello from Cavvy"),
        "Should output hello message, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_memory_test() {
    let output = compile_and_run_eol("test_docs/ffi_memory_test.cay")
        .expect("ffi_memory_test.cay should compile and run");
    assert!(
        output.contains("malloc"),
        "Should output malloc result, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_string_test() {
    let output = compile_and_run_eol("test_docs/ffi_string_test.cay")
        .expect("ffi_string_test.cay should compile and run");
    assert!(
        output.contains("Length:"),
        "Should output length, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_stdio_test() {
    let output = compile_and_run_eol("test_docs/ffi_stdio_test.cay")
        .expect("ffi_stdio_test.cay should compile and run");
    assert!(
        output.contains("Hello from printf"),
        "Should output printf message, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_math_test() {
    let output = compile_and_run_eol("test_docs/ffi_math_test.cay")
        .expect("ffi_math_test.cay should compile and run");
    assert!(
        output.contains("sqrt(16) = 4"),
        "Should output sqrt result, got: {}",
        output
    );
}

#[test]
fn test_doc_ffi_time_test() {
    let output = compile_and_run_eol("test_docs/ffi_time_test.cay")
        .expect("ffi_time_test.cay should compile and run");
    assert!(
        output.contains("Current time:"),
        "Should output current time, got: {}",
        output
    );
}

#[test]
fn test_doc_syntax_reference_snippets() {
    let output = compile_and_run_eol("test_docs/syntax_reference_snippets.cay")
        .expect("syntax_reference_snippets.cay should compile and run");
    assert!(
        output.contains("All syntax tests passed"),
        "Should pass all syntax tests, got: {}",
        output
    );
}
