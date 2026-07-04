// cay-sir: Cavvy Statement/SSA IR viewer
// 显示交给 IR Builder pipeline 的代码所生成的 SSA IR。

mod debug_common;

use cavvy::ast::{InlineIrStmt, Program, Stmt};
use cavvy::ir::inline_ir::InlineIrParser;
use cavvy::ir::{IrBasicBlock, IrFunction, IrInstruction, IrModule, IrTerminator, LlvmBackend};
use debug_common::*;
use serde::Serialize;
use std::process;

const VERSION: &str = env!("CAY_SIR_VERSION");

#[derive(Debug, Clone, Default)]
struct SirOptions {
    common: CommonOptions,
    emit_llvm: bool,
    function_filter: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct InlineIrEntry {
    source_line: usize,
    raw_lines: Vec<String>,
    instruction: IrInstruction,
}

fn main() {
    let args = args();
    let (options, file_path) = match parse_sir_args(&args) {
        Ok((opts, path)) => {
            if opts.common.show_version {
                print_version("cay-sir", VERSION);
                return;
            }
            if opts.common.show_help {
                print_usage(
                    "cay-sir",
                    VERSION,
                    &[
                        "--llvm              同时输出 LLVM IR 文本",
                        "--function <name>   仅显示指定函数的 IR",
                    ],
                );
                return;
            }
            (opts, path)
        }
        Err(e) => exit_with_error(&e),
    };

    let source = match read_source(&file_path) {
        Ok(s) => s,
        Err(e) => exit_with_error(&e),
    };

    let processed = match preprocess_source(&source, &file_path, options.common.no_preprocess) {
        Ok(s) => s,
        Err(e) => exit_with_error(&e),
    };

    let mut ast = match parse_source(&processed) {
        Ok(a) => a,
        Err(e) => {
            print_error(&e, &processed, &file_path);
            process::exit(1);
        }
    };

    if !options.common.no_semantics {
        let mut analyzer = cavvy::semantic::SemanticAnalyzer::new();
        analyzer.set_current_file(Some(file_path.clone()));
        match analyzer.analyze(ast) {
            Ok(a) => ast = a,
            Err(e) => {
                print_error(&e, &processed, &file_path);
                process::exit(1);
            }
        }
    }

    let module_result = build_module(&ast, options.common.no_semantics);

    match module_result {
        Ok(module) => {
            if options.common.json_output {
                output_module_json(&module, options.function_filter.as_deref());
            } else {
                print_module_pretty(&module, &options);
            }

            if options.emit_llvm {
                match LlvmBackend::emit_module(&module) {
                    Ok(llvm_ir) => {
                        println!();
                        println!("=== LLVM IR ===");
                        println!("{}", llvm_ir);
                    }
                    Err(e) => {
                        eprintln!("警告: 无法生成 LLVM IR 文本: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("警告: 无法构建完整 IR 模块: {}", e);
            eprintln!("退回到仅解析 __ir 块...");
            let entries = collect_inline_ir_entries(&ast);
            if options.common.json_output {
                match serde_json::to_string_pretty(&entries) {
                    Ok(json) => println!("{}", json),
                    Err(e) => exit_with_error(&format!("JSON 序列化失败: {}", e)),
                }
            } else {
                print_inline_ir_entries(&entries, options.common.no_color);
            }
        }
    }
}

fn parse_sir_args(args: &[String]) -> Result<(SirOptions, String), String> {
    let mut options = SirOptions::default();
    let mut input_file: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                options.common.show_help = true;
                return Ok((options, String::new()));
            }
            "-v" | "--version" => {
                options.common.show_version = true;
                return Ok((options, String::new()));
            }
            "--json" => options.common.json_output = true,
            "--no-color" => options.common.no_color = true,
            "--no-preprocess" => options.common.no_preprocess = true,
            "--no-semantics" => options.common.no_semantics = true,
            "--llvm" => options.emit_llvm = true,
            "--function" => {
                options.function_filter = Some(take_next_arg(args, &mut i, "--function")?);
            }
            arg if arg.starts_with('-') => {
                return Err(format!("未知选项: {}", arg));
            }
            _ => {
                if input_file.is_none() {
                    input_file = Some(arg.clone());
                } else {
                    return Err(format!("多余参数: {}", arg));
                }
            }
        }
        i += 1;
    }

    let input_file = input_file.ok_or("需要指定输入文件")?;
    Ok((options, input_file))
}

fn build_module(ast: &Program, no_semantics: bool) -> Result<IrModule, String> {
    let mut builder = cavvy::ir::IrBuilder::new();

    if !no_semantics {
        // If semantic analysis was skipped, we don't have a registry;
        // the builder will use an empty one.
        // This path is best-effort only.
    }

    builder.build_from_ast(ast).map_err(|e| format!("{}", e))
}

fn output_module_json(module: &IrModule, function_filter: Option<&str>) {
    if let Some(name) = function_filter {
        if let Some(func) = module.functions.iter().find(|f| f.name == name) {
            match serde_json::to_string_pretty(func) {
                Ok(json) => println!("{}", json),
                Err(e) => exit_with_error(&format!("JSON 序列化失败: {}", e)),
            }
        } else {
            eprintln!("未找到函数: {}", name);
            process::exit(1);
        }
    } else {
        match serde_json::to_string_pretty(module) {
            Ok(json) => println!("{}", json),
            Err(e) => exit_with_error(&format!("JSON 序列化失败: {}", e)),
        }
    }
}

fn print_module_pretty(module: &IrModule, options: &SirOptions) {
    let header_color = if options.common.no_color {
        ""
    } else {
        "\x1b[1;36m"
    };
    let section_color = if options.common.no_color {
        ""
    } else {
        "\x1b[1;35m"
    };
    let func_color = if options.common.no_color {
        ""
    } else {
        "\x1b[1;33m"
    };
    let block_color = if options.common.no_color {
        ""
    } else {
        "\x1b[1;34m"
    };
    let reset = if options.common.no_color {
        ""
    } else {
        "\x1b[0m"
    };

    println!("{}=== Module: {} ==={}", header_color, module.name, reset);
    println!("{}Target:{}", section_color, reset);
    println!("  {}", module.target_triple);

    if !module.extern_declarations.is_empty() {
        println!("{}Extern declarations:{}", section_color, reset);
        for ext in &module.extern_declarations {
            let params: Vec<String> = ext
                .params
                .iter()
                .map(|(_, ty)| format!("{}", ty.to_llvm_str()))
                .collect();
            println!(
                "  declare {} @{}({})",
                ext.return_type.to_llvm_str(),
                ext.name,
                params.join(", ")
            );
        }
    }

    if !module.globals.is_empty() {
        println!("{}Globals:{}", section_color, reset);
        for global in &module.globals {
            println!(
                "  {} {} = {:?}",
                if global.is_constant {
                    "constant"
                } else {
                    "global"
                },
                global.name,
                global.initializer
            );
        }
    }

    println!();
    let functions: Vec<&IrFunction> = if let Some(ref name) = options.function_filter {
        module
            .functions
            .iter()
            .filter(|f| f.name == *name)
            .collect()
    } else {
        module.functions.iter().collect()
    };

    if functions.is_empty() {
        println!("（无函数）");
        return;
    }

    for func in functions {
        print_function(func, func_color, block_color, reset);
    }
}

fn print_function(func: &IrFunction, func_color: &str, block_color: &str, reset: &str) {
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| format!("{} {}", p.ty.to_llvm_str(), p.name))
        .collect();
    println!(
        "{}define {} @{}({}) {}{}",
        func_color,
        func.return_type.to_llvm_str(),
        func.name,
        params.join(", "),
        format!("({} blocks)", func.blocks.len()),
        reset
    );

    for block in &func.blocks {
        print_block(block, block_color, reset);
    }
    println!();
}

fn print_block(block: &IrBasicBlock, block_color: &str, reset: &str) {
    println!("{}  {}:{}", block_color, block.label, reset);
    for inst in &block.instructions {
        print_instruction(inst, "    ");
    }
    if let Some(term) = &block.terminator {
        println!("    {}", terminator_str(term));
    }
}

fn print_instruction(inst: &IrInstruction, indent: &str) {
    match inst {
        IrInstruction::InlineIr { lines, .. } => {
            println!("{}; --- Inline IR ---", indent);
            for line in lines {
                println!("{}{}", indent, line);
            }
            println!("{}; --- End Inline IR ---", indent);
        }
        _ => println!("{}{:?}", indent, inst),
    }
}

fn terminator_str(term: &IrTerminator) -> String {
    match term {
        IrTerminator::Return { value: None } => "ret void".to_string(),
        IrTerminator::Return { value: Some(v) } => format!("ret {}", v),
        IrTerminator::Branch { target } => format!("br label %{}", target),
        IrTerminator::ConditionalBranch {
            condition,
            true_target,
            false_target,
        } => format!(
            "br {}, label %{}, label %{}",
            condition, true_target, false_target
        ),
        IrTerminator::Switch {
            value,
            default_target,
            cases,
            ..
        } => {
            let case_strs: Vec<String> = cases
                .iter()
                .map(|(v, t)| format!("{}: label %{}", v, t))
                .collect();
            format!(
                "switch {}, label %{} [{}]",
                value,
                default_target,
                case_strs.join(" ")
            )
        }
        IrTerminator::Unreachable => "unreachable".to_string(),
    }
}

fn collect_inline_ir_entries(program: &Program) -> Vec<InlineIrEntry> {
    let mut entries = Vec::new();
    let parser = InlineIrParser::new();

    let mut blocks = Vec::new();
    collect_inline_ir_stmts(program, &mut blocks);

    for stmt in blocks {
        let raw_text = stmt.raw_lines.join("\n");
        match parser.parse(&raw_text, &[], &[]) {
            Ok(block) => {
                entries.push(InlineIrEntry {
                    source_line: stmt.loc.line,
                    raw_lines: stmt.raw_lines.clone(),
                    instruction: parser.to_instruction(&block),
                });
            }
            Err(e) => {
                eprintln!("警告: 第 {} 行的 __ir 块解析失败: {}", stmt.loc.line, e);
            }
        }
    }

    entries
}

fn print_inline_ir_entries(entries: &[InlineIrEntry], no_color: bool) {
    let header_color = if no_color { "" } else { "\x1b[1;36m" };
    let reset = if no_color { "" } else { "\x1b[0m" };

    if entries.is_empty() {
        println!("未找到 __ir 块。");
        return;
    }

    for entry in entries {
        println!(
            "{}=== Inline IR block at line {} ==={}",
            header_color, entry.source_line, reset
        );
        for line in &entry.raw_lines {
            println!("  {}", line);
        }
        println!();
    }
}

fn collect_inline_ir_stmts(program: &Program, result: &mut Vec<InlineIrStmt>) {
    for class in &program.classes {
        for member in &class.members {
            match member {
                cavvy::ast::ClassMember::Method(m) => {
                    if let Some(body) = &m.body {
                        collect_inline_ir_from_block(body, result);
                    }
                }
                cavvy::ast::ClassMember::Constructor(c) => {
                    collect_inline_ir_from_block(&c.body, result);
                }
                cavvy::ast::ClassMember::Destructor(d) => {
                    collect_inline_ir_from_block(&d.body, result);
                }
                cavvy::ast::ClassMember::InstanceInitializer(b)
                | cavvy::ast::ClassMember::StaticInitializer(b) => {
                    collect_inline_ir_from_block(b, result);
                }
                _ => {}
            }
        }
    }

    for func in &program.top_level_functions {
        collect_inline_ir_from_block(&func.body, result);
    }
}

fn collect_inline_ir_from_block(block: &cavvy::ast::Block, result: &mut Vec<InlineIrStmt>) {
    for stmt in &block.statements {
        collect_inline_ir_from_stmt(stmt, result);
    }
}

fn collect_inline_ir_from_stmt(stmt: &Stmt, result: &mut Vec<InlineIrStmt>) {
    match stmt {
        Stmt::InlineIr(ir) => result.push(ir.clone()),
        Stmt::Block(b) => collect_inline_ir_from_block(b, result),
        Stmt::If(s) => {
            collect_inline_ir_from_stmt(&s.then_branch, result);
            if let Some(else_branch) = &s.else_branch {
                collect_inline_ir_from_stmt(else_branch, result);
            }
        }
        Stmt::While(s) => collect_inline_ir_from_stmt(&s.body, result),
        Stmt::DoWhile(s) => collect_inline_ir_from_stmt(&s.body, result),
        Stmt::For(s) => {
            if let Some(init) = &s.init {
                collect_inline_ir_from_stmt(init, result);
            }
            collect_inline_ir_from_stmt(&s.body, result);
        }
        Stmt::ForEach(s) => collect_inline_ir_from_stmt(&s.body, result),
        Stmt::Switch(s) => {
            for case in &s.cases {
                for stmt in &case.body {
                    collect_inline_ir_from_stmt(stmt, result);
                }
            }
            if let Some(default) = &s.default {
                for stmt in default {
                    collect_inline_ir_from_stmt(stmt, result);
                }
            }
        }
        Stmt::Scope(s) => collect_inline_ir_from_block(&s.body, result),
        _ => {}
    }
}
