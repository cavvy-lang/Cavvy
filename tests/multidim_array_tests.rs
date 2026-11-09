//! 多维数组布局与 length 回归。

mod common;

use common::compile_and_run_eol;

#[test]
fn multidim_array_has_length_and_correct_row_layout() {
    let output = compile_and_run_eol("examples/test_multidim_dijkstra.cay")
        .expect("multidimensional array Dijkstra example should compile and run");
    assert!(
        output.contains("0 2 4 3"),
        "expected shortest distances 0 2 4 3, got: {output}"
    );
}
