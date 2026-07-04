//! Cavvy 内置函数集成测试
//!
//! 测试 print/println 的 {} format、eprint/eprintln 和 exit

mod common;
use common::compile_and_run_eol;
use std::process::Command;

#[test]
fn test_format_braces() {
    let output = compile_and_run_eol("examples/test_builtin_new.cay")
        .expect("builtin new example should compile and run");
    // 验证 {} format 对各类型的输出（注意 Rust 字符串中 {{}} 表示字面量 {}）
    assert!(
        output.contains("format test: 42"),
        "int {{}} format should output 42, got: {}",
        output
    );
    assert!(
        output.contains("format test: 123456789"),
        "long {{}} format should output 123456789, got: {}",
        output
    );
    assert!(
        output.contains("format test: 3.140000"),
        "double {{}} format should output 3.140000, got: {}",
        output
    );
    assert!(
        output.contains("format test: true"),
        "bool {{}} format should output true, got: {}",
        output
    );
    assert!(
        output.contains("format test: A"),
        "char {{}} format should output A, got: {}",
        output
    );
    assert!(
        output.contains("format test: hello"),
        "String {{}} format should output hello, got: {}",
        output
    );
}

#[test]
fn test_exit_code() {
    // 手动编译并执行 exit 测试，因为需要检查退出码
    let exe_path = if cfg!(target_os = "windows") {
        "examples/test_exit_builtin.exe"
    } else {
        "examples/test_exit_builtin"
    };
    let ir_path = "examples/test_exit_builtin.ll";

    let cayc_path = if cfg!(target_os = "windows") {
        "./target/release/cayc.exe"
    } else {
        "./target/release/cayc"
    };

    let compile_output = Command::new(cayc_path)
        .args([
            "examples/test_exit.cay",
            exe_path,
        ])
        .output()
        .expect("cayc should compile exit test");

    let compile_stderr = String::from_utf8_lossy(&compile_output.stderr);
    assert!(
        compile_output.status.success(),
        "exit test should compile, got: {}",
        compile_stderr
    );

    let output = Command::new(exe_path)
        .output()
        .expect("test_exit executable should run");

    // 清理生成的文件
    let _ = std::fs::remove_file(exe_path);
    let _ = std::fs::remove_file(ir_path);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("before exit"),
        "exit test should print before exit, got: {}",
        stdout
    );
    assert!(
        !stdout.contains("after exit"),
        "exit test should not print after exit, got: {}",
        stdout
    );
    assert_eq!(
        output.status.code(),
        Some(42),
        "exit(42) should produce exit code 42"
    );
}
