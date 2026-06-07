/// 链接器模块
/// 自动检测和匹配所需的链接库
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// 链接器配置
#[derive(Debug, Clone)]
pub struct LinkerConfig {
    /// 库搜索路径
    pub lib_paths: Vec<PathBuf>,
    /// 要链接的库
    pub libraries: HashSet<String>,
    /// 目标平台
    pub target: String,
}

impl Default for LinkerConfig {
    fn default() -> Self {
        Self {
            lib_paths: get_default_lib_paths(),
            libraries: HashSet::new(),
            target: get_default_target(),
        }
    }
}

/// 获取默认目标平台
fn get_default_target() -> String {
    if cfg!(target_os = "windows") {
        "x86_64-w64-mingw32".to_string()
    } else if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-gnu".to_string()
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin".to_string()
    } else {
        "x86_64-unknown-linux-gnu".to_string()
    }
}

/// 获取默认库搜索路径
fn get_default_lib_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // 当前目录
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd);
    }

    // 程序所在目录
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            paths.push(exe_dir.to_path_buf());
            // 检查lib子目录
            let lib_dir = exe_dir.join("lib");
            if lib_dir.exists() {
                paths.push(lib_dir);
            }
        }
    }

    // 平台特定的路径
    if cfg!(target_os = "windows") {
        // MinGW库路径
        if let Ok(mingw_path) = std::env::var("MINGW_PATH") {
            paths.push(PathBuf::from(mingw_path.clone()).join("lib"));
            paths.push(PathBuf::from(mingw_path).join("x86_64-w64-mingw32/lib"));
        }

        // 检查常见的MinGW安装位置
        let mingw_locations = [
            r"C:\mingw64\lib",
            r"C:\msys64\mingw64\lib",
            r"C:\Program Files\mingw-w64\x86_64-8.1.0-posix-seh-rt_v6-rev0\mingw64\lib",
        ];
        for loc in &mingw_locations {
            let path = PathBuf::from(loc);
            if path.exists() {
                paths.push(path);
            }
        }
    } else if cfg!(target_os = "linux") {
        paths.push(PathBuf::from("/usr/lib"));
        paths.push(PathBuf::from("/usr/local/lib"));
        paths.push(PathBuf::from("/lib"));
        paths.push(PathBuf::from("/lib/x86_64-linux-gnu"));
        paths.push(PathBuf::from("/usr/lib/x86_64-linux-gnu"));
    } else if cfg!(target_os = "macos") {
        paths.push(PathBuf::from("/usr/lib"));
        paths.push(PathBuf::from("/usr/local/lib"));
    }

    paths
}

/// 自动链接器
pub struct AutoLinker {
    pub config: LinkerConfig,
}

impl AutoLinker {
    /// 创建新的自动链接器
    pub fn new(config: LinkerConfig) -> Self {
        Self { config }
    }

    /// 从源代码分析需要的库
    pub fn analyze_source(&mut self, source: &str) {
        // 分析源代码中使用的功能，推断需要的库

        // 检查是否使用了Windows API
        if source.contains("MessageBox") || source.contains("GetLastError") {
            self.config.libraries.insert("user32".to_string());
            self.config.libraries.insert("kernel32".to_string());
        }

        // 检查是否使用了数学函数
        if source.contains("Math.")
            || source.contains("sqrt(")
            || source.contains("sin(")
            || source.contains("cos(")
        {
            self.config.libraries.insert("m".to_string());
        }

        // 检查是否使用了网络功能
        if source.contains("Socket") || source.contains("Network") {
            if cfg!(target_os = "windows") {
                self.config.libraries.insert("ws2_32".to_string());
            }
        }

        // 检查是否使用了线程
        if source.contains("Thread") || source.contains("pthread") {
            if cfg!(target_os = "windows") {
                // Windows线程支持在kernel32中
            } else {
                self.config.libraries.insert("pthread".to_string());
            }
        }
    }

