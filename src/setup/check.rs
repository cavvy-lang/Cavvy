//! 环境检查模块
//!
//! 检查 Cavvy 编译和运行所需的外部依赖是否已安装。
//! 所有检查均为非侵入式，仅读取系统状态，不修改环境。

use super::{SetupError, SetupResult, detect_platform, find_cavvy_root};
use std::path::PathBuf;
use std::process::Command;

/// 单个检查项的结果
#[derive(Debug, Clone, PartialEq)]
pub enum CheckStatus {
    Ok,
    Warning(String),
    Missing(String),
}

/// 环境检查报告
#[derive(Debug, Clone, Default)]
pub struct CheckReport {
    pub items: Vec<(String, CheckStatus)>,
}

impl CheckReport {
    /// 是否所有必需项都通过
    pub fn all_required_ok(&self) -> bool {
        self.items.iter().all(|(_, status)| matches!(status, CheckStatus::Ok))
    }

    /// 是否有任何错误（Missing）
    pub fn has_errors(&self) -> bool {
        self.items
            .iter()
            .any(|(_, status)| matches!(status, CheckStatus::Missing(_)))
    }

    /// 获取所有错误信息
    pub fn errors(&self) -> Vec<String> {
        self.items
            .iter()
            .filter_map(|(name, status)| match status {
                CheckStatus::Missing(msg) => Some(format!("{}: {}", name, msg)),
                _ => None,
            })
            .collect()
    }
}

/// 运行完整环境检查
/// 时间复杂度: O(k), k 为检查命令数量
pub fn run_full_check() -> SetupResult<CheckReport> {
    let mut report = CheckReport::default();
    let (os, _arch) = detect_platform();

    // 1. Git
    report.items.push(check_command("git", &["--version"], "Git 版本控制工具"));

    // 2. Python（用于 setup-llvm.py 回退）
    report.items.push(check_command("python", &["--version"], "Python 解释器"));
    if !matches!(report.items.last().unwrap().1, CheckStatus::Ok) {
        report.items.push(check_command("python3", &["--version"], "Python3 解释器"));
    }

    // 3. Rust / Cargo
    report
        .items
        .push(check_command("cargo", &["--version"], "Rust Cargo 构建工具"));
    report
        .items
        .push(check_command("rustc", &["--version"], "Rust 编译器"));

    // 4. LLVM 工具链
    report.items.push(check_command(
        "llvm-config",
        &["--version"],
        "LLVM 配置工具 (llvm-config)",
    ));
    report
        .items
        .push(check_command("clang", &["--version"], "Clang 编译器"));
    report.items.push(check_command("llc", &["--version"], "LLVM IR 编译器 (llc)"));

    // 5. 链接器
    // lld 通用驱动在某些系统上 --version 返回错误，优先检查平台特定驱动
    let lld_ok = if os == "win" {
        check_command("lld-link", &["--version"], "LLD Windows 链接器").1 == CheckStatus::Ok
    } else {
        check_command("ld.lld", &["--version"], "LLVM ELF 链接器 (ld.lld)").1 == CheckStatus::Ok
    };
    if lld_ok {
        report.items.push((format!("LLVM 链接器 (lld) ({})", if os == "win" { "lld-link" } else { "ld.lld" }), CheckStatus::Ok));
    } else {
        report.items.push(check_command("lld", &["--version"], "LLVM 链接器 (lld)"));
    }

    // 6. MinGW (Windows 必需)
    if os == "win" {
        report
            .items
            .push(check_command("gcc", &["--version"], "MinGW/GCC 编译器"));
    }

    // 7. 检查 Cavvy 项目根目录
    report.items.push(check_cavvy_root());

    // 8. 检查 llvm-minimal 目录（如果存在）
    report.items.push(check_llvm_minimal());

    Ok(report)
}

/// 检查外部命令是否可用
fn check_command(cmd: &str, args: &[&str], description: &str) -> (String, CheckStatus) {
    let label = format!("{} ({})", description, cmd);

    match Command::new(cmd).args(args).output() {
        Ok(output) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let version_line = if stdout.trim().is_empty() {
                stderr.lines().next().unwrap_or("未知版本").to_string()
            } else {
                stdout.lines().next().unwrap_or("未知版本").to_string()
            };
            (label, CheckStatus::Ok)
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            (
                label,
                CheckStatus::Missing(format!(
                    "命令返回错误 (exit code: {:?}): {}",
                    output.status.code(),
                    stderr.trim()
                )),
            )
        }
        Err(e) => (label, CheckStatus::Missing(format!("无法执行命令: {}", e))),
    }
}

/// 检查 Cavvy 项目根目录
fn check_cavvy_root() -> (String, CheckStatus) {
    match find_cavvy_root() {
        Some(root) => (
            "Cavvy 项目根目录".to_string(),
            CheckStatus::Ok,
        ),
        None => (
            "Cavvy 项目根目录".to_string(),
            CheckStatus::Warning(
                "未找到 Cavvy 项目根目录 (未检测到 Cargo.toml + .verinfo)".to_string(),
            ),
        ),
    }
}

