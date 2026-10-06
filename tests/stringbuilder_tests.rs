//! StringBuilder 回归测试
//!
//! 覆盖此前因「假字节访问 / 假移动」而失效的字节级操作，
//! 以及 INT_MIN / LONG_MIN 追加与容量增长溢出。

mod common;
use common::compile_and_run_eol;

#[test]
fn test_stringbuilder_regressions() {
    let output = compile_and_run_eol("examples/test_stringbuilder.cay")
        .expect("test_stringbuilder.cay should compile and run");

    assert!(
        output.contains("StringBuilder tests passed"),
        "StringBuilder regression test should pass, got: {}",
        output
    );
}