    /// 从IR代码分析需要的库
    pub fn analyze_ir(&mut self, ir_code: &str) {
        // 分析IR中的外部函数声明

        // Windows API
        if ir_code.contains("MessageBox") || ir_code.contains("GetLastError") {
            self.config.libraries.insert("user32".to_string());
            self.config.libraries.insert("kernel32".to_string());
        }

        // 标准C库函数
        if ir_code.contains("@printf") || ir_code.contains("@scanf") || ir_code.contains("@malloc")
        {
            // 这些通常是自动链接的
        }

        // 数学库
        if ir_code.contains("@sqrt") || ir_code.contains("@sin") || ir_code.contains("@cos") {
            self.config.libraries.insert("m".to_string());
        }

        // 网络库
        if ir_code.contains("@socket") || ir_code.contains("@connect") {
            if cfg!(target_os = "windows") {
                self.config.libraries.insert("ws2_32".to_string());
            }
        }

        // 分析 extern 声明中的 C 库函数
        self.analyze_extern_declarations(ir_code);
    }

    /// 分析 extern 声明中的 C 库函数
    fn analyze_extern_declarations(&mut self, ir_code: &str) {
        // 常见的 C 标准库函数及其所属的库
        let c_stdlib_functions = [
            ("@printf", "c"),
            ("@scanf", "c"),
            ("@sprintf", "c"),
            ("@sscanf", "c"),
            ("@fprintf", "c"),
            ("@fscanf", "c"),
            ("@malloc", "c"),
            ("@calloc", "c"),
            ("@realloc", "c"),
            ("@free", "c"),
            ("@strlen", "c"),
            ("@strcpy", "c"),
            ("@strncpy", "c"),
            ("@strcat", "c"),
            ("@strncat", "c"),
            ("@strcmp", "c"),
            ("@strncmp", "c"),
            ("@strchr", "c"),
            ("@strstr", "c"),
            ("@memcpy", "c"),
            ("@memmove", "c"),
            ("@memset", "c"),
            ("@memcmp", "c"),
            ("@fopen", "c"),
            ("@fclose", "c"),
            ("@fread", "c"),
            ("@fwrite", "c"),
            ("@fgets", "c"),
            ("@fputs", "c"),
            ("@getchar", "c"),
            ("@putchar", "c"),
            ("@puts", "c"),
            ("@gets", "c"),
            ("@exit", "c"),
            ("@abort", "c"),
            ("@qsort", "c"),
            ("@bsearch", "c"),
            ("@time", "c"),
            ("@clock", "c"),
            ("@srand", "c"),
            ("@rand", "c"),
        ];

        let math_functions = [
            ("@sqrt", "m"),
            ("@sqrtf", "m"),
            ("@sin", "m"),
            ("@cos", "m"),
            ("@tan", "m"),
            ("@asin", "m"),
            ("@acos", "m"),
            ("@atan", "m"),
            ("@atan2", "m"),
            ("@exp", "m"),
            ("@log", "m"),
            ("@log10", "m"),
            ("@pow", "m"),
            ("@ceil", "m"),
            ("@floor", "m"),
            ("@fabs", "m"),
            ("@fmod", "m"),
            ("@round", "m"),
            ("@trunc", "m"),
        ];

        // 检查 C 标准库函数
        for (func, lib) in &c_stdlib_functions {
            if ir_code.contains(func) {
                self.config.libraries.insert(lib.to_string());
            }
        }

        // 检查数学库函数
        for (func, lib) in &math_functions {
            if ir_code.contains(func) {
                self.config.libraries.insert(lib.to_string());
            }
        }

        // Windows 特定函数
        if cfg!(target_os = "windows") {
            let windows_functions = [
                ("@MessageBox", "user32"),
                ("@GetLastError", "kernel32"),
                ("@Sleep", "kernel32"),
                ("@CreateThread", "kernel32"),
                ("@WaitForSingleObject", "kernel32"),
                ("@SetConsoleOutputCP", "kernel32"),
                ("@GetConsoleOutputCP", "kernel32"),
                ("@SetConsoleCP", "kernel32"),
                ("@GetConsoleCP", "kernel32"),
                ("@AllocConsole", "kernel32"),
                ("@FreeConsole", "kernel32"),
            ];

            for (func, lib) in &windows_functions {
                if ir_code.contains(func) {
                    self.config.libraries.insert(lib.to_string());
                }
            }
        }

        // POSIX / Linux 特定函数
        if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
            let posix_functions = [
                ("@pthread_create", "pthread"),
                ("@pthread_join", "pthread"),
                ("@pthread_mutex_lock", "pthread"),
                ("@dlopen", "dl"),
                ("@dlsym", "dl"),
                ("@dlclose", "dl"),
            ];

            for (func, lib) in &posix_functions {
                if ir_code.contains(func) {
                    self.config.libraries.insert(lib.to_string());
                }
            }
        }

