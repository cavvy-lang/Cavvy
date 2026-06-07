//! Cavvy 语言继承和 OOP 功能集成测试
//!
//! 测试继承、抽象类、接口、访问控制、instanceof 等

mod common;
use common::{compile_and_run_eol, compile_eol_expect_error};

// ==================== 0.4.0.x 继承体系测试 ====================

#[test]
fn test_inheritance_basic() {
    let output = compile_and_run_eol("examples/test_inheritance_basic.cay")
        .expect("inheritance basic should compile and run");
    assert!(
        output.contains("Animal speaks") && output.contains("Dog inherits from Animal"),
        "Basic inheritance should work, got: {}",
        output
    );
}

#[test]
fn test_override_annotation() {
    let output = compile_and_run_eol("examples/test_override_annotation.cay")
        .expect("override annotation should compile and run");
    assert!(
        output.contains("Drawing a circle") && output.contains("Area:"),
        "Override annotation should work, got: {}",
        output
    );
}

#[test]
fn test_access_control() {
    let output = compile_and_run_eol("examples/test_access_control.cay")
        .expect("access control should compile and run");
    assert!(
        output.contains("Public method") && output.contains("Protected method"),
        "Access control should work, got: {}",
        output
    );
}

#[test]
fn test_error_inheritance_undefined_parent() {
    let error = compile_eol_expect_error("examples/errors/error_inheritance_undefined_parent.cay")
        .expect("undefined parent class should fail to compile");
    assert!(
        error.contains("extends") || error.contains("undefined") || error.contains("not found"),
        "Should report undefined parent class error, got: {}",
        error
    );
}

#[test]
fn test_error_override_no_parent() {
    let error = compile_eol_expect_error("examples/errors/error_override_no_parent.cay")
        .expect("override without parent should fail to compile");
    assert!(
        error.contains("Override") || error.contains("parent") || error.contains("extend"),
        "Should report override without parent error, got: {}",
        error
    );
}

#[test]
fn test_error_override_not_exist() {
    let error = compile_eol_expect_error("examples/errors/error_override_not_exist.cay")
        .expect("override non-existent method should fail to compile");
    assert!(
        error.contains("Override") || error.contains("override") || error.contains("not exist"),
        "Should report override non-existent method error, got: {}",
        error
    );
}

// ==================== 0.4.1.0 接口、抽象类和 instanceof 测试 ====================

#[test]
fn test_instanceof() {
    let output = compile_and_run_eol("examples/test_instanceof.cay")
        .expect("instanceof should compile and run");
    assert!(
        output.contains("shape instanceof Shape: true"),
        "Should report shape is Shape, got: {}",
        output
    );
    assert!(
        output.contains("rect instanceof Rectangle: true"),
        "Should report rect is Rectangle, got: {}",
        output
    );
    assert!(
        output.contains("rect instanceof Shape: true"),
        "Should report rect is Shape (inheritance), got: {}",
        output
    );
    assert!(
        output.contains("rect instanceof Drawable: true"),
        "Should report rect is Drawable (interface), got: {}",
        output
    );
}

// ==================== 0.4.4.x 静态与 Final 语义测试 ====================

#[test]
fn test_static_fields() {
    let output = compile_and_run_eol("examples/test_static_fields.cay")
        .expect("static fields example should compile and run");
    // 测试静态字段初始值为0
    assert!(
        output.contains("Initial count:") && output.contains("0"),
        "Static fields should be zero-initialized, got: {}",
        output
    );
    assert!(
        output.contains("Initial total:") && output.contains("0"),
        "Static fields should be zero-initialized, got: {}",
        output
    );
    // 测试增量后的值
    assert!(
        output.contains("After 3 increments:"),
        "Should show after increments message, got: {}",
        output
    );
}

#[test]
fn test_error_final_class_inheritance() {
    let error = compile_eol_expect_error("examples/errors/error_final_class_inheritance.cay")
        .expect("final class inheritance should fail to compile");
    assert!(
        error.contains("cannot inherit from final") || error.contains("final class"),
        "Should report final class inheritance error, got: {}",
        error
    );
}

