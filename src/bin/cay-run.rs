use cavvy::Compiler;
use cavvy::bytecode::obfuscator;
use cavvy::bytecode::{jit, serializer};
use cavvy::miette_diagnostic::CayError;
use cavvy::miette_diagnostic::{
    print_error_with_context, print_miette_error, print_tool_error, print_warning,
};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Stdio};

const VERSION: &str = env!("CAY_RUN_VERSION");

/// 运行选项
struct RunOptions {
    keep_temp: bool,             // --keep-temp: 保留临时文件
    verbose: bool,               // --verbose: 详细输出
    output_file: Option<String>, // -o: 指定输出可执行文件名
    no_run: bool,                // --no-run: 只编译不运行
    obfuscate: bool,             // --obfuscate: 混淆字节码（仅对.cay有效）
    obfuscate_level: String,     // --obfuscate-level: 混淆级别
    link_libs: Vec<String>,      // -l: 链接的库
    lib_paths: Vec<String>,      // -L: 库搜索路径
    optimize: String,            // -O: 优化级别
    features: Vec<String>,       // -F/--feature: 启用的语言特性
    defines: Vec<String>,        // -D/--define: 预定义宏
    undefines: Vec<String>,      // -U/--undefine: 取消预定义宏
    use_embedded_llc: bool,      // --use-embedded-llc: 使用内嵌 llc
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            keep_temp: false,
            verbose: false,
            output_file: None,
            no_run: false,
            obfuscate: false,
            obfuscate_level: "normal".to_string(),
            link_libs: Vec::new(),
            lib_paths: Vec::new(),
            optimize: "-O2".to_string(),
            features: Vec::new(),
            defines: Vec::new(),
            undefines: Vec::new(),
            use_embedded_llc: false,
        }
    }
}

fn print_usage() {
    println!("Cavvy Runner v{}", VERSION);
    println!("Usage: cay-run [options] <file>");
    println!("");
    println!("支持的文件类型:");
    println!("  .cay     - Cavvy源代码文件");
    println!("  .caybc   - Cavvy字节码文件");
    println!("  .ll      - LLVM IR文件");
    println!("");
    println!("Options:");
    println!("  -o <file>              指定输出可执行文件名");
    println!("  --no-run               只编译不运行");
    println!("  --obfuscate            混淆字节码（仅对.cay文件）");
    println!("  --obfuscate-level <l>  混淆级别: light, normal, deep (默认: normal)");
    println!("  -l<lib>                链接指定库");
    println!("  -L<path>               添加库搜索路径");
    println!("  -O<level>              优化级别 (0, 1, 2, 3, s, z)");
    println!("  -F<feature>            启用语言特性 (如: -F=top_level_function)");
    println!("  -D<macro>[=<value>]    定义预处理器宏");
    println!("  -U<macro>              取消预处理器宏定义");
    println!("  --keep-temp            保留临时文件");
    println!("  --verbose, -v          显示详细编译信息");
    println!("  --use-embedded-llc     使用内嵌 llc 编译 IR (实验性)");
    println!("  --version, -V          显示版本号");
    println!("  --help, -h             显示帮助信息");
    println!("");
    println!("Examples:");
    println!("  cay-run hello.cay");
    println!("  cay-run -o myapp hello.cay");
    println!("  cay-run --obfuscate --obfuscate-level deep hello.cay");
    println!("  cay-run program.caybc");
    println!("  cay-run output.ll");
    println!("  cay-run -luser32 -lkernel32 winapp.cay");
}