        // SDL2 库函数检测 - 使用函数名推断而非硬编码列表
        // 所有以 "SDL_" 开头的函数都会触发 SDL2 库链接
        // 此逻辑已移至 infer_lib_from_function 方法，保持一致性
    }

    /// 从字节码模块分析需要的库
    pub fn analyze_bytecode(&mut self, module: &super::BytecodeModule) {
        // 添加模块声明的外部库
        for lib in &module.header.external_libs {
            self.config.libraries.insert(lib.clone());
        }

        // 分析类型定义和方法
        for type_def in &module.type_definitions {
            for method in &type_def.methods {
                if method.modifiers.is_native {
                    // 本地方法可能需要特定的库
                    if let Some(name) = module.constant_pool.get_string(method.name_index) {
                        self.infer_lib_from_function(&name);
                    }
                }
            }
        }
    }

    /// 根据函数名推断库
    fn infer_lib_from_function(&mut self, func_name: &str) {
        match func_name {
            "MessageBox" | "MessageBoxA" | "MessageBoxW" => {
                self.config.libraries.insert("user32".to_string());
            }
            "GetLastError" | "Sleep" | "CreateThread" => {
                self.config.libraries.insert("kernel32".to_string());
            }
            "socket" | "connect" | "bind" | "listen" | "accept" => {
                if cfg!(target_os = "windows") {
                    self.config.libraries.insert("ws2_32".to_string());
                }
            }
            "sqrt" | "sin" | "cos" | "tan" | "pow" | "log" | "exp" => {
                self.config.libraries.insert("m".to_string());
            }
            // SDL2 函数推断
            func if func.starts_with("SDL_") => {
                self.config.libraries.insert("SDL2".to_string());
            }
            _ => {}
        }
    }

    /// 查找库文件
    pub fn find_library(&self, lib_name: &str) -> Option<PathBuf> {
        let lib_file = self.get_library_filename(lib_name);

        for path in &self.config.lib_paths {
            let full_path = path.join(&lib_file);
            if full_path.exists() {
                return Some(full_path);
            }
        }

        None
    }

    /// 获取库文件名
    fn get_library_filename(&self, lib_name: &str) -> String {
        if cfg!(target_os = "windows") {
            format!("lib{}.a", lib_name)
        } else {
            format!("lib{}.so", lib_name)
        }
    }

    /// 获取链接参数
    pub fn get_link_args(&self) -> Vec<String> {
        let mut args = Vec::new();

        // 添加库搜索路径
        for path in &self.config.lib_paths {
            args.push("-L".to_string());
            args.push(path.to_string_lossy().to_string());
        }

        // 添加要链接的库
        for lib in &self.config.libraries {
            args.push(format!("-l{}", lib));
        }

        // 平台特定的默认库
        if cfg!(target_os = "linux") || cfg!(target_os = "macos") {
            // 确保数学库和 C 库被链接
            if !self.config.libraries.contains("m") {
                args.push("-lm".to_string());
            }
            if !self.config.libraries.contains("c") {
                args.push("-lc".to_string());
            }
        }

        args
    }

    /// 添加外部库链接（用于 extern 声明）
    pub fn add_external_library(&mut self, lib_name: &str) {
        self.config.libraries.insert(lib_name.to_string());
    }

    /// 添加多个外部库链接
    pub fn add_external_libraries(&mut self, libs: &[&str]) {
        for lib in libs {
            self.config.libraries.insert(lib.to_string());
        }
    }

    /// 从 extern 声明添加库
    pub fn link_extern_library(&mut self, lib_name: &str) -> bool {
        // 检查库是否存在
        if self.find_library(lib_name).is_some() {
            self.config.libraries.insert(lib_name.to_string());
            true
        } else {
            false
        }
    }

    /// 检查所有需要的库是否都存在
    pub fn check_libraries(&self) -> Result<(), Vec<String>> {
        let mut missing = Vec::new();

        for lib in &self.config.libraries {
            if self.find_library(lib).is_none() {
                missing.push(lib.clone());
            }
        }

        if missing.is_empty() {
            Ok(())
        } else {
            Err(missing)
        }
    }

    /// 获取库信息报告
    pub fn get_library_report(&self) -> String {
        let mut report = String::new();
        report.push_str("=== 链接库分析报告 ===\n\n");

        report.push_str("库搜索路径:\n");
        for path in &self.config.lib_paths {
            let status = if path.exists() {
                "[存在]"
            } else {
                "[不存在]"
            };
            report.push_str(&format!("  {} {}\n", status, path.display()));
        }

        report.push_str("\n需要的库:\n");
        for lib in &self.config.libraries {
            match self.find_library(lib) {
                Some(path) => {
                    report.push_str(&format!("  [找到] {} -> {}\n", lib, path.display()));
                }
                None => {
                    report.push_str(&format!("  [缺失] {}\n", lib));
                }
            }
        }

        report
    }
}

