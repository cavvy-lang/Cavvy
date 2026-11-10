//! 所有权转移（RAII）与析构语义测试
//!
//! 固定住「局部对象经容器存储方法入容器后，析构责任转移给容器」的语义，
//! 保证每个对象恰好析构一次（双重析构或泄漏都会让计数偏离）。

mod common;
use common::{
    compile_and_run_eol, compile_and_run_eol_with_features, compile_eol_expect_error,
    compile_eol_expect_error_with_features,
};

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

/// `@borrow` 只读访问、clear 析构、HashMap 覆盖、LinkedList 删除节点、
/// 非法索引 add 的 @owns 兜底析构等回归场景。
#[test]
fn test_raii_review_regressions() {
    let output = compile_and_run_eol("examples/test_raii_review.cay")
        .expect("test_raii_review.cay should compile and run");
    assert!(
        output.contains("raii review tests passed"),
        "RAII review regressions must hold, got: {}",
        output
    );
}

/// 同一局部变量第二次传给 `@owns` 形参必须编译失败，否则两个接收方会
/// 重复析构同一对象。
#[test]
fn test_repeated_owns_transfer_rejected() {
    let code = r#"
public class Box {
    public Box() {}
    ~Box() { println("~Box"); }
}

public class Holder {
    public void take(@owns Box b) { }
}

public class Main {
    public static void main() {
        Holder h = new Holder();
        Box b = new Box();
        h.take(b);
        h.take(b);
    }
}
"#;
    std::fs::write("examples/test_repeated_owns.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_repeated_owns.cay")
        .expect("重复 @owns 转移必须编译失败");
    assert!(
        error.contains("@owns") && error.contains("不能再次转移"),
        "应指出重复转移: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_repeated_owns.cay");
}

/// 接口方法与实现的 `@owns` 契约必须一致。
#[test]
fn test_owns_interface_contract_mismatch_rejected() {
    let code = r#"
public class Box {
    public Box() {}
    ~Box() { println("~Box"); }
}

public interface Sink {
    void put(Box b);
}

public class BadSink implements Sink {
    public void put(@owns Box b) { }
}

public class Main {
    public static void main() {
        BadSink s = new BadSink();
        Box b = new Box();
        s.put(b);
    }
}
"#;
    std::fs::write("examples/test_owns_interface_mismatch.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_owns_interface_mismatch.cay")
        .expect("接口实现改变 @owns 契约必须编译失败");
    assert!(
        error.contains("must implement interface method"),
        "应报告接口方法未实现: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_owns_interface_mismatch.cay");
}

/// `__cay_destroy<T>` 的类型实参必须与实参类型一致。
#[test]
fn test_wrong_destroy_type_rejected() {
    let code = r#"
public class Base {
    public Base() {}
    ~Base() { println("~Base"); }
}

public class Derived {
    public Derived() {}
    ~Derived() { println("~Derived"); }
}

public class Main {
    public static void main() {
        Derived d = new Derived();
        __cay_destroy<Base>(d);
    }
}
"#;
    std::fs::write("examples/test_wrong_destroy_type.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_wrong_destroy_type.cay")
        .expect("错误析构类型必须编译失败");
    assert!(
        error.contains("__cay_destroy<Base>") && error.contains("Derived"),
        "应报告析构类型不匹配: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_wrong_destroy_type.cay");
}

/// 构造函数的 `@owns` 适用性检查必须与普通方法一致。
#[test]
fn test_owns_rejects_primitive_constructor_param() {
    let code = r#"
public class Bad {
    public Bad(@owns int x) { }
}

public class Main {
    public static void main() { }
}
"#;
    std::fs::write("examples/test_owns_bad_ctor.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_owns_bad_ctor.cay")
        .expect("构造函数原始类型上的 @owns 必须编译失败");
    assert!(
        error.contains("@owns") && error.contains("构造函数"),
        "应指出构造函数 @owns 用法问题: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_owns_bad_ctor.cay");
}

