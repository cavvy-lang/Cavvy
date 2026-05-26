//! Cavvy Fuzz 集成测试 - Part 5: 输出正确性与边界压力 (60 tests)
//!
//! 验证特定输出内容、无panic检查、组合边界条件

mod common;
use common::compile_and_run_eol;

// === 输出内容精确验证 (10 tests) ===
#[test]
fn exact_output_float_op() {
    let output = compile_and_run_eol("fuzz_tests/output/cluster_02_float_operations/runner.cay").unwrap();
    assert!(output.contains("DONE"));
    assert!(!output.contains("panic"));
    assert!(!output.contains("error"));
}

#[test]
fn exact_output_type_cast() {
    let output = compile_and_run_eol("fuzz_tests/output/cluster_08_type_casting/runner.cay").unwrap();
    assert!(output.contains("DONE"));
    assert!(!output.contains("panic"));
}

#[test]
fn exact_output_number_lit() {
    let output = compile_and_run_eol("fuzz_tests/output/cluster_09_number_literals/runner.cay").unwrap();
    assert!(output.contains("0xFF"));
    assert!(output.contains("0b1"));
    assert!(output.contains("0o77"));
    assert!(output.contains("DONE"));
}

#[test]
fn exact_output_prefix_postfix() {
    let output = compile_and_run_eol("fuzz_tests/output/cluster_10_prefix_postfix/runner.cay").unwrap();
    assert!(output.contains("x="));
    assert!(output.contains("y="));
    assert!(output.contains("DONE"));
}

#[test]
fn exact_output_switch() {
    let output = compile_and_run_eol("fuzz_tests/output/cluster_17_switch_case/runner.cay").unwrap();
    assert!(output.contains("DONE"));
    assert!(!output.contains("FAIL"));
}

#[test]
fn exact_output_for_loop() {
    let output = compile_and_run_eol("fuzz_tests/output/cluster_18_for_loop/runner.cay").unwrap();
    assert!(output.contains("DONE"));
}

#[test]
fn exact_output_while_loop() {
    let output = compile_and_run_eol("fuzz_tests/output/cluster_19_while_loop/runner.cay").unwrap();
    assert!(output.contains("DONE"));
}

#[test]
fn exact_output_dowhile() {
    let output = compile_and_run_eol("fuzz_tests/output/cluster_20_do_while_loop/runner.cay").unwrap();
    assert!(output.contains("DONE"));
}

#[test]
fn exact_output_break() {
    let output = compile_and_run_eol("fuzz_tests/output/cluster_22_break_continue/runner.cay").unwrap();
    assert!(output.contains("DONE"));
    assert!(output.contains("--"));
}

#[test]
fn exact_output_arrays_1d() {
    let output = compile_and_run_eol("fuzz_tests/output/cluster_24_arrays_1d/runner.cay").unwrap();
    assert!(output.contains("sum="));
    assert!(output.contains("DONE"));
}

// === 无崩溃验证 (15 tests) ===
macro_rules! no_crash_test {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            let output = compile_and_run_eol($path).unwrap();
            assert!(output.contains("DONE"), "{} should complete", $path);
            assert!(!output.contains("Segmentation fault"));
            assert!(!output.contains("access violation"));
        }
    };
}

no_crash_test!(no_crash_compound_assign, "fuzz_tests/output/cluster_11_compound_assignment/runner.cay");
no_crash_test!(no_crash_comparison, "fuzz_tests/output/cluster_12_comparison_operators/runner.cay");
no_crash_test!(no_crash_logical, "fuzz_tests/output/cluster_13_logical_operators/runner.cay");
no_crash_test!(no_crash_bitwise, "fuzz_tests/output/cluster_14_bitwise_operators/runner.cay");
no_crash_test!(no_crash_ternary, "fuzz_tests/output/cluster_15_ternary_operator/runner.cay");
no_crash_test!(no_crash_ifelse, "fuzz_tests/output/cluster_16_if_else_chains/runner.cay");
no_crash_test!(no_crash_ret, "fuzz_tests/output/cluster_23_return_statement/runner.cay");
no_crash_test!(no_crash_arr2d, "fuzz_tests/output/cluster_25_arrays_2d/runner.cay");
no_crash_test!(no_crash_arr_multi, "fuzz_tests/output/cluster_26_arrays_multi/runner.cay");
no_crash_test!(no_crash_arr_init, "fuzz_tests/output/cluster_27_array_init/runner.cay");
no_crash_test!(no_crash_arr_edge, "fuzz_tests/output/cluster_28_array_edge/runner.cay");
no_crash_test!(no_crash_class_base, "fuzz_tests/output/cluster_29_class_basic/runner.cay");
no_crash_test!(no_crash_ctor, "fuzz_tests/output/cluster_30_class_constructor/runner.cay");
no_crash_test!(no_crash_static, "fuzz_tests/output/cluster_31_class_static/runner.cay");
no_crash_test!(no_crash_final, "fuzz_tests/output/cluster_32_class_final/runner.cay");

// === 确定性验证: 两次运行输出一致 (5 tests) ===
macro_rules! deterministic_test {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            let out1 = compile_and_run_eol($path).unwrap();
            let out2 = compile_and_run_eol($path).unwrap();
            assert_eq!(out1, out2, "Output should be deterministic for {}", $path);
        }
    };
}

