//! 下载与安装模块
//!
//! 负责下载 Cavvy 所需的外部依赖（LLVM minimal、MinGW 等），
//! 支持从 GitHub API 动态获取最新预编译包，解压、验证，
//! 并自动将安装目录添加到系统 PATH。
//!
//! 下载策略（按优先级）:
//! 1. 使用系统已安装的 curl / wget
//! 2. Windows 下使用 PowerShell Invoke-WebRequest
//! 3. 使用 Python urllib（回退）

use super::{
    SetupError, SetupResult, VersionInfo, detect_platform, find_cavvy_root, is_ci_environment,
};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// GitHub API 基础 URL
const GITHUB_API_BASE: &str = "https://api.github.com/repos";

/// 下载配置
pub struct DownloadConfig {
    pub github_repo: String,
    pub cavvy_repo: String,
    pub timeout_seconds: u64,
    pub use_full_llvm: bool,
    pub use_prebuilt: bool,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            github_repo: "cavvy-lang/Cavvy-src-Assets".to_string(),
            cavvy_repo: "cavvy-lang/Cavvy".to_string(),
            timeout_seconds: 300,
            use_full_llvm: std::env::var("CAVVY_USE_FULL_LLVM")
                .unwrap_or_default()
                .to_lowercase()
                .chars()
                .any(|c| c == '1' || c == 't' || c == 'y'),
            use_prebuilt: true,
        }
    }
}

/// GitHub Release 信息（从 API 解析的最小子集）
#[derive(Debug, Clone)]
pub struct GitHubRelease {
    pub tag_name: String,
    pub assets: Vec<GitHubAsset>,
}

/// GitHub Release Asset
#[derive(Debug, Clone)]
pub struct GitHubAsset {
    pub name: String,
    pub browser_download_url: String,
}

/// 使用 curl 调用 GitHub API 获取最新 release 信息
/// 时间复杂度: O(1)（网络请求常数时间）
/// 磁盘 IO: 无（纯内存操作）
fn fetch_github_api_json(api_url: &str) -> SetupResult<String> {
    // 尝试策略：标准请求 -> 禁用 SSL 证书吊销检查 -> 跳过证书验证
    let attempts = [
        vec!["-s", "-L", "-f"],
        vec!["-s", "-L", "-f", "--ssl-no-revoke"],
        vec!["-s", "-L", "-f", "-k"],
    ];

    let mut last_error = String::new();

    for (idx, flags) in attempts.iter().enumerate() {
        let mut cmd = Command::new("curl");
        for flag in flags.iter() {
            cmd.arg(*flag);
        }
        cmd.arg("-H")
            .arg("Accept: application/vnd.github+json")
            .arg("-H")
            .arg("User-Agent: cavvy-setup/1.0")
            .arg("--connect-timeout")
            .arg("30")
            .arg("--max-time")
            .arg("60")
            .arg(api_url);

        match cmd.output() {
            Ok(output) if output.status.success() => {
                let body = String::from_utf8_lossy(&output.stdout);
                return Ok(body.to_string());
            }
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                last_error = format!(
                    "尝试 {} 失败 (exit code: {:?}): {}",
                    idx + 1,
                    output.status.code(),
                    stderr.trim()
                );
                // 仅在 SSL 相关错误码时继续重试
                let code = output.status.code().unwrap_or(-1);
                if ![35, 58, 60, 77].contains(&code) {
                    break;
                }
            }
            Err(e) => {
                last_error = format!("尝试 {} curl 启动失败: {}", idx + 1, e);
                break;
            }
        }
    }

    Err(SetupError::CommandFailed(format!(
        "GitHub API 请求失败。可能原因：网络代理、SSL 证书问题、或 GitHub 服务不可用。\n{}",
        last_error
    )))
}

/// 从 GitHub API JSON 响应中解析 release 信息
/// 使用最小化手动解析，避免引入 serde 依赖（setup 模块保持轻量）
fn parse_release_json(json: &str) -> SetupResult<GitHubRelease> {
    let tag_name = extract_json_string(json, "tag_name")
        .ok_or_else(|| SetupError::Parse("GitHub API 响应缺少 tag_name".to_string()))?;

    let mut assets = Vec::new();
    if let Some(assets_start) = json.find("\"assets\"") {
        let mut bracket_depth = 0;
        let mut in_assets = false;
        let mut current_asset: Option<(String, String)> = None;

        // 简化解析：直接搜索每个 asset 对象中的 name 和 browser_download_url
        let after_assets = &json[assets_start..];
        if let Some(arr_start) = after_assets.find('[') {
            if let Some(arr_end) = find_matching_bracket(after_assets, arr_start, '[', ']') {
                let assets_array = &after_assets[arr_start + 1..arr_end];
                // 按对象分割
                for obj_str in split_json_objects(assets_array) {
                    if let (Some(name), Some(url)) = (
                        extract_json_string(obj_str, "name"),
                        extract_json_string(obj_str, "browser_download_url"),
                    ) {
                        assets.push(GitHubAsset {
                            name,
                            browser_download_url: url,
                        });
                    }
                }
            }
        }
    }

    Ok(GitHubRelease { tag_name, assets })
}