/// 顶层函数的 `@owns` 适用性检查必须与普通方法一致。
#[test]
fn test_owns_rejects_primitive_toplevel_param() {
    let code = r#"
public void bad(@owns int x) { }

public int main() {
    return 0;
}
"#;
    std::fs::write("examples/test_owns_bad_toplevel.cay", code).unwrap();
    let error =
        compile_eol_expect_error_with_features("examples/test_owns_bad_toplevel.cay", &[
            "-F=top_level_function",
        ])
        .expect("顶层函数原始类型上的 @owns 必须编译失败");
    assert!(
        error.contains("@owns") && error.contains("顶层函数"),
        "应指出顶层函数 @owns 用法问题: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_owns_bad_toplevel.cay");
}

/// 同参数个数的重载必须按实际选中的重载（而非第一个）应用 @owns。
#[test]
fn test_owns_follows_selected_overload() {
    let code = r#"
#include <c/stdio.cay>

public class Box {
    public static int dtors = 0;
    public int v;
    public Box(int v) { this.v = v; }
    ~Box() { Box.dtors = Box.dtors + 1; }
}

public class Holder {
    public void put(Box b, int x) { println("non-owning overload"); }
    public void put(int x, @owns Box b) {
        println("owning overload");
        __cay_destroy<Box>(b);
    }
}

public int main() {
    Holder h = new Holder();
    Box b = new Box(1);
    h.put(1, b);
    if (Box.dtors != 1) {
        println("FAILED overload dtors=" + String.valueOf(Box.dtors));
        return 1;
    }
    println("overload ownership passed");
    return 0;
}
"#;
    std::fs::write("examples/test_owns_overload.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_owns_overload.cay")
        .expect("重载 @owns 测试应编译运行通过");
    assert!(
        output.contains("overload ownership passed"),
        "选中重载的 @owns 必须生效: {}",
        output
    );
    let _ = std::fs::remove_file("examples/test_owns_overload.cay");
}

/// 顶层函数的 @owns 形参同样必须让调用方摘除析构候选。
#[test]
fn test_owns_toplevel_transfer() {
    let code = r#"
#include <c/stdio.cay>

public class Box {
    public static int dtors = 0;
    public int v;
    public Box(int v) { this.v = v; }
    ~Box() { Box.dtors = Box.dtors + 1; }
}

public void take(@owns Box b) {
    __cay_destroy<Box>(b);
}

public int main() {
    Box b = new Box(1);
    take(b);
    if (Box.dtors != 1) {
        println("FAILED toplevel dtors=" + String.valueOf(Box.dtors));
        return 1;
    }
    println("toplevel ownership passed");
    return 0;
}
"#;
    std::fs::write("examples/test_owns_toplevel.cay", code).unwrap();
    let output = compile_and_run_eol_with_features(
        "examples/test_owns_toplevel.cay",
        &["-F=top_level_function"],
    )
    .expect("顶层函数 @owns 测试应编译运行通过");
    assert!(
        output.contains("toplevel ownership passed"),
        "顶层函数 @owns 必须转移所有权: {}",
        output
    );
    let _ = std::fs::remove_file("examples/test_owns_toplevel.cay");
}

/// struct 构造函数参数同样应用 @owns 转移。
#[test]
fn test_owns_struct_constructor_transfer() {
    let code = r#"
#include <c/stdio.cay>

public class Box {
    public static int dtors = 0;
    public int v;
    public Box(int v) { this.v = v; }
    ~Box() { Box.dtors = Box.dtors + 1; }
}

public struct Holder {
    private Box b;
    public Holder(@owns Box b) { this.b = b; }
    public void drop() { __cay_destroy<Box>(this.b); }
}

public int main() {
    Box b = new Box(1);
    Holder h = new Holder(b);
    if (Box.dtors != 0) {
        println("FAILED struct ctor destroyed early=" + String.valueOf(Box.dtors));
        return 1;
    }
    h.drop();
    if (Box.dtors != 1) {
        println("FAILED struct ctor dtors=" + String.valueOf(Box.dtors));
        return 1;
    }
    println("struct constructor ownership passed");
    return 0;
}
"#;
    std::fs::write("examples/test_owns_struct_ctor.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_owns_struct_ctor.cay")
        .expect("struct 构造函数 @owns 测试应编译运行通过");
    assert!(
        output.contains("struct constructor ownership passed"),
        "struct 构造函数必须应用 @owns: {}",
        output
    );
    let _ = std::fs::remove_file("examples/test_owns_struct_ctor.cay");
}
