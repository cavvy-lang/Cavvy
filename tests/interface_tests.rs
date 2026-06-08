// 接口方法调用测试集
// Phase 3.1: 验证 B1/B2 修复后的接口功能

use common::compile_and_run_eol;

mod common;

/// 归一化行尾：去除 \r 以便跨平台比较
fn normalize(output: &str) -> String {
    output.replace("\r\n", "\n").trim().to_string()
}

// === 基础接口方法调用 ===

#[test]
fn test_interface_basic_method_call() {
    let code = r#"
interface Greetable {
    void greet();
}

class Person implements Greetable {
    String name;
    
    public Person(String name) {
        this.name = name;
    }
    
    public void greet() {
        println("Hello, I am " + this.name);
    }
}

public class Test {
    public static void main() {
        Greetable g = new Person("Alice");
        g.greet();
    }
}
"#;
    std::fs::write("examples/test_interface_basic.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_interface_basic.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Hello, I am Alice");
    let _ = std::fs::remove_file("examples/test_interface_basic.cay");
}

// === 接口方法返回值 ===

#[test]
fn test_interface_method_return_value() {
    let code = r#"
interface Describable {
    String describe();
}

class Item implements Describable {
    String itemName;
    
    public Item(String name) {
        this.itemName = name;
    }
    
    public String describe() {
        return "Item: " + this.itemName;
    }
}

public class Test {
    public static void main() {
        Describable d = new Item("Widget");
        println(d.describe());
    }
}
"#;
    std::fs::write("examples/test_interface_return.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_interface_return.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Item: Widget");
    let _ = std::fs::remove_file("examples/test_interface_return.cay");
}

// === 多接口实现 ===

