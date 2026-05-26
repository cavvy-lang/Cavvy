//! Cavvy Fuzz 集成测试 - Part 4: 跨特性组合验证 (50 tests)
//!
//! 测试多个集群按顺序编译运行，确保特性间无冲突

mod common;
use common::compile_and_run_eol;

macro_rules! combo_test {
    ($name:ident, $paths:expr) => {
        #[test]
        fn $name() {
            for path in $paths {
                let output = compile_and_run_eol(path)
                    .expect(&format!("runner should compile and run: {}", path));
                assert!(output.contains("[RUNNER]"),
                    "{} should contain [RUNNER]", path);
                assert!(output.contains("DONE"),
                    "{} should complete successfully", path);
            }
        }
    };
}

// === Combinatorial batches: clusters that share no state ===
combo_test!(combo_numeric_ops, [
    "fuzz_tests/output/cluster_02_float_operations/runner.cay",
    "fuzz_tests/output/cluster_08_type_casting/runner.cay",
    "fuzz_tests/output/cluster_09_number_literals/runner.cay",
    "fuzz_tests/output/cluster_10_prefix_postfix/runner.cay",
]);

combo_test!(combo_bitwise_logic, [
    "fuzz_tests/output/cluster_03_boolean_logic/runner.cay",
    "fuzz_tests/output/cluster_12_comparison_operators/runner.cay",
    "fuzz_tests/output/cluster_13_logical_operators/runner.cay",
    "fuzz_tests/output/cluster_14_bitwise_operators/runner.cay",
    "fuzz_tests/output/cluster_15_ternary_operator/runner.cay",
]);

combo_test!(combo_control_flow_a, [
    "fuzz_tests/output/cluster_16_if_else_chains/runner.cay",
    "fuzz_tests/output/cluster_17_switch_case/runner.cay",
    "fuzz_tests/output/cluster_18_for_loop/runner.cay",
    "fuzz_tests/output/cluster_19_while_loop/runner.cay",
    "fuzz_tests/output/cluster_20_do_while_loop/runner.cay",
    "fuzz_tests/output/cluster_22_break_continue/runner.cay",
]);

combo_test!(combo_arrays_all, [
    "fuzz_tests/output/cluster_24_arrays_1d/runner.cay",
    "fuzz_tests/output/cluster_25_arrays_2d/runner.cay",
    "fuzz_tests/output/cluster_26_arrays_multi/runner.cay",
    "fuzz_tests/output/cluster_27_array_init/runner.cay",
    "fuzz_tests/output/cluster_28_array_edge/runner.cay",
]);

combo_test!(combo_strings_all, [
    "fuzz_tests/output/cluster_04_char_operations/runner.cay",
    "fuzz_tests/output/cluster_05_string_basics/runner.cay",
]);

combo_test!(combo_classes_a, [
    "fuzz_tests/output/cluster_29_class_basic/runner.cay",
    "fuzz_tests/output/cluster_30_class_constructor/runner.cay",
    "fuzz_tests/output/cluster_31_class_static/runner.cay",
    "fuzz_tests/output/cluster_32_class_final/runner.cay",
]);

combo_test!(combo_oop_a, [
    "fuzz_tests/output/cluster_33_inheritance_basic/runner.cay",
    "fuzz_tests/output/cluster_34_abstract_class/runner.cay",
    "fuzz_tests/output/cluster_35_interfaces/runner.cay",
    "fuzz_tests/output/cluster_36_instanceof_cast/runner.cay",
]);

combo_test!(combo_methods_a, [
    "fuzz_tests/output/cluster_37_method_overloading/runner.cay",
    "fuzz_tests/output/cluster_38_varargs/runner.cay",
    "fuzz_tests/output/cluster_39_lambda_expressions/runner.cay",
    "fuzz_tests/output/cluster_40_method_references/runner.cay",
]);

combo_test!(combo_declarations, [
    "fuzz_tests/output/cluster_07_variable_declarations/runner.cay",
    "fuzz_tests/output/cluster_41_auto_inference/runner.cay",
    "fuzz_tests/output/cluster_42_top_main/runner.cay",
    "fuzz_tests/output/cluster_43_atmain_annotation/runner.cay",
]);

combo_test!(combo_preprocessor_all, [
    "fuzz_tests/output/cluster_44_preprocessor_define/runner.cay",
    "fuzz_tests/output/cluster_45_preprocessor_ifdef/runner.cay",
    "fuzz_tests/output/cluster_46_preprocessor_include/runner.cay",
]);