/// 查找匹配的闭合括号
fn find_matching_bracket(s: &str, start: usize, open: char, close: char) -> Option<usize> {
    let mut depth = 1;
    for (i, c) in s.char_indices().skip(start + 1) {
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(i);
            }
        }
    }
    None
}

/// 将 JSON 数组内容按顶层对象分割
fn split_json_objects(array_inner: &str) -> Vec<&str> {
    let mut objects = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let mut in_string = false;
    let mut escape = false;

    for (i, c) in array_inner.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if c == '\\' && in_string {
            escape = true;
            continue;
        }
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        if c == '{' {
            if depth == 0 {
                start = i;
            }
            depth += 1;
        } else if c == '}' {
            depth -= 1;
            if depth == 0 {
                objects.push(&array_inner[start..=i]);
            }
        }
    }
    objects
}

/// 从 JSON 字符串中提取指定键的字符串值（最简实现）
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\"", key);
    let pos = json.find(&pattern)?;
    let after_key = &json[pos + pattern.len()..];
    let colon_pos = after_key.find(':')?;
    let after_colon = &after_key[colon_pos + 1..];
    let trimmed = after_colon.trim_start();

    if trimmed.starts_with('"') {
        let mut result = String::new();
        let mut escape = false;
        for c in trimmed[1..].chars() {
            if escape {
                result.push(c);
                escape = false;
                continue;
            }
            if c == '\\' {
                escape = true;
                continue;
            }
            if c == '"' {
                return Some(result);
            }
            result.push(c);
        }
    }
    None
}

/// 获取指定仓库的最新 release
pub fn fetch_latest_release(repo: &str) -> SetupResult<GitHubRelease> {
    let api_url = format!("{}/{}/releases/latest", GITHUB_API_BASE, repo);
    let json = fetch_github_api_json(&api_url)?;
    parse_release_json(&json)
}

/// 根据平台匹配预编译包 asset
/// Windows: 匹配 *windows*x86_64*.7z
/// Linux:   匹配 *linux*x86_64*.tar.xz
pub fn match_prebuilt_asset<'a>(
    release: &'a GitHubRelease,
    os: &str,
    _arch: &str,
) -> Option<&'a GitHubAsset> {
    let (os_keyword, ext) = match os {
        "win" => ("windows", ".7z"),
        "linux" => ("linux", ".tar.xz"),
        _ => return None,
    };

    release.assets.iter().find(|asset| {
        let name_lower = asset.name.to_lowercase();
        name_lower.contains(os_keyword)
            && name_lower.contains("x86_64")
            && name_lower.ends_with(ext)
    })
}

/// 构建 LLVM minimal 下载 URL
/// 版本号从 .verinfo 动态读取，无硬编码
pub fn build_llvm_download_url(
    version: &str,
    os: &str,
    arch: &str,
    config: &DownloadConfig,
) -> String {
    if config.use_full_llvm {
        if os == "win" {
            format!(
                "https://github.com/llvm/llvm-project/releases/download/llvmorg-{}/clang+llvm-{}-x86_64-pc-windows-msvc.tar.xz",
                version, version
            )
        } else {
            format!(
                "https://github.com/llvm/llvm-project/releases/download/llvmorg-{}/clang+llvm-{}-{}-linux-gnu-ubuntu-22.04.tar.xz",
                version, version, arch
            )
        }
    } else {
        let bin_name = if os == "win" { "bin" } else { "bin-linux" };
        format!(
            "https://github.com/{}/releases/download/llvm-minimal/{}/{}-{}/bin/{}.tar.xz",
            config.github_repo, version, os, arch, bin_name
        )
    }
}

