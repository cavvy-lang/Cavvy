use std::env;
use std::process;
use std::path::{Path, PathBuf};

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

const VERSION: &str = env!("IR2EXE_VERSION");

struct CompileOptions {
    optimization: String,         // -O0, -O1, -O2, -O3, -Os, -Oz
    debug: bool,                  // -g
    extra_lib_paths: Vec<String>, // -L<path>
    extra_libs: Vec<String>,      // -l<lib>
    extra_ldflags: Vec<String>,   // --ldflags
    extra_cflags: Vec<String>,    // --cflags
    target: String,               // --target
    static_link: bool,            // --static
    position_independent: bool,   // -fPIC/-fPIE
    // LTO 选项
    lto: bool,                    // --lto, --lto=full
    lto_thin: bool,               // --lto=thin
    // CPU 指令集
    march: Option<String>,        // -march=<cpu>
    mtune: Option<String>,        // -mtune=<cpu>
    mcpu: Option<String>,         // -mcpu=<cpu> (ARM/AArch64)
    msse: Option<String>,         // -msse, -msse2, -msse3, etc.
    mavx: Option<String>,         // -mavx, -mavx2, -mavx512f, etc.
    mneon: bool,                  // -mfpu=neon (ARM)
    // PGO 选项
    pgo_gen: bool,                // -fprofile-generate
    pgo_use: Option<String>,      // -fprofile-use=<path>
    pgo_cs: bool,                 // -fcs-profile-generate (context sensitive)
    // 其他优化
    fno_exceptions: bool,         // -fno-exceptions
    fno_rtti: bool,               // -fno-rtti
    fomit_frame_pointer: bool,    // -fomit-frame-pointer
    funroll_loops: bool,          // -funroll-loops
    fvectorize: bool,             // -fvectorize
    fslp_vectorize: bool,         // -fslp-vectorize
}

