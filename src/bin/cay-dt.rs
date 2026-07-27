// cay-dt: Cavvy Debugger - Token PreViewer
// Token 预览工具 - 显示源代码的词法分析结果

use cavvy::lexer::{TokenWithLocation, lex_with_diagnostics};
use cavvy::miette_diagnostic::CayError;
use cavvy::preprocessor::preprocess;
use std::env;
use std::fs;
use std::path::Path;
use std::process;

const VERSION: &str = env!("CAY_DT_VERSION");

fn print_usage(program: &str) {
    eprintln!("Cavvy Debugger - Token PreViewer v{}", VERSION);
    eprintln!("用法: {} <源文件.cay> [选项]", program);
    eprintln!();
    eprintln!("选项:");
    eprintln!("  --json          以 JSON 格式输出 tokens");
    eprintln!("  --no-color      禁用彩色输出");
    eprintln!("  --show-location 显示详细位置信息");
    eprintln!("  --no-preprocess 禁用预处理器");
    eprintln!("  -h, --help      显示帮助信息");
    eprintln!("  -v, --version   显示版本信息");
}

#[derive(Debug)]
struct Options {
    json_output: bool,
    no_color: bool,
    show_location: bool,
    no_preprocess: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            json_output: false,
            no_color: false,
            show_location: false,
            no_preprocess: false,
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    if args.len() < 2 {
        print_usage(&program);
        process::exit(1);
    }

    let mut options = Options::default();
    let mut file_path: Option<String> = None;

    for arg in &args[1..] {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage(&program);
                process::exit(0);
            }
            "-v" | "--version" => {
                println!("cay-dt v{}", VERSION);
                process::exit(0);
            }
            "--json" => options.json_output = true,
            "--no-color" => options.no_color = true,
            "--show-location" => options.show_location = true,
            "--no-preprocess" => options.no_preprocess = true,
            _ if arg.starts_with('-') => {
                eprintln!("错误: 未知选项 {}", arg);
                print_usage(&program);
                process::exit(1);
            }
            _ => {
                if file_path.is_none() {
                    file_path = Some(arg.clone());
                } else {
                    eprintln!("错误: 只能指定一个源文件");
                    process::exit(1);
                }
            }
        }
    }

    let file_path = match file_path {
        Some(path) => path,
        None => {
            eprintln!("错误: 未指定源文件");
            print_usage(&program);
            process::exit(1);
        }
    };

    let source = match fs::read_to_string(&file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("错误: 无法读取文件 '{}': {}", file_path, e);
            process::exit(1);
        }
    };

    // 预处理阶段
    let source_to_lex = if options.no_preprocess {
        source
    } else {
        let base_dir = Path::new(&file_path)
            .parent()
            .map(|p| p.to_str().unwrap_or("."))
            .unwrap_or(".");

        match preprocess(&source, &file_path, base_dir) {
            Ok(processed) => processed,
            Err(e) => {
                eprintln!("预处理错误: {}", e);
                process::exit(1);
            }
        }
    };

    let (tokens, diagnostics) = lex_with_diagnostics(&source_to_lex);

    if options.json_output {
        print_tokens_json(&tokens, &diagnostics);
    } else {
        print_tokens_pretty(&tokens, &diagnostics, &options, &file_path);
    }

    if !diagnostics.is_empty() {
        process::exit(1);
    }
}

