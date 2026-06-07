//! Cavvy 语言错误测试
//!
//! 测试编译时错误和运行时错误

mod common;
use common::{compile_and_run_expect_error, compile_eol_expect_error};

// ==================== 错误测试 ====================

#[test]
fn test_error_string_plus_int() {
    let error = compile_eol_expect_error("examples/errors/error_string_plus_int.cay")
        .expect("string + int should fail to compile");
    assert!(
        error.contains("Cannot add") || error.contains("string") || error.contains("type"),
        "Should report type error for string + int, got: {}",
        error
    );
}

#[test]
fn test_error_string_plus_float() {
    let error = compile_eol_expect_error("examples/errors/error_string_plus_float.cay")
        .expect("string + float should fail to compile");
    assert!(
        error.contains("Cannot add") || error.contains("string") || error.contains("type"),
        "Should report type error for string + float, got: {}",
        error
    );
}

#[test]
fn test_error_type_mismatch_assign() {
    let error = compile_eol_expect_error("examples/errors/error_type_mismatch_assign.cay")
        .expect("type mismatch assignment should fail to compile");
    assert!(
        error.contains("type mismatch")
            || error.contains("Type")
            || error.contains("expected")
            || error.contains("Cannot assign")
            || error.contains("类型"),
        "Should report type mismatch error, got: {}",
        error
    );
}

#[test]
fn test_error_undefined_variable() {
    let error = compile_eol_expect_error("examples/errors/error_undefined_variable.cay")
        .expect("undefined variable should fail to compile");
    assert!(
        error.contains("未定义") && error.contains("'y'"),
        "Should report undefined variable 'y' error, got: {}",
        error
    );
}

#[test]
fn test_error_redefined_variable() {
    let error = compile_eol_expect_error("examples/errors/error_redefined_variable.cay")
        .expect("redefined variable should fail to compile");
    assert!(
        error.contains("already defined")
            || error.contains("redefined")
            || error.contains("Duplicate"),
        "Should report redefined variable error, got: {}",
        error
    );
}

#[test]
fn test_error_break_outside_loop() {
    let error = compile_eol_expect_error("examples/errors/error_break_outside_loop.cay")
        .expect("break outside loop should fail to compile");
    assert!(
        error.contains("break") || error.contains("loop") || error.contains("outside"),
        "Should report break outside loop error, got: {}",
        error
    );
}

#[test]
fn test_error_continue_outside_loop() {
    let error = compile_eol_expect_error("examples/errors/error_continue_outside_loop.cay")
        .expect("continue outside loop should fail to compile");
    assert!(
        error.contains("continue") || error.contains("loop") || error.contains("outside"),
        "Should report continue outside loop error, got: {}",
        error
    );
}

#[test]
fn test_error_invalid_cast() {
    let error = compile_eol_expect_error("examples/errors/error_invalid_cast.cay")
        .expect("invalid cast should fail to compile");
    assert!(
        error.contains("cast") || error.contains("Cast") || error.contains("unsupported"),
        "Should report invalid cast error, got: {}",
        error
    );
}

#[test]
fn test_error_array_index_type() {
    let error = compile_eol_expect_error("examples/errors/error_array_index_type.cay")
        .expect("array index with string should fail to compile");
    assert!(
        error.contains("index") || error.contains("integer") || error.contains("type"),
        "Should report array index type error, got: {}",
        error
    );
}

#[test]
fn test_error_missing_main() {
    let error = compile_eol_expect_error("examples/errors/error_missing_main.cay")
        .expect("missing main should fail to compile");
    assert!(
        error.contains("main")
            || error.contains("entry point")
            || error.contains("not found")
            || error.contains("WinMain")
            || error.contains("undefined symbol"),
        "Should report missing main error, got: {}",
        error
    );
}

#[test]
fn test_error_invalid_cast_string_to_int() {
    let error = compile_eol_expect_error("examples/errors/error_invalid_cast_string_to_int.cay")
        .expect("string to int cast should fail to compile");
    assert!(
        error.contains("cast")
            || error.contains("Cast")
            || error.contains("unsupported")
            || error.contains("Unsupported"),
        "Should report invalid cast error for string to int, got: {}",
        error
    );
}

#[test]
fn test_error_invalid_cast_array_to_int() {
    let error = compile_eol_expect_error("examples/errors/error_invalid_cast_array_to_int.cay")
        .expect("array to int cast should fail to compile");
    assert!(
        error.contains("cast")
            || error.contains("Cast")
            || error.contains("unsupported")
            || error.contains("Unsupported"),
        "Should report invalid cast error for array to int, got: {}",
        error
    );
}

// ==================== 新增错误测试 ====================

#[test]
fn test_error_duplicate_class() {
    let error = compile_eol_expect_error("examples/errors/error_duplicate_class.cay")
        .expect("duplicate class should fail to compile");
    assert!(
        error.contains("重复定义") && error.contains("TestDuplicateClass"),
        "Should report duplicate class error with class name, got: {}",
        error
    );
}

#[test]
fn test_error_final_reassignment() {
    let error = compile_eol_expect_error("examples/errors/error_final_reassignment.cay")
        .expect("final reassignment should fail to compile");
    assert!(
        error.contains("final") || error.contains("reassign") || error.contains("cannot assign"),
        "Should report final reassignment error, got: {}",
        error
    );
}

