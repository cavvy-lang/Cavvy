// 命名参数测试集
// Phase 3.3: 验证命名参数功能

use common::compile_and_run_eol;

mod common;

/// 归一化行尾：去除 \r 以便跨平台比较
fn normalize(output: &str) -> String {
    output.replace("\r\n", "\n").trim().to_string()
}

// === 基础命名参数 ===

#[test]
fn test_named_basic() {
    let code = r#"
public class Test {
    public static void greet(String name, int age) {
        println(name + " is " + String.valueOf(age) + " years old");
    }
    
    public static void main() {
        greet(name="Alice", age=30);
    }
}
"#;
    std::fs::write("examples/test_named_basic.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_named_basic.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Alice is 30 years old");
    let _ = std::fs::remove_file("examples/test_named_basic.cay");
}

// === 命名参数不同顺序 ===

#[test]
fn test_named_different_order() {
    let code = r#"
public class Test {
    public static void printInfo(String name, int age, String city) {
        println(name + ", " + String.valueOf(age) + ", " + city);
    }
    
    public static void main() {
        printInfo(age=25, name="Bob", city="NYC");
    }
}
"#;
    std::fs::write("examples/test_named_order.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_named_order.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Bob, 25, NYC");
    let _ = std::fs::remove_file("examples/test_named_order.cay");
}

// === 混合位置和命名参数 ===

#[test]
fn test_named_mixed() {
    let code = r#"
public class Test {
    public static void format(String prefix, String item, String suffix) {
        println(prefix + item + suffix);
    }
    
    public static void main() {
        format("Item: ", item="Widget", suffix="!");
    }
}
"#;
    std::fs::write("examples/test_named_mixed.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_named_mixed.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Item: Widget!");
    let _ = std::fs::remove_file("examples/test_named_mixed.cay");
}

// === 命名参数与可变参数 ===

#[test]
fn test_named_with_varargs() {
    let code = r#"
public class Test {
    public static void log(String... messages, String level) {
        for (int i = 0; i < messages.length; i++) {
            println("[" + level + "] " + messages[i]);
        }
    }
    
    public static void main() {
        log("error1", "error2", level="ERROR");
    }
}
"#;
    std::fs::write("examples/test_named_varargs.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_named_varargs.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "[ERROR] error1\n[ERROR] error2");
    let _ = std::fs::remove_file("examples/test_named_varargs.cay");
}

// === 命名参数在类方法中 ===

#[test]
fn test_named_in_class_method() {
    let code = r#"
public class Calculator {
    public static int compute(int a, int b, String op) {
        if (op == "add") {
            return a + b;
        } else if (op == "sub") {
            return a - b;
        } else {
            return 0;
        }
    }
}

public class Test {
    public static void main() {
        int result1 = Calculator.compute(a=10, b=5, op="add");
        int result2 = Calculator.compute(b=3, a=20, op="sub");
        println(String.valueOf(result1));
        println(String.valueOf(result2));
    }
}
"#;
    std::fs::write("examples/test_named_class.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_named_class.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "15\n17");
    let _ = std::fs::remove_file("examples/test_named_class.cay");
}

// === 命名参数与不同类型的参数 ===

#[test]
fn test_named_different_types() {
    let code = r#"
public class Test {
    public static void display(String text, int count, boolean flag, double value) {
        println(text);
        println(String.valueOf(count));
        if (flag) {
            println("true");
        } else {
            println("false");
        }
    }
    
    public static void main() {
        display(text="Hello", count=42, flag=true, value=3.14);
    }
}
"#;
    std::fs::write("examples/test_named_types.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_named_types.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Hello\n42\ntrue");
    let _ = std::fs::remove_file("examples/test_named_types.cay");
}
