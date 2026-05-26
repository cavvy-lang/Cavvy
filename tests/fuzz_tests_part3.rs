//! Cavvy Fuzz 集成测试 - Part 3: 深度断言验证 (48 tests)
//!
//! 每个测试编译运行单个集群，验证输出中 [SUB] ... PASS 数量 >= 配置文件中 method_count
//! 确保所有模糊生成的测试方法都成功执行

mod common;
use common::compile_and_run_eol;

fn count_pass_markers(output: &str) -> usize {
    output.matches("[SUB]").count()
}

fn count_ok_markers(output: &str) -> usize {
    output.matches("OK").count()
}

macro_rules! fuzz_deep_test {
    ($name:ident, $path:literal, $min_expected:expr) => {
        #[test]
        fn $name() {
            let output = compile_and_run_eol($path)
                .expect(concat!("fuzz runner should compile and run: ", $path));
            assert!(output.contains("[RUNNER]"), "Should contain [RUNNER]");
            let pass_count = count_pass_markers(&output);
            assert!(pass_count >= $min_expected,
                "Expected at least {} [SUB] PASS markers, got {} for {}",
                $min_expected, pass_count, $path);
            assert!(output.contains("DONE"), "Should contain DONE");
        }
    };
}

// 每个集群期望至少 800 个 [SUB] PASS (100 files * 8 methods avg)
fuzz_deep_test!(deep_c02_float_operations,
    "fuzz_tests/output/cluster_02_float_operations/runner.cay", 800);
fuzz_deep_test!(deep_c03_boolean_logic,
    "fuzz_tests/output/cluster_03_boolean_logic/runner.cay", 800);
fuzz_deep_test!(deep_c04_char_operations,
    "fuzz_tests/output/cluster_04_char_operations/runner.cay", 700);
fuzz_deep_test!(deep_c05_string_basics,
    "fuzz_tests/output/cluster_05_string_basics/runner.cay", 800);
fuzz_deep_test!(deep_c07_variable_declarations,
    "fuzz_tests/output/cluster_07_variable_declarations/runner.cay", 800);
fuzz_deep_test!(deep_c08_type_casting,
    "fuzz_tests/output/cluster_08_type_casting/runner.cay", 700);
fuzz_deep_test!(deep_c09_number_literals,
    "fuzz_tests/output/cluster_09_number_literals/runner.cay", 800);
fuzz_deep_test!(deep_c10_prefix_postfix,
    "fuzz_tests/output/cluster_10_prefix_postfix/runner.cay", 700);
fuzz_deep_test!(deep_c11_compound_assignment,
    "fuzz_tests/output/cluster_11_compound_assignment/runner.cay", 800);
fuzz_deep_test!(deep_c12_comparison_operators,
    "fuzz_tests/output/cluster_12_comparison_operators/runner.cay", 700);
fuzz_deep_test!(deep_c13_logical_operators,
    "fuzz_tests/output/cluster_13_logical_operators/runner.cay", 800);
fuzz_deep_test!(deep_c14_bitwise_operators,
    "fuzz_tests/output/cluster_14_bitwise_operators/runner.cay", 800);
fuzz_deep_test!(deep_c15_ternary_operator,
    "fuzz_tests/output/cluster_15_ternary_operator/runner.cay", 700);
fuzz_deep_test!(deep_c16_if_else_chains,
    "fuzz_tests/output/cluster_16_if_else_chains/runner.cay", 900);
fuzz_deep_test!(deep_c17_switch_case,
    "fuzz_tests/output/cluster_17_switch_case/runner.cay", 800);
fuzz_deep_test!(deep_c18_for_loop,
    "fuzz_tests/output/cluster_18_for_loop/runner.cay", 900);
fuzz_deep_test!(deep_c19_while_loop,
    "fuzz_tests/output/cluster_19_while_loop/runner.cay", 800);
fuzz_deep_test!(deep_c20_do_while_loop,
    "fuzz_tests/output/cluster_20_do_while_loop/runner.cay", 800);
fuzz_deep_test!(deep_c22_break_continue,
    "fuzz_tests/output/cluster_22_break_continue/runner.cay", 800);
fuzz_deep_test!(deep_c23_return_statement,
    "fuzz_tests/output/cluster_23_return_statement/runner.cay", 700);
