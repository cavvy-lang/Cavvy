use std::env;
use std::process;

use anyhow::{Context, Result};

use cavvy::cavly::audit::{AuditLogEntry, AuditLogger, SecurityEventType};
use cavvy::cavly::registry::SecureRegistry;
use cavvy::cavly::security::{sha256_hex, SecurityLevel};

// Cavly 版本 - 与 Cavvy 版本保持一致
const VERSION: &str = env!("CAVLY_VERSION");

/// 打印使用帮助
fn print_usage() {
    println!("Cavvy 包管理器 {}", VERSION);
    println!("版权所有 (c) 2026, Ethernos Studio");
    println!("使用 GNU 通用公共许可证 版本三 协议开源");
    println!();
    println!("用法: cavly <命令> [选项]");
    println!();
    println!("选项:");
    println!("  -v, --verbose     显示详细输出");
    println!("  -V, --version     显示版本号");
    println!("  -h, --help        显示帮助信息");
    println!();
    println!("命令:");
    println!("  init [名称]       初始化新可执行项目");
    println!("  init --lib [名称] 初始化新库项目");
    println!("  build             构建项目（默认构建所有 bin）");
    println!("  build --bin <名称> 只构建指定的二进制目标");
    println!("  clean             清理构建产物");
    println!("  run               构建并运行项目");
    println!("  run --bin <名称>  运行指定的二进制目标");
    println!("  test              编译并运行所有测试");
    println!("  test --filter <名称> 按名称过滤测试");
    println!("  info              显示项目信息");
    println!("  add <库>          添加系统库依赖");
    println!("  ffi <名称> <库>   添加 FFI 库配置");
    println!("  verify <包名>     验证包的安全证书和完整性");
    println!("  trust <公钥B64>   添加可信公钥到配置");
    println!("  audit-log         显示安全审计日志");
    println!("  help              显示此帮助信息");
    println!();
    println!("示例:");
    println!("  cavly init my-project");
    println!("  cavly init --lib my-library");
    println!("  cavly build");
    println!("  cavly build -v");
    println!("  cavly build --bin my-tool");
    println!("  cavly run");
    println!("  cavly run --bin my-tool");
    println!("  cavly test");
    println!("  cavly test --filter basic");
    println!("  cavly add m");
    println!("  cavly ffi sdl2 SDL2");
}

/// 从参数列表中提取指定标志后面的值
/// 例如 extract_flag_value(&args, "--bin") / extract_flag_value(&args, "--filter")
fn extract_flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|pos| args.get(pos + 1))
        .cloned()
}

/// 主函数
///
/// # 复杂度
/// - 时间: O(n)，n 为命令处理复杂度
/// - 空间: O(1) 额外空间
fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    let command = &args[1];
    let verbose = args.contains(&"-v".to_string()) || args.contains(&"--verbose".to_string());

    let result = match command.as_str() {
        "init" => cmd_init(&args),
        "build" => {
            let bin_name = extract_flag_value(&args, "--bin");
            cmd_build(verbose, bin_name)
        }
        "clean" => cmd_clean(verbose),
        "run" => {
            let bin_name = extract_flag_value(&args, "--bin");
            cmd_run(verbose, bin_name)
        }
        "test" => {
            let filter = extract_flag_value(&args, "--filter");
            cmd_test(verbose, filter)
        }
        "info" => cmd_info(),
        "add" => cmd_add(&args),
        "ffi" => cmd_ffi(&args),
        "verify" => cmd_verify(&args),
        "trust" => cmd_trust(&args),
        "audit-log" => cmd_audit_log(),
        "help" | "-h" | "--help" => {
            print_usage();
            Ok(())
        }
        "-V" | "--version" => {
            println!("Cavly v{}", VERSION);
            Ok(())
        }
        _ => {
            eprintln!("错误: 未知命令 '{}'", command);
            print_usage();
            process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("错误: {:#}", e);
        process::exit(1);
    }
}

/// 初始化新项目
///
/// # 复杂度
/// - 时间: O(1)
/// - 空间: O(1)
fn cmd_init(args: &[String]) -> Result<()> {
    use cavvy::cavly::config::ProjectType;

    // 解析参数
    let is_lib = args.contains(&"--lib".to_string()) || args.contains(&"-l".to_string());
    let project_type = if is_lib {
        ProjectType::Lib
    } else {
        ProjectType::Bin
    };

    // 找到项目名称参数（跳过 --lib 等选项）
    let project_name = args
        .iter()
        .skip(2)
        .find(|arg| !arg.starts_with('-'))
        .map(|s| s.as_str());

    let project_path = if let Some(name) = project_name {
        env::current_dir()?.join(name)
    } else {
        env::current_dir()?
    };

    cavvy::cavly::project::Project::init(&project_path, project_name, project_type)?;

    Ok(())
}

