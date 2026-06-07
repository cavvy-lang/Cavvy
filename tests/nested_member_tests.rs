//! Cavvy 语言链式成员访问赋值集成测试
//!
//! 测试 obj.field1.field2 = value 形式的赋值

mod common;
use common::compile_and_run_eol;

// ========== 链式成员访问赋值测试 ==========

#[test]
fn test_nested_member_assignment() {
    let output = compile_and_run_eol("examples/test_nested_member_assignment.cay")
        .expect("nested member assignment example should compile and run");

    // 测试单层成员访问赋值
    assert!(
        output.contains("outer.inner.value = 200"),
        "Should show outer.inner.value = 200, got: {}",
        output
    );

    // 测试链式成员访问赋值字符串字段
    assert!(
        output.contains("outer.inner.name = updated"),
        "Should show outer.inner.name = updated, got: {}",
        output
    );

    // 测试单层成员访问赋值
    assert!(
        output.contains("outer.directField = 100"),
        "Should show outer.directField = 100, got: {}",
        output
    );

    // 测试多层嵌套
    assert!(
        output.contains("outer2.inner.value = 999"),
        "Should show outer2.inner.value = 999, got: {}",
        output
    );

    // 测试通过标记
    assert!(
        output.contains("All nested member assignment tests passed!"),
        "All tests should pass, got: {}",
        output
    );
}
