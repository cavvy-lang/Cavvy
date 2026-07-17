use std::env;
use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use xz2::read::XzDecoder;

use crate::cli::InstallOptions;
use crate::release::{self, Asset};
use crate::{Error, Result};

pub const VERSION_FILE: &str = "version";
pub const LLVM_VERSION_FILE: &str = "llvm-version";
const ASSETS_REPOSITORY: &str = "cavvy-lang/Cavvy-src-Assets";

#[derive(Debug)]
pub struct InstallSummary {
    pub version: String,
    pub bin_dir: PathBuf,
    pub path_modified: bool,
}

pub fn platform() -> Result<(&'static str, &'static str)> {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        return Err(Error::Unsupported(
            "cay-setup 目前仅支持 Windows 和 Linux".to_string(),
        ));
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else {
        return Err(Error::Unsupported(
            "cay-setup 目前仅支持 x86_64".to_string(),
        ));
    };
    Ok((os, arch))
}

pub fn cavvy_home(explicit: Option<&Path>) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    if let Some(path) = env::var_os("CAVVY_HOME") {
        return Ok(PathBuf::from(path));
    }
    let home =
        env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok_or_else(|| {
            Error::InvalidArgument("无法确定用户主目录；请设置 CAVVY_HOME".to_string())
        })?;
    Ok(PathBuf::from(home).join(".cavvy"))
}

pub fn bin_dir(root: &Path) -> PathBuf {
    installed_version(root)
        .map(|version| {
            let versioned = toolchain_dir(root, &version);
            if versioned.exists() {
                versioned
            } else {
                root.join("bin")
            }
        })
        .unwrap_or_else(|| root.join("bin"))
}

fn toolchain_dir(root: &Path, version: &str) -> PathBuf {
    root.join("toolchains").join(version)
}

pub fn install(options: &InstallOptions) -> Result<InstallSummary> {
    let root = cavvy_home(options.root.as_deref())?;
    let previous_bin = installed_version(&root).map(|_| bin_dir(&root));
    fs::create_dir_all(&root)?;

    let requested = options.version.as_deref();
    eprintln!("info: 正在查询 Cavvy Release...");
    let release = release::fetch(requested)?;
    let (os, arch) = platform()?;
    let asset = release::platform_asset(&release, os, arch)?;
    let expected_sha256 = release::sha256(asset)?;
    let verinfo_asset = release::verinfo_asset(&release)?;
    let verinfo_sha256 = release::sha256(verinfo_asset)?;
    let version = release.tag_name.trim_start_matches('v');
    let bin = toolchain_dir(&root, version);

    if installed_version(&root).as_deref() == Some(version)
        && installed_llvm_version(&root).is_some()
        && validate_installation(&bin).is_ok()
    {
        eprintln!("info: Cavvy {} 已安装，无需重复下载", release.tag_name);
        if options.modify_path {
            add_to_path(&bin)?;
        }
        return Ok(InstallSummary {
            version: version.to_string(),
            bin_dir: bin,
            path_modified: options.modify_path,
        });
    }

    let work = root.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    ));
    let archive = work.join(&asset.name);
    let setup_archive = work.join(if cfg!(windows) {
        "cay-setup.exe"
    } else {
        "cay-setup"
    });
    let verinfo_path = work.join("release.verinfo");
    let backend_archive = work.join("llvm-minimal.tar.xz");
    let libraries_archive = work.join("lib.tar.xz");
    let staging = work.join("unpacked");
    fs::create_dir_all(&staging)?;

    let result: Result<()> = (|| {
        download(verinfo_asset, &verinfo_path, verinfo_sha256)?;
        let llvm_version = llvm_version_from_verinfo(&verinfo_path)?;
        download(asset, &archive, expected_sha256)?;
        eprintln!("info: 正在解压 {}", asset.name);
        extract_archive(&archive, &staging)?;
        install_backend(os, arch, &llvm_version, &backend_archive, &staging)?;
        if os == "windows" {
            install_link_libraries(&libraries_archive, &staging)?;
        }
        install_setup_binary(&release, os, arch, &setup_archive, &staging)?;
        validate_installation(&staging)?;
        activate_toolchain(&staging, &bin)?;
        if options.modify_path {
            switch_path(previous_bin.as_deref(), &bin)?;
        }
        fs::write(root.join(VERSION_FILE), format!("{version}\n"))?;
        fs::write(root.join(LLVM_VERSION_FILE), format!("{llvm_version}\n"))?;
        Ok(())
    })();

    let _ = fs::remove_dir_all(&work);
    result?;

    Ok(InstallSummary {
        version: version.to_string(),
        bin_dir: bin,
        path_modified: options.modify_path,
    })
}

