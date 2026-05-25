use std::env;
use std::process;
use std::path::{Path, PathBuf, Component};
use std::fs;
use cavvy::error::{print_miette_error, print_tool_error, print_warning};

/// 规范化路径，去除 . 和 ..

/// 源位置信息
#[derive(Debug, Clone)]
struct SourcePosition {
    file: String,
    line: usize,
    column: usize,
}

/// IR源映射表 - 从IR行号到源位置的映射
#[derive(Debug, Clone, Default)]
struct IRSourceMap {
    mappings: std::collections::HashMap<usize, SourcePosition>,
}

impl IRSourceMap {
    fn new() -> Self {
        Self {
            mappings: std::collections::HashMap::new(),
        }
    }

    fn add_mapping(&mut self, ir_line: usize, file: String, line: usize, column: usize) {
        self.mappings.insert(ir_line, SourcePosition { file, line, column });
    }

    fn get_source_position(&self, ir_line: usize) -> Option<&SourcePosition> {
        // 首先尝试精确匹配
        if let Some(pos) = self.mappings.get(&ir_line) {
            return Some(pos);
        }
        
        // 如果没有精确匹配，查找最近的映射（小于或等于给定行号的最大映射）
        // 这用于处理clang报告的错误行号是代码行，而源映射在注释行的情况
        let mut closest_line = 0usize;
        let mut found = false;
        
        for (&mapped_line, _) in &self.mappings {
            if mapped_line <= ir_line && mapped_line > closest_line {
                closest_line = mapped_line;
                found = true;
            }
        }
        
        if found {
            self.mappings.get(&closest_line)
        } else {
            None
        }
    }
}

/// 从IR文件中解析源映射注释
/// 格式: ; !source file.cay:10:5
fn parse_source_map_from_ir(ir_content: &str) -> IRSourceMap {
    let mut source_map = IRSourceMap::new();
    let mut current_line = 0usize;

    for line in ir_content.lines() {
        current_line += 1;
        
        // 检查是否是源映射注释
        if let Some(comment_start) = line.find("; !source ") {
            let comment = &line[comment_start + 10..]; // 跳过 "; !source "
            
            // 解析格式: file:line:column
            // 处理Windows路径 (E:\path\file.cay:10:5) - 从后往前找冒号
            if let Some(last_colon) = comment.rfind(':') {
                if let Some(second_last_colon) = comment[..last_colon].rfind(':') {
                    let file = comment[..second_last_colon].to_string();
                    let line_str = &comment[second_last_colon + 1..last_colon];
                    let col_str = &comment[last_colon + 1..];
                    
                    if let (Ok(line_num), Ok(col_num)) = (line_str.parse::<usize>(), col_str.parse::<usize>()) {
                        source_map.add_mapping(current_line, file, line_num, col_num);
                    }
                }
            }
        }
    }

    source_map
}

/// 解析clang错误信息中的行号
fn parse_clang_error_line(error_msg: &str) -> Option<usize> {
    // 匹配格式: filename.ll:123:45: error: ...
    // 或: <stdin>:123:45: error: ...
    for line in error_msg.lines() {
        // 查找 .ll: 后的数字
        if let Some(pos) = line.find(".ll:") {
            let rest = &line[pos + 4..];
            if let Some(colon_pos) = rest.find(':') {
                let line_num_str = &rest[..colon_pos];
                if let Ok(line_num) = line_num_str.parse::<usize>() {
                    return Some(line_num);
                }
            }
        }
        
        // 匹配 <stdin>: 格式
        if let Some(pos) = line.find("<stdin>:") {
            let rest = &line[pos + 8..];
            if let Some(colon_pos) = rest.find(':') {
                let line_num_str = &rest[..colon_pos];
                if let Ok(line_num) = line_num_str.parse::<usize>() {
                    return Some(line_num);
                }
            }
        }
    }
    None
}

