//! 内联IR调试测试

use std::path::Path;
use std::process::Command;

mod common;

/// 获取当前平台的可执行文件扩展名
fn get_exe_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

/// 获取 release 二进制文件路径
fn release_bin(name: &str) -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from("./target/release");
    path.push(format!("{}{}", name, get_exe_extension()));
    path
}

#[test]
fn test_inline_ir_parsing() {
    let _lock = common::TEST_LOCK.lock().unwrap();
    let cay_path = Path::new("examples/test_inline_ir_basic.cay");

    // 使用已编译的 cay-check 二进制文件检查语法
    // 避免使用 cargo run 以防止触发 llvm-sys 重新编译
    let cay_check_bin = release_bin("cay-check");
    assert!(
        cay_check_bin.exists(),
        "cay-check binary not found at {:?}",
        cay_check_bin
    );

    let output = Command::new(&cay_check_bin)
        .args(["examples/test_inline_ir_basic.cay"])
        .output()
        .expect("Failed to run cay-check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("cay-check stdout:\n{}", stdout);
    println!("cay-check stderr:\n{}", stderr);

    // 检查是否成功
    if !output.status.success() {
        panic!("cay-check failed: {}", stderr);
    }
}

#[test]
fn test_inline_ir_generation() {
    let _lock = common::TEST_LOCK.lock().unwrap();

    // 使用已编译的 cay-ir 二进制文件生成IR
    // 避免使用 cargo run 以防止触发 llvm-sys 重新编译
    let cay_ir_bin = release_bin("cay-ir");
    assert!(
        cay_ir_bin.exists(),
        "cay-ir binary not found at {:?}",
        cay_ir_bin
    );

    let output = Command::new(&cay_ir_bin)
        .args([
            "examples/test_inline_ir_basic.cay",
            "-o",
            "examples/test_inline_ir_debug.ll",
        ])
        .output()
        .expect("Failed to run cay-ir");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    println!("cay-ir stdout:\n{}", stdout);
    println!("cay-ir stderr:\n{}", stderr);

    // 读取生成的IR文件
    let ir_content = std::fs::read_to_string("examples/test_inline_ir_debug.ll")
        .expect("Failed to read generated IR file");

    println!("Generated IR:\n{}", ir_content);

    // 检查是否包含内联IR标记
    assert!(
        ir_content.contains("Inline IR block start"),
        "Should contain inline IR start marker"
    );
    assert!(
        ir_content.contains("Inline IR block end"),
        "Should contain inline IR end marker"
    );
}
