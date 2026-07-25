//! Cavvy 语言轻量级并发集成测试 (ROADMAP 6.2.x)
//!
//! 覆盖 caylibs 并发库：
//! - Thread.cay: spawn/join、捕获变量、ThreadBuilder、id/currentId、sleep/yield
//! - Atomic.cay: AtomicI32/I64/Bool/Ptr 与内存序、多线程 fetchAdd 原子性
//! - Mutex.cay: Mutex/RwLock、RAII Guard、tryLock 系列、多线程竞争
//!
//! 多线程输出顺序不确定，只断言确定性标记与最终聚合值。

mod common;
use common::{assert_output_contains, compile_and_run_eol};

// ==================== Thread ====================

#[test]
fn test_thread_basic() {
    let _lock = common::TEST_LOCK.lock().unwrap();
    let output = compile_and_run_eol("examples/test_thread.cay")
        .expect("test_thread should compile and run");
    assert_output_contains(
        &output,
        &[
            "t0-run",
            "t0-joined",
            "33", // 捕获变量 a+b = 11+22
            "w1",
            "w2",
            "w3",
            "w4",
            "workers-joined",
            "builder-run",
            "t3-done",
            "done",
        ],
        "test_thread",
    );
}

// ==================== Atomic ====================

#[test]
fn test_atomic_operations() {
    let _lock = common::TEST_LOCK.lock().unwrap();
    let output = compile_and_run_eol("examples/test_atomic.cay")
        .expect("test_atomic should compile and run");
    assert_output_contains(
        &output,
        &[
            "99",    // swap 后 get
            "true",  // CAS 成功
            "5678",  // AtomicPtr CAS 后的值
            "80000", // 8 线程 × 10000 次 fetchAdd，原子性保证恰好 80000
            "done",
        ],
        "test_atomic",
    );
}

// ==================== Mutex / RwLock ====================

#[test]
fn test_mutex_and_rwlock() {
    let _lock = common::TEST_LOCK.lock().unwrap();
    let output = compile_and_run_eol("examples/test_mutex.cay")
        .expect("test_mutex should compile and run");
    assert_output_contains(
        &output,
        &[
            "100000", // 4 线程 × 25000 次互斥递增
            "hello",
            "world",
            "done",
        ],
        "test_mutex",
    );
}

// ==================== 编译器回归：静态方法引用改编名 ====================

/// 回归：`ClassName::staticMethod` 作为 extern 函数指针参数时，
/// generate_method_ref 必须生成 Itanium 改编名（修复前生成未改编名
/// @Class.method，导致 "use of undefined value"）。
#[test]
fn test_method_ref_mangling() {
    let _lock = common::TEST_LOCK.lock().unwrap();
    let output = compile_and_run_eol("examples/test_method_ref_mangling.cay")
        .expect("test_method_ref_mangling should compile and run");
    assert_output_contains(&output, &["1", "2", "3", "4", "done"], "test_method_ref_mangling");
}
