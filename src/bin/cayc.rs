use std::env;
use std::fs;
use std::process;
use std::path::{Path, PathBuf};
use cavvy::Compiler;
use cavvy::error::{print_error_with_context, print_miette_error, print_tool_error, print_warning};

/// 根据平台获取 llvm-minimal 下的 clang 路径
#[cfg(target_os = "windows")]
fn get_bundled_clang_path(exe_dir: &Path) -> PathBuf {
    exe_dir.join("llvm-minimal/bin/clang.exe")
}

#[cfg(target_os = "linux")]
fn get_bundled_clang_path(exe_dir: &Path) -> PathBuf {
    exe_dir.join("llvm-minimal/bin-linux/clang-21")
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn get_bundled_clang_path(exe_dir: &Path) -> PathBuf {
    exe_dir.join("llvm-minimal/bin/clang")
}

/// 查找 clang 可执行文件
/// 1. 首先尝试直接调用 "clang"（系统 PATH 中）
/// 2. 如果失败，尝试查找编译器所在目录下的 llvm-minimal/bin/clang
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
            let bundled_clang = get_bundled_clang_path(exe_dir);
            if bundled_clang.exists() {
                return Ok(bundled_clang);
            }
        }
    }
    
    // 3. 都找不到，返回错误
    Err("找不到 clang 编译器。请确保 clang 已安装并在 PATH 中，或将 llvm-minimal 放在编译器同目录下。".to_string())
}

const VERSION: &str = env!("CAYC_VERSION");

struct CompileOptions {
    // 基础优化
    optimization: String,         // -O0, -O1, -O2, -O3, -Os, -Oz
    opt_ir: bool,                 // --opt-ir: 优化 IR 阶段
    debug: bool,                  // -g
    keep_ir: bool,                // --keep-ir
    extra_lib_paths: Vec<String>, // -L<path>
    extra_libs: Vec<String>,      // -l<lib>
    extra_ldflags: Vec<String>,   // --ldflags
    extra_cflags: Vec<String>,    // --cflags
    include_paths: Vec<String>,   // -I<path>
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
    mneon: bool,                  // --mneon (ARM)
    // PGO 选项
    pgo_gen: bool,                // -fprofile-generate
    pgo_use: Option<String>,      // -fprofile-use=<path>
    pgo_cs: bool,                 // -fcs-profile-generate
    // 其他优化
    fno_exceptions: bool,         // -fno-exceptions
    fno_rtti: bool,               // -fno-rtti
    fomit_frame_pointer: bool,    // -fomit-frame-pointer
    funroll_loops: bool,          // -funroll-loops
    fvectorize: bool,             // -fvectorize
    fslp_vectorize: bool,         // -fslp-vectorize
    // 工具链选项
    use_llc_lld: bool,            // --use-llc-lld
    // 语言特性
    features: Vec<String>,        // -F/--feature=<feature>
    // 测试模式
    test_mode: bool,              // --test
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
            opt_ir: false,
            debug: false,
            keep_ir: false,
            extra_lib_paths: Vec::new(),
            extra_libs: Vec::new(),
            extra_ldflags: Vec::new(),
            extra_cflags: Vec::new(),
            include_paths: Vec::new(),
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
            use_llc_lld: false,
            features: Vec::new(),
            test_mode: false,
        }
    }
}

