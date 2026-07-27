//! IR 到 EXE 编译库模块
//!
//! 该模块提供将 LLVM IR 编译为可执行文件的功能，
//! 被 cayc 和 ir2exe 共享使用。

use crate::embedded_llc::{self, EmbeddedLlcOptions};
use crate::miette_diagnostic::{print_miette_error, print_tool_error, print_warning};
use std::env;
use std::path::{Component, Path, PathBuf};
use std::process;

/// 源位置信息
#[derive(Debug, Clone)]
pub struct SourcePosition {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

/// IR源映射表 - 从IR行号到源位置的映射
#[derive(Debug, Clone, Default)]
pub struct IRSourceMap {
    pub mappings: std::collections::HashMap<usize, SourcePosition>,
}

impl IRSourceMap {
    pub fn new() -> Self {
        Self {
            mappings: std::collections::HashMap::new(),
        }
    }

    pub fn add_mapping(&mut self, ir_line: usize, file: String, line: usize, column: usize) {
        self.mappings
            .insert(ir_line, SourcePosition { file, line, column });
    }

    pub fn get_source_position(&self, ir_line: usize) -> Option<&SourcePosition> {
        // 首先尝试精确匹配
        if let Some(pos) = self.mappings.get(&ir_line) {
            return Some(pos);
        }

        // 如果没有精确匹配，查找最近的映射（小于或等于给定行号的最大映射）
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
pub fn parse_source_map_from_ir(ir_content: &str) -> IRSourceMap {
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

                    if let (Ok(line_num), Ok(col_num)) =
                        (line_str.parse::<usize>(), col_str.parse::<usize>())
                    {
                        source_map.add_mapping(current_line, file, line_num, col_num);
                    }
                }
            }
        }
    }

    source_map
}

/// 从 IR 内容中解析链接库元数据（`cayc`/`cay-run` 共用）
///
/// 格式: `; !link "libname"`（用户库）或 `; !link <libname>`（系统库）。
/// 由 `codegen/generator.rs` 在生成 IR 时写入（对应 `#link` 声明与 `#include_c` 自动链接）。
pub fn parse_link_libraries_from_ir(ir_content: &str) -> Vec<String> {
    let mut libraries = Vec::new();

    for line in ir_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("; !link ") {
            let lib_part = &trimmed[8..]; // 跳过 "; !link "
            let lib_name = if lib_part.starts_with('"') && lib_part.ends_with('"') {
                // 用户库: "libname"
                &lib_part[1..lib_part.len() - 1]
            } else if lib_part.starts_with('<') && lib_part.ends_with('>') {
                // 系统库: <libname>
                &lib_part[1..lib_part.len() - 1]
            } else {
                lib_part
            };
            if !lib_name.is_empty() && !libraries.contains(&lib_name.to_string()) {
                libraries.push(lib_name.to_string());
            }
        }
    }

    libraries
}

/// 解析clang错误信息中的行号
fn parse_clang_error_line(error_msg: &str) -> Option<usize> {
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
    let content = std::fs::read_to_string(file_path).ok()?;
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
pub fn remap_clang_error(error_msg: &str, source_map: &IRSourceMap, _ir_file_name: &str) -> String {
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
                    result.push_str(&format!(
                        "\n  at {}:{}:{}\n",
                        source_pos.file, source_pos.line, source_pos.column
                    ));

                    // 读取并显示源代码上下文
                    if let Some(context) = read_source_context(&source_pos.file, source_pos.line, 2)
                    {
                        result.push_str(&context);
                    }

                    last_source_file = Some(source_pos.file.clone());
                    last_source_line = Some(source_pos.line);
                }

                // 修改错误行，指向源文件
                let modified_line = if line.contains("error:") {
                    if let Some(error_pos) = line.rfind("error:") {
                        let error_msg_part = &line[error_pos + 6..];
                        format!("\n  error: {}", error_msg_part)
                    } else {
                        format!("\n  {}", line)
                    }
                } else if line.contains("warning:") {
                    if let Some(warning_pos) = line.rfind("warning:") {
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

        // 对于代码指示行（如 "    |     ^"），跳过
        if line.trim().starts_with('|') || line.trim().starts_with('^') {
            continue;
        }

        // 其他行
        if !line.trim().is_empty() {
            result.push('\n');
            result.push_str(line);
        }
    }

    result.trim_end().to_string()
}

/// 添加Clang错误映射的说明信息
pub fn add_clang_error_notice(remapped_error: &str) -> String {
    format!(
        "{}\n\n  请注意，当 LLVM 编译执行失败时，其映射代码输出并非 Cavvy 错误报告系统的组成部分，且不属于 Cavvy 错误。此项功能仅用于协助您排查相关问题。\n  如果您遇到 LLVM编译执行失败的报错信息，请立即通过提交 Issue 的方式告知我们，以便我们及时修复该问题。\n  Issue 提交地址：https://github.com/cavvy-lang/cavvy/issues",
        remapped_error
    )
}

/// 规范化路径，去除 . 和 ..
pub fn normalize_path(path: &Path) -> PathBuf {
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

/// 根据平台获取 llvm-minimal 下的 clang 路径列表
#[cfg(target_os = "windows")]
pub fn get_bundled_clang_paths(exe_dir: &Path) -> Vec<PathBuf> {
    get_llvm_minimal_paths(exe_dir, "llvm-minimal/bin/clang.exe")
}

#[cfg(target_os = "linux")]
pub fn get_bundled_clang_paths(exe_dir: &Path) -> Vec<PathBuf> {
    get_llvm_minimal_paths(exe_dir, "llvm-minimal/bin-linux/clang")
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn get_bundled_clang_paths(exe_dir: &Path) -> Vec<PathBuf> {
    get_llvm_minimal_paths(exe_dir, "llvm-minimal/bin/clang")
}

/// 工具链类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolchainType {
    /// 使用 clang 一站式编译
    Clang,
    /// 使用 llc + lld-link 分步编译
    LlcLld,
}

/// 查找 clang 可执行文件
///
/// 优先使用随编译器分发的捆绑工具（llvm-minimal/，构建时复制到可执行文件旁），
/// 保证工具链版本锁定、避免 PATH 劫持；PATH 中的 clang 仅作为兜底。
pub fn find_clang() -> Result<PathBuf, String> {
    // 1. 优先尝试编译器所在目录或项目根目录下的 llvm-minimal（hermetic）
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            for path in get_bundled_clang_paths(exe_dir) {
                if path.exists() {
                    return Ok(path);
                }
            }
        }
    }

    // 2. 兜底：系统 PATH 中的 clang
    if let Ok(output) = process::Command::new("clang").arg("--version").output() {
        if output.status.success() {
            return Ok(PathBuf::from("clang"));
        }
    }

    // 3. 都找不到，返回错误
    Err("找不到 clang 编译器。请将 llvm-minimal 放在编译器同目录下（推荐），或确保 clang 已安装并在 PATH 中。".to_string())
}