/// 读取源文件的指定行及其上下文
fn read_source_context(file_path: &str, line_num: usize, context_lines: usize) -> Option<String> {
    let content = fs::read_to_string(file_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    
    if line_num == 0 || line_num > lines.len() {
        return None;
    }
    
    let start = line_num.saturating_sub(context_lines + 1);
    let end = (line_num + context_lines).min(lines.len());
    
    let mut result = String::new();
    for i in start..end {
        let line_number = i + 1;
        let prefix = if line_number == line_num {
            "  > "
        } else {
            "    "
        };
        result.push_str(&format!("{}{:4} | {}\n", prefix, line_number, lines[i]));
    }
    
    Some(result)
}

/// 将clang错误信息中的IR行号替换为源位置，并显示源代码上下文
fn remap_clang_error(error_msg: &str, source_map: &IRSourceMap, _ir_file_name: &str) -> String {
    let mut result = String::new();
    let mut last_source_file: Option<String> = None;
    let mut last_source_line: Option<usize> = None;
    
    for line in error_msg.lines() {
        // 尝试解析错误行号
        if let Some(ir_line) = parse_clang_error_line(line) {
            if let Some(source_pos) = source_map.get_source_position(ir_line) {
                // 避免重复显示相同的源位置
                let is_duplicate = last_source_file.as_ref() == Some(&source_pos.file) 
                    && last_source_line == Some(source_pos.line);
                
                if !is_duplicate {
                    // 添加源文件位置头
                    result.push_str(&format!("\n  at {}:{}:{}\n", source_pos.file, source_pos.line, source_pos.column));
                    
                    // 读取并显示源代码上下文
                    if let Some(context) = read_source_context(&source_pos.file, source_pos.line, 2) {
                        result.push_str(&context);
                    }
                    
                    last_source_file = Some(source_pos.file.clone());
                    last_source_line = Some(source_pos.line);
                }
                
                // 修改错误行，指向源文件
                let modified_line = if line.contains("error:") {
                    // 提取错误消息部分
                    if let Some(error_pos) = line.find("error:") {
                        let error_msg_part = &line[error_pos + 6..];
                        format!("\n  error: {}", error_msg_part)
                    } else {
                        format!("\n  {}", line)
                    }
                } else if line.contains("warning:") {
                    if let Some(warning_pos) = line.find("warning:") {
                        let warning_msg_part = &line[warning_pos + 8..];
                        format!("\n  warning: {}", warning_msg_part)
                    } else {
                        format!("\n  {}", line)
                    }
                } else {
                    format!("\n  {}", line)
                };
                
                result.push_str(&modified_line);
                continue;
            }
        }
        
        // 对于代码指示行（如 "    |     ^"），跳过，因为我们已经显示了源代码上下文
        if line.trim().starts_with('|') || line.trim().starts_with('^') {
            continue;
        }
        
        // 其他行（如 "1 error generated.")
        if !line.trim().is_empty() {
            result.push('\n');
            result.push_str(line);
        }
    }
    
    result.trim_end().to_string()
}

/// 添加Clang错误映射的说明信息
fn add_clang_error_notice(remapped_error: &str) -> String {
    format!(
        "{}\n\n  请注意，当 Clang 执行失败时，其映射代码输出并非 Cavvy 错误报告系统的组成部分，且不属于 Cavvy 错误。此项功能仅用于协助您排查相关问题。\n  如果您遇到 Clang 执行失败的报错信息，请立即通过提交 Issue 的方式告知我们，以便我们及时修复该问题。\n  Issue 提交地址：https://github.com/cavvy-lang/cavvy/issues",
        remapped_error
    )
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => {
                components.push(Component::Prefix(prefix));
            }
            Component::RootDir => {
                components.push(Component::RootDir);
            }
            Component::CurDir => {
                // 忽略 .
            }
            Component::ParentDir => {
                // 处理 ..
                if let Some(last) = components.last() {
                    if !matches!(last, Component::ParentDir) {
                        components.pop();
                    } else {
                        components.push(Component::ParentDir);
                    }
                }
            }
            Component::Normal(normal) => {
                components.push(Component::Normal(normal));
            }
        }
    }
    
    let mut result = PathBuf::new();
    for component in components {
        result.push(component.as_os_str());
    }
    result
}

/// 根据平台获取 clang 可执行文件名
#[cfg(target_os = "windows")]
fn get_clang_exe_name() -> &'static str {
    "clang.exe"
}

