use std::env;
use std::fs;
use std::process;
use std::path::Path;
use cavvy::bytecode::{BytecodeModule, CodeBody, serializer, obfuscator};
use cavvy::bytecode::instructions::{Instruction, Opcode};
use cavvy::bytecode::constant_pool::ConstantPool;
use cavvy::error::{print_miette_error, print_tool_error, print_warning};

const VERSION: &str = env!("CAY_BCGEN_VERSION");

/// 字节码生成选项
struct BcgenOptions {
    obfuscate: bool,           // --obfuscate: 混淆字节码
    obfuscate_level: String,   // --obfuscate-level: 混淆级别 (light/normal/deep)
    output_file: Option<String>, // -o: 输出文件
    verbose: bool,             // --verbose: 详细输出
}

impl Default for BcgenOptions {
    fn default() -> Self {
        Self {
            obfuscate: false,
            obfuscate_level: "normal".to_string(),
            output_file: None,
            verbose: false,
        }
    }
}

fn print_usage() {
    println!("Cavvy Bytecode Generator v{}", VERSION);
    println!("警告：此工具为实验性版本，可能包含严重错误和不稳定性。");
    println!("Usage: cay-bcgen [options] <source_file.cay>");
    println!("");
    println!("Options:");
    println!("  -o <file>              指定输出文件（默认: 输入文件名.caybc）");
    println!("  --obfuscate            混淆生成的字节码");
    println!("  --obfuscate-level <l>  混淆级别: light, normal, deep (默认: normal)");
    println!("  --verbose, -v          显示详细编译信息");
    println!("  --version, -V          显示版本号");
    println!("  --help, -h             显示帮助信息");
    println!("");
    println!("Examples:");
    println!("  cay-bcgen hello.cay");
    println!("  cay-bcgen -o output.caybc hello.cay");
    println!("  cay-bcgen --obfuscate --obfuscate-level deep hello.cay");
}

fn parse_args(args: &[String]) -> Result<(BcgenOptions, String), String> {
    let mut options = BcgenOptions::default();
    let mut source_file: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        let arg = &args[i];

        match arg.as_str() {
            "--version" | "-V" => {
                println!("Cavvy Bytecode Generator v{}", VERSION);
                process::exit(0);
            }
            "--help" | "-h" => {
                print_usage();
                process::exit(0);
            }
            "--verbose" | "-v" => {
                options.verbose = true;
            }
            "--obfuscate" => {
                options.obfuscate = true;
            }
            "--obfuscate-level" => {
                if i + 1 < args.len() {
                    options.obfuscate_level = args[i + 1].clone();
                    if !["light", "normal", "deep"].contains(&options.obfuscate_level.as_str()) {
                        return Err(format!("无效的混淆级别: {}", options.obfuscate_level));
                    }
                    i += 1;
                } else {
                    return Err("--obfuscate-level 需要一个参数".to_string());
                }
            }
            "-o" => {
                if i + 1 < args.len() {
                    options.output_file = Some(args[i + 1].clone());
                    i += 1;
                } else {
                    return Err("-o 需要一个参数".to_string());
                }
            }
            _ => {
                if arg.starts_with('-') {
                    return Err(format!("未知选项: {}", arg));
                }
                if source_file.is_none() {
                    source_file = Some(arg.clone());
                } else {
                    return Err(format!("多余参数: {}", arg));
                }
            }
        }
        i += 1;
    }

    let source_file = source_file.ok_or("需要指定源文件")?;
    Ok((options, source_file))
}

