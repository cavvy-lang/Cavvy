use cavvy::ast::*;
use cavvy::bytecode::constant_pool::ConstantPool;
use cavvy::bytecode::instructions::{Instruction, Opcode};
use cavvy::bytecode::{BytecodeModule, CodeBody, obfuscator, serializer};
use cavvy::miette_diagnostic::{print_miette_error, print_tool_error, print_warning};
use std::env;
use std::fs;
use std::path::Path;
use std::process;

const VERSION: &str = env!("CAY_BCGEN_VERSION");

/// 字节码生成选项
struct BcgenOptions {
    obfuscate: bool,             // --obfuscate: 混淆字节码
    obfuscate_level: String,     // --obfuscate-level: 混淆级别 (light/normal/deep)
    output_file: Option<String>, // -o: 输出文件
    verbose: bool,               // --verbose: 详细输出
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
    let mut iter = args.iter().skip(1).peekable();

    while let Some(arg) = iter.next() {
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
                let level = iter
                    .next()
                    .ok_or("--obfuscate-level 需要一个参数")?;
                if !["light", "normal", "deep"].contains(&level.as_str()) {
                    return Err(format!("无效的混淆级别: {}", level));
                }
                options.obfuscate_level = level.clone();
            }
            "-o" => {
                let out = iter.next().ok_or("-o 需要一个参数")?;
                options.output_file = Some(out.clone());
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
    }

    let source_file = source_file.ok_or("需要指定源文件")?;
    Ok((options, source_file))
}

