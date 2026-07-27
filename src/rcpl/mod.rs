//! RCPL (Rust Cavvy Playground Loop) - Cavvy 交互式解释器
//!
//! 将 PowerShell REPL 转换为 Rust 实现，提供持久化上下文、
//! 表达式自动打印、智能代码生成等功能。

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub mod code_generator;
pub mod context;
pub mod input_parser;

use code_generator::CodeGenerator;
use context::Context;
use input_parser::{InputParser, InputType};

/// RCPL 版本号
pub const VERSION: &str = "0.1.0";

/// RCPL 主结构
pub struct Rcpl {
    context: Context,
    parser: InputParser,
    generator: CodeGenerator,
    debug_mode: bool,
    cay_run_path: PathBuf,
    use_embedded_llc: bool,
}

impl Rcpl {
    /// 创建新的 RCPL 实例
    pub fn new() -> anyhow::Result<Self> {
        Self::new_with_options(false)
    }

    /// 使用选项创建新的 RCPL 实例
    pub fn new_with_options(use_embedded_llc: bool) -> anyhow::Result<Self> {
        let cay_run_path = Self::find_cay_run()?;

        Ok(Rcpl {
            context: Context::new(),
            parser: InputParser::new(),
            generator: CodeGenerator::new(),
            debug_mode: false,
            cay_run_path,
            use_embedded_llc,
        })
    }

    /// 查找 cay-run 可执行文件
    fn find_cay_run() -> anyhow::Result<PathBuf> {
        // 1. 首先检查同目录
        let exe_dir = std::env::current_exe()?
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        for name in &["cay-run.exe", "cay-run"] {
            let path = exe_dir.join(name);
            if path.exists() {
                return Ok(path);
            }
        }

        // 2. 检查 PATH
        if let Ok(output) = Command::new("which").arg("cay-run").output() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() && Path::new(&path).exists() {
                return Ok(PathBuf::from(path));
            }
        }

