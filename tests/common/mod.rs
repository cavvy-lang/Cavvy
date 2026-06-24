//! Cavvy 语言集成测试公共模块
//!
//! 提供测试辅助函数和工具，被多个测试 crate 共享

use std::fs;
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

/// 获取当前平台的 cayc 可执行文件路径
/// Windows: ./target/release/cayc.exe
/// Linux: ./target/release/cayc
fn get_cayc_path() -> String {
    if cfg!(target_os = "windows") {
        "./target/release/cayc.exe".to_string()
    } else {
        "./target/release/cayc".to_string()
    }
}

/// 获取当前平台的可执行文件扩展名
/// Windows: .exe
/// Linux: 空字符串
fn get_exe_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

/// 全局测试锁，确保测试串行执行避免文件冲突
pub static TEST_LOCK: Mutex<()> = Mutex::new(());

/// 编译并运行单个 EOL 文件，返回输出结果
///
/// 使用 release 版本的 cayc.exe 编译 EOL 源代码为 EXE，
/// 然后执行生成的程序，最后清理生成的临时文件。
///
/// # Arguments
/// * `source_path` - EOL 源代码文件路径（相对于项目根目录）
///
/// # Returns
/// * `Ok(String)` - 成功时返回 stdout 字符串
/// * `Err(String)` - 失败时返回错误信息字符串
///
/// # Example
/// ```rust
/// let output = compile_and_run_eol("examples/hello.cay").expect("编译运行失败");
/// assert!(output.contains("Hello"));
/// ```
///
/// # Notes
/// - 时间复杂度: O(编译时间 + 执行时间)
/// - 会自动清理生成的 .exe 和 .ll 文件
pub fn compile_and_run_eol(source_path: &str) -> Result<String, String> {
    compile_and_run_eol_with_features(source_path, &[])
}

/// 编译并运行单个 EOL 文件，支持特性标志，返回输出结果
///
/// 使用 release 版本的 cayc.exe 编译 EOL 源代码为 EXE，
/// 支持传入特性标志（如 -F=top_level_function），
/// 然后执行生成的程序，最后清理生成的临时文件。
///
/// # Arguments
/// * `source_path` - EOL 源代码文件路径（相对于项目根目录）
/// * `features` - 特性标志列表，如 &["-F=top_level_function"]
///
/// # Returns
/// * `Ok(String)` - 成功时返回 stdout 字符串
/// * `Err(String)` - 失败时返回错误信息字符串
///
/// # Example
/// ```rust
/// let output = compile_and_run_eol_with_features(
///     "examples/toplevel.cay",
///     &["-F=top_level_function"]
/// ).expect("编译运行失败");
/// ```
pub fn compile_and_run_eol_with_features(
    source_path: &str,
    features: &[&str],
) -> Result<String, String> {
    compile_and_run_eol_with_timeout(source_path, features, Duration::from_secs(10))
}

/// 编译并运行单个 EOL 文件，支持特性标志和超时，返回输出结果
///
/// 使用 release 版本的 cayc.exe 编译 EOL 源代码为 EXE，
/// 支持传入特性标志（如 -F=top_level_function），
/// 然后执行生成的程序（带超时），最后清理生成的临时文件。
///
/// # Arguments
/// * `source_path` - EOL 源代码文件路径（相对于项目根目录）
/// * `features` - 特性标志列表，如 &["-F=top_level_function"]
/// * `timeout` - 执行超时时间
///
/// # Returns
/// * `Ok(String)` - 成功时返回 stdout 字符串
/// * `Err(String)` - 失败时返回错误信息字符串
///
/// # Example
/// ```rust
/// let output = compile_and_run_eol_with_timeout(
///     "examples/toplevel.cay",
///     &["-F=top_level_function"],
///     Duration::from_secs(5)
/// ).expect("编译运行失败");
/// ```
pub fn compile_and_run_eol_with_timeout(
    source_path: &str,
    features: &[&str],
    timeout: Duration,
) -> Result<String, String> {
    // 使用唯一ID生成输出文件名，避免测试冲突
    let unique_id = format!("{}_{:?}", std::process::id(), std::thread::current().id());
    let exe_ext = get_exe_extension();
    let exe_path = source_path.replace(".cay", &format!("_{}{}", unique_id, exe_ext));
    let ir_path = source_path.replace(".cay", &format!("_{}.ll", unique_id));

    // 构建参数
    let mut args = vec![source_path, &exe_path];
    for feature in features {
        args.push(feature);
    }

    // 1. 编译 EOL -> EXE (使用 release 版本)
    let cayc_path = get_cayc_path();
    let output = Command::new(&cayc_path)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute cayc: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("Compilation failed: {}", stderr));
    }

    // 2. 运行生成的 EXE（带超时）
    let mut child = Command::new(&exe_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", exe_path, e))?;

    let child_id = child.id();

    // 使用线程实现超时等待
    let result = wait_child_with_timeout(&mut child, timeout);

    // 如果超时或出错，强制终止进程树
    if result.is_err() {
        let _ = child.kill();
        let _ = child.wait();

        // 尝试强制终止（Windows）
        #[cfg(windows)]
        {
            let _ = Command::new("taskkill")
                .args(&["/F", "/T", "/PID", &child_id.to_string()])
                .output();
        }
    }

    // 3. 清理生成的文件
    let _ = fs::remove_file(&exe_path);
    let _ = fs::remove_file(&ir_path);

    result
}