fn download(asset: &Asset, destination: &Path, expected_sha256: &str) -> Result<()> {
    download_url(
        &asset.name,
        &asset.browser_download_url,
        destination,
        Some(asset.size),
        Some(expected_sha256),
    )
}

fn download_url(
    name: &str,
    url: &str,
    destination: &Path,
    expected_size: Option<u64>,
    expected_sha256: Option<&str>,
) -> Result<()> {
    eprintln!(
        "info: 正在下载 {}{}",
        name,
        expected_size
            .map(|size| format!(" ({:.1} MiB)", size as f64 / 1024.0 / 1024.0))
            .unwrap_or_default()
    );
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("cay-setup/", env!("CAY_SETUP_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(600))
        .build()?;
    let request = client
        .get(url)
        .send()
        .and_then(|response| response.error_for_status());
    match request {
        Ok(response) => stream_response(response, destination, expected_size.unwrap_or(0))?,
        Err(primary_error) => download_with_platform_fallback(url, destination, primary_error)?,
    }
    verify_download(destination, expected_size, expected_sha256)
}

fn install_backend(
    os: &str,
    arch: &str,
    version: &str,
    archive: &Path,
    staging: &Path,
) -> Result<()> {
    let os_slug = if os == "windows" { "win" } else { "linux" };
    let bin_name = if os == "windows" { "bin" } else { "bin-linux" };
    let url = format!(
        "https://github.com/{ASSETS_REPOSITORY}/releases/download/llvm-minimal/{version}/{os_slug}-{arch}/bin/{bin_name}.tar.xz"
    );
    let integrity = backend_integrity(os, arch, version)?;
    download_url(
        &format!("LLVM minimal {version}"),
        &url,
        archive,
        Some(integrity.size),
        Some(integrity.sha256),
    )?;
    let destination = staging.join("llvm-minimal").join(bin_name);
    fs::create_dir_all(&destination)?;
    eprintln!("info: 正在安装 LLVM minimal {version}");
    extract_archive(archive, &destination)
}

fn install_link_libraries(archive: &Path, staging: &Path) -> Result<()> {
    const URL: &str =
        "https://github.com/cavvy-lang/Cavvy-src-Assets/releases/download/lib/lib.tar.xz";
    const SIZE: u64 = 16_543_200;
    const SHA256: &str = "60110c339ed34b1bf93dd41514b9d2927f841826998b31c426f64e5054f85a60";
    download_url("Windows 链接库", URL, archive, Some(SIZE), Some(SHA256))?;
    eprintln!("info: 正在安装 Windows 链接库");
    extract_archive(archive, staging)
}

fn install_setup_binary(
    release: &release::Release,
    os: &str,
    arch: &str,
    downloaded: &Path,
    staging: &Path,
) -> Result<()> {
    let canonical_name = if os == "windows" {
        "cay-setup.exe"
    } else {
        "cay-setup"
    };
    if let Some(asset) = release::setup_asset(release, os, arch) {
        download(asset, downloaded, release::sha256(asset)?)?;
        fs::copy(downloaded, staging.join(canonical_name))?;
    } else {
        let current = env::current_exe()?;
        fs::copy(current, staging.join(canonical_name))?;
    }
    Ok(())
}

fn activate_toolchain(staging: &Path, destination: &Path) -> Result<()> {
    let parent = destination.parent().ok_or_else(|| {
        Error::Verification(format!("工具链目录无父目录: {}", destination.display()))
    })?;
    fs::create_dir_all(parent)?;
    if destination.exists() {
        let backup = parent.join(format!(
            ".replaced-{}-{}",
            destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("toolchain"),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis()
        ));
        fs::rename(destination, backup).map_err(|error| {
            Error::Io(std::io::Error::new(
                error.kind(),
                format!(
                    "无法替换正在使用的工具链 {}。请从外部下载的 cay-setup 运行修复: {error}",
                    destination.display()
                ),
            ))
        })?;
    }
    fs::rename(staging, destination)?;
    Ok(())
}

struct AssetIntegrity {
    size: u64,
    sha256: &'static str,
}

fn backend_integrity(os: &str, arch: &str, version: &str) -> Result<AssetIntegrity> {
    match (os, arch, version) {
        ("windows", "x86_64", "22.1.6") => Ok(AssetIntegrity {
            size: 155_283_936,
            sha256: "06277c9fb84b4f23d2c00264dae33d6c6347efe96184f058c236ca0be41626b0",
        }),
        ("linux", "x86_64", "22.1.6") => Ok(AssetIntegrity {
            size: 46_926_736,
            sha256: "0ef496f87279d3f7df72713089e2f56d418bf7c36ad0f0bbbec1cb14cdf615a4",
        }),
        _ => Err(Error::InvalidRelease(format!(
            "LLVM minimal {version} {os}-{arch} 没有受信任的大小和 SHA-256；请先更新 cay-setup 的校验表"
        ))),
    }
}

fn llvm_version_from_verinfo(path: &Path) -> Result<String> {
    let content = fs::read_to_string(path)?;
    let mut section = "";
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            section = &line[1..line.len() - 1];
        } else if section == "LLVM-MINIMAL"
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "version"
        {
            let version = value.trim().trim_matches('"');
            if !version.is_empty() {
                return Ok(version.to_string());
            }
        }
    }
    Err(Error::InvalidRelease(
        ".verinfo 缺少 LLVM-MINIMAL.version".to_string(),
    ))
}