fn print_usage() {
    println!("Cavvy Compiler v{}", VERSION);
    println!("Usage: cayc [options] <source_file.cay> [output_file.exe]");
    println!("");
    println!("Optimization Options:");
    println!("  -O0, -O1, -O2, -O3    优化级别 (默认: -O2)");
    println!("  -Os, -Oz              优化代码大小");
    println!("  --opt-ir              启用 IR 阶段优化 (使用 LLVM 优化 IR)");
    println!("  --lto[=<type>]        链接时优化 (full/thin)");
    println!("  -march=<arch>         目标 CPU 架构 (如 x86-64-v3, native)");
    println!("  -mtune=<cpu>          针对特定 CPU 优化 (如 intel, znver3)");
    println!("  -mcpu=<cpu>           针对 ARM/AArch64 CPU 优化");
    println!("  -msse=<ver>           SSE 版本 (1/2/3/4.1/4.2)");
    println!("  -mavx=<ver>           AVX 版本 (avx/avx2/avx512f)");
    println!("  --mneon               启用 ARM NEON");
    println!("  -funroll-loops        循环展开");
    println!("  -fvectorize           启用自动向量化");
    println!("  -fslp-vectorize       启用 SLP 向量化");
    println!("  -fomit-frame-pointer  省略帧指针");
    println!("");
    println!("PGO (Profile Guided Optimization):");
    println!("  -fprofile-generate     生成性能分析数据");
    println!("  -fprofile-use=<path>   使用性能分析数据优化");
    println!("  -fcs-profile-generate  上下文敏感的性能分析");
    println!("");
    println!("Code Generation:");
    println!("  -g                    生成调试信息");
    println!("  --keep-ir             保留中间 IR 文件 (.ll)");
    println!("  -I<path>              添加包含搜索路径（供 #include 使用）");
    println!("  -L<path>              添加库搜索路径");
    println!("  -l<lib>               链接额外的库");
    println!("  --ldflags <flags>     传递额外的链接器标志");
    println!("  --cflags <flags>      传递额外的编译器标志");
    println!("  --static              静态链接");
    println!("  -fPIC                 生成位置无关代码");
    println!("  --use-llc-lld         使用 llc+lld 工具链（不使用 clang）");
    println!("  -fno-exceptions       禁用异常处理");
    println!("  -fno-rtti             禁用运行时类型信息");
    println!("");
    println!("Language Features:");
    println!("  -F<feature>, --feature=<feature>  启用语言特性");
    println!("                                     top_level_function - 允许顶层函数");
    println!("");
    println!("Other Options:");
    println!("  --version, -v         显示版本号");
    println!("  --help, -h            显示帮助信息");
    println!("");
    println!("Examples:");
    println!("  cayc hello.cay");
    println!("  cayc -O3 hello.cay hello.exe");
    println!("  cayc --opt-ir -O3 --lto=full hello.cay");
    println!("  cayc -O3 -march=native -mtune=native -fvectorize hello.cay");
    println!("  cayc --static -O2 -L./libs -lmylib app.cay app.exe");
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
                println!("Cavvy Compiler v{}", VERSION);
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
                options.opt_ir = true;
            }
            "-g" => {
                options.debug = true;
            }
            "--keep-ir" => {
                options.keep_ir = true;
            }
            "--static" => {
                options.static_link = true;
            }
            "-fPIC" | "-fpic" => {
                options.position_independent = true;
            }
            "-fno-exceptions" => {
                options.fno_exceptions = true;
            }
            "-fno-rtti" => {
                options.fno_rtti = true;
            }
            "-fomit-frame-pointer" => {
                options.fomit_frame_pointer = true;
            }
            "-funroll-loops" => {
                options.funroll_loops = true;
            }
            "-fvectorize" => {
                options.fvectorize = true;
            }
            "-fslp-vectorize" => {
                options.fslp_vectorize = true;
            }
            "--use-llc-lld" => {
                options.use_llc_lld = true;
            }
            "--test" => {
                options.test_mode = true;
            }
            "--mneon" => {
                options.mneon = true;
            }
            "-fprofile-generate" => {
                options.pgo_gen = true;
            }
            "-fcs-profile-generate" => {
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
            _ if arg.starts_with("-march=") => {
                options.march = Some(arg[7..].to_string());
            }
            _ if arg.starts_with("-mtune=") => {
                options.mtune = Some(arg[7..].to_string());
            }
            _ if arg.starts_with("-mcpu=") => {
                options.mcpu = Some(arg[6..].to_string());
            }
            _ if arg.starts_with("-msse=") => {
                options.msse = Some(arg[6..].to_string());
            }
            _ if arg.starts_with("-mavx=") => {
                options.mavx = Some(arg[6..].to_string());
            }
            _ if arg.starts_with("-fprofile-use=") => {
                options.pgo_use = Some(arg[14..].to_string());
            }
            _ if arg.starts_with("-I") => {
                let path = if arg.len() > 2 {
                    arg[2..].to_string()
                } else {
                    i += 1;
                    if i >= args.len() {
                        return Err("-I 需要路径参数".to_string());
                    }
                    args[i].clone()
                };
                options.include_paths.push(path);
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
            _ if arg.starts_with("-F") => {
                // -F<feature> 或 -F=<feature> 格式
                let feature = if arg.len() > 2 {
                    if arg.starts_with("-F=") {
                        arg[3..].to_string()
                    } else {
                        arg[2..].to_string()
                    }
                } else {
                    i += 1;
                    if i >= args.len() {
                        return Err("-F 需要特性名称参数".to_string());
                    }
                    args[i].clone()
                };
                options.features.push(feature);
            }
            _ if arg.starts_with("--feature=") => {
                // --feature=<feature> 格式
                options.features.push(arg[10..].to_string());
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
        } else {
            // Linux和其他系统不使用.exe扩展名
            stem.to_string()
        }
    });

    Ok((options, input_file, output_file))
}

