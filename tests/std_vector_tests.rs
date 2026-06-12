//! std::vector integration tests

mod common;
use common::{assert_output_contains, compile_and_run_eol};

#[test]
fn test_std_vector_basic_operations() {
    let output = compile_and_run_eol("examples/test_std_vector.cay")
        .expect("test_std_vector.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            "initial empty = true",
            "size after push = 3",
            "middle before set = 20",
            "middle after set = 25",
            "front = 10",
            "back = 30",
            "size after pop = 2",
            "capacity after reserve = 8",
            "size after resize = 4",
            "capacity after resize = 8",
            "new element = 7",
            "size after erase = 3",
            "capacity after erase = 8",
            "element after erase = 7",
            "capacity after clear = 8",
            "empty after clear = true",
            "value after clear reuse = 99",
            "capacity after clear reuse = 8",
            "capacity after shrink = 1",
            "student 0 = Zhang, 18, 90.000000",
            "student 1 = Li, 19, 85.000000",
            "std::vector tests passed",
        ],
        "test_std_vector",
    );
}