fn stream_response(
    mut response: reqwest::blocking::Response,
    destination: &Path,
    expected_size: u64,
) -> Result<()> {
    let mut file = File::create(destination)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut downloaded = 0_u64;
    let mut next_progress = 10_u64;
    loop {
        let read = response.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
        downloaded += read as u64;
        if expected_size > 0 {
            let percent = downloaded.saturating_mul(100) / expected_size;
            if percent >= next_progress {
                eprintln!("info: 下载进度 {percent}%");
                next_progress = (percent / 10 + 1) * 10;
            }
        }
    }
    file.flush()?;
    Ok(())
}

#[cfg(windows)]
fn download_with_platform_fallback(
    url: &str,
    destination: &Path,
    primary_error: reqwest::Error,
) -> Result<()> {
    eprintln!("warn: 内置 TLS 无法下载，正在使用 Windows curl 安全回退");
    let status = Command::new("curl.exe")
        .args([
            "-sSL",
            "--show-error",
            "--fail",
            "--ssl-no-revoke",
            "--connect-timeout",
            "30",
            "--max-time",
            "600",
            "--output",
        ])
        .arg(destination)
        .arg(url)
        .status()
        .map_err(|_| Error::Http(primary_error))?;
    if !status.success() {
        return Err(Error::InvalidRelease(format!(
            "下载命令失败，退出码 {:?}",
            status.code()
        )));
    }
    Ok(())
}

#[cfg(not(windows))]
fn download_with_platform_fallback(
    _url: &str,
    _destination: &Path,
    primary_error: reqwest::Error,
) -> Result<()> {
    Err(Error::Http(primary_error))
}

