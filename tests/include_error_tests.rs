//! 测试 #include 预处理器指令的错误定位
//!
//! 验证：当通过 #include 引入的代码产生编译错误时，
//! 错误信息应指向被包含文件的正确位置，而不是主文件的 #include 行。

use std::process::Command;

/// 辅助函数：获取 release 版 cayc 可执行文件路径
fn get_cayc_path() -> String {
    let exe_name = if cfg!(windows) { "cayc.exe" } else { "cayc" };
    format!("target/release/{}", exe_name)
}

/// 测试：include 文件中产生的代码生成错误应正确定位
#[test]
fn test_include_error_location() {
    let cayc = get_cayc_path();
    let source = "examples/test_include_error_main.cay";
    
    let output = Command::new(&cayc)
        .args(&[source, "examples/test_include_error_temp.exe"])
        .output()
        .expect("Failed to execute cayc");
    
    // 期望编译失败
    assert!(!output.status.success(), "Expected compilation to fail");
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // 错误应指向被包含文件 test_include_error_lib.cay，而不是主文件 main.cay
    assert!(
        stderr.contains("test_include_error_lib.cay"),
        "Error should reference the included file, not just the main file.\nstderr:\n{}",
        stderr
    );
    
    // 错误不应指向主文件的 #include 行（line 1）
    let main_file_header = format!("{}.cay:1:1", source.trim_end_matches(".cay"));
    assert!(
        !stderr.contains(&main_file_header),
        "Error should NOT point to the #include line of the main file.\nstderr:\n{}",
        stderr
    );
    
    // 清理临时文件
    let _ = std::fs::remove_file("examples/test_include_error_temp.exe");
    let _ = std::fs::remove_file("examples/test_include_error_temp.ll");
}
