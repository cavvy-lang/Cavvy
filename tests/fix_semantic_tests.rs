//! 语义分析修复专项测试
//!
//! 覆盖区域C（semantic + types）修复的三类问题：
//! 1. null 类型大洞：Object 实例赋给 String/无关类必须报错；null 仍可赋给任何引用类型
//! 2. 比较运算符操作数检查：String 与 int 关系比较必须报错
//! 3. 数组初始化：所有元素类型必须一致

mod common;
use common::{assert_output_contains, compile_and_run_eol, compile_eol_expect_error};

/// Object 实例赋给 String 必须报语义错误
#[test]
fn test_object_instance_not_assignable_to_string() {
    let error = compile_eol_expect_error("examples/errors/fix_semantic_object_to_string.cay")
        .expect("Object instance assigned to String should fail to compile");
    assert!(
        error.contains("Cannot assign") || error.contains("type") || error.contains("Type"),
        "Should report type error for Object -> String assignment, got: {}",
        error
    );
}

/// String 与 int 的关系比较必须报语义错误
#[test]
fn test_string_relational_compare_with_int_errors() {
    let error = compile_eol_expect_error("examples/errors/fix_semantic_string_lt_int.cay")
        .expect("\"abc\" < 42 should fail to compile");
    assert!(
        error.contains("Cannot compare") || error.contains("operator") || error.contains("type"),
        "Should report comparison type error, got: {}",
        error
    );
}

/// 数组初始化元素类型不一致必须报语义错误
#[test]
fn test_array_init_mixed_element_types_error() {
    let error = compile_eol_expect_error("examples/errors/fix_semantic_array_mixed_init.cay")
        .expect("{1, \"hello\"} should fail to compile");
    assert!(
        error.contains("consistent") || error.contains("type") || error.contains("Type"),
        "Should report array element type mismatch, got: {}",
        error
    );
}

/// 泛型类默认类型参数必须参与构造函数实参类型检查
#[test]
fn test_arraylist_default_type_arg_mismatch_error() {
    let error = compile_eol_expect_error("examples/errors/fix_semantic_arraylist_default_type_arg.cay")
        .expect("ArrayList<ArrayList<int>>(int, ArrayList<int>) should fail to compile");
    assert!(
        error.contains("GlobalAlloc") && error.contains("std::ArrayList<int>"),
        "Should report default type parameter mismatch, got: {}",
        error
    );
}

/// 正向：null 可赋给任何引用类型、可与引用类型比较；合法比较与数组初始化不受影响
#[test]
fn test_null_assignment_and_comparisons_ok() {
    let output = compile_and_run_eol("examples/fix_semantic_null_ok.cay")
        .expect("null/comparison legal program should compile and run");
    assert_output_contains(
        &output,
        &[
            "s is null",
            "obj is null",
            "arr is null",
            "objs[0] is null",
            "objs2 ok",
            "6",
            "comparisons ok",
            "bool eq ok",
        ],
        "test_null_assignment_and_comparisons_ok",
    );
}
