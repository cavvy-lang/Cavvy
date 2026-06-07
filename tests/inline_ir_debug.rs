//! 内联IR调试测试

use std::path::Path;
use std::process::Command;

#[test]
fn test_inline_ir_parsing() {
    let cay_path = Path::new("examples/test_inline_ir_basic.cay");

    // 使用 cay-check 检查语法
    let output = Command::new("cargo")
        .args([
            "run",
            "--release",
            "--bin",
            "cay-check",
            "--",
            "examples/test_inline_ir_basic.cay",
        ])
        .current_dir("E:\\spj\\EOL")
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
    // 使用 cay-ir 生成IR
    let output = Command::new("cargo")
        .args([
            "run",
            "--release",
            "--bin",
            "cay-ir",
            "--",
            "examples/test_inline_ir_basic.cay",
            "-o",
            "examples/test_inline_ir_debug.ll",
        ])
        .current_dir("E:\\spj\\EOL")
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