/// 编译Cavvy源码为字节码模块
fn compile_to_bytecode(source: &str, source_path: &str) -> Result<BytecodeModule, String> {
    // 1. 词法分析
    let tokens = cavvy::lexer::lex(source).map_err(|e| format!("词法分析错误: {:?}", e))?;

    // 2. 语法分析
    let ast = cavvy::parser::parse(tokens).map_err(|e| format!("语法分析错误: {:?}", e))?;

    // 3. 语义分析
    let mut analyzer = cavvy::semantic::SemanticAnalyzer::new();
    let ast = analyzer
        .analyze(ast)
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
    type_registry: &cavvy::types::TypeRegistry,
) -> Result<(), String> {
    use cavvy::ast::*;
    use cavvy::bytecode::*;

    // 处理顶层函数
    for func in &ast.top_level_functions {
        let name_index = module.constant_pool.add_utf8(&func.name);
        let return_type_index = get_type_index(&func.return_type, &mut module.constant_pool);

        let mut param_type_indices = Vec::new();
        let mut param_name_indices = Vec::new();
        let mut param_names = Vec::new();

        for param in &func.params {
            param_type_indices.push(get_type_index(&param.param_type, &mut module.constant_pool));
            param_name_indices.push(module.constant_pool.add_utf8(&param.name));
            param_names.push(param.name.clone());
        }

        // 生成函数体
        let (body, max_locals) = generate_code_body(&func.body, module, &param_names)?;

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
            max_locals,
            max_stack: 10,
        };

        module.add_function(function_def);
    }

    // 处理类定义
    for class in &ast.classes {
        let name_index = module.constant_pool.add_utf8(&class.name);
        let parent_index = class
            .parent
            .as_ref()
            .map(|p| module.constant_pool.add_utf8(p));

        let mut interface_indices = Vec::new();
        for interface in &class.interfaces {
            interface_indices.push(module.constant_pool.add_utf8(&format!("{}", interface)));
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
                    // 处理字段初始化值（仅支持字面量常量；null 表示无初始值）
                    let initial_value = match field.initializer.as_ref() {
                        Some(Expr::Literal(lit)) => match &lit.value {
                            LiteralValue::Int32(v) => Some(module.constant_pool.add_integer(*v)),
                            LiteralValue::Int64(v) => Some(module.constant_pool.add_long(*v)),
                            LiteralValue::Float32(v) => Some(module.constant_pool.add_float(*v)),
                            LiteralValue::Float64(v) => Some(module.constant_pool.add_double(*v)),
                            LiteralValue::String(s) => Some(module.constant_pool.add_string(s)),
                            LiteralValue::Bool(true) => Some(module.constant_pool.add_integer(1)),
                            LiteralValue::Bool(false) => Some(module.constant_pool.add_integer(0)),
                            LiteralValue::Char(c) => {
                                Some(module.constant_pool.add_integer(*c as i32))
                            }
                            LiteralValue::Null => None,
                        },
                        Some(_) => {
                            return Err(format!(
                                "字段 '{}' 的初始化器不是字面量，暂不支持其字节码生成 (行 {})",
                                field.name, field.loc.line
                            ));
                        }
                        None => None,
                    };
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
                        initial_value,
                    };
                    fields.push(field_def);
                }
                ClassMember::Method(method) => {
                    let method_name_index = module.constant_pool.add_utf8(&method.name);
                    let return_type_index =
                        get_type_index(&method.return_type, &mut module.constant_pool);

                    let mut param_type_indices = Vec::new();
                    let mut param_name_indices = Vec::new();
                    let mut param_names = Vec::new();

                    for param in &method.params {
                        param_type_indices
                            .push(get_type_index(&param.param_type, &mut module.constant_pool));
                        param_name_indices.push(module.constant_pool.add_utf8(&param.name));
                        param_names.push(param.name.clone());
                    }

                    // 方法体生成失败必须报错，不能静默产出空方法体
                    let body = match method.body.as_ref() {
                        Some(b) => Some(
                            generate_code_body(b, module, &param_names).map_err(|e| {
                                format!("方法 '{}' 的方法体生成失败: {}", method.name, e)
                            })?,
                        ),
                        None => None,
                    };

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

                    let (body_raw, max_locals) = body.unwrap_or((
                        CodeBody {
                            instructions: Vec::new(),
                            exception_table: Vec::new(),
                            line_number_table: Vec::new(),
                        },
                        0,
                    ));

                    let method_def = MethodDefinition {
                        name_index: method_name_index,
                        return_type_index,
                        param_type_indices,
                        param_name_indices,
                        modifiers: method_modifiers,
                        body: Some(body_raw),
                        max_locals,
                        max_stack: 10,
                    };
                    methods.push(method_def);
                }
                ClassMember::Constructor(ctor) => {
                    return Err(format!(
                        "暂不支持类 '{}' 的构造函数的字节码生成 (行 {})",
                        class.name, ctor.loc.line
                    ));
                }
                ClassMember::Destructor(dtor) => {
                    return Err(format!(
                        "暂不支持类 '{}' 的析构函数的字节码生成 (行 {})",
                        class.name, dtor.loc.line
                    ));
                }
                ClassMember::InstanceInitializer(_) => {
                    return Err(format!(
                        "暂不支持类 '{}' 的实例初始化块的字节码生成",
                        class.name
                    ));
                }
                ClassMember::StaticInitializer(_) => {
                    return Err(format!(
                        "暂不支持类 '{}' 的静态初始化块的字节码生成",
                        class.name
                    ));
                }
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
fn generate_code_body(
    block: &cavvy::ast::Block,
    module: &mut BytecodeModule,
    params: &[String],
) -> Result<(CodeBody, u16), String> {
    use cavvy::bytecode::instructions::*;

    let mut instructions = Vec::new();
    let mut ctx = StatementContext::new();
    let mut tracker = LocalVarTracker::new();

    // 注册参数到局部变量表
    for param in params {
        tracker.register_param(param);
    }

    for stmt in &block.statements {
        generate_statement(stmt, &mut instructions, module, &mut ctx, &mut tracker)?;
    }

    // 添加默认返回
    instructions.push(Instruction::new(Opcode::Return));

    // 修复跳转偏移量
    fix_jump_offsets(&mut instructions, &ctx)?;

    let max_locals = tracker.max_locals();
    Ok((
        CodeBody {
            instructions,
            exception_table: Vec::new(),
            line_number_table: Vec::new(),
        },
        max_locals,
    ))
}

