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

/// RwLock 多线程竞争：1 写 + 3 读，最终值与读计数必须精确
#[test]
fn test_rwlock_contention() {
    let output = compile_and_run_eol("examples/test_rwlock_contention.cay")
        .expect("test_rwlock_contention should compile and run");
    assert_output_contains(
        &output,
        &[
            "2000", // 写线程递增 2000 次
            "1500", // 3 读线程 × 500 次
            "done",
        ],
        "test_rwlock_contention",
    );
}

// ==================== detach / 线程命名 ====================

/// detach() 后台运行 + ThreadBuilder.name() 命名效果（Linux/macOS 读回验证）
#[test]
fn test_thread_detach_and_naming() {
    let output = compile_and_run_eol("examples/test_thread_detach.cay")
        .expect("test_thread_detach should compile and run");
    let mut expected = vec!["1", "detached", "done"];
    // 线程名读回验证仅在 pthread 平台启用（示例内 #ifndef _WIN32 守卫）
    if !cfg!(target_os = "windows") {
        expected.push("true");
    }
    assert_output_contains(&output, &expected, "test_thread_detach");
}

// ==================== 编译器回归：静态方法引用改编名 ====================

/// 回归：`ClassName::staticMethod` 作为 extern 函数指针参数时，
/// generate_method_ref 必须生成 Itanium 改编名（修复前生成未改编名
/// @Class.method，导致 "use of undefined value"）。
#[test]
fn test_method_ref_mangling() {
    let output = compile_and_run_eol("examples/test_method_ref_mangling.cay")
        .expect("test_method_ref_mangling should compile and run");
    assert_output_contains(&output, &["1", "2", "3", "4", "done"], "test_method_ref_mangling");
}