/// 构建项目
///
/// # 复杂度
/// - 时间: O(n + m)，n 为源码大小，m 为链接复杂度
/// - 空间: O(n)
fn cmd_build(verbose: bool, bin_name: Option<String>) -> Result<()> {
    println!("Cavvy 包管理器 {}", VERSION);
    println!("版权所有 (c) 2026, Ethernos Studio");
    println!("使用 GNU 通用公共许可证 版本三 协议开源");

    let current_dir = env::current_dir()?;

    // 查找项目根目录
    let project_root = cavvy::cavly::find_project_root(&current_dir)
        .ok_or_else(|| anyhow::anyhow!("当前目录不是 Cavly 项目（找不到 cavly.toml）"))?;

    if verbose {
        println!("Cavly: 项目根目录: {}", project_root.display());
    }

    // 加载配置
    let config_path = project_root.join("cavly.toml");
    let config = cavvy::cavly::config::CavlyConfig::from_file(&config_path)?;

    if verbose {
        let type_str = if config.is_lib() {
            "库"
        } else {
            "可执行程序"
        };
        println!(
            "Cavly: 项目: {} v{} ({})",
            config.package.name, config.package.version, type_str
        );

        // 显示 bin 目标
        let bins = config.effective_bins();
        if !bins.is_empty() {
            println!("Cavly: 二进制目标:");
            for bin in &bins {
                println!("  - {} ({})", bin.name, bin.path);
            }
        }

        if !config.dependencies.is_empty() {
            println!(
                "Cavly: 依赖: {}",
                config
                    .dependencies
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        if !config.workspace.members.is_empty() {
            println!("Cavly: 工作区成员: {}", config.workspace.members.join(", "));
        }
    }

    // 构建
    let mut builder =
        cavvy::cavly::builder::Builder::with_dependencies(project_root.clone(), config)?
            .verbose(verbose);

    match bin_name {
        Some(name) => {
            let output = builder.build_bin_by_name(&name)?;
            println!("构建成功: {}", output.display());
        }
        None => {
            let outputs = builder.build_all_bins()?;
            if outputs.len() == 1 {
                println!("构建成功: {}", outputs[0].display());
            } else {
                println!("构建成功: {} 个目标", outputs.len());
                for output in &outputs {
                    println!("  {}", output.display());
                }
            }
        }
    }

    Ok(())
}

/// 清理构建产物
///
/// # 复杂度
/// - 时间: O(1)
/// - 空间: O(1)
fn cmd_clean(verbose: bool) -> Result<()> {
    let current_dir = env::current_dir()?;

    let project_root = cavvy::cavly::find_project_root(&current_dir)
        .ok_or_else(|| anyhow::anyhow!("当前目录不是 Cavly 项目（找不到 cavly.toml）"))?;

    let config_path = project_root.join("cavly.toml");
    let config = cavvy::cavly::config::CavlyConfig::from_file(&config_path)?;

    let builder = cavvy::cavly::builder::Builder::new(project_root, config).verbose(verbose);

    builder.clean()?;
    println!("清理完成");

    Ok(())
}

/// 构建并运行项目
///
/// # 复杂度
/// - 时间: O(n + m) + 运行时间
/// - 空间: O(n)
fn cmd_run(verbose: bool, bin_name: Option<String>) -> Result<()> {
    // 先构建
    cmd_build(verbose, bin_name.clone())?;

    let current_dir = env::current_dir()?;
    let project_root = cavvy::cavly::find_project_root(&current_dir)
        .ok_or_else(|| anyhow::anyhow!("当前目录不是 Cavly 项目（找不到 cavly.toml）"))?;

    let config_path = project_root.join("cavly.toml");
    let config = cavvy::cavly::config::CavlyConfig::from_file(&config_path)?;

    // 确定要运行的可执行文件
    let target_dir = project_root.join(&config.package.target_dir);

    let exe_name = if let Some(ref name) = bin_name {
        name.clone()
    } else {
        // 使用默认的 output_filename
        config
            .build
            .output_name
            .clone()
            .unwrap_or_else(|| config.package.name.clone())
    };

    let exe_path = if config
        .build
        .target
        .as_ref()
        .map(|t| t.contains("windows") || t.contains("mingw"))
        .unwrap_or(cfg!(target_os = "windows"))
    {
        target_dir.join(format!("{}.exe", exe_name))
    } else {
        target_dir.join(&exe_name)
    };

    if !exe_path.exists() {
        anyhow::bail!(
            "可执行文件不存在: {} (尝试先 cavly build)",
            exe_path.display()
        );
    }

    if verbose {
        println!("Cavly: 运行: {}", exe_path.display());
    }

    // 运行
    let status = std::process::Command::new(&exe_path)
        .status()
        .with_context(|| format!("运行失败: {}", exe_path.display()))?;

    if !status.success() {
        anyhow::bail!("程序退出码: {:?}", status.code());
    }

    Ok(())
}

/// 编译并运行测试
///
/// # 复杂度
/// - 时间: O(n*m)，n 为测试数，m 为每个测试的编译 + 运行时间
/// - 空间: O(n) 测试结果
fn cmd_test(verbose: bool, filter: Option<String>) -> Result<()> {
    println!("Cavvy 包管理器 {}", VERSION);
    println!("版权所有 (c) 2026, Ethernos Studio");
    println!("使用 GNU 通用公共许可证 版本三 协议开源");

    let current_dir = env::current_dir()?;

    let project_root = cavvy::cavly::find_project_root(&current_dir)
        .ok_or_else(|| anyhow::anyhow!("当前目录不是 Cavly 项目（找不到 cavly.toml）"))?;

    let config_path = project_root.join("cavly.toml");
    let config = cavvy::cavly::config::CavlyConfig::from_file(&config_path)?;

    if verbose {
        println!(
            "Cavly: 测试项目: {} v{}",
            config.package.name, config.package.version
        );

        let tests = config.discover_tests(&project_root);
        println!("Cavly: 发现 {} 个测试目标", tests.len());
        for test in &tests {
            println!(
                "  - {} ({}) [harness: {}]",
                test.name, test.path, test.harness
            );
        }
    }

    let runner = cavvy::cavly::tester::TestRunner::new(project_root, config)
        .verbose(verbose)
        .filter(filter);

    let summary = runner.run()?;

    if !summary.is_success() {
        anyhow::bail!("{} 个测试失败", summary.failed);
    }

    Ok(())
}

/// 显示项目信息
///
/// # 复杂度
/// - 时间: O(1)
/// - 空间: O(1)
fn cmd_info() -> Result<()> {
    let current_dir = env::current_dir()?;

    let project_root = cavvy::cavly::find_project_root(&current_dir)
        .ok_or_else(|| anyhow::anyhow!("当前目录不是 Cavly 项目（找不到 cavly.toml）"))?;

    let info = cavvy::cavly::project::Project::info(&project_root)?;
    info.print();

    Ok(())
}

/// 添加系统库依赖
///
/// # 复杂度
/// - 时间: O(1)
/// - 空间: O(1)
fn cmd_add(args: &[String]) -> Result<()> {
    let lib_name = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("请指定库名，例如: cavly add m"))?;

    let current_dir = env::current_dir()?;
    let project_root = cavvy::cavly::find_project_root(&current_dir)
        .ok_or_else(|| anyhow::anyhow!("当前目录不是 Cavly 项目（找不到 cavly.toml）"))?;

    cavvy::cavly::project::Project::add_system_lib(&project_root, lib_name)?;

    Ok(())
}

/// 添加 FFI 库配置
///
/// # 复杂度
/// - 时间: O(1)
/// - 空间: O(1)
fn cmd_ffi(args: &[String]) -> Result<()> {
    let name = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("请指定库配置名称，例如: cavly ffi sdl2 SDL2"))?;

    let lib = args
        .get(3)
        .ok_or_else(|| anyhow::anyhow!("请指定库名，例如: cavly ffi sdl2 SDL2"))?;

    let current_dir = env::current_dir()?;
    let project_root = cavvy::cavly::find_project_root(&current_dir)
        .ok_or_else(|| anyhow::anyhow!("当前目录不是 Cavly 项目（找不到 cavly.toml）"))?;

    cavvy::cavly::project::Project::add_ffi_lib(&project_root, name, lib)?;

    Ok(())
}

