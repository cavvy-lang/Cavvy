//! cay-IDLE - Cavvy 集成开发环境
//!
//! 基于egui的轻量级GUI IDE，提供：
//! - 代码编辑器（语法高亮、行号显示）
//! - 实时语法检查（基于cay-check）
//! - 一键编译运行（集成cay-run）
//! - 项目文件管理

use std::env;

const VERSION: &str = env!("CAY_IDLE_VERSION");

fn print_usage() {
    println!("cay-IDLE v{}", VERSION);
    println!("Cavvy 集成开发环境");
    println!();
    println!("用法: cay-idle [选项] [文件]");
    println!();
    println!("选项:");
    println!("  -h, --help     显示帮助信息");
    println!("  -v, --version  显示版本号");
    println!("  --cli          以命令行模式运行");
    println!();
    println!("示例:");
    println!("  cay-idle              启动图形界面");
    println!("  cay-idle hello.cay    打开指定文件");
    println!("  cay-idle --cli file   命令行模式检查文件");
}

fn main() {
    let args: Vec<String> = env::args().collect();
    
    // 解析命令行参数
    let mut input_file: Option<String> = None;
    let mut cli_mode = false;
    
    for i in 1..args.len() {
        match args[i].as_str() {
            "-h" | "--help" => {
                print_usage();
                return;
            }
            "-v" | "--version" => {
                println!("cay-IDLE v{}", VERSION);
                return;
            }
            "--cli" => {
                cli_mode = true;
            }
            arg => {
                if !arg.starts_with('-') && input_file.is_none() {
                    input_file = Some(arg.to_string());
                }
            }
        }
    }
    
    if cli_mode {
        run_cli_mode(input_file);
    } else {
        run_gui_mode(input_file);
    }
}

/// 运行GUI模式
fn run_gui_mode(input_file: Option<String>) {
    use eframe::NativeOptions;
    
    let options = NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    
    // 创建应用
    let result = eframe::run_native(
        "cay-IDLE",
        options,
        Box::new(|cc| {
            let mut app = cavvy::idle::IdleApp::new(cc);
            
            // 如果指定了文件，打开它
            if let Some(file_path) = input_file {
                let path = std::path::PathBuf::from(file_path);
                app.open_file(path);
            }
            
            Ok(Box::new(app))
        }),
    );
    
    if let Err(e) = result {
        eprintln!("启动GUI失败: {}", e);
        std::process::exit(1);
    }
}

/// 运行CLI模式（简单的命令行界面）
fn run_cli_mode(input_file: Option<String>) {
    println!("cay-IDLE CLI模式");
    println!();
    
    if let Some(file) = input_file {
        let path = std::path::PathBuf::from(&file);
        
        // 检查文件是否存在
        if !path.exists() {
            eprintln!("错误: 文件不存在: {}", file);
            std::process::exit(1);
        }
        
        // 读取文件内容
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("错误: 无法读取文件: {}", e);
                std::process::exit(1);
            }
        };
        
        println!("正在检查: {}", file);
        
        // 使用编译器API进行语法检查
        use cavvy::lexer;
        use cavvy::parser;
        use cavvy::semantic;
        
        // 词法分析
        match lexer::lex(&content) {
            Ok(tokens) => {
                println!("✓ 词法分析通过 ({} 个token)", tokens.len());
                
                // 语法分析
                match parser::parse(tokens) {
                    Ok(_ast) => {
                        println!("✓ 语法分析通过");
                        
                        // 语义分析
                        let mut analyzer = semantic::SemanticAnalyzer::new();
                        match analyzer.analyze(&_ast) {
                            Ok(_) => {
                                println!("✓ 语义分析通过");
                                println!();
                                println!("检查完成，未发现错误");
                            }
                            Err(e) => {
                                eprintln!("✗ 语义错误: {}", e);
                                std::process::exit(1);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("✗ 语法错误: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ 词法错误: {}", e);
                std::process::exit(1);
            }
        }
    } else {
        println!("请在GUI模式下使用完整功能，或使用: cay-idle --cli <文件>");
    }
}