deterministic_test!(det_var_decls, "fuzz_tests/output/cluster_07_variable_declarations/runner.cay");
deterministic_test!(det_boolean, "fuzz_tests/output/cluster_03_boolean_logic/runner.cay");
deterministic_test!(det_char, "fuzz_tests/output/cluster_04_char_operations/runner.cay");
deterministic_test!(det_string, "fuzz_tests/output/cluster_05_string_basics/runner.cay");
deterministic_test!(det_auto_inf, "fuzz_tests/output/cluster_41_auto_inference/runner.cay");

// === [TEST] 与 [SUB] 标记一致性 (10 tests) ===
macro_rules! marker_consistency {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            let output = compile_and_run_eol($path).unwrap();
            let test_count = output.matches("[TEST]").count();
            let sub_count = output.matches("[SUB]").count();
            assert!(test_count > 50, "{} should have >50 [TEST] markers, got {}", stringify!($name), test_count);
            assert!(sub_count > (test_count as f64 * 0.5) as usize,
                "{} [SUB] markers ({}) should be >= 50% of [TEST] ({})",
                stringify!($name), sub_count, test_count);
        }
    };
}

marker_consistency!(marker_float, "fuzz_tests/output/cluster_02_float_operations/runner.cay");
marker_consistency!(marker_bool, "fuzz_tests/output/cluster_03_boolean_logic/runner.cay");
marker_consistency!(marker_str, "fuzz_tests/output/cluster_05_string_basics/runner.cay");
marker_consistency!(marker_cast, "fuzz_tests/output/cluster_08_type_casting/runner.cay");
marker_consistency!(marker_lit, "fuzz_tests/output/cluster_09_number_literals/runner.cay");
marker_consistency!(marker_pp, "fuzz_tests/output/cluster_10_prefix_postfix/runner.cay");
marker_consistency!(marker_ca, "fuzz_tests/output/cluster_11_compound_assignment/runner.cay");
marker_consistency!(marker_cmp, "fuzz_tests/output/cluster_12_comparison_operators/runner.cay");
marker_consistency!(marker_arr1d, "fuzz_tests/output/cluster_24_arrays_1d/runner.cay");
marker_consistency!(marker_auto, "fuzz_tests/output/cluster_41_auto_inference/runner.cay");

// === 大型集群边界压测 (12 tests) ===
macro_rules! large_cluster_test {
    ($name:ident, $path:literal, $min_files:expr) => {
        #[test]
        fn $name() {
            let output = compile_and_run_eol($path).unwrap();
            assert!(output.contains("[RUNNER]"));
            assert!(output.contains("DONE"));
            let files_tested = output.matches("START").count();
            assert!(files_tested >= $min_files, "{} should test >= {} files, got {}", $path, $min_files, files_tested);
        }
    };
}

large_cluster_test!(large_float, "fuzz_tests/output/cluster_02_float_operations/runner.cay", 100);
large_cluster_test!(large_bool, "fuzz_tests/output/cluster_03_boolean_logic/runner.cay", 80);
large_cluster_test!(large_string_basic, "fuzz_tests/output/cluster_05_string_basics/runner.cay", 90);
large_cluster_test!(large_literals, "fuzz_tests/output/cluster_09_number_literals/runner.cay", 80);
large_cluster_test!(large_for_loop, "fuzz_tests/output/cluster_18_for_loop/runner.cay", 90);
large_cluster_test!(large_ifelse, "fuzz_tests/output/cluster_16_if_else_chains/runner.cay", 90);
large_cluster_test!(large_switch, "fuzz_tests/output/cluster_17_switch_case/runner.cay", 80);
large_cluster_test!(large_arr1d, "fuzz_tests/output/cluster_24_arrays_1d/runner.cay", 100);
large_cluster_test!(large_arr2d, "fuzz_tests/output/cluster_25_arrays_2d/runner.cay", 90);
large_cluster_test!(large_class, "fuzz_tests/output/cluster_29_class_basic/runner.cay", 90);
large_cluster_test!(large_overload, "fuzz_tests/output/cluster_37_method_overloading/runner.cay", 80);
large_cluster_test!(large_expr, "fuzz_tests/output/cluster_54_expressions_complex/runner.cay", 90);

// === 快速回归冒烟 (8 tests) ===
macro_rules! smoke_test {
    ($name:ident, $path:literal) => {
        #[test]
        fn $name() {
            let output = compile_and_run_eol($path).unwrap();
            assert!(output.len() > 100, "{} should produce substantial output", $path);
            assert!(output.contains("[SUB]"), "{} should have [SUB] markers", $path);
        }
    };
}

smoke_test!(smoke_inherit, "fuzz_tests/output/cluster_33_inheritance_basic/runner.cay");
smoke_test!(smoke_abstract, "fuzz_tests/output/cluster_34_abstract_class/runner.cay");
smoke_test!(smoke_interfaces, "fuzz_tests/output/cluster_35_interfaces/runner.cay");
smoke_test!(smoke_preproc_def, "fuzz_tests/output/cluster_44_preprocessor_define/runner.cay");
smoke_test!(smoke_preproc_ifdef, "fuzz_tests/output/cluster_45_preprocessor_ifdef/runner.cay");
smoke_test!(smoke_ns_block, "fuzz_tests/output/cluster_47_namespace_block/runner.cay");
smoke_test!(smoke_modifiers, "fuzz_tests/output/cluster_55_modifier_combinations/runner.cay");
smoke_test!(smoke_atmain, "fuzz_tests/output/cluster_43_atmain_annotation/runner.cay");
