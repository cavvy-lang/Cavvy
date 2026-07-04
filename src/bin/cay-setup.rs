//! cay-setup 二进制入口
//!
//! Cavvy 环境安装与设置工具（交互式命令行）。
//! 启动后显示交互菜单，用户通过输入数字选择操作。
//! 所有版本信息从 .verinfo 动态读取，通过 GitHub API 获取最新预编译包，
//! 无硬编码版本号，确保兼容未来所有 Cavvy 版本。

use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::process;

use cavvy::setup::check::{CheckStatus, run_full_check};
use cavvy::setup::download::{
    DownloadConfig, download_and_install_llvm, find_cavvy_install_dir, get_tool_version,
    install_cavvy_from_source, install_cavvy_prebuilt,
};
use cavvy::setup::{SetupResult, VersionInfo, find_verinfo, parse_verinfo};

const VERSION: &str = env!("CAY_SETUP_VERSION");

fn print_banner() {
    println!("{}", "=".repeat(50));
    println!("{:^50}", format!("Cavvy Setup {}", VERSION));
    println!("{}", "=".repeat(50));
    println!();
}

fn wait_enter() {
    print!("\n按 [Enter] 键返回主菜单...");
    let _ = io::stdout().flush();
    let mut buf = String::new();
    let _ = io::stdin().read_line(&mut buf);
}

fn read_line_trim() -> String {
    let mut buf = String::new();
    let _ = io::stdin().read_line(&mut buf);
    buf.trim().to_string()
}

fn ask_yn(prompt: &str, default_yes: bool) -> bool {
    let hint = if default_yes { "[Y/n]" } else { "[y/N]" };
    print!("{} {}: ", prompt, hint);
    let _ = io::stdout().flush();
    let input = read_line_trim().to_lowercase();
    if input.is_empty() {
        return default_yes;
    }
    input.starts_with('y')
}

fn load_verinfo() -> SetupResult<VersionInfo> {
    let path = find_verinfo()
        .ok_or_else(|| cavvy::setup::SetupError::NotFound("未找到 .verinfo 文件".to_string()))?;
    parse_verinfo(path)
}

fn menu_check() -> SetupResult<()> {
    println!("Cavvy 环境检查");
    println!("{}", "-".repeat(50));

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
        println!("提示: 选择主菜单 [2] 可自动下载 LLVM minimal。");
    } else if report.all_required_ok() {
        println!("结果: 所有检查项通过，环境就绪。");
    } else {
        println!("结果: 环境基本可用，但存在警告。");
    }

    Ok(())
}

fn menu_download() -> SetupResult<()> {
    loop {
        println!("下载选项");
        println!("{}", "-".repeat(50));
        println!("[1] 下载 LLVM minimal（推荐，约 100MB）");
        println!("[2] 下载完整 LLVM 开发包（约 2GB，CI 环境可用）");
        println!("[3] 返回上级菜单");
        print!("\n请输入选项编号 (1-3): ");
        let _ = io::stdout().flush();

        match read_line_trim().as_str() {
            "1" => {
                println!();
                println!("开始下载 LLVM minimal...");
                let verinfo = load_verinfo()?;
                let config = DownloadConfig::default();
                let install_dir = download_and_install_llvm(&verinfo, &config, None)?;
                println!();
                println!("环境变量设置提示:");
                cavvy::setup::download::print_env_setup_hint(&install_dir);
                return Ok(());
            }
            "2" => {
                if ask_yn("下载完整 LLVM 约 2GB，确认继续", false) {
                    println!();
                    println!("开始下载完整 LLVM...");
                    let verinfo = load_verinfo()?;
                    let mut config = DownloadConfig::default();
                    config.use_full_llvm = true;
                    let install_dir = download_and_install_llvm(&verinfo, &config, None)?;
                    println!();
                    println!("环境变量设置提示:");
                    cavvy::setup::download::print_env_setup_hint(&install_dir);
                } else {
                    println!("已取消。");
                }
                return Ok(());
            }
            "3" => return Ok(()),
            "" | _ => {
                println!("无效输入，请重新选择。");
                println!();
            }
        }
    }
}

