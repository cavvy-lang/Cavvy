//! cay-setup 二进制入口
//!
//! Cavvy 环境安装与设置工具。
//! 支持环境检查、依赖下载、预编译包安装、源码编译安装。
//! 所有版本信息从 .verinfo 动态读取，通过 GitHub API 获取最新预编译包，
//! 无硬编码版本号，确保兼容未来所有 Cavvy 版本。

use std::env;
use std::path::PathBuf;
use std::process;

use cavvy::setup::check::{CheckStatus, run_full_check};
use cavvy::setup::download::{DownloadConfig, install_cavvy_prebuilt, install_cavvy_from_source, download_and_install_llvm};
use cavvy::setup::{SetupResult, VersionInfo, find_verinfo, parse_verinfo};

const VERSION: &str = env!("CAY_SETUP_VERSION");

fn print_usage() {
    println!("Cavvy Setup {}", VERSION);
    println!("版权所有 (c) 2026, Ethernos Studio");
    println!("使用 GNU 通用公共许可证 版本三 协议开源");
    println!();
    println!("用法: cay-setup <命令> [选项]");
    println!();
    println!("命令:");
    println!("  check          检查当前环境是否满足 Cavvy 编译和运行需求");
    println!("  download       下载并安装 Cavvy 所需的外部依赖 (LLVM minimal)");
    println!("  install        编译并安装 Cavvy 工具链到系统");
    println!("  version        显示 Cavvy 生态各组件版本信息");
    println!("  help           显示此帮助信息");
    println!();
    println!("选项:");
    println!("  --prebuilt       使用 GitHub 预编译包安装（默认优先尝试）");
    println!("  --source         强制从源码编译安装");
    println!("  --use-full-llvm  下载完整 LLVM 开发包 (约 2GB，CI 环境可用)");
    println!("  --dest <目录>    指定安装目标目录 (仅 install 命令有效)");
    println!();
    println!("示例:");
    println!("  cay-setup check");
    println!("  cay-setup download");
    println!("  cay-setup install");
    println!("  cay-setup install --prebuilt");
    println!("  cay-setup install --source --dest C:\\Cavvy\\bin");
    println!("  cay-setup version");
}

fn load_verinfo() -> SetupResult<VersionInfo> {
    let path = find_verinfo().ok_or_else(|| {
        cavvy::setup::SetupError::NotFound("未找到 .verinfo 文件".to_string())
    })?;
    parse_verinfo(path)
}

fn cmd_check() -> SetupResult<()> {
    println!("Cavvy 环境检查");
    println!("{}", "=".repeat(50));

    let report = run_full_check()?;

    for (name, status) in &report.items {
        match status {
            CheckStatus::Ok => {
                println!("[OK]   {}", name);
            }
            CheckStatus::Warning(msg) => {
                println!("[WARN] {} - {}", name, msg);
            }
            CheckStatus::Missing(msg) => {
                println!("[MISS] {} - {}", name, msg);
            }
        }
    }

    println!();
    if report.has_errors() {
        println!("结果: 检测到缺失项，请安装上述依赖后重试。");
        println!("提示: 运行 'cay-setup download' 可自动下载 LLVM minimal。");
        process::exit(1);
    } else if report.all_required_ok() {
        println!("结果: 所有检查项通过，环境就绪。");
    } else {
        println!("结果: 环境基本可用，但存在警告。");
    }

    Ok(())
}

fn cmd_download(use_full_llvm: bool) -> SetupResult<()> {
    println!("Cavvy 依赖下载");
    println!("{}", "=".repeat(50));

    let verinfo = load_verinfo()?;
    let mut config = DownloadConfig::default();
    config.use_full_llvm = use_full_llvm;

    let install_dir = download_and_install_llvm(&verinfo, &config)?;

    println!();
    println!("环境变量设置提示:");
    cavvy::setup::download::print_env_setup_hint(&install_dir);

    Ok(())
}

fn cmd_install(dest_dir: Option<PathBuf>, use_prebuilt: bool, use_source: bool) -> SetupResult<()> {
    println!("Cavvy 工具链安装");
    println!("{}", "=".repeat(50));

    // 1. 先检查环境
    let report = run_full_check()?;
    if report.has_errors() {
        println!("环境检查未通过，请先运行 'cay-setup check' 查看详情。");
        println!("或运行 'cay-setup download' 下载缺失的依赖。");
        process::exit(1);
    }

    let config = DownloadConfig::default();

    // 2. 选择安装方式
    let installed = if use_source {
        // 强制源码编译
        install_cavvy_from_source(dest_dir.as_deref())?
    } else if use_prebuilt || config.use_prebuilt {
        // 优先预编译包
        match install_cavvy_prebuilt(&config, dest_dir.as_deref()) {
            Ok(installed) => installed,
            Err(e) => {
                eprintln!("[WARN] 预编译包安装失败: {}", e);
                eprintln!("[INFO] 回退到源码编译安装...");
                install_cavvy_from_source(dest_dir.as_deref())?
            }
        }
    } else {
        // 默认源码编译
        install_cavvy_from_source(dest_dir.as_deref())?
    };

    println!();
    println!("已安装二进制文件:");
    for path in &installed {
        println!("  {}", path.display());
    }

    println!();
    println!("[SUCCESS] Cavvy 工具链安装完成。");
    println!("[INFO] 请重新打开终端或注销登录以使 PATH 变更生效。");

    Ok(())
}

fn cmd_version() -> SetupResult<()> {
    println!("Cavvy 生态版本信息");
    println!("{}", "=".repeat(50));

    let verinfo = load_verinfo()?;
    let mut components = verinfo.list_components();
    components.sort_by(|a, b| a.0.cmp(b.0));

    for (name, version) in components {
        println!("  {:20} {}", name, version);
    }

    println!();
    println!("cay-setup 版本: {}", VERSION);

    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let command = args[1].as_str();
    let mut use_full_llvm = false;
    let mut use_prebuilt = false;
    let mut use_source = false;
    let mut dest_dir: Option<PathBuf> = None;

    // 解析全局选项
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--prebuilt" => {
                use_prebuilt = true;
            }
            "--source" => {
                use_source = true;
            }
            "--use-full-llvm" => {
                use_full_llvm = true;
            }
            "--dest" => {
                if i + 1 < args.len() {
                    dest_dir = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                } else {
                    eprintln!("错误: --dest 需要参数");
                    process::exit(1);
                }
            }
            "-h" | "--help" => {
                print_usage();
                process::exit(0);
            }
            _ => {
                eprintln!("警告: 未识别的选项: {}", args[i]);
            }
        }
        i += 1;
    }

    let result = match command {
        "check" => cmd_check(),
        "download" => cmd_download(use_full_llvm),
        "install" => cmd_install(dest_dir, use_prebuilt, use_source),
        "version" | "-V" | "--version" => cmd_version(),
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        _ => {
            eprintln!("错误: 未知命令 '{}'", command);
            print_usage();
            process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("[ERROR] {}", e);
        process::exit(1);
    }
}