/// 查找 llc 可执行文件
///
/// 优先使用捆绑的 llvm-minimal 工具，PATH 中的 llc 仅作为兜底。
pub fn find_llc() -> Result<PathBuf, String> {
    // 1. 优先尝试编译器所在目录或项目根目录下的 llvm-minimal（hermetic）
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

    // 2. 兜底：系统 PATH 中的 llc
    if let Ok(output) = process::Command::new("llc").arg("--version").output() {
        if output.status.success() {
            return Ok(PathBuf::from("llc"));
        }
    }

    // 3. 都找不到，返回错误
    Err("找不到 llc (LLVM IR 编译器)。请将 llvm-minimal 放在编译器同目录下（推荐），或确保 LLVM 已安装并在 PATH 中。".to_string())
}

/// 获取可能的 llvm-minimal 路径列表
pub fn get_llvm_minimal_paths(exe_dir: &Path, sub_path: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 1. 编译器所在目录
    paths.push(exe_dir.join(sub_path));

    // 2. 尝试向上查找项目根目录
    if let Some(parent) = exe_dir.parent() {
        if let Some(grandparent) = parent.parent() {
            paths.push(grandparent.join(sub_path));
        }
    }

    paths
}

/// 根据目标平台获取对应的 lld 链接器名称
pub fn get_lld_linker_name(target: &str) -> &'static str {
    if target.contains("msvc") {
        // MSVC 目标使用 lld-link (COFF 链接器) 配合 MSVC 风格参数
        "lld-link"
    } else if target.contains("windows") || target.contains("mingw") {
        // MinGW 目标使用 ld.lld (ELF 链接器) 配合 GNU 风格参数
        // ld.lld 支持 -flavor gnu 来使用 GNU ld 风格的参数
        "ld.lld"
    } else if target.contains("darwin") || target.contains("macos") {
        "ld64.lld"
    } else if target.contains("wasm") || target.contains("emscripten") {
        "wasm-ld"
    } else {
        "ld.lld"
    }
}

/// 查找指定平台的 lld 链接器
pub fn find_lld_for_target(target: &str) -> Result<PathBuf, String> {
    let linker_name = get_lld_linker_name(target);
    find_lld_linker(linker_name)
}

/// 查找指定名称的 lld 链接器
///
/// 优先使用捆绑的 llvm-minimal 工具，PATH 中的链接器仅作为兜底。
pub fn find_lld_linker(linker_name: &str) -> Result<PathBuf, String> {
    // 1. 优先尝试编译器所在目录或项目根目录下的 llvm-minimal（hermetic）
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

    // 2. 兜底：系统 PATH
    let test_arg = if linker_name == "lld-link" {
        "/?"
    } else {
        "--version"
    };
    if let Ok(output) = process::Command::new(linker_name).arg(test_arg).output() {
        if output.status.code().is_some() {
            return Ok(PathBuf::from(linker_name));
        }
    }

    // 3. 都找不到，返回错误
    Err(format!(
        "找不到 {} (LLVM 链接器)。请将 llvm-minimal 放在编译器同目录下（推荐），或确保 LLVM 已安装并在 PATH 中。",
        linker_name
    ))
}

