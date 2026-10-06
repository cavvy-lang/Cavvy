//! std::LinkedList<T> 双向链表集成测试

mod common;
use common::{assert_output_contains, compile_and_run_eol};

#[test]
fn test_std_linkedlist_full_api() {
    let output = compile_and_run_eol("examples/test_std_linkedlist.cay")
        .expect("test_std_linkedlist.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            // 空容器行为：所有取元素方法返回类型零值
            "empty size = 0",
            "empty isEmpty = true",
            "empty getFirst = 0",
            "empty getLast = 0",
            "empty removeFirst = 0",
            "empty removeLast = 0",
            "empty get(0) = 0",
            "empty get(-1) = 0",
            "empty indexOf 10 = -1",
            "empty contains 10 = false",
            "empty size after failed removes = 0",
            // 基本首尾增删与下标读取
            "list size after 4 adds = 4",
            "list isEmpty after adds = false",
            "list getFirst = 5",
            "list getLast = 30",
            "list get(0) = 5",
            "list get(1) = 10",
            "list get(2) = 20",
            "list get(3) = 30",
            "list get(4) out of range = 0",
            "list get(-1) out of range = 0",
            // 查找（== 比较）
            "list indexOf 5 = 0",
            "list indexOf 20 = 2",
            "list indexOf 99 = -1",
            "list contains 30 = true",
            "list contains 99 = false",
            // 双端删除
            "list removeFirst = 5",
            "list removeLast = 30",
            "list size after 2 removes = 2",
            "list getFirst after removes = 10",
            "list getLast after removes = 20",
            // 单元素删除后 head/tail 均置 null
            "single removeLast = 7",
            "single size = 0",
            "single isEmpty = true",
            "single getFirst = 0",
            "single getLast = 0",
            "single2 removeFirst = 8",
            "single2 size = 0",
            "single2 getFirst = 0",
            "single2 getLast = 0",
            // 首尾交替操作
            "alt get(0) = 0",
            "alt get(3) = 3",
            "alt removed front = 0",
            "alt removed back = 3",
            "alt size after removals = 2",
            "alt getFirst after removals = 1",
            "alt getLast after removals = 2",
            "alt removed front2 = 1",
            "alt removed back2 = 2",
            "alt size final = 0",
            "alt isEmpty final = true",
            // 增强 for 循环
            "foreach sum = 10",
            "foreach count = 5",
            "foreach empty iterations = 0",
            // clear 后复用
            "size after clear = 0",
            "isEmpty after clear = true",
            "getFirst after clear = 0",
            "getLast after clear = 0",
            "indexOf after clear = -1",
            "size after clear reuse = 1",
            "getFirst after clear reuse = 42",
            // 1000 节点 push/pop
            "big size after 1000 addLast = 1000",
            "big get(500) = 500",
            "big get(999) = 999",
            "big get(1000) out of range = 0",
            "big indexOf 999 = 999",
            "big foreach sum = 499500",
            "big size after 1000 removes = 0",
            "big last removeFirst = 499",
            "big last removeLast = 500",
            "big isEmpty = true",
            "big removeFirst on emptied = 0",
            "big removeLast on emptied = 0",
            // 600 次插入 + 400 次删除的连续操作正确性
            "seq size after 600 adds = 600",
            "seq get(0) = 299",
            "seq get(299) = 0",
            "seq get(300) = 1000",
            "seq get(599) = 1299",
            "seq indexOf 0 = 299",
            "seq mismatches = 0",
            "seq size after 400 removes = 200",
            "seq getFirst = 99",
            "seq getLast = 1099",
            // 收尾行
            "std::LinkedList tests passed",
        ],
        "test_std_linkedlist",
    );
}