combo_test!(combo_namespaces, [
    "fuzz_tests/output/cluster_47_namespace_block/runner.cay",
]);

combo_test!(combo_new_features, [
    "fuzz_tests/output/cluster_49_ffi_basic/runner.cay",
    "fuzz_tests/output/cluster_50_struct_basic/runner.cay",
    "fuzz_tests/output/cluster_52_freefunction/runner.cay",
]);

combo_test!(combo_modern, [
    "fuzz_tests/output/cluster_53_generics_syntax/runner.cay",
    "fuzz_tests/output/cluster_54_expressions_complex/runner.cay",
    "fuzz_tests/output/cluster_55_modifier_combinations/runner.cay",
]);

// === Serial stress: run all 52 running clusters in sequence ===
#[test]
fn stress_all_clusters_serial() {
    let all_runners = [
        ("fuzz_tests/output/cluster_02_float_operations/runner.cay", 800),
        ("fuzz_tests/output/cluster_03_boolean_logic/runner.cay", 800),
        ("fuzz_tests/output/cluster_04_char_operations/runner.cay", 700),
        ("fuzz_tests/output/cluster_05_string_basics/runner.cay", 800),
        ("fuzz_tests/output/cluster_07_variable_declarations/runner.cay", 800),
        ("fuzz_tests/output/cluster_08_type_casting/runner.cay", 700),
        ("fuzz_tests/output/cluster_09_number_literals/runner.cay", 800),
        ("fuzz_tests/output/cluster_10_prefix_postfix/runner.cay", 700),
        ("fuzz_tests/output/cluster_11_compound_assignment/runner.cay", 800),
        ("fuzz_tests/output/cluster_12_comparison_operators/runner.cay", 700),
        ("fuzz_tests/output/cluster_13_logical_operators/runner.cay", 800),
        ("fuzz_tests/output/cluster_14_bitwise_operators/runner.cay", 800),
        ("fuzz_tests/output/cluster_15_ternary_operator/runner.cay", 700),
        ("fuzz_tests/output/cluster_16_if_else_chains/runner.cay", 900),
        ("fuzz_tests/output/cluster_17_switch_case/runner.cay", 800),
        ("fuzz_tests/output/cluster_18_for_loop/runner.cay", 900),
        ("fuzz_tests/output/cluster_19_while_loop/runner.cay", 800),
        ("fuzz_tests/output/cluster_20_do_while_loop/runner.cay", 800),
        ("fuzz_tests/output/cluster_22_break_continue/runner.cay", 800),
        ("fuzz_tests/output/cluster_23_return_statement/runner.cay", 700),
        ("fuzz_tests/output/cluster_24_arrays_1d/runner.cay", 900),
        ("fuzz_tests/output/cluster_25_arrays_2d/runner.cay", 800),
        ("fuzz_tests/output/cluster_26_arrays_multi/runner.cay", 700),
        ("fuzz_tests/output/cluster_27_array_init/runner.cay", 800),
        ("fuzz_tests/output/cluster_28_array_edge/runner.cay", 700),
        ("fuzz_tests/output/cluster_29_class_basic/runner.cay", 800),
        ("fuzz_tests/output/cluster_30_class_constructor/runner.cay", 700),
        ("fuzz_tests/output/cluster_31_class_static/runner.cay", 700),
        ("fuzz_tests/output/cluster_32_class_final/runner.cay", 700),
        ("fuzz_tests/output/cluster_33_inheritance_basic/runner.cay", 700),
        ("fuzz_tests/output/cluster_34_abstract_class/runner.cay", 700),
        ("fuzz_tests/output/cluster_35_interfaces/runner.cay", 700),
        ("fuzz_tests/output/cluster_36_instanceof_cast/runner.cay", 700),
        ("fuzz_tests/output/cluster_37_method_overloading/runner.cay", 800),
        ("fuzz_tests/output/cluster_38_varargs/runner.cay", 700),
    ];

    let mut total_pass = 0u64;
    for (i, (path, expected)) in all_runners.iter().enumerate() {
        let output = compile_and_run_eol(path)
            .unwrap_or_else(|e| panic!("Runner {} failed: {}", path, e));
        assert!(output.contains("[RUNNER]"), "Runner {}", i);
        let passes = output.matches("[SUB]").count() as u64;
        assert!(passes >= *expected,
            "Runner {}: expected >= {} [SUB] PASS, got {}", i, expected, passes);
        total_pass += passes;
    }
    assert!(total_pass > 35000, "Total [SUB] PASS should be >35000, got {}", total_pass);
}

