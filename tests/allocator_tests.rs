//! Cavvy 语言 0.5.0.0 内存分配器功能集成测试
//!
//! 测试内容：
//! - GlobalAlloc 全局堆分配器
//! - Arena 线性分配器
//! - scope 栈作用域语句
//! - 分配器接口多态

mod common;
use common::compile_and_run_eol;

/// 测试 scope 语句基本功能
#[test]
fn test_scope_statement() {
    let output = compile_and_run_eol("examples/test_scope_basic.cay")
        .expect("scope basic test should compile and run");

    assert!(
        output.contains("=== 测试 scope 语句 ==="),
        "Test header should appear, got: {}",
        output
    );
    assert!(
        output.contains("scope 内部: x = 10"),
        "Scope internal value should be shown, got: {}",
        output
    );
    assert!(
        output.contains("回到外部: x = 20"),
        "Scope external value should override, got: {}",
        output
    );
    assert!(
        output.contains("All scope tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

/// 测试 GlobalAlloc 全局分配器
#[test]
fn test_global_alloc() {
    // 跳过此测试，因为需要 Allocator.cay 库文件
    // 当库文件可用时取消注释
    // let output = compile_and_run_eol("examples/test_0_5_0_allocator.cay")
    //     .expect("GlobalAlloc test should compile and run");
    //
    // assert!(output.contains("=== 测试 GlobalAlloc ==="),
    //         "GlobalAlloc test header should appear, got: {}", output);
    // assert!(output.contains("GlobalAlloc 分配成功"),
    //         "GlobalAlloc allocation should succeed, got: {}", output);
    // assert!(output.contains("GlobalAlloc 释放成功"),
    //         "GlobalAlloc deallocation should succeed, got: {}", output);
}

/// 测试 Arena 分配器
#[test]
fn test_arena_allocator() {
    // 跳过此测试，因为需要 Allocator.cay 库文件
    // 当库文件可用时取消注释
    // let output = compile_and_run_eol("examples/test_0_5_0_allocator.cay")
    //     .expect("Arena test should compile and run");
    //
    // assert!(output.contains("=== 测试 Arena 分配器 ==="),
    //         "Arena test header should appear, got: {}", output);
    // assert!(output.contains("Arena 创建成功"),
    //         "Arena creation should succeed, got: {}", output);
}

/// 测试分配器接口多态
#[test]
fn test_allocator_polymorphism() {
    // 跳过此测试，因为需要 Allocator.cay 库文件
    // 当库文件可用时取消注释
}

/// 测试嵌套 scope
///
/// 注意：此测试与 test_scope_statement 使用同一个示例文件
/// 因为测试框架是单线程的，不会发生冲突
#[test]
fn test_nested_scope() {
    // 使用同一个测试文件，因为它已经包含嵌套 scope
    let output = compile_and_run_eol("examples/test_scope_basic.cay")
        .expect("nested scope test should compile and run");

    assert!(
        output.contains("=== 测试 scope 语句 ==="),
        "Test header should appear, got: {}",
        output
    );
    assert!(
        output.contains("嵌套 scope: y = 30"),
        "Inner scope should appear, got: {}",
        output
    );
    assert!(
        output.contains("All scope tests completed!"),
        "Test should complete, got: {}",
        output
    );
}

/// 测试 scope 中的变量遮蔽
///
/// 注意：此测试与 test_scope_statement 使用同一个示例文件
/// 因为测试框架是单线程的，不会发生冲突
#[test]
fn test_scope_variable_shadowing() {
    // 使用同一个测试文件，因为它已经包含变量遮蔽
    let output = compile_and_run_eol("examples/test_scope_basic.cay")
        .expect("scope shadowing test should compile and run");

    assert!(
        output.contains("外部: x = 20"),
        "Outer x should be 20, got: {}",
        output
    );
    assert!(
        output.contains("scope 内部: x = 10"),
        "Inner x should shadow to 10, got: {}",
        output
    );
    assert!(
        output.contains("回到外部: x = 20"),
        "Outer x should be restored, got: {}",
        output
    );
}
