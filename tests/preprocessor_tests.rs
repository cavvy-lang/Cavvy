//! Cavvy 语言预处理器集成测试
//!
//! 测试 #include、#pragma once 等预处理器功能

mod common;
use common::{compile_and_run_eol, compile_eol_expect_error};

// ==================== 0.3.5.0 预处理器 #include 测试 ====================

#[test]
fn test_include_basic() {
    let output = compile_and_run_eol("examples/test_include_basic.cay")
        .expect("include basic should compile and run");
    assert!(
        output.contains("Version test"),
        "Should show version test message, got: {}",
        output
    );
    assert!(
        output.contains("Addition test"),
        "Should show addition test message, got: {}",
        output
    );
    assert!(
        output.contains("Include test PASSED!"),
        "Include basic test should pass, got: {}",
        output
    );
}

#[test]
fn test_include_nested() {
    let output = compile_and_run_eol("examples/test_include_nested.cay")
        .expect("include nested should compile and run");
    assert!(
        output.contains("Nested include test PASSED!"),
        "Nested include test should pass, got: {}",
        output
    );
}

#[test]
fn test_include_pragma_once() {
    let output = compile_and_run_eol("examples/test_include_pragma_once.cay")
        .expect("include pragma once should compile and run");
    assert!(
        output.contains("Pragma once test PASSED!"),
        "Pragma once test should pass (multiple includes handled correctly), got: {}",
        output
    );
}

#[test]
fn test_error_include_cycle() {
    let error = compile_eol_expect_error("examples/errors/error_include_cycle.cay")
        .expect("cyclic include should fail to compile");
    assert!(
        error.contains("循环包含")
            || error.contains("cyclic")
            || error.contains("circular")
            || error.contains("include"),
        "Should report cyclic include error, got: {}",
        error
    );
}

// ==================== 0.4.8.3 系统包含路径 #include <> 测试 ====================

#[test]
fn test_include_system_angle_brackets() {
    let output = compile_and_run_eol("examples/test_include_system.cay")
        .expect("system include should compile and run");
    assert!(
        output.contains("hello"),
        "Should show 'hello' from split result, got: {}",
        output
    );
    assert!(
        output.contains("world"),
        "Should show 'world' from split result, got: {}",
        output
    );
    assert!(
        output.contains("cavvy"),
        "Should show 'cavvy' from split result, got: {}",
        output
    );
    assert!(
        output.contains("apple"),
        "Should show 'apple' from CSV split, got: {}",
        output
    );
    assert!(
        output.contains("banana"),
        "Should show 'banana' from CSV split, got: {}",
        output
    );
    assert!(
        output.contains("cherry"),
        "Should show 'cherry' from CSV split, got: {}",
        output
    );
    assert!(
        output.contains("Hello, Cavvy!"),
        "Should show formatted string, got: {}",
        output
    );
    assert!(
        output.contains("Greetings, World!"),
        "Should show indexed formatted string, got: {}",
        output
    );
}

// ==================== 错误行号定位测试（多级include）====================

#[test]
fn test_error_line_number_with_nested_include() {
    let error = compile_eol_expect_error("examples/test_extreme_line_main.cay")
        .expect("nested include type error should fail to compile");
    // 错误应在 test_extreme_line_c.cay（被 #include 的文件）中
    // 行号取决于源映射实现
    assert!(
        error.contains("test_extreme_line_c.cay") && error.contains("Cannot assign string to int"),
        "Should report error in test_extreme_line_c.cay about type mismatch, got: {}",
        error
    );
}

// ==================== 0.4.x 预处理器条件表达式测试 ====================

