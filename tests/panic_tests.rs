//! panic / abort 内置函数测试（ROADMAP 6.1.x）
//!
//! - panic 消息输出（回归）
//! - `--no-panic` 编译选项：panic()/abort() 转为编译错误
//! - `-g` debug 模式：abort 前打印调用栈回溯

mod common;
use common::{TEST_LOCK, compile_and_run_expect_error, compile_eol_expect_error_with_features};

/// 获取当前平台的 cayc 可执行文件路径（与 tests/common 保持一致）
fn cayc_path() -> String {
    if let Ok(path) = std::env::var("CAYC_PATH") {
        return path;
    }
    if cfg!(target_os = "windows") {
        "./target/release/cayc.exe".to_string()
    } else {
        "./target/release/cayc".to_string()
    }
}

#[test]
fn test_panic_builtin() {
    let error = compile_and_run_expect_error("examples/test_panic.cay")
        .expect("test_panic.cay should fail at runtime");
    assert!(
        error.contains("panic: critical failure"),
        "Expected panic message in output, got: {}",
        error
    );
}

#[test]
fn test_no_panic_flag() {
    let error = compile_eol_expect_error_with_features("examples/test_panic.cay", &["--no-panic"])
        .expect("test_panic.cay should fail to compile with --no-panic");
    assert!(
        error.contains("--no-panic"),
        "Expected --no-panic compile error, got: {}",
        error
    );
}

#[test]
fn test_panic_backtrace_debug_mode() {
    // 与 test_panic_builtin / test_no_panic_flag 共用 examples/test_panic.cay，
    // 加锁防止并发编译时中间文件互相覆盖。
    let _lock = TEST_LOCK.lock().unwrap();

    let exe_ext = if cfg!(target_os = "windows") { ".exe" } else { "" };
    let unique_id = format!("{}_{:?}", std::process::id(), std::thread::current().id());
    let exe_path = format!("examples/test_panic_bt_{}{}", unique_id, exe_ext);

    // 用 -g 编译
    let compile = std::process::Command::new(cayc_path())
        .args(["examples/test_panic.cay", &exe_path, "-g"])
        .output()
        .expect("failed to execute cayc");
    assert!(
        compile.status.success(),
        "compile with -g failed: {}",
        String::from_utf8_lossy(&compile.stderr)
    );

    // 运行：应 abort，stderr 含 panic 消息与回溯
    let run = std::process::Command::new(&exe_path)
        .output()
        .expect("failed to run test binary");
    let _ = std::fs::remove_file(&exe_path);
    let _ = std::fs::remove_file(exe_path.replace(exe_ext, ".ll"));

    assert!(!run.status.success(), "panic binary should abort");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(
        stderr.contains("panic: critical failure"),
        "Expected panic message, got: {}",
        stderr
    );
    assert!(
        stderr.contains("stack backtrace:"),
        "Expected backtrace header in -g mode, got: {}",
        stderr
    );
}