/// 验证包的安全证书和完整性 (ESSO-10430)
///
/// # 复杂度
/// - 时间: O(n) 网络 + O(m) 哈希，m 为包大小
/// - 空间: O(m)
fn cmd_verify(args: &[String]) -> Result<()> {
    let package_name = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("请指定包名，例如: cavly verify my-pkg"))?;

    println!("正在验证包 '{}' 的安全证书...", package_name);

    let mut registry = SecureRegistry::new()?;
    let pkg = registry.find_package(package_name)?;

    println!("  包名: {}", pkg.name);
    println!("  指纹: {}", pkg.fingerprint);
    println!("  最新版本: {}", pkg.latest_version);
    println!("  仓库: {}", pkg.repository);

    // 获取元信息和证书
    let meta = registry.fetch_fingerprint_metadata(&pkg.fingerprint)?;
    let cert = registry.fetch_certificate(&pkg.fingerprint)?;

    println!("  发布者: {}", cert.publisher);
    println!("  证书时间: {}", cert.certified_at);
    println!("  包 SHA-256: {}", cert.package_sha256);

    // 如果本地存在包文件，验证完整性
    let current_dir = env::current_dir()?;
    let package_path = current_dir
        .join(".cavvy")
        .join("cache")
        .join("registry")
        .join(format!("{}-{}", pkg.name, pkg.latest_version));

    if package_path.exists() {
        let data = std::fs::read(&package_path)?;
        let hash = sha256_hex(&data);
        if hash == cert.package_sha256 {
            println!("  本地包完整性: 通过");
        } else {
            println!("  本地包完整性: 失败 (预期 {}, 实际 {})", cert.package_sha256, hash);
        }
    } else {
        println!("  本地包: 未下载");
    }

    println!("验证完成。");
    Ok(())
}