/// 跳转占位符，用于两阶段编译
#[derive(Debug, Clone)]
enum JumpPlaceholder {
    IfEq {
        condition_end: usize,
        else_start: Option<usize>,
    },
    Goto {
        from: usize,
    },
}

/// 可跳出作用域的种类
#[derive(Debug, Clone, Copy)]
enum BreakableKind {
    /// 循环：continue 跳转到条件判断处（指令索引）。
    /// while 的条件位置已知为 Some；do-while 的条件位置需事后修补为 None。
    Loop { continue_target: Option<usize> },
    /// switch：只支持 break
    Switch,
}

/// 可跳出作用域（循环 / switch），记录待修复的跳转占位符
#[derive(Debug)]
struct BreakableScope {
    kind: BreakableKind,
    /// break 语句生成的 goto 占位符位置，离开作用域时统一修复
    break_placeholders: Vec<usize>,
    /// continue 语句生成的 goto 占位符位置（仅 do-while 使用），条件位置确定后修复
    continue_placeholders: Vec<usize>,
}

/// 语句生成上下文
struct StatementContext {
    placeholders: Vec<(usize, JumpPlaceholder)>,
    /// 循环 / switch 作用域栈（内层在栈顶）
    scopes: Vec<BreakableScope>,
}

impl StatementContext {
    fn new() -> Self {
        Self {
            placeholders: Vec::new(),
            scopes: Vec::new(),
        }
    }

    /// 查找最近的循环作用域（跳过中间的 switch 作用域）
    fn nearest_loop_mut(&mut self) -> Option<&mut BreakableScope> {
        self.scopes
            .iter_mut()
            .rev()
            .find(|s| matches!(s.kind, BreakableKind::Loop { .. }))
    }
}

/// 修复一组 goto 占位符，使其跳转到目标位置
fn patch_goto_placeholders(instructions: &mut [Instruction], placeholders: &[usize], target: usize) {
    for pos in placeholders {
        let offset = (target as i16) - (*pos as i16) - 1;
        instructions[*pos] = Instruction::goto(offset);
    }
}

/// 局部变量追踪器
struct LocalVarTracker {
    /// 变量名 -> 索引
    vars: std::collections::HashMap<String, u16>,
    /// 下一个可用的局部变量索引
    next_index: u16,
}

impl LocalVarTracker {
    fn new() -> Self {
        Self {
            vars: std::collections::HashMap::new(),
            next_index: 0,
        }
    }

    /// 注册参数（从索引0开始）
    fn register_param(&mut self, name: &str) {
        self.vars.insert(name.to_string(), self.next_index);
        self.next_index += 1;
    }

    /// 注册新的局部变量，返回分配的索引
    fn register_var(&mut self, name: &str) -> u16 {
        let index = self.next_index;
        self.vars.insert(name.to_string(), index);
        self.next_index += 1;
        index
    }

    /// 查找变量的索引
    fn lookup(&self, name: &str) -> Option<u16> {
        self.vars.get(name).copied()
    }

    /// 获取最大局部变量数
    fn max_locals(&self) -> u16 {
        self.next_index
    }
}

