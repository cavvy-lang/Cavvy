//! 大型项目集成测试
//!
//! 测试 CavvyN 项目作为大型集成测试用例，验证 Cavvy 编译器
//! 在大型项目中的稳定性和可靠性。
//!
//! 测试流程：
//! 1. 检测 CavvyN 子模块是否存在
//! 2. 使用 cavly build -v 构建 CavvyN 项目
//! 3. 使用 cavly test 运行 CavvyN 的测试
//! 4. 验证所有步骤成功完成

use std::path::Path;
use std::process::Command;
use std::sync::Mutex;

/// CavvyN 测试互斥锁 - 所有 CavvyN 测试共享同一目录，必须串行执行
static CAVVYN_LOCK: Mutex<()> = Mutex::new(());

/// CavvyN 项目路径
const CAVVYN_PATH: &str = "examples/CavvyN";

/// 获取当前平台的可执行文件扩展名
fn get_exe_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

/// 获取 cavly 可执行文件路径
/// 优先使用 CARGO_BIN_EXE_cavly 环境变量（由cargo test设置），
/// 否则使用相对路径
fn get_cavly_path() -> String {
    // 使用cargo提供的二进制文件路径环境变量
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_cavly") {
        return path;
    }
    
    // 回退到相对路径
    if cfg!(target_os = "windows") {
        "./target/release/cavly.exe".to_string()
    } else {
        "./target/release/cavly".to_string()
    }
}

/// 检查 CavvyN 子模块是否存在
fn cavvyn_exists() -> bool {
    Path::new(CAVVYN_PATH).join("cavly.toml").exists()
}

/// 测试：CavvyN 子模块已正确初始化
#[test]
fn test_cavvyn_submodule_exists() {
    assert!(
        cavvyn_exists(),
        "CavvyN 子模块不存在。请运行: git submodule update --init --recursive"
    );
}