#[cfg(target_os = "linux")]
fn get_clang_exe_name() -> &'static str {
    "clang-21"
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn get_clang_exe_name() -> &'static str {
    "clang"
}

/// 根据平台获取 llvm-minimal 下的 clang 路径列表
#[cfg(target_os = "windows")]
fn get_bundled_clang_paths(exe_dir: &Path) -> Vec<PathBuf> {
    get_llvm_minimal_paths(exe_dir, "llvm-minimal/bin/clang.exe")
}

#[cfg(target_os = "linux")]
fn get_bundled_clang_paths(exe_dir: &Path) -> Vec<PathBuf> {
    get_llvm_minimal_paths(exe_dir, "llvm-minimal/bin-linux/clang-21")
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn get_bundled_clang_paths(exe_dir: &Path) -> Vec<PathBuf> {
    get_llvm_minimal_paths(exe_dir, "llvm-minimal/bin/clang")
}

/// 工具链类型
#[derive(Debug, Clone, Copy, PartialEq)]
enum ToolchainType {
    /// 使用 clang 一站式编译
    Clang,
    /// 使用 llc + lld-link 分步编译
    LlcLld,
}

/// 查找 clang 可执行文件
/// 1. 首先尝试直接调用 "clang"（系统 PATH 中）
/// 2. 如果失败，尝试查找编译器所在目录或项目根目录下的 llvm-minimal/bin/clang
/// 3. 如果都找不到，返回错误
fn find_clang() -> Result<PathBuf, String> {
    // 1. 首先尝试系统 PATH 中的 clang
    if let Ok(output) = process::Command::new("clang").arg("--version").output() {
        if output.status.success() {
            return Ok(PathBuf::from("clang"));
        }
    }
    
    // 2. 尝试编译器所在目录或项目根目录下的 llvm-minimal
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for path in get_bundled_clang_paths(exe_dir) {
                if path.exists() {
                    return Ok(path);
                }
            }
        }
    }
    
    // 3. 都找不到，返回错误
    Err("找不到 clang 编译器。请确保 clang 已安装并在 PATH 中，或将 llvm-minimal 放在编译器同目录下。".to_string())
}

/// 查找 llc 可执行文件
fn find_llc() -> Result<PathBuf, String> {
    // 1. 首先尝试系统 PATH 中的 llc
    if let Ok(output) = process::Command::new("llc").arg("--version").output() {
        if output.status.success() {
            return Ok(PathBuf::from("llc"));
        }
    }
    
    // 2. 尝试编译器所在目录或项目根目录下的 llvm-minimal
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let sub_path = if cfg!(target_os = "windows") {
                "llvm-minimal/bin/llc.exe"
            } else {
                "llvm-minimal/bin-linux/llc"
            };
            
            for path in get_llvm_minimal_paths(exe_dir, sub_path) {
                if path.exists() {
                    return Ok(path);
                }
            }
        }
    }
    
    // 3. 都找不到，返回错误
    Err("找不到 llc (LLVM IR 编译器)。请确保 LLVM 已安装并在 PATH 中，或将 llvm-minimal 放在编译器同目录下。".to_string())
}

/// 获取可能的 llvm-minimal 路径列表
fn get_llvm_minimal_paths(exe_dir: &Path, sub_path: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    
    // 1. 编译器所在目录
    paths.push(exe_dir.join(sub_path));
    
    // 2. 尝试向上查找项目根目录 (适用于 target/release 或 target/debug 的情况)
    if let Some(parent) = exe_dir.parent() {
        // target/release -> target
        if let Some(grandparent) = parent.parent() {
            // target -> 项目根目录
            paths.push(grandparent.join(sub_path));
        }
    }
    
    paths
}

/// 根据目标平台获取对应的 lld 链接器名称
fn get_lld_linker_name(target: &str) -> &'static str {
    if target.contains("windows") || target.contains("mingw") {
        // MinGW 使用 GNU ld 风格 (ld.lld)
        "ld.lld"
    } else if target.contains("msvc") {
        // MSVC 使用 COFF 风格 (lld-link)
        "lld-link"
    } else if target.contains("darwin") || target.contains("macos") {
        "ld64.lld"
    } else if target.contains("wasm") || target.contains("emscripten") {
        "wasm-ld"
    } else {
        // 默认使用 GNU ld 风格 (Linux, BSD, etc.)
        "ld.lld"
    }
}

