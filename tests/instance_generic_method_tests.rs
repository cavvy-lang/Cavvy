//! 实例泛型方法（方法级类型参数 method<U>）的语义分析与代码生成测试
//!
//! 语义级：直接驱动 SemanticAnalyzer，验证方法级类型参数 U 能从 lambda
//! 实参推断（U=long 时把结果赋给 Result<int, String> 必须是类型错误）。
//! 代码生成级：链式调用 r.map(f).getValue() 与 auto 接收复用与发射路径
//! 一致的返回类型推断，lambda 按单态化计划的期望 fn 类型发射签名。

mod common;

use cavvy::semantic::SemanticAnalyzer;
use common::compile_and_run_eol;

const RESULT_CLASS_SRC: &str = r#"
class Result<T, E> {
    private T value;
    private E error;
    public Result(T v, E e) { this.value = v; this.error = e; }
    public T getValue() { return this.value; }
    public Result<U, E> map<U>(fn(T) -> U mapper) {
        return new Result<U, E>(mapper(this.value), this.error);
    }
}
"#;

fn analyze_source(src: &str) -> Result<(), String> {
    let tokens = cavvy::lexer::lex(&src.to_string()).map_err(|e| format!("lex failed: {:?}", e))?;
    let program = cavvy::parser::parse(tokens).map_err(|e| format!("parse failed: {:?}", e))?;
    let mut analyzer = SemanticAnalyzer::new();
    analyzer
        .analyze(program)
        .map(|_| ())
        .map_err(|e| format!("{:?}", e))
}

#[test]
fn semantic_instance_generic_method_infers_method_type_param() {
    // U 从 lambda 返回类型推断为 long：赋给 Result<long, String> 应通过语义分析
    let src = format!(
        "{}\npublic fn main() {{\n    Result<int, String> r = new Result<int, String>(41, \"none\");\n    Result<long, String> r2 = r.map((x) -> (long)x);\n    println(r2.getValue());\n}}",
        RESULT_CLASS_SRC
    );
    analyze_source(&src).expect("U=long 推断后语义分析应通过");
}

#[test]
fn semantic_instance_generic_method_inference_failure_is_clean_error() {
    // 实参无法用于推断方法级类型实参（如 r.map(42)）时，
    // 必须报「类型实参推断失败」的清晰错误，而不是崩溃或静默通过
    let src = format!(
        "{}\npublic fn main() {{\n    Result<int, String> r = new Result<int, String>(41, \"none\");\n    r.map(42);\n}}",
        RESULT_CLASS_SRC
    );
    let err = analyze_source(&src).expect_err("r.map(42) 应报类型实参推断失败");
    assert!(
        err.contains("Cannot infer type arguments"),
        "应报类型实参推断失败，实际: {}",
        err
    );
}

#[test]
fn instance_generic_method_full_output() {
    let output = compile_and_run_eol("examples/test_instance_generic_method.cay")
        .expect("test_instance_generic_method.cay should compile and run");
    common::assert_output_contains(
        &output,
        &["41", "ok", "82", "41.500000", "none", "42"],
        "instance_generic_method_full_output",
    );
}

#[test]
fn instance_generic_method_chained_and_auto() {
    // 链式调用 r.map(f).getValue() 与 auto 接收：外层接收者类型的方法级
    // 类型参数 U 在内层调用点推断（codegen 与发射路径共用同一推断）。
    // U=double 时 lambda 必须按期望签名 double(int) 发射，否则运行期错乱。
    let output = compile_and_run_eol("examples/test_instance_generic_method_chained.cay")
        .expect("chained/auto instance generic method call should compile and run");
    common::assert_output_contains(
        &output,
        &["41", "42", "42", "41.000000", "41.000000"],
        "instance_generic_method_chained_and_auto",
    );
}