/// 生成语句
fn generate_statement(
    stmt: &cavvy::ast::Stmt,
    instructions: &mut Vec<Instruction>,
    module: &mut BytecodeModule,
    ctx: &mut StatementContext,
    tracker: &mut LocalVarTracker,
) -> Result<(), String> {
    use cavvy::ast::*;
    use cavvy::bytecode::instructions::*;

    match stmt {
        Stmt::Expr(expr) => {
            generate_expression(expr, instructions, module, tracker)?;
            // 弹出表达式结果
            instructions.push(Instruction::new(Opcode::Pop));
        }
        Stmt::VarDecl(var_decl) => {
            // 分配局部变量索引
            let index = tracker.register_var(&var_decl.name);
            if let Some(ref init) = var_decl.initializer {
                generate_expression(init, instructions, module, tracker)?;
                // 存储到局部变量
                instructions.push(Instruction::istore(index));
            }
        }
        Stmt::Return(Some(expr)) => {
            generate_expression(expr, instructions, module, tracker)?;
            instructions.push(Instruction::new(Opcode::Ireturn));
        }
        Stmt::Return(None) => {
            instructions.push(Instruction::new(Opcode::Return));
        }
        Stmt::If(if_stmt) => {
            generate_expression(&if_stmt.condition, instructions, module, tracker)?;

            // 条件跳转 - 记录占位符位置
            let ifeq_pos = instructions.len();
            instructions.push(Instruction::ifeq(0)); // 占位符，稍后修复

            // then 分支
            generate_statement(&if_stmt.then_branch, instructions, module, ctx, tracker)?;

            if let Some(ref else_branch) = if_stmt.else_branch {
                // 需要跳过 else 分支的跳转
                let goto_pos = instructions.len();
                instructions.push(Instruction::goto(0)); // 占位符

                // 记录 else 分支开始位置
                let else_start = instructions.len();

                // else 分支
                generate_statement(else_branch, instructions, module, ctx, tracker)?;

                // 记录占位符用于后续修复
                ctx.placeholders.push((
                    ifeq_pos,
                    JumpPlaceholder::IfEq {
                        condition_end: else_start as usize,
                        else_start: Some(else_start as usize),
                    },
                ));
                ctx.placeholders.push((
                    goto_pos,
                    JumpPlaceholder::Goto {
                        from: instructions.len(),
                    },
                ));
            } else {
                // 没有 else 分支，条件不满足时跳转到 if 之后
                let after_then = instructions.len();
                ctx.placeholders.push((
                    ifeq_pos,
                    JumpPlaceholder::IfEq {
                        condition_end: after_then,
                        else_start: None,
                    },
                ));
            }
        }
        Stmt::Block(block) => {
            for stmt in &block.statements {
                generate_statement(stmt, instructions, module, ctx, tracker)?;
            }
        }
        Stmt::While(while_stmt) => {
            if while_stmt.label.is_some() {
                return Err("暂不支持带标签的 while 循环的字节码生成".to_string());
            }
            // while 循环: while (cond) { body }
            let loop_start = instructions.len();
            // continue 跳转到条件判断处
            ctx.scopes.push(BreakableScope {
                kind: BreakableKind::Loop {
                    continue_target: Some(loop_start),
                },
                break_placeholders: Vec::new(),
                continue_placeholders: Vec::new(),
            });
            // 生成条件表达式
            generate_expression(&while_stmt.condition, instructions, module, tracker)?;
            // 条件为假时跳出循环
            let ifeq_pos = instructions.len();
            instructions.push(Instruction::ifeq(0)); // 占位符
            // 生成循环体
            generate_statement(&while_stmt.body, instructions, module, ctx, tracker)?;
            // 无条件跳回循环开始
            let goto_pos = instructions.len();
            let loop_offset = (loop_start as i16) - (goto_pos as i16) - 1;
            instructions.push(Instruction::goto(loop_offset));
            // 修复条件跳转偏移量
            let after_loop = instructions.len();
            let cond_offset = (after_loop as i16) - (ifeq_pos as i16) - 1;
            instructions[ifeq_pos] = Instruction::ifeq(cond_offset);
            // 修复循环体内的 break 跳转
            let scope = ctx.scopes.pop().expect("循环作用域栈不平衡");
            patch_goto_placeholders(instructions, &scope.break_placeholders, after_loop);
        }
        Stmt::DoWhile(do_while_stmt) => {
            if do_while_stmt.label.is_some() {
                return Err("暂不支持带标签的 do-while 循环的字节码生成".to_string());
            }
            // do-while 循环: do { body } while (cond);
            let loop_start = instructions.len();
            // continue 目标（条件判断处）要在循环体生成后才知道，先占位
            ctx.scopes.push(BreakableScope {
                kind: BreakableKind::Loop {
                    continue_target: None,
                },
                break_placeholders: Vec::new(),
                continue_placeholders: Vec::new(),
            });
            // 生成循环体
            generate_statement(&do_while_stmt.body, instructions, module, ctx, tracker)?;
            // continue 跳转到条件判断处，修复循环体内的 continue 占位符
            let cond_pos = instructions.len();
            if let Some(scope) = ctx.scopes.last_mut() {
                scope.kind = BreakableKind::Loop {
                    continue_target: Some(cond_pos),
                };
                patch_goto_placeholders(instructions, &scope.continue_placeholders, cond_pos);
            }
            // 生成条件表达式
            generate_expression(&do_while_stmt.condition, instructions, module, tracker)?;
            // 条件为真时跳回循环开始
            let ifne_pos = instructions.len();
            // 使用 Ifne 操作码：不等于0时跳转
            let loop_offset = (loop_start as i16) - (ifne_pos as i16) - 1;
            instructions.push(Instruction::with_operands(
                Opcode::Ifne,
                loop_offset.to_le_bytes().to_vec(),
            ));
            // 修复循环体内的 break 跳转
            let after_loop = instructions.len();
            let scope = ctx.scopes.pop().expect("循环作用域栈不平衡");
            patch_goto_placeholders(instructions, &scope.break_placeholders, after_loop);
        }
        Stmt::Break(label, loc) => {
            if label.is_some() {
                return Err(format!(
                    "暂不支持带标签的 break 语句的字节码生成 (行 {})",
                    loc.line
                ));
            }
            // break 必须位于循环或 switch 内
            if ctx.scopes.is_empty() {
                return Err(format!("break 语句必须位于循环或 switch 内 (行 {})", loc.line));
            }
            // 跳出 switch 前需弹出 switch 值，保持栈平衡
            if matches!(
                ctx.scopes.last().map(|s| s.kind),
                Some(BreakableKind::Switch)
            ) {
                instructions.push(Instruction::new(Opcode::Pop));
            }
            // 生成 goto 占位符，离开作用域时统一修复为跳转到作用域之后
            let pos = instructions.len();
            instructions.push(Instruction::goto(0));
            ctx.scopes
                .last_mut()
                .expect("已检查作用域栈非空")
                .break_placeholders
                .push(pos);
        }
        Stmt::Continue(label, loc) => {
            if label.is_some() {
                return Err(format!(
                    "暂不支持带标签的 continue 语句的字节码生成 (行 {})",
                    loc.line
                ));
            }
            // continue 跳转到最近的循环的条件判断处（允许隔着 switch）
            let loop_scope = ctx.nearest_loop_mut();
            match loop_scope {
                Some(scope) => match scope.kind {
                    BreakableKind::Loop {
                        continue_target: Some(target),
                    } => {
                        let pos = instructions.len();
                        let offset = (target as i16) - (pos as i16) - 1;
                        instructions.push(Instruction::goto(offset));
                    }
                    BreakableKind::Loop {
                        continue_target: None,
                    } => {
                        // do-while：条件位置尚未确定，生成占位符，事后修补
                        let pos = instructions.len();
                        instructions.push(Instruction::goto(0));
                        scope.continue_placeholders.push(pos);
                    }
                    BreakableKind::Switch => unreachable!("已过滤为循环作用域"),
                },
                None => {
                    return Err(format!("continue 语句必须位于循环内 (行 {})", loc.line));
                }
            }
        }
        Stmt::Switch(switch_stmt) => {
            // switch 语句 - 简化实现：生成条件分支链
            generate_expression(&switch_stmt.expr, instructions, module, tracker)?;
            // break 可跳出 switch
            ctx.scopes.push(BreakableScope {
                kind: BreakableKind::Switch,
                break_placeholders: Vec::new(),
                continue_placeholders: Vec::new(),
            });
            // 简化处理：将 switch 转换为 if-else 链
            for case in &switch_stmt.cases {
                // 复制 switch 值
                instructions.push(Instruction::new(Opcode::Dup));
                // 生成 case 值（仅处理整数常量）
                match &case.value {
                    CaseValue::Integer(v) => {
                        if *v >= -128 && *v <= 127 {
                            instructions.push(Instruction::iconst(*v as i8));
                        } else {
                            let index = module.constant_pool.add_integer(*v as i32);
                            instructions.push(Instruction::ldc(index));
                        }
                    }
                    CaseValue::EnumVariant {
                        enum_name,
                        variant_name,
                    } => {
                        return Err(format!(
                            "暂不支持 switch 枚举分支 ({}.{}) 的字节码生成 (行 {})",
                            enum_name, variant_name, case.loc.line
                        ));
                    }
                }
                // 比较 - 使用 if_icmpne 判断不相等则跳过
                let ifne_pos = instructions.len();
                instructions.push(Instruction::with_operands(
                    Opcode::IfIcmpne,
                    0i16.to_le_bytes().to_vec(),
                ));
                // 生成 case 体
                for stmt in &case.body {
                    generate_statement(stmt, instructions, module, ctx, tracker)?;
                }
                // 修复跳转
                let after_case = instructions.len();
                let offset = (after_case as i16) - (ifne_pos as i16) - 1;
                instructions[ifne_pos] =
                    Instruction::with_operands(Opcode::IfIcmpne, offset.to_le_bytes().to_vec());
            }
            // 处理 default 分支
            if let Some(ref default_body) = switch_stmt.default {
                for stmt in default_body {
                    generate_statement(stmt, instructions, module, ctx, tracker)?;
                }
            }
            // 弹出 switch 值
            instructions.push(Instruction::new(Opcode::Pop));
            // 修复 switch 体内的 break 跳转（跳到 switch 结束之后）
            let after_switch = instructions.len();
            let scope = ctx.scopes.pop().expect("switch 作用域栈不平衡");
            patch_goto_placeholders(instructions, &scope.break_placeholders, after_switch);
        }
        Stmt::Scope(scope_stmt) => {
            // scope 语句 - 生成块内容
            for stmt in &scope_stmt.body.statements {
                generate_statement(stmt, instructions, module, ctx, tracker)?;
            }
        }
        Stmt::For(for_stmt) => {
            return Err(format!(
                "暂不支持 for 循环的字节码生成 (行 {})",
                for_stmt.loc.line
            ));
        }
        Stmt::ForEach(for_each) => {
            return Err(format!(
                "暂不支持 for-each 循环的字节码生成 (行 {})",
                for_each.loc.line
            ));
        }
        Stmt::InlineIr(inline_ir) => {
            return Err(format!(
                "暂不支持内联 IR 语句的字节码生成 (行 {})",
                inline_ir.loc.line
            ));
        }
    }

    Ok(())
}