fn print_tokens_pretty(
    tokens: &[TokenWithLocation],
    diagnostics: &[CayError],
    options: &Options,
    file_path: &str,
) {
    let header_color = if options.no_color { "" } else { "\x1b[1;36m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let token_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let loc_color = if options.no_color { "" } else { "\x1b[90m" };
    let value_color = if options.no_color { "" } else { "\x1b[32m" };
    let error_color = if options.no_color { "" } else { "\x1b[1;31m" };
    let warning_color = if options.no_color { "" } else { "\x1b[1;33m" };

    println!(
        "{}╔══════════════════════════════════════════════════════════════╗{}",
        header_color, reset
    );
    println!(
        "{}║         Cavvy Debugger - Token PreViewer v{}              ║{}",
        header_color, VERSION, reset
    );
    println!(
        "{}╚══════════════════════════════════════════════════════════════╝{}",
        header_color, reset
    );
    println!();
    println!("源文件: {}", file_path);
    println!("Token 数量: {}", tokens.len());
    println!();

    if !diagnostics.is_empty() {
        println!("{}诊断信息:{}", header_color, reset);
        for diag in diagnostics.iter() {
            let sev = diag.severity();
            let color = if sev == cavvy::miette_diagnostic::Severity::Error {
                error_color
            } else {
                warning_color
            };
            let severity = if sev == cavvy::miette_diagnostic::Severity::Error {
                "错误"
            } else {
                "警告"
            };
            let (line, col) = diag.location().unwrap_or((0, 0));
            println!(
                "  {}{}[{}]{} {} (行 {}, 列 {})",
                color,
                severity,
                diag.error_code(),
                reset,
                diag.message(),
                line,
                col
            );
        }
        println!();
    }

    println!("{}Tokens:{}", header_color, reset);
    println!("{:<5} {:<25} {:<20} {}", "序号", "Token 类型", "值", "位置");
    println!("{}", "─".repeat(80));

    for (i, token) in tokens.iter().enumerate() {
        let token_name = format!("{:?}", token.token);
        let token_value = get_token_value(&token.token);
        let location = if options.show_location {
            format!(
                "行 {}, 列 {} (源: {:?})",
                token.loc.line,
                token.loc.column,
                token.source_file.as_deref().unwrap_or("N/A")
            )
        } else {
            format!("行 {}, 列 {}", token.loc.line, token.loc.column)
        };

        if token_value.is_empty() {
            println!(
                "{:<5} {}{}{} {:20} {}{}{}",
                i, token_color, token_name, reset, "", loc_color, location, reset
            );
        } else {
            println!(
                "{:<5} {}{:<25}{} {}{:<20}{} {}{}{}",
                i,
                token_color,
                token_name,
                reset,
                value_color,
                token_value,
                reset,
                loc_color,
                location,
                reset
            );
        }
    }
}

fn print_tokens_json(tokens: &[TokenWithLocation], diagnostics: &[CayError]) {
    use std::io::Write;

    // 用 serde_json 构建，保证输出一定是合法 JSON（手写拼接会产生尾随逗号）
    let tokens_json: Vec<serde_json::Value> = tokens
        .iter()
        .enumerate()
        .map(|(i, token)| {
            let mut obj = serde_json::json!({
                "index": i,
                "type": format!("{:?}", token.token),
                "value": get_token_value(&token.token),
                "line": token.loc.line,
                "column": token.loc.column,
            });
            if let Some(ref file) = token.source_file {
                obj["source_file"] = serde_json::json!(file);
            }
            if let Some(line) = token.source_line {
                obj["source_line"] = serde_json::json!(line);
            }
            obj
        })
        .collect();

    let diagnostics_json: Vec<serde_json::Value> = diagnostics
        .iter()
        .map(|diag| {
            let (line, col) = diag.location().unwrap_or((0, 0));
            serde_json::json!({
                "severity": format!("{:?}", diag.severity()),
                "code": diag.error_code(),
                "message": diag.message(),
                "line": line,
                "column": col,
            })
        })
        .collect();

    let output = serde_json::json!({
        "tokens": tokens_json,
        "diagnostics": diagnostics_json,
    });
    let output = serde_json::to_string_pretty(&output)
        .unwrap_or_else(|e| format!("{{\"error\": \"JSON 序列化失败: {}\"}}", e));

    if let Err(e) = std::io::stdout().write_all(output.as_bytes()) {
        eprintln!("写入stdout失败: {}", e);
    }
}

fn get_token_value(token: &cavvy::lexer::Token) -> String {
    use cavvy::lexer::Token;

    match token {
        Token::Identifier(s) => s.clone(),
        Token::StringLiteral(Some(s)) => format!("\"{}\"", s),
        Token::StringLiteral(None) => "\"\"".to_string(),
        Token::CharLiteral(Some(c)) => format!("'{}'", c),
        Token::CharLiteral(None) => "''".to_string(),
        Token::IntegerLiteral(Some((i, _))) => i.to_string(),
        Token::IntegerLiteral(None) => String::new(),
        Token::FloatLiteral(Some((f, _))) => f.to_string(),
        Token::FloatLiteral(None) => String::new(),
        _ => String::new(),
    }
}
