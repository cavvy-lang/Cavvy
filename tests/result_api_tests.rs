//! Result<T, E> 完整 API 与 ? 运算符错误转换测试 (ROADMAP 6.1.x)
//!
//! - Result 补全 API：unwrapOrElse/map/mapErr/andThen/flatMap/inspect/inspectErr
//!   （map 家族为实例泛型方法，依赖编译器实例泛型方法支持）
//! - unwrap/unwrapErr 失败时 panic
//! - ? 运算符：E 实现 Into<E2> 时自动错误转换

mod common;
use common::{
    assert_output_contains, compile_and_run_eol, compile_and_run_expect_error,
    compile_eol_expect_error,
};

#[test]
fn test_result_full_api() {
    let output = compile_and_run_eol("examples/test_result_api.cay")
        .expect("test_result_api.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "42",     // unwrap / expect / inspect 值
            "7",      // unwrapOr
            "9",      // unwrapOrElse
            "43",     // map U=long
            "1000",   // map U=int（值类型健全性）
            "str",    // map U=String
            "mapped", // mapErr
            "84",     // andThen
            "stop",   // flatMap 传播 err
            "boom",   // inspectErr
            "true",
            "done",
        ],
        "test_result_api",
    );
}

#[test]
fn test_result_unwrap_panics_on_err() {
    let error = compile_and_run_expect_error("examples/test_result_unwrap_panic.cay")
        .expect("test_result_unwrap_panic.cay should panic at runtime");
    assert!(
        error.contains("called unwrap() on an Err Result"),
        "Expected unwrap panic message, got: {}",
        error
    );
}

#[test]
fn test_try_operator_into_conversion() {
    let output = compile_and_run_eol("examples/test_try_into.cay")
        .expect("test_try_into.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "ok=8080",   // 无错误时正常传播
            "err=bad port", // ConfigError 经 into() 转为 IOError 后 message 保留
            "true",      // kind() == IOErrorKind.InvalidInput
            "done",
        ],
        "test_try_into",
    );
}

#[test]
fn test_try_operator_multi_into_instantiations() {
    let output = compile_and_run_eol("examples/test_try_into_multi.cay")
        .expect("test_try_into_multi.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "io=dual failure",    // ? 静态分派到 Into<IOError>::into()
            "true",               // kind() == IOErrorKind.InvalidInput
            "parse=dual failure", // ? 静态分派到 Into<ParseError>::into()
            "7",                  // ParseError.line
            "3",                  // ParseError.column
            "done",
        ],
        "test_try_into_multi",
    );
}

#[test]
fn test_try_into_ambiguous_direct_call_is_compile_error() {
    let error = compile_eol_expect_error("examples/errors/try_into_ambiguous_direct_call.cay")
        .expect("try_into_ambiguous_direct_call.cay should be a compile error");
    assert!(
        error.contains("有歧义"),
        "Expected ambiguity error for direct into() call, got: {}",
        error
    );
}

/// `?` 在 `Optional<T>` 上的展开：
/// - hasValue == true → 提取 value 继续
/// - hasValue == false → 提前返回 `Optional<U>.empty()`（U == T）
#[test]
fn test_try_operator_on_optional() {
    let output = compile_and_run_eol("examples/test_try_optional.cay")
        .expect("test_try_optional.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "present=84",                  // Optional.of(42)? -> 42; 42*2 -> Optional.of(84)
            "empty=propagated",            // 源为空时 ? 提前返回空 Optional<int>
            "chain_inner_empty=propagated", // 链式 ? 内层为空时立即返回空
            "chain_outer_empty=propagated", // 链式 ? 外层为空时立即返回空
            "chain_both=84",               // 链式 ? 两侧都有值：42+42=84
            "done",
        ],
        "test_try_optional",
    );
}

/// `?` 在未实现 std::Try<T, E> 的类型上必须报语义错误。
#[test]
fn test_try_operator_on_non_result_optional_is_compile_error() {
    let error = compile_eol_expect_error("examples/errors/try_on_non_result_optional.cay")
        .expect("try_on_non_result_optional.cay should be a compile error");
    assert!(
        error.contains("?") && (error.contains("Try") || error.contains("Result") || error.contains("Optional")),
        "Expected '?' operator type error, got: {}",
        error
    );
}

/// 自定义 Try<T, E> 实现与 ? 运算符测试 (ROADMAP 6.3.x)。
///
/// 验证任何实现了 std::Try<T, E> 的类型（非 Result/Optional）均可使用 ? 运算符，
/// 就像实现 Iterator 即可用于 for 循环一样。
///
/// 覆盖路径：
/// - 同型快路径：Loader<int> ? → Loader<int>（Self == R，直接返回操作数）
/// - fromError 分派：Loader<int> ? → Result<int, String>（Self != R，经返回类型
///   vtable 调用 Try::fromError 构造失败值）
/// - 链式 ? 传播
#[test]
fn test_try_operator_on_custom_try_interface() {
    let output = compile_and_run_eol("examples/test_try_interface.cay")
        .expect("test_try_interface.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "loader_ok=8080",        // 成功路径：isOk()==true，getValue() 经 vtable 提取
            "loader_err=invalid port", // 失败路径：同型直返，getError() 经 vtable 提取
            "result_ok=8080",        // fromError 分派成功路径
            "result_err=invalid port", // fromError 分派：Loader getError + Result fromError
            "chain_both=16160",      // 链式 ? 两侧成功：8080+8080
            "chain_first_err=invalid port", // 链式 ? 第一个失败：立即传播
            "done",
        ],
        "test_try_interface",
    );
}

/// 多 Into 实例化的接口动态分派：通过 `Into<IOError>` 与 `Into<ParseError>`
/// 接口引用调用 `into()` 时，vtable 必须按接口类型实参分派到正确的重载。
///
/// 验证 vtable 槽位按泛型接口实例化独立分配，且槽位填充按返回类型消歧：
/// - `Into<IOError>  ref.into()` 命中返回 `IOError`  的重载
/// - `Into<ParseError> ref.into()` 命中返回 `ParseError` 的重载
#[test]
fn test_into_dynamic_dispatch_multi_instantiations() {
    let output = compile_and_run_eol("examples/test_into_dynamic_dispatch.cay")
        .expect("test_into_dynamic_dispatch.cay should compile and run");
    assert_output_contains(
        &output,
        &[
            "io=dual failure",  // Into<IOError> 接口引用分派到 IOError 版本
            "parse=dual failure", // Into<ParseError> 接口引用分派到 ParseError 版本
            "7",                // ParseError.line
            "3",                // ParseError.column
            "done",
        ],
        "test_into_dynamic_dispatch",
    );
}
