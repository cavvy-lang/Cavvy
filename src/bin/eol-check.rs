use std::env;
use std::fs;
use std::process;
use eol::error::print_error_with_context;
use eol::lexer;
use eol::parser;
use eol::semantic;

const VERSION: &str = env!("EOL_CHECK_VERSION");

fn print_usage() {
    println!("eol-check v{}", VERSION);
    println!("Usage: eol-check [options] <source_file.eol>");
    println!("");
    println!("Options:");
    println!("  --lex-only            只进行词法分析");
    println!("  --parse-only          进行词法和语法分析（不进行语义分析）");
    println!("  --version, -v         显示版本号");
    println!("  --help, -h            显示帮助信息");
    println!("");
    println!("Examples:");
    println!("  eol-check hello.eol");
    println!("  eol-check --lex-only hello.eol");
    println!("  eol-check --parse-only hello.eol");
}

#[derive(Debug, Clone, Copy)]
enum CheckLevel {
    LexOnly,
    ParseOnly,
    Full,
}

impl Default for CheckLevel {
    fn default() -> Self {
        CheckLevel::Full
    }
}

struct CheckOptions {
    level: CheckLevel,
}

impl Default for CheckOptions {
    fn default() -> Self {
        CheckOptions {
            level: CheckLevel::default(),
        }
    }
}

fn parse_args(args: &[String]) -> Result<(CheckOptions, String), String> {
    let mut options = CheckOptions::default();
    let mut input_file: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        let arg = &args[i];

        match arg.as_str() {
            "--version" | "-v" => {
                println!("eol-check v{}", VERSION);
                process::exit(0);
            }
            "--help" | "-h" => {
                print_usage();
                process::exit(0);
            }
            "--lex-only" => {
                options.level = CheckLevel::LexOnly;
            }
            "--parse-only" => {
                options.level = CheckLevel::ParseOnly;
            }
            _ => {
                if arg.starts_with('-') {
                    return Err(format!("未知选项: {}", arg));
                }
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

fn main() {
    let args: Vec<String> = env::args().collect();

    let (options, source_path) = match parse_args(&args) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("错误: {}", e);
            print_usage();
            process::exit(1);
        }
    };

    println!("eol-check v{}", VERSION);
    println!("检查文件: {}", source_path);
    println!("检查级别: {}", match options.level {
        CheckLevel::LexOnly => "词法分析",
        CheckLevel::ParseOnly => "语法分析",
        CheckLevel::Full => "完整检查（词法+语法+语义）",
    });
    println!("");

    let source = match fs::read_to_string(&source_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("错误读取源文件 '{}': {}", source_path, e);
            process::exit(1);
        }
    };

    let start_time = std::time::Instant::now();

    match options.level {
        CheckLevel::LexOnly => {
            println!("[1] 词法分析...");
            match lexer::lex(&source) {
                Ok(tokens) => {
                    let elapsed = start_time.elapsed();
                    println!("  [+] 词法分析通过");
                    println!("      发现 {} 个 token", tokens.len());
                    println!("");
                    println!("[+] 语法检查完成! (耗时: {:?})", elapsed);
                }
                Err(e) => {
                    print_error_with_context(&e, &source, &source_path);
                    process::exit(1);
                }
            }
        }
        CheckLevel::ParseOnly => {
            println!("[1] 词法分析...");
            let tokens = match lexer::lex(&source) {
                Ok(tokens) => {
                    println!("  [+] 词法分析通过");
                    tokens
                }
                Err(e) => {
                    print_error_with_context(&e, &source, &source_path);
                    process::exit(1);
                }
            };

            println!("");
            println!("[2] 语法分析...");
            match parser::parse(tokens) {
                Ok(ast) => {
                    let elapsed = start_time.elapsed();
                    println!("  [+] 语法分析通过");
                    println!("      发现 {} 个类定义", ast.classes.len());
                    println!("");
                    println!("[+] 语法检查完成! (耗时: {:?})", elapsed);
                }
                Err(e) => {
                    print_error_with_context(&e, &source, &source_path);
                    process::exit(1);
                }
            }
        }
        CheckLevel::Full => {
            println!("[1] 词法分析...");
            let tokens = match lexer::lex(&source) {
                Ok(tokens) => {
                    println!("  [+] 词法分析通过");
                    tokens
                }
                Err(e) => {
                    print_error_with_context(&e, &source, &source_path);
                    process::exit(1);
                }
            };

            println!("");
            println!("[2] 语法分析...");
            let ast = match parser::parse(tokens) {
                Ok(ast) => {
                    println!("  [+] 语法分析通过");
                    ast
                }
                Err(e) => {
                    print_error_with_context(&e, &source, &source_path);
                    process::exit(1);
                }
            };

            println!("");
            println!("[3] 语义分析...");
            let mut analyzer = semantic::SemanticAnalyzer::new();
            match analyzer.analyze(&ast) {
                Ok(_) => {
                    let elapsed = start_time.elapsed();
                    println!("  [+] 语义分析通过");
                    println!("");
                    println!("[+] 语法检查完成! (耗时: {:?})", elapsed);
                }
                Err(e) => {
                    print_error_with_context(&e, &source, &source_path);
                    process::exit(1);
                }
            }
        }
    }
}