/// 自动编译 Linux 版本的 Cavvy 运行时库（libcayrt-linux.a）。
///
/// 仅当目标平台为 Linux、运行时库缺失且存在 build.sh 时触发。
/// 失败时返回明确错误（编译器宁可 noisy 报错，不可带着缺失的运行时库继续链接）：
/// - 环境无 bash：提示安装 bash 或手动执行 build.sh；
/// - build.sh 非零退出：附带回显的 stderr。
fn auto_build_linux_runtime(cayrt_path: &Path, target: &str) -> Result<(), String> {
    if !target.contains("linux") {
        return Ok(());
    }
    let linux_lib = cayrt_path.join("libcayrt-linux.a");
    if linux_lib.exists() {
        return Ok(());
    }
    let build_script = cayrt_path.join("build.sh");
    if !build_script.exists() {
        return Ok(());
    }

    eprintln!("  [I] 未找到 Linux 运行时库，正在自动编译...");
    let output = process::Command::new("bash")
        .arg(&build_script)
        .arg("linux")
        .current_dir(cayrt_path)
        .output()
        .map_err(|e| {
            format!(
                "无法执行 {}（需要 bash）: {}。请安装 bash，或手动在 {} 下运行 `bash build.sh linux` 生成 libcayrt-linux.a。",
                build_script.display(),
                e,
                cayrt_path.display()
            )
        })?;

    if output.status.success() {
        eprintln!("  [+] Linux 运行时库编译成功");
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "自动编译 Linux 运行时库失败 ({}): {}",
            build_script.display(),
            stderr.trim()
        ))
    }
}

/// 检测可用的工具链
/// 默认优先使用 llc + lld 模式以获得更好的编译速度和可控性
pub fn detect_toolchain(target: &str) -> Result<(ToolchainType, PathBuf, Option<PathBuf>), String> {
    // 默认优先使用 llc + lld 模式（编译速度更快，行为更可控）
    if let Ok(llc_path) = find_llc() {
        if let Ok(lld_path) = find_lld_for_target(target) {
            return Ok((ToolchainType::LlcLld, llc_path, Some(lld_path)));
        }
    }

    // 如果 llc + lld 不可用，回退到 clang
    if let Ok(clang_path) = find_clang() {
        return Ok((ToolchainType::Clang, clang_path, None));
    }

    // 都找不到
    Err(format!(
        "找不到可用的编译工具链。请确保以下之一可用：\n\
         1. llc + {} (默认)\n\
         2. clang\n\
         或将 llvm-minimal 目录放在编译器同目录下。",
        get_lld_linker_name(target)
    ))
}

/// IR 到 EXE 编译选项
#[derive(Debug, Clone)]
pub struct Ir2ExeOptions {
    pub optimization: String,
    pub debug: bool,
    pub extra_lib_paths: Vec<String>,
    pub extra_libs: Vec<String>,
    pub extra_ldflags: Vec<String>,
    pub extra_cflags: Vec<String>,
    pub target: String,
    pub static_link: bool,
    pub position_independent: bool,
    pub lto: bool,
    pub lto_thin: bool,
    pub march: Option<String>,
    pub mtune: Option<String>,
    pub mcpu: Option<String>,
    pub msse: Option<String>,
    pub mavx: Option<String>,
    pub mneon: bool,
    pub pgo_gen: bool,
    pub pgo_use: Option<String>,
    pub pgo_cs: bool,
    pub fno_exceptions: bool,
    pub fno_rtti: bool,
    pub fomit_frame_pointer: bool,
    pub funroll_loops: bool,
    pub fvectorize: bool,
    pub fslp_vectorize: bool,
    /// 强制使用 clang 工具链
    pub use_clang: bool,
    /// 强制使用 llc+lld 工具链
    pub use_llc_lld: bool,
    /// 实验性: 使用 llvm-sys 内嵌 llc 以提高编译速度
    pub use_embedded_llc: bool,
}

