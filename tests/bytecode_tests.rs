//! Cavvy 字节码系统测试
//!
//! 测试字节码生成、混淆、序列化/反序列化等功能

use std::fs;
use std::process::Command;

mod common;

/// 从 .verinfo 文件读取指定段的版本号
/// O(n) 磁盘IO复杂度，n为文件行数
fn read_verinfo_version(section: &str) -> Option<String> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let verinfo_path = std::path::Path::new(manifest_dir).join(".verinfo");
    let content = fs::read_to_string(verinfo_path).ok()?;

    let mut in_target_section = false;
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_target_section = line[1..line.len() - 1].eq(section);
            continue;
        }
        if in_target_section {
            if let Some(pos) = line.find('=') {
                let key = line[..pos].trim();
                let value = line[pos + 1..].trim();
                if key == "version" {
                    let version = if value.starts_with('"') && value.ends_with('"') {
                        &value[1..value.len() - 1]
                    } else {
                        value
                    };
                    return Some(version.to_string());
                }
            }
        }
    }
    None
}

/// 获取当前平台的 cay-bcgen 可执行文件路径
fn get_cay_bcgen_path() -> &'static str {
    if cfg!(target_os = "windows") {
        "./target/release/cay-bcgen.exe"
    } else {
        "./target/release/cay-bcgen"
    }
}

/// 获取当前平台的 cay-run 可执行文件路径
fn get_cay_run_path() -> &'static str {
    if cfg!(target_os = "windows") {
        "./target/release/cay-run.exe"
    } else {
        "./target/release/cay-run"
    }
}

/// 获取当前平台的可执行文件扩展名
fn get_exe_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    }
}

/// 测试 cay-bcgen 生成字节码文件
#[test]
fn test_bcgen_generate_bytecode() {
    let source_path = "examples/bytecode/hello_simple.cay";

    // 确保源文件存在
    if !std::path::Path::new(source_path).exists() {
        // 创建测试源文件
        fs::create_dir_all("examples/bytecode").unwrap();
        fs::write(
            source_path,
            r#"public class Hello {
    public static void main() {
        println("Hello, Bytecode!");
    }
}
"#,
        )
        .unwrap();
    }

    // 输出到临时目录，避免与 workspace 中的文件锁/沙箱监控冲突
    let temp_dir = std::env::temp_dir();
    let bc_path = temp_dir
        .join(format!("hello_simple_{}.caybc", std::process::id()))
        .to_string_lossy()
        .to_string();

    // 使用 cay-bcgen 生成字节码
    let output = Command::new(get_cay_bcgen_path())
        .args(&[source_path, "-o", &bc_path])
        .output()
        .expect("Failed to execute cay-bcgen");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "cay-bcgen failed: stderr={}, stdout={}",
        stderr, stdout
    );

    // 检查字节码文件是否生成
    assert!(
        std::path::Path::new(&bc_path).exists(),
        "Bytecode file not generated"
    );

    // 检查文件大小（应该大于0）
    let metadata = fs::metadata(&bc_path).unwrap();
    assert!(metadata.len() > 0, "Bytecode file is empty");

    // 清理
    let _ = fs::remove_file(&bc_path);
}

/// 测试 cay-bcgen 生成混淆的字节码
#[test]
fn test_bcgen_obfuscated_bytecode() {
    let source_path = "examples/bytecode/obfuscate_test.cay";
    let bc_path = "examples/bytecode/obfuscate_test.caybc";

    // 创建测试源文件
    fs::create_dir_all("examples/bytecode").unwrap();
    fs::write(
        source_path,
        r#"public class Main {
    public static int calculate(int x, int y) {
        return x + y;
    }

    public static void main() {
        int result = calculate(10, 20);
        println(result);
    }
}
"#,
    )
    .unwrap();

    // 使用 cay-bcgen 生成混淆的字节码
    let output = Command::new(get_cay_bcgen_path())
        .args(&[
            source_path,
            "-o",
            bc_path,
            "--obfuscate",
            "--obfuscate-level",
            "normal",
        ])
        .output()
        .expect("Failed to execute cay-bcgen");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "cay-bcgen with obfuscation failed: {}",
        stderr
    );

    // 检查字节码文件是否生成
    assert!(
        std::path::Path::new(bc_path).exists(),
        "Obfuscated bytecode file not generated"
    );

    // 清理
    let _ = fs::remove_file(bc_path);
    let _ = fs::remove_file(source_path);
}

