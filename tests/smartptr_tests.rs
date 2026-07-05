//! Cavvy 智能指针集成测试
//!
//! 覆盖 ROADMAP 5.3.x 四种智能指针：UniquePtr、ScopedPtr、Rc、WeakPtr。

mod common;
use common::compile_and_run_eol;

#[test]
fn test_smartptr_basic() {
    let output = compile_and_run_eol("examples/test_smartptr_basic.cay")
        .expect("basic smart pointer example should compile and run");

    assert!(
        output.contains("p.v=1"),
        "UniquePtr should dereference managed object, got: {}",
        output
    );
    assert!(
        output.contains("s.v=2"),
        "ScopedPtr should dereference managed object, got: {}",
        output
    );
    // 作用域退出后应按声明逆序调用析构函数：先 s (v=2)，后 p (v=1)
    assert!(
        output.contains("dtor 2"),
        "ScopedPtr should trigger destructor on scope exit, got: {}",
        output
    );
    assert!(
        output.contains("dtor 1"),
        "UniquePtr should trigger destructor on scope exit, got: {}",
        output
    );
    assert!(
        output.contains("after scope"),
        "Program should continue after scope block, got: {}",
        output
    );
}

#[test]
fn test_smartptr_rc() {
    let output = compile_and_run_eol("examples/test_smartptr_rc.cay")
        .expect("Rc example should compile and run");

    assert!(
        output.contains("c1=1"),
        "Rc should start with refcount 1, got: {}",
        output
    );
    assert!(
        output.contains("c2=2"),
        "Rc clone should increment refcount to 2, got: {}",
        output
    );
    assert!(
        output.contains("clone ok"),
        "Rc clone test should pass, got: {}",
        output
    );
    assert!(
        output.contains("dtor 1"),
        "Managed object should be destroyed when last Rc drops, got: {}",
        output
    );
}

#[test]
fn test_smartptr_weak() {
    let output = compile_and_run_eol("examples/test_smartptr_weak.cay")
        .expect("WeakPtr example should compile and run");

    assert!(
        output.contains("rc.count=1"),
        "Rc should start with refcount 1, got: {}",
        output
    );
    assert!(
        output.contains("weak alive ok"),
        "WeakPtr should report not expired while object lives, got: {}",
        output
    );
    assert!(
        output.contains("upgraded present ok"),
        "WeakPtr upgrade should succeed while object lives, got: {}",
        output
    );
    assert!(
        output.contains("upgraded.v=1"),
        "Upgraded Rc should access managed object, got: {}",
        output
    );
    assert!(
        output.contains("rc.count=2 ok"),
        "WeakPtr upgrade should increment refcount to 2, got: {}",
        output
    );
    assert!(
        output.contains("dtor 1"),
        "Managed object should be destroyed after scope exit, got: {}",
        output
    );
}
