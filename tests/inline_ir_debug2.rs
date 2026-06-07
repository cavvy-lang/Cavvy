//! 内联IR调试测试2

use std::process::Command;

#[test]
fn test_inline_ir_parsing_debug() {
    // 直接测试解析
    let source = r#"
public class Test {
    public static int testAdd(int a, int b) {
        int result;
        __ir {
            %sum = add i32 %0, %1
            store i32 %sum, i32* %2
        }
        return result;
    }
}
"#;

    // 词法分析
    let tokens = cavvy::lexer::lex(source).expect("Lexing failed");

    println!("Tokens:");
    for (i, t) in tokens.iter().enumerate() {
        println!("  {}: {:?} at {:?}", i, t.token, t.loc);
    }

    // 语法分析（带源代码）
    let ast = cavvy::parser::parse_with_source(tokens, source.to_string()).expect("Parsing failed");

    // 查找内联IR语句
    println!("\nTotal functions: {}", ast.top_level_functions.len());
    println!("Total classes: {}", ast.classes.len());

    for (ci, class) in ast.classes.iter().enumerate() {
        println!("\nClass [{}]: {:?}", ci, class.name);
        println!("  Members count: {}", class.members.len());
        for (mi, member) in class.members.iter().enumerate() {
            if let cavvy::ast::ClassMember::Method(method) = member {
                println!("\n  Method [{}]: {:?}", mi, method.name);
                if let Some(body) = &method.body {
                    println!("    Statements count: {}", body.statements.len());
                    for (si, stmt) in body.statements.iter().enumerate() {
                        let disc = std::mem::discriminant(stmt);
                        println!("    Statement [{}]: {:?}", si, disc);
                        match stmt {
                            cavvy::ast::Stmt::InlineIr(inline_ir) => {
                                println!(
                                    "      -> Inline IR with {} lines:",
                                    inline_ir.raw_lines.len()
                                );
                                for (j, line) in inline_ir.raw_lines.iter().enumerate() {
                                    println!("         [{}]: '{}'", j, line);
                                }
                            }
                            cavvy::ast::Stmt::VarDecl(v) => {
                                println!("      -> VarDecl: {:?}", v.name);
                            }
                            cavvy::ast::Stmt::Return(_) => {
                                println!("      -> Return");
                            }
                            _ => {
                                println!("      -> Other statement type");
                            }
                        }
                    }
                }
            }
        }
    }

    // 验证内联IR行数
    let mut found = false;
    for class in &ast.classes {
        for member in &class.members {
            if let cavvy::ast::ClassMember::Method(method) = member {
                if let Some(body) = &method.body {
                    for stmt in &body.statements {
                        if let cavvy::ast::Stmt::InlineIr(inline_ir) = stmt {
                            println!("Found Inline IR with {} lines:", inline_ir.raw_lines.len());
                            for (i, line) in inline_ir.raw_lines.iter().enumerate() {
                                println!("  [{}]: '{}'", i, line);
                            }
                            if inline_ir.raw_lines.len() == 2 {
                                found = true;
                            }
                        }
                    }
                }
            }
        }
    }

    assert!(found, "Should find inline IR with 2 lines");
}
