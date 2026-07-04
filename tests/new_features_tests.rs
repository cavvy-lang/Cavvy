//! Cavvy 新特性集成测试
//!
//! 测试 struct、enum、@FreeFunction、泛型语法等功能

mod common;
use common::{assert_output_contains, compile_and_run_eol, compile_eol_expect_error};

// ============================================================
// struct 测试
// ============================================================

#[test]
fn test_struct_declaration() {
    let output = compile_and_run_eol("examples/test_struct.cay")
        .expect("test_struct.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "=== struct 声明测试 ===",
            "p.x = 10",
            "p.y = 20",
            "p.getX() = 10",
            "p.getY() = 20",
            "after p2.x = 99, p.x = 10",
            "after p2.x = 99, p2.x = 99",
            "=== struct 测试通过 ===",
        ],
        "test_struct",
    );
}

// ============================================================
// enum 测试
// ============================================================

#[test]
fn test_enum_declaration() {
    let output = compile_and_run_eol("examples/test_enum.cay")
        .expect("test_enum.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "=== enum 集成测试 ===",
            "Color test PASS",
            "Result.Ok(42) PASS",
            "Result.Err test PASS",
            "MyOption.Some PASS",
            "MyOption.None PASS",
            "=== enum 集成测试全部通过 ===",
        ],
        "test_enum",
    );
}

// ============================================================
// @FreeFunction 测试
// ============================================================

#[test]
fn test_freefunction_basic() {
    let output = compile_and_run_eol("examples/test_freefunction.cay")
        .expect("test_freefunction.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "=== @FreeFunction 测试 ===",
            "add(10, 20) = 30",
            "multiply(6, 7) = 42",
            "=== @FreeFunction 测试通过 ===",
        ],
        "test_freefunction",
    );
}

#[test]
fn test_freefunction_namespace() {
    let output = compile_and_run_eol("examples/test_freefunction_ns.cay")
        .expect("test_freefunction_ns.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "=== @FreeFunction 命名空间测试 ===",
            "math::square(5) = 25",
            "=== @FreeFunction 命名空间测试通过 ===",
        ],
        "test_freefunction_ns",
    );
}

#[test]
fn test_freefunction_conflict() {
    let error = compile_eol_expect_error("examples/test_freefunction_conflict.cay")
        .expect("test_freefunction_conflict.cay should fail to compile");
    // 应该包含 @FreeFunction 冲突相关的错误信息
    let has_conflict_msg = error.contains("@FreeFunction")
        || error.contains("greet")
        || error.contains("重复定义")
        || error.contains("DuplicateDefinition")
        || error.contains("已定义");
    assert!(
        has_conflict_msg,
        "Expected @FreeFunction conflict error, got: {}",
        error
    );
}

#[test]
fn test_freefunction_varargs() {
    let output = compile_and_run_eol("examples/test_freefunction_varargs.cay")
        .expect("test_freefunction_varargs.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "=== @FreeFunction VarArgs 测试 ===",
            "sumVarArgs() = 0",
            "sumVarArgs(42) = 42",
            "sumVarArgs(1, 2, 3, 4, 5) = 15",
            "sumVarArgs({10, 20, 30}) = 60",
            "joinStrings(\", \", \"apple\", \"banana\", \"cherry\") = apple, banana, cherry",
            "joinStrings(\"-\", \"hello\") = hello",
            "=== @FreeFunction VarArgs 测试通过 ===",
        ],
        "test_freefunction_varargs",
    );
}

#[test]
fn test_named_args() {
    let output = compile_and_run_eol("examples/test_named_args.cay")
        .expect("test_named_args.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "=== 命名参数测试 ===",
            "format('apple','banana','cherry', separator=', ') = apple, banana, cherry",
            "format('x','y', separator='-') = x-y",
            "add(a=10, b=20) = 30",
            "add(5, b=15) = 20",
            "repeat('num:', 1, 2, 3, suffix='!') = num:123!",
            "=== 命名参数测试通过 ===",
        ],
        "test_named_args",
    );
}

// ============================================================
// 泛型语法测试
// ============================================================

#[test]
fn test_generics_syntax() {
    let output = compile_and_run_eol("examples/test_generics.cay")
        .expect("test_generics.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "=== 泛型语法测试 ===",
            "class Box<T> syntax: OK",
            "enum Option<T> syntax: OK",
            "OptionalInt.of(42).get() = 42",
            "OptionalInt.empty().isEmpty() = true",
            "OptionalBool test: OK",
            "=== 泛型语法测试通过 ===",
        ],
        "test_generics",
    );
}

// ============================================================
// 编译期单态化测试
// ============================================================

#[test]
fn test_monomorphization() {
    let output = compile_and_run_eol("examples/test_monomorphization.cay")
        .expect("test_monomorphization.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "=== 编译期单态化测试 ===",
            "Box<int>(42).get() = 42",
            "Box<String>(\"Hello Mono\").get() = Hello Mono",
            "Box<int>.describe() = Box<int> specialized",
            "Box<String>.describe() = Generic Box",
            "Container<int>(100).getItem() = 100",
            "Container<String>(\"World\").getItem() = World",
            "=== 编译期单态化测试通过 ===",
        ],
        "test_monomorphization",
    );
}

// ============================================================
// 泛型静态工厂与实例方法单态化回归测试
// ============================================================

#[test]
fn test_generic_static_factory() {
    let output = compile_and_run_eol("examples/test_generic_static_factory.cay")
        .expect("test_generic_static_factory.cay should compile and run");
    assert_output_contains(
        &output,
        &["42", "10"],
        "test_generic_static_factory",
    );
}

// ============================================================
// 已有 Optional 测试回归
// ============================================================

#[test]
fn test_optional_full() {
    let output = compile_and_run_eol("examples/test_optional.cay")
        .expect("test_optional.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "=== OptionalInt 测试 ===",
            "some.isPresent() = true",
            "some.get() = 42",
            "none.isEmpty() = true",
            "some.orElse(0) = 42",
            "none.orElse(100) = 100",
            "=== OptionalBool 测试 ===",
            "=== OptionalDouble 测试 ===",
            "=== 所有 Optional 测试通过! ===",
        ],
        "test_optional",
    );
}
