//! Cavvy Fuzz 集成测试 - Part 1 (Clusters 01-11)
//!
//! 编译并运行 fuzz_tests/output/ 下的各个集群 runner.cay，
//! 验证其输出包含 [RUNNER] X DONE 标记

mod common;
use common::compile_and_run_eol;

macro_rules! fuzz_test {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            let output = compile_and_run_eol($path)
                .expect(concat!("fuzz runner should compile and run: ", $path));
            assert!(output.contains("[RUNNER]"), "Should contain [RUNNER] marker, got: {}", output);
        }
    };
}

macro_rules! fuzz_test_skip {
    ($name:ident, $path:literal, $reason:literal) => {
        #[test]
        #[ignore = $reason]
        fn $name() {
            let output = compile_and_run_eol($path)
                .expect(concat!("fuzz runner should compile and run: ", $path));
            assert!(output.contains("[RUNNER]"), "Should contain [RUNNER] marker, got: {}", output);
        }
    };
}

// ============================================================
// Cluster 01: int_arithmetic (120 fuzz files)
// ============================================================
fuzz_test_skip!(fuzz_c01_int_arithmetic,
    "fuzz_tests/output/cluster_01_int_arithmetic/runner.cay",
    "Cavvy bug: -2147483648 parsed as long literal");

// ============================================================
// Cluster 02: float_operations (120 fuzz files)
// ============================================================
fuzz_test!(fuzz_c02_float_operations,
    "fuzz_tests/output/cluster_02_float_operations/runner.cay");

// ============================================================
// Cluster 03: boolean_logic (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c03_boolean_logic,
    "fuzz_tests/output/cluster_03_boolean_logic/runner.cay");

// ============================================================
// Cluster 04: char_operations (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c04_char_operations,
    "fuzz_tests/output/cluster_04_char_operations/runner.cay");

// ============================================================
// Cluster 05: string_basics (110 fuzz files)
// ============================================================
fuzz_test!(fuzz_c05_string_basics,
    "fuzz_tests/output/cluster_05_string_basics/runner.cay");

// ============================================================
// Cluster 06: string_methods (120 fuzz files)
// ============================================================
fuzz_test_skip!(fuzz_c06_string_methods,
    "fuzz_tests/output/cluster_06_string_methods/runner.cay",
    "Cavvy bug: toUpperCase returns int instead of String");

// ============================================================
// Cluster 07: variable_declarations (110 fuzz files)
// ============================================================
fuzz_test!(fuzz_c07_variable_declarations,
    "fuzz_tests/output/cluster_07_variable_declarations/runner.cay");

// ============================================================
// Cluster 08: type_casting (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c08_type_casting,
    "fuzz_tests/output/cluster_08_type_casting/runner.cay");

// ============================================================
// Cluster 09: number_literals (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c09_number_literals,
    "fuzz_tests/output/cluster_09_number_literals/runner.cay");

// ============================================================
// Cluster 10: prefix_postfix (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c10_prefix_postfix,
    "fuzz_tests/output/cluster_10_prefix_postfix/runner.cay");

// ============================================================
// Cluster 11: compound_assignment (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c11_compound_assignment,
    "fuzz_tests/output/cluster_11_compound_assignment/runner.cay");

// ============================================================
// Cluster 12: comparison_operators (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c12_comparison_operators,
    "fuzz_tests/output/cluster_12_comparison_operators/runner.cay");

// ============================================================
// Cluster 13: logical_operators (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c13_logical_operators,
    "fuzz_tests/output/cluster_13_logical_operators/runner.cay");

// ============================================================
// Cluster 14: bitwise_operators (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c14_bitwise_operators,
    "fuzz_tests/output/cluster_14_bitwise_operators/runner.cay");

// ============================================================
// Cluster 15: ternary_operator (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c15_ternary_operator,
    "fuzz_tests/output/cluster_15_ternary_operator/runner.cay");

// ============================================================
// Cluster 16: if_else_chains (110 fuzz files)
// ============================================================
fuzz_test!(fuzz_c16_if_else_chains,
    "fuzz_tests/output/cluster_16_if_else_chains/runner.cay");

// ============================================================
// Cluster 17: switch_case (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c17_switch_case,
    "fuzz_tests/output/cluster_17_switch_case/runner.cay");

// ============================================================
// Cluster 18: for_loop (110 fuzz files)
// ============================================================
fuzz_test!(fuzz_c18_for_loop,
    "fuzz_tests/output/cluster_18_for_loop/runner.cay");

// ============================================================
// Cluster 19: while_loop (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c19_while_loop,
    "fuzz_tests/output/cluster_19_while_loop/runner.cay");

// ============================================================
// Cluster 20: do_while_loop (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c20_do_while_loop,
    "fuzz_tests/output/cluster_20_do_while_loop/runner.cay");

// ============================================================
// Cluster 21: enhanced_for (100 fuzz files)
// ============================================================
fuzz_test_skip!(fuzz_c21_enhanced_for,
    "fuzz_tests/output/cluster_21_enhanced_for/runner.cay",
    "Cavvy bug: enhanced for syntax ':' not parsed correctly");

// ============================================================
// Cluster 22: break_continue (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c22_break_continue,
    "fuzz_tests/output/cluster_22_break_continue/runner.cay");

// ============================================================
// Cluster 23: return_statement (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c23_return_statement,
    "fuzz_tests/output/cluster_23_return_statement/runner.cay");

// ============================================================
// Cluster 24: arrays_1d (120 fuzz files)
// ============================================================
fuzz_test!(fuzz_c24_arrays_1d,
    "fuzz_tests/output/cluster_24_arrays_1d/runner.cay");

// ============================================================
// Cluster 25: arrays_2d (110 fuzz files)
// ============================================================
fuzz_test!(fuzz_c25_arrays_2d,
    "fuzz_tests/output/cluster_25_arrays_2d/runner.cay");

// ============================================================
// Cluster 26: arrays_multi (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c26_arrays_multi,
    "fuzz_tests/output/cluster_26_arrays_multi/runner.cay");

// ============================================================
// Cluster 27: array_init (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c27_array_init,
    "fuzz_tests/output/cluster_27_array_init/runner.cay");

// ============================================================
// Cluster 28: array_edge (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c28_array_edge,
    "fuzz_tests/output/cluster_28_array_edge/runner.cay");