fn menu_install() -> SetupResult<()> {
    println!("Cavvy 工具链安装");
    println!("{}", "-".repeat(50));

    // 1. 先检查环境
    let report = run_full_check()?;
    if report.has_errors() {
        println!("环境检查未通过，请先选择主菜单 [1] 查看详情。");
        println!("或选择主菜单 [2] 下载缺失的依赖。");
        return Ok(());
    }

    // 2. 询问安装目录
    print!("请输入安装目标目录 [直接回车使用默认 PATH 目录]: ");
    let _ = io::stdout().flush();
    let dest_input = read_line_trim();
    let dest_dir: Option<PathBuf> = if dest_input.is_empty() {
        None
    } else {
        Some(PathBuf::from(dest_input))
    };

    // 3. 安装方式子菜单
    loop {
        println!();
        println!("安装选项");
        println!("{}", "-".repeat(50));
        println!("[1] 自动选择（优先预编译包，失败回退源码编译）");
        println!("[2] 强制使用 GitHub 预编译包");
        println!("[3] 强制从源码编译");
        println!("[4] 取消安装");
        print!("\n请输入选项编号 (1-4): ");
        let _ = io::stdout().flush();

        let config = DownloadConfig::default();
        let result = match read_line_trim().as_str() {
            "1" => {
                println!();
                println!("正在尝试安装（自动选择模式）...");
                match install_cavvy_prebuilt(&config, dest_dir.as_deref()) {
                    Ok(installed) => Ok(installed),
                    Err(e) => {
                        eprintln!("[WARN] 预编译包安装失败: {}", e);
                        if ask_yn("是否回退到源码编译", true) {
                            install_cavvy_from_source(dest_dir.as_deref())
                        } else {
                            return Ok(());
                        }
                    }
                }
            }
            "2" => {
                println!();
                println!("正在使用预编译包安装...");
                install_cavvy_prebuilt(&config, dest_dir.as_deref())
            }
            "3" => {
                println!();
                println!("正在从源码编译安装...");
                install_cavvy_from_source(dest_dir.as_deref())
            }
            "4" => {
                println!("已取消安装。");
                return Ok(());
            }
            "" | _ => {
                println!("无效输入，请重新选择。");
                continue;
            }
        };

        match result {
            Ok(installed) => {
                println!();
                println!("已安装二进制文件:");
                for path in &installed {
                    println!("  {}", path.display());
                }
                println!();
                println!("[SUCCESS] Cavvy 工具链安装完成。");
                println!("[INFO] 请重新打开终端或注销登录以使 PATH 变更生效。");

                // 安装成功后，若 Cavvy 安装目录下缺少 llvm-minimal，提示用户下载
                let cavvy_dir = dest_dir.clone().or_else(find_cavvy_install_dir);
                if let Some(ref dir) = cavvy_dir {
                    if !dir.join("llvm-minimal").exists() {
                        println!();
                        if ask_yn(
                            "检测到当前 Cavvy 安装目录缺少 LLVM minimal，是否立即下载",
                            true,
                        ) {
                            println!();
                            let verinfo = load_verinfo()?;
                            let config = DownloadConfig::default();
                            match download_and_install_llvm(&verinfo, &config, Some(dir)) {
                                Ok(llvm_dir) => {
                                    println!(
                                        "[SUCCESS] LLVM minimal 已安装到: {}",
                                        llvm_dir.display()
                                    );
                                }
                                Err(e) => {
                                    eprintln!("[WARN] LLVM minimal 下载失败: {}", e);
                                }
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[ERROR] 安装失败: {}", e);
            }
        }
        return Ok(());
    }
}

fn menu_version() -> SetupResult<()> {
    println!("Cavvy 生态版本信息");
    println!("{}", "-".repeat(50));

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

fn menu_version_tools() -> SetupResult<()> {
    println!("版本管理");
    println!("{}", "-".repeat(50));

    let tools = [
        "cayc",
        "cay-ir",
        "ir2exe",
        "cay-check",
        "cay-run",
        "cay-setup",
    ];
    let mut found_any = false;

    for tool in &tools {
        match get_tool_version(tool) {
            Ok((path, version)) => {
                found_any = true;
                println!("  {:15} {}", tool, version);
                println!("    路径: {}", path.display());
            }
            Err(_) => {
                println!("  {:15} 未安装", tool);
            }
        }
    }

    if !found_any {
        println!("未检测到任何 Cavvy 工具链二进制文件。");
        println!("请运行主菜单 [3] 安装 Cavvy 工具链。");
    }

    Ok(())
}

fn menu_help() {
    println!("帮助信息");
    println!("{}", "-".repeat(50));
    println!("cay-setup 是 Cavvy 编译器生态的一站式环境配置工具。");
    println!();
    println!("主菜单功能说明:");
    println!("  [1] 检查环境 - 检测 Git、Rust、LLVM 等依赖是否就绪");
    println!("  [2] 下载依赖 - 自动下载并安装 LLVM minimal 或完整包");
    println!("  [3] 安装工具链 - 自动下载或编译 Cavvy 并添加到 PATH");
    println!("  [4] 版本信息 - 显示 .verinfo 中的组件版本号");
    println!("  [5] 版本管理 - 显示 PATH 中各二进制文件路径及版本");
    println!("  [6] 帮助 - 显示此说明");
    println!("  [7] 退出 - 退出程序");
    println!();
    println!("提示:");
    println!("  - 首次使用建议依次执行 [1] -> [2] -> [3]");
    println!("  - 安装过程需要网络连接，用于访问 GitHub API 和 Release");
    println!("  - Windows 安装完成后请重新打开终端以更新 PATH");
}

fn run_interactive() {
    loop {
        print_banner();
        println!("[1] 检查当前环境");
        println!("[2] 下载并安装依赖 (LLVM minimal)");
        println!("[3] 安装 Cavvy 工具链");
        println!("[4] 显示版本信息 (.verinfo)");
        println!("[5] 版本管理 (已安装二进制)");
        println!("[6] 帮助");
        println!("[7] 退出");
        print!("\n请输入选项编号 (1-7): ");
        let _ = io::stdout().flush();

        let choice = read_line_trim();
        println!();

        let result = match choice.as_str() {
            "1" => menu_check(),
            "2" => menu_download(),
            "3" => menu_install(),
            "4" => menu_version(),
            "5" => menu_version_tools(),
            "6" => {
                menu_help();
                Ok(())
            }
            "7" | "q" | "quit" | "exit" => {
                println!("感谢使用 Cavvy Setup，再见！");
                process::exit(0);
            }
            "" | _ => {
                println!("无效输入，请重新选择。");
                Ok(())
            }
        };

        if let Err(e) = result {
            eprintln!("[ERROR] {}", e);
        }

        wait_enter();
        println!();
    }
}

fn main() {
    // 为兼容可能的脚本调用，保留对 --help 和 --version 的快速响应
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "-h" | "--help" => {
                menu_help();
                return;
            }
            "-V" | "--version" => {
                println!("cay-setup {}", VERSION);
                return;
            }
            _ => {
                println!("cay-setup 已改为交互式运行，请直接执行: cay-setup");
                println!("如需查看版本: cay-setup --version");
                println!("如需查看帮助: cay-setup --help");
                process::exit(1);
            }
        }
    }

    run_interactive();
}
