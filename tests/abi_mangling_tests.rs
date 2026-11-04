//! 顶层函数必须使用 Itanium C++ ABI 名称。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn cayc_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_cayc") {
        return PathBuf::from(path);
    }
    PathBuf::from(if cfg!(windows) {
        "target/release/cayc.exe"
    } else {
        "target/release/cayc"
    })
}

#[test]
fn top_level_function_uses_itanium_abi() {
    let dir = PathBuf::from("target/tmp/abi_mangling");
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join("main.cay");
    fs::copy("examples/test_fn_auto_return.cay", &source).unwrap();
    let output = dir.join(if cfg!(windows) { "main.exe" } else { "main" });
    let _ = fs::remove_file(&output);
    let _ = fs::remove_file(dir.join("main.ll"));

    let result = Command::new(cayc_path())
        .arg(&source)
        .arg("-o")
        .arg(&output)
        .arg("--keep-ir")
        .arg("-F=top_level_function")
        .output()
        .expect("failed to run cayc");
    assert!(
        result.status.success(),
        "top-level function compile failed: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let ir_path: PathBuf = dir.join("main.ll");
    let ir = fs::read_to_string(&ir_path).expect("missing generated IR");
    assert!(
        ir.contains("@_Z3addii"),
        "top-level function definition is not Itanium-mangled:\n{ir}"
    );
    assert!(
        ir.contains("call i64 @_Z3addii"),
        "top-level function call is not Itanium-mangled:\n{ir}"
    );
    assert!(
        !ir.contains("__toplevel_"),
        "legacy __toplevel_ name leaked into IR:\n{ir}"
    );

    let _ = fs::remove_file(output);
    let _ = fs::remove_file(ir_path);
    let _ = fs::remove_file(dir.join("main.obj"));
    let _ = fs::remove_dir_all(Path::new(&dir));
}