#[test]
fn test_error_void_assignment() {
    let error = compile_eol_expect_error("examples/errors/error_void_assignment.cay")
        .expect("void assignment should fail to compile");
    assert!(
        error.contains("void") || error.contains("type") || error.contains("mismatch"),
        "Should report void assignment error, got: {}",
        error
    );
}

#[test]
fn test_error_array_negative_size() {
    let error = compile_eol_expect_error("examples/errors/error_array_negative_size.cay")
        .expect("array negative size should fail to compile");
    assert!(
        error.contains("array") || error.contains("size") || error.contains("negative"),
        "Should report array negative size error, got: {}",
        error
    );
}

#[test]
fn test_error_division_by_zero() {
    let error = compile_and_run_expect_error("examples/errors/error_division_by_zero.cay")
        .expect("division by zero should fail to compile or run");
    assert!(
        error.contains("zero") || error.contains("divide") || error.contains("runtime"),
        "Should report division by zero error, got: {}",
        error
    );
}

#[test]
fn test_error_modulo_by_zero() {
    let error = compile_and_run_expect_error("examples/errors/error_modulo_by_zero.cay")
        .expect("modulo by zero should fail to compile or run");
    assert!(
        error.contains("zero") || error.contains("modulo") || error.contains("remainder"),
        "Should report modulo by zero error, got: {}",
        error
    );
}

#[test]
fn test_error_undefined_method() {
    let error = compile_eol_expect_error("examples/errors/error_undefined_method.cay")
        .expect("undefined method should fail to compile");
    assert!(
        error.contains("undefined") || error.contains("not found") || error.contains("method"),
        "Should report undefined method error, got: {}",
        error
    );
}

#[test]
fn test_error_missing_return() {
    let error = compile_eol_expect_error("examples/errors/error_missing_return.cay")
        .expect("missing return should fail to compile");
    assert!(
        error.contains("return") || error.contains("missing") || error.contains("expected"),
        "Should report missing return error, got: {}",
        error
    );
}

#[test]
fn test_error_return_type_mismatch() {
    let error = compile_eol_expect_error("examples/errors/error_return_type_mismatch.cay")
        .expect("return type mismatch should fail to compile");
    assert!(
        error.contains("return") || error.contains("type") || error.contains("mismatch"),
        "Should report return type mismatch error, got: {}",
        error
    );
}

#[test]
fn test_error_string_index() {
    let error = compile_eol_expect_error("examples/errors/error_string_index.cay")
        .expect("string index access should fail to compile");
    assert!(
        error.contains("string") || error.contains("index") || error.contains("[]"),
        "Should report string index error, got: {}",
        error
    );
}

#[test]
fn test_error_invalid_operator() {
    let error = compile_eol_expect_error("examples/errors/error_invalid_operator.cay")
        .expect("invalid operator should fail to compile");
    assert!(
        error.contains("operator") || error.contains("syntax") || error.contains("unexpected"),
        "Should report invalid operator error, got: {}",
        error
    );
}

#[test]
fn test_error_method_call_wrong_args() {
    let error = compile_eol_expect_error("examples/errors/error_method_call_wrong_args.cay")
        .expect("method call with wrong args should fail to compile");
    assert!(
        error.contains("argument") || error.contains("parameter") || error.contains("mismatch"),
        "Should report method argument error, got: {}",
        error
    );
}

#[test]
fn test_error_method_call_few_args() {
    let error = compile_eol_expect_error("examples/errors/error_method_call_few_args.cay")
        .expect("method call with too few args should fail to compile");
    assert!(
        error.contains("argument") || error.contains("parameter") || error.contains("few"),
        "Should report too few arguments error, got: {}",
        error
    );
}

#[test]
fn test_error_multiple_main() {
    let error = compile_eol_expect_error("examples/errors/error_multiple_main.cay")
        .expect("multiple main should fail to compile");
    assert!(
        error.contains("main") || error.contains("multiple") || error.contains("duplicate"),
        "Should report multiple main error, got: {}",
        error
    );
}

#[test]
fn test_error_incompatible_types() {
    let error = compile_eol_expect_error("examples/errors/error_incompatible_types.cay")
        .expect("incompatible types should fail to compile");
    assert!(
        error.contains("type")
            || error.contains("incompatible")
            || error.contains("mismatch")
            || error.contains("Cannot assign"),
        "Should report incompatible types error, got: {}",
        error
    );
}

#[test]
fn test_error_abstract_class() {
    let error = compile_eol_expect_error("examples/errors/error_abstract_class.cay")
        .expect("abstract class instantiation should fail to compile");
    assert!(
        error.contains("abstract") || error.contains("instantiate") || error.contains("class"),
        "Should report abstract class error, got: {}",
        error
    );
}

#[test]
fn test_error_field_private() {
    let error = compile_eol_expect_error("examples/errors/error_field_private.cay")
        .expect("access to private field should fail to compile");
    assert!(
        error.contains("private") || error.contains("access") || error.contains("field"),
        "Should report private field access error, got: {}",
        error
    );
}

#[test]
fn test_error_array_store() {
    let error = compile_eol_expect_error("examples/errors/error_array_store.cay")
        .expect("array type store error should fail to compile");
    assert!(
        error.contains("array") || error.contains("type") || error.contains("store"),
        "Should report array store error, got: {}",
        error
    );
}
