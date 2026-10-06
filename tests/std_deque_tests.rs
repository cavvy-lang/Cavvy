//! std::Deque<T, A> 双端队列集成测试

mod common;
use common::{assert_output_contains, compile_and_run_eol};

#[test]
fn test_std_deque_full_api() {
    let output = compile_and_run_eol("examples/test_std_deque.cay")
        .expect("test_std_deque.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            // 空容器行为：读取/弹出均返回 T 零值
            "empty size = 0",
            "empty isEmpty = true",
            "empty peekFront = 0",
            "empty peekBack = 0",
            "empty popFront = 0",
            "empty popBack = 0",
            "empty get(0) = 0",
            "empty get(-1) = 0",
            "empty foreach sum = 0",
            "empty size after reads = 0",
            // 基本增删：pushBack 2,3 后 pushFront 1 => [1,2,3]
            "after pushes size = 3",
            "front = 1",
            "back = 3",
            "get(0) = 1",
            "get(1) = 2",
            "get(2) = 3",
            "get(3) oob = 0",
            "popFront = 1",
            "popBack = 3",
            "size after pops = 1",
            "peekFront after pops = 2",
            "peekBack after pops = 2",
            "get(1) after pops oob = 0",
            "size after pushFront = 2",
            "front after pushFront = 0",
            "back after pushFront = 2",
            "foreach sum = 2",
            // clear 与复用
            "after clear size = 0",
            "after clear isEmpty = true",
            "cleared popFront = 0",
            "reuse size = 2",
            "reuse front = 7",
            "reuse back = 8",
            // 窗口偏移（head > 0）下的迭代器遍历
            "windowed size = 5",
            "windowed front = 15",
            "windowed back = 19",
            "windowed foreach sum = 85",
            "windowed get sum = 85",
            // 200+ 连续操作：含压缩前后的下标映射
            "big size after 100 pushBack = 100",
            "big size after 40 popFront = 60",
            "big front after 40 popFront = 40",
            "big size after 30 pushFront = 90",
            "big front after 30 pushFront = -30",
            "big get(29) = -1",
            "big get(30) = 40",
            "big back after 30 popBack = 69",
            "big size after 25 popFront = 35",
            "big front after compact = -5",
            "big back after compact = 69",
            "big get(4) = -1",
            "big get(5) = 40",
            "big get(34) = 69",
            "big foreach sum = 1620",
            "big drain sum = 1620",
            "big drained size = 0",
            "big drained isEmpty = true",
            // 200 次混合操作与 ArrayList 模型逐元素比对
            "mixed model match after 200 ops = true",
            "mixed size = 68",
            "mixed isEmpty = false",
            "mixed front = 199",
            "mixed back = 198",
            // 显式分配器构造
            "allocator ctor size = 2",
            "allocator ctor front = 22",
            "allocator ctor back = 11",
            "std::Deque tests passed",
        ],
        "test_std_deque",
    );
}