#[test]
fn test_error_final_method_override() {
    let error = compile_eol_expect_error("examples/errors/error_final_method_override.cay")
        .expect("final method override should fail to compile");
    assert!(
        error.contains("cannot override final") || error.contains("final method"),
        "Should report final method override error, got: {}",
        error
    );
}

#[test]
fn test_error_static_access_instance() {
    let error = compile_eol_expect_error("examples/errors/error_static_access_instance.cay")
        .expect("static access instance should fail to compile");
    assert!(
        error.contains("non-static") || error.contains("static context"),
        "Should report non-static access from static context error, got: {}",
        error
    );
}

#[test]
fn test_0_4_4_static_member() {
    let output = compile_and_run_eol("examples/test_0_4_4_static_member.cay")
        .expect("0.4.4 static member should compile and run");
    assert!(
        output.contains("Initial counter: 0"),
        "Should output initial counter, got: {}",
        output
    );
    assert!(
        output.contains("Max count: 100"),
        "Should output max count, got: {}",
        output
    );
    assert!(
        output.contains("After 3 instances, counter: 3"),
        "Should output counter after 3 instances, got: {}",
        output
    );
    assert!(
        output.contains("Obj1 ID: 1"),
        "Should output obj1 ID, got: {}",
        output
    );
    assert!(
        output.contains("Obj2 ID: 2"),
        "Should output obj2 ID, got: {}",
        output
    );
    assert!(
        output.contains("Obj3 ID: 3"),
        "Should output obj3 ID, got: {}",
        output
    );
}

#[test]
fn test_instance_fields() {
    let output = compile_and_run_eol("examples/test_instance_fields.cay")
        .expect("Instance fields test should compile and run");
    assert!(
        output.contains("Int: 42"),
        "Should output int field, got: {}",
        output
    );
    assert!(
        output.contains("Long: 123456789"),
        "Should output long field, got: {}",
        output
    );
    assert!(
        output.contains("String: Hello"),
        "Should output string field, got: {}",
        output
    );
}

#[test]
fn test_this_access() {
    let output = compile_and_run_eol("examples/test_this_access.cay")
        .expect("This access test should compile and run");
    assert!(
        output.contains("Value (implicit): 42"),
        "Should output implicit value, got: {}",
        output
    );
    assert!(
        output.contains("Value (explicit): 42"),
        "Should output explicit value, got: {}",
        output
    );
    assert!(
        output.contains("Get value: 42"),
        "Should output get value, got: {}",
        output
    );
}

// ==================== vtable 动态分派测试 ====================

#[test]
fn test_vtable_dynamic_dispatch() {
    let output = compile_and_run_eol("examples/test_vtable_dynamic_dispatch.cay")
        .expect("vtable dynamic dispatch should compile and run");
    // 验证通过基类类型调用时正确分派到子类方法
    assert!(
        output.contains("Rex says: Woof!"),
        "Dog.speak() should be called through base type, got: {}",
        output
    );
    assert!(
        output.contains("Kitty says: Meow!"),
        "Cat.speak() should be called through base type, got: {}",
        output
    );
    // 验证继承方法的调用
    assert!(
        output.contains("Animal eats"),
        "Animal.eat() should be called for Dog, got: {}",
        output
    );
    assert!(
        output.contains("Kitty eats fish"),
        "Cat.eat() should be called through base type, got: {}",
        output
    );
    // 验证具体类型的直接调用
    assert!(
        output.contains("Buddy says: Woof!"),
        "Direct Dog.speak() should work, got: {}",
        output
    );
    assert!(
        output.contains("Buddy barks loudly!"),
        "Dog.bark() should work, got: {}",
        output
    );
    assert!(
        output.contains("vtable test passed"),
        "Should complete all tests, got: {}",
        output
    );
}
