// 错误诊断测试集
// Phase 3.4: 验证编译器正确拒绝无效代码并提供清晰的错误信息

use common::compile_eol_expect_error;

mod common;

// === 类型错误 ===

#[test]
fn test_error_type_mismatch_return() {
    let code = r#"
public class Test {
    public static int getValue() {
        return "hello";
    }
    
    public static void main() {
        println(getValue());
    }
}
"#;
    std::fs::write("examples/test_err_diag1.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag1.cay").expect("应该编译失败");
    assert!(
        error.contains("type") || error.contains("Type") || error.contains("return"),
        "应报告类型错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag1.cay");
}

#[test]
fn test_error_type_mismatch_operator() {
    let code = r#"
public class Test {
    public static void main() {
        boolean result = "hello" + 42;
    }
}
"#;
    std::fs::write("examples/test_err_diag2.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag2.cay").expect("应该编译失败");
    assert!(
        error.contains("Cannot add") || error.contains("type") || error.contains("string"),
        "应报告类型错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag2.cay");
}

#[test]
fn test_error_type_mismatch_comparison() {
    let code = r#"
public class Test {
    public static void main() {
        boolean result = "hello" > 42;
    }
}
"#;
    std::fs::write("examples/test_err_diag3.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag3.cay").expect("应该编译失败");
    assert!(
        error.contains("Cannot compare") || error.contains("type") || error.contains("String"),
        "应报告类型错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag3.cay");
}

// === 未定义变量 ===

#[test]
fn test_error_undefined_variable() {
    let code = r#"
public class Test {
    public static void main() {
        println(undefinedVar);
    }
}
"#;
    std::fs::write("examples/test_err_diag4.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag4.cay").expect("应该编译失败");
    assert!(
        error.contains("undefined") || error.contains("not found") || error.contains("Unknown"),
        "应报告未定义变量: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag4.cay");
}

// === 重复定义 ===

#[test]
fn test_error_duplicate_variable() {
    let code = r#"
public class Test {
    public static void main() {
        int x = 5;
        int x = 10;
    }
}
"#;
    std::fs::write("examples/test_err_diag5.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag5.cay").expect("应该编译失败");
    assert!(
        error.contains("already defined")
            || error.contains("redefined")
            || error.contains("Duplicate"),
        "应报告重复定义: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag5.cay");
}

// === 方法调用错误 ===

#[test]
fn test_error_wrong_arg_count() {
    let code = r#"
public class Test {
    public static void add(int a, int b) {
        println(a + b);
    }
    
    public static void main() {
        add(1, 2, 3);
    }
}
"#;
    std::fs::write("examples/test_err_diag6.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag6.cay").expect("应该编译失败");
    assert!(
        error.contains("argument") || error.contains("parameter") || error.contains("count"),
        "应报告参数数量错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag6.cay");
}

// === 数组错误 ===

#[test]
fn test_error_array_index_type() {
    let code = r#"
public class Test {
    public static void main() {
        int[] arr = new int[5];
        arr["hello"] = 10;
    }
}
"#;
    std::fs::write("examples/test_err_diag7.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag7.cay").expect("应该编译失败");
    assert!(
        error.contains("index") || error.contains("type") || error.contains("integer"),
        "应报告数组索引类型错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag7.cay");
}

// === 控制流错误 ===

#[test]
fn test_error_break_outside_loop() {
    let code = r#"
public class Test {
    public static void main() {
        break;
    }
}
"#;
    std::fs::write("examples/test_err_diag8.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag8.cay").expect("应该编译失败");
    assert!(
        error.contains("break") || error.contains("loop") || error.contains("outside"),
        "应报告 break 在循环外错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag8.cay");
}

#[test]
fn test_error_continue_outside_loop() {
    let code = r#"
public class Test {
    public static void main() {
        continue;
    }
}
"#;
    std::fs::write("examples/test_err_diag9.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag9.cay").expect("应该编译失败");
    assert!(
        error.contains("continue") || error.contains("loop") || error.contains("outside"),
        "应报告 continue 在循环外错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag9.cay");
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
    std::fs::write("examples/test_err_diag10.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag10.cay").expect("应该编译失败");
    assert!(
        error.contains("expected") || error.contains("syntax") || error.contains(";"),
        "应报告语法错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag10.cay");
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
    std::fs::write("examples/test_err_diag11.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag11.cay").expect("应该编译失败");
    assert!(
        error.contains("expected") || error.contains("EOF") || error.contains("}"),
        "应报告缺少大括号: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag11.cay");
}

// === 类/接口错误 ===

#[test]
fn test_error_missing_method_implementation() {
    let code = r#"
interface Greetable {
    void greet();
}

class Person implements Greetable {
    public Person() {
    }
}

public class Test {
    public static void main() {
        Person p = new Person();
    }
}
"#;
    std::fs::write("examples/test_err_diag12.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag12.cay").expect("应该编译失败");
    assert!(
        error.contains("implement") || error.contains("method") || error.contains("abstract"),
        "应报告未实现接口方法: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag12.cay");
}

// === 除零错误（编译时） ===

#[test]
fn test_error_division_by_zero_compile_time() {
    let code = r#"
public class Test {
    public static void main() {
        int x = 10 / 0;
    }
}
"#;
    std::fs::write("examples/test_err_diag13.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag13.cay").expect("应该编译失败");
    assert!(
        error.contains("division") || error.contains("zero") || error.contains("divide"),
        "应报告除零错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag13.cay");
}

// === 访问控制错误 ===

#[test]
fn test_error_private_access() {
    let code = r#"
public class MyClass {
    private int secret = 42;
}

public class Test {
    public static void main() {
        MyClass obj = new MyClass();
        println(obj.secret);
    }
}
"#;
    std::fs::write("examples/test_err_diag14.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag14.cay").expect("应该编译失败");
    assert!(
        error.contains("private") || error.contains("access") || error.contains("Cannot access"),
        "应报告访问控制错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag14.cay");
}

// === 数组长度错误 ===

#[test]
fn test_error_array_negative_length() {
    let code = r#"
public class Test {
    public static void main() {
        int[] arr = new int[-5];
    }
}
"#;
    std::fs::write("examples/test_err_diag15.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_err_diag15.cay").expect("应该编译失败");
    assert!(
        error.contains("length") || error.contains("negative") || error.contains("size"),
        "应报告数组长度错误: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_err_diag15.cay");
}
