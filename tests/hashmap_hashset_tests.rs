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

/// `+0.0` 与 `-0.0` 在 equals 下相等，hashCode 必须规范化符号零，
/// 否则相等键会落入不同桶，导致查询/覆盖失败。
#[test]
fn test_hashmap_signed_zero_keys() {
    let code = r#"
#include <std/hashmap.cay>
#include <c/stdio.cay>

using std::HashMap;

public int main() {
    HashMap<double, int> m = new HashMap<double, int>();
    m.put(0.0, 1);
    if (!m.containsKey(-0.0)) {
        println("FAILED double contains -0.0");
        return 1;
    }
    m.put(-0.0, 2);
    if (m.size() != 1 || m.get(0.0) != 2) {
        println("FAILED double overwrite");
        return 1;
    }

    HashMap<float, int> f = new HashMap<float, int>();
    f.put(0.0f, 3);
    if (!f.containsKey(-0.0f)) {
        println("FAILED float contains -0.0");
        return 1;
    }
    println("signed zero hash passed");
    return 0;
}
"#;
    std::fs::write("examples/test_hashmap_signed_zero.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_hashmap_signed_zero.cay")
        .expect("符号零键测试应编译运行通过");
    assert!(
        output.contains("signed zero hash passed"),
        "相等浮点零必须产生相同哈希: {}",
        output
    );
    let _ = std::fs::remove_file("examples/test_hashmap_signed_zero.cay");
}