/// 检查 llvm-minimal 目录是否存在且包含关键二进制文件
/// 优先检查 Cavvy 安装目录，其次检查项目源码根目录
fn check_llvm_minimal() -> (String, CheckStatus) {
    let candidates: Vec<PathBuf> = {
        let mut v = Vec::new();
        if let Some(install_dir) = super::download::find_cavvy_install_dir() {
            v.push(install_dir.join("llvm-minimal"));
        }
        if let Some(root) = find_cavvy_root() {
            v.push(root.join("llvm-minimal"));
        }
        v
    };

    if candidates.is_empty() {
        return (
            "llvm-minimal 本地工具".to_string(),
            CheckStatus::Warning("未找到 Cavvy 项目根目录或安装目录".to_string()),
        );
    }

    let mut last_status = CheckStatus::Warning(
        "llvm-minimal 目录不存在，运行 'cay-setup download' 可自动下载".to_string(),
    );

    for llvm_minimal in &candidates {
        if !llvm_minimal.exists() {
            continue;
        }

        let bin_dir = llvm_minimal.join("bin");
        if !bin_dir.exists() {
            last_status = CheckStatus::Warning(format!(
                "{} 下 bin 目录不存在",
                llvm_minimal.display()
            ));
            continue;
        }

        let essential = ["clang", "llc", "lld"];
        let mut missing = Vec::new();
        for bin in &essential {
            let exe = if cfg!(target_os = "windows") {
                format!("{}.exe", bin)
            } else {
                bin.to_string()
            };
            if !bin_dir.join(&exe).exists() {
                missing.push(bin.to_string());
            }
        }

        if missing.is_empty() {
            return (
                "llvm-minimal 本地工具".to_string(),
                CheckStatus::Ok,
            );
        } else {
            last_status = CheckStatus::Warning(format!(
                "{} 缺少关键二进制文件: {}",
                llvm_minimal.display(),
                missing.join(", ")
            ));
        }
    }

    ("llvm-minimal 本地工具".to_string(), last_status)
}

/// 查找 clang 可执行文件路径
/// 搜索顺序: 系统 PATH -> Cavvy 安装目录下的 llvm-minimal/bin -> 项目源码根目录下的 llvm-minimal/bin
/// 时间复杂度: O(1)
pub fn find_clang() -> SetupResult<PathBuf> {
    if let Ok(output) = Command::new("clang").arg("--version").output() {
        if output.status.success() {
            return Ok(PathBuf::from("clang"));
        }
    }

    // 检查 Cavvy 安装目录
    if let Some(install_dir) = super::download::find_cavvy_install_dir() {
        let bundled = install_dir.join("llvm-minimal/bin/clang");
        let bundled_exe = if cfg!(target_os = "windows") {
            bundled.with_extension("exe")
        } else {
            bundled
        };
        if bundled_exe.exists() {
            return Ok(bundled_exe);
        }
    }

    if let Some(root) = find_cavvy_root() {
        let bundled = root.join("llvm-minimal/bin/clang");
        let bundled_exe = if cfg!(target_os = "windows") {
            bundled.with_extension("exe")
        } else {
            bundled
        };
        if bundled_exe.exists() {
            return Ok(bundled_exe);
        }
    }

    Err(SetupError::NotFound(
        "找不到 clang 编译器。请安装 LLVM 或运行 'cay-setup download'".to_string(),
    ))
}

/// 查找 llvm-config 可执行文件路径
/// 搜索顺序: 系统 PATH -> Cavvy 安装目录下的 llvm-minimal/bin -> 项目源码根目录下的 llvm-minimal/bin
pub fn find_llvm_config() -> SetupResult<PathBuf> {
    if let Ok(output) = Command::new("llvm-config").arg("--version").output() {
        if output.status.success() {
            return Ok(PathBuf::from("llvm-config"));
        }
    }

    // 检查 Cavvy 安装目录
    if let Some(install_dir) = super::download::find_cavvy_install_dir() {
        let bundled = install_dir.join("llvm-minimal/bin/llvm-config");
        let bundled_exe = if cfg!(target_os = "windows") {
            bundled.with_extension("exe")
        } else {
            bundled
        };
        if bundled_exe.exists() {
            return Ok(bundled_exe);
        }
    }

    if let Some(root) = find_cavvy_root() {
        let bundled = root.join("llvm-minimal/bin/llvm-config");
        let bundled_exe = if cfg!(target_os = "windows") {
            bundled.with_extension("exe")
        } else {
            bundled
        };
        if bundled_exe.exists() {
            return Ok(bundled_exe);
        }
    }

    Err(SetupError::NotFound(
        "找不到 llvm-config。请安装 LLVM 或运行 'cay-setup download'".to_string(),
    ))
}

/// 获取 LLVM 版本号（通过 llvm-config）
pub fn get_llvm_version() -> SetupResult<String> {
    let llvm_config = find_llvm_config()?;
    let output = Command::new(&llvm_config)
        .arg("--version")
        .output()
        .map_err(|e| SetupError::CommandFailed(format!("无法运行 llvm-config: {}", e)))?;

    if !output.status.success() {
        return Err(SetupError::CommandFailed(
            "llvm-config --version 执行失败".to_string(),
        ));
    }

    let version = String::from_utf8_lossy(&output.stdout);
    Ok(version.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_report_ok_when_empty() {
        let report = CheckReport::default();
        assert!(report.all_required_ok());
        assert!(!report.has_errors());
    }

    #[test]
    fn test_check_report_detects_errors() {
        let mut report = CheckReport::default();
        report.items.push(("A".to_string(), CheckStatus::Ok));
        report.items.push(("B".to_string(), CheckStatus::Missing("x".to_string())));
        assert!(!report.all_required_ok());
        assert!(report.has_errors());
        assert_eq!(report.errors().len(), 1);
    }

    #[test]
    fn test_check_command_git() {
        let (label, status) = check_command("git", &["--version"], "Git");
        assert!(label.contains("Git"));
    }
}