/// 下载文件到指定路径
/// 优先使用 curl，其次 wget，Windows 下使用 PowerShell
/// 时间复杂度: O(n), n 为文件大小
/// 磁盘IO: 顺序写入临时文件，完成后原子重命名
pub fn download_file(url: &str, dest: &Path, timeout_seconds: u64) -> SetupResult<()> {
    if dest.exists() {
        fs::remove_file(dest).ok();
    }

    let parent = dest.parent().ok_or_else(|| {
        SetupError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            "目标路径没有父目录",
        ))
    })?;
    fs::create_dir_all(parent)?;

    let temp_path = dest.with_extension("tmp");

    // 策略1: curl（带 SSL 错误自动重试）
    if is_command_available("curl") {
        let attempts = [
            vec!["-L", "-f"],
            vec!["-L", "-f", "--ssl-no-revoke"],
            vec!["-L", "-f", "-k"],
        ];

        for (idx, flags) in attempts.iter().enumerate() {
            let mut cmd = Command::new("curl");
            for flag in flags.iter() {
                cmd.arg(*flag);
            }
            cmd.arg("-o")
                .arg(&temp_path)
                .arg("--connect-timeout")
                .arg("30")
                .arg("--max-time")
                .arg(&timeout_seconds.to_string())
                .arg(url);

            match cmd.output() {
                Ok(output) if output.status.success() => {
                    atomic_rename(&temp_path, dest)?;
                    return Ok(());
                }
                Ok(output) => {
                    let _ = fs::remove_file(&temp_path);
                    let code = output.status.code().unwrap_or(-1);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!(
                        "[WARN] curl 下载尝试 {} 失败 (exit code: {}): {}",
                        idx + 1,
                        code,
                        stderr.trim()
                    );
                    if ![35, 58, 60, 77].contains(&code) {
                        return Err(SetupError::CommandFailed(format!(
                            "curl 下载失败 (exit code: {:?})",
                            output.status.code()
                        )));
                    }
                }
                Err(e) => {
                    let _ = fs::remove_file(&temp_path);
                    return Err(SetupError::CommandFailed(format!("curl 启动失败: {}", e)));
                }
            }
        }

        return Err(SetupError::CommandFailed(
            "curl 所有下载尝试均失败，可能原因：网络代理、SSL 证书问题、或 GitHub 服务不可用。"
                .to_string(),
        ));
    }

    // 策略2: wget
    if is_command_available("wget") {
        let mut cmd = Command::new("wget");
        cmd.arg("-O")
            .arg(&temp_path)
            .arg("--timeout")
            .arg(&timeout_seconds.to_string())
            .arg("--tries=3")
            .arg(url);

        let output = cmd
            .output()
            .map_err(|e| SetupError::CommandFailed(format!("wget 启动失败: {}", e)))?;
        if !output.status.success() {
            let _ = fs::remove_file(&temp_path);
            return Err(SetupError::CommandFailed(format!(
                "wget 下载失败 (exit code: {:?})",
                output.status.code()
            )));
        }

        atomic_rename(&temp_path, dest)?;
        return Ok(());
    }

    // 策略3: Windows PowerShell Invoke-WebRequest
    if cfg!(target_os = "windows") {
        let ps_cmd = format!(
            "Invoke-WebRequest -Uri '{}' -OutFile '{}' -MaximumRedirection 10",
            url,
            temp_path.display()
        );
        let output = Command::new("powershell")
            .args(&["-Command", &ps_cmd])
            .output()
            .map_err(|e| SetupError::CommandFailed(format!("PowerShell 启动失败: {}", e)))?;
        if !output.status.success() {
            let _ = fs::remove_file(&temp_path);
            return Err(SetupError::CommandFailed(format!(
                "PowerShell 下载失败 (exit code: {:?}): {}",
                output.status.code(),
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        atomic_rename(&temp_path, dest)?;
        return Ok(());
    }

    // 策略4: Python urllib 回退
    let python_cmd = format!(
        "import urllib.request; urllib.request.urlretrieve('{}', '{}')",
        url,
        temp_path.display()
    );
    for py in &["python3", "python"] {
        if is_command_available(py) {
            let output = Command::new(py)
                .args(&["-c", &python_cmd])
                .output()
                .map_err(|e| SetupError::CommandFailed(format!("{} 启动失败: {}", py, e)))?;
            if output.status.success() {
                atomic_rename(&temp_path, dest)?;
                return Ok(());
            }
            let _ = fs::remove_file(&temp_path);
        }
    }

    Err(SetupError::NotFound(
        "无可用的下载工具 (curl, wget, PowerShell, python)".to_string(),
    ))
}

/// 检查命令是否可用
/// 7-Zip 等工具不支持 `--version`，故只要能启动（exit code 存在）即认为可用
fn is_command_available(cmd: &str) -> bool {
    Command::new(cmd)
        .arg("--version")
        .output()
        .map(|o| o.status.code().is_some())
        .unwrap_or(false)
}

/// 原子重命名: 临时文件 -> 目标文件
fn atomic_rename(temp: &Path, dest: &Path) -> SetupResult<()> {
    if !temp.exists() {
        return Err(SetupError::Io(io::Error::new(
            io::ErrorKind::NotFound,
            format!("临时文件不存在: {}", temp.display()),
        )));
    }
    if temp.metadata()?.len() == 0 {
        return Err(SetupError::Io(io::Error::new(
            io::ErrorKind::InvalidData,
            "下载文件大小为 0，可能下载失败",
        )));
    }
    fs::rename(temp, dest)?;
    Ok(())
}

/// 解压 tar.xz 文件
/// 优先使用系统 tar 命令，Windows 下可用 7z 或 PowerShell
/// 时间复杂度: O(n), n 为归档大小
pub fn extract_tar_xz(archive: &Path, dest: &Path) -> SetupResult<()> {
    fs::create_dir_all(dest)?;

    // 策略1: tar 命令
    let tar_output = Command::new("tar")
        .args(&[
            "-xf",
            archive.to_str().unwrap_or_default(),
            "-C",
            dest.to_str().unwrap_or_default(),
        ])
        .output()
        .map_err(|e| SetupError::CommandFailed(format!("tar 启动失败: {}", e)))?;

    if tar_output.status.success() {
        return Ok(());
    }

    // 策略2: 7z (Windows 常见)
    // 注意：7-Zip 的 -o 参数必须紧贴目录路径，不能有空格：-o<dest>
    if cfg!(target_os = "windows") {
        let out_switch = format!("-o{}", dest.to_str().unwrap_or_default());
        let output = Command::new("7z")
            .args(&["x", archive.to_str().unwrap_or_default(), &out_switch, "-y"])
            .output()
            .map_err(|e| SetupError::CommandFailed(format!("7z 启动失败: {}", e)))?;
        if output.status.success() {
            return Ok(());
        }
    }

    Err(SetupError::CommandFailed(
        "无法解压 tar.xz 文件。请确保系统安装了 tar 或 7z".to_string(),
    ))
}

/// 解压 7z 文件
/// 优先使用 7z 命令，其次使用 PowerShell 7z 模块
pub fn extract_7z(archive: &Path, dest: &Path) -> SetupResult<()> {
    fs::create_dir_all(dest)?;

    // 策略1: 7z 命令
    // 注意：7-Zip 的 -o 参数必须紧贴目录路径，不能有空格：-o<dest>
    if is_command_available("7z") {
        let out_switch = format!("-o{}", dest.to_str().unwrap_or_default());
        let output = Command::new("7z")
            .args(&["x", archive.to_str().unwrap_or_default(), &out_switch, "-y"])
            .output()
            .map_err(|e| SetupError::CommandFailed(format!("7z 启动失败: {}", e)))?;
        if output.status.success() {
            return Ok(());
        }
    }

    // 策略2: 7za 命令 (p7zip 备用)
    if is_command_available("7za") {
        let out_switch = format!("-o{}", dest.to_str().unwrap_or_default());
        let output = Command::new("7za")
            .args(&["x", archive.to_str().unwrap_or_default(), &out_switch, "-y"])
            .output()
            .map_err(|e| SetupError::CommandFailed(format!("7za 启动失败: {}", e)))?;
        if output.status.success() {
            return Ok(());
        }
    }

    // 策略3: PowerShell 使用 Expand-Archive（.7z 不支持，但如果重命名为 .zip 可能有效）
    // 对于 .7z，PowerShell 原生不支持，需要 7z 命令行工具
    Err(SetupError::CommandFailed(
        "无法解压 7z 文件。请安装 7-Zip 并确保 7z 在 PATH 中。https://www.7-zip.org/".to_string(),
    ))
}

/// 验证 llvm-minimal 安装是否完整
pub fn verify_llvm_minimal(install_dir: &Path, os: &str) -> SetupResult<()> {
    let bin_dir = install_dir.join("bin");
    if !bin_dir.exists() {
        return Err(SetupError::VerificationFailed(format!(
            "bin 目录不存在: {}",
            bin_dir.display()
        )));
    }

    let essentials = [
        "clang",
        "ld.lld",
        "ld64.lld",
        "llc",
        "lld-link",
        "lld",
        "llvm-ar",
        "llvm-profdata",
        "llvm-profgen",
        "wasm-ld",
    ];
    let mut missing = Vec::new();

    for bin in &essentials {
        let exe = if os == "win" {
            format!("{}.exe", bin)
        } else {
            bin.to_string()
        };
        if !bin_dir.join(&exe).exists() {
            missing.push(bin.to_string());
        }
    }

    if !missing.is_empty() {
        return Err(SetupError::VerificationFailed(format!(
            "缺少关键二进制文件: {}",
            missing.join(", ")
        )));
    }

    Ok(())
}

/// 查找 Cavvy 安装目录
/// 通过搜索 PATH 中的 cayc/cayc.exe，推断安装目录（假设结构 <install_dir>/bin/cayc）
/// 时间复杂度: O(PATH 条目数)
/// 磁盘 IO: O(PATH 条目数)
pub fn find_cavvy_install_dir() -> Option<PathBuf> {
    let exe_name = if cfg!(target_os = "windows") {
        "cayc.exe"
    } else {
        "cayc"
    };

    if let Ok(path_var) = std::env::var("PATH") {
        let separator = if cfg!(target_os = "windows") {
            ';'
        } else {
            ':'
        };
        for dir in path_var.split(separator) {
            let candidate = PathBuf::from(dir).join(exe_name);
            if candidate.exists() {
                return candidate.parent().map(|p| p.to_path_buf());
            }
        }
    }

    None
}

/// 查找指定 Cavvy 工具的可执行文件路径
/// 搜索顺序: PATH -> Cavvy 安装目录/bin -> 项目源码 target/release
/// 时间复杂度: O(PATH 条目数)
pub fn find_tool_path(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(target_os = "windows") {
        format!("{}.exe", name)
    } else {
        name.to_string()
    };

    // 1. 搜索 PATH
    if let Ok(path_var) = std::env::var("PATH") {
        let sep = if cfg!(target_os = "windows") {
            ';'
        } else {
            ':'
        };
        for dir in path_var.split(sep) {
            let candidate = PathBuf::from(dir).join(&exe);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // 2. 搜索 Cavvy 安装目录
    if let Some(install_dir) = find_cavvy_install_dir() {
        let candidate = install_dir.join(&exe);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // 3. 搜索项目源码 target/release
    if let Some(root) = super::find_cavvy_root() {
        let candidate = root.join("target/release").join(&exe);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    None
}

/// 获取指定工具的路径和版本号
/// 尝试 --version，失败则尝试 -V
/// 返回 (路径, 版本首行)
pub fn get_tool_version(name: &str) -> SetupResult<(PathBuf, String)> {
    let path = find_tool_path(name)
        .ok_or_else(|| SetupError::NotFound(format!("未找到工具: {}", name)))?;

    let output = Command::new(&path)
        .arg("--version")
        .output()
        .or_else(|_| Command::new(&path).arg("-V").output())
        .map_err(|e| SetupError::CommandFailed(format!("无法运行 {}: {}", path.display(), e)))?;

    if !output.status.success() {
        return Ok((path, "未知".to_string()));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let line = if stdout.trim().is_empty() {
        stderr.lines().next().unwrap_or("未知").to_string()
    } else {
        stdout.lines().next().unwrap_or("未知").to_string()
    };

    Ok((path, line))
}

/// 下载并安装 LLVM minimal
/// target_dir: 显式指定的安装根目录（LLVM 将安装到 <target_dir>/llvm-minimal）
/// 若未提供，优先使用 Cavvy 安装目录，其次回退到项目源码根目录
pub fn download_and_install_llvm(
    version_info: &VersionInfo,
    config: &DownloadConfig,
    target_dir: Option<&Path>,
) -> SetupResult<PathBuf> {
    let (os, arch) = detect_platform();
    let version = version_info
        .llvm_minimal_version()
        .ok_or_else(|| SetupError::Parse(".verinfo 中缺少 LLVM-MINIMAL 版本号".to_string()))?;

    let (install_dir, download_root) = if let Some(dir) = target_dir {
        let d = dir.join("llvm-minimal");
        (d, dir.to_path_buf())
    } else if let Some(cavvy_dir) = find_cavvy_install_dir() {
        let d = cavvy_dir.join("llvm-minimal");
        (d, cavvy_dir)
    } else {
        let root = find_cavvy_root()
            .ok_or_else(|| SetupError::NotFound("未找到 Cavvy 项目根目录或安装目录".to_string()))?;
        (root.join("llvm-minimal"), root)
    };

    // 如果已安装且验证通过，直接返回
    if verify_llvm_minimal(&install_dir, os).is_ok() {
        eprintln!("[INFO] LLVM minimal 已安装且验证通过，跳过下载");
        return Ok(install_dir);
    }

    let url = build_llvm_download_url(version, os, arch, config);
    let archive_name = if config.use_full_llvm {
        format!("clang+llvm-{}-{}-{}.tar.xz", version, os, arch)
    } else {
        format!("llvm-minimal-{}-{}-{}.tar.xz", version, os, arch)
    };
    let archive_path = download_root.join(&archive_name);

    eprintln!("[INFO] 下载 LLVM minimal {} 从 {}", version, url);
    download_file(&url, &archive_path, config.timeout_seconds)?;

    eprintln!("[INFO] 解压到 {}", install_dir.display());
    extract_tar_xz(&archive_path, &install_dir)?;

    // 如果是完整包，可能需要处理嵌套目录结构
    if config.use_full_llvm {
        flatten_single_subdir(&install_dir)?;
    }

    // 清理压缩包
    let _ = fs::remove_file(&archive_path);

    // 验证
    verify_llvm_minimal(&install_dir, os)?;

    // 自动将 llvm-minimal/bin 添加到 PATH
    let bin_dir = install_dir.join("bin");
    if bin_dir.exists() {
        if let Err(e) = add_to_path(&bin_dir) {
            eprintln!("[WARN] 无法自动添加 PATH: {}", e);
            eprintln!("[INFO] 请手动将以下目录添加到 PATH: {}", bin_dir.display());
        }
    }

    eprintln!("[SUCCESS] LLVM minimal 安装完成: {}", install_dir.display());
    Ok(install_dir)
}

/// 如果目录下只有一个子目录且无文件，将子目录内容提升到当前层
fn flatten_single_subdir(dir: &Path) -> SetupResult<()> {
    let entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    let subdirs: Vec<_> = entries
        .iter()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .collect();
    let files: Vec<_> = entries
        .iter()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .collect();

    if subdirs.len() == 1 && files.is_empty() {
        let sub = subdirs[0].path();
        for entry in fs::read_dir(&sub)? {
            let entry = entry?;
            let dest = dir.join(entry.file_name());
            if dest.exists() {
                if dest.is_dir() {
                    fs::remove_dir_all(&dest)?;
                } else {
                    fs::remove_file(&dest)?;
                }
            }
            fs::rename(entry.path(), dest)?;
        }
        fs::remove_dir(&sub)?;
    }

    Ok(())
}

/// 从预编译包下载并安装 Cavvy
/// 使用 GitHub API 获取最新 release，自动识别平台并下载对应 asset
/// 时间复杂度: O(下载时间)
/// 磁盘IO: 顺序写入，原子重命名
pub fn install_cavvy_prebuilt(
    config: &DownloadConfig,
    dest_dir: Option<&Path>,
) -> SetupResult<Vec<PathBuf>> {
    let (os, arch) = detect_platform();

    eprintln!("[INFO] 通过 GitHub API 获取最新预编译包...");
    let release = fetch_latest_release(&config.cavvy_repo)?;
    eprintln!("[INFO] 最新 release: {}", release.tag_name);

    let asset = match_prebuilt_asset(&release, os, arch)
        .ok_or_else(|| SetupError::NotFound(format!("未找到 {}-{} 平台的预编译包", os, arch)))?;

    eprintln!("[INFO] 匹配 asset: {}", asset.name);

    let install_dir = match dest_dir {
        Some(d) => {
            fs::create_dir_all(d)?;
            d.to_path_buf()
        }
        None => {
            let default = if cfg!(target_os = "windows") {
                std::env::var("LOCALAPPDATA")
                    .map(|d| PathBuf::from(d).join("Cavvy/bin"))
                    .unwrap_or_else(|_| PathBuf::from(r"C:\Cavvy\bin"))
            } else {
                std::env::var("HOME")
                    .map(|d| PathBuf::from(d).join(".local/bin"))
                    .unwrap_or_else(|_| PathBuf::from("/usr/local/bin"))
            };
            fs::create_dir_all(&default)?;
            default
        }
    };

    // 下载到临时目录
    let temp_dir = std::env::temp_dir().join("cavvy-setup-download");
    fs::create_dir_all(&temp_dir)?;
    let archive_path = temp_dir.join(&asset.name);

    eprintln!(
        "[INFO] 下载 {} -> {}",
        asset.browser_download_url,
        archive_path.display()
    );
    download_file(
        &asset.browser_download_url,
        &archive_path,
        config.timeout_seconds,
    )?;

    // 解压
    eprintln!("[INFO] 解压到 {}", install_dir.display());
    if asset.name.ends_with(".7z") {
        extract_7z(&archive_path, &install_dir)?;
    } else if asset.name.ends_with(".tar.xz") {
        extract_tar_xz(&archive_path, &install_dir)?;
    } else {
        return Err(SetupError::VerificationFailed(format!(
            "不支持的压缩格式: {}",
            asset.name
        )));
    }

    // 清理临时文件
    let _ = fs::remove_file(&archive_path);

    // 如果解压后只有一个子目录，将其扁平化
    flatten_single_subdir(&install_dir)?;

    // 验证关键二进制文件
    let bins = [
        "cayc",
        "cay-ir",
        "ir2exe",
        "cay-check",
        "cay-run",
        "cay-lsp",
        "cavly",
        "cay-setup",
    ];
    let ext = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };
    let mut installed = Vec::new();
    let mut missing = Vec::new();

    for bin in &bins {
        let path = install_dir.join(format!("{}{}", bin, ext));
        if path.exists() {
            installed.push(path);
        } else {
            missing.push(bin.to_string());
        }
    }

    if !missing.is_empty() {
        eprintln!("[WARN] 缺少以下二进制文件: {}", missing.join(", "));
    }

    eprintln!(
        "[SUCCESS] 已安装 {} 个二进制文件到 {}",
        installed.len(),
        install_dir.display()
    );

    // 自动添加 PATH
    add_to_path(&install_dir)?;

    Ok(installed)
}

/// 从源码编译 Cavvy（cargo build --release）
/// 时间复杂度: 取决于编译时间
/// 磁盘IO: 写入 target/release 目录
pub fn build_cavvy_from_source() -> SetupResult<PathBuf> {
    let root = find_cavvy_root()
        .ok_or_else(|| SetupError::NotFound("未找到 Cavvy 项目根目录".to_string()))?;

    eprintln!("[INFO] 从源码编译 Cavvy: {}", root.display());

    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--release")
        .current_dir(&root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let status = cmd
        .status()
        .map_err(|e| SetupError::CommandFailed(format!("无法启动 cargo: {}", e)))?;

    if !status.success() {
        return Err(SetupError::CommandFailed(
            "cargo build --release 失败".to_string(),
        ));
    }

    let target_dir = root.join("target/release");
    if !target_dir.exists() {
        return Err(SetupError::VerificationFailed(
            "target/release 目录不存在".to_string(),
        ));
    }

    eprintln!("[SUCCESS] Cavvy 编译完成: {}", target_dir.display());
    Ok(target_dir)
}

/// 从源码编译并安装 Cavvy 二进制文件
pub fn install_cavvy_from_source(dest_dir: Option<&Path>) -> SetupResult<Vec<PathBuf>> {
    let source_dir = build_cavvy_from_source()?;
    let target_dir = match dest_dir {
        Some(d) => {
            fs::create_dir_all(d)?;
            d.to_path_buf()
        }
        None => {
            let default = if cfg!(target_os = "windows") {
                std::env::var("LOCALAPPDATA")
                    .map(|d| PathBuf::from(d).join("Cavvy/bin"))
                    .unwrap_or_else(|_| PathBuf::from(r"C:\Cavvy\bin"))
            } else {
                std::env::var("HOME")
                    .map(|d| PathBuf::from(d).join(".local/bin"))
                    .unwrap_or_else(|_| PathBuf::from("/usr/local/bin"))
            };
            fs::create_dir_all(&default)?;
            default
        }
    };

    let bins = [
        "cayc",
        "cay-ir",
        "ir2exe",
        "cay-check",
        "cay-run",
        "cay-lsp",
        "cavly",
        "cay-setup",
    ];
    let mut installed = Vec::new();
    let ext = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };

    for bin in &bins {
        let src = source_dir.join(format!("{}{}", bin, ext));
        let dst = target_dir.join(format!("{}{}", bin, ext));
        if src.exists() {
            let temp = dst.with_extension("tmp");
            fs::copy(&src, &temp).map_err(|e| {
                SetupError::Io(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    format!("无法复制 {} -> {}: {}", src.display(), dst.display(), e),
                ))
            })?;
            fs::rename(&temp, &dst)?;
            installed.push(dst);
        }
    }

    eprintln!(
        "[SUCCESS] 已安装 {} 个二进制文件到 {}",
        installed.len(),
        target_dir.display()
    );

    // 自动添加 PATH
    add_to_path(&target_dir)?;

    Ok(installed)
}

/// 自动将目录添加到系统 PATH（持久化 + 当前进程生效）
/// Windows: 修改用户环境变量注册表，并设置当前进程 PATH
/// Linux: 修改 ~/.bashrc 或 ~/.zshrc，并设置当前进程 PATH
pub fn add_to_path(dir: &Path) -> SetupResult<()> {
    // 1. 对当前进程立即生效
    // SAFETY: cay-setup 为单线程程序，set_var 不会在并发环境下导致数据竞争
    let dir_str = dir.to_string_lossy().to_string();
    let current = std::env::var("PATH").unwrap_or_default();
    let sep = if cfg!(target_os = "windows") {
        ';'
    } else {
        ':'
    };
    let new_path = format!("{}{}{}", dir_str, sep, current);
    unsafe {
        std::env::set_var("PATH", &new_path);
    }

    // 2. 持久化到系统
    if cfg!(target_os = "windows") {
        add_to_path_windows(dir)
    } else {
        add_to_path_linux(dir)
    }
}

/// Windows: 使用 PowerShell 修改用户 PATH 环境变量
fn add_to_path_windows(dir: &Path) -> SetupResult<()> {
    let dir_str = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let dir_str = dir_str.to_string_lossy().to_string();
    // canonicalize 在 Windows 上会生成 \\?\ 前缀的 UNC 路径，需去除以保持 PATH 可读性
    let dir_str = if dir_str.starts_with(r"\\?\") {
        dir_str.trim_start_matches(r"\\?\").to_string()
    } else {
        dir_str
    };

    // 使用 PowerShell 读取当前用户 PATH
    let ps_read = format!("[Environment]::GetEnvironmentVariable('Path', 'User')");
    let output = Command::new("powershell")
        .args(&["-Command", &ps_read])
        .output()
        .map_err(|e| SetupError::CommandFailed(format!("无法读取用户 PATH: {}", e)))?;

    if !output.status.success() {
        return Err(SetupError::CommandFailed("读取用户 PATH 失败".to_string()));
    }

    let current_path = String::from_utf8_lossy(&output.stdout).trim().to_string();

    // 检查是否已包含
    if current_path
        .to_lowercase()
        .contains(&dir_str.to_lowercase())
    {
        eprintln!("[INFO] PATH 中已包含 {}", dir_str);
        return Ok(());
    }

    // 使用 PowerShell 设置新的用户 PATH
    let new_path = format!("{};{}", dir_str, current_path);
    let ps_write = format!(
        "[Environment]::SetEnvironmentVariable('Path', '{}', 'User')",
        new_path
    );
    let output = Command::new("powershell")
        .args(&["-Command", &ps_write])
        .output()
        .map_err(|e| SetupError::CommandFailed(format!("无法设置用户 PATH: {}", e)))?;

    if !output.status.success() {
        return Err(SetupError::CommandFailed(format!(
            "设置用户 PATH 失败: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    eprintln!("[SUCCESS] 已自动添加 {} 到用户 PATH", dir_str);
    eprintln!("[INFO] 请重新打开终端或注销登录以使 PATH 变更生效。");
    Ok(())
}

/// Linux: 修改 shell profile 添加 PATH
fn add_to_path_linux(dir: &Path) -> SetupResult<()> {
    let dir_str = dir
        .canonicalize()
        .unwrap_or_else(|_| dir.to_path_buf())
        .to_string_lossy()
        .to_string();

    // 检测当前使用的 shell
    let shell = std::env::var("SHELL").unwrap_or_default();
    let home = match std::env::var("HOME") {
        Ok(h) => PathBuf::from(h),
        Err(_) => return Err(SetupError::NotFound("无法确定用户 home 目录".to_string())),
    };
    let profile = if shell.contains("zsh") {
        home.join(".zshrc")
    } else {
        home.join(".bashrc")
    };

    let export_line = format!("export PATH=\"{}:$PATH\"\n", dir_str);

    // 检查是否已包含
    if profile.exists() {
        let content = fs::read_to_string(&profile)?;
        if content.contains(&dir_str) {
            eprintln!("[INFO] PATH 中已包含 {}", dir_str);
            return Ok(());
        }
    }

    // 追加到 profile
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&profile)?;
    file.write_all(export_line.as_bytes())?;
    file.flush()?;
    drop(file);

    eprintln!("[SUCCESS] 已自动添加 {} 到 {}", dir_str, profile.display());
    eprintln!(
        "[INFO] 请运行 'source {}' 或重新打开终端以使变更生效。",
        profile.display()
    );
    Ok(())
}

/// 打印环境变量设置提示（手动模式）
pub fn print_env_setup_hint(install_dir: &Path) {
    let bin_path = install_dir
        .canonicalize()
        .unwrap_or_else(|_| install_dir.to_path_buf());

    if cfg!(target_os = "windows") {
        println!("PowerShell (当前会话):");
        println!("  $env:PATH = \"{};\" + $env:PATH", bin_path.display());
        println!();
        println!("CMD (当前会话):");
        println!("  set PATH={};%PATH%", bin_path.display());
        println!();
        println!("永久设置 (PowerShell 管理员):");
        println!(
            "  [Environment]::SetEnvironmentVariable(\"Path\", \"{};\" + [Environment]::GetEnvironmentVariable(\"Path\", \"User\"), \"User\")",
            bin_path.display()
        );
    } else {
        println!("Bash/Zsh (当前会话):");
        println!("  export PATH=\"{}:$PATH\"", bin_path.display());
        println!();
        println!("永久设置 (添加到 ~/.bashrc 或 ~/.zshrc):");
        println!(
            "  echo 'export PATH=\"{}:$PATH\"' >> ~/.bashrc",
            bin_path.display()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_llvm_download_url_minimal() {
        let config = DownloadConfig::default();
        let url = build_llvm_download_url("22.1.6", "win", "x86_64", &config);
        assert!(url.contains("llvm-minimal"));
        assert!(url.contains("22.1.6"));
    }

    #[test]
    fn test_atomic_rename_nonexistent_fails() {
        let tmp = std::env::temp_dir().join("cavvy_setup_test_nonexistent.tmp");
        let dest = std::env::temp_dir().join("cavvy_setup_test_dest.txt");
        assert!(atomic_rename(&tmp, &dest).is_err());
    }

    #[test]
    fn test_download_config_from_env() {
        let _ = DownloadConfig::default();
    }

    #[test]
    fn test_extract_json_string() {
        let json = r#"{"name":"cavvy-5.1.0-windows-x86_64.7z","url":"https://example.com"}"#;
        assert_eq!(
            extract_json_string(json, "name"),
            Some("cavvy-5.1.0-windows-x86_64.7z".to_string())
        );
        assert_eq!(
            extract_json_string(json, "url"),
            Some("https://example.com".to_string())
        );
        assert_eq!(extract_json_string(json, "missing"), None);
    }

    #[test]
    fn test_split_json_objects() {
        let inner = r#"{"a":1},{"b":2}"#;
        let objs = split_json_objects(inner);
        assert_eq!(objs.len(), 2);
    }

    #[test]
    fn test_match_prebuilt_asset() {
        let release = GitHubRelease {
            tag_name: "v5.1.0".to_string(),
            assets: vec![
                GitHubAsset {
                    name: "cavvy-5.1.0-windows-x86_64.7z".to_string(),
                    browser_download_url: "http://win".to_string(),
                },
                GitHubAsset {
                    name: "cavvy-5.1.0-linux-x86_64.tar.xz".to_string(),
                    browser_download_url: "http://linux".to_string(),
                },
            ],
        };
        assert!(match_prebuilt_asset(&release, "win", "x86_64").is_some());
        assert!(match_prebuilt_asset(&release, "linux", "x86_64").is_some());
        assert!(match_prebuilt_asset(&release, "macos", "x86_64").is_none());
    }

    #[test]
    fn test_is_command_available_git() {
        // git 通常可用
        let has_git = is_command_available("git");
        // 仅验证不 panic
        let _ = has_git;
    }
}