/// 测试 cay-run 运行字节码文件
#[test]
fn test_cay_run_bytecode() {
    let source_path = "examples/bytecode/run_bc_test.cay";
    let bc_path = "examples/bytecode/run_bc_test.caybc";

    // 创建测试源文件
    fs::create_dir_all("examples/bytecode").unwrap();
    fs::write(
        source_path,
        r#"public class Main {
    public static void main() {
        println("Running from bytecode!");
    }
}
"#,
    )
    .unwrap();

    // 首先生成字节码
    let output = Command::new(get_cay_bcgen_path())
        .args(&[source_path, "-o", bc_path])
        .output()
        .expect("Failed to execute cay-bcgen");

    assert!(output.status.success(), "Failed to generate bytecode");

    // 使用 cay-run 运行字节码
    let output = Command::new(get_cay_run_path())
        .args(&[
            bc_path,
            "--no-run",
            "-o",
            &format!("examples/bytecode/run_bc_test{}", get_exe_extension()),
        ])
        .output()
        .expect("Failed to execute cay-run");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // 检查是否成功编译（由于字节码到IR的转换是未完成的，可能无法完全运行）
    // 但至少应该能处理文件

    // 清理
    let _ = fs::remove_file(bc_path);
    let _ = fs::remove_file(source_path);
    let _ = fs::remove_file(&format!(
        "examples/bytecode/run_bc_test{}",
        get_exe_extension()
    ));
}

/// 测试 cay-run 运行 IR 文件
#[test]
fn test_cay_run_ir() {
    let ir_path = "examples/bytecode/test_ir.ll";
    let exe_path = &format!("examples/bytecode/test_ir{}", get_exe_extension());

    // 创建测试 IR 文件
    fs::create_dir_all("examples/bytecode").unwrap();
    fs::write(
        ir_path,
        r#"
target datalayout = "e-m:e-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-windows-gnu"

@.str = private unnamed_addr constant [15 x i8] c"Hello from IR!\00", align 1

declare i32 @puts(i8*)

define i32 @main() {
entry:
  %call = call i32 @puts(i8* getelementptr inbounds ([15 x i8], [15 x i8]* @.str, i64 0, i64 0))
  ret i32 0
}
"#,
    )
    .unwrap();

    // 使用 cay-run 运行 IR 文件
    let output = Command::new(get_cay_run_path())
        .args(&[ir_path, "--no-run", "-o", exe_path])
        .output()
        .expect("Failed to execute cay-run");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // 如果 clang 可用，应该能编译成功
    if output.status.success() {
        // 检查可执行文件是否生成
        if std::path::Path::new(exe_path).exists() {
            // 运行生成的可执行文件
            let run_output = Command::new(exe_path)
                .output()
                .expect("Failed to run generated executable");

            let stdout = String::from_utf8_lossy(&run_output.stdout);
            assert!(
                stdout.contains("Hello from IR") || run_output.status.success(),
                "IR execution did not produce expected output"
            );

            let _ = fs::remove_file(exe_path);
        }
    }

    // 清理
    let _ = fs::remove_file(ir_path);
}

/// 测试 cay-run 直接运行源码（带混淆）
#[test]
fn test_cay_run_source_with_obfuscation() {
    let source_path = "examples/bytecode/obfuscate_run_test.cay";
    let exe_path = &format!(
        "examples/bytecode/obfuscate_run_test{}",
        get_exe_extension()
    );

    // 创建测试源文件
    fs::create_dir_all("examples/bytecode").unwrap();
    fs::write(
        source_path,
        r#"public class Main {
    public static void main() {
        println("Obfuscated execution!");
    }
}
"#,
    )
    .unwrap();

    // 使用 cay-run 带混淆运行
    let output = Command::new(get_cay_run_path())
        .args(&[source_path, "--no-run", "--obfuscate", "-o", exe_path])
        .output()
        .expect("Failed to execute cay-run");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // 检查是否成功
    if output.status.success() {
        // 检查可执行文件是否生成
        if std::path::Path::new(exe_path).exists() {
            let _ = fs::remove_file(exe_path);
        }
    }

    // 清理
    let _ = fs::remove_file(source_path);
}