fn parse_args(args: &[String]) -> Result<(RunOptions, String), String> {
    let mut options = RunOptions::default();
    let mut input_file: Option<String> = None;
    let mut i = 1;

    while i < args.len() {
        let arg = &args[i];

        if arg.starts_with("-l") && arg.len() > 2 && !arg.starts_with("-F") {
            // -lxxx 格式
            options.link_libs.push(arg[2..].to_string());
        } else if arg.starts_with("-L") && arg.len() > 2 {
            // -Lxxx 格式
            options.lib_paths.push(arg[2..].to_string());
        } else if arg.starts_with("-O") && arg.len() > 1 {
            // -Oxxx 格式
            options.optimize = format!("-O{}", &arg[2..]);
        } else if arg.starts_with("-F") {
            // -F<feature> 或 -F=<feature> 格式
            let feature = if arg.starts_with("-F=") {
                arg[3..].to_string()
            } else if arg.len() > 2 {
                arg[2..].to_string()
            } else {
                i += 1;
                if i >= args.len() {
                    return Err("-F 需要特性名称参数".to_string());
                }
                args[i].clone()
            };
            options.features.push(feature);
        } else {
            match arg.as_str() {
                "--version" | "-V" => {
                    println!("Cavvy Runner v{}", VERSION);
                    process::exit(0);
                }
                "--help" | "-h" => {
                    print_usage();
                    process::exit(0);
                }
                "--verbose" | "-v" => {
                    options.verbose = true;
                }
                "--use-embedded-llc" => {
                    options.use_embedded_llc = true;
                }
                "--keep-temp" => {
                    options.keep_temp = true;
                }
                "--no-run" => {
                    options.no_run = true;
                }
                "--obfuscate" => {
                    options.obfuscate = true;
                }
                "--obfuscate-level" => {
                    if i + 1 < args.len() {
                        options.obfuscate_level = args[i + 1].clone();
                        if !["light", "normal", "deep"].contains(&options.obfuscate_level.as_str())
                        {
                            return Err(format!("无效的混淆级别: {}", options.obfuscate_level));
                        }
                        i += 1;
                    } else {
                        return Err("--obfuscate-level 需要一个参数".to_string());
                    }
                }
                "-o" => {
                    if i + 1 < args.len() {
                        options.output_file = Some(args[i + 1].clone());
                        i += 1;
                    } else {
                        return Err("-o 需要一个参数".to_string());
                    }
                }
                "-l" => {
                    if i + 1 < args.len() {
                        options.link_libs.push(args[i + 1].clone());
                        i += 1;
                    } else {
                        return Err("-l 需要一个参数".to_string());
                    }
                }
                "-L" => {
                    if i + 1 < args.len() {
                        options.lib_paths.push(args[i + 1].clone());
                        i += 1;
                    } else {
                        return Err("-L 需要一个参数".to_string());
                    }
                }
                "-O" => {
                    if i + 1 < args.len() {
                        options.optimize = format!("-O{}", args[i + 1]);
                        i += 1;
                    } else {
                        return Err("-O 需要一个参数".to_string());
                    }
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
                    } else {
                        return Err(format!("多余参数: {}", arg));
                    }
                }
            }
        }
        i += 1;
    }

    let input_file = input_file.ok_or("需要指定输入文件")?;
    Ok((options, input_file))
}

