// Lambda 表达式测试集
// Phase 3.2: 验证 Lambda 表达式功能

use common::compile_and_run_eol;

mod common;

/// 归一化行尾：去除 \r 以便跨平台比较
fn normalize(output: &str) -> String {
    output.replace("\r\n", "\n").trim().to_string()
}

// === 基础 Lambda 表达式 ===

#[test]
fn test_lambda_basic_no_params() {
    let code = r#"
public class Test {
    public static void main() {
        var greet = () -> { println("Hello from lambda"); };
        greet();
    }
}
"#;
    std::fs::write("examples/test_lambda_basic.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_lambda_basic.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "Hello from lambda");
    let _ = std::fs::remove_file("examples/test_lambda_basic.cay");
}

#[test]
fn test_lambda_with_params() {
    let code = r#"
public class Test {
    public static void main() {
        var add = (int a, int b) -> { return a + b; };
        println(add(3, 4));
    }
}
"#;
    std::fs::write("examples/test_lambda_params.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_lambda_params.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "7");
    let _ = std::fs::remove_file("examples/test_lambda_params.cay");
}

#[test]
fn test_lambda_expression_body() {
    let code = r#"
public class Test {
    public static void main() {
        var double = (int x) -> x * 2;
        println(double(5));
    }
}
"#;
    std::fs::write("examples/test_lambda_expr.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_lambda_expr.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "10");
    let _ = std::fs::remove_file("examples/test_lambda_expr.cay");
}

// === Lambda 作为参数 ===

#[test]
fn test_lambda_as_parameter() {
    let code = r#"
public class Test {
    public static void apply(int x, int f(int n)) {
        println(f(x));
    }
    
    public static void main() {
        apply(10, (int n) -> n * n);
    }
}
"#;
    std::fs::write("examples/test_lambda_param2.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_lambda_param2.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "100");
    let _ = std::fs::remove_file("examples/test_lambda_param2.cay");
}

// === Lambda 返回值 ===

#[test]
fn test_lambda_return_value() {
    let code = r#"
public class Test {
    public static int make_adder(int x) {
        return (int y) -> x + y;
    }
    
    public static void main() {
        var add5 = make_adder(5);
        println(add5(10));
    }
}
"#;
    std::fs::write("examples/test_lambda_return.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_lambda_return.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "15");
    let _ = std::fs::remove_file("examples/test_lambda_return.cay");
}

// === Lambda 在循环中 ===

#[test]
fn test_lambda_in_loop() {
    let code = r#"
public class Test {
    public static void main() {
        for (int i = 0; i < 3; i++) {
            var lambda = (int x) -> x + i;
            println(lambda(100));
        }
    }
}
"#;
    std::fs::write("examples/test_lambda_loop.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_lambda_loop.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "100\n101\n102");
    let _ = std::fs::remove_file("examples/test_lambda_loop.cay");
}

// === Lambda 多行体 ===

#[test]
fn test_lambda_multiline_body() {
    let code = r#"
public class Test {
    public static void main() {
        var process = (int a, int b) -> {
            int sum = a + b;
            int product = a * b;
            println("Sum: " + String.valueOf(sum));
            println("Product: " + String.valueOf(product));
        };
        process(3, 4);
    }
}
"#;
    std::fs::write("examples/test_lambda_multi.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_lambda_multi.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "Sum: 7\nProduct: 12");
    let _ = std::fs::remove_file("examples/test_lambda_multi.cay");
}

// === Lambda 嵌套 ===

#[test]
fn test_lambda_nested() {
    let code = r#"
public class Test {
    public static void main() {
        var outer = (int x) -> {
            var inner = (int y) -> x + y;
            return inner(10);
        };
        println(outer(5));
    }
}
"#;
    std::fs::write("examples/test_lambda_nested.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_lambda_nested.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "15");
    let _ = std::fs::remove_file("examples/test_lambda_nested.cay");
}

// === Lambda 与字符串操作 ===

#[test]
fn test_lambda_string_operation() {
    let code = r#"
public class Test {
    public static void main() {
        var toUpper = (String s) -> {
            // 简单字符串操作
            return "Result: " + s;
        };
        println(toUpper("hello"));
    }
}
"#;
    std::fs::write("examples/test_lambda_string.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_lambda_string.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "Result: hello");
    let _ = std::fs::remove_file("examples/test_lambda_string.cay");
}
