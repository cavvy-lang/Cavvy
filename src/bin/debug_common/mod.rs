//! Shared helpers for Cavvy debug CLI binaries (cay-ast, cay-pl, cay-sir).
#![allow(dead_code)]

use cavvy::miette_diagnostic::{CayError, print_error_with_context};
use cavvy::lexer;
use cavvy::parser;
use cavvy::preprocessor;
use std::env;
use std::fs;
use std::path::Path;
use std::process;

/// Common CLI options shared by all debug binaries.
#[derive(Debug, Clone, Default)]
pub struct CommonOptions {
    pub json_output: bool,
    pub no_color: bool,
    pub no_preprocess: bool,
    pub no_semantics: bool,
    pub compact: bool,
    pub show_version: bool,
    pub show_help: bool,
}

/// Read a source file from disk.
pub fn read_source(path: &str) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("无法读取源文件 '{}': {}", path, e))
}

/// Preprocess source unless disabled.
pub fn preprocess_source(source: &str, path: &str, no_preprocess: bool) -> Result<String, String> {
    if no_preprocess {
        return Ok(source.to_string());
    }

    let base_dir = Path::new(path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or(".");

    preprocessor::preprocess(source, path, base_dir)
        .map_err(|e| format!("预处理错误: {}", e))
}

/// Lex and parse source into an AST.
pub fn parse_source(source: &str) -> Result<cavvy::ast::Program, CayError> {
    let tokens = lexer::lex(source)?;
    parser::parse_with_source(tokens, source.to_string())
}

/// Pretty-print a compiler error with source context.
pub fn print_error(error: &CayError, source: &str, path: &str) {
    print_error_with_context(error, source, path);
}

/// Parse common arguments, returning options and the input file path.
pub fn parse_common_args(args: &[String]) -> Result<(CommonOptions, String), String> {
    let mut options = CommonOptions::default();
    let mut input_file: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "-h" | "--help" => {
                options.show_help = true;
                return Ok((options, String::new()));
            }
            "-v" | "--version" => {
                options.show_version = true;
                return Ok((options, String::new()));
            }
            "--json" => options.json_output = true,
            "--no-color" => options.no_color = true,
            "--no-preprocess" => options.no_preprocess = true,
            "--no-semantics" => options.no_semantics = true,
            "--compact" => options.compact = true,
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

/// Parse an optional value flag (e.g. `--function name`).
pub fn take_next_arg(args: &[String], i: &mut usize, flag: &str) -> Result<String, String> {
    *i += 1;
    if *i >= args.len() {
        return Err(format!("{} 需要一个参数", flag));
    }
    Ok(args[*i].clone())
}

/// Print a standard version line.
pub fn print_version(tool_name: &str, version: &str) {
    println!("{} v{}", tool_name, version);
}

/// Print standard usage help.
pub fn print_usage(tool_name: &str, version: &str, extra_flags: &[&str]) {
    eprintln!("{} v{}", tool_name, version);
    eprintln!("Usage: {} [options] <source_file.cay>", tool_name);
    eprintln!();
    eprintln!("Options:");
    eprintln!("  --json              以 JSON 格式输出");
    eprintln!("  --no-preprocess     跳过预处理器");
    eprintln!("  --no-semantics      跳过语义分析");
    eprintln!("  --no-color          禁用彩色输出");
    eprintln!("  --compact           紧凑输出模式");
    for flag in extra_flags {
        eprintln!("  {}", flag);
    }
    eprintln!("  -v, --version       显示版本号");
    eprintln!("  -h, --help          显示帮助信息");
}

/// Exit the process with a CLI error message.
pub fn exit_with_error(message: &str) -> ! {
    eprintln!("错误: {}", message);
    process::exit(1)
}

/// Read command-line arguments.
pub fn args() -> Vec<String> {
    env::args().collect()
}
