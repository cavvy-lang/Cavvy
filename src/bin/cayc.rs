use cavvy::ir2exe_lib::{
    IRSourceMap, Ir2ExeOptions, compile_ir_to_object, link_objects_to_exe,
    parse_link_libraries_from_ir, parse_source_map_from_ir,
};
use cavvy::miette_diagnostic::{
    print_error_with_context, print_miette_error, print_tool_error,
};
use std::env;
use std::fs;
use std::path::Path;
use std::process;

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
    lto: bool,      // --lto, --lto=full
    lto_thin: bool, // --lto=thin
    // CPU 指令集
    march: Option<String>, // -march=<cpu>
    mtune: Option<String>, // -mtune=<cpu>
    mcpu: Option<String>,  // -mcpu=<cpu> (ARM/AArch64)
    msse: Option<String>,  // -msse, -msse2, -msse3, etc.
    mavx: Option<String>,  // -mavx, -mavx2, -mavx512f, etc.
    mneon: bool,           // --mneon (ARM)
    // PGO 选项
    pgo_gen: bool,           // -fprofile-generate
    pgo_use: Option<String>, // -fprofile-use=<path>
    pgo_cs: bool,            // -fcs-profile-generate
    // 其他优化
    fno_exceptions: bool,      // -fno-exceptions
    fno_rtti: bool,            // -fno-rtti
    fomit_frame_pointer: bool, // -fomit-frame-pointer
    funroll_loops: bool,       // -funroll-loops
    fvectorize: bool,          // -fvectorize
    fslp_vectorize: bool,      // -fslp-vectorize
    // 工具链选项
    use_clang: bool,        // --use-clang (强制使用clang)
    use_llc_lld: bool,      // --use-llc-lld (强制使用llc+lld)
    use_embedded_llc: bool, // --use-embedded-llc (实验性)
    // 语言特性
    features: Vec<String>, // -F/--feature=<feature>
    // 宏定义
    defines: Vec<String>,   // -D/--define=<macro>[=value]
    undefines: Vec<String>, // -U/--undefine=<macro>
    // 测试模式
    test_mode: bool, // --test
    // Rc 循环引用检测
    detect_cycles: bool, // --detect-cycles
    // 禁止 panic：panic()/abort() 转为编译错误
    no_panic: bool, // --no-panic
    // 仅编译到目标文件，不链接
    compile_only: bool, // -c
    // 显式输出文件名
    output: Option<String>, // -o / --output
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
            use_clang: false,
            use_llc_lld: false,
            use_embedded_llc: false,
            features: Vec::new(),
            defines: Vec::new(),
            undefines: Vec::new(),
            test_mode: false,
            detect_cycles: false,
            no_panic: false,
            compile_only: false,
            output: None,
        }
    }
}

fn print_usage() {
    println!("Cavvy Compiler v{}", VERSION);
    println!("Usage: cayc [options] <source1.cay> [source2.cay ...] [output_file.exe]");
    println!("");
    println!("Compilation Mode:");
    println!("  -c                    仅编译到目标文件 (.obj)，不链接");
    println!("  -o <file>, --output <file>  指定输出文件名（可替代最后一个位置参数）");
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
    println!("  --use-clang           强制使用 clang 工具链");
    println!("  --use-llc-lld         强制使用 llc+lld 工具链（默认）");
    println!("  --use-embedded-llc    实验性: 使用内嵌 llc (llvm-sys) 提高编译速度");
    println!("  --detect-cycles       启用 Rc<T> 循环引用运行时检测");
    println!("  --no-panic            将 panic()/abort() 调用转为编译错误（嵌入式等场景）");
    println!("  -fno-exceptions       禁用异常处理");
    println!("  -fno-rtti             禁用运行时类型信息");
    println!("");
    println!("Language Features:");
    println!("  -F<feature>, --feature=<feature>  启用语言特性");
    println!("                                     top_level_function - 允许顶层函数");
    println!("");
    println!("Preprocessor:");
    println!("  -D<macro>[=<value>], --define=<macro>[=<value>]  定义预处理器宏");
    println!("  -U<macro>, --undefine=<macro>                        取消预处理器宏定义");
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
    println!("  cayc helper.cay main.cay");
    println!("  cayc helper.cay main.cay myapp");
    println!("  cayc helper.cay main.cay -o myapp");
    println!("  cayc -c helper.cay");
    println!("  cayc -c helper.cay main.cay");
}

