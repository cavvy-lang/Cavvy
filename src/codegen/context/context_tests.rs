//! IRGenerator 单元测试（从 context.rs 拆分）
//!
//! 聚焦 Itanium C++ ABI 名称改编、类型签名映射等核心算法。

use crate::codegen::context::IRGenerator;
use crate::types::Type;

/// 验证 Itanium ABI 方法名改编符合 C++ 互操作约定。
/// 时间复杂度 O(p)，p 为参数类型数量；空间复杂度 O(1)（不考虑输出字符串）。
#[test]
fn test_mangle_itanium_method_basic() {
    let generator = IRGenerator::new();

    // HHH::Helper::add(int, int) -> _ZN3HHH6Helper3addEii
    assert_eq!(
        generator.mangle_itanium_method(
            "HHH::Helper",
            "add",
            &[Type::Int32, Type::Int32],
            false,
            false
        ),
        "_ZN3HHH6Helper3addEii"
    );

    // HHH::Inst::getNum() -> _ZN3HHH4Inst6getNumEv
    assert_eq!(
        generator.mangle_itanium_method("HHH::Inst", "getNum", &[], false, false),
        "_ZN3HHH4Inst6getNumEv"
    );

    // HHH::Inst::Inst(int) -> _ZN3HHH4InstC1Ei
    assert_eq!(
        generator.mangle_itanium_method("HHH::Inst", "C1", &[Type::Int32], true, false),
        "_ZN3HHH4InstC1Ei"
    );

    // Object default constructor -> _ZN6ObjectC1Ev
    assert_eq!(
        generator.mangle_itanium_method("Object", "C1", &[], true, false),
        "_ZN6ObjectC1Ev"
    );
}

/// 验证命名空间嵌套与析构函数改编。
#[test]
fn test_mangle_itanium_method_nested_and_dtor() {
    let generator = IRGenerator::new();

    // A::B::C::foo(long long, double) -> _ZN1A1B1C3fooExd
    // Cavvy Int64 映射为 C++ long long（Itanium ABI 编码 'x'），Float64 编码 'd'。
    assert_eq!(
        generator.mangle_itanium_method(
            "A::B::C",
            "foo",
            &[Type::Int64, Type::Float64],
            false,
            false
        ),
        "_ZN1A1B1C3fooExd"
    );

    // A::B::C::~C() -> _ZN1A1B1CD1Ev
    assert_eq!(
        generator.mangle_itanium_method("A::B::C", "D1", &[], false, true),
        "_ZN1A1B1CD1Ev"
    );
}
