use cavvy::Compiler;
use cavvy::miette_diagnostic::print_miette_error;
use std::env;
use std::fs;
use std::process;

fn print_usage() {
    println!("Usage: cayc <source_file.cay> [output_file.ll]");
    println!("");
    println!("Cay Compiler");
    println!("Compiles .cay source files to LLVM IR (.ll)");
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let source_path = &args[1];
    let output_path = if args.len() >= 3 {
        args[2].clone()
    } else {
        // 默认输出文件名
        if source_path.ends_with(".cay") {
            source_path.replace(".cay", ".ll")
        } else {
            format!("{}.ll", source_path)
        }
    };

    // 读取源文件
    let source = match fs::read_to_string(source_path) {
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

    println!("Compiling: {}", source_path);
    println!("Output: {}", output_path);
    println!("");

    // 编译
    let compiler = Compiler::new();
    match compiler.compile(&source, &output_path) {
        Ok(_) => {
            println!("");
            println!("Compilation successful!");
            println!("Generated: {}", output_path);
        }
        Err(e) => {
            print_miette_error(
                "cavvy::compile_error",
                &format!("编译错误: {}", e),
                Some("请检查代码语法和语义是否正确"),
            );
            process::exit(1);
        }
    }
}
