//! std::Queue<T, A> FIFO queue integration tests

mod common;
use common::{assert_output_contains, compile_and_run_eol};

#[test]
fn test_std_queue_full_api() {
    let output = compile_and_run_eol("examples/test_std_queue.cay")
        .expect("test_std_queue.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            "empty isEmpty = true",
            "empty size = 0",
            "empty dequeue = 0",
            "empty pop = 0",
            "empty front = 0",
            "empty back = 0",
            "size after 3 enqueue = 3",
            "front after enqueue = 10",
            "back after enqueue = 30",
            "dequeue = 10",
            "size after dequeue = 2",
            "front after dequeue = 20",
            "back after dequeue = 30",
            "pop via alias = 20",
            "size after pop = 2",
            "after clear isEmpty = true",
            "after clear size = 0",
            "after clear front = 0",
            "after clear back = 0",
            "reuse dequeue = 99",
            "foreach count = 2",
            "foreach sum = 5",
            "batch size after 100 enqueue = 100",
            "batch ordered = true",
            "batch sum = 5050",
            "batch final size = 0",
            "batch empty front = 0",
            "batch empty back = 0",
            "alternating ok = 200",
            "alternating final size = 0",
            "capacity ctor dequeue = 7",
            "allocator ctor dequeue = 8",
            "std::Queue tests passed",
        ],
        "test_std_queue",
    );
}