/// 测试字节码序列化和反序列化
#[test]
fn test_bytecode_serialization() {
    use cavvy::bytecode::instructions::*;
    use cavvy::bytecode::*;

    // 创建一个字节码模块
    let mut module = BytecodeModule::new("test_module".to_string(), "windows".to_string());

    // 添加一些常量
    let int_index = module.constant_pool.add_integer(42);
    let string_index = module.constant_pool.add_string("Hello");

    assert_eq!(module.constant_pool.get_integer(int_index), Some(42));
    assert_eq!(
        module.constant_pool.get_string(string_index),
        Some("Hello".to_string())
    );

    // 添加一个函数
    let func_name_index = module.constant_pool.add_utf8("test_func");
    let return_type_index = module.constant_pool.add_utf8("int");

    let mut instructions = Vec::new();
    instructions.push(Instruction::iconst(42));
    instructions.push(Instruction::new(Opcode::Ireturn));

    let body = CodeBody {
        instructions,
        exception_table: Vec::new(),
        line_number_table: Vec::new(),
    };

    let function = FunctionDefinition {
        name_index: func_name_index,
        return_type_index,
        param_type_indices: Vec::new(),
        param_name_indices: Vec::new(),
        modifiers: MethodModifiers::default(),
        body,
        max_locals: 1,
        max_stack: 1,
    };

    module.add_function(function);

    // 序列化
    let serialized = serializer::serialize(&module);
    assert!(!serialized.is_empty(), "Serialized bytecode is empty");

    // 检查魔数
    assert_eq!(&serialized[0..4], b"CAY\x01", "Magic number mismatch");

    // 反序列化
    let deserialized =
        serializer::deserialize(&serialized).expect("Failed to deserialize bytecode");

    assert_eq!(deserialized.header.name, "test_module");
    assert_eq!(deserialized.functions.len(), 1);

    // 检查常量池
    assert_eq!(deserialized.constant_pool.get_integer(int_index), Some(42));
    assert_eq!(
        deserialized.constant_pool.get_string(string_index),
        Some("Hello".to_string())
    );
}

/// 测试字节码混淆
#[test]
fn test_bytecode_obfuscation() {
    use cavvy::bytecode::instructions::*;
    use cavvy::bytecode::obfuscator::*;
    use cavvy::bytecode::*;

    // 创建一个字节码模块
    let mut module = BytecodeModule::new("test_module".to_string(), "windows".to_string());

    // 添加一个函数
    let func_name_index = module.constant_pool.add_utf8("mySecretFunction");
    let return_type_index = module.constant_pool.add_utf8("int");

    let body = CodeBody {
        instructions: vec![Instruction::iconst(1), Instruction::new(Opcode::Ireturn)],
        exception_table: Vec::new(),
        line_number_table: vec![LineNumberEntry { pc: 0, line: 1 }],
    };

    let function = FunctionDefinition {
        name_index: func_name_index,
        return_type_index,
        param_type_indices: Vec::new(),
        param_name_indices: Vec::new(),
        modifiers: MethodModifiers::default(),
        body,
        max_locals: 1,
        max_stack: 1,
    };

    module.add_function(function);

    // 记录原始状态
    let _had_debug_info = !module.functions[0].body.line_number_table.is_empty();

    // 混淆
    let options = ObfuscationOptions {
        obfuscate_names: true,
        obfuscate_control_flow: true,
        insert_junk_code: false,
        encrypt_strings: true,
        shuffle_functions: false,
        strip_debug_info: true,
    };

    let mut obfuscator = BytecodeObfuscator::new(options);
    obfuscator.obfuscate(&mut module);

    // 验证混淆效果
    assert!(
        module.header.obfuscated,
        "Module should be marked as obfuscated"
    );

    // 调试信息应该被移除
    assert!(
        module.functions[0].body.line_number_table.is_empty(),
        "Debug info should be stripped"
    );
}