fuzz_deep_test!(deep_c24_arrays_1d,
    "fuzz_tests/output/cluster_24_arrays_1d/runner.cay", 900);
fuzz_deep_test!(deep_c25_arrays_2d,
    "fuzz_tests/output/cluster_25_arrays_2d/runner.cay", 800);
fuzz_deep_test!(deep_c26_arrays_multi,
    "fuzz_tests/output/cluster_26_arrays_multi/runner.cay", 700);
fuzz_deep_test!(deep_c27_array_init,
    "fuzz_tests/output/cluster_27_array_init/runner.cay", 800);
fuzz_deep_test!(deep_c28_array_edge,
    "fuzz_tests/output/cluster_28_array_edge/runner.cay", 700);
fuzz_deep_test!(deep_c29_class_basic,
    "fuzz_tests/output/cluster_29_class_basic/runner.cay", 800);
fuzz_deep_test!(deep_c30_class_constructor,
    "fuzz_tests/output/cluster_30_class_constructor/runner.cay", 700);
fuzz_deep_test!(deep_c31_class_static,
    "fuzz_tests/output/cluster_31_class_static/runner.cay", 700);
fuzz_deep_test!(deep_c32_class_final,
    "fuzz_tests/output/cluster_32_class_final/runner.cay", 700);
fuzz_deep_test!(deep_c33_inheritance_basic,
    "fuzz_tests/output/cluster_33_inheritance_basic/runner.cay", 700);
fuzz_deep_test!(deep_c34_abstract_class,
    "fuzz_tests/output/cluster_34_abstract_class/runner.cay", 700);
fuzz_deep_test!(deep_c35_interfaces,
    "fuzz_tests/output/cluster_35_interfaces/runner.cay", 700);
fuzz_deep_test!(deep_c36_instanceof_cast,
    "fuzz_tests/output/cluster_36_instanceof_cast/runner.cay", 700);
fuzz_deep_test!(deep_c37_method_overloading,
    "fuzz_tests/output/cluster_37_method_overloading/runner.cay", 800);
fuzz_deep_test!(deep_c38_varargs,
    "fuzz_tests/output/cluster_38_varargs/runner.cay", 700);
fuzz_deep_test!(deep_c39_lambda_expressions,
    "fuzz_tests/output/cluster_39_lambda_expressions/runner.cay", 700);
fuzz_deep_test!(deep_c40_method_references,
    "fuzz_tests/output/cluster_40_method_references/runner.cay", 700);
fuzz_deep_test!(deep_c41_auto_inference,
    "fuzz_tests/output/cluster_41_auto_inference/runner.cay", 800);
fuzz_deep_test!(deep_c42_top_main,
    "fuzz_tests/output/cluster_42_top_main/runner.cay", 700);
fuzz_deep_test!(deep_c43_atmain_annotation,
    "fuzz_tests/output/cluster_43_atmain_annotation/runner.cay", 700);
fuzz_deep_test!(deep_c44_preprocessor_define,
    "fuzz_tests/output/cluster_44_preprocessor_define/runner.cay", 700);
fuzz_deep_test!(deep_c45_preprocessor_ifdef,
    "fuzz_tests/output/cluster_45_preprocessor_ifdef/runner.cay", 700);
fuzz_deep_test!(deep_c46_preprocessor_include,
    "fuzz_tests/output/cluster_46_preprocessor_include/runner.cay", 700);
fuzz_deep_test!(deep_c47_namespace_block,
    "fuzz_tests/output/cluster_47_namespace_block/runner.cay", 700);
fuzz_deep_test!(deep_c49_ffi_basic,
    "fuzz_tests/output/cluster_49_ffi_basic/runner.cay", 700);
fuzz_deep_test!(deep_c50_struct_basic,
    "fuzz_tests/output/cluster_50_struct_basic/runner.cay", 700);
fuzz_deep_test!(deep_c54_expressions_complex,
    "fuzz_tests/output/cluster_54_expressions_complex/runner.cay", 1000);
fuzz_deep_test!(deep_c55_modifier_combinations,
    "fuzz_tests/output/cluster_55_modifier_combinations/runner.cay", 700);