#[test]
fn test_interface_multiple_interfaces() {
    let code = r#"
interface Printable {
    void print();
}

interface Drawable {
    void draw();
}

class MultiWidget implements Printable, Drawable {
    public void print() {
        println("print");
    }
    
    public void draw() {
        println("draw");
    }
}

public class Test {
    public static void main() {
        Printable p = new MultiWidget();
        p.print();
        Drawable d = new MultiWidget();
        d.draw();
    }
}
"#;
    std::fs::write("examples/test_interface_multi.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_interface_multi.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "print\ndraw");
    let _ = std::fs::remove_file("examples/test_interface_multi.cay");
}

// === 接口作为参数 ===
// 当前接口方法调用覆盖单实现和参数传递场景。

#[test]
fn test_interface_as_parameter() {
    let code = r#"
interface Greetable {
    void greet();
}

class Person implements Greetable {
    String name;
    
    public Person(String name) {
        this.name = name;
    }
    
    public void greet() {
        println("Hi, " + this.name);
    }
}

class Greeter {
    public static void greetPerson(Greetable g) {
        g.greet();
    }
}

public class Test {
    public static void main() {
        Greeter.greetPerson(new Person("Alice"));
        Greeter.greetPerson(new Person("Bob"));
    }
}
"#;
    std::fs::write("examples/test_interface_param.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_interface_param.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Hi, Alice\nHi, Bob");
    let _ = std::fs::remove_file("examples/test_interface_param.cay");
}

// === 接口类型赋值兼容性与运行时动态分发 ===

#[test]
fn test_interface_assignment_compatibility() {
    let code = r#"
interface Animal {
    void speak();
}

class Dog implements Animal {
    public void speak() {
        println("Woof");
    }
}

class Cat implements Animal {
    public void speak() {
        println("Meow");
    }
}

public class Test {
    public static void main() {
        Animal a1 = new Dog();
        a1.speak();
        Animal a2 = new Cat();
        a2.speak();
    }
}
"#;
    std::fs::write("examples/test_interface_assign.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_interface_assign.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Woof\nMeow");
    let _ = std::fs::remove_file("examples/test_interface_assign.cay");
}

#[test]
fn test_interface_dispatch_uses_runtime_type_with_different_class_slots() {
    let code = r#"
interface Animal {
    void speak();
}

class Dog implements Animal {
    public void bark() {
        println("Dog.bark");
    }

    public void speak() {
        println("Dog.speak");
    }
}

class Cat implements Animal {
    public void speak() {
        println("Cat.speak");
    }

    public void zoom() {
        println("Cat.zoom");
    }
}

public class Test {
    public static void main() {
        Animal a1 = new Dog();
        a1.speak();
        Animal a2 = new Cat();
        a2.speak();
    }
}
"#;
    std::fs::write("examples/test_interface_dispatch_slots.cay", code).unwrap();
    let output =
        compile_and_run_eol("examples/test_interface_dispatch_slots.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Dog.speak\nCat.speak");
    let _ = std::fs::remove_file("examples/test_interface_dispatch_slots.cay");
}

#[test]
fn test_interface_dispatch_with_args_and_return_uses_runtime_type() {
    let code = r#"
interface Scorer {
    int score(int base);
}

class Doubler implements Scorer {
    public int score(int base) {
        return base * 2;
    }

    public void later() {
        println("unused");
    }
}

class Tripler implements Scorer {
    public void earlier() {
        println("unused");
    }

    public int score(int base) {
        return base * 3;
    }
}

public class Test {
    public static void main() {
        Scorer s1 = new Doubler();
        println(s1.score(7));
        Scorer s2 = new Tripler();
        println(s2.score(7));
    }
}
"#;
    std::fs::write("examples/test_interface_dispatch_return.cay", code).unwrap();
    let output =
        compile_and_run_eol("examples/test_interface_dispatch_return.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "14\n21");
    let _ = std::fs::remove_file("examples/test_interface_dispatch_return.cay");
}

// === 接口方法带参数 ===
// 测试参数传递。

#[test]
fn test_interface_method_with_args() {
    let code = r#"
interface Calculable {
    int calculate(int a, int b);
}

class Adder implements Calculable {
    public int calculate(int a, int b) {
        return a + b;
    }
}

public class Test {
    public static void main() {
        Calculable calc = new Adder();
        println(calc.calculate(3, 4));
        println(calc.calculate(10, 20));
    }
}
"#;
    std::fs::write("examples/test_interface_args.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_interface_args.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "7\n30");
    let _ = std::fs::remove_file("examples/test_interface_args.cay");
}

// === 接口与多方法类 ===

#[test]
fn test_interface_multiple_methods_on_class() {
    let code = r#"
interface Operations {
    int add(int a, int b);
    int subtract(int a, int b);
}

class MathOps implements Operations {
    public int add(int a, int b) {
        return a + b;
    }
    
    public int subtract(int a, int b) {
        return a - b;
    }
}

public class Test {
    public static void main() {
        Operations ops = new MathOps();
        println(ops.add(10, 5));
        println(ops.subtract(10, 5));
    }
}
"#;
    std::fs::write("examples/test_interface_inherit.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_interface_inherit.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "15\n5");
    let _ = std::fs::remove_file("examples/test_interface_inherit.cay");
}

// === 接口与继承组合 ===

#[test]
fn test_interface_with_class_inheritance() {
    let code = r#"
interface Flyable {
    void fly();
}

class Animal {
    public String name;
    
    public Animal(String name) {
        this.name = name;
    }
    
    public void speak() {
        println(this.name + " speaks");
    }
}

class Bird extends Animal implements Flyable {
    public Bird(String name) {
        super(name);
    }
    
    public void fly() {
        println(this.name + " flies");
    }
}

public class Test {
    public static void main() {
        Bird b = new Bird("Eagle");
        b.speak();
        b.fly();
        
        Animal a = b;
        a.speak();
        
        Flyable f = b;
        f.fly();
    }
}
"#;
    std::fs::write("examples/test_interface_class_inherit.cay", code).unwrap();
    let output =
        compile_and_run_eol("examples/test_interface_class_inherit.cay").expect("编译运行失败");
    assert_eq!(
        normalize(&output),
        "Eagle speaks\nEagle flies\nEagle speaks\nEagle flies"
    );
    let _ = std::fs::remove_file("examples/test_interface_class_inherit.cay");
}

// === 空接口 ===

#[test]
fn test_empty_interface() {
    let code = r#"
interface Marker {
}

class Marked implements Marker {
    public String value;
    
    public Marked(String v) {
        this.value = v;
    }
}

public class Test {
    public static void main() {
        Marked m = new Marked("hello");
        println(m.value);
    }
}
"#;
    std::fs::write("examples/test_interface_empty.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_interface_empty.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "hello");
    let _ = std::fs::remove_file("examples/test_interface_empty.cay");
}

// === 接口多方法 + 复杂逻辑 ===

#[test]
fn test_interface_complex_methods() {
    let code = r#"
interface MathOps {
    int add(int a, int b);
    int multiply(int a, int b);
}

class StdMath implements MathOps {
    public int add(int a, int b) {
        return a + b;
    }
    
    public int multiply(int a, int b) {
        return a * b;
    }
}

public class Test {
    public static void main() {
        MathOps math = new StdMath();
        int sum = math.add(10, 20);
        int product = math.multiply(10, 20);
        println("Sum: " + String.valueOf(sum));
        println("Product: " + String.valueOf(product));
    }
}
"#;
    std::fs::write("examples/test_interface_complex.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_interface_complex.cay").expect("编译运行失败");
    assert_eq!(normalize(&output), "Sum: 30\nProduct: 200");
    let _ = std::fs::remove_file("examples/test_interface_complex.cay");
}
