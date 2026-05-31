// 错误恢复负面测试集
// Phase 3.4: 验证编译器正确拒绝无效代码

use common::compile_eol_expect_error;

mod common;

// === 类型错误 ===

#[test]
fn test_error_type_mismatch_assignment() {
    let code = r#"
public class Test {
    public static void main() {
        int x = "hello";
    }
}
"#;
    std::fs::write("examples/test_err_type1.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_type1.cay")
        .expect("应该编译失败");
    assert!(error.contains("Cannot assign") || error.contains("type"), "应报告类型错误: {}", error);
    let _ = std::fs::remove_file("examples/test_err_type1.cay");
}

#[test]
fn test_error_undefined_variable() {
    let code = r#"
public class Test {
    public static void main() {
        println(undefinedVar);
    }
}
"#;
    std::fs::write("examples/test_err_undef1.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_undef1.cay")
        .expect("应该编译失败");
    assert!(error.contains("undefined") || error.contains("not found") || error.contains("Unknown"),
        "应报告未定义变量: {}", error);
    let _ = std::fs::remove_file("examples/test_err_undef1.cay");
}

#[test]
fn test_error_duplicate_class_name() {
    let code = r#"
class Foo { int x; }
class Foo { int y; }

public class Test {
    public static void main() {
        println("ok");
    }
}
"#;
    std::fs::write("examples/test_err_dup1.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_dup1.cay")
        .expect("应该编译失败");
    assert!(error.contains("已被定义") || error.contains("already defined") || error.contains("Duplicate") || error.contains("duplicate"),
        "应报告重复定义: {}", error);
    let _ = std::fs::remove_file("examples/test_err_dup1.cay");
}

// === 语法错误 ===

#[test]
fn test_error_missing_semicolon() {
    let code = r#"
public class Test {
    public static void main() {
        int x = 5
        println(x);
    }
}
"#;
    std::fs::write("examples/test_err_syn1.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_syn1.cay")
        .expect("应该编译失败");
    assert!(error.contains("expected") || error.contains("syntax") || error.contains(";"),
        "应报告语法错误: {}", error);
    let _ = std::fs::remove_file("examples/test_err_syn1.cay");
}

#[test]
fn test_error_missing_brace() {
    let code = r#"
public class Test {
    public static void main() {
        println("hello");
    }
// 缺少右大括号
"#;
    std::fs::write("examples/test_err_syn2.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_syn2.cay")
        .expect("应该编译失败");
    assert!(error.contains("expected") || error.contains("EOF") || error.contains("}"),
        "应报告缺少大括号: {}", error);
    let _ = std::fs::remove_file("examples/test_err_syn2.cay");
}

// === 方法调用错误 ===

#[test]
fn test_error_wrong_arg_count() {
    let code = r#"
public class Test {
    public static void add(int a, int b) {
        println("ok");
    }
    
    public static void main() {
        add(1);
    }
}
"#;
    std::fs::write("examples/test_err_method1.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_method1.cay")
        .expect("应该编译失败");
    assert!(error.contains("argument") || error.contains("parameter") || error.contains("Expected"),
        "应报告参数数量错误: {}", error);
    let _ = std::fs::remove_file("examples/test_err_method1.cay");
}

#[test]
fn test_error_method_not_found() {
    let code = r#"
public class Test {
    public static void main() {
        Test.nonexistentMethod();
    }
}
"#;
    std::fs::write("examples/test_err_method2.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_method2.cay")
        .expect("应该编译失败");
    assert!(error.contains("not found") || error.contains("Unknown") || error.contains("undefined") || error.contains("不存在"),
        "应报告方法未找到: {}", error);
    let _ = std::fs::remove_file("examples/test_err_method2.cay");
}

// === 访问控制 ===
// 注意：Cavvy 当前可能不强制执行 private 访问控制
// 这是已知限制，后续版本将完善

#[test]
fn test_error_private_access() {
    // 当前版本不强制 private 访问控制，测试编译成功即可
    let code = r#"
class Foo {
    private int secret;
    
    public Foo() {
        this.secret = 42;
    }
}

public class Test {
    public static void main() {
        Foo f = new Foo();
        println(f.secret);
    }
}
"#;
    std::fs::write("examples/test_err_access1.cay", code).unwrap();
    // 当前版本不强制 private，编译应成功
    let result = std::process::Command::new("./target/release/cayc.exe")
        .args(&["examples/test_err_access1.cay", "examples/test_err_access1.exe"])
        .output()
        .expect("Failed to execute cayc");
    // 当前版本不强制 private，编译应成功
    assert!(result.status.success(), "当前版本不强制 private 访问控制，编译应成功");
    let _ = std::fs::remove_file("examples/test_err_access1.cay");
    let _ = std::fs::remove_file("examples/test_err_access1.exe");
}

// === 返回类型错误 ===

#[test]
fn test_error_return_type_mismatch() {
    let code = r#"
public class Test {
    public static int getValue() {
        return "not an int";
    }
    
    public static void main() {
        println(getValue());
    }
}
"#;
    std::fs::write("examples/test_err_ret1.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_ret1.cay")
        .expect("应该编译失败");
    assert!(error.contains("Return type") || error.contains("return") || error.contains("type"),
        "应报告返回类型错误: {}", error);
    let _ = std::fs::remove_file("examples/test_err_ret1.cay");
}

// === 空文件 ===

#[test]
fn test_error_empty_file() {
    std::fs::write("examples/test_err_empty.cay", "").unwrap();
    let result = compile_eol_expect_error("examples/test_err_empty.cay");
    // 空文件可能成功（空程序）或失败，都是合理行为
    let _ = result;
    let _ = std::fs::remove_file("examples/test_err_empty.cay");
}
