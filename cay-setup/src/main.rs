use std::env;
use std::error::Error as _;
use std::io::{self, IsTerminal, Write};

use cay_setup::cli::{self, Command};
use cay_setup::install;
use cay_setup::{Error, Result};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        let mut source = error.source();
        while let Some(cause) = source {
            eprintln!("  caused by: {cause}");
            source = cause.source();
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    match cli::parse(env::args().skip(1))? {
        Command::Install(options) => {
            let root = install::cavvy_home(options.root.as_deref())?;
            if !confirm_install(&options, &root)? {
                println!("安装已取消。");
                return Ok(());
            }
            finish_install(install::install(&options)?)
        }
        Command::Update(mut options) => {
            options.yes = true;
            finish_install(install::install(&options)?)
        }
        Command::Uninstall { yes, root } => {
            let root = install::cavvy_home(root.as_deref())?;
            if !yes && !confirm(&format!("将删除 {}，继续吗", root.display()), false)? {
                println!("卸载已取消。");
                return Ok(());
            }
            if install::uninstall(&root)? {
                println!("Cavvy 已卸载。请重新打开终端使 PATH 更新生效。");
            } else {
                println!("Cavvy 尚未安装。");
            }
            Ok(())
        }
        Command::Show { root } => {
            let root = install::cavvy_home(root.as_deref())?;
            match install::installed_version(&root) {
                Some(version) => {
                    let bin = install::bin_dir(&root);
                    println!("Cavvy {version}");
                    println!("安装目录: {}", bin.display());
                    println!(
                        "当前终端 PATH: {}",
                        if install::path_contains(&bin) {
                            "已生效"
                        } else {
                            "未生效"
                        }
                    );
                }
                None => println!("Cavvy 尚未安装。"),
            }
            Ok(())
        }
        Command::Doctor { root } => {
            let root = install::cavvy_home(root.as_deref())?;
            let version = install::doctor(&root)?;
            println!("[OK] {version}");
            println!("[OK] caylibs 标准库");
            println!(
                "[OK] LLVM minimal {}",
                install::installed_llvm_version(&root).unwrap_or_else(|| "未知版本".to_string())
            );
            println!("[OK] 编译与链接探针");
            let bin = install::bin_dir(&root);
            if install::path_contains(&bin) {
                println!("[OK] 当前终端 PATH");
            } else {
                println!("[WARN] 当前终端尚未加载新 PATH，请重新打开终端");
            }
            Ok(())
        }
        Command::Version => {
            println!("cay-setup {}", env!("CAY_SETUP_VERSION"));
            Ok(())
        }
        Command::Help => {
            print!("{}", cli::HELP);
            Ok(())
        }
    }
}

fn confirm_install(options: &cli::InstallOptions, root: &std::path::Path) -> Result<bool> {
    if options.yes {
        return Ok(true);
    }
    println!("Cavvy 将安装到: {}", install::bin_dir(root).display());
    println!("安装包包含编译器、标准库和本地工具链，不需要 Rust、Python 或系统 LLVM。");
    confirm("继续安装吗", true)
}

fn confirm(prompt: &str, default_yes: bool) -> Result<bool> {
    if !io::stdin().is_terminal() {
        return Err(Error::InvalidArgument(
            "非交互环境请添加 `--yes`".to_string(),
        ));
    }
    let hint = if default_yes { "Y/n" } else { "y/N" };
    print!("{prompt} [{hint}]: ");
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim().to_ascii_lowercase();
    if input.is_empty() {
        Ok(default_yes)
    } else {
        Ok(input == "y" || input == "yes")
    }
}

fn finish_install(summary: install::InstallSummary) -> Result<()> {
    println!();
    println!("Cavvy {} 安装完成。", summary.version);
    println!("安装目录: {}", summary.bin_dir.display());
    if summary.path_modified {
        println!("请重新打开终端，然后运行 `cayc --version`。");
    } else {
        println!("你选择了不修改 PATH，请手动添加上述目录。");
    }
    Ok(())
}
