mod common;
use common::compile_and_run_eol;

#[test]
fn test_enum_integration() {
    let output = compile_and_run_eol("examples/test_enum.cay")
        .expect("enum integration test should compile and run");

    // === 阶段标题 ===
    assert!(
        output.contains("=== enum 集成测试 ==="),
        "Missing test header"
    );

    // --- Color 枚举（无 payload）---
    assert!(
        output.contains("Color.Red -> code: 1"),
        "Color.Red switch matching failed"
    );
    assert!(output.contains("Color test PASS"), "Color enum test failed");

    // --- Result 枚举（带 payload 解构）---
    assert!(
        output.contains("Ok with value: 42"),
        "Result.Ok(42) payload extraction failed"
    );
    assert!(
        output.contains("Result.Ok(42) PASS"),
        "Result.Ok assertion failed"
    );
    assert!(
        output.contains("Err with message: Something went wrong"),
        "Result.Err payload extraction failed"
    );
    assert!(
        output.contains("Result.Err test PASS"),
        "Result.Err assertion failed"
    );

    // --- MyOption 枚举（Some/None）---
    assert!(
        output.contains("MyOption.Some(100)"),
        "MyOption.Some payload extraction failed"
    );
    assert!(
        output.contains("MyOption.Some PASS"),
        "MyOption.Some assertion failed"
    );
    assert!(
        output.contains("MyOption.None matched"),
        "MyOption.None pattern matching failed"
    );
    assert!(
        output.contains("MyOption.None PASS"),
        "MyOption.None assertion failed"
    );

    // --- Status 枚举（数组 + 循环 + switch）---
    assert!(
        output.contains("Status.Active -> Active"),
        "Status.Active switch matching failed"
    );
    assert!(
        output.contains("statuses[0] is Active"),
        "Status array index 0 failed"
    );
    assert!(
        output.contains("statuses[1] is Pending"),
        "Status array index 1 failed"
    );
    assert!(
        output.contains("statuses[2] is Deleted"),
        "Status array index 2 failed"
    );

    // === 总通过标记 ===
    assert!(
        output.contains("=== enum 集成测试全部通过 ==="),
        "Enum integration test did not complete successfully. Full output:\n{}",
        output
    );
}
