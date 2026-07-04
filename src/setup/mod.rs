//! Cavvy Setup 模块
//!
//! 提供环境检查、依赖下载和安装功能，被 cay-setup 二进制工具使用。
//!
//! 所有版本信息从 .verinfo 文件动态读取，不硬编码任何版本号，
//! 确保兼容未来所有 Cavvy 版本。

pub mod check;
pub mod download;

use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Setup 操作结果类型
pub type SetupResult<T> = Result<T, SetupError>;

/// Setup 错误类型
#[derive(Debug)]
pub enum SetupError {
    Io(io::Error),
    Parse(String),
    NotFound(String),
    CommandFailed(String),
    VerificationFailed(String),
}

impl fmt::Display for SetupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SetupError::Io(e) => write!(f, "IO错误: {}", e),
            SetupError::Parse(msg) => write!(f, "解析错误: {}", msg),
            SetupError::NotFound(msg) => write!(f, "未找到: {}", msg),
            SetupError::CommandFailed(msg) => write!(f, "命令执行失败: {}", msg),
            SetupError::VerificationFailed(msg) => write!(f, "验证失败: {}", msg),
        }
    }
}

impl std::error::Error for SetupError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SetupError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for SetupError {
    fn from(err: io::Error) -> Self {
        SetupError::Io(err)
    }
}

/// Cavvy 版本信息（从 .verinfo 解析）
#[derive(Debug, Clone, Default)]
pub struct VersionInfo {
    /// 各组件版本映射: 组件名 -> {属性名 -> 属性值}
    pub sections: HashMap<String, HashMap<String, String>>,
}

impl VersionInfo {
    /// 获取指定组件的版本号
    pub fn version(&self, component: &str) -> Option<&str> {
        self.sections
            .get(component)
            .and_then(|map| map.get("version"))
            .map(|s| s.as_str())
    }

    /// 获取 Cavvy 主版本号（优先使用 CAYC 的版本）
    pub fn cavvy_version(&self) -> Option<&str> {
        self.version("CAYC")
    }

    /// 获取 LLVM-MINIMAL 版本号
    pub fn llvm_minimal_version(&self) -> Option<&str> {
        self.version("LLVM-MINIMAL")
    }

    /// 获取 MINGW-MINIMAL 版本号
    pub fn mingw_minimal_version(&self) -> Option<&str> {
        self.version("MINGW-MINIMAL")
    }

    /// 列出所有已知组件及其版本
    pub fn list_components(&self) -> Vec<(&str, &str)> {
        let mut result = Vec::new();
        for (name, map) in &self.sections {
            if let Some(ver) = map.get("version") {
                result.push((name.as_str(), ver.as_str()));
            }
        }
        result
    }
}

/// 解析 .verinfo 文件
/// 时间复杂度: O(n), n 为文件行数
/// 空间复杂度: O(k), k 为节和键值对数量
pub fn parse_verinfo<P: AsRef<Path>>(path: P) -> SetupResult<VersionInfo> {
    let content = fs::read_to_string(path.as_ref())?;
    let mut sections = HashMap::new();
    let mut current_section: Option<String> = None;

    for (line_no, line) in content.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            let section_name = line[1..line.len() - 1].to_string();
            current_section = Some(section_name.clone());
            sections.entry(section_name).or_insert_with(HashMap::new);
        } else if let Some(ref section) = current_section {
            if let Some(pos) = line.find('=') {
                let key = line[..pos].trim().to_string();
                let value = line[pos + 1..].trim().to_string();
                let value = if value.starts_with('"') && value.ends_with('"') {
                    value[1..value.len() - 1].to_string()
                } else {
                    value
                };

                sections
                    .entry(section.clone())
                    .or_insert_with(HashMap::new)
                    .insert(key, value);
            }
        } else {
            return Err(SetupError::Parse(format!(
                "第 {} 行不在任何节内: {}",
                line_no + 1,
                line
            )));
        }
    }

    Ok(VersionInfo { sections })
}

/// 尝试从多个位置查找 .verinfo 文件
/// 搜索顺序: 当前工作目录 -> 可执行文件所在目录 -> 上级目录（用于开发环境）
pub fn find_verinfo() -> Option<PathBuf> {
    let candidates = [
        PathBuf::from(".verinfo"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join(".verinfo")))
            .unwrap_or_else(|| PathBuf::from(".verinfo")),
        PathBuf::from("../.verinfo"),
        PathBuf::from("../../.verinfo"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Some(candidate.clone());
        }
    }
    None
}

/// 检测当前平台
/// 返回: (os_name, arch)
/// os_name: "win" | "linux"
/// arch: "x86_64"
pub fn detect_platform() -> (&'static str, &'static str) {
    let os = if cfg!(target_os = "windows") {
        "win"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        panic!("不支持的操作系统: 仅支持 Windows 和 Linux")
    };

    let arch = if cfg!(target_arch = "x86_64") || cfg!(target_arch = "amd64") {
        "x86_64"
    } else {
        panic!("不支持的架构: 仅支持 x86_64")
    };

    (os, arch)
}

/// 判断是否在 CI 环境中运行
pub fn is_ci_environment() -> bool {
    let ci_vars = [
        "CI",
        "GITHUB_ACTIONS",
        "GITLAB_CI",
        "TRAVIS",
        "CIRCLECI",
        "APPVEYOR",
        "BUILDKITE",
        "DRONE",
        "JENKINS_URL",
        "TF_BUILD",
    ];
    ci_vars.iter().any(|var| std::env::var(var).is_ok())
}

/// 获取 Cavvy 项目根目录
/// 优先从 CAVVY_ROOT 环境变量读取，否则尝试从当前目录或上级目录推断
pub fn find_cavvy_root() -> Option<PathBuf> {
    if let Ok(root) = std::env::var("CAVVY_ROOT") {
        let path = PathBuf::from(root);
        if path.join("Cargo.toml").exists() {
            return Some(path);
        }
    }

    let candidates = [
        PathBuf::from("."),
        PathBuf::from(".."),
        PathBuf::from("../.."),
    ];

    for candidate in &candidates {
        if candidate.join("Cargo.toml").exists() && candidate.join(".verinfo").exists() {
            return Some(
                candidate
                    .canonicalize()
                    .unwrap_or_else(|_| candidate.clone()),
            );
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_platform() {
        let (os, arch) = detect_platform();
        assert!(os == "win" || os == "linux");
        assert_eq!(arch, "x86_64");
    }

    #[test]
    fn test_is_ci_environment_does_not_panic() {
        let _ = is_ci_environment();
    }

    #[test]
    fn test_find_cavvy_root_or_none() {
        let _ = find_cavvy_root();
    }
}
