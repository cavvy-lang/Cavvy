//! 引导编译器（CavvyN35 开发期发现）Bug 台账回归测试。
//!
//! 覆盖 8 个 bug 的端到端行为，用例源码位于 `examples/bootstrap_bugs/`：
//! - BUG-001 三元表达式分支类型统一（i32/i64 混合）
//! - BUG-002 switch case 支持 `类名.静态常量`
//! - BUG-003 panic() 与 c/stdlib.cay 的 abort 声明共存
//! - BUG-004 enum 的 char 载荷零扩展
//! - BUG-005 enum 作为 class/struct 字段的布局
//! - BUG-006 命名空间类静态工厂返回值上的链式调用 mangling
//! - BUG-007 内联向下转型作为字段访问接收者
//! - BUG-008 ArrayList 局部/返回值为 null 时的析构 null 守卫

mod common;

use common::assert_output_contains;
use common::compile_and_run_eol;
use common::compile_and_run_expect_error;

/// BUG-001：三元表达式的 then/else 分支分别为 i32 与 i64 字面量时，
/// 旧实现直接以 then 分支类型生成 phi，llc 报
/// "'%t' defined with type 'i64' but expected 'i32'"。
#[test]
fn test_bug001_ternary_branch_type_unify() {
    let output = compile_and_run_eol("examples/bootstrap_bugs/bug001_ternary_type_unify.cay")
        .expect("BUG-001: 三元表达式应能编译运行");
    assert_output_contains(&output, &["BUG-001 OK"], "test_bug001_ternary_branch_type_unify");
}

/// BUG-002：`case 类名.静态常量:` 旧实现报
/// "E5004 未知的 enum 'TokenType' 在 case 标签中"；
/// 修复后 static final 常量（含常量表达式/负常量）可作为 case 标签，
/// enum variant 标签行为保持不变。
#[test]
fn test_bug002_switch_class_static_const() {
    let output = compile_and_run_eol("examples/bootstrap_bugs/bug002_switch_class_const.cay")
        .expect("BUG-002: 类静态常量 case 标签应能编译运行");
    assert_output_contains(&output, &["BUG-002 OK"], "test_bug002_switch_class_static_const");
}

/// BUG-003：c/stdlib.cay 的 `void abort();`（带属性组 #0）与 panic 内置路径
/// 生成的 `declare void @abort()` 冲突，llc 报 "invalid redefinition of
/// function 'abort'"。修复后能编译，且运行时确实打印 panic 消息并 abort。
#[test]
fn test_bug003_panic_with_stdlib_abort() {
    let err = compile_and_run_expect_error(
        "examples/bootstrap_bugs/bug003_panic_with_stdlib_abort.cay",
    )
    .expect("BUG-003: panic 应导致运行时终止");
    assert_output_contains(&err, &["expected-abort"], "test_bug003_panic_with_stdlib_abort");
}

/// BUG-004：enum 的 char（i8）载荷旧实现直接 insertvalue 进 i64 槽，
/// llc 报 "'%t' defined with type 'i8' but expected 'i64'"。
/// 修复后 i1/i8 零扩展、i16 符号扩展。
#[test]
fn test_bug004_enum_char_payload() {
    let output = compile_and_run_eol("examples/bootstrap_bugs/bug004_enum_char_payload.cay")
        .expect("BUG-004: char 载荷 enum 应能编译运行");
    assert_output_contains(&output, &["BUG-004 OK"], "test_bug004_enum_char_payload");
}

/// BUG-005：enum 字段按语义类型尺寸（8 字节）而非运行时尺寸（16 字节）布局，
/// class 字段越界写破坏堆（malloc(): corrupted top size），struct 相邻字段读脏。
/// 用例通过 1000 次构造触发 malloc 完整性断言并校验相邻字段。
#[test]
fn test_bug005_enum_field_layout() {
    let output = compile_and_run_eol("examples/bootstrap_bugs/bug005_enum_field_layout.cay")
        .expect("BUG-005: enum 字段布局应正确");
    assert_output_contains(&output, &["BUG-005 OK"], "test_bug005_enum_field_layout");
}

/// BUG-006：命名空间类（std::StringBuilder / ns::Maker）静态工厂返回值上的
/// 链式实例方法调用，旧实现丢失命名空间前缀生成 `@_ZN13StringBuilder8toStringEv`
/// （无 this，未定义符号）。修复后接收者类名规范化为注册表键，与生成顺序无关。
#[test]
fn test_bug006_namespace_chain_call() {
    let output = compile_and_run_eol("examples/bootstrap_bugs/bug006_namespace_chain_call.cay")
        .expect("BUG-006: 命名空间链式调用应能编译运行");
    assert_output_contains(&output, &["BUG-006 OK"], "test_bug006_namespace_chain_call");
}

/// BUG-007：`((B)a).field` 内联 cast 直接做字段访问接收者时，旧实现不读字段
/// 而是返回对象指针本身（比较时报 i8* 与 i32 类型错误）。修复后
/// 字段访问接收者类型推断覆盖 cast 表达式并读取正确字段。
#[test]
fn test_bug007_inline_cast_receiver() {
    let output = compile_and_run_eol("examples/bootstrap_bugs/bug007_inline_cast_receiver.cay")
        .expect("BUG-007: 内联 cast 接收者字段访问应正确");
    assert_output_contains(&output, &["BUG-007 OK"], "test_bug007_inline_cast_receiver");
}

/// BUG-008：局部/返回值为 null 的 std::ArrayList 在作用域退出时旧实现直接调用
/// `~ArrayList()`（空 this 解引用）→ SIGSEGV。修复后析构前有空守卫，
/// 非 null 析构路径保持不变。
#[test]
fn test_bug008_arraylist_null_dtor() {
    let output = compile_and_run_eol("examples/bootstrap_bugs/bug008_arraylist_null_dtor.cay")
        .expect("BUG-008: null ArrayList 析构不应崩溃");
    assert_output_contains(&output, &["BUG-008 OK"], "test_bug008_arraylist_null_dtor");
}