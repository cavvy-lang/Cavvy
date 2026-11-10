//! std::Stack<T, A> LIFO container integration tests

mod common;
use common::{assert_output_contains, compile_and_run_eol};

#[test]
fn test_std_stack_full_api() {
    let output = compile_and_run_eol("examples/test_std_stack.cay")
        .expect("test_std_stack.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            "empty isEmpty = true",
            "empty size = 0",
            "empty pop = 0",
            "empty peek = 0",
            "size after 3 pushes = 3",
            "isEmpty after push = false",
            "peek after push = 30",
            "pop 1 = 30",
            "pop 2 = 20",
            "size after 2 pops = 1",
            "peek after 2 pops = 10",
            "iterator order encoded = 321",
            "iterator count = 3",
            "size after iteration = 3",
            "size after clear = 0",
            "isEmpty after clear = true",
            "pop after clear = 0",
            "peek after clear reuse = 7",
            "bulk size after 200 pushes = 200",
            "bulk 200 pops ordered = true",
            "bulk 200 pops sum = 19900",
            "bulk size after 200 pops = 0",
            "bulk isEmpty after 200 pops = true",
            "bulk empty pop after drain = 0",
            "mixed peek after 125 pops = 124",
            "mixed size after refill = 150",
            "mixed peek after refill = 1024",
            "mixed top pop = 1024",
            "mixed size after top pop = 149",
            "arena used before pushes = 0",
            "arena used increased = true",
            "arena stack size = 5",
            "arena stack peek = 50",
            "arena stack pops ordered = true",
            "arena stack isEmpty after drain = true",
            "arena-backed Stack tests passed",
            "std::Stack tests passed",
        ],
        "test_std_stack",
    );
}