/// 修复跳转偏移量
fn fix_jump_offsets(
    instructions: &mut [Instruction],
    ctx: &StatementContext,
) -> Result<(), String> {
    use cavvy::bytecode::instructions::*;

    for (pos, placeholder) in &ctx.placeholders {
        match placeholder {
            JumpPlaceholder::IfEq {
                condition_end,
                else_start: _,
            } => {
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
    module: &mut BytecodeModule,
    tracker: &mut LocalVarTracker,
) -> Result<(), String> {
    use cavvy::ast::*;
    use cavvy::bytecode::instructions::*;

    match expr {
        Expr::Literal(lit_expr) => match &lit_expr.value {
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
        },
        Expr::Identifier(ident) => {
            // 加载局部变量；查不到必须报错，不能静默加载 slot 0
            match tracker.lookup(&ident.name) {
                Some(index) => instructions.push(Instruction::iload(index)),
                None => {
                    return Err(format!(
                        "未定义的变量: '{}' (行 {})",
                        ident.name, ident.loc.line
                    ));
                }
            }
        }
        Expr::Binary(bin) => {
            generate_expression(&bin.left, instructions, module, tracker)?;
            generate_expression(&bin.right, instructions, module, tracker)?;

            match bin.op {
                BinaryOp::Add => instructions.push(Instruction::new(Opcode::Iadd)),
                BinaryOp::Sub => instructions.push(Instruction::new(Opcode::Isub)),
                BinaryOp::Mul => instructions.push(Instruction::new(Opcode::Imul)),
                BinaryOp::Div => instructions.push(Instruction::new(Opcode::Idiv)),
                BinaryOp::Mod => instructions.push(Instruction::new(Opcode::Irem)),
                other => {
                    return Err(format!(
                        "暂不支持二元运算符 {:?} 的字节码生成 (行 {})",
                        other, bin.loc.line
                    ));
                }
            }
        }
        Expr::Call(call) => {
            // 生成参数
            for arg in &call.args {
                generate_expression(arg, instructions, module, tracker)?;
            }

            // 处理函数调用
            match call.callee.as_ref() {
                Expr::Identifier(ident) => {
                    let index = module.constant_pool.add_utf8(&ident.name);
                    instructions.push(Instruction::invokestatic(index));
                }
                Expr::MemberAccess(member) => {
                    // 处理 object.method() 调用
                    // 生成 object 引用
                    generate_expression(&member.object, instructions, module, tracker)?;
                    let index = module.constant_pool.add_utf8(&member.member);
                    instructions.push(Instruction::with_operands(
                        Opcode::Invokevirtual,
                        index.to_le_bytes().to_vec(),
                    ));
                }
                _ => {
                    return Err(format!(
                        "暂不支持该调用形式的字节码生成 (行 {})",
                        call.loc.line
                    ));
                }
            }
        }
        Expr::Unary(unary) => {
            generate_expression(&unary.operand, instructions, module, tracker)?;
            match unary.op {
                UnaryOp::Neg => {
                    instructions.push(Instruction::new(Opcode::Ineg));
                }
                UnaryOp::Not => {
                    instructions.push(Instruction::new(Opcode::Iconst1));
                    instructions.push(Instruction::new(Opcode::Ixor));
                }
                UnaryOp::PreInc => {
                    instructions.push(Instruction::new(Opcode::Iconst1));
                    instructions.push(Instruction::new(Opcode::Iadd));
                }
                UnaryOp::PreDec => {
                    instructions.push(Instruction::new(Opcode::Iconst1));
                    instructions.push(Instruction::new(Opcode::Isub));
                }
                other => {
                    return Err(format!(
                        "暂不支持一元运算符 {:?} 的字节码生成 (行 {})",
                        other, unary.loc.line
                    ));
                }
            }
        }
        Expr::Assignment(assignment) => {
            // 生成右值
            generate_expression(&assignment.value, instructions, module, tracker)?;
            // 处理赋值目标
            if let Expr::Identifier(ident) = assignment.target.as_ref() {
                match tracker.lookup(&ident.name) {
                    Some(index) => instructions.push(Instruction::istore(index)),
                    None => {
                        return Err(format!(
                            "赋值目标未定义: '{}' (行 {})",
                            ident.name, ident.loc.line
                        ));
                    }
                }
            } else {
                return Err(format!(
                    "暂不支持非标识符赋值目标的字节码生成 (行 {})",
                    assignment.loc.line
                ));
            }
        }
        Expr::MemberAccess(member) => {
            // 成员访问 - 简化实现：加载对象后加载字段
            generate_expression(&member.object, instructions, module, tracker)?;
            let index = module.constant_pool.add_utf8(&member.member);
            instructions.push(Instruction::with_operands(
                Opcode::Getfield,
                index.to_le_bytes().to_vec(),
            ));
        }
        Expr::ArrayAccess(array_access) => {
            // 数组访问 - 生成数组和索引
            generate_expression(&array_access.array, instructions, module, tracker)?;
            generate_expression(&array_access.index, instructions, module, tracker)?;
            instructions.push(Instruction::new(Opcode::Iaload));
        }
        Expr::New(new_expr) => {
            // new 表达式 - 简化实现
            let type_index = module.constant_pool.add_utf8(&new_expr.class_name);
            instructions.push(Instruction::with_operands(
                Opcode::New,
                type_index.to_le_bytes().to_vec(),
            ));
            instructions.push(Instruction::new(Opcode::Dup));
            // 生成构造函数参数
            for arg in &new_expr.args {
                generate_expression(arg, instructions, module, tracker)?;
            }
            instructions.push(Instruction::with_operands(
                Opcode::Invokespecial,
                type_index.to_le_bytes().to_vec(),
            ));
        }
        Expr::Ternary(ternary) => {
            // 三元表达式: condition ? true_branch : false_branch
            generate_expression(&ternary.condition, instructions, module, tracker)?;
            let ifeq_pos = instructions.len();
            instructions.push(Instruction::ifeq(0)); // 占位符
            generate_expression(&ternary.true_branch, instructions, module, tracker)?;
            let goto_pos = instructions.len();
            instructions.push(Instruction::goto(0)); // 占位符
            let false_start = instructions.len();
            generate_expression(&ternary.false_branch, instructions, module, tracker)?;
            let after_false = instructions.len();
            // 修复跳转
            let true_offset = (false_start as i16) - (ifeq_pos as i16) - 1;
            instructions[ifeq_pos] = Instruction::ifeq(true_offset);
            let false_offset = (after_false as i16) - (goto_pos as i16) - 1;
            instructions[goto_pos] = Instruction::goto(false_offset);
        }
        other => {
            return Err(format!(
                "暂不支持 {} 表达式的字节码生成 (行 {})",
                expr_kind_name(other),
                other.location().line
            ));
        }
    }

    Ok(())
}

/// 获取表达式种类名称（用于错误信息）
fn expr_kind_name(expr: &cavvy::ast::Expr) -> &'static str {
    use cavvy::ast::Expr;
    match expr {
        Expr::Literal(_) => "字面量",
        Expr::Identifier(_) => "标识符",
        Expr::Binary(_) => "二元运算",
        Expr::Unary(_) => "一元运算",
        Expr::Call(_) => "函数调用",
        Expr::MemberAccess(_) => "成员访问",
        Expr::New(_) => "new",
        Expr::Assignment(_) => "赋值",
        Expr::Cast(_) => "类型转换",
        Expr::ArrayCreation(_) => "数组创建",
        Expr::ArrayAccess(_) => "数组访问",
        Expr::ArrayInit(_) => "数组初始化",
        Expr::MethodRef(_) => "方法引用",
        Expr::Lambda(_) => "Lambda",
        Expr::Ternary(_) => "三元运算",
        Expr::If(_) => "if 表达式",
        Expr::Try(_) => "try (?)",
        Expr::InstanceOf(_) => "instanceof",
        Expr::Alloc(_) => "__cay_alloc",
        Expr::Dealloc(_) => "__cay_free",
        Expr::AllocArray(_) => "__cay_alloc_array",
        Expr::NamedArg(_) => "命名参数",
    }
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
                Some("请检查命令行参数是否正确"),
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
            Some("请检查文件路径是否正确"),
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
                Some("请检查文件路径是否正确，文件是否存在"),
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
        // 混淆库当前明确返回"不可用"错误；必须处理，不能静默产出未混淆的产物
        if let Err(e) = obfuscator.obfuscate(&mut module) {
            print_tool_error(
                "字节码混淆器",
                &e.to_string(),
                Some("请去掉 --obfuscate 选项后重试"),
            );
            process::exit(1);
        }
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
            Some("请检查输出目录是否有写入权限"),
        );
        process::exit(1);
    }

    // 获取文件大小
    let file_size = fs::metadata(&output_path).map(|m| m.len()).unwrap_or(0);

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