/// 编译Cavvy源码为字节码模块
fn compile_to_bytecode(source: &str, source_path: &str) -> Result<BytecodeModule, String> {
    // 1. 词法分析
    let tokens = cavvy::lexer::lex(source)
        .map_err(|e| format!("词法分析错误: {:?}", e))?;

    // 2. 语法分析
    let ast = cavvy::parser::parse(tokens)
        .map_err(|e| format!("语法分析错误: {:?}", e))?;

    // 3. 语义分析
    let mut analyzer = cavvy::semantic::SemanticAnalyzer::new();
    analyzer.analyze(&ast)
        .map_err(|e| format!("语义分析错误: {:?}", e))?;

    // 4. 生成字节码模块
    let mut module = BytecodeModule::new(
        Path::new(source_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_string(),
        std::env::consts::OS.to_string(),
    );

    // 5. 从AST生成字节码
    generate_bytecode_from_ast(&ast, &mut module, analyzer.get_type_registry())
        .map_err(|e| format!("字节码生成错误: {}", e))?;

    Ok(module)
}

/// 从AST生成字节码
fn generate_bytecode_from_ast(
    ast: &cavvy::ast::Program,
    module: &mut BytecodeModule,
    type_registry: &cavvy::types::TypeRegistry
) -> Result<(), String> {
    use cavvy::bytecode::*;
    use cavvy::ast::*;

    // 处理顶层函数
    for func in &ast.top_level_functions {
        let name_index = module.constant_pool.add_utf8(&func.name);
        let return_type_index = get_type_index(&func.return_type, &mut module.constant_pool);

        let mut param_type_indices = Vec::new();
        let mut param_name_indices = Vec::new();

        for param in &func.params {
            param_type_indices.push(get_type_index(&param.param_type, &mut module.constant_pool));
            param_name_indices.push(module.constant_pool.add_utf8(&param.name));
        }

        // 生成函数体
        let body = generate_code_body(&func.body, module)?;

        let modifiers = MethodModifiers {
            is_public: func.modifiers.contains(&Modifier::Public),
            is_private: func.modifiers.contains(&Modifier::Private),
            is_protected: func.modifiers.contains(&Modifier::Protected),
            is_static: func.modifiers.contains(&Modifier::Static),
            is_final: func.modifiers.contains(&Modifier::Final),
            is_abstract: false,
            is_native: false,
            is_override: false,
        };

        let function_def = FunctionDefinition {
            name_index,
            return_type_index,
            param_type_indices,
            param_name_indices,
            modifiers,
            body,
            max_locals: 10, // TODO: 动态计算
            max_stack: 10,
        };

        module.add_function(function_def);
    }

    // 处理类定义
    for class in &ast.classes {
        let name_index = module.constant_pool.add_utf8(&class.name);
        let parent_index = class.parent.as_ref()
            .map(|p| module.constant_pool.add_utf8(p));

        let mut interface_indices = Vec::new();
        for interface in &class.interfaces {
            interface_indices.push(module.constant_pool.add_utf8(interface));
        }

        let modifiers = TypeModifiers {
            is_public: class.modifiers.contains(&Modifier::Public),
            is_final: class.modifiers.contains(&Modifier::Final),
            is_abstract: class.modifiers.contains(&Modifier::Abstract),
            is_interface: false,
        };

        let mut fields = Vec::new();
        let mut methods = Vec::new();

        for member in &class.members {
            match member {
                ClassMember::Field(field) => {
                    let field_def = FieldDefinition {
                        name_index: module.constant_pool.add_utf8(&field.name),
                        type_index: get_type_index(&field.field_type, &mut module.constant_pool),
                        modifiers: FieldModifiers {
                            is_public: field.modifiers.contains(&Modifier::Public),
                            is_private: field.modifiers.contains(&Modifier::Private),
                            is_protected: field.modifiers.contains(&Modifier::Protected),
                            is_static: field.modifiers.contains(&Modifier::Static),
                            is_final: field.modifiers.contains(&Modifier::Final),
                        },
                        initial_value: None, // TODO: 处理字段初始化值
                    };
                    fields.push(field_def);
                }
                ClassMember::Method(method) => {
                    let method_name_index = module.constant_pool.add_utf8(&method.name);
                    let return_type_index = get_type_index(&method.return_type, &mut module.constant_pool);

                    let mut param_type_indices = Vec::new();
                    let mut param_name_indices = Vec::new();

                    for param in &method.params {
                        param_type_indices.push(get_type_index(&param.param_type, &mut module.constant_pool));
                        param_name_indices.push(module.constant_pool.add_utf8(&param.name));
                    }

                    let body = method.body.as_ref()
                        .map(|b| generate_code_body(b, module).ok())
                        .flatten();

                    let method_modifiers = MethodModifiers {
                        is_public: method.modifiers.contains(&Modifier::Public),
                        is_private: method.modifiers.contains(&Modifier::Private),
                        is_protected: method.modifiers.contains(&Modifier::Protected),
                        is_static: method.modifiers.contains(&Modifier::Static),
                        is_final: method.modifiers.contains(&Modifier::Final),
                        is_abstract: method.modifiers.contains(&Modifier::Abstract),
                        is_native: method.modifiers.contains(&Modifier::Native),
                        is_override: method.modifiers.contains(&Modifier::Override),
                    };

                    let method_def = MethodDefinition {
                        name_index: method_name_index,
                        return_type_index,
                        param_type_indices,
                        param_name_indices,
                        modifiers: method_modifiers,
                        body,
                        max_locals: 10,
                        max_stack: 10,
                    };
                    methods.push(method_def);
                }
                _ => {}
            }
        }

        let type_def = TypeDefinition {
            name_index,
            parent_index,
            interface_indices,
            modifiers,
            fields,
            methods,
        };

        module.add_type_definition(type_def);
    }

    Ok(())
}

/// 生成代码体
fn generate_code_body(block: &cavvy::ast::Block, module: &mut BytecodeModule) -> Result<CodeBody, String> {
    use cavvy::bytecode::instructions::*;

    let mut instructions = Vec::new();
    let mut ctx = StatementContext::new();

    for stmt in &block.statements {
        generate_statement(stmt, &mut instructions, module, &mut ctx)?;
    }

    // 添加默认返回
    instructions.push(Instruction::new(Opcode::Return));

    // 修复跳转偏移量
    fix_jump_offsets(&mut instructions, &ctx)?;

    Ok(CodeBody {
        instructions,
        exception_table: Vec::new(),
        line_number_table: Vec::new(),
    })
}

/// 跳转占位符，用于两阶段编译
#[derive(Debug, Clone)]
enum JumpPlaceholder {
    IfEq { condition_end: usize, else_start: Option<usize> },
    Goto { from: usize },
}

/// 语句生成上下文
struct StatementContext {
    placeholders: Vec<(usize, JumpPlaceholder)>,
}

impl StatementContext {
    fn new() -> Self {
        Self {
            placeholders: Vec::new(),
        }
    }
}

/// 生成语句
fn generate_statement(
    stmt: &cavvy::ast::Stmt,
    instructions: &mut Vec<Instruction>,
    module: &mut BytecodeModule,
    ctx: &mut StatementContext
) -> Result<(), String> {
    use cavvy::bytecode::instructions::*;
    use cavvy::ast::*;

    match stmt {
        Stmt::Expr(expr) => {
            generate_expression(expr, instructions, module)?;
            // 弹出表达式结果
            instructions.push(Instruction::new(Opcode::Pop));
        }
        Stmt::VarDecl(var_decl) => {
            if let Some(ref init) = var_decl.initializer {
                generate_expression(init, instructions, module)?;
                // 存储到局部变量, TODO: 动态计算局部变量索引
                instructions.push(Instruction::istore(0));
            }
        }
        Stmt::Return(Some(expr)) => {
            generate_expression(expr, instructions, module)?;
            instructions.push(Instruction::new(Opcode::Ireturn));
        }
        Stmt::Return(None) => {
            instructions.push(Instruction::new(Opcode::Return));
        }
        Stmt::If(if_stmt) => {
            generate_expression(&if_stmt.condition, instructions, module)?;

            // 条件跳转 - 记录占位符位置
            let ifeq_pos = instructions.len();
            instructions.push(Instruction::ifeq(0)); // 占位符，稍后修复

            // then 分支
            generate_statement(&if_stmt.then_branch, instructions, module, ctx)?;

            if let Some(ref else_branch) = if_stmt.else_branch {
                // 需要跳过 else 分支的跳转
                let goto_pos = instructions.len();
                instructions.push(Instruction::goto(0)); // 占位符

                // 记录 else 分支开始位置
                let else_start = instructions.len();

                // else 分支
                generate_statement(else_branch, instructions, module, ctx)?;

                // 记录占位符用于后续修复
                ctx.placeholders.push((ifeq_pos, JumpPlaceholder::IfEq {
                    condition_end: else_start as usize,
                    else_start: Some(else_start as usize),
                }));
                ctx.placeholders.push((goto_pos, JumpPlaceholder::Goto {
                    from: instructions.len(),
                }));
            } else {
                // 没有 else 分支，条件不满足时跳转到 if 之后
                let after_then = instructions.len();
                ctx.placeholders.push((ifeq_pos, JumpPlaceholder::IfEq {
                    condition_end: after_then,
                    else_start: None,
                }));
            }
        }
        Stmt::Block(block) => {
            for stmt in &block.statements {
                generate_statement(stmt, instructions, module, ctx)?;
            }
        }
        _ => {
            // 其他语句类型 TODO: 处理其他语句类型
        }
    }

    Ok(())
}

/// 修复跳转偏移量
fn fix_jump_offsets(instructions: &mut [Instruction], ctx: &StatementContext) -> Result<(), String> {
    use cavvy::bytecode::instructions::*;

    for (pos, placeholder) in &ctx.placeholders {
        match placeholder {
            JumpPlaceholder::IfEq { condition_end, else_start: _ } => {
                // 计算从 ifeq 指令到目标位置的偏移量
                // ifeq 指令本身占3字节（1字节opcode + 2字节offset）
                let offset = (*condition_end as i16) - (*pos as i16) - 1;

                // 确保偏移量在有效范围内
                if offset < -32768 || offset > 32767 {
                    return Err(format!("跳转偏移量超出范围: {}", offset));
                }

                // 修复 ifeq 指令的偏移量
                instructions[*pos] = Instruction::ifeq(offset);
            }
            JumpPlaceholder::Goto { from } => {
                // 计算从 goto 指令到目标位置的偏移量
                let offset = (*from as i16) - (*pos as i16) - 1;

                // 确保偏移量在有效范围内
                if offset < -32768 || offset > 32767 {
                    return Err(format!("跳转偏移量超出范围: {}", offset));
                }

                // 修复 goto 指令的偏移量
                instructions[*pos] = Instruction::goto(offset);
            }
        }
    }

    Ok(())
}

/// 生成表达式
fn generate_expression(
    expr: &cavvy::ast::Expr,
    instructions: &mut Vec<Instruction>,
    module: &mut BytecodeModule
) -> Result<(), String> {
    use cavvy::bytecode::instructions::*;
    use cavvy::ast::*;

    match expr {
        Expr::Literal(lit) => {
            match lit {
                LiteralValue::Int32(v) => {
                    if *v >= -128 && *v <= 127 {
                        instructions.push(Instruction::iconst(*v as i8));
                    } else {
                        let index = module.constant_pool.add_integer(*v);
                        instructions.push(Instruction::ldc(index));
                    }
                }
                LiteralValue::Int64(v) => {
                    let index = module.constant_pool.add_long(*v);
                    instructions.push(Instruction::ldc(index));
                }
                LiteralValue::Float32(v) => {
                    let index = module.constant_pool.add_float(*v);
                    instructions.push(Instruction::ldc(index));
                }
                LiteralValue::Float64(v) => {
                    let index = module.constant_pool.add_double(*v);
                    instructions.push(Instruction::ldc(index));
                }
                LiteralValue::Bool(true) => {
                    instructions.push(Instruction::iconst(1));
                }
                LiteralValue::Bool(false) => {
                    instructions.push(Instruction::iconst(0));
                }
                LiteralValue::String(s) => {
                    let index = module.constant_pool.add_string(s);
                    instructions.push(Instruction::ldc(index));
                }
                LiteralValue::Char(c) => {
                    instructions.push(Instruction::iconst(*c as i8));
                }
                LiteralValue::Null => {
                    instructions.push(Instruction::new(Opcode::AconstNull));
                }
            }
        }
        Expr::Identifier(ident) => {
            // 加载局部变量 TODO: 动态计算局部变量索引
            let _name = &ident.name;
            instructions.push(Instruction::iload(0));
        }
        Expr::Binary(bin) => {
            generate_expression(&bin.left, instructions, module)?;
            generate_expression(&bin.right, instructions, module)?;

            match bin.op {
                BinaryOp::Add => instructions.push(Instruction::new(Opcode::Iadd)),
                BinaryOp::Sub => instructions.push(Instruction::new(Opcode::Isub)),
                BinaryOp::Mul => instructions.push(Instruction::new(Opcode::Imul)),
                BinaryOp::Div => instructions.push(Instruction::new(Opcode::Idiv)),
                BinaryOp::Mod => instructions.push(Instruction::new(Opcode::Irem)),
                _ => {}
            }
        }
        Expr::Call(call) => {
            // 生成参数
            for arg in &call.args {
                generate_expression(arg, instructions, module)?;
            }

            // 调用函数 TODO: 处理函数调用
            if let Expr::Identifier(ident) = call.callee.as_ref() {
                let index = module.constant_pool.add_utf8(&ident.name);
                instructions.push(Instruction::invokestatic(index));
            }
        }
        _ => {
            // 其他表达式类型 TODO: 处理其他表达式类型
        }
    }

    Ok(())
}

/// 获取类型索引
fn get_type_index(ty: &cavvy::types::Type, pool: &mut ConstantPool) -> u16 {
    let type_name = match ty {
        cavvy::types::Type::Void => "void",
        cavvy::types::Type::Int32 => "int",
        cavvy::types::Type::Int64 => "long",
        cavvy::types::Type::Float32 => "float",
        cavvy::types::Type::Float64 => "double",
        cavvy::types::Type::Bool => "boolean",
        cavvy::types::Type::Char => "char",
        cavvy::types::Type::String => "String",
        cavvy::types::Type::Object(name) => name.as_str(),
        cavvy::types::Type::Array(inner) => {
            let inner_name = match inner.as_ref() {
                cavvy::types::Type::Int32 => "int",
                cavvy::types::Type::Int64 => "long",
                _ => "Object",
            };
            return pool.add_utf8(&format!("{}[]", inner_name));
        }
        _ => "Object",
    };
    pool.add_utf8(type_name)
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let (options, source_path) = match parse_args(&args) {
        Ok(result) => result,
        Err(e) => {
            print_miette_error(
                "cavvy::argument_error",
                &e,
                Some("请检查命令行参数是否正确")
            );
            print_usage();
            process::exit(1);
        }
    };

    // 检查源文件是否存在
    if !Path::new(&source_path).exists() {
        print_miette_error(
            "cavvy::io_error",
            &format!("源文件 '{}' 不存在", source_path),
            Some("请检查文件路径是否正确")
        );
        process::exit(1);
    }

    // 确定输出文件
    let output_path = options.output_file.unwrap_or_else(|| {
        if source_path.ends_with(".cay") {
            source_path.replace(".cay", ".caybc")
        } else {
            format!("{}.caybc", source_path)
        }
    });

    if options.verbose {
        println!("Cavvy Bytecode Generator v{}", VERSION);
        println!("源文件: {}", source_path);
        println!("输出文件: {}", output_path);
        println!();
    }

    // 读取源文件
    let source = match fs::read_to_string(&source_path) {
        Ok(content) => content,
        Err(e) => {
            print_miette_error(
                "cavvy::io_error",
                &format!("无法读取源文件 '{}': {}", source_path, e),
                Some("请检查文件路径是否正确，文件是否存在")
            );
            process::exit(1);
        }
    };

    // 编译为字节码
    if options.verbose {
        println!("[1/3] 编译源码到字节码...");
    }

    let mut module = match compile_to_bytecode(&source, &source_path) {
        Ok(m) => m,
        Err(e) => {
            print_tool_error("字节码编译器", &e, Some("请检查代码语法和语义"));
            process::exit(1);
        }
    };

    // 混淆字节码
    if options.obfuscate {
        if options.verbose {
            println!("[2/3] 混淆字节码 (级别: {})...", options.obfuscate_level);
        }

        let obf_options = match options.obfuscate_level.as_str() {
            "light" => obfuscator::ObfuscationOptions {
                obfuscate_names: true,
                obfuscate_control_flow: false,
                insert_junk_code: false,
                encrypt_strings: false,
                shuffle_functions: false,
                strip_debug_info: true,
            },
            "normal" => obfuscator::ObfuscationOptions {
                obfuscate_names: true,
                obfuscate_control_flow: true,
                insert_junk_code: false,
                encrypt_strings: true,
                shuffle_functions: false,
                strip_debug_info: true,
            },
            "deep" => obfuscator::ObfuscationOptions {
                obfuscate_names: true,
                obfuscate_control_flow: true,
                insert_junk_code: true,
                encrypt_strings: true,
                shuffle_functions: true,
                strip_debug_info: true,
            },
            _ => obfuscator::ObfuscationOptions::default(),
        };

        let mut obfuscator = obfuscator::BytecodeObfuscator::new(obf_options);
        obfuscator.obfuscate(&mut module);
    } else if options.verbose {
        println!("[2/3] 跳过混淆");
    }

    // 序列化字节码
    if options.verbose {
        println!("[3/3] 序列化字节码...");
    }

    let bytecode = serializer::serialize(&module);

    // 写入文件
    if let Err(e) = fs::write(&output_path, bytecode) {
        print_miette_error(
            "cavvy::io_error",
            &format!("无法写入输出文件 '{}': {}", output_path, e),
            Some("请检查输出目录是否有写入权限")
        );
        process::exit(1);
    }

    // 获取文件大小
    let file_size = fs::metadata(&output_path)
        .map(|m| m.len())
        .unwrap_or(0);

    if options.verbose {
        println!();
        println!("编译成功!");
        println!("输出: {} ({} bytes)", output_path, file_size);

        if options.obfuscate {
            println!("字节码已混淆");
        }
    } else {
        println!("已生成: {} ({} bytes)", output_path, file_size);
    }
}