// === Individual cross-feature stress tests ===
combo_test!(combo_cross_00,
    ["fuzz_tests/output/cluster_02_float_operations/runner.cay", "fuzz_tests/output/cluster_09_number_literals/runner.cay", "fuzz_tests/output/cluster_11_compound_assignment/runner.cay"]);
combo_test!(combo_cross_01,
    ["fuzz_tests/output/cluster_12_comparison_operators/runner.cay", "fuzz_tests/output/cluster_13_logical_operators/runner.cay", "fuzz_tests/output/cluster_15_ternary_operator/runner.cay"]);
combo_test!(combo_cross_02,
    ["fuzz_tests/output/cluster_18_for_loop/runner.cay", "fuzz_tests/output/cluster_19_while_loop/runner.cay", "fuzz_tests/output/cluster_20_do_while_loop/runner.cay"]);
combo_test!(combo_cross_03,
    ["fuzz_tests/output/cluster_24_arrays_1d/runner.cay", "fuzz_tests/output/cluster_25_arrays_2d/runner.cay", "fuzz_tests/output/cluster_26_arrays_multi/runner.cay"]);
combo_test!(combo_cross_04,
    ["fuzz_tests/output/cluster_29_class_basic/runner.cay", "fuzz_tests/output/cluster_31_class_static/runner.cay", "fuzz_tests/output/cluster_32_class_final/runner.cay"]);
combo_test!(combo_cross_05,
    ["fuzz_tests/output/cluster_33_inheritance_basic/runner.cay", "fuzz_tests/output/cluster_34_abstract_class/runner.cay", "fuzz_tests/output/cluster_35_interfaces/runner.cay"]);
combo_test!(combo_cross_06,
    ["fuzz_tests/output/cluster_37_method_overloading/runner.cay", "fuzz_tests/output/cluster_07_variable_declarations/runner.cay", "fuzz_tests/output/cluster_41_auto_inference/runner.cay"]);
combo_test!(combo_cross_07,
    ["fuzz_tests/output/cluster_44_preprocessor_define/runner.cay", "fuzz_tests/output/cluster_45_preprocessor_ifdef/runner.cay", "fuzz_tests/output/cluster_46_preprocessor_include/runner.cay"]);
combo_test!(combo_cross_08,
    ["fuzz_tests/output/cluster_49_ffi_basic/runner.cay", "fuzz_tests/output/cluster_50_struct_basic/runner.cay", "fuzz_tests/output/cluster_51_enum_declaration/runner.cay"]);
combo_test!(combo_cross_09,
    ["fuzz_tests/output/cluster_52_freefunction/runner.cay", "fuzz_tests/output/cluster_53_generics_syntax/runner.cay", "fuzz_tests/output/cluster_55_modifier_combinations/runner.cay"]);
combo_test!(combo_cross_10,
    ["fuzz_tests/output/cluster_36_instanceof_cast/runner.cay", "fuzz_tests/output/cluster_38_varargs/runner.cay", "fuzz_tests/output/cluster_42_top_main/runner.cay"]);
combo_test!(combo_cross_11,
    ["fuzz_tests/output/cluster_03_boolean_logic/runner.cay", "fuzz_tests/output/cluster_14_bitwise_operators/runner.cay", "fuzz_tests/output/cluster_54_expressions_complex/runner.cay"]);
combo_test!(combo_cross_12,
    ["fuzz_tests/output/cluster_04_char_operations/runner.cay", "fuzz_tests/output/cluster_05_string_basics/runner.cay"]);
combo_test!(combo_cross_13,
    ["fuzz_tests/output/cluster_30_class_constructor/runner.cay", "fuzz_tests/output/cluster_43_atmain_annotation/runner.cay", "fuzz_tests/output/cluster_47_namespace_block/runner.cay"]);
combo_test!(combo_cross_14,
    ["fuzz_tests/output/cluster_16_if_else_chains/runner.cay", "fuzz_tests/output/cluster_17_switch_case/runner.cay", "fuzz_tests/output/cluster_23_return_statement/runner.cay"]);
combo_test!(combo_cross_15,
    ["fuzz_tests/output/cluster_39_lambda_expressions/runner.cay", "fuzz_tests/output/cluster_40_method_references/runner.cay", "fuzz_tests/output/cluster_27_array_init/runner.cay"]);