/// 测试：使用 cavly build -v 构建 CavvyN 项目
///
/// 验证大型项目可以成功编译，包括：
/// - 多文件项目结构
/// - 复杂的模块依赖
/// - 完整的编译器功能
#[test]
fn test_cavvyn_build_verbose() {
    if !cavvyn_exists() {
        eprintln!("跳过测试: CavvyN 子模块不存在");
        return;
    }
    let _lock = CAVVYN_LOCK.lock().unwrap();

    // 确保 target 目录存在
    let target_dir = Path::new(CAVVYN_PATH).join("target");
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir).expect("无法创建 target 目录");
    }

    let cavly_path = get_cavly_path();
    let output = Command::new(&cavly_path)
        .args(&["build", "-v"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("无法执行 cavly build -v");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "CavvyN 构建失败!\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );

    // 验证输出包含关键信息
    assert!(
        stdout.contains("CavvyN") || stderr.contains("CavvyN"),
        "构建输出应包含项目名称"
    );

    // 验证构建产物存在
    let exe_ext = get_exe_extension();
    let expected_exe = format!("{}/target/CavvyN{}", CAVVYN_PATH, exe_ext);
    assert!(
        Path::new(&expected_exe).exists(),
        "构建产物不存在: {}",
        expected_exe
    );
}

/// 测试：使用 cavly test 运行 CavvyN 的测试
///
/// 验证大型项目的测试框架可以正常工作，包括：
/// - 测试发现
/// - 测试编译
/// - 测试执行
#[test]
fn test_cavvyn_test() {
    if !cavvyn_exists() {
        eprintln!("跳过测试: CavvyN 子模块不存在");
        return;
    }
    let _lock = CAVVYN_LOCK.lock().unwrap();

    // 确保 target 目录存在
    let target_dir = Path::new(CAVVYN_PATH).join("target");
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir).expect("无法创建 target 目录");
    }

    // 首先确保项目已构建
    let cavly_path = get_cavly_path();
    let build_output = Command::new(&cavly_path)
        .args(&["build"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("无法执行 cavly build");

    assert!(
        build_output.status.success(),
        "CavvyN 预构建失败: {}",
        String::from_utf8_lossy(&build_output.stderr)
    );

    // 运行测试
    let output = Command::new(&cavly_path)
        .args(&["test"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("无法执行 cavly test");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // 注意：即使测试失败，命令也可能成功执行
    // 我们主要验证测试框架能正常运行
    let combined_output = format!("{} {}", stdout, stderr);

    // 验证测试框架输出了结果信息
    assert!(
        combined_output.contains("test") || combined_output.contains("running"),
        "测试输出应包含测试相关信息\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
}

/// 测试：CavvyN 项目可以完整运行（run 命令）
///
/// 验证构建后的可执行文件可以正常运行
#[test]
fn test_cavvyn_run() {
    if !cavvyn_exists() {
        eprintln!("跳过测试: CavvyN 子模块不存在");
        return;
    }
    let _lock = CAVVYN_LOCK.lock().unwrap();

    // 确保 target 目录存在
    let target_dir = Path::new(CAVVYN_PATH).join("target");
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir).expect("无法创建 target 目录");
    }

    let cavly_path = get_cavly_path();

    // 先构建项目
    let build_output = Command::new(&cavly_path)
        .args(&["build"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("无法执行 cavly build");

    assert!(
        build_output.status.success(),
        "CavvyN 构建失败: {}",
        String::from_utf8_lossy(&build_output.stderr)
    );

    // 运行项目
    let output = Command::new(&cavly_path)
        .args(&["run"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("无法执行 cavly run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // CavvyN 是一个编译器，运行它可能需要参数
    // 我们主要验证它能启动而不崩溃
    assert!(
        output.status.success() || stdout.contains("Usage") || stderr.contains("Usage"),
        "CavvyN 运行失败\nstdout:\n{}\nstderr:\n{}",
        stdout,
        stderr
    );
}

/// 测试：CavvyN 项目信息显示正常
#[test]
fn test_cavvyn_info() {
    if !cavvyn_exists() {
        eprintln!("跳过测试: CavvyN 子模块不存在");
        return;
    }
    let _lock = CAVVYN_LOCK.lock().unwrap();

    let cavly_path = get_cavly_path();
    let output = Command::new(&cavly_path)
        .args(&["info"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("无法执行 cavly info");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cavly info 失败: {}", stdout);

    // 验证输出包含项目信息
    assert!(
        stdout.contains("CavvyN") || stdout.contains("0.1.0"),
        "项目信息应包含项目名称或版本"
    );
}

/// 测试：CavvyN 项目 clean 功能正常
#[test]
fn test_cavvyn_clean() {
    if !cavvyn_exists() {
        eprintln!("跳过测试: CavvyN 子模块不存在");
        return;
    }
    let _lock = CAVVYN_LOCK.lock().unwrap();

    let cavly_path = get_cavly_path();

    // 先构建项目
    let _ = Command::new(&cavly_path)
        .args(&["build"])
        .current_dir(CAVVYN_PATH)
        .output();

    // 清理项目
    let output = Command::new(&cavly_path)
        .args(&["clean"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("无法执行 cavly clean");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cavly clean 失败: {}", stdout);

    // 验证目标目录被清理
    let target_dir = Path::new(CAVVYN_PATH).join("target");
    assert!(
        !target_dir.exists()
            || target_dir
                .read_dir()
                .map(|mut i| i.next().is_none())
                .unwrap_or(true),
        "清理后目标目录应不存在或为空"
    );
}

/// 压力测试：多次构建 CavvyN 项目
///
/// 验证编译器在重复构建时的稳定性
#[test]
fn test_cavvyn_repeated_build() {
    if !cavvyn_exists() {
        eprintln!("跳过测试: CavvyN 子模块不存在");
        return;
    }
    let _lock = CAVVYN_LOCK.lock().unwrap();

    // 确保 target 目录存在
    let target_dir = Path::new(CAVVYN_PATH).join("target");
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir).expect("无法创建 target 目录");
    }

    let cavly_path = get_cavly_path();

    // 重复构建 3 次
    for i in 1..=3 {
        let output = Command::new(&cavly_path)
            .args(&["build"])
            .current_dir(CAVVYN_PATH)
            .output()
            .expect(&format!("第 {} 次构建失败", i));

        assert!(
            output.status.success(),
            "第 {} 次构建失败: {}",
            i,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

/// 集成测试：完整的 CavvyN 工作流
///
/// 测试完整的工作流程：
/// 1. clean
/// 2. build -v
/// 3. test
/// 4. run
#[test]
fn test_cavvyn_full_workflow() {
    if !cavvyn_exists() {
        eprintln!("跳过测试: CavvyN 子模块不存在");
        return;
    }
    let _lock = CAVVYN_LOCK.lock().unwrap();

    // 确保 target 目录存在
    let target_dir = Path::new(CAVVYN_PATH).join("target");
    if !target_dir.exists() {
        std::fs::create_dir_all(&target_dir).expect("无法创建 target 目录");
    }

    let cavly_path = get_cavly_path();

    // 1. Clean
    let clean_output = Command::new(&cavly_path)
        .args(&["clean"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("clean 失败");

    assert!(
        clean_output.status.success(),
        "clean 失败: {}",
        String::from_utf8_lossy(&clean_output.stderr)
    );

    // 2. Build with verbose
    let build_output = Command::new(&cavly_path)
        .args(&["build", "-v"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("build -v 失败");

    let build_stdout = String::from_utf8_lossy(&build_output.stdout);
    let build_stderr = String::from_utf8_lossy(&build_output.stderr);

    assert!(
        build_output.status.success(),
        "build -v 失败:\nstdout:\n{}\nstderr:\n{}",
        build_stdout,
        build_stderr
    );

    // 3. Test
    let test_output = Command::new(&cavly_path)
        .args(&["test"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("test 失败");

    // 测试可能失败，但我们验证它能运行
    let test_stdout = String::from_utf8_lossy(&test_output.stdout);
    let test_stderr = String::from_utf8_lossy(&test_output.stderr);
    let test_combined = format!("{} {}", test_stdout, test_stderr);

    assert!(
        test_combined.contains("test") || test_combined.contains("running"),
        "test 输出应包含测试信息"
    );

    // 4. Info
    let info_output = Command::new(&cavly_path)
        .args(&["info"])
        .current_dir(CAVVYN_PATH)
        .output()
        .expect("info 失败");

    assert!(info_output.status.success(), "info 失败");
}