/// 查找指定平台的 lld 链接器
fn find_lld_for_target(target: &str) -> Result<PathBuf, String> {
    let linker_name = get_lld_linker_name(target);
    
    // 1. 首先尝试系统 PATH
    let test_arg = if linker_name == "lld-link" { "/?" } else { "--version" };
    if let Ok(output) = process::Command::new(linker_name).arg(test_arg).output() {
        if output.status.code().is_some() {
            return Ok(PathBuf::from(linker_name));
        }
    }
    
    // 2. 尝试编译器所在目录或项目根目录下的 llvm-minimal
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let exe_name = if cfg!(target_os = "windows") {
                format!("{}.exe", linker_name)
            } else {
                linker_name.to_string()
            };
            let sub_path = if cfg!(target_os = "windows") {
                format!("llvm-minimal/bin/{}", exe_name)
            } else {
                format!("llvm-minimal/bin-linux/{}", exe_name)
            };
            
            for path in get_llvm_minimal_paths(exe_dir, &sub_path) {
                if path.exists() {
                    return Ok(path);
                }
            }
        }
    }
    
    // 3. 都找不到，返回错误
    Err(format!("找不到 {} (LLVM 链接器)。请确保 LLVM 已安装并在 PATH 中，或将 llvm-minimal 放在编译器同目录下。", linker_name))
}