impl Default for Ir2ExeOptions {
    fn default() -> Self {
        Self {
            optimization: "-O2".to_string(),
            debug: false,
            extra_lib_paths: Vec::new(),
            extra_libs: Vec::new(),
            extra_ldflags: Vec::new(),
            extra_cflags: Vec::new(),
            target: get_default_target(),
            static_link: false,
            // Windows 上默认启用 PIC 以避免重定位截断错误
            position_independent: cfg!(target_os = "windows"),
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
        }
    }
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

/// 编译 IR 到 EXE 的结果
#[derive(Debug)]
pub struct CompileResult {
    pub success: bool,
    pub output_file: String,
    pub exe_size_kb: f64,
    pub messages: Vec<String>,
}

/// 将 IR 编译为 EXE（主入口函数）
///
/// # Arguments
/// * `input_file` - 输入 IR 文件路径
/// * `output_file` - 输出可执行文件路径
/// * `options` - 编译选项
/// * `source_map` - 可选的源映射表（用于错误映射）
///
/// # Returns
/// 编译成功返回 Ok(CompileResult)，失败返回 Err(String)
pub fn compile_ir_to_exe(
    input_file: &str,
    output_file: &str,
    options: &Ir2ExeOptions,
    source_map: Option<&IRSourceMap>,
) -> Result<CompileResult, String> {
    // 读取IR文件内容
    let ir_content = match std::fs::read_to_string(input_file) {
        Ok(content) => content,
        Err(e) => {
            return Err(format!("无法读取IR文件 '{}': {}", input_file, e));
        }
    };

    // 如果没有提供源映射，尝试从IR内容解析
    let parsed_source_map = if source_map.is_none() {
        parse_source_map_from_ir(&ir_content)
    } else {
        IRSourceMap::new()
    };

    let effective_source_map = source_map.unwrap_or(&parsed_source_map);

    // 检测工具链并编译
    if options.use_embedded_llc {
        // 实验性: 使用内嵌 llc (llvm-sys)
        compile_with_embedded_llc(input_file, output_file, options, effective_source_map)
    } else if options.use_clang {
        // 强制使用 clang
        let clang_path = find_clang()?;
        compile_with_clang(
            input_file,
            output_file,
            options,
            effective_source_map,
            &clang_path,
        )
    } else if options.use_llc_lld {
        // 强制使用 llc + 对应平台的 lld
        let llc_path = find_llc()?;
        let lld_path = find_lld_for_target(&options.target)?;
        compile_with_llc_lld(
            input_file,
            output_file,
            options,
            effective_source_map,
            &llc_path,
            &lld_path,
        )
    } else {
        // 自动检测工具链
        let (toolchain_type, tool_path, tool_path2) = detect_toolchain(&options.target)?;

        match toolchain_type {
            ToolchainType::Clang => compile_with_clang(
                input_file,
                output_file,
                options,
                effective_source_map,
                &tool_path,
            ),
            ToolchainType::LlcLld => {
                let lld_path = tool_path2.ok_or("lld path should exist")?;
                compile_with_llc_lld(
                    input_file,
                    output_file,
                    options,
                    effective_source_map,
                    &tool_path,
                    &lld_path,
                )
            }
        }
    }
}

/// 使用 clang 编译 IR 到 EXE
fn compile_with_clang(
    input_file: &str,
    output_file: &str,
    options: &Ir2ExeOptions,
    source_map: &IRSourceMap,
    clang_exe: &PathBuf,
) -> Result<CompileResult, String> {
    // 设置库路径
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    // 根据目标平台选择库路径
    let mut lib_paths: Vec<PathBuf> =
        if options.target.contains("windows") || options.target.contains("mingw") {
            vec![
                exe_dir.join("lib/mingw64/x86_64-w64-mingw32/lib"),
                exe_dir.join("lib/mingw64/lib"),
                exe_dir.join("lib/mingw64/lib/gcc/x86_64-w64-mingw32/15.2.0"),
            ]
        } else {
            vec![]
        };

    // 添加 Cavvy 运行时库路径
    let cayrt_path = exe_dir.join("caylibs/bin");
    if cayrt_path.exists() {
        lib_paths.push(cayrt_path.clone());

        // 检查是否需要自动编译 Linux 版本的运行时库
        auto_build_linux_runtime(&cayrt_path, &options.target)?;
    }

    // 构建 clang 命令
    let mut cmd = process::Command::new(clang_exe);
    cmd.arg(input_file)
        .arg("-o")
        .arg(output_file)
        .arg("-target")
        .arg(&options.target)
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
            "avx" => {
                cmd.arg("-mavx");
            }
            "avx2" => {
                cmd.arg("-mavx2");
            }
            "avx512f" => {
                cmd.arg("-mavx512f");
            }
            "avx512" => {
                cmd.arg("-mavx512f");
            }
            _ => {
                cmd.arg(format!("-m{}", mavx));
            }
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

    // 检测是否使用内置clang
    let is_bundled_clang = clang_exe.to_string_lossy().contains("llvm-minimal");

    if !is_bundled_clang {
        // 系统clang可以使用 -fuse-ld=lld
        cmd.arg("-fuse-ld=lld");
    }

    // 根据目标平台选择正确的 Cavvy 运行时库
    let cayrt_lib_name = if options.target.contains("linux") {
        "cayrt-linux"
    } else {
        "cayrt"
    };
    cmd.arg(format!("-l{}", cayrt_lib_name));

    // 根据目标平台选择默认库
    if options.target.contains("windows") || options.target.contains("mingw") {
        cmd.arg("-lkernel32").arg("-lmsvcrt").arg("-ladvapi32");
    } else if options.target.contains("linux") {
        cmd.arg("-lc").arg("-lm").arg("-lpthread");
    } else if options.target.contains("darwin") {
        cmd.arg("-lc").arg("-lm");
    } else {
        cmd.arg("-lc").arg("-lm");
    }

    // 额外库
    for lib in &options.extra_libs {
        cmd.arg(format!("-l{}", lib));
    }

    // 额外的链接器标志
    for flag in &options.extra_ldflags {
        cmd.arg(flag);
    }

    let output = cmd
        .output()
        .map_err(|e| format!("执行 clang 失败: {}", e))?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);

        // 使用源映射重新映射错误信息
        let remapped_error = if !source_map.mappings.is_empty() {
            let mapped = remap_clang_error(&error_msg, source_map, input_file);
            add_clang_error_notice(&mapped)
        } else {
            error_msg.to_string()
        };

        return Err(format!(
            "clang 编译失败 (exit code: {}): {}",
            output.status.code().unwrap_or(-1),
            remapped_error
        ));
    }

    if !output.stderr.is_empty() {
        let warn_msg = String::from_utf8_lossy(&output.stderr);
        // 使用源映射重新映射警告信息
        let remapped_warning = if !source_map.mappings.is_empty() {
            remap_clang_error(&warn_msg, source_map, input_file)
        } else {
            warn_msg.to_string()
        };
        eprintln!("  [W] {}", remapped_warning);
    }

    let exe_size = std::fs::metadata(output_file)
        .map(|m| m.len() as f64 / 1024.0)
        .unwrap_or(0.0);

    Ok(CompileResult {
        success: true,
        output_file: output_file.to_string(),
        exe_size_kb: exe_size,
        messages: vec![format!("生成: {} ({:.1} KB)", output_file, exe_size)],
    })
}

