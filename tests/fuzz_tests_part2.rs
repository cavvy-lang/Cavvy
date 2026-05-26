//! Cavvy Fuzz 集成测试 - Part 2 (Clusters 29-55)
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

// ============================================================
// Cluster 29: class_basic (110 fuzz files)
// ============================================================
fuzz_test!(fuzz_c29_class_basic,
    "fuzz_tests/output/cluster_29_class_basic/runner.cay");

// ============================================================
// Cluster 30: class_constructor (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c30_class_constructor,
    "fuzz_tests/output/cluster_30_class_constructor/runner.cay");

// ============================================================
// Cluster 31: class_static (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c31_class_static,
    "fuzz_tests/output/cluster_31_class_static/runner.cay");

// ============================================================
// Cluster 32: class_final (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c32_class_final,
    "fuzz_tests/output/cluster_32_class_final/runner.cay");

// ============================================================
// Cluster 33: inheritance_basic (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c33_inheritance_basic,
    "fuzz_tests/output/cluster_33_inheritance_basic/runner.cay");

// ============================================================
// Cluster 34: abstract_class (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c34_abstract_class,
    "fuzz_tests/output/cluster_34_abstract_class/runner.cay");

// ============================================================
// Cluster 35: interfaces (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c35_interfaces,
    "fuzz_tests/output/cluster_35_interfaces/runner.cay");

// ============================================================
// Cluster 36: instanceof_cast (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c36_instanceof_cast,
    "fuzz_tests/output/cluster_36_instanceof_cast/runner.cay");

// ============================================================
// Cluster 37: method_overloading (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c37_method_overloading,
    "fuzz_tests/output/cluster_37_method_overloading/runner.cay");

// ============================================================
// Cluster 38: varargs (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c38_varargs,
    "fuzz_tests/output/cluster_38_varargs/runner.cay");

// ============================================================
// Cluster 39: lambda_expressions (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c39_lambda_expressions,
    "fuzz_tests/output/cluster_39_lambda_expressions/runner.cay");

// ============================================================
// Cluster 40: method_references (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c40_method_references,
    "fuzz_tests/output/cluster_40_method_references/runner.cay");

// ============================================================
// Cluster 41: auto_inference (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c41_auto_inference,
    "fuzz_tests/output/cluster_41_auto_inference/runner.cay");

// ============================================================
// Cluster 42: top_main (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c42_top_main,
    "fuzz_tests/output/cluster_42_top_main/runner.cay");

// ============================================================
// Cluster 43: atmain_annotation (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c43_atmain_annotation,
    "fuzz_tests/output/cluster_43_atmain_annotation/runner.cay");

// ============================================================
// Cluster 44: preprocessor_define (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c44_preprocessor_define,
    "fuzz_tests/output/cluster_44_preprocessor_define/runner.cay");

// ============================================================
// Cluster 45: preprocessor_ifdef (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c45_preprocessor_ifdef,
    "fuzz_tests/output/cluster_45_preprocessor_ifdef/runner.cay");

// ============================================================
// Cluster 46: preprocessor_include (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c46_preprocessor_include,
    "fuzz_tests/output/cluster_46_preprocessor_include/runner.cay");

// ============================================================
// Cluster 47: namespace_block (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c47_namespace_block,
    "fuzz_tests/output/cluster_47_namespace_block/runner.cay");

// ============================================================
// Cluster 48: namespace_using (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c48_namespace_using,
    "fuzz_tests/output/cluster_48_namespace_using/runner.cay");

// ============================================================
// Cluster 49: ffi_basic (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c49_ffi_basic,
    "fuzz_tests/output/cluster_49_ffi_basic/runner.cay");

// ============================================================
// Cluster 50: struct_basic (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c50_struct_basic,
    "fuzz_tests/output/cluster_50_struct_basic/runner.cay");

// ============================================================
// Cluster 51: enum_declaration (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c51_enum_declaration,
    "fuzz_tests/output/cluster_51_enum_declaration/runner.cay");

// ============================================================
// Cluster 52: freefunction (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c52_freefunction,
    "fuzz_tests/output/cluster_52_freefunction/runner.cay");

// ============================================================
// Cluster 53: generics_syntax (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c53_generics_syntax,
    "fuzz_tests/output/cluster_53_generics_syntax/runner.cay");

// ============================================================
// Cluster 54: expressions_complex (110 fuzz files)
// ============================================================
fuzz_test!(fuzz_c54_expressions_complex,
    "fuzz_tests/output/cluster_54_expressions_complex/runner.cay");

// ============================================================
// Cluster 55: modifier_combinations (100 fuzz files)
// ============================================================
fuzz_test!(fuzz_c55_modifier_combinations,
    "fuzz_tests/output/cluster_55_modifier_combinations/runner.cay");
