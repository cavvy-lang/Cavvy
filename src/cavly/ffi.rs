use crate::cavly::config::{CavlyConfig, ExternalLibrary, PlatformConfig};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// FFI 库解析器
///
/// 处理跨平台库链接配置
pub struct FfiResolver {
    /// 当前平台标识
    platform: String,
}

impl FfiResolver {
    /// 创建新的 FFI 解析器
    pub fn new() -> Self {
        let platform = if cfg!(target_os = "windows") {
            "windows"
        } else if cfg!(target_os = "linux") {
            "linux"
        } else if cfg!(target_os = "macos") {
            "macos"
        } else {
            "unknown"
        }
        .to_string();

        Self { platform }
    }

    /// 解析库配置，获取当前平台的有效配置
    ///
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    pub fn resolve_library(&self, lib: &ExternalLibrary) -> ResolvedLibrary {
        // 优先使用平台特定配置
        let platform_config = lib.platform.get(&self.platform);

        ResolvedLibrary {
            name: lib.name.clone(),
            lib: platform_config
                .and_then(|p| p.lib.clone())
                .unwrap_or_else(|| lib.lib.clone()),
            path: platform_config
                .and_then(|p| p.path.clone())
                .or_else(|| lib.path.clone()),
            static_lib: lib.static_lib,
            deps: lib.deps.clone(),
            ldflags: platform_config
                .map(|p| p.ldflags.clone())
                .unwrap_or_default(),
        }
    }

    /// 生成链接命令参数
    ///
    /// # 复杂度
    /// - 时间: O(n)，n 为库数量
    /// - 空间: O(n)
    pub fn generate_link_args(&self, config: &CavlyConfig) -> Vec<String> {
        let mut args = Vec::new();

        // 系统库
        for lib in &config.ffi.system_libs {
            args.push("-l".to_string());
            args.push(lib.clone());
        }

        // 第三方库
        for lib_config in config.ffi.libraries.values() {
            let resolved = self.resolve_library(lib_config);

            // 库路径
            if let Some(ref path) = resolved.path {
                args.push("-L".to_string());
                args.push(path.clone());
            }

            // 平台特定链接选项
            for flag in &resolved.ldflags {
                args.push(flag.clone());
            }

            // 静态链接标记
            if resolved.static_lib {
                args.push("-static".to_string());
            }

            // 主库
            args.push("-l".to_string());
            args.push(resolved.lib);

            // 依赖库
            for dep in &resolved.deps {
                args.push("-l".to_string());
                args.push(dep.clone());
            }
        }

        // FFI 链接选项
        for opt in &config.ffi.link_options {
            args.push(opt.clone());
        }

        args
    }

    /// 获取平台特定的库文件名
    ///
    /// # 示例
    /// - Windows: `mylib` -> `mylib.dll` / `mylib.lib`
    /// - Linux: `mylib` -> `libmylib.so` / `libmylib.a`
    pub fn format_lib_name(&self, name: &str, static_lib: bool) -> String {
        match self.platform.as_str() {
            "windows" => {
                if static_lib {
                    format!("{}.lib", name)
                } else {
                    format!("{}.dll", name)
                }
            }
            "linux" | "macos" => {
                if static_lib {
                    format!("lib{}.a", name)
                } else {
                    format!("lib{}.so", name)
                }
            }
            _ => name.to_string(),
        }
    }

    /// 检测系统已安装的库
    ///
    /// # 复杂度
    /// - 时间: O(n)，n 为搜索路径数量
    /// - 空间: O(1)
    pub fn detect_system_library(&self, name: &str) -> Option<PathBuf> {
        let lib_name = self.format_lib_name(name, false);

        // 常见系统库路径
        let search_paths = self.get_system_lib_paths();

        for path in search_paths {
            let full_path = path.join(&lib_name);
            if full_path.exists() {
                return Some(full_path);
            }
        }

        None
    }

    /// 获取系统库搜索路径
    fn get_system_lib_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        match self.platform.as_str() {
            "windows" => {
                // Windows 系统路径
                if let Ok(windir) = std::env::var("WINDIR") {
                    paths.push(PathBuf::from(windir).join("System32"));
                }

                // MinGW 路径
                if let Ok(path) = std::env::var("PATH") {
                    for p in path.split(';') {
                        if p.to_lowercase().contains("mingw") {
                            paths.push(PathBuf::from(p));
                        }
                    }
                }
            }
            "linux" => {
                // 标准 Linux 库路径
                paths.push(PathBuf::from("/usr/lib"));
                paths.push(PathBuf::from("/usr/lib64"));
                paths.push(PathBuf::from("/usr/local/lib"));
                paths.push(PathBuf::from("/lib"));
                paths.push(PathBuf::from("/lib64"));

                // 从 ldconfig 获取
                if let Ok(output) = std::process::Command::new("ldconfig")
                    .args(&["-v", "-N"])
                    .output()
                {
                    if output.status.success() {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        for line in stdout.lines() {
                            if line.starts_with('\t') {
                                continue;
                            }
                            if let Some(colon_pos) = line.find(':') {
                                let path = &line[..colon_pos];
                                paths.push(PathBuf::from(path));
                            }
                        }
                    }
                }
            }
            "macos" => {
                paths.push(PathBuf::from("/usr/lib"));
                paths.push(PathBuf::from("/usr/local/lib"));

                // Homebrew 路径
                if Path::new("/opt/homebrew/lib").exists() {
                    paths.push(PathBuf::from("/opt/homebrew/lib"));
                }
                if Path::new("/usr/local/opt").exists() {
                    paths.push(PathBuf::from("/usr/local/opt"));
                }
            }
            _ => {}
        }

