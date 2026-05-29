use std::env;
use std::process;
use std::path::{Path, PathBuf};
use cavvy::error::{print_miette_error, print_tool_error, print_warning};
use cavvy::ir2exe_lib::{
    Ir2ExeOptions, compile_ir_to_exe, parse_source_map_from_ir, 
    normalize_path, IRSourceMap
};

const VERSION: &str = env!("IR2EXE_VERSION");

/// 根据当前操作系统自动选择默认目标平台
fn get_default_target() -> String {
    if cfg!(target_os = "windows") {
        "x86_64-w64-mingw32".to_string()
    } else if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-gnu".to_string()
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin".to_string()
    } else {
        std::env::var("TARGET").unwrap_or_else(|_| {
            if cfg!(target_arch = "x86_64") {
                "x86_64-unknown-linux-gnu".to_string()
            } else if cfg!(target_arch = "aarch64") {
                "aarch64-unknown-linux-gnu".to_string()
            } else {
                "x86_64-unknown-linux-gnu".to_string()
            }
        })
    }
}

/// 获取默认目标平台（用于帮助信息）
fn get_default_target_for_help() -> &'static str {
    if cfg!(target_os = "windows") {
        "x86_64-w64-mingw32"
    } else if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin"
    } else {
        "x86_64-unknown-linux-gnu"
    }
}

/// 获取输出文件扩展名示例
fn get_output_extension() -> &'static str {
    if cfg!(target_os = "windows") {
        "output.exe"
    } else {
        "output"
    }
}

fn print_usage() {
    let default_target = get_default_target_for_help();
    let output_ext = get_output_extension();

    println!("ir2exe v{}", VERSION);
    println!("Usage: ir2exe [options] <input_file.ll> [output_file]");
    println!("");
    println!("Optimization Options:");
    println!("  -O0, -O1, -O2, -O3    优化级别 (默认: -O2)");
    println!("  -Os, -Oz              优化代码大小");
    println!("  --lto[=<type>]        链接时优化 (full/thin)");
    println!("  --march <arch>        指定目标 CPU 架构 (如 x86-64-v3, native)");
    println!("  --mtune <cpu>         针对特定 CPU 优化 (如 intel, znver3)");
    println!("  --mcpu <cpu>          针对 ARM/AArch64 CPU 优化");
    println!("  --msse <ver>          启用 SSE (1/2/3/4.1/4.2)");
    println!("  --mavx <ver>          启用 AVX (avx/avx2/avx512f)");
    println!("  --mneon               启用 ARM NEON");
    println!("  --funroll-loops       循环展开");
    println!("  --fvectorize          启用自动向量化");
    println!("  --fslp-vectorize      启用 SLP 向量化");
    println!("  --fomit-frame-pointer 省略帧指针");
    println!("");
    println!("PGO (Profile Guided Optimization):");
    println!("  --pgo-gen             生成性能分析数据");
    println!("  --pgo-use <path>      使用性能分析数据优化");
    println!("  --pgo-cs              上下文敏感的性能分析");
    println!("");
    println!("Code Generation:");
    println!("  -g                    生成调试信息");
    println!("  -L<path>              添加库搜索路径");
    println!("  -l<lib>               链接额外的库");
    println!("  --ldflags <flags>     传递额外的链接器标志");
    println!("  --cflags <flags>      传递额外的编译器标志");
    println!("  --static              静态链接");
    println!("  -fPIC                 生成位置无关代码");
    println!("  --target <target>     指定目标平台 (默认: {})", default_target);
    println!("  --fno-exceptions      禁用异常处理");
    println!("  --fno-rtti            禁用运行时类型信息");
    println!("");
    println!("Toolchain Options:");
    println!("  --use-clang           使用 clang 工具链（默认使用 llc+lld）");
    println!("  --use-embedded-llc    实验性: 使用内嵌 llc (llvm-sys) 提高编译速度");
    println!("");
    println!("Other Options:");
    println!("  --version, -v         显示版本号");
    println!("  --help, -h            显示帮助信息");
    println!("");
    println!("Examples:");
    println!("  ir2exe input.ll {}", output_ext);
    println!("  ir2exe -O3 --lto input.ll {}", output_ext);
    println!("  ir2exe -O3 --march=native --mtune=native input.ll {}", output_ext);
    println!("  ir2exe -O3 --mavx2 --fvectorize input.ll {}", output_ext);
    println!("  ir2exe --pgo-gen -O2 input.ll {}      # 编译分析版本", output_ext);
    println!("  # 运行程序生成 .profraw 文件后...");
    println!("  llvm-profdata merge *.profraw -o app.profdata");
    println!("  ir2exe --pgo-use app.profdata -O3 input.ll {}  # 编译优化版本", output_ext);
}

