use std::env;
use std::fs;
use std::process;
use std::path::{Path, PathBuf};
use cavvy::Compiler;
use cavvy::error::print_error_with_context;

/// 查找 clang 可执行文件
/// 1. 首先尝试直接调用 "clang"（系统 PATH 中）
/// 2. 如果失败，尝试查找编译器所在目录下的 llvm-minimal/bin/clang.exe
/// 3. 如果都找不到，返回错误
fn find_clang() -> Result<PathBuf, String> {
    // 1. 首先尝试系统 PATH 中的 clang
    if let Ok(output) = process::Command::new("clang").arg("--version").output() {
        if output.status.success() {
            return Ok(PathBuf::from("clang"));
        }
    }
    
    // 2. 尝试编译器所在目录下的 llvm-minimal
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bundled_clang = exe_dir.join("llvm-minimal/bin/clang.exe");
            if bundled_clang.exists() {
                return Ok(bundled_clang);
            }
        }
    }
    
    // 3. 都找不到，返回错误
    Err("找不到 clang 编译器。请确保 clang 已安装并在 PATH 中，或将 llvm-minimal 放在编译器同目录下。".to_string())
}

const VERSION: &str = env!("CAY-IR_VERSION");

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
    println!("Cavvy IR Generator v{}", VERSION);
    println!("Usage: cay-ir [options] <source_file.cay> [output_file.ll]");
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
    println!("  cay-ir hello.cay");
    println!("  cay-ir -O3 hello.cay hello.ll");
    println!("  cay-ir --opt-ir -O3 hello.cay         # 生成优化后的 IR");
    println!("  cay-ir --opt-ir --emit-optimized -O3 hello.cay  # 输出优化后的 IR");
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
                println!("Cavvy IR Generator v{}", VERSION);
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
        if input_file.ends_with(".cay") {
            input_file.replace(".cay", ".ll")
        } else {
            format!("{}.ll", input_file)
        }
    });

    Ok((options, input_file, output_file))
}

fn optimize_ir(ir_file: &str, opt_level: &str) -> Result<String, String> {
    let clang_exe = find_clang()?;

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

    println!("Cavvy IR Generator v{}", VERSION);
    println!("Compiling: {}", source_path);
    println!("Output: {}", output_path);
    if options.optimize_ir {
        println!("IR 优化: 启用 ({})", options.optimization);
    }
    println!("");

    // 编译 Cavvy → IR
    let compiler = Compiler::new();
    let temp_ir_file = format!("{}.tmp.ll", output_path.trim_end_matches(".ll"));

    match compiler.compile_file(&source_path, &temp_ir_file) {
        Ok(_) => {
            println!("  [+] Cavvy → IR 编译成功");
        }
        Err(e) => {
            print_error_with_context(&e, &source, &source_path);
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
