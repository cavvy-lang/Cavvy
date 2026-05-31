// 访问控制测试集
// Phase 4: 验证 private/protected 访问控制

use common::{compile_and_run_eol, compile_eol_expect_error};

mod common;

/// 归一化行尾：去除 \r 以便跨平台比较
fn normalize(output: &str) -> String {
    output.replace("\r\n", "\n").trim().to_string()
}

// === public 访问（默认） ===

#[test]
fn test_public_field_access() {
    let code = r#"
public class Test {
    public int value;
    
    public Test(int v) {
        this.value = v;
    }
    
    public static void main() {
        Test t = new Test(42);
        println(String.valueOf(t.value));
    }
}
"#;
    std::fs::write("examples/test_access_public.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_access_public.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "42");
    let _ = std::fs::remove_file("examples/test_access_public.cay");
}

// === private 访问（同类内可访问） ===

#[test]
fn test_private_field_same_class() {
    let code = r#"
public class Test {
    private int secret;
    
    public Test(int s) {
        this.secret = s;
    }
    
    public int getSecret() {
        return this.secret;
    }
    
    public static void main() {
        Test t = new Test(100);
        println(String.valueOf(t.getSecret()));
    }
}
"#;
    std::fs::write("examples/test_access_private_same.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_access_private_same.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "100");
    let _ = std::fs::remove_file("examples/test_access_private_same.cay");
}

// === private 访问（不同类不可访问） ===

#[test]
fn test_private_field_different_class() {
    let code = r#"
public class MyClass {
    private int secret = 42;
}

public class Test {
    public static void main() {
        MyClass obj = new MyClass();
        println(String.valueOf(obj.secret));
    }
}
"#;
    std::fs::write("examples/test_access_private_diff.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_access_private_diff.cay")
        .expect("应该编译失败");
    assert!(error.contains("private") || error.contains("access"),
        "应报告 private 访问错误: {}", error);
    let _ = std::fs::remove_file("examples/test_access_private_diff.cay");
}

// === protected 访问（子类可访问） ===

#[test]
fn test_protected_field_subclass() {
    let code = r#"
public class Parent {
    protected int value;
    
    public Parent(int v) {
        this.value = v;
    }
}

public class Child extends Parent {
    public Child(int v) {
        super(v);
    }
    
    public int getValue() {
        return this.value;
    }
    
    public static void main() {
        Child c = new Child(55);
        println(String.valueOf(c.getValue()));
    }
}
"#;
    std::fs::write("examples/test_access_protected_sub.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_access_protected_sub.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "55");
    let _ = std::fs::remove_file("examples/test_access_protected_sub.cay");
}

// === protected 访问（非子类不可访问） ===

#[test]
fn test_protected_field_non_subclass() {
    let code = r#"
public class Parent {
    protected int value = 10;
}

public class Test {
    public static void main() {
        Parent p = new Parent();
        println(String.valueOf(p.value));
    }
}
"#;
    std::fs::write("examples/test_access_protected_non.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_access_protected_non.cay")
        .expect("应该编译失败");
    assert!(error.contains("protected") || error.contains("access"),
        "应报告 protected 访问错误: {}", error);
    let _ = std::fs::remove_file("examples/test_access_protected_non.cay");
}

// === private 方法访问 ===

#[test]
fn test_private_method_different_class() {
    let code = r#"
public class MyClass {
    private void secretMethod() {
        println("secret");
    }
    
    public void callSecret() {
        this.secretMethod();
    }
}

public class Test {
    public static void main() {
        MyClass obj = new MyClass();
        obj.secretMethod();
    }
}
"#;
    std::fs::write("examples/test_access_private_method.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_access_private_method.cay")
        .expect("应该编译失败");
    assert!(error.contains("private") || error.contains("access"),
        "应报告 private 方法访问错误: {}", error);
    let _ = std::fs::remove_file("examples/test_access_private_method.cay");
}

// === protected 方法访问（子类可访问） ===

#[test]
fn test_protected_method_subclass() {
    let code = r#"
public class Animal {
    protected void speak() {
        println("speaking");
    }
}

public class Dog extends Animal {
    public void bark() {
        this.speak();
    }
    
    public static void main() {
        Dog d = new Dog();
        d.bark();
    }
}
"#;
    std::fs::write("examples/test_access_protected_method.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_access_protected_method.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "speaking");
    let _ = std::fs::remove_file("examples/test_access_protected_method.cay");
}

// === 静态成员访问控制 ===

#[test]
fn test_static_private_field() {
    let code = r#"
public class Config {
    private static int maxRetries = 3;
}

public class Test {
    public static void main() {
        println(String.valueOf(Config.maxRetries));
    }
}
"#;
    std::fs::write("examples/test_access_static_private.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_access_static_private.cay")
        .expect("应该编译失败");
    assert!(error.contains("private") || error.contains("access"),
        "应报告 private 静态字段访问错误: {}", error);
    let _ = std::fs::remove_file("examples/test_access_static_private.cay");
}

// === 构造函数访问控制 ===

#[test]
fn test_private_constructor() {
    let code = r#"
public class Singleton {
    private static Singleton instance;
    
    private Singleton() {
    }
    
    public static Singleton getInstance() {
        if (Singleton.instance == null) {
            Singleton.instance = new Singleton();
        }
        return Singleton.instance;
    }
    
    public static void main() {
        Singleton s = Singleton.getInstance();
        println("ok");
    }
}
"#;
    std::fs::write("examples/test_access_private_ctor.cay", code).unwrap();
    // 私有构造函数在类内部可以调用
    let output = compile_and_run_eol("examples/test_access_private_ctor.cay")
        .expect("编译运行失败");
    assert_eq!(normalize(&output), "ok");
    let _ = std::fs::remove_file("examples/test_access_private_ctor.cay");
}
