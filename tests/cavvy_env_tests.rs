//! cay-setup 集成测试
//!
//! 验证 cay-setup 核心逻辑（环境检查、版本解析、下载匹配）在真实环境中正确运行。
//! 注意：Windows UAC 会拦截文件名含 "setup" 的可执行文件，故不直接调用二进制，
//! 而是测试库公共 API，确保与二进制行为一致。

use cavvy::setup::check::{CheckStatus, run_full_check};
use cavvy::setup::download::{DownloadConfig, GitHubAsset, GitHubRelease, match_prebuilt_asset};
use cavvy::setup::{find_verinfo, parse_verinfo, detect_platform, is_ci_environment};

#[test]
fn test_setup_version_parsing() {
    let path = find_verinfo().expect("应在工作区中找到 .verinfo");
    let verinfo = parse_verinfo(&path).expect("应能解析 .verinfo");

    assert!(
        verinfo.cavvy_version().is_some(),
        ".verinfo 应包含 Cavvy 版本号"
    );
    assert!(
        verinfo.llvm_minimal_version().is_some(),
        ".verinfo 应包含 LLVM-MINIMAL 版本号"
    );
}

#[test]
fn test_setup_check_runs_without_panic() {
    let report = run_full_check().expect("环境检查不应 panic");
    // 检查输出至少包含若干项
    assert!(!report.items.is_empty(), "报告不应为空");
}

#[test]
fn test_setup_check_report_status_variants() {
    let mut report = run_full_check().expect("环境检查不应 panic");
    // 强制插入各种状态，确保 has_errors / all_required_ok 逻辑正确
    report.items.push(("模拟-OK".to_string(), CheckStatus::Ok));
    report.items.push(("模拟-Warn".to_string(), CheckStatus::Warning("测试警告".to_string())));
    report.items.push(("模拟-Miss".to_string(), CheckStatus::Missing("测试缺失".to_string())));

    assert!(report.has_errors(), "包含 Missing 时 has_errors 应为 true");
    assert!(!report.all_required_ok(), "包含 Missing 时 all_required_ok 应为 false");

    // 移除 Missing 和 Warning，仅保留 Ok
    report.items.retain(|(_, s)| !matches!(s, CheckStatus::Missing(_)));
    assert!(!report.has_errors(), "无 Missing 时 has_errors 应为 false");
    assert!(!report.all_required_ok(), "仍有 Warning 时 all_required_ok 应为 false");

    report.items.retain(|(_, s)| !matches!(s, CheckStatus::Warning(_)));
    assert!(report.all_required_ok(), "全为 Ok 时 all_required_ok 应为 true");
}

#[test]
fn test_setup_detect_platform() {
    let (os, arch) = detect_platform();
    assert!(
        os == "win" || os == "linux" || os == "macos",
        "应检测到已知操作系统: got {}",
        os
    );
    assert!(
        !arch.is_empty(),
        "架构不应为空"
    );
}

#[test]
fn test_setup_ci_detection_does_not_panic() {
    // 仅确保不 panic，返回值取决于环境
    let _ = is_ci_environment();
}

#[test]
fn test_setup_download_config_default() {
    let config = DownloadConfig::default();
    assert!(config.use_prebuilt, "默认应优先使用预编译包");
    assert!(!config.use_full_llvm, "默认不应使用完整 LLVM");
    assert!(!config.use_full_llvm, "默认不应使用完整 LLVM");
}

#[test]
fn test_setup_match_prebuilt_asset_windows() {
    let release = GitHubRelease {
        tag_name: "v5.1.0".to_string(),
        assets: vec![
            GitHubAsset {
                name: "cavvy-5.1.0-windows-x86_64.7z".to_string(),
                browser_download_url: "https://example.com/win.7z".to_string(),
            },
            GitHubAsset {
                name: "cavvy-5.1.0-linux-x86_64.tar.xz".to_string(),
                browser_download_url: "https://example.com/linux.tar.xz".to_string(),
            },
            GitHubAsset {
                name: "source.zip".to_string(),
                browser_download_url: "https://example.com/src.zip".to_string(),
            },
        ],
    };

    let win = match_prebuilt_asset(&release, "win", "x86_64");
    assert!(win.is_some(), "应匹配 Windows 预编译包");
    assert!(win.unwrap().name.contains("windows"));

    let linux = match_prebuilt_asset(&release, "linux", "x86_64");
    assert!(linux.is_some(), "应匹配 Linux 预编译包");
    assert!(linux.unwrap().name.contains("linux"));

    assert!(
        match_prebuilt_asset(&release, "macos", "x86_64").is_none(),
        "macos 不应匹配任何包"
    );
    assert!(
        match_prebuilt_asset(&release, "win", "aarch64").is_some(),
        "当前实现暂不按架构精确过滤，aarch64 仍应返回 windows 包"
    );
}

#[test]
fn test_setup_match_prebuilt_asset_empty() {
    let release = GitHubRelease {
        tag_name: "v1.0.0".to_string(),
        assets: vec![],
    };
    assert!(match_prebuilt_asset(&release, "win", "x86_64").is_none());
}
