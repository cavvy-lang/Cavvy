//! ArrayList<T, A> and enhanced for-loop integration tests

mod common;
use common::{assert_output_contains, compile_and_run_eol};

#[test]
fn test_arraylist_foreach_basic() {
    let output = compile_and_run_eol("examples/test_arraylist_foreach.cay")
        .expect("test_arraylist_foreach.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            "sum = 60",
            "concat = helloworld",
            "size = 3",
            "capacity = 4",
            "isEmpty = false",
            "reserved capacity = 10",
            "ArrayList foreach tests passed",
        ],
        "test_arraylist_foreach",
    );
}
