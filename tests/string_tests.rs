//! Cavvy 语言字符串功能集成测试
//!
//! 测试字符串操作、字符串方法等

mod common;
use common::compile_and_run_eol;

#[test]
fn test_string_methods() {
    let output = compile_and_run_eol("examples/test_string_methods.cay")
        .expect("string methods example should compile and run");
    // 测试字符串方法
    assert!(
        output.contains("Length: 13"),
        "String length should be 13, got: {}",
        output
    );
    assert!(
        output.contains("substring(7): World!"),
        "substring(7) should be 'World!', got: {}",
        output
    );
    assert!(
        output.contains("substring(0, 5): Hello"),
        "substring(0, 5) should be 'Hello', got: {}",
        output
    );
    assert!(
        output.contains("indexOf(World): 7"),
        "indexOf('World') should be 7, got: {}",
        output
    );
    // charAt 返回 ASCII 码值（H=72, W=87）
    assert!(
        output.contains("charAt(0): 72"),
        "charAt(0) should return ASCII 72, got: {}",
        output
    );
    assert!(
        output.contains("charAt(7): 87"),
        "charAt(7) should return ASCII 87, got: {}",
        output
    );
    assert!(
        output.contains("replace result: Hello, EOL!"),
        "replace result should be 'Hello, EOL!', got: {}",
        output
    );
    assert!(
        output.contains("All tests completed!"),
        "All string method tests should complete, got: {}",
        output
    );
}

#[test]
fn test_string_ops() {
    let output = compile_and_run_eol("examples/test_string_ops.cay")
        .expect("string ops example should compile and run");
    // 测试字符串操作
    assert!(
        output.contains("=== String Operations Tests ==="),
        "Should show string ops test header, got: {}",
        output
    );
    assert!(
        output.contains("Test 1: String concatenation"),
        "Should test string concatenation, got: {}",
        output
    );
    assert!(
        output.contains("Combined: Hello, World!"),
        "Combined string should be 'Hello, World!', got: {}",
        output
    );
    assert!(
        output.contains("Test 2: Empty string"),
        "Should test empty string, got: {}",
        output
    );
    assert!(
        output.contains("Test 4: String equality"),
        "Should test string equality, got: {}",
        output
    );
    assert!(
        output.contains("a == b: true"),
        "Same strings should be equal, got: {}",
        output
    );
    assert!(
        output.contains("a == c: false"),
        "Different strings should not be equal, got: {}",
        output
    );
    assert!(
        output.contains("Test 5: Substring operations"),
        "Should test substring operations, got: {}",
        output
    );
    assert!(
        output.contains("Test 6: String array"),
        "Should test string array, got: {}",
        output
    );
    assert!(
        output.contains("=== All string operations tests PASSED! ==="),
        "String operations tests should pass, got: {}",
        output
    );
}

#[test]
fn test_string_concat_advanced() {
    let output = compile_and_run_eol("examples/test_string_concat_advanced.cay")
        .expect("advanced string concat example should compile and run");
    // 强类型语言：只允许 string + string，不允许隐式转换
    assert!(
        output.contains("Test 1: Value: 42"),
        "String + string should work, got: {}",
        output
    );
    assert!(
        output.contains("All advanced string concat tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

// ========== 新增字符串方法测试 ==========

#[test]
fn test_string_length() {
    let output = compile_and_run_eol("examples/test_string_length.cay")
        .expect("string length example should compile and run");
    assert!(
        output.contains("s1 length = 5")
            && output.contains("s2 length = 13")
            && output.contains("s3 length = 0"),
        "String length should work, got: {}",
        output
    );
}

#[test]
fn test_string_substring() {
    let output = compile_and_run_eol("examples/test_string_substring.cay")
        .expect("string substring example should compile and run");
    assert!(
        output.contains("substring(7) = World!") && output.contains("substring(0, 5) = Hello"),
        "String substring should work, got: {}",
        output
    );
}

#[test]
fn test_string_indexof() {
    let output = compile_and_run_eol("examples/test_string_indexof.cay")
        .expect("string indexof example should compile and run");
    assert!(
        output.contains("indexOf('World') = 7") && output.contains("indexOf('Java') = -1"),
        "String indexOf should work, got: {}",
        output
    );
}

#[test]
fn test_string_replace() {
    let output = compile_and_run_eol("examples/test_string_replace.cay")
        .expect("string replace example should compile and run");
    assert!(
        output.contains("Replaced: Hello, EOL! EOL is great!"),
        "String replace should work, got: {}",
        output
    );
}

#[test]
fn test_string_charat() {
    let output = compile_and_run_eol("examples/test_string_charat.cay")
        .expect("string charat example should compile and run");
    assert!(
        output.contains("charAt(0) = 65") && output.contains("charAt(2) = 67"),
        "String charAt should work, got: {}",
        output
    );
}

#[test]
fn test_string_complex_ops() {
    let output = compile_and_run_eol("examples/test_string_complex_ops.cay")
        .expect("string complex ops should compile and run");
    assert!(
        output.contains("completed"),
        "String complex ops test should complete, got: {}",
        output
    );
}

#[test]
fn test_string_palindrome() {
    let output = compile_and_run_eol("examples/test_string_palindrome.cay")
        .expect("string palindrome should compile and run");
    assert!(
        output.contains("PASSED"),
        "String palindrome test should pass, got: {}",
        output
    );
}