/// 使用嵌入式 llc (llvm-sys) 编译 IR 到 EXE
///
/// 时间复杂度: O(n) 其中 n 为 IR 代码行数
/// 空间复杂度: O(m) 其中 m 为目标代码大小
fn compile_with_embedded_llc(
    input_file: &str,
    output_file: &str,
    options: &Ir2ExeOptions,
    _source_map: &IRSourceMap,
) -> Result<CompileResult, String> {
    // 检查嵌入式 LLVM 是否可用
    if !embedded_llc::is_embedded_llvm_available() {
        return Err("嵌入式 LLVM 支持未启用。".to_string()
            + "请使用 --features embedded-llvm 重新编译 Cavvy，"
            + "或使用外部 llc 工具链。");
    }

    // 读取 IR 文件内容
    let ir_content = std::fs::read_to_string(input_file)
        .map_err(|e| format!("无法读取 IR 文件 '{}': {}", input_file, e))?;

    // 设置库路径
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    // 检查并自动构建 Cavvy 运行时库（如果需要）
    let cayrt_path = exe_dir.join("caylibs/bin");
    if cayrt_path.exists() {
        // 检查是否需要自动编译 Linux 版本的运行时库
        auto_build_linux_runtime(&cayrt_path, &options.target)?;
    }

    // 生成临时目标文件路径
    let obj_file = format!("{}.obj", input_file);

    // 构建嵌入式 llc 选项
    let llc_opts = EmbeddedLlcOptions {
        opt_level: embedded_llc::parse_opt_level(&options.optimization),
        target_triple: options.target.clone(),
        cpu: options.march.clone(),
        features: None,
        position_independent: options.position_independent,
    };

    // 步骤1: 使用嵌入式 llc 将 IR 编译为目标文件
    eprintln!("  [I] 使用嵌入式 llc 编译...");
    embedded_llc::compile_ir_to_object(&ir_content, Path::new(&obj_file), &llc_opts)
        .map_err(|e| format!("嵌入式 llc 编译失败: {}", e))?;

    // 步骤2: 使用链接器链接目标文件
    // 注意: 嵌入式 llc 生成的是 COFF 格式的目标文件 (Windows/MinGW 目标)
    let is_windows = options.target.contains("windows") || options.target.contains("mingw");
    let is_darwin = options.target.contains("darwin") || options.target.contains("macos");
    let is_wasm = options.target.contains("wasm");
    let is_msvc_target = options.target.contains("msvc");

    // 在 Windows 上，使用 ld.lld 以正确处理 COFF 格式的 MinGW 库
    let (mut lld_cmd, linker_name) = if is_windows {
        if is_msvc_target {
            // MSVC 目标使用 lld-link
            let path = find_lld_linker("lld-link")?;
            eprintln!("  [I] 使用 LLVM 链接器: lld-link");
            (process::Command::new(&path), "lld-link")
        } else {
            // MinGW 目标使用 ld.lld (GNU 风格)
            let path = find_lld_linker("ld.lld")?;
            eprintln!("  [I] 使用 LLVM 链接器: ld.lld");
            (process::Command::new(&path), "ld.lld")
        }
    } else {
        // 其他目标使用对应平台的 lld
        let path = find_lld_for_target(&options.target)?;
        let name = get_lld_linker_name(&options.target);
        (process::Command::new(&path), name)
    };

    // 根据平台设置链接器参数
    if is_windows {
        if is_msvc_target {
            // MSVC 目标: 使用纯 MSVC 风格参数 (lld-link)
            lld_cmd.arg(format!("/OUT:{}", output_file));
            lld_cmd.arg(&obj_file);

            // Cavvy 运行时库路径
            let cayrt_path = exe_dir.join("caylibs/bin");
            if cayrt_path.exists() {
                lld_cmd.arg(format!("/LIBPATH:{}", cayrt_path.display()));
            }

            // 默认库
            lld_cmd
                .arg("cayrt.lib")
                .arg("kernel32.lib")
                .arg("user32.lib")
                .arg("advapi32.lib")
                .arg("msvcrt.lib");

            // 额外库
            for lib in &options.extra_libs {
                lld_cmd.arg(format!("{}.lib", lib));
            }
        } else {
            // MinGW 目标: 使用 ld.lld (GNU ld 兼容模式)
            lld_cmd.arg("-m").arg("i386pep");
            lld_cmd.arg("-o").arg(output_file);

            // 输入目标文件
            lld_cmd.arg(&obj_file);

            // Cavvy 运行时库路径
            let cayrt_path = exe_dir.join("caylibs/bin");
            if cayrt_path.exists() {
                lld_cmd.arg("-L").arg(&cayrt_path);
            }

            // 添加本地 lib 路径 (MinGW 库)
            let local_lib_paths = vec![
                exe_dir.join("lib/mingw64/x86_64-w64-mingw32/lib"),
                exe_dir.join("lib/mingw64/lib"),
                exe_dir.join("lib/mingw64/lib/gcc/x86_64-w64-mingw32/15.2.0"),
            ];
            for lib_path in &local_lib_paths {
                if lib_path.exists() {
                    lld_cmd.arg("-L").arg(lib_path);
                }
            }

            // 额外库路径
            for path in &options.extra_lib_paths {
                lld_cmd.arg("-L").arg(path);
            }

            // 添加启动文件 (crt)
            let crt_paths = vec![
                exe_dir.join("lib/mingw64/x86_64-w64-mingw32/lib"),
                exe_dir.join("lib/mingw64/lib"),
            ];
            for crt_path in &crt_paths {
                if crt_path.exists() {
                    let crt2 = crt_path.join("crt2.o");
                    if crt2.exists() {
                        lld_cmd.arg(&crt2);
                        break;
                    }
                }
            }

            // Cavvy 运行时库
            lld_cmd.arg("-lcayrt");

            // 默认库
            lld_cmd
                .arg("-lmingw32")
                .arg("-lmingwex")
                .arg("-lmsvcrt")
                .arg("-lkernel32")
                .arg("-ladvapi32")
                .arg("-lgcc")
                .arg("-lgcc_eh");

            // 添加 crtend.o
            for crt_path in &crt_paths {
                if crt_path.exists() {
                    let crtend = crt_path.join("crtend.o");
                    if crtend.exists() {
                        lld_cmd.arg(&crtend);
                        break;
                    }
                }
            }

            // 额外库
            for lib in &options.extra_libs {
                lld_cmd.arg(format!("-l{}", lib));
            }

            if options.static_link {
                lld_cmd.arg("-static");
            }
        }
    } else if is_darwin {
        // macOS: 使用 ld64 风格参数
        lld_cmd.arg("-flavor").arg("darwin");
        lld_cmd.arg("-o").arg(output_file);
        lld_cmd.arg(&obj_file);
        lld_cmd.arg("-arch").arg("x86_64");
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
    } else {
        // Linux/Unix: 使用 GNU ld 风格参数
        lld_cmd.arg("-flavor").arg("gnu");
        lld_cmd.arg("-o").arg(output_file);

        // 添加标准库搜索路径 - ld.lld 不会自动搜索系统库路径
        // 时间复杂度: O(1) - 固定数量的路径
        let default_lib_paths = vec![
            "/usr/lib",
            "/usr/lib64",
            "/usr/local/lib",
            "/lib",
            "/lib64",
            "/lib/x86_64-linux-gnu",
            "/usr/lib/x86_64-linux-gnu",
        ];
        for lib_path in &default_lib_paths {
            if std::path::Path::new(lib_path).exists() {
                lld_cmd.arg("-L").arg(lib_path);
            }
        }

        // 额外库路径
        for path in &options.extra_lib_paths {
            lld_cmd.arg("-L").arg(path);
        }

        // Cavvy 运行时库路径
        let cayrt_path = exe_dir.join("caylibs/bin");
        if cayrt_path.exists() {
            lld_cmd.arg("-L").arg(&cayrt_path);
        }

        // 添加 C 运行时启动文件 - 这些文件提供 _start 入口点
        // _start 负责初始化栈、调用 main、处理程序退出
        // 没有这些文件，入口点会是 0x0，导致段错误
        let mut crt_files_added = false;
        for lib_path in &default_lib_paths {
            if std::path::Path::new(lib_path).exists() {
                // 尝试找到 Scrt1.o (PIE) 或 crt1.o (非 PIE)
                let scrt1 = std::path::Path::new(lib_path).join("Scrt1.o");
                let crt1 = std::path::Path::new(lib_path).join("crt1.o");
                let crti = std::path::Path::new(lib_path).join("crti.o");
                let crtn = std::path::Path::new(lib_path).join("crtn.o");

                // 如果已经添加过启动文件，跳过此路径（避免重复符号）
                if crt_files_added {
                    continue;
                }

                if scrt1.exists() {
                    lld_cmd.arg(&scrt1);
                    crt_files_added = true;
                } else if crt1.exists() {
                    lld_cmd.arg(&crt1);
                    crt_files_added = true;
                }

                // 只在找到主启动文件后，才添加 init/fini 文件
                if crt_files_added {
                    if crti.exists() {
                        lld_cmd.arg(&crti);
                    }
                    if crtn.exists() {
                        lld_cmd.arg(&crtn);
                    }
                }
            }
        }

        // 输入目标文件（放在启动文件之后）
        lld_cmd.arg(&obj_file);

        // 根据目标平台选择运行时库
        let cayrt_lib_name = if options.target.contains("linux") {
            "cayrt-linux"
        } else {
            "cayrt"
        };
        lld_cmd.arg(format!("-l{}", cayrt_lib_name));

        // 添加启动文件和默认库
        // 注意: ld.lld 需要显式链接这些库，不像 GNU ld 那样自动处理
        lld_cmd.arg("-lc");
        lld_cmd.arg("-lm");
        lld_cmd.arg("-ldl");
        lld_cmd.arg("-lpthread");

        // 额外库
        for lib in &options.extra_libs {
            lld_cmd.arg(format!("-l{}", lib));
        }

        // 添加动态链接器路径 - 必须设置否则生成的 ELF 没有 INTERP 段
        // 没有 INTERP 段，内核无法找到动态链接器，导致程序无法启动
        if !options.static_link {
            // 尝试找到系统动态链接器
            let interp_paths = [
                "/lib64/ld-linux-x86-64.so.2",
                "/usr/lib64/ld-linux-x86-64.so.2",
                "/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2",
                "/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2",
            ];
            for interp in &interp_paths {
                if std::path::Path::new(interp).exists() {
                    lld_cmd.arg("--dynamic-linker").arg(interp);
                    break;
                }
            }
        }

        if options.static_link {
            lld_cmd.arg("-static");
        }
    }

    // 其他链接器标志
    for flag in &options.extra_ldflags {
        lld_cmd.arg(flag);
    }

    let lld_output = lld_cmd.output().map_err(|e| {
        let _ = std::fs::remove_file(&obj_file);
        format!("执行 {} 失败: {}", linker_name, e)
    })?;

    if !lld_output.status.success() {
        // 保留目标文件以便调试
        // eprintln!("  [DEBUG] 链接失败，保留目标文件: {}", obj_file);
        let error_msg = String::from_utf8_lossy(&lld_output.stderr);
        return Err(format!(
            "{} 链接失败 (exit code: {}): {}",
            linker_name,
            lld_output.status.code().unwrap_or(-1),
            error_msg
        ));
    }

    let exe_size = std::fs::metadata(output_file)
        .map(|m| m.len() as f64 / 1024.0)
        .unwrap_or(0.0);

    Ok(CompileResult {
        success: true,
        output_file: output_file.to_string(),
        exe_size_kb: exe_size,
        messages: vec![format!(
            "生成: {} ({:.1} KB) [嵌入式 llc]",
            output_file, exe_size
        )],
    })
}

