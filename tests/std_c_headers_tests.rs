//! C++ 风格 C 标准库包装回归测试。

mod common;

use common::compile_and_run_eol;

#[test]
fn std_c_headers_are_namespaced_and_work() {
    let output = compile_and_run_eol("examples/test_std_c_headers.cay")
        .expect("std::c* wrappers should compile and run");
    assert!(
        output.contains("std c headers"),
        "expected namespaced C stdio output, got: {output}"
    );
    assert!(
        !output.contains("std c headers failed"),
        "std::c* runtime checks failed: {output}"
    );
}