/// 检测文件类型
fn detect_file_type(path: &str) -> Result<FileType, String> {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .ok_or("无法确定文件类型")?;

    match ext.to_lowercase().as_str() {
        "cay" => Ok(FileType::CaySource),
        "caybc" => Ok(FileType::CayBytecode),
        "ll" => Ok(FileType::LlvmIr),
        _ => Err(format!("不支持的文件类型: {}", ext)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FileType {
    CaySource,   // .cay
    CayBytecode, // .caybc
    LlvmIr,      // .ll
}

/// 获取临时目录
fn get_temp_dir() -> PathBuf {
    let temp_dir = env::temp_dir().join("cavvy-run");
    let _ = fs::create_dir_all(&temp_dir);
    temp_dir
}

/// 生成唯一文件名
fn generate_unique_filename(prefix: &str, ext: &str) -> PathBuf {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let pid = process::id();
    get_temp_dir().join(format!("{}_{}_{}.{}", prefix, timestamp, pid, ext))
}

/// 获取系统包含路径（caylibs目录）
fn get_system_include_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. 从可执行文件所在目录查找 caylibs
    // 使用 canonicalize 解析符号链接（Linux上current_exe可能返回/proc/self/exe）
    if let Ok(exe_path) = env::current_exe() {
        let exe_path = exe_path.canonicalize().unwrap_or(exe_path);
        if let Some(exe_dir) = exe_path.parent() {
            let exe_caylibs = exe_dir.join("caylibs");
            if exe_caylibs.exists() {
                paths.push(exe_caylibs);
            }
        }
    }

    // 2. 从当前工作目录查找 caylibs
    let cwd_caylibs = PathBuf::from("caylibs");
    if cwd_caylibs.exists() && !paths.contains(&cwd_caylibs) {
        paths.push(cwd_caylibs);
    }

    paths
}

/// 编译Cay源码为IR
fn compile_cay_to_ir(source_path: &str, options: &RunOptions) -> Result<String, CayError> {
    let source = fs::read_to_string(source_path).map_err(|e| CayError::Io {
        error_code: "I0001",
        file: Some(source_path.to_string()),
        message: format!("读取源文件失败: {}", e),
    })?;

    // 预处理
    let base_dir = Path::new(source_path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    // 获取系统包含路径
    let system_paths = get_system_include_paths();

    // 使用带系统路径的预处理器（带源映射）
    let base_dir_str = base_dir.to_str().unwrap_or(".");
    let preprocess_result = if system_paths.is_empty() {
        let mut pp = cavvy::preprocessor::Preprocessor::new(base_dir_str);
        pp.process_with_source_map(&source, source_path)
    } else {
        let mut pp =
            cavvy::preprocessor::Preprocessor::with_include_paths(base_dir_str, system_paths);
        pp.process_with_source_map(&source, source_path)
    }
    .map_err(|e| CayError::Preprocessor {
        error_code: cavvy::miette_diagnostic::ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
        file: Some(source_path.to_string()),
        line: 0,
        column: 0,
        message: format!("预处理失败: {:?}", e),
        suggestion: "请检查预处理指令".to_string(),
    })?;

    // 转换源映射为HashMap格式
    let source_map: std::collections::HashMap<usize, (String, usize)> = preprocess_result
        .source_map
        .mappings
        .iter()
        .enumerate()
        .map(|(idx, pos)| (idx + 1, (pos.file.clone(), pos.line)))
        .collect();

    // 编译
    let compiler_options = cavvy::CompilerOptions {
        target_os: env::consts::OS.to_string(),
        features: options.features.clone(),
        no_features: Vec::new(),
        defines: options.defines.clone(),
        undefines: options.undefines.clone(),
        obfuscate: options.obfuscate,
        debug: false,
        include_paths: Vec::new(),
        test_mode: false,
    };

    let compiler = Compiler::with_options(compiler_options);

    // 使用临时文件
    let temp_ir_file = generate_unique_filename("cay", "ll");
    let temp_ir_str = temp_ir_file.to_str().ok_or_else(|| CayError::Io {
        error_code: "I0001",
        file: None,
        message: "临时文件路径包含无效UTF-8字符".to_string(),
    })?;
    compiler.compile_with_source_map(&preprocess_result.code, source_map, temp_ir_str)?;

    let ir = fs::read_to_string(&temp_ir_file).map_err(|e| CayError::Io {
        error_code: "I0001",
        file: temp_ir_file.to_str().map(|s| s.to_string()),
        message: format!("读取IR文件失败: {}", e),
    })?;

    if !options.keep_temp {
        let _ = fs::remove_file(&temp_ir_file);
    }

    Ok(ir)
}

/// 编译Cay源码为字节码
fn compile_cay_to_bytecode(
    source_path: &str,
    options: &RunOptions,
) -> Result<cavvy::bytecode::BytecodeModule, String> {
    let source = fs::read_to_string(source_path).map_err(|e| format!("读取源文件失败: {}", e))?;

    // 词法分析
    let tokens = cavvy::lexer::lex(&source).map_err(|e| format!("词法分析错误: {:?}", e))?;

    // 语法分析
    let ast = cavvy::parser::parse(tokens).map_err(|e| format!("语法分析错误: {:?}", e))?;

    // 语义分析
    let mut analyzer = cavvy::semantic::SemanticAnalyzer::new();
    let ast = analyzer
        .analyze(ast)
        .map_err(|e| format!("语义分析错误: {:?}", e))?;

    // 生成字节码模块
    let mut module = cavvy::bytecode::BytecodeModule::new(
        Path::new(source_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unnamed")
            .to_string(),
        env::consts::OS.to_string(),
    );

    // TODO: 实现完全完整的字节码模块生成逻辑

    if options.obfuscate {
        let obf_options = match options.obfuscate_level.as_str() {
            "light" => obfuscator::ObfuscationOptions {
                obfuscate_names: true,
                obfuscate_control_flow: false,
                insert_junk_code: false,
                encrypt_strings: false,
                shuffle_functions: false,
                strip_debug_info: true,
            },
            "normal" => obfuscator::ObfuscationOptions {
                obfuscate_names: true,
                obfuscate_control_flow: true,
                insert_junk_code: false,
                encrypt_strings: true,
                shuffle_functions: false,
                strip_debug_info: true,
            },
            "deep" => obfuscator::ObfuscationOptions {
                obfuscate_names: true,
                obfuscate_control_flow: true,
                insert_junk_code: true,
                encrypt_strings: true,
                shuffle_functions: true,
                strip_debug_info: true,
            },
            _ => obfuscator::ObfuscationOptions::default(),
        };

        let mut obfuscator = obfuscator::BytecodeObfuscator::new(obf_options);
        obfuscator.obfuscate(&mut module);
    }

    Ok(module)
}

/// 编译字节码为IR
fn compile_bytecode_to_ir(bytecode_path: &str, options: &RunOptions) -> Result<String, String> {
    // 读取字节码文件
    let module = serializer::deserialize_from_file(bytecode_path)
        .map_err(|e| format!("反序列化字节码失败: {}", e))?;

    if options.verbose {
        println!("字节码模块: {}", module.header.name);
        println!("目标平台: {}", module.header.target_platform);
        println!("已混淆: {}", module.header.obfuscated);
        println!("函数数量: {}", module.functions.len());
        println!("类型数量: {}", module.type_definitions.len());
    }

    // 使用JIT编译器将字节码转换为IR
    let jit_options = jit::JitOptions {
        optimization: options.optimize.clone(),
        target: env::consts::OS.to_string(),
        keep_intermediate: options.keep_temp,
        output_dir: Some(get_temp_dir().to_string_lossy().to_string()),
        link_libs: options.link_libs.clone(),
        lib_paths: options.lib_paths.clone(),
    };

    let compiler = jit::JitCompiler::new(jit_options);

    // 将字节码转换为IR字符串
    compiler
        .bytecode_to_ir(&module)
        .map_err(|e| format!("字节码转IR失败: {}", e))
}

/// 编译IR为可执行文件（使用ir2exe）
fn compile_ir_to_executable(
    ir_code: &str,
    output_path: &str,
    options: &RunOptions,
) -> Result<(), String> {
    // 创建临时IR文件
    let temp_ir_file = generate_unique_filename("cay", "ll");
    fs::write(&temp_ir_file, ir_code).map_err(|e| format!("写入临时IR文件失败: {}", e))?;

    if options.verbose {
        println!("编译IR到可执行文件...");
    }

    // 获取可执行文件所在目录
    let current_exe = env::current_exe().map_err(|e| format!("无法获取当前执行路径: {}", e))?;
    let bin_dir = current_exe.parent().ok_or("无法获取执行目录")?;

    // 查找 ir2exe
    let ir2exe_paths = [bin_dir.join("ir2exe"), bin_dir.join("ir2exe.exe")];

    let ir2exe_path = ir2exe_paths
        .iter()
        .find(|path| path.exists())
        .ok_or_else(|| {
            let paths_str = ir2exe_paths
                .iter()
                .map(|p| format!("  {:?}", p))
                .collect::<Vec<_>>()
                .join("\n");
            format!(
                "错误: 找不到 ir2exe 或 ir2exe.exe 在以下位置:\n{}",
                paths_str
            )
        })?;

    // 构建 ir2exe 参数
    let mut ir2exe_args: Vec<String> = vec![];

    // 优化级别
    ir2exe_args.push(options.optimize.clone());

    // 使用内嵌 llc
    if options.use_embedded_llc {
        ir2exe_args.push("--use-embedded-llc".to_string());
    }

    // 额外库路径
    for path in &options.lib_paths {
        ir2exe_args.push(format!("-L{}", path));
    }

    #[cfg(target_os = "windows")]
    if ir_code.contains("WSAStartup") || ir_code.contains("socket(") || ir_code.contains("@socket(")
    {
        ir2exe_args.push("-lws2_32".to_string());
    }

    // 额外库
    for lib in &options.link_libs {
        ir2exe_args.push(format!("-l{}", lib));
    }

    // 输入输出文件
    ir2exe_args.push(temp_ir_file.to_string_lossy().to_string());
    ir2exe_args.push(output_path.to_string());

    if options.verbose {
        println!("调用: {} {}", ir2exe_path.display(), ir2exe_args.join(" "));
    }

    // 调用ir2exe
    let output = Command::new(&ir2exe_path)
        .args(&ir2exe_args)
        .output()
        .map_err(|e| format!("执行ir2exe失败: {}", e))?;

    // 清理临时IR文件
    if !options.keep_temp {
        let _ = fs::remove_file(&temp_ir_file);
    }

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("编译IR失败: {}", stderr));
    }

    Ok(())
}

/// 运行可执行文件
fn run_executable(exe_path: &str, options: &RunOptions) -> Result<i32, String> {
    // 转换为绝对路径
    let exe_path_abs = Path::new(exe_path);
    let exe_path_abs = if exe_path_abs.is_absolute() {
        exe_path_abs.to_path_buf()
    } else {
        env::current_dir()
            .map_err(|e| format!("无法获取当前目录: {}", e))?
            .join(exe_path_abs)
    };
    let exe_path_str = exe_path_abs.to_string_lossy().to_string();

    if options.verbose {
        println!("运行: {}", exe_path_str);
        println!();
    }

    let mut cmd = Command::new(&exe_path_str);
    cmd.stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd.status().map_err(|e| format!("运行程序失败: {}", e))?;

    Ok(status.code().unwrap_or(1))
}

fn main() {
    let args: Vec<String> = env::args().collect();

    let (options, input_path) = match parse_args(&args) {
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

    // 检查输入文件是否存在
    if !Path::new(&input_path).exists() {
        print_miette_error(
            "cavvy::io_error",
            &format!("输入文件 '{}' 不存在", input_path),
            Some("请检查文件路径是否正确"),
        );
        process::exit(1);
    }

    // 检测文件类型
    let file_type = match detect_file_type(&input_path) {
        Ok(t) => t,
        Err(e) => {
            print_miette_error(
                "cavvy::file_type_error",
                &e,
                Some("支持的文件类型: .cay, .caybc, .ll"),
            );
            process::exit(1);
        }
    };

    if options.verbose {
        println!("Cavvy Runner v{}", VERSION);
        println!("输入文件: {} ({:?})", input_path, file_type);
        println!();
    }

    // 确定输出可执行文件名
    let output_exe = options.output_file.clone().unwrap_or_else(|| {
        let stem = Path::new(&input_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("a");

        if cfg!(target_os = "windows") {
            format!("{}.exe", stem)
        } else {
            stem.to_string()
        }
    });

    // 根据文件类型处理
    let ir_code = match file_type {
        FileType::CaySource => {
            if options.verbose {
                println!("[1/3] 编译Cay源码到IR...");
            }

            // 如果启用了混淆，先编译到字节码
            if options.obfuscate {
                if options.verbose {
                    println!("使用字节码混淆模式...");
                }
                let module = compile_cay_to_bytecode(&input_path, &options).unwrap_or_else(|e| {
                    print_tool_error("字节码编译器", &e, Some("请检查代码语法和语义"));
                    process::exit(1);
                });

                // 保存字节码到临时文件
                let temp_bc_file = generate_unique_filename("cay", "caybc");
                let bytecode = serializer::serialize(&module);
                fs::write(&temp_bc_file, bytecode).unwrap_or_else(|e| {
                    print_miette_error(
                        "cavvy::io_error",
                        &format!("写入字节码文件失败: {}", e),
                        Some("请检查输出目录权限"),
                    );
                    process::exit(1);
                });

                // 从字节码编译到IR
                let bc_path = temp_bc_file.to_str().unwrap_or_else(|| {
                    print_miette_error(
                        "cavvy::io_error",
                        "字节码临时文件路径包含无效UTF-8字符",
                        None,
                    );
                    process::exit(1);
                });
                let ir = compile_bytecode_to_ir(bc_path, &options).unwrap_or_else(|e| {
                    print_tool_error("字节码转IR", &e, Some("请检查字节码文件是否正确"));
                    process::exit(1);
                });

                if !options.keep_temp {
                    let _ = fs::remove_file(&temp_bc_file);
                }

                ir
            } else {
                match compile_cay_to_ir(&input_path, &options) {
                    Ok(ir) => ir,
                    Err(e) => {
                        let source = fs::read_to_string(&input_path).unwrap_or_default();
                        print_error_with_context(&e, &source, &input_path);
                        process::exit(1);
                    }
                }
            }
        }
        FileType::CayBytecode => {
            if options.verbose {
                println!("[1/3] 编译字节码到IR...");
            }
            compile_bytecode_to_ir(&input_path, &options).unwrap_or_else(|e| {
                print_tool_error("字节码编译器", &e, Some("请检查字节码文件是否正确"));
                process::exit(1);
            })
        }
        FileType::LlvmIr => {
            if options.verbose {
                println!("[1/3] 读取IR文件...");
            }
            fs::read_to_string(&input_path).unwrap_or_else(|e| {
                print_miette_error(
                    "cavvy::io_error",
                    &format!("读取IR文件失败: {}", e),
                    Some("请检查文件路径是否正确"),
                );
                process::exit(1);
            })
        }
    };

    if options.verbose {
        println!("[2/3] 编译IR到可执行文件...");
    }

    // 编译IR到可执行文件
    compile_ir_to_executable(&ir_code, &output_exe, &options).unwrap_or_else(|e| {
        print_tool_error("ir2exe", &e, Some("请检查 IR 代码是否正确"));
        process::exit(1);
    });

    if options.verbose {
        println!(
            "[3/3] {}...",
            if options.no_run {
                "跳过运行"
            } else {
                "运行程序"
            }
        );
        println!();
    }

    // 运行可执行文件
    if !options.no_run {
        let exit_code = run_executable(&output_exe, &options).unwrap_or_else(|e| {
            print_miette_error(
                "cavvy::runtime_error",
                &format!("运行失败: {}", e),
                Some("请检查程序是否正确编译"),
            );
            process::exit(1);
        });

        if options.verbose {
            println!();
            println!("程序退出码: {}", exit_code);
        }

        process::exit(exit_code);
    } else {
        if options.verbose {
            println!("可执行文件已生成: {}", output_exe);
        } else {
            println!("已生成: {}", output_exe);
        }
    }
}
