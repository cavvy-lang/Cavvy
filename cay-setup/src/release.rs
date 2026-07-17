use serde::Deserialize;
#[cfg(windows)]
use std::process::Command;
use std::time::Duration;

use crate::{Error, Result};

pub const DEFAULT_REPOSITORY: &str = "cavvy-lang/Cavvy";

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    pub digest: Option<String>,
}

pub fn api_url(version: Option<&str>) -> String {
    if let Ok(base) = std::env::var("CAVVY_RELEASE_API") {
        let base = base.trim_end_matches('/');
        return match version {
            Some(version) => format!("{base}/tags/v{}", normalize_version(version)),
            None => format!("{base}/latest"),
        };
    }

    let base = format!("https://api.github.com/repos/{DEFAULT_REPOSITORY}/releases");
    match version {
        Some(version) => format!("{base}/tags/v{}", normalize_version(version)),
        None => format!("{base}/latest"),
    }
}

pub fn normalize_version(version: &str) -> &str {
    version.strip_prefix('v').unwrap_or(version)
}

pub fn platform_asset<'a>(release: &'a Release, os: &str, arch: &str) -> Result<&'a Asset> {
    let version = normalize_version(&release.tag_name);
    let expected = format!("cavvy-{version}-{os}-{arch}.tar.xz");
    release
        .assets
        .iter()
        .find(|asset| asset.name == expected)
        .ok_or_else(|| {
            Error::InvalidRelease(format!(
                "{} 中没有当前平台资产 `{expected}`",
                release.tag_name
            ))
        })
}

pub fn verinfo_asset(release: &Release) -> Result<&Asset> {
    release
        .assets
        .iter()
        .find(|asset| asset.name.to_ascii_lowercase().ends_with(".verinfo"))
        .ok_or_else(|| {
            Error::InvalidRelease(format!(
                "{} 没有 .verinfo 资产，无法确定配套 LLVM 版本",
                release.tag_name
            ))
        })
}

pub fn setup_asset<'a>(release: &'a Release, os: &str, arch: &str) -> Option<&'a Asset> {
    let expected = match os {
        "windows" => format!("cay-setup-windows-{arch}.exe"),
        "linux" => format!("cay-setup-linux-{arch}"),
        _ => return None,
    };
    release.assets.iter().find(|asset| asset.name == expected)
}

pub fn sha256(asset: &Asset) -> Result<&str> {
    let digest = asset.digest.as_deref().ok_or_else(|| {
        Error::InvalidRelease(format!("资产 `{}` 没有 SHA-256 digest", asset.name))
    })?;
    digest
        .strip_prefix("sha256:")
        .ok_or_else(|| Error::InvalidRelease(format!("资产 `{}` 的 digest 格式无效", asset.name)))
}

pub fn fetch(version: Option<&str>) -> Result<Release> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("cay-setup/", env!("CAY_SETUP_VERSION")))
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(60))
        .build()?;
    let url = api_url(version);
    match client
        .get(&url)
        .send()
        .and_then(|response| response.error_for_status())
    {
        Ok(response) => Ok(response.json()?),
        Err(primary_error) => fetch_with_platform_fallback(&url, primary_error),
    }
}

#[cfg(windows)]
fn fetch_with_platform_fallback(url: &str, primary_error: reqwest::Error) -> Result<Release> {
    eprintln!("warn: 内置 TLS 无法连接，正在使用 Windows curl 安全回退");
    let output = Command::new("curl.exe")
        .args([
            "-sSLf",
            "--ssl-no-revoke",
            "--connect-timeout",
            "30",
            "--max-time",
            "60",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            concat!("User-Agent: cay-setup/", env!("CAY_SETUP_VERSION")),
            url,
        ])
        .output()
        .map_err(|_| Error::Http(primary_error))?;
    if !output.status.success() {
        return Err(Error::InvalidRelease(format!(
            "GitHub API 请求失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| Error::InvalidRelease(format!("Release JSON 无效: {error}")))
}

#[cfg(not(windows))]
fn fetch_with_platform_fallback(_url: &str, primary_error: reqwest::Error) -> Result<Release> {
    Err(Error::Http(primary_error))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actual_610_release_shape() -> Release {
        Release {
            tag_name: "v6.1.0".to_string(),
            assets: vec![
                Asset {
                    name: "cay-setup-windows-x86_64.exe".to_string(),
                    browser_download_url: "setup".to_string(),
                    size: 6_328_832,
                    digest: Some("sha256:setup".to_string()),
                },
                Asset {
                    name: "cavvy-6.1.0-windows-x86_64.tar.xz".to_string(),
                    browser_download_url: "windows".to_string(),
                    size: 36_817_040,
                    digest: Some(format!("sha256:{}", "a".repeat(64))),
                },
                Asset {
                    name: "delete.name.only.verinfo.verinfo".to_string(),
                    browser_download_url: "verinfo".to_string(),
                    size: 703,
                    digest: Some("sha256:verinfo".to_string()),
                },
            ],
        }
    }

    #[test]
    fn matches_the_real_release_archive_name_not_the_setup_executable() {
        let release = actual_610_release_shape();
        let asset = platform_asset(&release, "windows", "x86_64").unwrap();
        assert_eq!(asset.browser_download_url, "windows");
    }

    #[test]
    fn version_tags_accept_a_leading_v() {
        assert_eq!(normalize_version("v6.1.0"), "6.1.0");
        assert_eq!(normalize_version("6.1.0"), "6.1.0");
    }

    #[test]
    fn requires_github_sha256_digest() {
        let release = actual_610_release_shape();
        assert_eq!(
            sha256(platform_asset(&release, "windows", "x86_64").unwrap()).unwrap(),
            "a".repeat(64)
        );
    }

    #[test]
    fn finds_verinfo_regardless_of_release_prefix() {
        let release = actual_610_release_shape();
        assert_eq!(
            verinfo_asset(&release).unwrap().name,
            "delete.name.only.verinfo.verinfo"
        );
    }

    #[test]
    fn selects_the_separate_windows_bootstrap_asset() {
        let release = actual_610_release_shape();
        assert_eq!(
            setup_asset(&release, "windows", "x86_64")
                .unwrap()
                .browser_download_url,
            "setup"
        );
    }
}