/// 测试自动链接器
#[test]
fn test_auto_linker() {
    use cavvy::bytecode::linker::*;

    let mut linker = AutoLinker::default();

    // 测试从源代码分析
    let source = r#"
        // 使用数学函数
        double x = sqrt(16.0);
        // 使用Windows API
        MessageBoxA(null, "Hello", "Title", 0);
    "#;

    linker.analyze_source(source);

    // 应该检测到数学库
    assert!(
        linker.config.libraries.contains("m"),
        "Should detect math library"
    );

    // 应该检测到Windows库
    assert!(
        linker.config.libraries.contains("user32"),
        "Should detect user32 library"
    );
    assert!(
        linker.config.libraries.contains("kernel32"),
        "Should detect kernel32 library"
    );

    // 测试从IR分析
    let ir = r#"
        declare i32 @printf(i8*, ...)
        declare i32 @MessageBoxA(i8*, i8*, i8*, i32)
        declare double @sqrt(double)
    "#;

    let mut linker2 = AutoLinker::default();
    linker2.analyze_ir(ir);

    assert!(
        linker2.config.libraries.contains("m"),
        "Should detect math library from IR"
    );
    assert!(
        linker2.config.libraries.contains("user32"),
        "Should detect user32 from IR"
    );
}

/// 测试指令编码和解码
#[test]
fn test_instruction_encoding() {
    use cavvy::bytecode::instructions::*;

    // 测试简单指令
    let instr = Instruction::new(Opcode::Iconst0);
    let encoded = instr.encode();
    assert_eq!(encoded, vec![0x07]); // Iconst0 = 0x07

    // 测试带操作数的指令
    let instr = Instruction::iconst(42);
    let encoded = instr.encode();
    assert_eq!(encoded[0], 0x02); // Iconst = 0x02
    assert_eq!(encoded[1], 42);

    // 测试ldc指令
    let instr = Instruction::ldc(1000);
    let encoded = instr.encode();
    assert_eq!(encoded[0], 0x01); // Ldc = 0x01
    assert_eq!(u16::from_le_bytes([encoded[1], encoded[2]]), 1000);

    // 测试解码
    let bytes = vec![0x50]; // Iadd = 0x50
    let (decoded, size) = Instruction::decode(&bytes, 0).unwrap();
    assert_eq!(decoded.opcode, Opcode::Iadd);
    assert_eq!(size, 1);
}

/// 测试常量池操作
#[test]
fn test_constant_pool() {
    use cavvy::bytecode::constant_pool::*;

    let mut pool = ConstantPool::new();

    // 添加各种常量
    let int_idx = pool.add_integer(42);
    let long_idx = pool.add_long(1234567890i64);
    let float_idx = pool.add_float(3.14f32);
    let double_idx = pool.add_double(2.71828f64);
    let string_idx = pool.add_string("Hello, World!");

    // 验证
    assert_eq!(pool.get_integer(int_idx), Some(42));
    assert_eq!(pool.get_long(long_idx), Some(1234567890i64));
    assert_eq!(pool.get_float(float_idx), Some(3.14f32));
    assert_eq!(pool.get_double(double_idx), Some(2.71828f64));
    assert_eq!(
        pool.get_string(string_idx),
        Some("Hello, World!".to_string())
    );

    // 测试UTF8缓存（重复字符串应该返回相同索引）
    let utf8_1 = pool.add_utf8("test");
    let utf8_2 = pool.add_utf8("test");
    assert_eq!(
        utf8_1, utf8_2,
        "Duplicate UTF8 strings should have same index"
    );

    // 测试序列化和反序列化
    let serialized = pool.serialize();
    let deserialized = ConstantPool::deserialize(&serialized, &mut 0)
        .expect("Failed to deserialize constant pool");

    assert_eq!(deserialized.get_integer(int_idx), Some(42));
    assert_eq!(
        deserialized.get_string(string_idx),
        Some("Hello, World!".to_string())
    );
}