fn parse_args(args: &[String]) -> Result<(CompileOptions, Vec<String>, String), String> {
    let mut options = CompileOptions::default();
    let mut positional: Vec<String> = Vec::new();
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
            "-c" => {
                options.compile_only = true;
            }
            "-o" | "--output" => {
                i += 1;
                if i >= args.len() {
                    return Err("-o/--output 需要输出文件名参数".to_string());
                }
                options.output = Some(args[i].clone());
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
            "--use-clang" => {
                options.use_clang = true;
            }
            "--use-llc-lld" => {
                options.use_llc_lld = true;
            }
            "--use-embedded-llc" => {
                options.use_embedded_llc = true;
            }
            "--test" => {
                options.test_mode = true;
            }
            "--detect-cycles" => {
                options.detect_cycles = true;
            }
            "--no-panic" => {
                options.no_panic = true;
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
            _ if arg.starts_with("--define=") => {
                options.defines.push(arg[10..].to_string());
            }
            _ if arg.starts_with("--undefine=") => {
                options.undefines.push(arg[12..].to_string());
            }
            _ if arg.starts_with("-D:") => {
                // 兼容旧语法 -D:NAME
                options.defines.push(arg[3..].to_string());
            }
            _ if arg.starts_with("-U:") => {
                // 兼容旧语法 -U:NAME
                options.undefines.push(arg[3..].to_string());
            }
            _ if arg.starts_with("-D") => {
                options.defines.push(arg[2..].to_string());
            }
            _ if arg.starts_with("-U") => {
                options.undefines.push(arg[2..].to_string());
            }
            _ => {
                if arg.starts_with('-') {
                    return Err(format!("未知选项: {}", arg));
                }
                positional.push(arg.clone());
            }
        }
        i += 1;
    }

    // 解析位置参数：.cay 结尾的是源文件，最后一个非 .cay 的是输出文件
    let mut source_files: Vec<String> = Vec::new();
    let mut output_file: Option<String> = None;
    let mut has_positional_output = false;

    for (idx, arg) in positional.iter().enumerate() {
        let is_source = arg.to_lowercase().ends_with(".cay");
        if is_source {
            source_files.push(arg.clone());
        } else if idx == positional.len() - 1 {
            output_file = Some(arg.clone());
            has_positional_output = true;
        } else {
            return Err(format!(
                "多余参数 '{}': 非 .cay 文件只能作为最后一个输出参数",
                arg
            ));
        }
    }

    if source_files.is_empty() {
        return Err("需要指定至少一个输入文件".to_string());
    }

    let mut output_file = output_file.unwrap_or_else(|| {
        let first_source = &source_files[0];
        let stem = Path::new(first_source)
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

    if let Some(explicit_output) = options.output.as_ref() {
        if has_positional_output && output_file != explicit_output.as_str() {
            // 如果 -o 与最后一个位置参数冲突，给出明确错误
            return Err(format!(
                "不能同时使用 -o/--output 和位置参数指定输出文件名: -o {} 与 {}",
                explicit_output, output_file
            ));
        }
        output_file = explicit_output.clone();
    }

    Ok((options, source_files, output_file))
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let (options, source_paths, exe_output) = match parse_args(&args) {
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

    println!("Cavvy 编译器 v{}", VERSION);
    println!("源文件: {}", source_paths.join(", "));
    if !options.compile_only {
        println!("输出: {}", exe_output);
    }
    println!("优化级别: {}", options.optimization);

    if options.compile_only {
        println!("模式: 仅编译 (-c)");
    }
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
    if options.use_clang {
        println!("工具链: clang");
    } else if options.use_llc_lld {
        println!("工具链: llc+lld (强制)");
    } else if options.use_embedded_llc {
        println!("工具链: 内嵌 llc + lld (实验性)");
    } else {
        println!("工具链: llc+lld (默认)");
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

    // 1. 构建编译器选项
    let compiler_options = cavvy::CompilerOptions {
        target_os: std::env::consts::OS.to_string(),
        features: options.features.clone(),
        no_features: Vec::new(),
        defines: options.defines.clone(),
        undefines: options.undefines.clone(),
        obfuscate: false,
        debug: options.debug,
        include_paths: options.include_paths.clone(),
        test_mode: options.test_mode,
        detect_cycles: options.detect_cycles,
        no_panic: options.no_panic,
    };
    let compiler = cavvy::Compiler::with_options(compiler_options);

    // 为每个源文件生成 IR 和对象文件路径
    let mut ir_files: Vec<String> = Vec::new();
    let mut obj_files: Vec<String> = Vec::new();

    for source_path in &source_paths {
        let ir_file = Path::new(source_path)
            .with_extension("ll")
            .to_string_lossy()
            .to_string();
        let obj_file = Path::new(source_path)
            .with_extension("obj")
            .to_string_lossy()
            .to_string();
        ir_files.push(ir_file);
        obj_files.push(obj_file);
    }

    // 2. Cavvy → IR（每个源文件独立编译）
    println!("[1] Cavvy → IR 编译...");
    for (idx, source_path) in source_paths.iter().enumerate() {
        let ir_file = &ir_files[idx];
        println!("  编译: {} -> {}", source_path, ir_file);

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

        match compiler.compile_file(source_path, ir_file) {
            Ok(_) => {
                println!("    [+] 编译成功");
            }
            Err(e) => {
                print_error_with_context(&e, &source, source_path);
                cleanup_temp_files(&ir_files, &obj_files, options.keep_ir, options.compile_only);
                process::exit(1);
            }
        }
    }

    // 3. IR 优化 (如果启用)
    if options.opt_ir {
        println!("");
        println!("[2] IR 优化 ({})...", options.optimization);
        // IR 优化现在由 ir2exe_lib 在编译时自动处理
        println!("  [I] IR 优化将在编译阶段自动进行");
    }

    // 4. 构建 ir2exe 选项，并收集所有 #link 声明和 ws2_32
    let mut ir2exe_options = Ir2ExeOptions {
        optimization: options.optimization.clone(),
        debug: options.debug,
        extra_lib_paths: options.extra_lib_paths.clone(),
        extra_libs: options.extra_libs.clone(),
        extra_ldflags: options.extra_ldflags.clone(),
        extra_cflags: options.extra_cflags.clone(),
        target: options.target.clone(),
        static_link: options.static_link,
        position_independent: options.position_independent,
        lto: options.lto,
        lto_thin: options.lto_thin,
        march: options.march.clone(),
        mtune: options.mtune.clone(),
        mcpu: options.mcpu.clone(),
        msse: options.msse.clone(),
        mavx: options.mavx.clone(),
        mneon: options.mneon,
        pgo_gen: options.pgo_gen,
        pgo_use: options.pgo_use.clone(),
        pgo_cs: options.pgo_cs,
        fno_exceptions: options.fno_exceptions,
        fno_rtti: options.fno_rtti,
        fomit_frame_pointer: options.fomit_frame_pointer,
        funroll_loops: options.funroll_loops,
        fvectorize: options.fvectorize,
        fslp_vectorize: options.fslp_vectorize,
        use_clang: options.use_clang,
        use_llc_lld: options.use_llc_lld,
        use_embedded_llc: options.use_embedded_llc,
    };

    // 解析每个 IR 文件的源映射和链接库信息
    let mut per_file_source_maps: Vec<IRSourceMap> = Vec::new();
    for ir_file in &ir_files {
        let ir_content = match fs::read_to_string(ir_file) {
            Ok(content) => content,
            Err(e) => {
                print_miette_error(
                    "cavvy::io_error",
                    &format!("无法读取IR文件 '{}': {}", ir_file, e),
                    Some("请检查IR文件路径是否正确"),
                );
                cleanup_temp_files(&ir_files, &obj_files, options.keep_ir, options.compile_only);
                process::exit(1);
            }
        };

        let source_map = parse_source_map_from_ir(&ir_content);
        if !source_map.mappings.is_empty() {
            println!("  [I] {} 已加载源映射: {} 个映射点", ir_file, source_map.mappings.len());
        }

        let link_libraries = parse_link_libraries_from_ir(&ir_content);
        if !link_libraries.is_empty() {
            println!("  [I] {} 发现链接库声明: {:?}", ir_file, link_libraries);
        }
        for lib in link_libraries {
            if !ir2exe_options.extra_libs.contains(&lib) {
                ir2exe_options.extra_libs.push(lib);
            }
        }

        // Windows 平台自动检测 socket 相关函数并添加 ws2_32 库
        #[cfg(target_os = "windows")]
        if ir_content.contains("WSAStartup")
            || ir_content.contains("socket(")
            || ir_content.contains("@socket(")
        {
            if !ir2exe_options.extra_libs.contains(&"ws2_32".to_string()) {
                ir2exe_options.extra_libs.push("ws2_32".to_string());
            }
        }

        per_file_source_maps.push(source_map);
    }

    // 5. IR → OBJ（每个 IR 文件独立编译为目标文件）
    println!("");
    let step_num = if options.opt_ir { "[3]" } else { "[2]" };
    println!("{} IR → OBJ 编译...", step_num);

    for (idx, ir_file) in ir_files.iter().enumerate() {
        let obj_file = &obj_files[idx];
        let source_map = &per_file_source_maps[idx];
        println!("  编译: {} -> {}", ir_file, obj_file);

        match compile_ir_to_object(ir_file, obj_file, &ir2exe_options, Some(source_map)) {
            Ok(result) => {
                for msg in &result.messages {
                    println!("    {}", msg);
                }
            }
            Err(e) => {
                print_tool_error("ir2exe", &format!("IR→OBJ编译失败: {}", ir_file), Some(&e));
                cleanup_temp_files(&ir_files, &obj_files, options.keep_ir, options.compile_only);
                process::exit(1);
            }
        }
    }

    // 6. -c 模式：到此为止，保留 .obj 文件
    if options.compile_only {
        println!("");
        println!("[+] 仅编译完成，生成目标文件:");
        for obj_file in &obj_files {
            println!("  {}", obj_file);
        }

        // 删除中间 .ll 文件（如果不保留）
        if !options.keep_ir {
            for ir_file in &ir_files {
                let _ = fs::remove_file(ir_file);
            }
        } else {
            println!("");
            println!("[I] 保留 IR 文件:");
            for ir_file in &ir_files {
                println!("  {}", ir_file);
            }
        }
        return;
    }

    // 7. OBJ → EXE（链接所有目标文件）
    println!("");
    let link_step_num = if options.opt_ir { "[4]" } else { "[3]" };
    println!("{} OBJ → EXE 链接...", link_step_num);

    match link_objects_to_exe(&obj_files, &exe_output, &ir2exe_options) {
        Ok(result) => {
            for msg in &result.messages {
                println!("  {}", msg);
            }
        }
        Err(e) => {
            print_tool_error("ir2exe", "OBJ→EXE链接失败", Some(&e));
            cleanup_temp_files(&ir_files, &obj_files, options.keep_ir, options.compile_only);
            process::exit(1);
        }
    }

    // 清理中间文件
    cleanup_temp_files(&ir_files, &obj_files, options.keep_ir, options.compile_only);

    println!("");
    println!("[+] 编译完成!");
    println!("生成: {}", exe_output);
}

/// 清理中间 .ll 和 .obj 文件
fn cleanup_temp_files(
    ir_files: &[String],
    obj_files: &[String],
    keep_ir: bool,
    compile_only: bool,
) {
    if !keep_ir {
        for ir_file in ir_files {
            let _ = fs::remove_file(ir_file);
        }
    }

    // 非 -c 模式才删除 .obj；否则 .obj 是最终产物
    if !compile_only {
        for obj_file in obj_files {
            let _ = fs::remove_file(obj_file);
        }
    }
}