fn verify_download(
    destination: &Path,
    expected_size: Option<u64>,
    expected_sha256: Option<&str>,
) -> Result<()> {
    let downloaded = fs::metadata(destination)?.len();
    let mut file = File::open(destination)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if let Some(expected_size) = expected_size
        && downloaded != expected_size
    {
        return Err(Error::Verification(format!(
            "下载大小不符：预期 {} 字节，实际 {} 字节",
            expected_size, downloaded
        )));
    }
    if let Some(expected_sha256) = expected_sha256 {
        let actual = format!("{:x}", hasher.finalize());
        if !actual.eq_ignore_ascii_case(expected_sha256) {
            return Err(Error::Verification(format!(
                "SHA-256 不匹配：预期 {expected_sha256}，实际 {actual}"
            )));
        }
    }
    Ok(())
}

fn extract_archive(archive: &Path, destination: &Path) -> Result<()> {
    let file = File::open(archive)?;
    let decoder = XzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);
    tar.unpack(destination)?;
    Ok(())
}

fn validate_installation(directory: &Path) -> Result<()> {
    let executable = directory.join(if cfg!(windows) { "cayc.exe" } else { "cayc" });
    if !executable.is_file() {
        return Err(Error::Verification(format!(
            "归档缺少 {}",
            executable.display()
        )));
    }
    let setup = directory.join(if cfg!(windows) {
        "cay-setup.exe"
    } else {
        "cay-setup"
    });
    if !setup.is_file() {
        return Err(Error::Verification(format!(
            "安装目录缺少管理器 {}",
            setup.display()
        )));
    }
    if !directory.join("caylibs").is_dir() {
        return Err(Error::Verification("归档缺少 caylibs 标准库".to_string()));
    }
    let backend = backend_bin(directory);
    let required: &[&str] = if cfg!(windows) {
        &["clang.exe", "llc.exe", "lld-link.exe", "ld.lld.exe"]
    } else {
        &["clang", "llc", "ld.lld"]
    };
    for tool in required {
        if !backend.join(tool).is_file() {
            return Err(Error::Verification(format!(
                "LLVM minimal 缺少 {}",
                backend.join(tool).display()
            )));
        }
    }
    if cfg!(windows) && !directory.join("lib/mingw64").is_dir() {
        return Err(Error::Verification("安装包缺少 Windows 链接库".to_string()));
    }
    Ok(())
}

fn backend_bin(directory: &Path) -> PathBuf {
    directory
        .join("llvm-minimal")
        .join(if cfg!(windows) { "bin" } else { "bin-linux" })
}

