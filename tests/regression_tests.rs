//! Regression tests for bugs found in the CavvyN project.
//!
//! Bug 1: #include path deduplication (E4002 with different include paths)
//! Bug 2: Array element field access (E5001: i8* vs i32)
//! Bug 3: Constructor name mangling with long parameter

mod common;

use common::assert_output_contains;
use common::compile_and_run_eol;

/// Bug 1: #include path deduplication
///
/// Before fix: including the same file via different relative paths
/// ("../bug1_helper.cay" vs "bug1_helper.cay") caused E4002 duplicate definition
/// because the preprocessor used raw path strings for dedup.
///
/// After fix: paths are canonicalized, so both resolve to the same file.
#[test]
fn test_bug1_include_path_dedup() {
    let output = compile_and_run_eol("examples/bug1_main.cay")
        .expect("Bug 1: compilation should succeed without duplicate definition error");
    // Bug1Helper.getValue() returns 42
    assert_output_contains(&output, &["42"], "test_bug1_include_path_dedup");
}

/// Bug 2: Array element field access
///
/// Before fix: tokens[0].intValue returned i8* (object pointer) instead of i64,
/// because MemberAccess on ArrayAccess was not handled in generate_member_access.
/// The expression tokens[0].intValue == literal caused E5001 (unsupported types).
///
/// After fix: ArrayAccess is handled, field access returns correct type.
#[test]
fn test_bug2_array_element_field_access() {
    let output = compile_and_run_eol("examples/bug2_array_field.cay")
        .expect("Bug 2: compilation should succeed");
    // Token(100).intValue prints 100
    assert_output_contains(&output, &["100"], "test_bug2_array_element_field_access");
}

/// Bug 3: Constructor name mangling with long parameter
///
/// Before fix: calling new Bug3Target(42) inferred the literal 42 as Int32 ("i"),
/// but the constructor parameter is long ("l"), causing name mangling mismatch
/// and linker error at IR→EXE stage.
///
/// After fix: constructor param types are looked up from type registry,
/// so the correct signature ("l" for long) is used.
#[test]
fn test_bug3_constructor_mangling() {
    let output = compile_and_run_eol("examples/bug3_ctor_mangling.cay")
        .expect("Bug 3: compilation and linking should succeed");
    // Bug3Target(42).getValue() returns 42
    assert_output_contains(&output, &["42"], "test_bug3_constructor_mangling");
}

/// Bug 4: Constructor overload resolution with same arg count, different types
///
/// Before fix: get_constructor_param_signatures used .find() to pick the first
/// constructor with matching parameter count. When multiple constructors had the
/// same arg count but different types (e.g., Token(int, long, SourceLocation) vs
/// Token(int, String, SourceLocation)), the wrong overload was selected.
///
/// After fix: a scoring system matches constructor parameter types against the
/// inferred argument types, preferring integer-family matches over String mismatches.
#[test]
fn test_bug4_constructor_overload_resolution() {
    let output = compile_and_run_eol("examples/bug_overload_resolution.cay")
        .expect("Bug 4: compilation and linking should succeed");
    // Test 1: int→long match
    assert_output_contains(&output, &["t1.intValue = 42"], "bug4_t1");
    // Test 2: long→long exact match
    assert_output_contains(&output, &["t2.intValue = 99"], "bug4_t2");
    // Test 3: String→String exact match
    assert_output_contains(&output, &["t3.strValue = hello"], "bug4_t3");
}

/// Bug 5: private static overloaded methods with value type parameters (ABI mismatch)
///
/// Before fix: when a class has multiple assertEquals overloads (double+double+String
/// and String+String+String), the compiler picked the wrong overload for param type
/// resolution. Arguments were incorrectly boxed as i8* pointers (bitcast→inttoptr),
/// causing the callee to receive garbage values in XMM registers.
///
/// After fix: get_method_param_types and get_method_return_type use resolve_best_method
/// which does signature-based matching to find the correct overload.
#[test]
fn test_bug5_overload_abi() {
    let output = compile_and_run_eol("examples/bug5_overload_abi.cay")
        .expect("Bug 5: compilation and linking should succeed");
    assert_output_contains(
        &output,
        &["OK: bug5_overload_abi"],
        "test_bug5_overload_abi",
    );
}

/// Bug 6: method chain get(0).asNumber() drops this pointer + wrong return type
///
/// Before fix: chain call obj.get(0).asNumber() generated IR where asNumber() was
/// called without the this pointer (result of get(0)), and the return type was
/// incorrectly inferred as i64 instead of double. This caused SIGSEGV at runtime.
///
/// After fix: resolve_best_method correctly matches overloaded methods by signature,
/// and infer_call_return_type uses find_method with proper arg types for chain calls.
#[test]
fn test_bug6_chain_call() {
    let output = compile_and_run_eol("examples/bug6_chain_call.cay")
        .expect("Bug 6: compilation and linking should succeed");
    assert_output_contains(&output, &["OK: bug6_chain_call"], "test_bug6_chain_call");
}
