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

/// 原始类型键：HashMap/HashSet 的键协议（hashCode/equals）此前只对类类型可用，
/// `HashMap<int, V>` 会在 `key.equals(other)` 处把变量名当类名 mangle，
/// 生成不存在的符号直到链接期才报错。原始类型现已支持 Object 协议方法。
#[test]
fn test_hashmap_int_keys() {
    let output = compile_and_run_eol("examples/test_hashmap_int_keys.cay")
        .expect("HashMap<int,int> 应可编译运行");

    assert!(
        output.contains("get(2) = 20")
            && output.contains("contains(9) = false")
            && output.contains("after overwrite get(2) = 99")
            && output.contains("size = 2"),
        "int 键的哈希表语义应正确: {}",
        output
    );
}
