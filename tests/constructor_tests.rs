// 构造函数委托测试集
// Phase 3.2: 验证 this() 和 super() 构造函数委托

use common::compile_and_run_eol;

mod common;

/// 归一化行尾
fn normalize(output: &str) -> String {
    output.replace("\r\n", "\n").trim().to_string()
}

// === this() 委托：无参到有参 ===

#[test]
fn test_this_delegation_no_arg_to_arg() {
    let code = r#"
class Point {
    int x;
    int y;
    
    public Point() {
        this(0, 0);
    }
    
    public Point(int x, int y) {
        this.x = x;
        this.y = y;
    }
    
    public void print() {
        println("(" + String.valueOf(this.x) + ", " + String.valueOf(this.y) + ")");
    }
}

public class Test {
    public static void main() {
        Point p1 = new Point();
        p1.print();
        
        Point p2 = new Point(3, 4);
        p2.print();
    }
}
"#;
    std::fs::write("examples/test_ctor_this1.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_ctor_this1.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "(0, 0)\n(3, 4)");
    let _ = std::fs::remove_file("examples/test_ctor_this1.cay");
}

// === this() 委托：带默认值 ===

#[test]
fn test_this_delegation_default_values() {
    let code = r#"
class Config {
    String name;
    int value;
    
    public Config() {
        this("default", 42);
    }
    
    public Config(String name) {
        this(name, 0);
    }
    
    public Config(String name, int value) {
        this.name = name;
        this.value = value;
    }
    
    public void print() {
        println(this.name + "=" + String.valueOf(this.value));
    }
}

public class Test {
    public static void main() {
        new Config().print();
        new Config("custom").print();
        new Config("special", 100).print();
    }
}
"#;
    std::fs::write("examples/test_ctor_this2.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_ctor_this2.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "default=42\ncustom=0\nspecial=100");
    let _ = std::fs::remove_file("examples/test_ctor_this2.cay");
}

// === super() 委托 ===

#[test]
fn test_super_delegation() {
    let code = r#"
class Base {
    String type;
    
    public Base() {
        this.type = "base";
    }
    
    public Base(String type) {
        this.type = type;
    }
    
    public void printType() {
        println("Type: " + this.type);
    }
}

class Child extends Base {
    public Child() {
        super("child");
    }
    
    public Child(String t) {
        super(t);
    }
}

public class Test {
    public static void main() {
        Child c1 = new Child();
        c1.printType();
        
        Child c2 = new Child("custom");
        c2.printType();
    }
}
"#;
    std::fs::write("examples/test_ctor_super.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_ctor_super.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Type: child\nType: custom");
    let _ = std::fs::remove_file("examples/test_ctor_super.cay");
}

// === 构造函数链：多级委托 ===

#[test]
fn test_constructor_chain() {
    let code = r#"
class Animal {
    String species;
    int legs;
    
    public Animal() {
        this("unknown", 4);
    }
    
    public Animal(String species) {
        this(species, 4);
    }
    
    public Animal(String species, int legs) {
        this.species = species;
        this.legs = legs;
    }
    
    public void describe() {
        println(this.species + " (" + String.valueOf(this.legs) + " legs)");
    }
}

public class Test {
    public static void main() {
        new Animal().describe();
        new Animal("dog").describe();
        new Animal("snake", 0).describe();
    }
}
"#;
    std::fs::write("examples/test_ctor_chain.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_ctor_chain.cay").expect("编译运行失败");
    assert_eq!(
        normalize(&output),
        "unknown (4 legs)\ndog (4 legs)\nsnake (0 legs)"
    );
    let _ = std::fs::remove_file("examples/test_ctor_chain.cay");
}

// === 构造函数与字段初始化 ===

#[test]
fn test_constructor_field_init() {
    let code = r#"
class Rectangle {
    int width;
    int height;
    
    public Rectangle(int w, int h) {
        this.width = w;
        this.height = h;
    }
    
    public int area() {
        return this.width * this.height;
    }
}

public class Test {
    public static void main() {
        Rectangle r = new Rectangle(5, 3);
        println("Area: " + String.valueOf(r.area()));
    }
}
"#;
    std::fs::write("examples/test_ctor_field.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_ctor_field.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Area: 15");
    let _ = std::fs::remove_file("examples/test_ctor_field.cay");
}
