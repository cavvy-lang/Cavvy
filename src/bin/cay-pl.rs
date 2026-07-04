// cay-pl: Cavvy Pipeline Line classifier
// 显示每一行源代码在编译时被交给了 Codegen pipeline 还是 IR Builder pipeline。

mod debug_common;

use cavvy::ast::{
    Block, ClassDecl, ClassMember, ConstructorDecl, DestructorDecl, EnumDecl, InlineIrStmt,
    NamespaceDecl, Program, SpecializeClassDecl, Stmt, StructDecl,
};
use debug_common::*;
use serde::Serialize;
use std::process;

const VERSION: &str = env!("CAY_PL_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
enum Pipeline {
    Codegen,
    IrBuilder,
}

impl std::fmt::Display for Pipeline {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Pipeline::Codegen => write!(f, "Codegen"),
            Pipeline::IrBuilder => write!(f, "IR Builder"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct LineInfo {
    line: usize,
    pipeline: Pipeline,
    context: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct PipelineReport {
    file: String,
    lines: Vec<LineInfo>,
    summary: Summary,
}

#[derive(Debug, Clone, Serialize)]
struct Summary {
    total_lines: usize,
    codegen_lines: usize,
    ir_builder_lines: usize,
}

#[derive(Debug, Clone, Default)]
struct PlOptions {
    common: CommonOptions,
    show_ranges: bool,
}

fn main() {
    let args = args();
    let (options, file_path) = match parse_pl_args(&args) {
        Ok((opts, path)) => {
            if opts.common.show_version {
                print_version("cay-pl", VERSION);
                return;
            }
            if opts.common.show_help {
                print_usage(
                    "cay-pl",
                    VERSION,
                    &["--show-ranges       以行范围形式输出（更紧凑）"],
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

    let total_lines = processed.lines().count();
    let lines = classify_lines(&ast, total_lines, options.common.no_color);
    let summary = compute_summary(total_lines, &lines);

    if options.common.json_output {
        let report = PipelineReport {
            file: file_path,
            lines,
            summary,
        };
        match serde_json::to_string_pretty(&report) {
            Ok(json) => println!("{}", json),
            Err(e) => exit_with_error(&format!("JSON 序列化失败: {}", e)),
        }
    } else if options.show_ranges {
        print_ranges(&lines, &file_path, options.common.no_color);
    } else {
        print_table(&lines, options.common.no_color);
    }
}

fn parse_pl_args(args: &[String]) -> Result<(PlOptions, String), String> {
    let mut options = PlOptions::default();
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
            "--show-ranges" => options.show_ranges = true,
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

fn classify_lines(ast: &Program, total_lines: usize, _no_color: bool) -> Vec<LineInfo> {
    let mut lines: Vec<LineInfo> = (1..=total_lines)
        .map(|line| LineInfo {
            line,
            pipeline: Pipeline::Codegen,
            context: None,
        })
        .collect();

    let blocks = collect_inline_ir_stmts(ast);
    for block in blocks {
        let start = block.loc.line.saturating_sub(1);
        // raw_lines excludes the surrounding braces; add 1 for the closing brace line.
        let raw_len = block.raw_lines.len();
        let end = if raw_len == 0 {
            start
        } else {
            start + raw_len + 1
        }
        .min(total_lines.saturating_sub(1));

        let context = format!("__ir block at line {}", block.loc.line);
        for i in start..=end {
            lines[i].pipeline = Pipeline::IrBuilder;
            lines[i].context = Some(context.clone());
        }
    }

    lines
}

fn compute_summary(total_lines: usize, lines: &[LineInfo]) -> Summary {
    let codegen_lines = lines
        .iter()
        .filter(|l| l.pipeline == Pipeline::Codegen)
        .count();
    let ir_builder_lines = lines
        .iter()
        .filter(|l| l.pipeline == Pipeline::IrBuilder)
        .count();
    Summary {
        total_lines,
        codegen_lines,
        ir_builder_lines,
    }
}

fn print_table(lines: &[LineInfo], no_color: bool) {
    let header_color = if no_color { "" } else { "\x1b[1;36m" };
    let ir_color = if no_color { "" } else { "\x1b[1;33m" };
    let reset = if no_color { "" } else { "\x1b[0m" };

    println!("{}Line  Pipeline     Context{}", header_color, reset);
    println!("{}----  --------     -------{}", header_color, reset);
    for info in lines {
        let (pipeline_str, ctx) = if info.pipeline == Pipeline::IrBuilder {
            (
                format!("{}{}{}", ir_color, info.pipeline, reset),
                info.context.as_deref().unwrap_or(""),
            )
        } else {
            (format!("{}", info.pipeline), "")
        };
        println!("{:4}  {:12} {}", info.line, pipeline_str, ctx);
    }

    let summary = compute_summary(lines.len(), lines);
    println!();
    println!(
        "总计 {} 行: {} Codegen, {} IR Builder",
        summary.total_lines, summary.codegen_lines, summary.ir_builder_lines
    );
}

fn print_ranges(lines: &[LineInfo], file_path: &str, no_color: bool) {
    let ir_color = if no_color { "" } else { "\x1b[1;33m" };
    let reset = if no_color { "" } else { "\x1b[0m" };

    println!("文件: {}", file_path);
    let mut current_start = 0usize;
    let mut current_pipeline = Pipeline::Codegen;

    for (i, info) in lines.iter().enumerate() {
        if i == 0 {
            current_start = info.line;
            current_pipeline = info.pipeline;
        } else if info.pipeline != current_pipeline {
            print_range(
                current_start,
                lines[i - 1].line,
                current_pipeline,
                ir_color,
                reset,
            );
            current_start = info.line;
            current_pipeline = info.pipeline;
        }
    }
    if !lines.is_empty() {
        print_range(
            current_start,
            lines.last().unwrap().line,
            current_pipeline,
            ir_color,
            reset,
        );
    }

    let summary = compute_summary(lines.len(), lines);
    println!();
    println!(
        "总计 {} 行: {} Codegen, {} IR Builder",
        summary.total_lines, summary.codegen_lines, summary.ir_builder_lines
    );
}

fn print_range(start: usize, end: usize, pipeline: Pipeline, ir_color: &str, reset: &str) {
    if start == end {
        println!("  line {:4}: {}{}{}", start, ir_color, pipeline, reset);
    } else {
        println!(
            "  lines {:4}-{:4}: {}{}{}",
            start, end, ir_color, pipeline, reset
        );
    }
}

fn collect_inline_ir_stmts(program: &Program) -> Vec<InlineIrStmt> {
    let mut result = Vec::new();

    for ns in &program.namespace_decls {
        collect_inline_ir_from_namespace(ns, &mut result);
    }

    for class in &program.classes {
        collect_inline_ir_from_class(class, &mut result);
    }

    for st in &program.structs {
        collect_inline_ir_from_struct(st, &mut result);
    }

    for en in &program.enums {
        collect_inline_ir_from_enum(en, &mut result);
    }

    for func in &program.top_level_functions {
        collect_inline_ir_from_block(&func.body, &mut result);
    }

    for ext in &program.extern_declarations {
        for func in &ext.functions {
            // Extern declarations have no body; nothing to collect.
            let _ = func;
        }
    }

    for spec in &program.specialize_classes {
        collect_inline_ir_from_specialize_class(spec, &mut result);
    }

    result
}

fn collect_inline_ir_from_namespace(ns: &NamespaceDecl, result: &mut Vec<InlineIrStmt>) {
    for class in &ns.classes {
        collect_inline_ir_from_class(class, result);
    }
    for st in &ns.structs {
        collect_inline_ir_from_struct(st, result);
    }
    for en in &ns.enums {
        collect_inline_ir_from_enum(en, result);
    }
    for func in &ns.top_level_functions {
        collect_inline_ir_from_block(&func.body, result);
    }
    for nested in &ns.nested_namespaces {
        collect_inline_ir_from_namespace(nested, result);
    }
}

fn collect_inline_ir_from_class(class: &ClassDecl, result: &mut Vec<InlineIrStmt>) {
    for member in &class.members {
        match member {
            ClassMember::Method(m) => {
                if let Some(body) = &m.body {
                    collect_inline_ir_from_block(body, result);
                }
            }
            ClassMember::Constructor(c) => collect_inline_ir_from_constructor(c, result),
            ClassMember::Destructor(d) => collect_inline_ir_from_destructor(d, result),
            ClassMember::InstanceInitializer(b) | ClassMember::StaticInitializer(b) => {
                collect_inline_ir_from_block(b, result);
            }
            _ => {}
        }
    }
}

fn collect_inline_ir_from_struct(st: &StructDecl, result: &mut Vec<InlineIrStmt>) {
    for method in &st.methods {
        if let Some(body) = &method.body {
            collect_inline_ir_from_block(body, result);
        }
    }
}

fn collect_inline_ir_from_enum(_en: &EnumDecl, _result: &mut Vec<InlineIrStmt>) {
    // Cavvy enum declarations currently do not carry method bodies in the AST.
}

fn collect_inline_ir_from_specialize_class(
    spec: &SpecializeClassDecl,
    result: &mut Vec<InlineIrStmt>,
) {
    for member in &spec.members {
        match member {
            ClassMember::Method(m) => {
                if let Some(body) = &m.body {
                    collect_inline_ir_from_block(body, result);
                }
            }
            ClassMember::Constructor(c) => collect_inline_ir_from_constructor(c, result),
            ClassMember::Destructor(d) => collect_inline_ir_from_destructor(d, result),
            ClassMember::InstanceInitializer(b) | ClassMember::StaticInitializer(b) => {
                collect_inline_ir_from_block(b, result);
            }
            _ => {}
        }
    }
}

fn collect_inline_ir_from_constructor(ctor: &ConstructorDecl, result: &mut Vec<InlineIrStmt>) {
    collect_inline_ir_from_block(&ctor.body, result);
}

fn collect_inline_ir_from_destructor(dest: &DestructorDecl, result: &mut Vec<InlineIrStmt>) {
    collect_inline_ir_from_block(&dest.body, result);
}

fn collect_inline_ir_from_block(block: &Block, result: &mut Vec<InlineIrStmt>) {
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