/// 检测可用的工具链
/// 优先使用 clang，如果不可用则尝试 llc + lld (根据目标平台自动选择链接器)
fn detect_toolchain(target: &str) -> Result<(ToolchainType, PathBuf, Option<PathBuf>), String> {
    // 首先尝试 clang
    if let Ok(clang_path) = find_clang() {
        return Ok((ToolchainType::Clang, clang_path, None));
    }
    
    // 如果 clang 不可用，尝试 llc + 对应平台的 lld
    if let Ok(llc_path) = find_llc() {
        if let Ok(lld_path) = find_lld_for_target(target) {
            return Ok((ToolchainType::LlcLld, llc_path, Some(lld_path)));
        }
    }
    
    // 都找不到
    Err(format!("找不到可用的编译工具链。请确保以下之一可用：\n\
         1. clang (推荐)\n\
         2. llc + {}\n\
         或将 llvm-minimal 目录放在编译器同目录下。", get_lld_linker_name(target)))
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
    // 工具链选择
    use_llc_lld: bool,            // --use-llc-lld
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
            use_llc_lld: false,
        }
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
    println!("  --use-llc-lld         强制使用 llc + lld-link 工具链（而不是 clang）");
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
            "--use-llc-lld" => {
                options.use_llc_lld = true;
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
    // 规范化路径（去除 . 和 ..）
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
    // 规范化路径（去除 . 和 ..）
    let output_file_abs = normalize_path(&output_file_abs);
    let output_file = output_file_abs.to_string_lossy().to_string();

    // 确保输出目录存在
    if let Some(parent) = output_file_abs.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
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
    let ir_content = match fs::read_to_string(&input_file) {
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

// 检测工具链
    if options.use_llc_lld {
        // 强制使用 llc + 对应平台的 lld
        let llc_path = match find_llc() {
            Ok(path) => path,
            Err(e) => {
                print_tool_error("llc", &e, Some("请确保 LLVM 已正确安装"));
                process::exit(1);
            }
        };
        let lld_path = match find_lld_for_target(&options.target) {
            Ok(path) => path,
            Err(e) => {
                print_tool_error(get_lld_linker_name(&options.target), &e, Some("请确保 LLVM 已正确安装"));
                process::exit(1);
            }
        };
        let linker_name = get_lld_linker_name(&options.target);
        println!("[I] 使用工具链: llc + {} (强制模式)", linker_name);
        println!("[I] 正在编译 IR → EXE...");
        compile_with_llc_lld(&input_file, &output_file, &options, &source_map, &llc_path, &lld_path);
    } else {
        // 自动检测工具链
        let (toolchain_type, tool_path, tool_path2) = match detect_toolchain(&options.target) {
            Ok(result) => result,
            Err(e) => {
                print_tool_error("toolchain", &e, Some("请确保 LLVM/Clang 已正确安装"));
                process::exit(1);
            }
        };

        match toolchain_type {
            ToolchainType::Clang => {
                println!("[I] 使用工具链: clang");
                println!("[I] 正在编译 IR → EXE...");
                compile_with_clang(&input_file, &output_file, &options, &source_map, &tool_path);
            }
            ToolchainType::LlcLld => {
                let linker_name = get_lld_linker_name(&options.target);
                println!("[I] 使用工具链: llc + {} (自动检测)", linker_name);
                println!("[I] 正在编译 IR → EXE...");
                let lld_path = tool_path2.expect("lld path should exist");
                compile_with_llc_lld(&input_file, &output_file, &options, &source_map, &tool_path, &lld_path);
            }
        }
    }
}

/// 使用 clang 编译 IR 到 EXE
fn compile_with_clang(
    input_file: &str,
    output_file: &str,
    options: &CompileOptions,
    source_map: &IRSourceMap,
    clang_exe: &PathBuf,
) {
    // 设置库路径 - 先获取可执行文件所在目录
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    // 根据目标平台选择库路径
    let mut lib_paths: Vec<PathBuf> = if options.target.contains("windows") || options.target.contains("mingw") {
        // Windows/MinGW 库路径
        vec![
            exe_dir.join("lib/mingw64/x86_64-w64-mingw32/lib"),
            exe_dir.join("lib/mingw64/lib"),
            exe_dir.join("lib/mingw64/lib/gcc/x86_64-w64-mingw32/15.2.0"),
        ]
    } else {
        // Linux/Unix 系统使用系统默认库路径
        vec![]
    };

    // 添加 Cavvy 运行时库路径（libcayrt.a 所在目录）
    let cayrt_path = exe_dir.join("caylibs/bin");
    if cayrt_path.exists() {
        lib_paths.push(cayrt_path);
    }

    // 构建 clang 命令
    let mut cmd = process::Command::new(clang_exe);
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

    // 使用 lld 链接器（仅在非内置clang时使用，内置clang需要确保lld在PATH中）
    // 检测是否使用内置clang
    let is_bundled_clang = clang_exe.to_string_lossy().contains("llvm-minimal");
    
    if !is_bundled_clang {
        // 系统clang可以使用 -fuse-ld=lld
        cmd.arg("-fuse-ld=lld");
    }
    // 内置clang使用默认链接器（它会自动找到同目录下的lld-link）

    // 链接 Cavvy 运行时库（所有平台都需要）
    cmd.arg("-lcayrt");

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
            print_tool_error("clang", &format!("执行失败: {}", e), Some("请检查 clang 是否正确安装"));
            process::exit(1);
        });

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        
        // 使用源映射重新映射错误信息
        let remapped_error = if !source_map.mappings.is_empty() {
            let mapped = remap_clang_error(&error_msg, &source_map, &input_file);
            add_clang_error_notice(&mapped)
        } else {
            error_msg.to_string()
        };
        
        print_tool_error(
            "clang",
            &format!("编译失败 (exit code: {})", output.status.code().unwrap_or(-1)),
            Some(&remapped_error)
        );
        process::exit(1);
    }

    if !output.stderr.is_empty() {
        let warn_msg = String::from_utf8_lossy(&output.stderr);
        // 使用源映射重新映射警告信息
        let remapped_warning = if !source_map.mappings.is_empty() {
            remap_clang_error(&warn_msg, &source_map, &input_file)
        } else {
            warn_msg.to_string()
        };
        print_warning(&remapped_warning);
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
        println!("    ir2exe --pgo-use app.profdata [其他选项] input.ll {}", 
            if cfg!(target_os = "windows") { "output.exe" } else { "output" });
    }

    print_completion_message(output_file, options);
}