/// 使用 llc + lld 编译 IR 到 EXE
fn compile_with_llc_lld(
    input_file: &str,
    output_file: &str,
    options: &Ir2ExeOptions,
    source_map: &IRSourceMap,
    llc_exe: &PathBuf,
    lld_exe: &PathBuf,
) -> Result<CompileResult, String> {
    // 设置库路径
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    // 检查并自动构建 Cavvy 运行时库（如果需要）
    let cayrt_path = exe_dir.join("caylibs/bin");
    if cayrt_path.exists() {
        // 检查是否需要自动编译 Linux 版本的运行时库
        auto_build_linux_runtime(&cayrt_path, &options.target)?;
    }

    // 生成临时目标文件路径
    let obj_file = format!("{}.obj", input_file);

    // 步骤1: 使用 llc 将 IR 编译为目标文件
    let mut llc_cmd = process::Command::new(llc_exe);
    llc_cmd
        .arg("-filetype=obj")
        .arg("-o")
        .arg(&obj_file)
        .arg(input_file);

    // 优化级别
    let opt_level = match options.optimization.as_str() {
        "-O0" => "-O=0",
        "-O1" => "-O=1",
        "-O2" => "-O=2",
        "-O3" => "-O=3",
        "-Os" => "-O=2",
        "-Oz" => "-O=2",
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
    } else {
        // Linux/Unix: 必须显式设置目标三元组以覆盖 IR 文件中嵌入的目标
        // 否则 llc 会使用 IR 中的目标（可能是 Windows），导致目标文件格式不匹配
        llc_cmd.arg("-mtriple=x86_64-unknown-linux-gnu");
    }

    // CPU 指令集
    if let Some(ref march) = options.march {
        llc_cmd.arg(format!("-mcpu={}", march));
    }

    let llc_output = llc_cmd
        .output()
        .map_err(|e| format!("执行 llc 失败: {}", e))?;

    if !llc_output.status.success() {
        let error_msg = String::from_utf8_lossy(&llc_output.stderr);
        let remapped_error = if !source_map.mappings.is_empty() {
            let mapped = remap_clang_error(&error_msg, source_map, input_file);
            add_clang_error_notice(&mapped)
        } else {
            error_msg.to_string()
        };
        let _ = std::fs::remove_file(&obj_file);
        return Err(format!(
            "llc 编译失败 (exit code: {}): {}",
            llc_output.status.code().unwrap_or(-1),
            remapped_error
        ));
    }

    // 步骤2: 使用对应平台的 lld 链接目标文件
    let linker_name = get_lld_linker_name(&options.target);
    let mut lld_cmd = process::Command::new(lld_exe);

    if is_windows {
        // Windows/MinGW: 使用 ld.lld 的 GNU 风格参数
        lld_cmd.arg("-flavor").arg("gnu");
        lld_cmd.arg("-m").arg("i386pep");
        lld_cmd.arg("-o").arg(output_file);

        // 添加启动文件
        let crt2 = exe_dir.join("lib/mingw64/x86_64-w64-mingw32/lib/crt2.o");
        let crtbegin = exe_dir.join("lib/mingw64/lib/gcc/x86_64-w64-mingw32/15.2.0/crtbegin.o");
        let crtend = exe_dir.join("lib/mingw64/lib/gcc/x86_64-w64-mingw32/15.2.0/crtend.o");

        if crt2.exists() {
            lld_cmd.arg(&crt2);
        }
        if crtbegin.exists() {
            lld_cmd.arg(&crtbegin);
        }

        // 输入目标文件
        lld_cmd.arg(&obj_file);

        // Cavvy 运行时库路径
        let cayrt_path = exe_dir.join("caylibs/bin");
        if cayrt_path.exists() {
            lld_cmd.arg("-L").arg(&cayrt_path);
        }

        // 本地 MinGW 库路径
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

        // 额外库路径
        for path in &options.extra_lib_paths {
            lld_cmd.arg("-L").arg(path);
        }

        // Cavvy 运行时库
        lld_cmd.arg("-lcayrt");

        // 默认库
        lld_cmd
            .arg("-lmingw32")
            .arg("-lmingwex")
            .arg("-lmsvcrt")
            .arg("-lkernel32")
            .arg("-ladvapi32")
            .arg("-lgcc")
            .arg("-lgcc_eh");

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
        lld_cmd.arg("-o").arg(output_file);
        lld_cmd.arg(&obj_file);
        lld_cmd.arg("-arch").arg("x86_64");
        lld_cmd
            .arg("-platform_version")
            .arg("macos")
            .arg("11.0")
            .arg("11.0");
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
        // WebAssembly
        lld_cmd.arg("-o").arg(output_file);
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
        lld_cmd.arg("-flavor").arg("gnu");
        lld_cmd.arg("-o").arg(output_file);

        // 添加标准库搜索路径 - ld.lld 不会自动搜索系统库路径
        // 时间复杂度: O(1) - 固定数量的路径
        let default_lib_paths = vec![
            "/usr/lib",
            "/usr/lib64",
            "/usr/local/lib",
            "/lib",
            "/lib64",
            "/lib/x86_64-linux-gnu",
            "/usr/lib/x86_64-linux-gnu",
        ];
        for lib_path in &default_lib_paths {
            if std::path::Path::new(lib_path).exists() {
                lld_cmd.arg("-L").arg(lib_path);
            }
        }

        // 额外库路径
        for path in &options.extra_lib_paths {
            lld_cmd.arg("-L").arg(path);
        }

        // Cavvy 运行时库路径
        let cayrt_path = exe_dir.join("caylibs/bin");
        if cayrt_path.exists() {
            lld_cmd.arg("-L").arg(&cayrt_path);
        }

        // 添加 C 运行时启动文件 - 这些文件提供 _start 入口点
        // _start 负责初始化栈、调用 main、处理程序退出
        // 没有这些文件，入口点会是 0x0，导致段错误
        let mut crt_files_added = false;
        for lib_path in &default_lib_paths {
            if std::path::Path::new(lib_path).exists() {
                // 尝试找到 Scrt1.o (PIE) 或 crt1.o (非 PIE)
                let scrt1 = std::path::Path::new(lib_path).join("Scrt1.o");
                let crt1 = std::path::Path::new(lib_path).join("crt1.o");
                let crti = std::path::Path::new(lib_path).join("crti.o");
                let crtn = std::path::Path::new(lib_path).join("crtn.o");

                // 如果已经添加过启动文件，跳过此路径（避免重复符号）
                if crt_files_added {
                    continue;
                }

                if scrt1.exists() {
                    lld_cmd.arg(&scrt1);
                    crt_files_added = true;
                } else if crt1.exists() {
                    lld_cmd.arg(&crt1);
                    crt_files_added = true;
                }

                // 只在找到主启动文件后，才添加 init/fini 文件
                if crt_files_added {
                    if crti.exists() {
                        lld_cmd.arg(&crti);
                    }
                    if crtn.exists() {
                        lld_cmd.arg(&crtn);
                    }
                }
            }
        }

        // 输入目标文件（放在启动文件之后）
        lld_cmd.arg(&obj_file);

        // 根据目标平台选择正确的 Cavvy 运行时库
        let cayrt_lib_name = if options.target.contains("linux") {
            "cayrt-linux"
        } else {
            "cayrt"
        };
        lld_cmd.arg(format!("-l{}", cayrt_lib_name));

        // 添加启动文件和默认库
        // 注意: ld.lld 需要显式链接这些库，不像 GNU ld 那样自动处理
        lld_cmd.arg("-lc");
        lld_cmd.arg("-lm");
        lld_cmd.arg("-ldl");
        lld_cmd.arg("-lpthread");

        // 额外库
        for lib in &options.extra_libs {
            lld_cmd.arg(format!("-l{}", lib));
        }

        // 添加动态链接器路径 - 必须设置否则生成的 ELF 没有 INTERP 段
        // 没有 INTERP 段，内核无法找到动态链接器，导致程序无法启动
        if !options.static_link {
            // 尝试找到系统动态链接器
            let interp_paths = [
                "/lib64/ld-linux-x86-64.so.2",
                "/usr/lib64/ld-linux-x86-64.so.2",
                "/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2",
                "/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2",
            ];
            for interp in &interp_paths {
                if std::path::Path::new(interp).exists() {
                    lld_cmd.arg("--dynamic-linker").arg(interp);
                    break;
                }
            }
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

    let lld_output = lld_cmd.output().map_err(|e| {
        let _ = std::fs::remove_file(&obj_file);
        format!("执行 {} 失败: {}", linker_name, e)
    })?;

    if !lld_output.status.success() {
        // 保留目标文件以便调试
        // eprintln!("  [DEBUG] 链接失败，保留目标文件: {}", obj_file);
        let error_msg = String::from_utf8_lossy(&lld_output.stderr);
        let remapped_error = if !source_map.mappings.is_empty() {
            let mapped = remap_clang_error(&error_msg, source_map, input_file);
            add_clang_error_notice(&mapped)
        } else {
            error_msg.to_string()
        };
        return Err(format!(
            "{} 链接失败 (exit code: {}): {}",
            linker_name,
            lld_output.status.code().unwrap_or(-1),
            remapped_error
        ));
    }

    // 清理临时目标文件
    let _ = std::fs::remove_file(&obj_file);

    let exe_size = std::fs::metadata(output_file)
        .map(|m| m.len() as f64 / 1024.0)
        .unwrap_or(0.0);

    Ok(CompileResult {
        success: true,
        output_file: output_file.to_string(),
        exe_size_kb: exe_size,
        messages: vec![format!("生成: {} ({:.1} KB)", output_file, exe_size)],
    })
}
