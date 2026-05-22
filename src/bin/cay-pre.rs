//! Cay-Pre - Cavvy预处理器专用工具
//!
//! 用于调试预处理器和查看预处理后的代码
//! 支持输出带源映射的预处理结果

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use cavvy::preprocessor::{Preprocessor, SourceMap};
use cavvy::error::print_miette_error;

const VERSION: &str = env!("CAY_PRE_VERSION");

/// 预处理器选项
struct PreprocessOptions {
    output_file: Option<String>,    // -o: 输出文件
    verbose: bool,                  // --verbose: 详细输出
    show_source_map: bool,          // --source-map: 显示源映射
    system_paths: Vec<PathBuf>,     // -I: 系统包含路径
}

impl Default for PreprocessOptions {
    fn default() -> Self {
        Self {
            output_file: None,
            verbose: false,
            show_source_map: false,
            system_paths: Vec::new(),
        }
    }
}

fn print_usage() {
    println!("Cavvy Preprocessor v{}", VERSION);
    println!("Usage: cay-pre [options] <file>");
    println!("");
    println!("Description:");
    println!("  预处理Cavvy源文件，展开所有预处理指令");
    println!("  包括: #include, #define, #ifdef/#ifndef/#else/#elif/#endif");
    println!("");
    println!("Options:");
    println!("  -o <file>              指定输出文件（默认输出到stdout）");
    println!("  -I<path>               添加系统包含路径");
    println!("  --source-map           显示源映射信息");
    println!("  --verbose, -v          显示详细处理信息");
    println!("  --version, -V          显示版本号");
    println!("  --help, -h             显示帮助信息");
    println!("");
    println!("Examples:");
    println!("  cay-pre hello.cay                    # 预处理并输出到stdout");
    println!("  cay-pre -o hello.pre.cay hello.cay   # 预处理并保存到文件");
    println!("  cay-pre -I./caylibs hello.cay        # 添加包含路径");
    println!("  cay-pre --source-map hello.cay       # 显示源映射");
}

fn parse_args(args: &[String]) -> Result<(PreprocessOptions, String), String> {
    let mut options = PreprocessOptions::default();
    let mut input_file: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        let arg = &args[i];

        if arg.starts_with("-I") && arg.len() > 2 {
            // -Ipath 格式
            options.system_paths.push(PathBuf::from(&arg[2..]));
        } else if arg == "-I" && i + 1 < args.len() {
            // -I path 格式
            i += 1;
            options.system_paths.push(PathBuf::from(&args[i]));
        } else {
            match arg.as_str() {
                "--version" | "-V" => {
                    println!("Cavvy Preprocessor v{}", VERSION);
                    process::exit(0);
                }
                "--help" | "-h" => {
                    print_usage();
                    process::exit(0);
                }
                "--verbose" | "-v" => {
                    options.verbose = true;
                }
                "--source-map" => {
                    options.show_source_map = true;
                }
                "-o" => {
                    if i + 1 < args.len() {
                        i += 1;
                        options.output_file = Some(args[i].clone());
                    } else {
                        return Err("错误: -o 选项需要指定输出文件".to_string());
                    }
                }
                _ => {
                    if arg.starts_with("-") {
                        return Err(format!("未知选项: {}", arg));
                    } else if input_file.is_none() {
                        input_file = Some(arg.clone());
                    } else {
                        return Err("错误: 只能指定一个输入文件".to_string());
                    }
                }
            }
        }
        i += 1;
    }

    match input_file {
        Some(file) => Ok((options, file)),
        None => Err("错误: 未指定输入文件".to_string()),
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let (options, input_file) = match parse_args(&args) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("{}", e);
            eprintln!("使用 --help 查看帮助信息");
            process::exit(1);
        }
    };

    // 读取输入文件
    let source = match fs::read_to_string(&input_file) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("错误: 无法读取文件 '{}': {}", input_file, e);
            process::exit(1);
        }
    };

    if options.verbose {
        eprintln!("[INFO] 预处理文件: {}", input_file);
        eprintln!("[INFO] 文件大小: {} 字节", source.len());
        if !options.system_paths.is_empty() {
            eprintln!("[INFO] 系统包含路径:");
            for path in &options.system_paths {
                eprintln!("       - {}", path.display());
            }
        }
    }

    // 获取文件所在目录作为基础目录
    let base_dir = Path::new(&input_file)
        .parent()
        .unwrap_or(Path::new("."));

    // 创建预处理器
    let mut preprocessor = if options.system_paths.is_empty() {
        Preprocessor::new(base_dir)
    } else {
        Preprocessor::with_include_paths(base_dir, options.system_paths.clone())
    };

    // 执行预处理
    let result = match preprocessor.process_with_source_map(&source, &input_file) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("预处理错误: {}", e);
            process::exit(1);
        }
    };

    if options.verbose {
        eprintln!("[INFO] 预处理完成");
        eprintln!("[INFO] 输出行数: {}", result.code.lines().count());
        eprintln!("[INFO] 源映射条目: {}", result.source_map.len());
    }

    // 输出结果
    let output = if options.show_source_map {
        format_with_source_map(&result.code, &result.source_map)
    } else {
        result.code
    };

    match options.output_file {
        Some(output_path) => {
            if let Err(e) = fs::write(&output_path, output) {
                eprintln!("错误: 无法写入文件 '{}': {}", output_path, e);
                process::exit(1);
            }
            if options.verbose {
                eprintln!("[INFO] 输出已保存到: {}", output_path);
            }
        }
        None => {
            println!("{}", output);
        }
    }
}

/// 将源代码与源映射合并格式化输出
fn format_with_source_map(code: &str, source_map: &SourceMap) -> String {
    let lines: Vec<&str> = code.lines().collect();
    let mut output = String::new();

    for (i, line) in lines.iter().enumerate() {
        let line_num = i + 1; // 1-based

        // 添加源映射注释
        if let Some(pos) = source_map.get_source_position(line_num) {
            output.push_str(&format!("// #source {} {}\n", pos.file, pos.line));
        }

        output.push_str(line);
        output.push('\n');
    }

    output
}
