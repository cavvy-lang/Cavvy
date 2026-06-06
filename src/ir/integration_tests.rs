//! IR 集成测试
//!
//! 端到端测试：源码 → 词法 → 语法 → AST → IR Builder → Verifier → LLVM Backend

#[cfg(test)]
mod integration_tests {
    use crate::ir::*;
    use crate::lexer;
    use crate::parser;
    use crate::semantic;

    /// 辅助函数：从源代码构建 IR 模块（启用顶层函数特性）
    fn build_ir(source: &str) -> IrModule {
        let tokens = lexer::lex(source).expect("Lexing failed");
        let ast = parser::parse(tokens).expect("Parsing failed");

        // 语义分析（启用顶层函数特性）
        let mut analyzer = semantic::SemanticAnalyzer::with_features(vec!["top_level_function".to_string()]);
        analyzer.analyze(&ast).expect("Semantic analysis failed");

        // IR 构建
        let mut builder = IrBuilder::new();
        builder.set_type_registry(analyzer.get_type_registry().clone());
        builder.build_from_ast(&ast).expect("IR building failed")
    }

    /// 辅助函数：验证 IR 并发射 LLVM IR
    fn verify_and_emit(module: &IrModule) -> String {
        // 验证
        let result = IrVerifier::new().verify(module);
        assert!(result.is_valid, "IR verification failed: {:?}", result.errors);

        // 发射
        LlvmBackend::emit_module(module).expect("LLVM emission failed")
    }

    // ============================================================
    // 基础编译测试
    // ============================================================