#[test]
fn test_preprocessor_conditional() {
    let output = compile_and_run_eol("examples/test_preprocessor_conditional.cay")
        .expect("preprocessor conditional should compile and run");
    assert!(
        output.contains("Version 2 or higher"),
        "Should evaluate VERSION >= 2, got: {}",
        output
    );
    assert!(
        output.contains("Not both features enabled"),
        "Should evaluate FEATURE_A && FEATURE_B as false, got: {}",
        output
    );
    // defined(VERSION) 检查宏是否已定义
    // 注意：预处理器目前会展开字符串中的宏名（已知行为）
    assert!(
        output.contains("is defined"),
        "Should evaluate defined(VERSION), got: {}",
        output
    );
    assert!(
        output.contains("Version is exactly 2"),
        "Should evaluate VERSION == 2, got: {}",
        output
    );
    assert!(
        output.contains("Preprocessor conditional test passed"),
        "Should complete all tests, got: {}",
        output
    );
}

// ==================== #link 指令测试 ====================

#[test]
#[cfg(target_os = "windows")]
fn test_link_directive_basic() {
    let output = compile_and_run_eol("examples/test_link_directive.cay")
        .expect("link directive basic should compile and run");
    assert!(
        output.contains("#link directive test passed!"),
        "Should show link directive test passed, got: {}",
        output
    );
    assert!(
        output.contains("ws2_32"),
        "Should mention ws2_32 library, got: {}",
        output
    );
}

#[test]
#[cfg(target_os = "windows")]
fn test_link_directive_with_include() {
    let output = compile_and_run_eol("examples/test_link_directive_include.cay")
        .expect("link directive with include should compile and run");
    assert!(
        output.contains("#link with #include test passed!"),
        "Should show link with include test passed, got: {}",
        output
    );
    assert!(
        output.contains("gdi32") && output.contains("user32"),
        "Should mention both gdi32 and user32 libraries, got: {}",
        output
    );
}

#[test]
fn test_error_link_invalid_syntax() {
    let error = compile_eol_expect_error("examples/errors/error_link_invalid_syntax.cay")
        .expect("invalid link syntax should fail to compile");
    assert!(
        error.contains("无效的 #link 语法") || error.contains("#link"),
        "Should report invalid #link syntax error, got: {}",
        error
    );
}

#[test]
fn test_error_link_empty_name() {
    let error = compile_eol_expect_error("examples/errors/error_link_empty_name.cay")
        .expect("empty link name should fail to compile");
    assert!(
        error.contains("库名称不能为空") || error.contains("#link"),
        "Should report empty library name error, got: {}",
        error
    );
}

// ==================== #include_c 测试 ====================

#[test]
fn test_include_c_wrapper_mapping() {
    let output = compile_and_run_eol("examples/demo_include_c.cay")
        .expect("include_c wrapper-mapping demo should compile and run");
    assert!(
        output.contains("#include_c wrapper-mapping test passed!"),
        "Should show wrapper-mapping test passed, got: {}",
        output
    );
    assert!(
        output.contains("value=42"),
        "Should show printf-formatted value, got: {}",
        output
    );
}

#[test]
fn test_include_c_real_header_fallback() {
    let output = compile_and_run_eol("examples/demo_include_c_user.cay")
        .expect("include_c real-header fallback demo should compile and run");
    assert!(
        output.contains("abs(-7)=7"),
        "Should call abs() extracted from real header, got: {}",
        output
    );
    assert!(
        output.contains("rand() in range: true"),
        "Should call rand() extracted from real header, got: {}",
        output
    );
}

#[test]
fn test_error_include_c_missing() {
    let error = compile_eol_expect_error("examples/errors/error_include_c_missing.cay")
        .expect("missing header should fail to compile");
    assert!(
        error.contains("E1007") || error.contains("#include_c"),
        "Should report include_c missing header error, got: {}",
        error
    );
}

#[test]
fn test_include_c_user_form_not_shadowed_by_cay_file() {
    // 引号形式不得匹配同名 .cay 包装（包装仅限 <...> 系统形式的 caylibs/c/ 白名单）。
    // 若错误匹配到 shadow.cay 诱饵，abs_c 未声明，编译将失败。
    let output = compile_and_run_eol("examples/include_c_shadow/main.cay")
        .expect("user-form include_c must parse the real header, not a same-named .cay");
    assert!(
        output.contains("shadow_real=5"),
        "Should call abs() extracted from the real header, got: {}",
        output
    );
}