fn optimize_ir(ir_file: &str, opt_level: &str) -> Result<(), String> {
    let clang_exe = find_clang()?;

    let temp_file = format!("{}.opt.tmp", ir_file);

    let output = process::Command::new(&clang_exe)
        .arg("-x").arg("ir")
        .arg(ir_file)
        .arg("-S")
        .arg("-emit-llvm")
        .arg(opt_level)
        .arg("-o").arg(&temp_file)
        .output()
        .map_err(|e| format!("执行 clang 失败: {}", e))?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        let _ = fs::remove_file(&temp_file);
        return Err(format!("IR 优化失败: {}", error_msg));
    }

    fs::rename(&temp_file, ir_file)
        .map_err(|e| format!("无法替换 IR 文件: {}", e))?;

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let (options, source_path, exe_output) = match parse_args(&args) {
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

    let ir_file = Path::new(&exe_output)
        .with_extension("ll")
        .to_string_lossy()
        .to_string();

    println!("Cavvy 编译器 v{}", VERSION);
    println!("源文件: {}", source_path);
    println!("输出: {}", exe_output);
    println!("优化级别: {}", options.optimization);

    if options.opt_ir {
        println!("IR 优化: 启用");
    }
    if options.lto {
        if options.lto_thin {
            println!("LTO: Thin LTO");
        } else {
            println!("LTO: Full LTO");
        }
    }
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
    if options.fvectorize {
        println!("自动向量化: 启用");
    }
    if options.fslp_vectorize {
        println!("SLP 向量化: 启用");
    }
    if options.funroll_loops {
        println!("循环展开: 启用");
    }
    if options.use_llc_lld {
        println!("工具链: llc+lld");
    }
    if options.debug {
        println!("调试信息: 启用");
    }
    if options.keep_ir {
        println!("保留 IR: 是");
    }
    if options.static_link {
        println!("链接模式: 静态链接");
    }
    println!("");

    // 1. Cavvy → IR
    println!("[1] Cavvy → IR 编译...");
    let source = match fs::read_to_string(&source_path) {
        Ok(content) => content,
        Err(e) => {
            print_miette_error(
                "cavvy::io_error",
                &format!("无法读取源文件 '{}': {}", source_path, e),
                Some("请检查文件路径是否正确，文件是否存在")
            );
            process::exit(1);
        }
    };

    // 创建编译器选项
    let compiler_options = cavvy::CompilerOptions {
        target_os: std::env::consts::OS.to_string(),
        features: options.features.clone(),
        no_features: Vec::new(),
        defines: Vec::new(),
        undefines: Vec::new(),
        obfuscate: false,
        debug: options.debug,
        include_paths: options.include_paths.clone(),
        test_mode: options.test_mode,
    };
    let compiler = cavvy::Compiler::with_options(compiler_options);
    match compiler.compile_file(&source_path, &ir_file) {
        Ok(_) => {
            println!("  [+] Cavvy 编译成功");
        }
        Err(e) => {
            print_error_with_context(&e, &source, &source_path);
            process::exit(1);
        }
    }

    // 2. IR 优化 (如果启用)
    if options.opt_ir {
        println!("");
        println!("[2] IR 优化 ({})...", options.optimization);
        match optimize_ir(&ir_file, &options.optimization) {
            Ok(_) => {
                println!("  [+] IR 优化完成");
            }
            Err(e) => {
                print_warning(&format!("IR 优化失败: {}", e));
                println!("  [I] 继续编译未优化的 IR");
            }
        }
    }

    // 3. IR → EXE (调用ir2exe)
    println!("");
    let step_num = if options.opt_ir { "[3]" } else { "[2]" };
    println!("{} IR → EXE 编译...", step_num);

    let current_exe = match env::current_exe() {
        Ok(path) => path,
        Err(_) => {
            print_miette_error(
                "cavvy::internal_error",
                "无法获取当前执行路径",
                Some("请尝试重新运行编译器")
            );
            process::exit(1);
        }
    };

    let bin_dir = current_exe.parent().unwrap_or_else(|| {
        print_miette_error(
            "cavvy::internal_error",
            "无法获取执行目录",
            Some("请检查编译器安装")
        );
        process::exit(1);
    });

    // 尝试搜索 ir2exe 和 ir2exe.exe 两个文件名
    let ir2exe_paths = [
        bin_dir.join("ir2exe"),
        bin_dir.join("ir2exe.exe")
    ];
    
    let ir2exe_path = match ir2exe_paths.iter().find(|path| path.exists()) {
        Some(path) => path,
        None => {
            let paths_str = ir2exe_paths.iter()
                .map(|p| format!("  {:?}", p))
                .collect::<Vec<_>>()
                .join("\n");
            print_miette_error(
                "cavvy::tool_not_found",
                &format!("找不到 ir2exe 或 ir2exe.exe\n搜索位置:\n{}", paths_str),
                Some("请确保 ir2exe 与 cayc 在同一目录下")
            );
            let _ = fs::remove_file(&ir_file);
            process::exit(1);
        }
    };


    // 构建 ir2exe 参数
    let mut ir2exe_args: Vec<String> = vec![];

    // 目标平台
    ir2exe_args.push("--target".to_string());
    ir2exe_args.push(options.target.clone());

    // 基础优化
    ir2exe_args.push(options.optimization.clone());

    // LTO
    if options.lto {
        if options.lto_thin {
            ir2exe_args.push("--lto=thin".to_string());
        } else {
            ir2exe_args.push("--lto=full".to_string());
        }
    }

    // CPU 指令集
    if let Some(ref march) = options.march {
        ir2exe_args.push(format!("-march={}", march));
    }
    if let Some(ref mtune) = options.mtune {
        ir2exe_args.push(format!("-mtune={}", mtune));
    }
    if let Some(ref mcpu) = options.mcpu {
        ir2exe_args.push(format!("-mcpu={}", mcpu));
    }
    if let Some(ref msse) = options.msse {
        ir2exe_args.push(format!("-msse={}", msse));
    }
    if let Some(ref mavx) = options.mavx {
        ir2exe_args.push(format!("-mavx={}", mavx));
    }
    if options.mneon {
        ir2exe_args.push("--mneon".to_string());
    }

    // PGO
    if options.pgo_gen {
        ir2exe_args.push("-fprofile-generate".to_string());
    }
    if options.pgo_cs {
        ir2exe_args.push("-fcs-profile-generate".to_string());
    }
    if let Some(ref pgo_data) = options.pgo_use {
        ir2exe_args.push(format!("-fprofile-use={}", pgo_data));
    }

    // 调试信息
    if options.debug {
        ir2exe_args.push("-g".to_string());
    }

    // 位置无关代码
    if options.position_independent {
        ir2exe_args.push("-fPIC".to_string());
    }

    // 静态链接
    if options.static_link {
        ir2exe_args.push("--static".to_string());
    }

    // 代码生成选项
    if options.fno_exceptions {
        ir2exe_args.push("-fno-exceptions".to_string());
    }
    if options.fno_rtti {
        ir2exe_args.push("-fno-rtti".to_string());
    }
    if options.fomit_frame_pointer {
        ir2exe_args.push("-fomit-frame-pointer".to_string());
    }
    if options.funroll_loops {
        ir2exe_args.push("-funroll-loops".to_string());
    }
    if options.fvectorize {
        ir2exe_args.push("-fvectorize".to_string());
    }
    if options.fslp_vectorize {
        ir2exe_args.push("-fslp-vectorize".to_string());
    }

    // 工具链选项
    if options.use_llc_lld {
        ir2exe_args.push("--use-llc-lld".to_string());
    }

    // 额外库路径
    for path in &options.extra_lib_paths {
        ir2exe_args.push(format!("-L{}", path));
    }

    
    #[cfg(target_os = "windows")]
    if let Ok(ir_content) = fs::read_to_string(&ir_file) {
        if ir_content.contains("WSAStartup") || ir_content.contains("socket(") || ir_content.contains("@socket(") {
            ir2exe_args.push("-lws2_32".to_string());
        }
    }

    // extra libs
    for lib in &options.extra_libs {
        ir2exe_args.push(format!("-l{}", lib));
    }

    // cflags
    if !options.extra_cflags.is_empty() {
        ir2exe_args.push("--cflags".to_string());
        ir2exe_args.push(options.extra_cflags.join(" "));
    }

    // 额外的链接器标志
    if !options.extra_ldflags.is_empty() {
        ir2exe_args.push("--ldflags".to_string());
        ir2exe_args.push(options.extra_ldflags.join(" "));
    }

    // 输入输出文件
    ir2exe_args.push(ir_file.clone());
    ir2exe_args.push(exe_output.clone());

    // 调试：显示实际调用的命令
    println!("  [D] 调用: {} {}", ir2exe_path.display(), ir2exe_args.join(" "));
    
    // 调试：显示实际调用的命令
    println!("  [D] 调用: {} {}", ir2exe_path.display(), ir2exe_args.join(" "));
    
    // 调用ir2exe
    let output = process::Command::new(&ir2exe_path)
        .args(&ir2exe_args)
        .output()
        .unwrap_or_else(|e| {
            print_tool_error("ir2exe", &format!("执行失败: {}", e), Some("请检查 ir2exe 是否正确安装"));
            if !options.keep_ir {
                let _ = fs::remove_file(&ir_file);
            }
            process::exit(1);
        });

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        print_tool_error("ir2exe", "IR→EXE编译失败", Some(&error_msg));
        if !options.keep_ir {
            let _ = fs::remove_file(&ir_file);
        }
        process::exit(1);
    }

    // 清理IR文件（如果不保留）
    if !options.keep_ir {
        if let Err(e) = fs::remove_file(&ir_file) {
            print_warning(&format!("无法清理临时文件 {}: {}", ir_file, e));
        }
    } else {
        println!("");
        println!("[I] 保留 IR 文件: {}", ir_file);
    }

    println!("");
    println!("[+] 编译完成!");
    println!("生成: {}", exe_output);
}
