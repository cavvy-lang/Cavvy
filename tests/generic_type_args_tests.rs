//! 泛型类型实参 must 按位置校验（不变性）
//!
//! 早期实现里 `types_compatible` 对 `Generic` vs `Generic` 只比较基础类名，
//! 完全忽略类型实参 —— 于是 `ArrayList<int, CAlloc>` 被当作
//! `ArrayList<int, GlobalAlloc>`（默认分配器）接受，错误程序照常编译，
//! 直到运行期以 `free(): invalid pointer` 之类的内存错误暴露。

mod common;
use common::{compile_and_run_eol, compile_eol_expect_error};

/// 把元素类型写成 `ArrayList<int>`（默认 GlobalAlloc）却推入
/// `ArrayList<int, CAlloc>` 必须编译失败。
#[test]
fn test_mismatched_generic_args_rejected() {
    let code = r#"
#include <std/arraylist.cay>
#include <std/stack.cay>
#include <Allocator.cay>

using std::ArrayList;
using std::Stack;
using std::Allocator;

public class CAlloc implements Allocator {
    public CAlloc() {}
    public long allocate(long size) {
        long p = 0; long sz = size;
        __ir {
            %sv = load i64, i64* %sz
            %mp = call i8* @malloc(i64 %sv)
            %mv = ptrtoint i8* %mp to i64
            store i64 %mv, i64* %p
        }
        return p;
    }
    public long allocateAligned(long size, long alignBytes) { return this.allocate(size); }
    public void deallocate(long ptr) {
        if (ptr == 0) { return; }
        long p = ptr;
        __ir {
            %pv = load i64, i64* %p
            %fp = inttoptr i64 %pv to i8*
            call void @free(i8* %fp)
        }
    }
}

public class Main {
    public static int main(String[] args) {
        CAlloc c = new CAlloc();
        /* 元素类型是 ArrayList<int>，即 ArrayList<int, GlobalAlloc> */
        Stack<ArrayList<int>, CAlloc> s = new Stack<ArrayList<int>, CAlloc>(c);
        /* 推入 ArrayList<int, CAlloc>：泛型实参不匹配 */
        ArrayList<int, CAlloc> inner = new ArrayList<int, CAlloc>(c);
        inner.add(7);
        s.push(inner);
        return 0;
    }
}
"#;
    std::fs::write("examples/test_generic_args_mismatch.cay", code).unwrap();
    let error = compile_eol_expect_error("examples/test_generic_args_mismatch.cay")
        .expect("泛型实参不匹配的调用必须编译失败");
    assert!(
        error.contains("type mismatch"),
        "应报告类型不匹配: {}",
        error
    );
    let _ = std::fs::remove_file("examples/test_generic_args_mismatch.cay");
}

/// 正例：显式写全默认类型实参（或两边都省略）时不得误报。
#[test]
fn test_matching_generic_args_accepted() {
    let code = r#"
#include <std/arraylist.cay>
#include <std/stack.cay>
#include <c/stdio.cay>

using std::ArrayList;
using std::Stack;

public class Main {
    public static int main(String[] args) {
        /* 两边都省略分配器实参：同一类型，必须通过 */
        Stack<ArrayList<int>> s = new Stack<ArrayList<int>>();
        ArrayList<int> inner = new ArrayList<int>();
        inner.add(7);
        s.push(inner);
        println("ok " + String.valueOf(s.size()));
        return 0;
    }
}
"#;
    std::fs::write("examples/test_generic_args_match.cay", code).unwrap();
    let output = compile_and_run_eol("examples/test_generic_args_match.cay")
        .expect("泛型实参一致的调用应编译运行通过");
    assert!(output.contains("ok 1"), "应正常推入元素: {}", output);
    let _ = std::fs::remove_file("examples/test_generic_args_match.cay");
}