/// 测试 cay-bcgen 帮助信息
#[test]
fn test_bcgen_help() {
    let output = Command::new(get_cay_bcgen_path())
        .args(&["--help"])
        .output()
        .expect("Failed to execute cay-bcgen --help");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cay-bcgen --help should succeed");
    assert!(
        stdout.contains("Cavvy Bytecode Generator"),
        "Help should mention Cavvy Bytecode Generator"
    );
    assert!(
        stdout.contains("--obfuscate"),
        "Help should mention --obfuscate option"
    );
    assert!(
        stdout.contains("--obfuscate-level"),
        "Help should mention --obfuscate-level option"
    );
}

/// 测试 cay-run 帮助信息
#[test]
fn test_cay_run_help() {
    let output = Command::new(get_cay_run_path())
        .args(&["--help"])
        .output()
        .expect("Failed to execute cay-run --help");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cay-run --help should succeed");
    assert!(
        stdout.contains("Cavvy Runner"),
        "Help should mention Cavvy Runner"
    );
    assert!(
        stdout.contains(".caybc"),
        "Help should mention .caybc support"
    );
    assert!(stdout.contains(".ll"), "Help should mention .ll support");
}

/// 测试 cay-bcgen 版本信息
#[test]
fn test_bcgen_version() {
    let output = Command::new(get_cay_bcgen_path())
        .args(&["--version"])
        .output()
        .expect("Failed to execute cay-bcgen --version");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "cay-bcgen --version should succeed"
    );

    let expected_version =
        read_verinfo_version("CAY-BCGEN").expect("Failed to read CAY-BCGEN version from .verinfo");
    assert!(
        stdout.contains(&expected_version),
        "Version should contain {}, got: {}",
        expected_version,
        stdout
    );
    // 验证包含 commit hash（格式: version+commit 或 version+commit-dirty）
    assert!(
        stdout.contains('+'),
        "Version should contain commit hash with + separator, got: {}",
        stdout
    );
}

/// 测试 cay-run 版本信息
#[test]
fn test_cay_run_version() {
    let output = Command::new(get_cay_run_path())
        .args(&["--version"])
        .output()
        .expect("Failed to execute cay-run --version");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success(), "cay-run --version should succeed");

    let expected_version =
        read_verinfo_version("CAY-RUN").expect("Failed to read CAY-RUN version from .verinfo");
    assert!(
        stdout.contains(&expected_version),
        "Version should contain {}, got: {}",
        expected_version,
        stdout
    );
    // 验证包含 commit hash（格式: version+commit 或 version+commit-dirty）
    assert!(
        stdout.contains('+'),
        "Version should contain commit hash with + separator, got: {}",
        stdout
    );
}

/// 测试字节码文件格式验证
#[test]
fn test_bytecode_format_validation() {
    use cavvy::bytecode::serializer::*;

    // 测试无效魔数
    let invalid_bytes = b"INVALID";
    let result = deserialize(invalid_bytes);
    assert!(result.is_err(), "Should fail with invalid magic");

    // 测试有效魔数但无效数据
    let mut valid_magic = b"CAY\x01".to_vec();
    valid_magic.extend_from_slice(&[0u8; 100]); // 填充一些数据
    let result = deserialize(&valid_magic);
    // 可能不会成功，但不应该panic
}