/// 根据当前操作系统自动选择默认目标平台
fn get_default_target() -> String {
    if cfg!(target_os = "windows") {
        "x86_64-w64-mingw32".to_string()
    } else if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-gnu".to_string()
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin".to_string()
    } else {
        // 默认使用当前系统的目标
        std::env::var("TARGET").unwrap_or_else(|_| {
            // 如果无法获取环境变量，回退到通用目标
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

impl Default for CompileOptions {
    fn default() -> Self {
        CompileOptions {
            optimization: "-O2".to_string(),
            debug: false,
            extra_lib_paths: Vec::new(),
            extra_libs: Vec::new(),
            extra_ldflags: Vec::new(),
            extra_cflags: Vec::new(),
            target: get_default_target(),
            static_link: false,
            position_independent: false,
            lto: false,
            lto_thin: false,
            march: None,
            mtune: None,
            mcpu: None,
            msse: None,
            mavx: None,
            mneon: false,
            pgo_gen: false,
            pgo_use: None,
            pgo_cs: false,
            fno_exceptions: false,
            fno_rtti: false,
            fomit_frame_pointer: false,
            funroll_loops: false,
            fvectorize: false,
            fslp_vectorize: false,
        }
    }
}

fn print_usage() {
    println!("ir2exe v{}", VERSION);
    println!("Usage: ir2exe [options] <input_file.ll> [output_file.exe]");
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
    println!("  --target <target>     指定目标平台 (默认: x86_64-w64-mingw32)");
    println!("  --fno-exceptions      禁用异常处理");
    println!("  --fno-rtti            禁用运行时类型信息");
    println!("");
    println!("Other Options:");
    println!("  --version, -v         显示版本号");
    println!("  --help, -h            显示帮助信息");
    println!("");
    println!("Examples:");
    println!("  ir2exe input.ll output.exe");
    println!("  ir2exe -O3 --lto input.ll output.exe");
    println!("  ir2exe -O3 --march=native --mtune=native input.ll output.exe");
    println!("  ir2exe -O3 --mavx2 --fvectorize input.ll output.exe");
    println!("  ir2exe --pgo-gen -O2 input.ll output.exe      # 编译分析版本");
    println!("  # 运行程序生成 .profraw 文件后...");
    println!("  llvm-profdata merge *.profraw -o app.profdata");
    println!("  ir2exe --pgo-use app.profdata -O3 input.ll output.exe  # 编译优化版本");
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
            // macOS 通常没有扩展名，或者使用 .app
            stem.to_string()
        } else {
            // Linux 和其他 Unix-like 系统
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
            eprintln!("错误: {}", e);
            print_usage();
            process::exit(1);
        }
    };

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

    let clang_exe = match find_clang() {
        Ok(path) => path,
        Err(e) => {
            eprintln!("错误: {}", e);
            process::exit(1);
        }
    };

    println!("[I] 正在编译 IR → EXE...");

    // 设置库路径 - 先获取可执行文件所在目录
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    // 根据目标平台选择库路径
    let lib_paths: Vec<PathBuf> = if options.target.contains("windows") || options.target.contains("mingw") {
        // Windows/MinGW 库路径
        vec![
            exe_dir.join("lib/mingw64/x86_64-w64-mingw32/lib"),
            exe_dir.join("lib/mingw64/lib"),
            exe_dir.join("lib/mingw64/lib/gcc/x86_64-w64-mingw32/15.2.0")
        ]
    } else {
        // Linux/Unix 系统使用系统默认库路径
        // 不添加额外的库路径，让链接器使用系统默认路径
        vec![]
    };

    // 构建 clang 命令
    let mut cmd = process::Command::new(&clang_exe);
    cmd.arg(&input_file)
        .arg("-o").arg(&output_file)
        .arg("-target").arg(&options.target)
        .arg(&options.optimization)
        .arg("-Wno-override-module");

    // LTO 设置
    if options.lto {
        if options.lto_thin {
            cmd.arg("-flto=thin");
        } else {
            cmd.arg("-flto=full");
        }
    }

    // CPU 指令集
    if let Some(ref march) = options.march {
        cmd.arg(format!("-march={}", march));
    }
    if let Some(ref mtune) = options.mtune {
        cmd.arg(format!("-mtune={}", mtune));
    }
    if let Some(ref mcpu) = options.mcpu {
        cmd.arg(format!("-mcpu={}", mcpu));
    }
    if let Some(ref msse) = options.msse {
        cmd.arg(format!("-msse{}", msse));
    }
    if let Some(ref mavx) = options.mavx {
        match mavx.as_str() {
            "avx" => cmd.arg("-mavx"),
            "avx2" => cmd.arg("-mavx2"),
            "avx512f" => cmd.arg("-mavx512f"),
            "avx512" => cmd.arg("-mavx512f"),
            _ => cmd.arg(format!("-m{}", mavx)),
        };
    }
    if options.mneon {
        cmd.arg("-mfpu=neon");
    }

    // PGO
    if options.pgo_gen {
        if options.pgo_cs {
            cmd.arg("-fcs-profile-generate");
        } else {
            cmd.arg("-fprofile-generate");
        }
    }
    if let Some(ref pgo_data) = options.pgo_use {
        cmd.arg(format!("-fprofile-use={}", pgo_data));
    }

    // 调试信息
    if options.debug {
        cmd.arg("-g");
    }

    // 位置无关代码
    if options.position_independent {
        cmd.arg("-fPIC");
    }

    // 静态链接
    if options.static_link {
        cmd.arg("-static");
    }

    // 代码生成选项
    if options.fno_exceptions {
        cmd.arg("-fno-exceptions");
    }
    if options.fno_rtti {
        cmd.arg("-fno-rtti");
    }
    if options.fomit_frame_pointer {
        cmd.arg("-fomit-frame-pointer");
    }
    if options.funroll_loops {
        cmd.arg("-funroll-loops");
    }
    if options.fvectorize {
        cmd.arg("-fvectorize");
    }
    if options.fslp_vectorize {
        cmd.arg("-fslp-vectorize");
    }

    // 添加库路径
    for lib_path in &lib_paths {
        if lib_path.exists() {
            cmd.arg("-L").arg(lib_path);
        }
    }

    // 额外库路径
    for path in &options.extra_lib_paths {
        cmd.arg("-L").arg(path);
    }

    // 额外 cflags
    for flag in &options.extra_cflags {
        cmd.arg(flag);
    }

    // 使用 lld 链接器
    cmd.arg("-fuse-ld=lld");

    // 根据目标平台选择默认库
    if options.target.contains("windows") || options.target.contains("mingw") {
        // Windows 平台库
        cmd.arg("-lkernel32")
            .arg("-lmsvcrt")
            .arg("-ladvapi32");
    } else if options.target.contains("linux") {
        // Linux 平台库
        cmd.arg("-lc")
            .arg("-lm")
            .arg("-lpthread");
    } else if options.target.contains("darwin") {
        // macOS 平台库
        cmd.arg("-lc")
            .arg("-lm");
    } else {
        // 通用库
        cmd.arg("-lc")
            .arg("-lm");
    }

    // 额外库
    for lib in &options.extra_libs {
        cmd.arg(format!("-l{}", lib));
    }

    // 额外的链接器标志
    for flag in &options.extra_ldflags {
        cmd.arg(flag);
    }

    let output = cmd.output()
        .unwrap_or_else(|e| {
            eprintln!("执行clang失败: {}", e);
            process::exit(1);
        });

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        eprintln!("编译失败 (clang exit code: {})", output.status.code().unwrap_or(-1));
        eprintln!("错误: {}", error_msg);
        process::exit(1);
    }

    if !output.stderr.is_empty() {
        let warn_msg = String::from_utf8_lossy(&output.stderr);
        println!("  [W] {}", warn_msg);
    }

    let exe_size = std::fs::metadata(&output_file)
        .map(|m| m.len() as f64 / 1024.0)
        .unwrap_or(0.0);
    println!("  [+] 生成: {} ({:.1} KB)", output_file, exe_size);

    // PGO 提示
    if options.pgo_gen {
        println!("");
        println!("[I] PGO: 运行程序生成 .profraw 文件后，执行:");
        println!("    llvm-profdata merge *.profraw -o app.profdata");
        println!("    ir2exe --pgo-use app.profdata [其他选项] input.ll output.exe");
    }

    println!("");
    println!("[I] 提示: 使用 '{}' 可直接运行并测速", output_file);
    println!("");
    println!("编译完成 (MinGW-w64 模式)");
}