/// 使用 llc + lld 编译 IR 到 EXE (跨平台)
/// 根据目标平台自动选择正确的链接器格式
fn compile_with_llc_lld(
    input_file: &str,
    output_file: &str,
    options: &CompileOptions,
    _source_map: &IRSourceMap,
    llc_exe: &PathBuf,
    lld_exe: &PathBuf,
) {
    use std::fs;
    
    // 设置库路径
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    
    // 生成临时目标文件路径
    let obj_file = format!("{}.obj", input_file);
    
    // 步骤1: 使用 llc 将 IR 编译为目标文件
    println!("  [1] IR → OBJ (llc)...");
    let mut llc_cmd = process::Command::new(llc_exe);
    llc_cmd.arg("-filetype=obj")
        .arg("-o").arg(&obj_file)
        .arg(&input_file);
    
    // 优化级别
    let opt_level = match options.optimization.as_str() {
        "-O0" => "-O=0",
        "-O1" => "-O=1",
        "-O2" => "-O=2",
        "-O3" => "-O=3",
        "-Os" => "-O=2",  // llc 没有 Os，使用 O2
        "-Oz" => "-O=2",  // llc 没有 Oz，使用 O2
        _ => "-O=2",
    };
    llc_cmd.arg(opt_level);
    
    // 目标平台
    let is_windows = options.target.contains("windows") || options.target.contains("mingw");
    let is_darwin = options.target.contains("darwin") || options.target.contains("macos");
    let is_wasm = options.target.contains("wasm");
    
    if is_windows {
        llc_cmd.arg("-mtriple=x86_64-w64-mingw32");
    } else if is_darwin {
        llc_cmd.arg("-mtriple=x86_64-apple-darwin");
    } else if is_wasm {
        llc_cmd.arg("-mtriple=wasm32-unknown-unknown");
    }
    
    // CPU 指令集
    if let Some(ref march) = options.march {
        llc_cmd.arg(format!("-mcpu={}", march));
    }
    
    let llc_output = llc_cmd.output()
        .unwrap_or_else(|e| {
            print_tool_error("llc", &format!("执行失败: {}", e), Some("请检查 llc 是否正确安装"));
            process::exit(1);
        });
    
    if !llc_output.status.success() {
        let error_msg = String::from_utf8_lossy(&llc_output.stderr);
        print_tool_error(
            "llc",
            &format!("编译失败 (exit code: {})", llc_output.status.code().unwrap_or(-1)),
            Some(&error_msg)
        );
        let _ = fs::remove_file(&obj_file);
        process::exit(1);
    }
    
    // 步骤2: 使用对应平台的 lld 链接目标文件
    let linker_name = get_lld_linker_name(&options.target);
    println!("  [2] OBJ → EXE ({})...", linker_name);
    let mut lld_cmd = process::Command::new(lld_exe);
    
    if is_windows {
        // Windows/MinGW: 使用 GNU ld 风格参数 (ld.lld)
        lld_cmd.arg("-m").arg("i386pep");  // PE+ 格式 (64-bit Windows)
        lld_cmd.arg("-o").arg(&output_file);
        
        // 添加启动文件 - 使用 crt1.o (控制台程序) 而不是 crt2.o (GUI程序)
        let crt1 = exe_dir.join("lib/mingw64/x86_64-w64-mingw32/lib/crt1.o");
        let crtbegin = exe_dir.join("lib/mingw64/lib/gcc/x86_64-w64-mingw32/15.2.0/crtbegin.o");
        let crtend = exe_dir.join("lib/mingw64/lib/gcc/x86_64-w64-mingw32/15.2.0/crtend.o");
        
        if crt1.exists() {
            lld_cmd.arg(&crt1);
        }
        if crtbegin.exists() {
            lld_cmd.arg(&crtbegin);
        }
        
        // 库路径
        let lib_paths = vec![
            exe_dir.join("lib/mingw64/x86_64-w64-mingw32/lib"),
            exe_dir.join("lib/mingw64/lib"),
            exe_dir.join("lib/mingw64/lib/gcc/x86_64-w64-mingw32/15.2.0"),
        ];
        
        for lib_path in &lib_paths {
            if lib_path.exists() {
                lld_cmd.arg("-L").arg(lib_path);
            }
        }

        // Cavvy 运行时库路径
        let cayrt_path = exe_dir.join("caylibs/bin");
        if cayrt_path.exists() {
            lld_cmd.arg("-L").arg(&cayrt_path);
        }
        
        // 额外库路径
        for path in &options.extra_lib_paths {
            lld_cmd.arg("-L").arg(path);
        }
        
        // 输入目标文件
        lld_cmd.arg(&obj_file);
        
        // Cavvy 运行时库
        lld_cmd.arg("-lcayrt");

        // 默认库 - 按照 MinGW 的标准顺序
        lld_cmd.arg("-lmingw32")
            .arg("-lmingwex")
            .arg("-lmsvcrt")
            .arg("-lkernel32")
            .arg("-ladvapi32")
            .arg("-lgcc")
            .arg("-lgcc_eh")
            .arg("-lmoldname")
            .arg("-lmingwex")
            .arg("-lmsvcrt")
            .arg("-lkernel32");
        
        // 额外库
        for lib in &options.extra_libs {
            lld_cmd.arg(format!("-l{}", lib));
        }
        
        // 结束文件
        if crtend.exists() {
            lld_cmd.arg(&crtend);
        }
        
        if options.static_link {
            lld_cmd.arg("-static");
        }
        if options.debug {
            lld_cmd.arg("-g");
        }
    } else if is_darwin {
        // macOS: 使用 ld64 风格参数
        lld_cmd.arg("-o").arg(&output_file);
        lld_cmd.arg(&obj_file);
        lld_cmd.arg("-arch").arg("x86_64");
        lld_cmd.arg("-platform_version").arg("macos").arg("11.0").arg("11.0");
        lld_cmd.arg("-lSystem");
        
        // 额外库路径
        for path in &options.extra_lib_paths {
            lld_cmd.arg("-L").arg(path);
        }
        
        // 额外库
        for lib in &options.extra_libs {
            lld_cmd.arg(format!("-l{}", lib));
        }
        
        if options.static_link {
            lld_cmd.arg("-static");
        }
        if options.debug {
            lld_cmd.arg("-g");
        }
    } else if is_wasm {
        // WebAssembly: 使用 wasm-ld 风格参数
        lld_cmd.arg("-o").arg(&output_file);
        lld_cmd.arg(&obj_file);
        lld_cmd.arg("--no-entry");
        lld_cmd.arg("--export-dynamic");
        
        // 额外库路径
        for path in &options.extra_lib_paths {
            lld_cmd.arg("-L").arg(path);
        }
        
        // 额外库
        for lib in &options.extra_libs {
            lld_cmd.arg(format!("-l{}", lib));
        }
    } else {
        // Linux/Unix: 使用 GNU ld 风格参数
        lld_cmd.arg("-o").arg(&output_file);
        lld_cmd.arg(&obj_file);
        
        // 额外库路径
        for path in &options.extra_lib_paths {
            lld_cmd.arg("-L").arg(path);
        }
        
        // 默认库
        lld_cmd.arg("-lc");  // C 标准库
        
        // 额外库
        for lib in &options.extra_libs {
            lld_cmd.arg(format!("-l{}", lib));
        }
        
        if options.static_link {
            lld_cmd.arg("-static");
        }
        if options.debug {
            lld_cmd.arg("-g");
        }
    }
    
    // 其他链接器标志
    for flag in &options.extra_ldflags {
        lld_cmd.arg(flag);
    }
    
    let lld_output = lld_cmd.output()
        .unwrap_or_else(|e| {
            print_tool_error(linker_name, &format!("执行失败: {}", e), Some("请检查链接器是否正确安装"));
            let _ = fs::remove_file(&obj_file);
            process::exit(1);
        });
    
    // 清理临时目标文件
    let _ = fs::remove_file(&obj_file);
    
    if !lld_output.status.success() {
        let error_msg = String::from_utf8_lossy(&lld_output.stderr);
        print_tool_error(
            linker_name,
            &format!("链接失败 (exit code: {})", lld_output.status.code().unwrap_or(-1)),
            Some(&error_msg)
        );
        process::exit(1);
    }
    
    let exe_size = std::fs::metadata(&output_file)
        .map(|m| m.len() as f64 / 1024.0)
        .unwrap_or(0.0);
    println!("  [+] 生成: {} ({:.1} KB)", output_file, exe_size);
    
    print_completion_message(output_file, options);
}

/// 打印完成消息
fn print_completion_message(output_file: &str, options: &CompileOptions) {
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
