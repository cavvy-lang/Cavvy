//! 所有权转移（RAII）与析构语义测试
//!
//! 固定住「局部对象经容器存储方法入容器后，析构责任转移给容器」的语义，
//! 保证每个对象恰好析构一次（双重析构或泄漏都会让计数偏离）。

mod common;
use common::compile_and_run_eol;

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