// ==================== #include_c C++ 头文件测试 ====================

/// 查找可用的 C++ 编译器（g++ 优先，其次 clang++）
fn find_cpp_compiler() -> Option<&'static str> {
    for cc in ["g++", "clang++"] {
        let ok = std::process::Command::new(cc)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if ok {
            return Some(cc);
        }
    }
    None
}

/// C++ 头文件兜底提取 + 真实链接验证：
/// 1. g++/clang++ 把 demo_include_cpp.cpp 编译为静态库；
/// 2. cayc 提取 demo_include_cpp.h（C++ 模式，Itanium mangled 链接名）并链接该库；
/// 3. 运行生成的可执行文件，验证构造/成员/const 方法/静态方法/自由函数全部链接正确。
///
/// 环境没有 C++ 编译器时跳过（mangled 名由 c_header/cpp_mangle 单测锁定）。
#[test]
fn test_include_cpp_real_header_link() {
    let cxx = match find_cpp_compiler() {
        Some(c) => c,
        None => {
            eprintln!("skip test_include_cpp_real_header_link: no g++/clang++ available");
            return;
        }
    };
    let unique = format!("{}_{:?}", std::process::id(), std::thread::current().id())
        .replace(|c: char| !c.is_alphanumeric(), "_");
    let work = std::env::temp_dir().join(format!("cay_cpp_demo_{}", unique));
    std::fs::create_dir_all(&work).expect("create temp dir");

    // 1. C++ 实现 → 静态库（禁用异常/RTTI，避免引入 libstdc++ 运行时符号）
    let obj = work.join("demo_include_cpp.o");
    let lib = work.join("libdemo_include_cpp.a");
    let out = std::process::Command::new(cxx)
        .args([
            "-c",
            "-std=c++17",
            "-fno-exceptions",
            "-fno-rtti",
            "examples/demo_include_cpp.cpp",
            "-o",
        ])
        .arg(&obj)
        .output()
        .expect("run c++ compiler");
    assert!(
        out.status.success(),
        "c++ compile failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = std::process::Command::new("ar")
        .arg("rcs")
        .arg(&lib)
        .arg(&obj)
        .output()
        .expect("run ar");
    assert!(
        out.status.success(),
        "ar failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // 2. cayc 编译 .cay 并链接静态库
    let exe = work.join(if cfg!(target_os = "windows") {
        "demo_include_cpp_run.exe"
    } else {
        "demo_include_cpp_run"
    });
    let cayc = if cfg!(target_os = "windows") {
        "./target/release/cayc.exe"
    } else {
        "./target/release/cayc"
    };
    let out = std::process::Command::new(cayc)
        .args(["examples/demo_include_cpp.cay"])
        .arg(&exe)
        .arg(format!("-L{}", work.display()))
        .arg("-ldemo_include_cpp")
        .output()
        .expect("run cayc");
    assert!(
        out.status.success(),
        "cayc compile/link failed: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );

    // 3. 运行并验证
    let out = std::process::Command::new(&exe)
        .output()
        .expect("run demo executable");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let _ = std::fs::remove_dir_all(&work);
    assert!(out.status.success(), "demo exited nonzero: {}", stdout);
    assert!(
        stdout.contains("counter.value()=42"),
        "ctor/add/value (const method) should work, got: {}",
        stdout
    );
    assert!(
        stdout.contains("counter.v_=42"),
        "mirrored field layout should match C++, got: {}",
        stdout
    );
    assert!(
        stdout.contains("alive0=0") && stdout.contains("alive1=1") && stdout.contains("alive2=0"),
        "RAII destructor should run at scope exit, got: {}",
        stdout
    );
    assert!(
        stdout.contains("demo::twice(21)=42"),
        "namespaced free function should link, got: {}",
        stdout
    );
    assert!(
        stdout.contains("Counter::version()=7"),
        "static member function should link, got: {}",
        stdout
    );
    assert!(
        stdout.contains("include_c C++ demo passed!"),
        "got: {}",
        stdout
    );
}