/// 编译 EOL 文件，期望编译失败，返回错误信息
///
/// 用于测试应该产生编译错误的代码。
/// 编译失败后返回 stderr 输出，如果编译成功则返回错误。
///
/// # Arguments
/// * `source_path` - EOL 源代码文件路径（相对于项目根目录）
///
/// # Returns
/// * `Ok(String)` - 编译失败时返回 stderr 字符串
/// * `Err(String)` - 编译成功时返回错误
///
/// # Example
/// ```rust
/// let error = compile_eol_expect_error("examples/errors/error_test.cay")
///     .expect("应该编译失败");
/// assert!(error.contains("type mismatch"));
/// ```
pub fn compile_eol_expect_error(source_path: &str) -> Result<String, String> {
    compile_eol_expect_error_with_features(source_path, &[])
}

/// 编译 EOL 文件，期望编译失败，支持特性标志
pub fn compile_eol_expect_error_with_features(
    source_path: &str,
    features: &[&str],
) -> Result<String, String> {
    // 使用唯一ID生成输出文件名，避免测试冲突
    let unique_id = format!("{}_{:?}", std::process::id(), std::thread::current().id());
    let exe_ext = get_exe_extension();
    let exe_path = source_path.replace(".cay", &format!("_{}{}", unique_id, exe_ext));
    let ir_path = source_path.replace(".cay", &format!("_{}.ll", unique_id));

    // 1. 编译 EOL -> EXE (使用 release 版本)
    let cayc_path = get_cayc_path();
    let mut args = vec![source_path, &exe_path];
    for f in features {
        args.push(f);
    }
    let output = Command::new(&cayc_path)
        .args(&args)
        .output()
        .map_err(|e| format!("Failed to execute cayc: {}", e))?;

    // 清理可能生成的文件
    let _ = fs::remove_file(&exe_path);
    let _ = fs::remove_file(&ir_path);

    if output.status.success() {
        return Err("Expected compilation to fail, but it succeeded".to_string());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Ok(stderr)
}

/// 编译并运行 EOL 文件，期望执行失败（用于运行时错误测试），返回错误信息
///
/// 用于测试应该产生运行时错误的代码。
/// 编译成功后执行，如果执行失败返回错误信息，如果执行成功则返回错误。
///
/// # Arguments
/// * `source_path` - EOL 源代码文件路径（相对于项目根目录）
///
/// # Returns
/// * `Ok(String)` - 执行失败时返回错误信息字符串
/// * `Err(String)` - 执行成功时返回错误
///
/// # Example
/// ```rust
/// let error = compile_and_run_expect_error("examples/errors/runtime_error.cay")
///     .expect("应该运行时失败");
/// assert!(error.contains("division by zero"));
/// ```
pub fn compile_and_run_expect_error(source_path: &str) -> Result<String, String> {
    let exe_ext = get_exe_extension();
    let exe_path = source_path.replace(".cay", exe_ext);
    let ir_path = source_path.replace(".cay", ".ll");

    // 1. 编译 EOL -> EXE (使用 release 版本)
    let cayc_path = get_cayc_path();
    let output = Command::new(&cayc_path)
        .args(&[source_path, &exe_path])
        .output()
        .map_err(|e| format!("Failed to execute cayc: {}", e))?;

    if !output.status.success() {
        // 编译失败也返回错误信息
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let _ = fs::remove_file(&exe_path);
        let _ = fs::remove_file(&ir_path);
        return Ok(stderr);
    }

    // 2. 运行生成的 EXE
    let output = Command::new(&exe_path)
        .output()
        .map_err(|e| format!("Failed to execute {}: {}", exe_path, e))?;

    // 3. 清理生成的文件
    let _ = fs::remove_file(&exe_path);
    let _ = fs::remove_file(&ir_path);

    // 如果执行失败（非零退出码），返回错误信息
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        // 合并 stdout 和 stderr，因为错误信息可能输出到 stdout
        let combined = format!("{} {}", stdout, stderr);
        return Ok(format!("runtime error: {}", combined));
    }

    Err("Expected execution to fail, but it succeeded".to_string())
}

