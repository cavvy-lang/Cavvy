//! 泛型 ADT 集成测试
//!
//! 验证 struct 与 enum 的泛型支持、完整单态化以及值类型语义。

mod common;
use common::compile_and_run_eol;

#[test]
fn test_generic_struct_integration() {
    let output = compile_and_run_eol("examples/test_generic_struct.cay")
        .expect("generic struct integration test should compile and run");

    assert!(
        output.contains("Point: 10, 20"),
        "Generic struct field access failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Generic struct test PASS"),
        "Generic struct test did not report PASS. Full output:\n{}",
        output
    );
}

#[test]
fn test_generic_enum_integration() {
    let output = compile_and_run_eol("examples/test_generic_enum.cay")
        .expect("generic enum integration test should compile and run");

    assert!(
        output.contains("=== Generic enum test ==="),
        "Missing test header. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Some int: 42"),
        "Option<int>.Some payload extraction failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Option<int>.Some PASS"),
        "Option<int>.Some assertion failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Option<int>.None PASS"),
        "Option<int>.None assertion failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Ok int: 7"),
        "Result<int,String>.Ok payload extraction failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Result<int,String>.Ok PASS"),
        "Result Ok assertion failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Err string: fail"),
        "Result<int,String>.Err payload extraction failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Result<int,String>.Err PASS"),
        "Result Err assertion failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("=== Generic enum test end ==="),
        "Generic enum test did not complete. Full output:\n{}",
        output
    );
}

#[test]
fn test_generic_adt_comprehensive() {
    let output = compile_and_run_eol("examples/test_generic_adt_comprehensive.cay")
        .expect("generic ADT comprehensive test should compile and run");

    assert!(
        output.contains("Pair: 1, one"),
        "Generic struct Pair<int,String> failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Boxed first: 1"),
        "Nested Boxed<Pair<int,String>> payload extraction failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Nested generic ADT PASS"),
        "Nested generic ADT assertion failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Boxed<int>.Empty matched"),
        "Generic enum empty variant failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Generic ADT comprehensive PASS"),
        "Generic ADT comprehensive test did not report PASS. Full output:\n{}",
        output
    );
}

#[test]
fn test_generic_adt_stress() {
    let output = compile_and_run_eol("examples/test_generic_adt_stress.cay")
        .expect("generic ADT stress test should compile and run");

    assert!(
        output.contains("Pair<int,String> OK"),
        "Pair<int,String> instantiation failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Pair<int,int> OK"),
        "Pair<int,int> instantiation failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Wrapper<Pair<int,String>> OK"),
        "Nested Wrapper<Pair<int,String>> failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Deep nested Boxed<Wrapper<Pair<int,String>>> OK"),
        "Deep nested generic ADT failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Generic ADT array OK"),
        "Generic ADT array failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Boxed<int>.Empty OK"),
        "Generic enum empty variant failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Value type copy independence OK"),
        "Value type generic struct copy independence failed. Full output:\n{}",
        output
    );
    assert!(
        output.contains("Generic ADT stress PASS"),
        "Generic ADT stress test did not report PASS. Full output:\n{}",
        output
    );
}

/// struct 与 enum 是值类型：赋值与传参均为拷贝，修改副本不影响原值。
#[test]
fn test_adt_value_semantics() {
    let output = compile_and_run_eol("examples/test_adt_value_semantics.cay")
        .expect("ADT value semantics test should compile and run");

    assert!(
        output.contains("struct assignment copy OK"),
        "struct assignment must copy, not alias. Full output:\n{}",
        output
    );
    assert!(
        output.contains("struct pass-by-value OK"),
        "struct parameters must be passed by value. Full output:\n{}",
        output
    );
    assert!(
        output.contains("enum assignment copy OK"),
        "enum assignment must copy, not alias. Full output:\n{}",
        output
    );
    assert!(
        output.contains("struct return-by-value OK"),
        "struct return values must be independent copies. Full output:\n{}",
        output
    );
    assert!(
        output.contains("value semantics probe DONE"),
        "Value semantics probe did not complete. Full output:\n{}",
        output
    );
}
