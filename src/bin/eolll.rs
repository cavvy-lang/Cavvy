use std::env;
use std::fs;
use std::process;
use std::path::Path;
use eol::Compiler;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct CompileOptions {
    optimization: String,    // -O0, -O1, -O2, -O3, -Os, -Oz
    optimize_ir: bool,       // --opt-ir: 使用 clang 优化 IR
    emit_optimized: bool,    // --emit-optimized: 输出发优化后的 IR
}

impl Default for CompileOptions {
    fn default() -> Self {
        CompileOptions {
            optimization: "-O2".to_string(),
            optimize_ir: false,
            emit_optimized: false,
        }
    }
}

fn print_usage() {
    println!("eolll v{}", VERSION);
    println!("Usage: eolll [options] <source_file.eol> [output_file.ll]");
    println!("");
    println!("Options:");
    println!("  -O0, -O1, -O2, -O3    编译器优化级别 (默认: -O2)");
    println!("  -Os, -Oz              优化代码大小");
    println!("  --opt-ir              使用 LLVM 优化 IR (增加编译时间，提高运行时性能)");
    println!("  --emit-optimized      输出优化后的 IR (与 --opt-ir 一起使用)");
    println!("  --version, -v         显示版本号");
    println!("  --help, -h            显示帮助信息");
    println!("");
    println!("Examples:");
    println!("  eolll hello.eol");
    println!("  eolll -O3 hello.eol hello.ll");
    println!("  eolll --opt-ir -O3 hello.eol         # 生成优化后的 IR");
    println!("  eolll --opt-ir --emit-optimized -O3 hello.eol  # 输出优化后的 IR");
}

fn parse_args(args: &[String]) -> Result<(CompileOptions, String, String), String> {
    let mut options = CompileOptions::default();
    let mut input_file: Option<String> = None;
    let mut output_file: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        let arg = &args[i];

        match arg.as_str() {
            "--version" | "-v" => {
                println!("eolll v{}", VERSION);
                process::exit(0);
            }
            "--help" | "-h" => {
                print_usage();
                process::exit(0);
            }
            "-O0" | "-O1" | "-O2" | "-O3" | "-Os" | "-Oz" => {
                options.optimization = arg.clone();
            }
            "--opt-ir" => {
                options.optimize_ir = true;
            }
            "--emit-optimized" => {
                options.emit_optimized = true;
            }
            _ => {
                if arg.starts_with('-') {
                    return Err(format!("未知选项: {}", arg));
                }
                if input_file.is_none() {
                    input_file = Some(arg.clone());
                } else if output_file.is_none() {
                    output_file = Some(arg.clone());
                } else {
                    return Err(format!("多余参数: {}", arg));
                }
            }
        }
        i += 1;
    }

    let input_file = input_file.ok_or("需要指定输入文件")?;
    let output_file = output_file.unwrap_or_else(|| {
        if input_file.ends_with(".eol") {
            input_file.replace(".eol", ".ll")
        } else {
            format!("{}.ll", input_file)
        }
    });

    Ok((options, input_file, output_file))
}

fn optimize_ir(ir_file: &str, opt_level: &str) -> Result<String, String> {
    let current_dir = env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let clang_exe = current_dir.join("llvm-minimal/bin/clang.exe");

    if !clang_exe.exists() {
        return Err("找不到 clang.exe，无法优化 IR".to_string());
    }

    // 生成临时优化后的文件名
    let optimized_file = format!("{}.opt.ll", ir_file.trim_end_matches(".ll"));

    // 使用 clang 优化 IR
    // -S -emit-llvm: 输出 LLVM IR
    // -x ir: 输入类型为 IR
    let mut cmd = process::Command::new(&clang_exe);
    cmd.arg("-x")
        .arg("ir")
        .arg(ir_file)
        .arg("-S")
        .arg("-emit-llvm")
        .arg(opt_level)
        .arg("-o")
        .arg(&optimized_file);

    let output = cmd.output()
        .map_err(|e| format!("执行 clang 优化失败: {}", e))?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(format!("IR 优化失败: {}", error_msg));
    }

    Ok(optimized_file)
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let (options, source_path, output_path) = match parse_args(&args) {
        Ok(result) => result,
        Err(e) => {
            eprintln!("错误: {}", e);
            print_usage();
            process::exit(1);
        }
    };

    // 读取源文件
    let source = match fs::read_to_string(&source_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("错误读取源文件 '{}': {}", source_path, e);
            process::exit(1);
        }
    };

    println!("eolll v{}", VERSION);
    println!("Compiling: {}", source_path);
    println!("Output: {}", output_path);
    if options.optimize_ir {
        println!("IR 优化: 启用 ({ })", options.optimization);
    }
    println!("");

    // 编译 EOL → IR
    let compiler = Compiler::new();
    let temp_ir_file = format!("{}.tmp.ll", output_path.trim_end_matches(".ll"));

    match compiler.compile(&source, &temp_ir_file) {
        Ok(_) => {
            println!("  [+] EOL → IR 编译成功");
        }
        Err(e) => {
            eprintln!("Compilation error: {}", e);
            let _ = fs::remove_file(&temp_ir_file);
            process::exit(1);
        }
    }

    // 如果需要优化 IR
    let final_ir_file = if options.optimize_ir {
        println!("");
        println!("[2] 优化 IR ({})...", options.optimization);
        match optimize_ir(&temp_ir_file, &options.optimization) {
            Ok(optimized_file) => {
                println!("  [+] IR 优化完成");
                // 删除临时文件
                let _ = fs::remove_file(&temp_ir_file);
                optimized_file
            }
            Err(e) => {
                eprintln!("  [W] IR 优化失败: {}", e);
                eprintln!("  [I] 使用未优化的 IR");
                temp_ir_file
            }
        }
    } else {
        temp_ir_file
    };

    // 移动/复制到最终输出位置
    let final_output = if options.emit_optimized && options.optimize_ir {
        output_path
    } else if options.optimize_ir {
        // 如果不输出优化后的 IR，但进行了优化，重命名为普通名称
        format!("{}.ll", output_path.trim_end_matches(".ll"))
    } else {
        output_path
    };

    if final_ir_file != final_output {
        if let Err(e) = fs::rename(&final_ir_file, &final_output) {
            // 如果重命名失败（可能跨磁盘），尝试复制
            if let Err(e2) = fs::copy(&final_ir_file, &final_output) {
                eprintln!("错误: 无法创建输出文件 '{}': {} / {}", final_output, e, e2);
                let _ = fs::remove_file(&final_ir_file);
                process::exit(1);
            }
            let _ = fs::remove_file(&final_ir_file);
        }
    }

    // 获取文件大小
    let ir_size = fs::metadata(&final_output)
        .map(|m| m.len() as f64 / 1024.0)
        .unwrap_or(0.0);

    println!("");
    println!("Compilation successful!");
    println!("Generated: {} ({:.1} KB)", final_output, ir_size);
}