fn parse_args(args: &[String]) -> Result<(Ir2ExeOptions, String, String), String> {
    let mut options = Ir2ExeOptions::default();
    options.target = get_default_target();
    
    let mut input_file: Option<String> = None;
    let mut output_file: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        let arg = &args[i];

        match arg.as_str() {
            "--version" | "-v" => {
                println!("ir2exe v{}", VERSION);
                process::exit(0);
            }
            "--help" | "-h" => {
                print_usage();
                process::exit(0);
            }
            "-O0" | "-O1" | "-O2" | "-O3" | "-Os" | "-Oz" => {
                options.optimization = arg.clone();
            }
            "-g" => {
                options.debug = true;
            }
            "--static" => {
                options.static_link = true;
            }
            "-fPIC" | "-fpic" => {
                options.position_independent = true;
            }
            "--fno-exceptions" | "-fno-exceptions" => {
                options.fno_exceptions = true;
            }
            "--fno-rtti" | "-fno-rtti" => {
                options.fno_rtti = true;
            }
            "--fomit-frame-pointer" | "-fomit-frame-pointer" => {
                options.fomit_frame_pointer = true;
            }
            "--funroll-loops" | "-funroll-loops" => {
                options.funroll_loops = true;
            }
            "--fvectorize" | "-fvectorize" => {
                options.fvectorize = true;
            }
            "--fslp-vectorize" | "-fslp-vectorize" => {
                options.fslp_vectorize = true;
            }
            "--mneon" => {
                options.mneon = true;
            }
            "--use-clang" => {
                options.use_llc_lld = false;
            }
            "--use-embedded-llc" => {
                options.use_embedded_llc = true;
            }
            "--pgo-gen" | "-fprofile-generate" => {
                options.pgo_gen = true;
            }
            "--pgo-cs" | "-fcs-profile-generate" => {
                options.pgo_cs = true;
            }
            "--lto" => {
                options.lto = true;
            }
            "--target" => {
                i += 1;
                if i >= args.len() {
                    return Err("--target 需要参数".to_string());
                }
                options.target = args[i].clone();
            }
            "--march" => {
                i += 1;
                if i >= args.len() {
                    return Err("--march 需要参数".to_string());
                }
                options.march = Some(args[i].clone());
            }
            "--mtune" => {
                i += 1;
                if i >= args.len() {
                    return Err("--mtune 需要参数".to_string());
                }
                options.mtune = Some(args[i].clone());
            }
            "--mcpu" => {
                i += 1;
                if i >= args.len() {
                    return Err("--mcpu 需要参数".to_string());
                }
                options.mcpu = Some(args[i].clone());
            }
            "--msse" => {
                i += 1;
                if i >= args.len() {
                    return Err("--msse 需要参数".to_string());
                }
                options.msse = Some(args[i].clone());
            }
            "--mavx" => {
                i += 1;
                if i >= args.len() {
                    return Err("--mavx 需要参数".to_string());
                }
                options.mavx = Some(args[i].clone());
            }
            "--pgo-use" => {
                i += 1;
                if i >= args.len() {
                    return Err("--pgo-use 需要参数".to_string());
                }
                options.pgo_use = Some(args[i].clone());
            }
            "-o" => {
                i += 1;
                if i >= args.len() {
                    return Err("-o 需要输出文件参数".to_string());
                }
                output_file = Some(args[i].clone());
            }
            "--ldflags" => {
                i += 1;
                if i >= args.len() {
                    return Err("--ldflags 需要参数".to_string());
                }
                for flag in args[i].split_whitespace() {
                    options.extra_ldflags.push(flag.to_string());
                }
            }
            "--cflags" => {
                i += 1;
                if i >= args.len() {
                    return Err("--cflags 需要参数".to_string());
                }
                for flag in args[i].split_whitespace() {
                    options.extra_cflags.push(flag.to_string());
                }
            }
            _ if arg.starts_with("--lto=") => {
                let lto_type = &arg[6..];
                match lto_type {
                    "full" => {
                        options.lto = true;
                        options.lto_thin = false;
                    }
                    "thin" => {
                        options.lto = true;
                        options.lto_thin = true;
                    }
                    _ => return Err(format!("未知的 LTO 类型: {}", lto_type)),
                }
            }
            _ if arg.starts_with("--march=") => {
                options.march = Some(arg[8..].to_string());
            }
            _ if arg.starts_with("--mtune=") => {
                options.mtune = Some(arg[8..].to_string());
            }
            _ if arg.starts_with("--mcpu=") => {
                options.mcpu = Some(arg[7..].to_string());
            }
            _ if arg.starts_with("--msse=") => {
                options.msse = Some(arg[7..].to_string());
            }
            _ if arg.starts_with("--mavx=") => {
                options.mavx = Some(arg[7..].to_string());
            }
            _ if arg.starts_with("-L") => {
                let path = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    i += 1;
                    if i >= args.len() {
                        return Err("-L 需要路径参数".to_string());
                    }
                    args[i].clone()
                };
                options.extra_lib_paths.push(path);
            }
            _ if arg.starts_with("-l") => {
                let lib = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    i += 1;
                    if i >= args.len() {
                        return Err("-l 需要库名参数".to_string());
                    }
                    args[i].clone()
                };
                options.extra_libs.push(lib);
            }
            _ if arg.starts_with("-march=") => {
                options.march = Some(arg[7..].to_string());
            }
            _ if arg.starts_with("-mtune=") => {
                options.mtune = Some(arg[7..].to_string());
            }
            _ if arg.starts_with("-mcpu=") => {
                options.mcpu = Some(arg[6..].to_string());
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
        let stem = Path::new(&input_file)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("output");
        
        // 根据目标平台选择扩展名
        if options.target.contains("windows") || options.target.contains("mingw") {
            format!("{}.exe", stem)
        } else if options.target.contains("darwin") {
            stem.to_string()
        } else {
            stem.to_string()
        }
    });

    Ok((options, input_file, output_file))
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let (options, input_file, output_file) = match parse_args(&args) {
        Ok(result) => result,
        Err(e) => {
            print_miette_error(
                "cavvy::argument_error",
                &e,
                Some("请检查命令行参数是否正确")
            );
            print_usage();
            process::exit(1);
        }
    };

    // 将输入文件转换为规范化绝对路径
    let input_path = Path::new(&input_file);
    let input_file_abs = if input_path.is_absolute() {
        input_path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|e| format!("无法获取当前目录: {}", e))
            .unwrap_or_else(|e| {
                print_miette_error(
                    "cavvy::io_error",
                    &e,
                    Some("请检查当前目录权限")
                );
                process::exit(1);
            })
            .join(input_path)
    };
    let input_file_abs = normalize_path(&input_file_abs);
    let input_file = input_file_abs.to_string_lossy().to_string();

    // 将输出文件转换为规范化绝对路径
    let output_path = Path::new(&output_file);
    let output_file_abs = if output_path.is_absolute() {
        output_path.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|e| format!("无法获取当前目录: {}", e))
            .unwrap_or_else(|e| {
                print_miette_error(
                    "cavvy::io_error",
                    &e,
                    Some("请检查当前目录权限")
                );
                process::exit(1);
            })
            .join(output_path)
    };
    let output_file_abs = normalize_path(&output_file_abs);
    let output_file = output_file_abs.to_string_lossy().to_string();

    // 确保输出目录存在
    if let Some(parent) = output_file_abs.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("无法创建输出目录: {}", e))
                .unwrap_or_else(|e| {
                    print_miette_error(
                        "cavvy::io_error",
                        &e,
                        Some("请检查输出目录权限")
                    );
                    process::exit(1);
                });
        }
    }

    // 根据目标平台显示编译模式
    let mode = if options.target.contains("windows") || options.target.contains("mingw") {
        "MinGW-w64 模式"
    } else if options.target.contains("linux") {
        "Linux 模式"
    } else if options.target.contains("darwin") {
        "macOS 模式"
    } else {
        "通用模式"
    };
    
    println!("IR 编译器 v{} ({})", VERSION, mode);
    println!("IR 文件: {}", input_file);
    println!("输出: {}", output_file);
    println!("目标平台: {}", options.target);
    println!("优化级别: {}", options.optimization);

    // 显示 CPU 优化信息
    if let Some(ref march) = options.march {
        println!("目标架构: {}", march);
    }
    if let Some(ref mtune) = options.mtune {
        println!("优化目标 CPU: {}", mtune);
    }
    if let Some(ref mcpu) = options.mcpu {
        println!("目标 CPU: {}", mcpu);
    }
    if let Some(ref msse) = options.msse {
        println!("SSE 版本: {}", msse);
    }
    if let Some(ref mavx) = options.mavx {
        println!("AVX 版本: {}", mavx);
    }
    if options.mneon {
        println!("NEON: 启用");
    }

    // 显示 LTO 信息
    if options.lto {
        if options.lto_thin {
            println!("LTO: Thin LTO");
        } else {
            println!("LTO: Full LTO");
        }
    }

    // 显示 PGO 信息
    if options.pgo_gen {
        if options.pgo_cs {
            println!("PGO: 上下文敏感分析生成");
        } else {
            println!("PGO: 分析生成模式");
        }
    }
    if let Some(ref pgo_data) = options.pgo_use {
        println!("PGO: 使用分析数据 {}", pgo_data);
    }

    // 显示其他优化
    if options.fvectorize {
        println!("自动向量化: 启用");
    }
    if options.fslp_vectorize {
        println!("SLP 向量化: 启用");
    }
    if options.funroll_loops {
        println!("循环展开: 启用");
    }
    if options.fomit_frame_pointer {
        println!("省略帧指针: 是");
    }

    if options.debug {
        println!("调试信息: 启用");
    }
    if options.static_link {
        println!("链接模式: 静态链接");
    }
    if options.position_independent {
        println!("位置无关代码: 启用");
    }
    if !options.extra_lib_paths.is_empty() {
        println!("额外库路径: {:?}", options.extra_lib_paths);
    }
    if !options.extra_libs.is_empty() {
        println!("额外库: {:?}", options.extra_libs);
    }
    println!("");

    // 读取IR文件内容以解析源映射
    let ir_content = match std::fs::read_to_string(&input_file) {
        Ok(content) => content,
        Err(e) => {
            print_miette_error(
                "cavvy::io_error",
                &format!("无法读取IR文件: {}", e),
                Some("请检查IR文件路径是否正确")
            );
            process::exit(1);
        }
    };

    // 解析源映射
    let source_map = parse_source_map_from_ir(&ir_content);
    if !source_map.mappings.is_empty() {
        println!("  [I] 已加载源映射: {} 个映射点", source_map.mappings.len());
    }

    // 调用 ir2exe_lib 进行编译
    match compile_ir_to_exe(&input_file, &output_file, &options, Some(&source_map)) {
        Ok(result) => {
            for msg in &result.messages {
                println!("  {}", msg);
            }
            
            // PGO 提示
            if options.pgo_gen {
                println!("");
                println!("[I] PGO: 运行程序生成 .profraw 文件后，执行:");
                println!("    llvm-profdata merge *.profraw -o app.profdata");
                println!("    ir2exe --pgo-use app.profdata [其他选项] input.ll {}", 
                    if cfg!(target_os = "windows") { "output.exe" } else { "output" });
            }
            
            println!("");
            println!("[I] 提示: 使用 './{}' 可直接运行并测速", output_file);
            println!("");
            
            // 根据目标平台显示完成消息
            let mode_str = if options.target.contains("windows") || options.target.contains("mingw") {
                "MinGW-w64 模式"
            } else if options.target.contains("linux") {
                "Linux ELF 模式"
            } else if options.target.contains("darwin") {
                "macOS 模式"
            } else {
                "通用模式"
            };
            println!("编译完成 ({})", mode_str);
        }
        Err(e) => {
            print_tool_error("ir2exe", "编译失败", Some(&e));
            process::exit(1);
        }
    }
}
