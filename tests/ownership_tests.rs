//! 所有权转移（RAII）与析构语义测试
//!
//! 固定住「局部对象经容器存储方法入容器后，析构责任转移给容器」的语义，
//! 保证每个对象恰好析构一次（双重析构或泄漏都会让计数偏离）。

mod common;
use common::{compile_and_run_eol, compile_eol_expect_error};

#[test]
fn test_ownership_transfer_baseline() {
    let output = compile_and_run_eol("examples/test_ownership_transfer.cay")
        .expect("test_ownership_transfer.cay should compile and run");

    assert!(
        output.contains("ownership transfer tests passed"),
        "ownership transfer invariants must hold, got: {}",
        output
    );
}

/// 容器内存回收：用计数分配器验证每个容器的析构链都把缓冲区归还给分配器
/// （alloc == free），并验证容器持有的元素恰好析构一次。
#[test]
fn test_container_release() {
    let output = compile_and_run_eol("examples/test_container_release.cay")
        .expect("test_container_release.cay should compile and run");

    assert!(
        output.contains("container release tests passed"),
        "containers must return every allocation to their allocator, got: {}",
        output
    );
}

/// `@owns` 只对可被析构的对象类型有意义。标在原始类型上只会静默抑制调用方的
/// 析构（泄漏）而无任何接管，属 API 作者笔误 —— 必须报错而不是放过。
#[test]
fn test_owns_rejects_primitive_param() {
    let code = r#"
public class C {
    public void f(@owns int x) { println(String.valueOf(x)); }
}

public class Main {
    public static void main() { C c = new C(); c.f(1); }
}
"#;
    std::fs::write("examples/test_owns_bad_primitive.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_owns_bad_primitive.cay")
        .expect("原始类型上的 @owns 应该编译失败");
    assert!(
        error.contains("@owns"),
        "错误信息应指出 @owns 用法问题: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_owns_bad_primitive.cay");
}

/// `@owns` 不能用于可变参数：可变参数是数组语义，没有单一所有权可转移。
#[test]
fn test_owns_rejects_varargs_param() {
    let code = r#"
public class C {
    public void g(@owns int... xs) { println("v"); }
}

public class Main {
    public static void main() { C c = new C(); c.g(1); }
}
"#;
    std::fs::write("examples/test_owns_bad_varargs.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_owns_bad_varargs.cay")
        .expect("@owns 标在可变参数上应该编译失败");
    assert!(
        error.contains("@owns"),
        "错误信息应指出 @owns 用法问题: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_owns_bad_varargs.cay");
}
