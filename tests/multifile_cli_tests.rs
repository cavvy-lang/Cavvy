//! cayc CLI 多文件编译与 `-c` 选项回归测试

use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn cayc_path() -> String {
    if let Ok(path) = std::env::var("CAYC_PATH") {
        return path;
    }
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_cayc") {
        return path;
    }
    if cfg!(target_os = "windows") {
        "./target/release/cayc.exe".to_string()
    } else {
        "./target/release/cayc".to_string()
    }
}

fn exe_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

#[test]
fn compile_only_creates_object_files() {
    let tmp = PathBuf::from("target/tmp/multifile_compile_only");
    fs::create_dir_all(&tmp).unwrap();

    let helper = tmp.join("helper.cay");
    let main_file = tmp.join("main.cay");
    let helper_obj = tmp.join("helper.obj");
    let main_obj = tmp.join("main.obj");

    fs::write(
        &helper,
        r#"public class Helper {
    public static int getValue() {
        return 42;
    }
}
"#,
    )
    .unwrap();

    fs::write(
        &main_file,
        r#"public class MainOnly {
    public static void main() {
        println("ok");
    }
}
"#,
    )
    .unwrap();

    // 清理之前可能残留的文件
    let _ = fs::remove_file(&helper_obj);
    let _ = fs::remove_file(&main_obj);

    let output = Command::new(cayc_path())
        .arg("-c")
        .arg(&helper)
        .arg(&main_file)
        .output()
        .expect("failed to run cayc -c");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "cayc -c should succeed:\n{}",
        stderr
    );
    assert!(
        helper_obj.exists(),
        "helper.obj should be created next to source"
    );
    assert!(
        main_obj.exists(),
        "main.obj should be created next to source"
    );

    // 清理
    let _ = fs::remove_file(&helper_obj);
    let _ = fs::remove_file(&main_obj);
    let _ = fs::remove_file(&tmp.join("helper.ll"));
    let _ = fs::remove_file(&tmp.join("main.ll"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn compile_only_with_output_names_object_file() {
    let tmp = PathBuf::from("target/tmp/multifile_compile_only_output");
    fs::create_dir_all(&tmp).unwrap();

    let main_file = tmp.join("main.cay");
    let named_obj = tmp.join("build").join("main.obj");
    fs::create_dir_all(named_obj.parent().unwrap()).unwrap();

    fs::write(
        &main_file,
        r#"public class MainOnly {
    public static void main() {
        println("ok");
    }
}
"#,
    )
    .unwrap();

    let _ = fs::remove_file(&named_obj);
    let _ = fs::remove_file(tmp.join("main.obj"));

    // 单源文件 -c 时，-o 指定目标文件名（与 gcc/clang 一致）
    let output = Command::new(cayc_path())
        .arg("-c")
        .arg(&main_file)
        .arg("-o")
        .arg(&named_obj)
        .output()
        .expect("failed to run cayc -c -o");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "cayc -c -o should succeed:\n{}",
        stderr
    );
    assert!(
        named_obj.exists(),
        "-o named object file should be created"
    );
    assert!(
        !tmp.join("main.obj").exists(),
        "default source-stem object file should not be created"
    );

    // 多源文件 -c 时指定输出名应报错
    let helper = tmp.join("helper.cay");
    fs::write(
        &helper,
        r#"public class Helper {
    public static int getValue() {
        return 42;
    }
}
"#,
    )
    .unwrap();

    let output = Command::new(cayc_path())
        .arg("-c")
        .arg(&helper)
        .arg(&main_file)
        .arg("-o")
        .arg(&named_obj)
        .output()
        .expect("failed to run cayc -c with multiple sources");

    assert!(
        !output.status.success(),
        "cayc -c with multiple sources and -o should fail"
    );

    // 清理
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn multiple_sources_link_to_single_executable() {
    let tmp = PathBuf::from("target/tmp/multifile_link");
    fs::create_dir_all(&tmp).unwrap();

    let helper = tmp.join("helper.cay");
    let main_file = tmp.join("main.cay");
    let exe = tmp.join(format!("combined{}", exe_extension()));

    fs::write(
        &helper,
        r#"public class Helper {
    public static int getValue() {
        return 42;
    }
}
"#,
    )
    .unwrap();

    fs::write(
        &main_file,
        r#"public class MainApp {
    public static void main() {
        println("linked");
    }
}
"#,
    )
    .unwrap();

    let _ = fs::remove_file(&exe);

    let output = Command::new(cayc_path())
        .arg(&helper)
        .arg(&main_file)
        .arg(&exe)
        .output()
        .expect("failed to run cayc with multiple sources");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "cayc should compile and link multiple sources:\n{}",
        stderr
    );
    assert!(exe.exists(), "combined executable should be created");

    let run_output = Command::new(&exe)
        .output()
        .expect("failed to run combined executable");
    let stdout = String::from_utf8_lossy(&run_output.stdout);
    assert!(
        stdout.contains("linked"),
        "combined executable should print 'linked', got: {}",
        stdout
    );

    // 清理
    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(&tmp.join("helper.obj"));
    let _ = fs::remove_file(&tmp.join("main.obj"));
    let _ = fs::remove_file(&tmp.join("helper.ll"));
    let _ = fs::remove_file(&tmp.join("main.ll"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn single_source_with_output_still_works() {
    let tmp = PathBuf::from("target/tmp/single_source_output");
    fs::create_dir_all(&tmp).unwrap();

    let source = tmp.join("hello.cay");
    let exe = tmp.join(format!("hello_custom{}", exe_extension()));

    fs::write(
        &source,
        r#"public class Hello {
    public static void main() {
        println("single");
    }
}
"#,
    )
    .unwrap();

    let _ = fs::remove_file(&exe);

    let output = Command::new(cayc_path())
        .arg(&source)
        .arg(&exe)
        .output()
        .expect("failed to run cayc with single source and output");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "cayc single source with explicit output should succeed:\n{}",
        stderr
    );
    assert!(exe.exists(), "custom-named executable should be created");

    let run_output = Command::new(&exe)
        .output()
        .expect("failed to run executable");
    let stdout = String::from_utf8_lossy(&run_output.stdout);
    assert!(
        stdout.contains("single"),
        "executable should print 'single', got: {}",
        stdout
    );

    // 清理
    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(&tmp.join("hello.obj"));
    let _ = fs::remove_file(&tmp.join("hello.ll"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn multiple_sources_with_dash_o() {
    let tmp = PathBuf::from("target/tmp/multifile_dash_o");
    fs::create_dir_all(&tmp).unwrap();

    let helper = tmp.join("helper.cay");
    let main_file = tmp.join("main.cay");
    let exe = tmp.join(format!("myapp{}", exe_extension()));

    fs::write(
        &helper,
        r#"public class Helper {
    public static int getValue() {
        return 42;
    }
}
"#,
    )
    .unwrap();

    fs::write(
        &main_file,
        r#"public class MainApp {
    public static void main() {
        println("dash-o");
    }
}
"#,
    )
    .unwrap();

    let _ = fs::remove_file(&exe);

    let output = Command::new(cayc_path())
        .arg(&helper)
        .arg(&main_file)
        .arg("-o")
        .arg(&exe)
        .output()
        .expect("failed to run cayc with -o");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "cayc should compile and link with -o:\n{}",
        stderr
    );
    assert!(exe.exists(), "output from -o should be created");

    let run_output = Command::new(&exe)
        .output()
        .expect("failed to run executable from -o");
    let stdout = String::from_utf8_lossy(&run_output.stdout);
    assert!(
        stdout.contains("dash-o"),
        "executable should print 'dash-o', got: {}",
        stdout
    );

    // 清理
    let _ = fs::remove_file(&exe);
    let _ = fs::remove_file(&tmp.join("helper.obj"));
    let _ = fs::remove_file(&tmp.join("main.obj"));
    let _ = fs::remove_file(&tmp.join("helper.ll"));
    let _ = fs::remove_file(&tmp.join("main.ll"));
    let _ = fs::remove_dir_all(&tmp);
}
