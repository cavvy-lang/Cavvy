//! HashMap<K, V, A> and HashSet<T, A> integration tests

mod common;
use common::{assert_output_contains, compile_and_run_eol};

#[test]
fn test_hashmap_hashset_basic() {
    let output = compile_and_run_eol("examples/test_hashmap_hashset.cay")
        .expect("test_hashmap_hashset.cay should compile and run");

    assert_output_contains(
        &output,
        &[
            "Alice = 90",
            "Bob = 85",
            "size = 3",
            "contains Alice = true",
            "size after remove = 2",
            "contains Bob = false",
            "keySum = 178",
            "set size = 2",
            "contains cavvy = true",
            "contains java = false",
            "HashMap HashSet tests passed",
        ],
        "test_hashmap_hashset",
    );
}