/// 断言输出包含所有指定的子字符串
///
/// # Arguments
/// * `output` - 实际的输出字符串
/// * `expected_substrings` - 预期包含的子字符串数组
/// * `test_name` - 测试名称，用于错误信息
pub fn assert_output_contains(output: &str, expected_substrings: &[&str], test_name: &str) {
    for substring in expected_substrings {
        assert!(
            output.contains(substring),
            "{}: Expected output to contain '{}', got: {}",
            test_name,
            substring,
            output
        );
    }
}

/// 断言输出包含任意一个指定的子字符串
///
/// # Arguments
/// * `output` - 实际的输出字符串
/// * `expected_substrings` - 预期包含的子字符串数组（至少包含一个）
/// * `test_name` - 测试名称，用于错误信息
pub fn assert_output_contains_any(output: &str, expected_substrings: &[&str], test_name: &str) {
    let found = expected_substrings.iter().any(|s| output.contains(s));
    assert!(
        found,
        "{}: Expected output to contain at least one of {:?}, got: {}",
        test_name, expected_substrings, output
    );
}

/// 等待子进程完成，带超时
///
/// 使用跨平台方式实现超时等待，避免使用非标准库的 wait_timeout
///
/// # Arguments
/// * `child` - 子进程句柄
/// * `timeout` - 超时时间
///
/// # Returns
/// * `Ok(String)` - 成功时返回 stdout 字符串
/// * `Err(String)` - 超时或失败时返回错误信息
fn wait_child_with_timeout(child: &mut Child, timeout: Duration) -> Result<String, String> {
    use std::sync::mpsc;

    // 获取输出管道
    let mut stdout_pipe = child.stdout.take().ok_or("Failed to get stdout pipe")?;
    let mut stderr_pipe = child.stderr.take().ok_or("Failed to get stderr pipe")?;

    // 使用通道接收输出
    let (tx, rx) = mpsc::channel();

    // 在线程中读取输出
    thread::spawn(move || {
        let mut stdout_buf = String::new();
        let mut stderr_buf = String::new();

        let stdout_result = stdout_pipe.read_to_string(&mut stdout_buf);
        let stderr_result = stderr_pipe.read_to_string(&mut stderr_buf);

        let _ = tx.send((stdout_buf, stderr_buf, stdout_result, stderr_result));
    });

    // 等待进程完成或超时
    let start = std::time::Instant::now();
    loop {
        // 检查进程是否已退出
        match child.try_wait() {
            Ok(Some(status)) => {
                // 进程已结束，等待输出读取完成
                let (stdout, stderr, stdout_res, stderr_res) = rx
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| "Timeout waiting for output read")?;

                if stdout_res.is_err() {
                    return Err(format!("Failed to read stdout: {:?}", stdout_res.err()));
                }
                if stderr_res.is_err() {
                    return Err(format!("Failed to read stderr: {:?}", stderr_res.err()));
                }

                if !status.success() {
                    return Err(format!("Execution failed: {}", stderr));
                }
                return Ok(stdout);
            }
            Ok(None) => {
                // 进程仍在运行，检查是否超时
                if start.elapsed() >= timeout {
                    return Err(format!("Execution timeout after {:?}", timeout));
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => {
                return Err(format!("Failed to check process status: {}", e));
            }
        }
    }
}