        paths
    }
}

impl Default for FfiResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// 解析后的库配置
#[derive(Debug, Clone)]
pub struct ResolvedLibrary {
    pub name: String,
    pub lib: String,
    pub path: Option<String>,
    pub static_lib: bool,
    pub deps: Vec<String>,
    pub ldflags: Vec<String>,
}

/// FFI 帮助工具
pub struct FfiHelper;

impl FfiHelper {
    /// 生成 FFI 声明模板
    ///
    /// # 复杂度
    /// - 时间: O(1)
    /// - 空间: O(1)
    pub fn generate_ffi_template(lib_name: &str, functions: &[&str]) -> String {
        let mut template = format!("// FFI 声明 for {}\n\n", lib_name);

        for func in functions {
            template.push_str(&format!(
                "extern func {}(/* params */) /* return type */;\n",
                func
            ));
        }

        template.push_str("\n// 使用示例:\n");
        template.push_str(&format!("// {}_function(args);\n", lib_name.to_lowercase()));

        template
    }

    /// 验证 FFI 配置
    ///
    /// # 复杂度
    /// - 时间: O(n)，n 为库数量
    /// - 空间: O(1)
    pub fn validate_ffi_config(config: &CavlyConfig) -> Result<()> {
        let resolver = FfiResolver::new();

        for (name, lib) in &config.ffi.libraries {
            let resolved = resolver.resolve_library(lib);

            // 检查路径是否存在（如果指定了）
            if let Some(ref path) = resolved.path {
                let path_obj = Path::new(path);
                if !path_obj.exists() {
                    eprintln!("警告: 库 '{}' 的路径不存在: {}", name, path);
                }
            }

            // 检查系统库是否可检测
            if resolver.detect_system_library(&resolved.lib).is_none() {
                // 只是警告，不报错，因为可能在其他位置
                if resolved.path.is_none() {
                    eprintln!("警告: 库 '{}' 未在系统路径中找到，请确保路径配置正确", name);
                }
            }
        }

        Ok(())
    }

    /// 打印 FFI 配置信息
    pub fn print_ffi_info(config: &CavlyConfig) {
        let resolver = FfiResolver::new();

        println!("FFI 配置:");
        println!("  平台: {}", resolver.platform);

        if !config.ffi.system_libs.is_empty() {
            println!("  系统库:");
            for lib in &config.ffi.system_libs {
                println!("    - {}", lib);
            }
        }

        if !config.ffi.libraries.is_empty() {
            println!("  第三方库:");
            for (name, lib) in &config.ffi.libraries {
                let resolved = resolver.resolve_library(lib);
                println!("    {}:", name);
                println!("      库名: {}", resolved.lib);
                if let Some(ref path) = resolved.path {
                    println!("      路径: {}", path);
                }
                if resolved.static_lib {
                    println!("      类型: 静态库");
                }
                if !resolved.deps.is_empty() {
                    println!("      依赖: {}", resolved.deps.join(", "));
                }
            }
        }

        if !config.ffi.include_paths.is_empty() {
            println!("  头文件路径:");
            for path in &config.ffi.include_paths {
                println!("    - {}", path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ffi_resolver_new() {
        let resolver = FfiResolver::new();
        assert!(!resolver.platform.is_empty());
    }

    #[test]
    fn test_resolve_library() {
        let resolver = FfiResolver::new();

        let lib = ExternalLibrary {
            name: "Test".to_string(),
            lib: "test".to_string(),
            path: Some("/usr/lib".to_string()),
            static_lib: false,
            deps: vec!["dep1".to_string()],
            platform: HashMap::new(),
        };

        let resolved = resolver.resolve_library(&lib);
        assert_eq!(resolved.name, "Test");
        assert_eq!(resolved.lib, "test");
        assert_eq!(resolved.path, Some("/usr/lib".to_string()));
    }

    #[test]
    fn test_format_lib_name() {
        let resolver = FfiResolver::new();

        // 测试不同平台的库名格式
        let lib = resolver.format_lib_name("mylib", false);
        assert!(lib.contains("mylib"));

        let static_lib = resolver.format_lib_name("mylib", true);
        assert!(static_lib.contains("mylib"));
    }

    #[test]
    fn test_generate_ffi_template() {
        let template = FfiHelper::generate_ffi_template("MyLib", &["func1", "func2"]);
        assert!(template.contains("MyLib"));
        assert!(template.contains("func1"));
        assert!(template.contains("func2"));
        assert!(template.contains("extern func"));
    }
}