/// 添加可信公钥到项目配置
///
/// # 复杂度
/// - 时间: O(1)
/// - 空间: O(1)
fn cmd_trust(args: &[String]) -> Result<()> {
    let public_key_b64 = args
        .get(2)
        .ok_or_else(|| anyhow::anyhow!("请指定 Base64 编码的 Ed25519 公钥"))?;

    // 验证公钥格式
    let pk = cavvy::cavly::security::Ed25519PublicKey::from_base64("trusted", public_key_b64)?;
    let fingerprint = cavvy::cavly::security::compute_key_fingerprint(&pk.bytes);

    let current_dir = env::current_dir()?;
    let project_root = cavvy::cavly::find_project_root(&current_dir)
        .ok_or_else(|| anyhow::anyhow!("当前目录不是 Cavly 项目（找不到 cavly.toml）"))?;

    let config_path = project_root.join("cavly.toml");
    let mut config = cavvy::cavly::config::CavlyConfig::from_file(&config_path)?;

    if config.security.trusted_keys.contains(public_key_b64) {
        println!("公钥已在信任列表中");
        return Ok(());
    }

    config.security.trusted_keys.push(public_key_b64.clone());
    config.to_file(&config_path)?;

    println!("已添加可信公钥 (指纹: {})", fingerprint);

    // 审计日志
    let logger = AuditLogger::new().unwrap_or_default();
    logger.log_silent(
        &AuditLogEntry::new(SecurityEventType::UserConfirmed, "cmd_trust")
            .with_details(&format!("添加可信公钥，指纹: {}", fingerprint)),
    );

    Ok(())
}

/// 显示安全审计日志
///
/// # 复杂度
/// - 时间: O(n)，n 为日志条目数
/// - 空间: O(n)
fn cmd_audit_log() -> Result<()> {
    let logger = AuditLogger::new()?;
    let entries = logger.read_all()?;

    if entries.is_empty() {
        println!("审计日志为空");
        return Ok(());
    }

    println!("安全审计日志 (共 {} 条):", entries.len());
    println!();

    for entry in &entries {
        let pkg_info = match (&entry.package_name, &entry.package_version) {
            (Some(name), Some(ver)) => format!(" [{}@{}]", name, ver),
            (Some(name), None) => format!(" [{}]", name),
            _ => String::new(),
        };
        println!(
            "[{}] {:?} | {}{} | {}",
            entry.timestamp,
            entry.event_type,
            entry.operation,
            pkg_info,
            entry.verification_result.as_deref().unwrap_or("-")
        );
        if let Some(ref details) = entry.details {
            println!("  详情: {}", details);
        }
        if let Some(ref decision) = entry.user_decision {
            println!("  用户决策: {}", decision);
        }
    }

    Ok(())
}