impl Default for AutoLinker {
    fn default() -> Self {
        Self::new(LinkerConfig::default())
    }
}

/// 便捷的链接函数

/// 自动链接并生成可执行文件
pub fn auto_link(
    obj_file: &Path,
    output_path: &str,
    source_analysis: Option<&str>,
) -> Result<(), String> {
    let mut linker = AutoLinker::default();

    // 分析源代码
    if let Some(source) = source_analysis {
        linker.analyze_source(source);
    }

    // 检查库
    if let Err(missing) = linker.check_libraries() {
        return Err(format!("缺少以下库: {:?}", missing));
    }

    // 查找clang
    let clang = find_clang().map_err(|e| e.to_string())?;

    // 构建链接命令
    let mut cmd = std::process::Command::new(&clang);
    cmd.arg(obj_file).arg("-o").arg(output_path);

    // 添加链接参数
    for arg in linker.get_link_args() {
        cmd.arg(arg);
    }

    // 执行链接
    let output = cmd.output().map_err(|e| format!("链接失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("链接错误: {}", stderr));
    }

    Ok(())
}

/// 查找clang编译器
fn find_clang() -> Result<PathBuf, String> {
    // 1. 尝试系统PATH中的clang
    if let Ok(output) = std::process::Command::new("clang")
        .arg("--version")
        .output()
    {
        if output.status.success() {
            return Ok(PathBuf::from("clang"));
        }
    }

    // 2. 尝试gcc
    if let Ok(output) = std::process::Command::new("gcc").arg("--version").output() {
        if output.status.success() {
            return Ok(PathBuf::from("gcc"));
        }
    }

    // 3. 尝试编译器所在目录下的llvm-minimal
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let bundled_clang = exe_dir.join("llvm-minimal/bin/clang.exe");
            if bundled_clang.exists() {
                return Ok(bundled_clang);
            }
        }
    }

    Err("找不到clang或gcc编译器".to_string())
}

/// 库检测工具

/// 检测系统上可用的库
pub fn detect_available_libs() -> Vec<String> {
    let mut available = Vec::new();
    let linker = AutoLinker::default();

    // 常见的库列表
    let common_libs = [
        "m", "pthread", "dl", "rt", "user32", "kernel32", "ws2_32", "gdi32", "stdc++", "gcc",
        "gcc_s",
    ];

    for lib in &common_libs {
        if linker.find_library(lib).is_some() {
            available.push(lib.to_string());
        }
    }

    available
}

/// 打印库检测报告
pub fn print_library_detection_report() {
    let linker = AutoLinker::default();
    println!("{}", linker.get_library_report());

    println!("\n系统检测到的可用库:");
    let available = detect_available_libs();
    for lib in available {
        println!("  - {}", lib);
    }
}