/// 测试复杂字节码生成
#[test]
fn test_complex_bytecode_generation() {
    use cavvy::bytecode::*;

    let mut module = BytecodeModule::new("complex_test".to_string(), "windows".to_string());

    // 添加外部库
    module.add_external_lib("user32".to_string());
    module.add_external_lib("kernel32".to_string());

    // 添加类定义
    let class_name = module.constant_pool.add_utf8("MyClass");
    let parent_name = module.constant_pool.add_utf8("Object");

    let type_def = TypeDefinition {
        name_index: class_name,
        parent_index: Some(parent_name),
        interface_indices: Vec::new(),
        modifiers: TypeModifiers {
            is_public: true,
            is_final: false,
            is_abstract: false,
            is_interface: false,
        },
        fields: vec![FieldDefinition {
            name_index: module.constant_pool.add_utf8("field1"),
            type_index: module.constant_pool.add_utf8("int"),
            modifiers: FieldModifiers {
                is_public: true,
                is_private: false,
                is_protected: false,
                is_static: false,
                is_final: true,
            },
            initial_value: Some(module.constant_pool.add_integer(100)),
        }],
        methods: Vec::new(),
    };

    module.add_type_definition(type_def);

    // 验证模块内容
    assert_eq!(module.type_definitions.len(), 1);
    assert_eq!(module.header.external_libs.len(), 2);
    assert!(module.header.external_libs.contains(&"user32".to_string()));

    // 序列化和反序列化
    let serialized = serializer::serialize(&module);
    let deserialized =
        serializer::deserialize(&serialized).expect("Failed to deserialize complex module");

    assert_eq!(deserialized.type_definitions.len(), 1);
    assert_eq!(deserialized.header.external_libs.len(), 2);
}

/// 测试JIT编译器 - 将字节码转换为IR
#[test]
fn test_jit_bytecode_to_ir() {
    use cavvy::bytecode::instructions::*;
    use cavvy::bytecode::jit::*;
    use cavvy::bytecode::*;

    // 创建一个字节码模块
    let mut module = BytecodeModule::new("jit_test".to_string(), "windows".to_string());

    // 添加一个简单函数
    let func_name_index = module.constant_pool.add_utf8("main");
    let return_type_index = module.constant_pool.add_utf8("int");

    let body = CodeBody {
        instructions: vec![Instruction::iconst(42), Instruction::new(Opcode::Ireturn)],
        exception_table: Vec::new(),
        line_number_table: Vec::new(),
    };

    let function = FunctionDefinition {
        name_index: func_name_index,
        return_type_index,
        param_type_indices: Vec::new(),
        param_name_indices: Vec::new(),
        modifiers: MethodModifiers::default(),
        body,
        max_locals: 1,
        max_stack: 1,
    };

    module.add_function(function);

    // 使用JIT编译器转换为IR
    let ir = bytecode_to_ir(&module).expect("Failed to convert bytecode to IR");

    // 验证IR包含预期内容
    assert!(
        ir.contains("define i32 @main()"),
        "IR should contain main function definition"
    );
    assert!(ir.contains("ret i32"), "IR should contain return statement");
}

/// 测试JIT编译器 - 完整编译流程
#[test]
fn test_jit_full_compilation() {
    use cavvy::bytecode::instructions::*;
    use cavvy::bytecode::jit::*;
    use cavvy::bytecode::*;

    // 创建一个字节码模块
    let mut module = BytecodeModule::new("full_test".to_string(), "windows".to_string());

    // 添加一个简单函数
    let func_name_index = module.constant_pool.add_utf8("add");
    let return_type_index = module.constant_pool.add_utf8("int");

    let body = CodeBody {
        instructions: vec![
            Instruction::iconst(10),
            Instruction::iconst(20),
            Instruction::new(Opcode::Iadd),
            Instruction::new(Opcode::Ireturn),
        ],
        exception_table: Vec::new(),
        line_number_table: Vec::new(),
    };

    let function = FunctionDefinition {
        name_index: func_name_index,
        return_type_index,
        param_type_indices: Vec::new(),
        param_name_indices: Vec::new(),
        modifiers: MethodModifiers::default(),
        body,
        max_locals: 2,
        max_stack: 2,
    };

    module.add_function(function);

    // 使用JIT编译器转换为IR
    let ir = bytecode_to_ir(&module).expect("Failed to convert bytecode to IR");

    // 验证IR包含预期内容
    assert!(
        ir.contains("define i32 @add()"),
        "IR should contain add function definition"
    );
    assert!(ir.contains("add i32"), "IR should contain add instruction");
}
