//! std::ArrayList<T, A> allocator-backed integration tests

mod common;
use common::{assert_output_contains, compile_and_run_eol};

#[test]
fn test_std_arraylist_full_api() {
    let output = compile_and_run_eol("examples/test_std_arraylist.cay")
        .expect("test_std_arraylist.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            "empty initial = true",
            "size initial = 0",
            "capacity initial = 1",
            "size after add = 3",
            "capacity after add = 4",
            "isEmpty after add = false",
            "get(0) = 10",
            "get(1) = 20",
            "get(2) = 30",
            "get(1) after set = 25",
            "size after insert = 4",
            "indexOf 25 = 2",
            "indexOf 99 = -1",
            "lastIndexOf 10 = 0",
            "contains 30 = true",
            "contains 99 = false",
            "removed index 2 = 25",
            "size after remove = 3",
            "size after remove value 25 = 3",
            "indexOf 25 after remove = -1",
            "removeLast = 30",
            "size after removeLast = 2",
            "size after addAll = 4",
            "get after addAll 0 = 10",
            "get after addAll 1 = 15",
            "get after addAll 2 = 100",
            "get after addAll 3 = 200",
            "capacity after reserve 32 = 32",
            "capacity after ensureCapacity 40 = 64",
            "capacity after trimToSize = 4",
            "toArray sum = 325",
            "size after clear = 0",
            "isEmpty after clear = true",
            "get after clear reuse = 77",
            "reserved capacity = 10",
            "foreach sum = 6",
            "student 0 = Zhang, 18, 90.000000",
            "student 1 = Li, 19, 85.000000",
            "std::ArrayList tests passed",
        ],
        "test_std_arraylist",
    );
}

#[test]
fn test_std_arraylist_arena_allocator() {
    let output = compile_and_run_eol("examples/test_std_arraylist_arena.cay")
        .expect("test_std_arraylist_arena.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            "global list size = 3",
            "arena used before adds = 0",
            "arena used after 5 adds = 96",
            "arena used after reserve = 360",
            "arena-backed ArrayList tests passed",
        ],
        "test_std_arraylist_arena",
    );
}

#[test]
fn test_std_arraylist_nested() {
    let output = compile_and_run_eol("examples/test_std_arraylist_nested.cay")
        .expect("test_std_arraylist_nested.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            "nested sum = 21",
            "rows = 2",
            "cols = 3",
            "matrix[1][2] = 6",
            "std::ArrayList nested tests passed",
        ],
        "test_std_arraylist_nested",
    );
}

#[test]
fn test_std_arraylist_allocator_constructor() {
    let output = compile_and_run_eol("examples/test_std_arraylist_allocator.cay")
        .expect("test_std_arraylist_allocator.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            "matrix.size() = 1",
            "matrix.get(0).size() = 3",
            "matrix.get(0).get(2) = 3",
            "std::ArrayList allocator constructor tests passed",
        ],
        "test_std_arraylist_allocator_constructor",
    );
}
