use cavvy::Compiler;
use cavvy::miette_diagnostic::{
    print_error_with_context, print_miette_error, print_tool_error, print_warning,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

/// 查找 clang 可执行文件
/// 1. 首先尝试直接调用 "clang"（系统 PATH 中）
/// 2. 如果失败，尝试查找编译器所在目录下捆绑的 llvm-minimal/clang（Windows 为 clang.exe）
///    注意：捆绑目录是扁平布局，可执行文件直接位于 llvm-minimal/ 下
/// 3. 如果都找不到，返回错误
fn find_clang() -> Result<PathBuf, String> {
    // 1. 首先尝试系统 PATH 中的 clang
    if let Ok(output) = process::Command::new("clang").arg("--version").output() {
        if output.status.success() {
            return Ok(PathBuf::from("clang"));
        }
    }

    // 2. 尝试编译器所在目录下的 llvm-minimal（扁平布局）
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let clang_name = if cfg!(windows) { "clang.exe" } else { "clang" };
            let bundled_clang = exe_dir.join("llvm-minimal").join(clang_name);
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
    optimization: String,       // -O0, -O1, -O2, -O3, -Os, -Oz
    optimize_ir: bool,          // --opt-ir: 使用 clang 优化 IR
    emit_optimized: bool,       // --emit-optimized: 输出发优化后的 IR
    debug: bool,                // -g: 生成 DWARF 调试信息
    target_os: String,          // --target: 目标操作系统
    features: Vec<String>,      // -f:XX 或 --feature:XX 开启特性
    no_features: Vec<String>,   // -No:XX 关闭特性
    defines: Vec<String>,       // -D:XX 定义宏
    undefines: Vec<String>,     // -U:XX 取消定义宏
    obfuscate: bool,            // --obfuscate 混淆 IR 代码
    include_paths: Vec<String>, // -I:XX 包含路径
    detect_cycles: bool,        // --detect-cycles 启用 Rc 循环引用检测
    no_panic: bool,             // --no-panic panic()/abort() 转为编译错误
}

impl Default for CompileOptions {
    fn default() -> Self {
        CompileOptions {
            include_paths: Vec::new(),
            optimization: "-O2".to_string(),
            optimize_ir: false,
            emit_optimized: false,
            debug: false,
            target_os: std::env::consts::OS.to_string(),
            features: Vec::new(),
            no_features: Vec::new(),
            defines: Vec::new(),
            undefines: Vec::new(),
            obfuscate: false,
            detect_cycles: false,
            no_panic: false,
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
    println!("  -g                    生成 DWARF 调试信息");
    println!("  --opt-ir              使用 LLVM 优化 IR (增加编译时间，提高运行时性能)");
    println!("  --emit-optimized      输出优化后的 IR (与 --opt-ir 一起使用)");
    println!("  --target <os>         目标操作系统 (windows, linux, macos)");
    println!("  --obfuscate           混淆 IR 代码");
    println!("  --detect-cycles       启用 Rc<T> 循环引用运行时检测");
    println!("  -f:XX, --feature:XX   启用特定功能");
    println!("  -No:XX                禁用特定功能");
    println!("  -D:XX                 定义宏（兼容旧语法）");
    println!("  -U:XX                 取消定义宏（兼容旧语法）");
    println!("  -D<macro>[=<value>], --define=<macro>[=<value>]  定义宏");
    println!("  -U<macro>, --undefine=<macro>                        取消定义宏");
    println!("  -I<<XX>>              添加包含搜索路径");
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
            "--target" => {
                if i + 1 < args.len() {
                    options.target_os = args[i + 1].clone();
                    i += 1;
                } else {
                    return Err("--target 需要一个参数，如 windows、linux、macos".to_string());
                }
            }
            "--obfuscate" => {
                options.obfuscate = true;
            }
            "-g" => {
                options.debug = true;
            }
            "--detect-cycles" => {
                options.detect_cycles = true;
            }
            "--no-panic" => {
                options.no_panic = true;
            }
            "-o" => {
                if i + 1 < args.len() {
                    output_file = Some(args[i + 1].clone());
                    i += 1;
                } else {
                    return Err("-o 需要一个输出文件参数".to_string());
                }
            }
            arg if arg.starts_with("-f:") || arg.starts_with("--feature:") => {
                let feature = if arg.starts_with("-f:") {
                    &arg[3..]
                } else {
                    &arg[10..]
                };
                options.features.push(feature.to_string());
            }
            arg if arg.starts_with("-No:") => {
                let feature = &arg[4..];
                options.no_features.push(feature.to_string());
            }
            arg if arg.starts_with("-D:") => {
                let define = &arg[3..];
                options.defines.push(define.to_string());
            }
            arg if arg.starts_with("-U:") => {
                let undefine = &arg[3..];
                options.undefines.push(undefine.to_string());
            }
            arg if arg.starts_with("-I:") => {
                let include_path = &arg[3..];
                options.include_paths.push(include_path.to_string());
            }
            "--define" | "-D" => {
                i += 1;
                if i >= args.len() {
                    return Err("--define/-D 需要宏名称参数".to_string());
                }
                options.defines.push(args[i].clone());
            }
            "--undefine" | "-U" => {
                i += 1;
                if i >= args.len() {
                    return Err("--undefine/-U 需要宏名称参数".to_string());
                }
                options.undefines.push(args[i].clone());
            }
            arg if arg.starts_with("--define=") => {
                options.defines.push(arg[10..].to_string());
            }
            arg if arg.starts_with("--undefine=") => {
                options.undefines.push(arg[12..].to_string());
            }
            arg if arg.starts_with("-D") => {
                options.defines.push(arg[2..].to_string());
            }
            arg if arg.starts_with("-U") => {
                options.undefines.push(arg[2..].to_string());
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

    let output = cmd
        .output()
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
            print_miette_error(
                "cavvy::argument_error",
                &e,
                Some("请检查命令行参数是否正确"),
            );
            print_usage();
            process::exit(1);
        }
    };

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

    println!("Cavvy IR Generator v{}", VERSION);
    println!("Compiling: {}", source_path);
    println!("Output: {}", output_path);
    if options.optimize_ir {
        println!("IR 优化: 启用 ({})", options.optimization);
    }
    println!("");

    // 创建多平台编译器配置
    let compiler_options = cavvy::CompilerOptions {
        target_os: options.target_os,
        features: options.features,
        no_features: options.no_features,
        defines: options.defines,
        undefines: options.undefines,
        obfuscate: options.obfuscate,
        debug: options.debug,
        include_paths: Vec::new(),
        test_mode: false,
        detect_cycles: options.detect_cycles,
        no_panic: options.no_panic,
    };

    // 编译 Cavvy → IR
    let compiler = Compiler::with_options(compiler_options);
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
                print_warning(&format!("IR 优化失败: {}", e));
                println!("  [I] 使用未优化的 IR");
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
                print_miette_error(
                    "cavvy::io_error",
                    &format!("无法创建输出文件 '{}': {} / {}", final_output, e, e2),
                    Some("请检查输出目录是否有写入权限"),
                );
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