    #[test]
    fn test_compile_empty_class() {
        let source = "public class Empty {}";
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("Module"));
        assert!(ir.contains("define void @Empty.__ctor"));
    }

    #[test]
    fn test_compile_hello_world() {
        let source = r#"
public class Hello {
    public static void main() {
        println("Hello");
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("define void @Hello.main"));
    }

    // ============================================================
    // 变量和表达式测试
    // ============================================================

    #[test]
    fn test_variable_declaration() {
        let source = r#"
public class Test {
    public int compute() {
        int x = 42;
        int y = x + 10;
        return y;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("alloca i32"));
        assert!(ir.contains("add"));
        assert!(ir.contains("ret i32"));
    }

    #[test]
    fn test_float_expressions() {
        let source = r#"
public class Test {
    public double compute() {
        double pi = 3.14159;
        double result = pi * 2.0;
        return result;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("alloca double"));
        assert!(ir.contains("fmul"));
        assert!(ir.contains("ret double"));
    }

    #[test]
    fn test_boolean_operations() {
        let source = r#"
public class Test {
    public boolean check(int a, int b) {
        boolean result = a > b && a != 0;
        return result;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("icmp"));
        assert!(ir.contains("ret i1"));
    }

    // ============================================================
    // 控制流测试
    // ============================================================

    #[test]
    fn test_if_else_control_flow() {
        let source = r#"
public class Test {
    public int branch(int x) {
        int result;
        if (x > 10) {
            result = 1;
        } else {
            result = 0;
        }
        return result;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        // 检查基本块结构
        assert!(ir.contains("then."));
        assert!(ir.contains("else."));
        assert!(ir.contains("ifmerge."));
        assert!(ir.contains("br i1"));
        assert!(ir.contains("br label"));
    }

    #[test]
    fn test_while_loop() {
        let source = r#"
public class Test {
    public int sum(int n) {
        int s = 0;
        int i = 0;
        while (i < n) {
            s = s + i;
            i = i + 1;
        }
        return s;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("while.cond"));
        assert!(ir.contains("while.body"));
        assert!(ir.contains("while.end"));
    }

    #[test]
    fn test_for_loop() {
        let source = r#"
public class Test {
    public int factorial(int n) {
        int result = 1;
        for (int i = 1; i <= n; i++) {
            result = result * i;
        }
        return result;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("for.cond"));
        assert!(ir.contains("for.body"));
        assert!(ir.contains("for.update"));
        assert!(ir.contains("for.end"));
    }

    #[test]
    fn test_do_while_loop() {
        let source = r#"
public class Test {
    public int countdown(int n) {
        int count = 0;
        do {
            count = count + 1;
            n = n - 1;
        } while (n > 0);
        return count;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("dowhile.cond"));
        assert!(ir.contains("dowhile.body"));
        assert!(ir.contains("dowhile.end"));
    }

    #[test]
    fn test_switch_statement() {
        let source = r#"
public class Test {
    public int grade(int score) {
        int result;
        switch (score) {
            case 1: result = 10; break;
            case 2: result = 20; break;
            default: result = 0; break;
        }
        return result;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("switch"));
    }

    #[test]
    fn test_ternary_expression() {
        let source = r#"
public class Test {
    public int max(int a, int b) {
        int result = a > b ? a : b;
        return result;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        // Ternary is lowered to select in the new IR
        assert!(ir.contains("select"));
    }

    // ============================================================
    // 类型转换测试
    // ============================================================

    #[test]
    fn test_type_casting() {
        let source = r#"
public class Test {
    public int cast(double d) {
        int i = (int)d;
        long l = (long)d;
        return i;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        // 应该有类型转换指令
        assert!(ir.contains("fptosi") || ir.contains("sext") || ir.contains("trunc"));
    }

    #[test]
    fn test_compound_assignment() {
        let source = r#"
public class Test {
    public int compute(int x) {
        int val = 10;
        val += x;
        val -= 2;
        val *= 3;
        return val;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("add"));
        assert!(ir.contains("sub"));
        assert!(ir.contains("mul"));
    }

    // ============================================================
    // 数组测试
    // ============================================================

    #[test]
    fn test_array_creation() {
        let source = r#"
public class Test {
    public int arrayTest() {
        int[] arr = new int[5];
        arr[0] = 42;
        int val = arr[0];
        return val;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        // 应该有 GEP 或数组相关操作
        assert!(ir.contains("alloca"));
    }

    // ============================================================
    // 类和继承测试
    // ============================================================

    #[test]
    fn test_class_with_constructor() {
        let source = r#"
public class Point {
    private int x;
    private int y;
    
    public Point(int px, int py) {
        x = px;
        y = py;
    }
    
    public int getX() {
        return x;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        // 确认生成了 __ctor 和 getX
        assert!(ir.contains("Point.__ctor"));
        assert!(ir.contains("Point.getX"));
    }

    #[test]
    fn test_inheritance_chain() {
        let source = r#"
public class Animal {
    protected String name;
    public Animal(String n) {
        name = n;
    }
}

public class Dog extends Animal {
    public Dog(String n) {
        super(n);
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("Animal.__ctor"));
        assert!(ir.contains("Dog.__ctor"));
    }

    #[test]
    fn test_static_fields() {
        let source = r#"
public class Counter {
    private static int count = 0;
    
    public Counter() {
        count = count + 1;
    }
    
    public static int getCount() {
        return count;
    }
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("count_s"));
    }

    // ============================================================
    // 顶层函数测试
    // ============================================================

    #[test]
    fn test_top_level_function() {
        let source = r#"
public int square(int x) {
    return x * x;
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        assert!(ir.contains("__toplevel_square"));
        assert!(ir.contains("define i32"));
    }

    // ============================================================
    // 综合编译测试（使用实际示例文件）
    // ============================================================

    #[test]
    fn test_compile_minimal_program() {
        // 一个最小但完整的程序
        let source = r#"
public int main() {
    int a = 10;
    int b = 20;
    int c = a + b;
    println("Sum: " + String.valueOf(c));
    return 0;
}
"#;
        let module = build_ir(source);
        let ir = verify_and_emit(&module);

        // 验证生成了完整模块
        assert!(ir.contains("target triple"));
        assert!(ir.contains("define i32 @__toplevel_main()"));
        assert!(ir.contains("__toplevel_main"));
    }

    // ============================================================
    // IR 优化测试
    // ============================================================

    #[test]
    fn test_ir_verifier_accepts_valid() {
        let source = r#"
public class A {
    public int f() { return 1; }
}
"#;
        let module = build_ir(source);
        let result = IrVerifier::new().verify(&module);
        assert!(result.is_valid, "Verifier should accept valid IR: {:?}", result.errors);
    }

    #[test]
    fn test_ir_verifier_rejects_missing_terminator() {
        // 创建一个手动构造的、缺少终止指令的 IR
        let module = IrModule::new("test".to_string(), "x86_64-unknown-linux-gnu".to_string());
        let func = IrFunction::new("bad".to_string(), IrType::Void, Vec::new());
        // 注意：IrFunction::new 会创建 entry 块，但没有 terminator
        let mut bad_module = module;
        bad_module.add_function(func);
        
        let result = IrVerifier::new().verify(&bad_module);
        assert!(!result.is_valid);
    }

    #[test]
    fn test_ir_inliner_config() {
        let config = InlinerConfig {
            max_instructions: 100,
            max_depth: 5,
            inline_recursive: false,
            min_instructions: 1,
        };
        let inliner = Inliner::with_config(config);
        assert_eq!(inliner.stats().functions_inlined, 0);
    }

    // ============================================================
    // 内联 IR 安全测试
    // ============================================================

    #[test]
    fn test_inline_ir_parser_allows_arithmetic() {
        let parser = InlineIrParser::new();
        let result = parser.parse("%r = add i32 %a, %b", &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_parser_allows_alloca() {
        let parser = InlineIrParser::new();
        let result = parser.parse("%p = alloca i32", &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_parser_rejects_invoke() {
        let parser = InlineIrParser::new();
        let result = parser.parse("invoke void @foo() to label %c unwind label %e", &[], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invoke"));
    }

    #[test]
    fn test_inline_ir_parser_rejects_system_call() {
        let parser = InlineIrParser::new();
        let result = parser.parse("call i32 @system(i8* %cmd)", &[], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("system"));
    }

    // ============================================================
    // 内联IR综合功能测试
    // ============================================================

    #[test]
    fn test_inline_ir_complex_arithmetic() {
        let parser = InlineIrParser::new();
        let ir = r#"
            %t1 = mul i32 %a, %b
            %t2 = add i32 %t1, %c
            %result = sdiv i32 %t2, %d
            ret i32 %result
        "#;
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
        
        let block = result.unwrap();
        assert_eq!(block.raw_lines.len(), 4);
    }

    #[test]
    fn test_inline_ir_memory_operations() {
        let parser = InlineIrParser::new();
        let ir = r#"
            %p = alloca i32
            store i32 42, i32* %p
            %v = load i32, i32* %p
            ret i32 %v
        "#;
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_type_casting() {
        let parser = InlineIrParser::new();
        let ir = r#"
            %ext = sext i16 %a to i32
            %flt = sitofp i32 %ext to double
            %trc = fptrunc double %flt to float
            ret float %trc
        "#;
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_control_flow() {
        let parser = InlineIrParser::new();
        let ir = r#"
            entry:
            %cmp = icmp eq i32 %a, %b
            br i1 %cmp, label %eq, label %ne
            eq:
            ret i32 1
            ne:
            ret i32 0
        "#;
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_with_bindings() {
        let parser = InlineIrParser::new();
        let inputs = vec![
            ("a".to_string(), IrValue::IntConst(10, IrType::I32)),
            ("b".to_string(), IrValue::IntConst(20, IrType::I32)),
        ];
        let outputs = vec![
            ("%result".to_string(), IrType::I32),
        ];
        let ir = "%result = add i32 %a, %b";
        let result = parser.parse(ir, &inputs, &outputs);
        assert!(result.is_ok());
        
        let block = result.unwrap();
        assert_eq!(block.inputs.len(), 2);
        assert_eq!(block.outputs.len(), 1);
    }

    #[test]
    fn test_inline_ir_to_ir_instruction() {
        let parser = InlineIrParser::new();
        let ir = r#"
            %t = add i32 %a, %b
            %result = mul i32 %t, %c
        "#;
        let block = parser.parse(ir, &[], &[]).unwrap();
        let inst = parser.to_instruction(&block);
        
        match inst {
            crate::ir::value::IrInstruction::InlineIr { lines, .. } => {
                assert_eq!(lines.len(), 2);
                assert!(lines[0].contains("add"));
                assert!(lines[1].contains("mul"));
            }
            _ => panic!("Expected InlineIr instruction"),
        }
    }

    #[test]
    fn test_inline_ir_all_allowed_opcodes() {
        let parser = InlineIrParser::new();
        let opcodes = vec![
            "add", "sub", "mul", "sdiv", "srem",
            "fadd", "fsub", "fmul", "fdiv", "frem",
            "and", "or", "xor", "shl", "ashr", "lshr",
            "icmp", "fcmp",
            "sext", "zext", "trunc", "sitofp", "fptosi", "fpext", "fptrunc",
            "bitcast", "ptrtoint", "inttoptr",
            "getelementptr", "alloca", "load", "store",
            "call", "ret", "br", "select", "phi", "switch", "unreachable",
        ];
        
        for opcode in opcodes {
            let ir = if opcode.starts_with('i') || opcode.starts_with('f') {
                format!("%r = {} eq i32 %a, %b", opcode)
            } else if opcode == "br" {
                "br label %block".to_string()
            } else if opcode == "ret" {
                "ret i32 %r".to_string()
            } else if opcode == "store" {
                "store i32 %v, i32* %p".to_string()
            } else if opcode == "call" {
                "%r = call i32 @foo()".to_string()
            } else if opcode == "unreachable" {
                opcode.to_string()
            } else {
                format!("%r = {} i32 %a, %b", opcode)
            };
            
            let result = parser.parse(&ir, &[], &[]);
            assert!(result.is_ok(), "Opcode '{}' should be allowed", opcode);
        }
    }

    #[test]
    fn test_inline_ir_security_comprehensive() {
        let parser = InlineIrParser::new();
        let dangerous_calls = vec![
            "system", "exec", "popen", "fork", "vfork",
            "dlopen", "dlsym", "dlclose",
            "mmap", "munmap", "mprotect",
        ];
        
        for call in dangerous_calls {
            let ir = format!("call i32 @{}()", call);
            let result = parser.parse(&ir, &[], &[]);
            assert!(result.is_err(), "'{}' should be rejected", call);
        }
    }

    // ============================================================
    // IR 类型系统测试
    // ============================================================

    #[test]
    fn test_ir_type_conversion_from_cavvy() {
        use crate::types::Type;
        
        assert_eq!(IrType::from(&Type::Int32), IrType::I32);
        assert_eq!(IrType::from(&Type::Int64), IrType::I64);
        assert_eq!(IrType::from(&Type::Float32), IrType::F32);
        assert_eq!(IrType::from(&Type::Float64), IrType::F64);
        assert_eq!(IrType::from(&Type::Bool), IrType::I1);
        assert_eq!(IrType::from(&Type::Char), IrType::I8);
        assert_eq!(IrType::from(&Type::Void), IrType::Void);
        assert_eq!(IrType::from(&Type::String), IrType::Pointer(Box::new(IrType::I8)));
    }

    #[test]
    fn test_ir_type_to_llvm_string() {
        assert_eq!(IrType::I32.to_llvm_str(), "i32");
        assert_eq!(IrType::I64.to_llvm_str(), "i64");
        assert_eq!(IrType::F32.to_llvm_str(), "float");
        assert_eq!(IrType::F64.to_llvm_str(), "double");
        assert_eq!(IrType::Void.to_llvm_str(), "void");
        assert_eq!(IrType::I1.to_llvm_str(), "i1");
        assert_eq!(IrType::I8.to_llvm_str(), "i8");
        assert_eq!(IrType::Pointer(Box::new(IrType::I8)).to_llvm_str(), "i8*");
    }

    // ============================================================
    // IR 值系统测试
    // ============================================================

    #[test]
    fn test_ir_value_int_const() {
        let val = IrValue::IntConst(42, IrType::I32);
        assert_eq!(val.ir_type(), IrType::I32);
        assert_eq!(val.to_raw_str(), "42");
        assert!(val.is_const());
    }

    #[test]
    fn test_ir_value_register() {
        let val = IrValue::Register("%t0".to_string(), IrType::I32);
        assert_eq!(val.ir_type(), IrType::I32);
        assert!(val.is_register());
        assert!(!val.is_const());
    }

    #[test]
    fn test_ir_value_bool() {
        let val = IrValue::BoolConst(true);
        assert_eq!(val.ir_type(), IrType::I1);
        assert_eq!(val.to_raw_str(), "1");
    }

    #[test]
    fn test_ir_value_null() {
        let val = IrValue::NullConst(IrType::Pointer(Box::new(IrType::I8)));
        assert!(val.ir_type().is_pointer());
    }

    // ============================================================
    // IR 模块统计测试
    // ============================================================

    #[test]
    fn test_ir_module_stats() {
        let source = r#"
public class Stats {
    public int f1() { return 1; }
    public int f2() { return 2; }
}
"#;
        let module = build_ir(source);
        let stats = module.stats();

        // f1, f2, __ctor → 3 个函数
        assert!(stats.function_count >= 3, "Expected >= 3 functions, got {}", stats.function_count);
        assert!(stats.instruction_count > 0, "Expected instructions");
    }

    // ============================================================
    // 大规模综合编译测试
    // ============================================================

    #[test]
    fn test_compile_large_program() {
        let source = std::fs::read_to_string("examples/test_ir_features.cay")
            .expect("Could not read test_ir_features.cay");

        let module = build_ir(&source);
        let ir = verify_and_emit(&module);

        let stats = module.stats();

        // 验证关键结构存在（方法名格式: ClassName.__methodName_paramTypes 或 ClassName.methodName）
        assert!(ir.contains("IRTest.__ctor"));
        assert!(ir.contains("IRTest.__compute_i_i"));
        assert!(ir.contains("IRTest.__testIfElse_i"));
        assert!(ir.contains("IRTest.__testWhile_i"));
        assert!(ir.contains("IRTest.__testFor_i"));
        assert!(ir.contains("IRTest.__testDoWhile_i"));
        assert!(ir.contains("IRTest.__testSwitch_i"));
        assert!(ir.contains("IRTest.main"));

        // 验证控制流块存在
        assert!(ir.contains("while.cond"));
        assert!(ir.contains("while.body"));
        assert!(ir.contains("while.end"));
        assert!(ir.contains("for.cond"));
        assert!(ir.contains("for.body"));
        assert!(ir.contains("for.end"));

        // 验证接口和基类
        assert!(ir.contains("BaseMath.__ctor"));
        assert!(ir.contains("BaseMath.getBase"));

        println!("Large program stats: {} functions, {} blocks, {} instructions, {} strings",
            stats.function_count, stats.block_count, stats.instruction_count, stats.string_count);

        // 统计应该合理
        assert!(stats.function_count > 15, "Expected >15 functions, got {}", stats.function_count);
        assert!(stats.block_count > 30, "Expected >30 blocks, got {}", stats.block_count);
        assert!(stats.instruction_count > 100, "Expected >100 instructions, got {}", stats.instruction_count);
    }
}