#[cfg(windows)]
fn powershell_quote(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

pub fn installed_version(root: &Path) -> Option<String> {
    fs::read_to_string(root.join(VERSION_FILE))
        .ok()
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
}

pub fn installed_llvm_version(root: &Path) -> Option<String> {
    fs::read_to_string(root.join(LLVM_VERSION_FILE))
        .ok()
        .map(|version| version.trim().to_string())
        .filter(|version| !version.is_empty())
}

pub fn doctor(root: &Path) -> Result<String> {
    let bin = bin_dir(root);
    validate_installation(&bin)?;
    let clang = backend_bin(&bin).join(if cfg!(windows) { "clang.exe" } else { "clang" });
    let clang_status = Command::new(&clang).arg("--version").status()?;
    if !clang_status.success() {
        return Err(Error::Verification(format!(
            "{} --version 执行失败",
            clang.display()
        )));
    }
    let cayc = bin.join(if cfg!(windows) { "cayc.exe" } else { "cayc" });
    let output = Command::new(&cayc).arg("--version").output()?;
    if !output.status.success() {
        return Err(Error::Verification(format!(
            "{} --version 执行失败",
            cayc.display()
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let version = if stdout.trim().is_empty() {
        stderr.trim()
    } else {
        stdout.trim()
    };
    compile_probe(root, &cayc)?;
    Ok(version.lines().next().unwrap_or("版本未知").to_string())
}

fn compile_probe(root: &Path, cayc: &Path) -> Result<()> {
    let work = root.join(format!(".doctor-{}", std::process::id()));
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work)?;
    let source = work.join("probe.cay");
    let output = work.join(if cfg!(windows) { "probe.exe" } else { "probe" });
    fs::write(&source, "public int main() { return 0; }\n")?;
    let result = Command::new(cayc).args([&source, &output]).output();
    let _ = fs::remove_dir_all(&work);
    let result = result?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(Error::Verification(format!(
            "编译探针失败: {}",
            stderr.trim()
        )));
    }
    Ok(())
}

pub fn uninstall(root: &Path) -> Result<bool> {
    if !root.exists() {
        return Ok(false);
    }
    remove_from_path(&bin_dir(root))?;

    #[cfg(windows)]
    {
        let current = env::current_exe().unwrap_or_default();
        if current.starts_with(root) {
            schedule_remove_tree(root)?;
            return Ok(true);
        }
    }

    fs::remove_dir_all(root)?;
    Ok(true)
}

#[cfg(windows)]
fn schedule_remove_tree(root: &Path) -> Result<()> {
    let script = format!(
        "Start-Sleep -Milliseconds 700; Remove-Item -Recurse -Force -LiteralPath '{}'",
        powershell_quote(root)
    );
    hidden_powershell(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}

fn add_to_path(bin: &Path) -> Result<()> {
    #[cfg(windows)]
    return update_windows_path(bin, true);
    #[cfg(not(windows))]
    return update_shell_profile(bin, true);
}

fn switch_path(previous: Option<&Path>, current: &Path) -> Result<()> {
    #[cfg(windows)]
    return write_windows_path(previous, Some(current));
    #[cfg(not(windows))]
    return update_shell_profile(current, true);
}

fn remove_from_path(bin: &Path) -> Result<()> {
    #[cfg(windows)]
    return update_windows_path(bin, false);
    #[cfg(not(windows))]
    return update_shell_profile(bin, false);
}

#[cfg(windows)]
fn update_windows_path(bin: &Path, add: bool) -> Result<()> {
    write_windows_path(Some(bin), add.then_some(bin))
}

#[cfg(windows)]
fn write_windows_path(remove: Option<&Path>, add: Option<&Path>) -> Result<()> {
    let script = r#"
$target = $env:CAVVY_PATH_TARGET
$remove = $env:CAVVY_PATH_REMOVE
$current = [Environment]::GetEnvironmentVariable('Path', 'User')
$parts = @($current -split ';' | Where-Object { $_ -and $_ -ne $target -and $_ -ne $remove })
if ($target) { $parts += $target }
[Environment]::SetEnvironmentVariable('Path', ($parts -join ';'), 'User')
"#;
    let status = hidden_powershell(script)
        .env("CAVVY_PATH_TARGET", add.unwrap_or_else(|| Path::new("")))
        .env("CAVVY_PATH_REMOVE", remove.unwrap_or_else(|| Path::new("")))
        .status()?;
    if !status.success() {
        return Err(Error::Io(std::io::Error::other("无法修改用户 PATH")));
    }
    Ok(())
}

#[cfg(windows)]
fn hidden_powershell(script: &str) -> Command {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut command = Command::new("powershell.exe");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW);
    command
}

#[cfg(not(windows))]
fn update_shell_profile(bin: &Path, add: bool) -> Result<()> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| Error::InvalidArgument("无法确定 HOME".to_string()))?;
    let shell = env::var("SHELL").unwrap_or_default();
    let profile = if shell.ends_with("zsh") {
        home.join(".zshrc")
    } else {
        home.join(".profile")
    };
    let existing = fs::read_to_string(&profile).unwrap_or_default();
    let cleaned = remove_cavvy_path_block(&existing);
    let content = if add {
        format!(
            "{}{}# >>> cavvy >>>\nexport PATH=\"{}:$PATH\"\n# <<< cavvy <<<\n",
            cleaned,
            if cleaned.is_empty() || cleaned.ends_with('\n') {
                ""
            } else {
                "\n"
            },
            bin.display()
        )
    } else {
        cleaned
    };
    fs::write(profile, content)?;
    Ok(())
}

#[cfg(any(not(windows), test))]
fn remove_cavvy_path_block(content: &str) -> String {
    let mut output = Vec::new();
    let mut skipping = false;
    for line in content.lines() {
        if line == "# >>> cavvy >>>" {
            skipping = true;
            continue;
        }
        if line == "# <<< cavvy <<<" {
            skipping = false;
            continue;
        }
        if !skipping {
            output.push(line);
        }
    }
    let mut cleaned = output.join("\n");
    if content.ends_with('\n') && !cleaned.is_empty() {
        cleaned.push('\n');
    }
    cleaned
}

pub fn path_contains(bin: &Path) -> bool {
    let path = env::var_os("PATH").unwrap_or_else(|| OsString::from(""));
    env::split_paths(&path).any(|entry| paths_equal(&entry, bin))
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    if cfg!(windows) {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    } else {
        left == right
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_layout_keeps_resources_next_to_executables() {
        assert_eq!(
            toolchain_dir(Path::new("/home/me/.cavvy"), "6.1.0"),
            Path::new("/home/me/.cavvy/toolchains/6.1.0")
        );
    }

    #[test]
    fn removes_only_the_managed_profile_block() {
        let profile = "export EDITOR=vim\n# >>> cavvy >>>\nexport PATH=\"/old:$PATH\"\n# <<< cavvy <<<\nexport LANG=C\n";
        assert_eq!(
            remove_cavvy_path_block(profile),
            "export EDITOR=vim\nexport LANG=C\n"
        );
    }

    #[test]
    fn validates_the_release_layout() {
        let root = env::temp_dir().join(format!("cay-setup-layout-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("caylibs")).unwrap();
        fs::create_dir_all(backend_bin(&root)).unwrap();
        if cfg!(windows) {
            fs::create_dir_all(root.join("lib/mingw64")).unwrap();
        }
        fs::write(
            root.join(if cfg!(windows) { "cayc.exe" } else { "cayc" }),
            b"test",
        )
        .unwrap();
        fs::write(
            root.join(if cfg!(windows) {
                "cay-setup.exe"
            } else {
                "cay-setup"
            }),
            b"test",
        )
        .unwrap();
        for tool in if cfg!(windows) {
            &["clang.exe", "llc.exe", "lld-link.exe", "ld.lld.exe"][..]
        } else {
            &["clang", "llc", "ld.lld"][..]
        } {
            fs::write(backend_bin(&root).join(tool), b"test").unwrap();
        }
        assert!(validate_installation(&root).is_ok());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn parses_llvm_version_from_release_metadata() {
        let path = env::temp_dir().join(format!("cay-setup-verinfo-test-{}", std::process::id()));
        fs::write(
            &path,
            "[CAYC]\nversion=6.1.0\n[LLVM-MINIMAL]\nversion=22.1.6\n",
        )
        .unwrap();
        assert_eq!(llvm_version_from_verinfo(&path).unwrap(), "22.1.6");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn refuses_unpinned_backend_assets() {
        assert!(backend_integrity("windows", "x86_64", "99.0.0").is_err());
    }

    #[test]
    fn activates_a_complete_staging_directory_atomically() {
        let root = env::temp_dir().join(format!("cay-setup-activate-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let staging = root.join("staging");
        let destination = root.join("toolchains/6.1.0");
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("marker"), b"complete").unwrap();
        activate_toolchain(&staging, &destination).unwrap();
        assert!(!staging.exists());
        assert_eq!(fs::read(destination.join("marker")).unwrap(), b"complete");
        fs::remove_dir_all(root).unwrap();
    }
}
