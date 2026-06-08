//! 文档代码块集成测试
//!
//! 该测试调用 scripts/doc-test.py，自动扫描 README.md 和 docs/**/*.md 中的
//! Cavvy 代码块。运行前仍需先构建 release 编译器二进制。

use std::process::Command;

#[test]
fn markdown_cavvy_code_blocks_are_valid() {
    let python = if cfg!(target_os = "windows") {
        "python"
    } else {
        "python3"
    };

    let output = Command::new(python)
        .arg("scripts/doc-test.py")
        .output()
        .expect("failed to run scripts/doc-test.py");

    assert!(
        output.status.success(),
        "documentation code examples failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
