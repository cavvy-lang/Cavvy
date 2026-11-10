//! std::PriorityQueue<T, A> binary-heap integration tests

mod common;
use common::{assert_output_contains, compile_and_run_eol};

#[test]
fn test_std_priorityqueue_full_api() {
    let output = compile_and_run_eol("examples/test_std_priorityqueue.cay")
        .expect("test_std_priorityqueue.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            // 空容器行为：零值返回
            "empty size = 0",
            "empty isEmpty = true",
            "empty peek = 0",
            "empty peekPriority = 0",
            "empty pop = 0",
            "empty size after pop = 0",
            // 大顶堆
            "max size = 5",
            "max isEmpty = false",
            "max peek = 50",
            "max peekPriority = 5",
            "max pop1 = 50",
            "max pop2 = 40",
            "max pop3 = 30",
            "max size after pops = 2",
            "max pop4 = 20",
            "max pop5 = 10",
            "max isEmpty after drain = true",
            "max pop on empty = 0",
            // 小顶堆
            "min peek = 10",
            "min peekPriority = 1",
            "min pop1 = 10",
            "min pop2 = 20",
            "min pop3 = 30",
            "min pop4 = 40",
            "min pop5 = 50",
            "min isEmpty after drain = true",
            // 同优先级 FIFO 稳定性
            "fifo pop1 = 100",
            "fifo pop2 = 101",
            "fifo pop3 = 102",
            "fifo pop4 = 103",
            "fifo pop5 = 104",
            "mixed pop1 = 900",
            "mixed pop2 = 901",
            "mixed pop3 = 500",
            "mixed pop4 = 501",
            "mixed pop5 = 502",
            "min fifo pop1 = 10",
            "min fifo pop2 = 11",
            "min fifo pop3 = 12",
            // size / isEmpty / clear 与 clear 后复用
            "life size before clear = 3",
            "life isEmpty before clear = false",
            "life size after clear = 0",
            "life isEmpty after clear = true",
            "life peek after clear = 0",
            "life peekPriority after clear = 0",
            "life size after reuse = 3",
            "life peek after reuse = 80",
            "life pop after reuse 1 = 80",
            "life pop after reuse 2 = 60",
            "life pop after reuse 3 = 40",
            // 指定分配器的构造函数
            "allocator ctor size = 3",
            "allocator ctor peek = 20",
            "allocator ctor peekPriority = 2",
            // 出队序列的 for-each 遍历
            "drained size = 4",
            "drained first = 1",
            "drained last = 4",
            "drained sum = 10",
            "drained ascending = true",
            // 200 次连续 push / pop
            "stress size after 200 pushes = 200",
            "stress isEmpty after 200 pushes = false",
            "stress ordered = true",
            "stress size after 200 pops = 0",
            "stress isEmpty after 200 pops = true",
            // 交错 push / pop
            "mix size = 100",
            "mix peek = 1049",
            "mix peekPriority = 149",
            "mix priority ordered = true",
            "mix isEmpty after drain = true",
            // 50 个同优先级元素的稳定性压力测试
            "stable fifo = true",
            "stable isEmpty = true",
            "std::PriorityQueue tests passed",
        ],
        "test_std_priorityqueue",
    );
}
