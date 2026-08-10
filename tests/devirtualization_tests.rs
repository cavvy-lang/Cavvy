//! 编译期去虚拟化优化集成测试
//!
//! 验证：方法未被重写（或为 final）时，编译器可将其降级为直接调用，
//! 同时保持被重写方法的动态分派语义。

mod common;
use common::compile_and_run_eol;

#[test]
fn test_devirtualization() {
    let output = compile_and_run_eol("examples/test_devirtualization.cay")
        .expect("devirtualization example should compile and run");

    assert!(
        output.contains("Base.notOverridden"),
        "Non-overridden method should be callable, got: {}",
        output
    );
    assert!(
        output.contains("Derived.overridden"),
        "Overridden method should still dispatch dynamically, got: {}",
        output
    );
    assert!(
        output.contains("Base.finalMethod"),
        "Final method should be callable, got: {}",
        output
    );
    assert!(
        output.contains("FinalLeaf.leafMethod"),
        "Final class method should be callable, got: {}",
        output
    );
    assert!(
        output.contains("devirtualization test passed"),
        "Test should complete successfully, got: {}",
        output
    );
}
