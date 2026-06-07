//! cay-rcpl - Cavvy Rust Playground Loop
//!
//! RCPL 是 Cavvy 的交互式解释器，提供：
//! - 持久化变量定义和赋值
//! - 表达式自动打印
//! - 多行输入支持
//! - 上下文查看和管理

use cavvy::error::print_miette_error;
use std::env;

const VERSION: &str = env!("CAY_RCPL_VERSION");

fn print_usage() {
    println!("Cavvy RCPL (Rust Cavvy Playground Loop) v{}", VERSION);
    println!("Usage: cay-rcpl [options]");
    println!();
    println!("Options:");
    println!("  -h, --help     显示帮助信息");
    println!("  -v, --version  显示版本号");
    println!();
    println!("在 REPL 中可用的命令:");
    println!("  :q, :quit, exit  退出 REPL");
    println!("  :h, :help        显示帮助");
    println!("  :c, :clear       清屏");
    println!("  :ctx             显示当前上下文");
    println!("  :debug           切换调试模式");
    println!("  :clearctx        清空上下文");
}

fn main() {
    // 解析命令行参数
    let args: Vec<String> = env::args().collect();

    for arg in &args[1..] {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return;
            }
            "-v" | "--version" => {
                println!("cay-rcpl {}", VERSION);
                return;
            }
            _ => {}
        }
    }

    // 运行 RCPL
    match cavvy::rcpl::Rcpl::new() {
        Ok(mut rcpl) => {
            if let Err(e) = rcpl.run() {
                print_miette_error(
                    "cavvy::rcpl_error",
                    &format!("RCPL 错误: {}", e),
                    Some("请检查 RCPL 环境配置"),
                );
                std::process::exit(1);
            }
        }
        Err(e) => {
            print_miette_error(
                "cavvy::init_error",
                &format!("初始化失败: {}", e),
                Some("请检查编译器是否正确安装"),
            );
            std::process::exit(1);
        }
    }
}