        anyhow::bail!("未找到 cay-run 可执行文件")
    }

    /// 运行 REPL 主循环
    pub fn run(&mut self) -> anyhow::Result<()> {
        self.print_banner();

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        loop {
            // 打印提示符
            print!("cavvy> ");
            stdout.flush()?;

            // 读取输入（支持多行）
            let input = self.read_input(&stdin)?;

            // 处理命令
            let trimmed = input.trim();
            if self.handle_command(trimmed) {
                continue;
            }

            if trimmed.is_empty() {
                continue;
            }

            // 解析输入类型
            let input_type = self.parser.parse(&input);

            // 更新上下文
            let update_context = self.update_context(&input_type);

            // 生成代码
            let program = self.generator.generate(&self.context, &input_type);

            if self.debug_mode {
                println!("\n=== 生成的代码 ===");
                println!("{}", program);
                println!("==================\n");
            }

            // 执行程序
            match self.execute(&program) {
                Ok(output) => {
                    if !output.is_empty() {
                        print!("{}", output);
                        if !output.ends_with('\n') {
                            println!();
                        }
                    }
                }
                Err(e) => {
                    // 执行失败，回滚上下文
                    if update_context {
                        self.rollback_context(&input_type);
                    }
                    crate::miette_diagnostic::print_miette_error(
                        "cavvy::runtime_error",
                        &format!("{}", e),
                        Some("请检查代码语法和语义"),
                    );
                }
            }
        }
    }

    /// 读取输入（支持多行）
    ///
    /// EOF（Ctrl-D / stdin 关闭）时优雅退出进程，与 :q 行为一致，
    /// 避免 read_line 返回 Ok(0) 导致的忙等死循环。
    fn read_input(&self, stdin: &io::Stdin) -> anyhow::Result<String> {
        let mut lines = Vec::new();
        let mut reader = stdin.lock();

        // 读取第一行
        let mut first_line = String::new();
        if reader.read_line(&mut first_line)? == 0 {
            // EOF：stdin 已关闭，继续循环只会无限刷提示符
            println!("\nBye!");
            std::process::exit(0);
        }
        lines.push(first_line.trim_end().to_string());

        // 检查是否需要多行输入
        while self.is_multiline_incomplete(&lines) {
            print!("... ");
            io::stdout().flush()?;

            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                // 多行输入未闭合时遇到 EOF：同样优雅退出，避免死循环
                println!("\nBye!");
                std::process::exit(0);
            }

            // 空行结束多行输入
            if line.trim().is_empty() {
                break;
            }

            lines.push(line.trim_end().to_string());
        }

        Ok(lines.join("\n"))
    }

    /// 检查多行输入是否不完整
    fn is_multiline_incomplete(&self, lines: &[String]) -> bool {
        let code = lines.join("\n");

        // 移除字符串内容避免误判
        let clean = self.remove_strings(&code);

        // 计算括号匹配
        let braces = clean.matches('{').count() as i32 - clean.matches('}').count() as i32;
        let parens = clean.matches('(').count() as i32 - clean.matches(')').count() as i32;
        let brackets = clean.matches('[').count() as i32 - clean.matches(']').count() as i32;

        // 引号配对
        let quotes = clean.matches('"').count() % 2;

        braces > 0 || parens > 0 || brackets > 0 || quotes != 0
    }

    /// 移除字符串内容
    fn remove_strings(&self, code: &str) -> String {
        let mut result = String::new();
        let mut chars = code.chars().peekable();
        let mut in_string = false;

        while let Some(c) = chars.next() {
            if c == '"' {
                in_string = !in_string;
                result.push(c);
            } else if in_string && c == '\\' {
                // 转义字符，跳过下一个
                result.push(c);
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            } else if !in_string {
                result.push(c);
            } else {
                // 在字符串内，替换为空格保持位置
                result.push(' ');
            }
        }

        result
    }

    /// 处理 REPL 命令，返回 true 表示已处理
    fn handle_command(&mut self, cmd: &str) -> bool {
        match cmd {
            ":q" | ":quit" | "exit" => {
                println!("\nBye!");
                std::process::exit(0);
            }
            ":h" | ":help" => {
                self.print_help();
                true
            }
            ":c" | ":clear" => {
                // 清屏
                print!("\x1B[2J\x1B[1;1H");
                true
            }
            ":ctx" => {
                self.context.show();
                true
            }
            ":debug" => {
                self.debug_mode = !self.debug_mode;
                println!("调试模式: {}", self.debug_mode);
                true
            }
            ":clearctx" => {
                self.context.clear();
                println!("上下文已清空");
                true
            }
            _ => false,
        }
    }

    /// 更新上下文，返回是否成功添加
    fn update_context(&mut self, input_type: &InputType) -> bool {
        match input_type {
            InputType::VarDecl { name, code, .. } => {
                self.context.add_persistent_stmt(code.clone());
                println!("[已持久化定义 '{}']", name);
                true
            }
            InputType::Assignment { lval, code } => {
                let action = self
                    .context
                    .add_or_update_assignment(lval.clone(), code.clone());
                println!("[{} '{}']", action, lval);
                true
            }
            InputType::StaticField { code } => {
                self.context.add_static_field(code.clone());
                true
            }
            InputType::Method { name, code, .. } => {
                self.context.add_method(code.clone());
                println!("[已定义方法 '{}']", name);
                true
            }
            InputType::Class { name, code } | InputType::Interface { name, code } => {
                self.context.add_class(code.clone());
                println!(
                    "[已定义 {} '{}']",
                    if matches!(input_type, InputType::Class { .. }) {
                        "类"
                    } else {
                        "接口"
                    },
                    name
                );
                true
            }
            InputType::Preprocessor { code } => {
                self.context.add_preprocessor_directive(code.clone());
                // 提取预处理器指令类型用于显示
                let directive_type = code.trim().split_whitespace().next().unwrap_or("#");
                println!("[已添加预处理器指令 '{}']", directive_type);
                true
            }
            _ => false,
        }
    }

    /// 回滚上下文
    fn rollback_context(&mut self, input_type: &InputType) {
        match input_type {
            InputType::VarDecl { .. } | InputType::Assignment { .. } => {
                self.context.remove_last_persistent_stmt();
            }
            InputType::StaticField { .. } => {
                self.context.remove_last_static_field();
            }
            InputType::Method { .. } => {
                self.context.remove_last_method();
            }
            InputType::Class { .. } | InputType::Interface { .. } => {
                self.context.remove_last_class();
            }
            InputType::Preprocessor { .. } => {
                self.context.remove_last_preprocessor_directive();
            }
            _ => {}
        }
    }

    /// 执行生成的代码
    fn execute(&self, program: &str) -> anyhow::Result<String> {
        // 创建临时文件（tempfile 保证文件名不可预测，并在离开作用域时自动清理）
        let mut tmp = tempfile::Builder::new()
            .prefix("cavvy_rcpl_")
            .suffix(".cay")
            .tempfile()?;
        tmp.write_all(program.as_bytes())?;
        tmp.flush()?;
        let temp_file = tmp.path().to_path_buf();

        // 获取 cay-run 所在目录作为工作目录
        let cay_run_dir = self
            .cay_run_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
            });

        // 执行 cay-run，使用 cay-run 所在目录作为工作目录
        let mut cmd = Command::new(&self.cay_run_path);
        cmd.arg(&temp_file)
            .current_dir(&cay_run_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if self.use_embedded_llc {
            cmd.arg("--use-embedded-llc");
        }
        let output = cmd.output()?;

        // 临时文件随 tmp 离开作用域自动删除

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            Ok(stdout.to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let error = stderr
                .to_string()
                .replace(&temp_file.to_string_lossy().to_string(), "repl");
            anyhow::bail!("{}", error)
        }
    }

    /// 打印横幅
    fn print_banner(&self) {
        println!("Cavvy RCPL v{}", VERSION);
        println!("使用: {}", self.cay_run_path.display());
        println!("提示: :h 帮助, :q 退出, :debug 查看生成代码, 直接输入表达式自动打印");
    }

    /// 打印帮助
    fn print_help(&self) {
        println!("Cavvy RCPL v{} - Rust Cavvy Playground Loop", VERSION);
        println!("命令: :q退出 :h帮助 :c清屏 :ctx显示上下文 :debug切换调试 :clearctx清空上下文");
        println!();
        println!("特性:");
        println!("  • 变量定义自动持久化: int a = 1;");
        println!("  • 赋值智能合并: 相同变量更新而非重复定义");
        println!("  • 表达式自动打印: a + 1 等价于 print(a + 1);");
        println!("  • 所有定义累积在 __ReplMain 中，main 方法执行当前语句");
    }
}

// 不提供 Default 实现：Rcpl 的创建依赖 cay-run 可执行文件等外部资源，
// 可能失败，必须由调用方通过 Rcpl::new()/new_with_options() 处理 Result，
// 而不是在 Default::default() 里 expect panic。
