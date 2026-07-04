//! Cavvy 预处理器模块
//!
//! 实现 0.3.5.0 版本的预处理指令系统：
//! - #include "path"  - 文件包含（隐式 #pragma once）
//! - #define NAME value  - 常量定义（无参数宏）
//! - #ifdef / #ifndef / #else / #elif / #endif  - 条件编译
//! - #error "message"  - 编译期错误
//! - #warning "message"  - 编译期警告
//! - #link "libname"  - 声明需要链接的库
//!
//! 设计约束：
//! - 仅支持简单常量定义，禁止宏函数
//! - 隐式 #pragma once 基于绝对路径哈希
//! - 预处理在词法分析之前执行，生成纯源代码
//! - 生成 #source <file> <line> 标记以支持源映射

use crate::miette_diagnostic::{CayError, CayResult, ErrorCodes};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// 源位置信息
#[derive(Debug, Clone)]
pub struct SourcePosition {
    pub file: String,
    pub line: usize,
}

/// 源映射表：将输出行号映射到原始源位置
#[derive(Debug, Clone, Default)]
pub struct SourceMap {
    pub mappings: Vec<SourcePosition>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self {
            mappings: Vec::new(),
        }
    }

    /// 添加一个源位置映射
    pub fn add_mapping(&mut self, file: String, line: usize) {
        self.mappings.push(SourcePosition { file, line });
    }

    /// 获取指定输出行号对应的源位置
    pub fn get_source_position(&self, output_line: usize) -> Option<&SourcePosition> {
        // output_line 是1-based的
        self.mappings.get(output_line.saturating_sub(1))
    }

    /// 获取映射数量
    pub fn len(&self) -> usize {
        self.mappings.len()
    }

    /// 序列化为字符串（用于嵌入到预处理后的代码中）
    pub fn serialize(&self) -> String {
        self.mappings
            .iter()
            .map(|pos| format!("#source {} {}", pos.file, pos.line))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 链接库声明
#[derive(Debug, Clone)]
pub struct LinkLibrary {
    pub name: String,
    pub is_system: bool,
}

/// 预处理结果，包含处理后的代码和源映射
#[derive(Debug, Clone)]
pub struct PreprocessResult {
    pub code: String,
    pub source_map: SourceMap,
    pub link_libraries: Vec<LinkLibrary>,
}

/// 预处理器状态
pub struct Preprocessor {
    /// 已定义的宏常量 (name -> value)
    defines: HashMap<String, String>,
    /// 已包含的文件路径集合（用于 #pragma once 语义）
    included_files: HashSet<String>,
    /// 基础目录（用于解析相对路径）
    base_dir: PathBuf,
    /// 当前条件编译栈
    conditional_stack: Vec<ConditionalState>,
    /// 是否处于被跳过的代码块中
    skipping: bool,
    /// 包含栈（用于循环包含检测和错误报告）
    include_stack: Vec<String>,
    /// 系统包含路径列表
    system_include_paths: Vec<PathBuf>,
}

/// 条件编译状态
#[derive(Debug, Clone, Copy, PartialEq)]
enum ConditionalState {
    /// 当前条件为真，正在处理代码
    Active,
    /// 当前条件为假，但处于可能执行 #else 的链中
    Inactive,
    /// 当前条件为假，且已经执行过某个分支，跳过后续所有 #elif/#else
    Done,
}

/// 预处理指令枚举
#[derive(Debug, Clone)]
enum Directive {
    /// #include "path" 或 #include <path>
    Include(String, bool), // (路径, 是否系统路径)
    /// #define name value
    Define(String, String), // (名称, 值)
    /// #ifdef name
    Ifdef(String),
    /// #ifndef name
    Ifndef(String),
    /// #if expression
    If(String),
    /// #else
    Else,
    /// #elif expression
    Elif(String),
    /// #endif
    Endif,
    /// #error "message"
    Error(String),
    /// #warning "message"
    Warning(String),
    /// #pragma once
    PragmaOnce,
    /// #link "libname" 或 #link <libname>
    Link(String, bool), // (库名, 是否系统库)
}

/// 指令处理结果
#[derive(Debug, Clone)]
enum DirectiveResult {
    /// 单行输出（普通指令）
    Single(Option<String>),
    /// 多行输出（包含文件）
    Multi { code: String, source_map: SourceMap },
    /// 链接库声明
    Link { lib_name: String, is_system: bool },
}

impl Preprocessor {
    /// 创建新的预处理器实例
    ///
    /// # Arguments
    /// * `base_dir` - 源代码基础目录，用于解析相对路径
    ///
    /// # Returns
    /// 初始化后的预处理器
    pub fn new(base_dir: impl AsRef<Path>) -> Self {
        Self::with_include_paths(base_dir, Vec::new())
    }

    /// 创建带有额外包含路径的预处理器实例
    ///
    /// # Arguments
    /// * `base_dir` - 源代码基础目录
    /// * `include_paths` - 额外的包含路径列表（供 #include 搜索）
    ///
    /// # Returns
    /// 初始化后的预处理器
    pub fn with_include_paths(base_dir: impl AsRef<Path>, include_paths: Vec<PathBuf>) -> Self {
        let mut defines = HashMap::new();

        // 自动定义平台宏
        #[cfg(target_os = "windows")]
        {
            defines.insert("_WIN32".to_string(), "".to_string());
        }
        #[cfg(target_os = "linux")]
        {
            defines.insert("__linux__".to_string(), "".to_string());
        }
        #[cfg(target_os = "macos")]
        {
            defines.insert("__APPLE__".to_string(), "".to_string());
        }

        Self {
            defines,
            included_files: HashSet::new(),
            base_dir: base_dir.as_ref().to_path_buf(),
            conditional_stack: Vec::new(),
            skipping: false,
            include_stack: Vec::new(),
            system_include_paths: include_paths,
        }
    }

    /// 预处理源文件，返回处理后的源代码（带源映射）
    ///
    /// # Arguments
    /// * `source` - 原始源代码
    /// * `file_path` - 源文件路径（用于错误报告）
    ///
    /// # Returns
    /// 预处理后的结果，包含代码和源映射
    ///
    /// # Errors
    /// 当遇到预处理错误时返回错误
    pub fn process_with_source_map(
        &mut self,
        source: &str,
        file_path: &str,
    ) -> CayResult<PreprocessResult> {
        let mut output_lines = Vec::new();
        let mut source_map = SourceMap::new();
        let mut link_libraries = Vec::new();
        let lines: Vec<&str> = source.lines().collect();

        for (line_number, line) in lines.iter().enumerate() {
            let line_number = line_number + 1; // 转换为1-based

            // 检查是否是预处理指令行（以 # 开头，可以有前导空白）
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') {
                // 解析预处理指令
                match self.parse_directive(trimmed, line_number, file_path) {
                    Ok(Some(directive)) => {
                        match self.process_directive(directive, file_path, line_number)? {
                            DirectiveResult::Single(processed_line) => {
                                source_map.add_mapping(file_path.to_string(), line_number);
                                output_lines.push(processed_line.unwrap_or_default());
                            }
                            DirectiveResult::Multi {
                                code,
                                source_map: included_source_map,
                            } => {
                                // 包含文件返回多行，需要合并源映射
                                // 记录当前输出行数，用于正确对齐包含文件的源映射
                                let current_line_count = output_lines.len();
                                let code_lines: Vec<_> = code.lines().collect();
                                // 修复：确保 code_lines 和 included_source_map.mappings 的长度一致
                                // 如果 code_lines 比 mappings 少，添加空行
                                let mut lines_to_add = code_lines.len();
                                for included_line in code_lines {
                                    output_lines.push(included_line.to_string());
                                }
                                // 如果 code_lines 比 mappings 少，添加空行以保持对齐
                                while lines_to_add < included_source_map.mappings.len() {
                                    output_lines.push("".to_string());
                                    lines_to_add += 1;
                                }
                                // 合并源映射 - 保持正确的行号对应关系
                                // included_source_map.mappings 的索引对应包含文件中的行号
                                // 需要将这些映射按顺序添加到 source_map 中
                                for mapping in included_source_map.mappings.iter() {
                                    source_map.add_mapping(mapping.file.clone(), mapping.line);
                                }
                            }
                            DirectiveResult::Link {
                                lib_name,
                                is_system,
                            } => {
                                // 收集链接库信息
                                link_libraries.push(LinkLibrary {
                                    name: lib_name,
                                    is_system,
                                });
                                source_map.add_mapping(file_path.to_string(), line_number);
                                output_lines.push("".to_string());
                            }
                        }
                    }
                    Ok(None) => {
                        source_map.add_mapping(file_path.to_string(), line_number);
                        output_lines.push("".to_string());
                    }
                    Err(e) => return Err(e),
                }
            } else if self.skipping {
                // 处于条件编译跳过状态，不输出代码行
                // 但仍需跟踪行号以保持行号映射
                source_map.add_mapping(file_path.to_string(), line_number);
                output_lines.push("".to_string());
            } else {
                // 普通代码行，进行宏替换后输出
                let processed = self.expand_macros(line);
                source_map.add_mapping(file_path.to_string(), line_number);
                output_lines.push(processed);
            }
        }

        // 检查条件编译栈是否为空
        if !self.conditional_stack.is_empty() {
            return Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(file_path.to_string()),
                line: lines.len(),
                column: 1,
                message: "未闭合的条件编译指令，缺少 #endif".to_string(),
                suggestion: "请为每个 #ifdef 或 #ifndef 添加对应的 #endif".to_string(),
            });
        }

        Ok(PreprocessResult {
            code: output_lines.join("\n"),
            source_map,
            link_libraries,
        })
    }

    /// 解析单行预处理指令
    ///
    /// # Arguments
    /// * `line` - 已去除前导空白的行内容
    /// * `line_num` - 行号（用于错误报告）
    /// * `file_path` - 文件路径（用于错误报告）
    ///
    /// # Returns
    /// 解析出的指令或 None
    fn parse_directive(
        &self,
        line: &str,
        line_num: usize,
        file_path: &str,
    ) -> CayResult<Option<Directive>> {
        // 去除 # 后面的空白
        let content = line[1..].trim_start();

        if content.is_empty() {
            return Ok(None);
        }

        // 提取指令名和参数（移除块注释）
        let mut parts = content.splitn(2, |c: char| c.is_whitespace());
        let directive_name = parts.next().unwrap_or("");
        let args_raw = parts.next().unwrap_or("");
        let args_cleaned = Self::remove_block_comments(args_raw);
        let args = args_cleaned.trim();

        match directive_name {
            "include" => {
                // 解析 #include "path" 或 #include <path>
                let (path, is_system) = self.parse_include_path(args, line_num, file_path)?;
                Ok(Some(Directive::Include(path, is_system)))
            }
            "define" => {
                // 解析 #define name value
                let (name, value) = self.parse_define_args(args, line_num, file_path)?;
                Ok(Some(Directive::Define(name, value)))
            }
            "ifdef" => {
                let name = self.parse_identifier(args, line_num, file_path)?;
                Ok(Some(Directive::Ifdef(name)))
            }
            "ifndef" => {
                let name = self.parse_identifier(args, line_num, file_path)?;
                Ok(Some(Directive::Ifndef(name)))
            }
            "if" => {
                let expr = args.trim().to_string();
                Ok(Some(Directive::If(expr)))
            }
            "else" => {
                // 允许 #else 后面有注释
                Ok(Some(Directive::Else))
            }
            "elif" => {
                let name = self.parse_identifier(args, line_num, file_path)?;
                Ok(Some(Directive::Elif(name)))
            }
            "endif" => {
                // 允许 #endif 后面有注释（如 #endif /* CONDITION */）
                Ok(Some(Directive::Endif))
            }
            "error" => {
                let message = self.parse_string_literal(args, line_num, file_path)?;
                Ok(Some(Directive::Error(message)))
            }
            "warning" => {
                let message = self.parse_string_literal(args, line_num, file_path)?;
                Ok(Some(Directive::Warning(message)))
            }
            "pragma" => {
                // 解析 #pragma 指令
                if args == "once" {
                    Ok(Some(Directive::PragmaOnce))
                } else {
                    // 其他 #pragma 指令暂时忽略
                    Ok(None)
                }
            }
            "link" => {
                // 解析 #link "libname" 或 #link <libname>
                let (lib_name, is_system) = self.parse_link_args(args, line_num, file_path)?;
                Ok(Some(Directive::Link(lib_name, is_system)))
            }
            _ => {
                Err(CayError::Preprocessor {
                    error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                    file: Some(file_path.to_string()),
                    line: line_num,
                    column: 1,
                    message: format!("未知的预处理指令: {}", directive_name),
                    suggestion: "支持的指令: #include, #define, #ifdef, #ifndef, #else, #elif, #endif, #error, #warning, #link".to_string(),
                })
            }
        }
    }

    /// 移除 C 风格块注释 /* ... */
    fn remove_block_comments(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '/' && chars.peek() == Some(&'*') {
                // 找到注释开始 /*
                chars.next(); // 消费 *
                // 跳过直到 */
                while let Some(ch) = chars.next() {
                    if ch == '*' && chars.peek() == Some(&'/') {
                        chars.next(); // 消费 /
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        }

        result
    }

    /// 处理单个预处理指令
    ///
    /// # Returns
    /// - Ok(DirectiveResult::Single(line)) - 生成单行输出
    /// - Ok(DirectiveResult::Multi{code, source_map}) - 生成多行输出（包含文件）
    fn process_directive(
        &mut self,
        directive: Directive,
        file_path: &str,
        line_num: usize,
    ) -> CayResult<DirectiveResult> {
        match directive {
            Directive::Include(path, is_system) => {
                if self.skipping {
                    return Ok(DirectiveResult::Single(None));
                }

                // 读取包含文件
                match self.read_include_file(&path, is_system, file_path)? {
                    Some((include_content, full_path)) => {
                        // 添加到包含栈（用于循环检测）- 使用完整路径
                        self.include_stack.push(full_path.clone());

                        // 保存当前条件编译状态
                        let saved_conditional_stack = self.conditional_stack.clone();
                        let saved_skipping = self.skipping;

                        // 重置条件编译状态用于包含文件
                        self.conditional_stack = Vec::new();
                        self.skipping = false;

                        // 递归处理包含的文件 - 使用完整路径
                        let included_result =
                            self.process_with_source_map(&include_content, &full_path)?;

                        // 恢复条件编译状态
                        self.conditional_stack = saved_conditional_stack;
                        self.skipping = saved_skipping;

                        // 处理完成后从栈中移除
                        self.include_stack.pop();

                        // 返回处理后的内容和源映射
                        Ok(DirectiveResult::Multi {
                            code: included_result.code,
                            source_map: included_result.source_map,
                        })
                    }
                    None => {
                        // 文件已经包含过（#pragma once 语义），跳过
                        Ok(DirectiveResult::Single(Some(String::new())))
                    }
                }
            }
            Directive::Define(name, value) => {
                if self.skipping {
                    return Ok(DirectiveResult::Single(None));
                }
                self.defines.insert(name, value);
                Ok(DirectiveResult::Single(None))
            }
            Directive::Ifdef(name) => {
                let condition = self.defines.contains_key(&name);
                self.push_conditional(condition);
                Ok(DirectiveResult::Single(None))
            }
            Directive::Ifndef(name) => {
                let condition = !self.defines.contains_key(&name);
                self.push_conditional(condition);
                Ok(DirectiveResult::Single(None))
            }
            Directive::If(expr) => {
                // TODO: 实现完整的条件表达式评估
                let condition = self.evaluate_condition(&expr);
                self.push_conditional(condition);
                Ok(DirectiveResult::Single(None))
            }
            Directive::Else => {
                self.handle_else(file_path)?;
                Ok(DirectiveResult::Single(None))
            }
            Directive::Elif(expr) => {
                let condition = self.evaluate_condition(&expr);
                self.handle_elif(condition, file_path)?;
                Ok(DirectiveResult::Single(None))
            }
            Directive::Endif => {
                self.pop_conditional(file_path)?;
                Ok(DirectiveResult::Single(None))
            }
            Directive::Error(message) => {
                if !self.skipping {
                    return Err(CayError::Preprocessor {
                        error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                        file: Some(file_path.to_string()),
                        line: line_num,
                        column: 1,
                        message: format!("#error: {}", message),
                        suggestion: "这是源代码中显式要求的编译错误".to_string(),
                    });
                }
                Ok(DirectiveResult::Single(None))
            }
            Directive::Warning(message) => {
                if !self.skipping {
                    eprintln!("警告: {}", message);
                }
                Ok(DirectiveResult::Single(None))
            }
            Directive::PragmaOnce => {
                // 隐式处理：基于绝对路径的哈希
                Ok(DirectiveResult::Single(None))
            }
            Directive::Link(lib_name, is_system) => {
                if self.skipping {
                    return Ok(DirectiveResult::Single(None));
                }
                // #link 指令返回链接库信息
                Ok(DirectiveResult::Link {
                    lib_name,
                    is_system,
                })
            }
        }
    }

    /// 评估条件表达式 - 支持完整的 C 预处理器条件表达式语法
    ///
    /// 支持的语法:
    /// - `defined(MACRO)` 或 `defined MACRO` - 检查宏是否已定义
    /// - `!expr` - 逻辑非
    /// - `expr && expr` - 逻辑与
    /// - `expr || expr` - 逻辑或
    /// - `expr == expr`, `expr != expr` - 相等比较
    /// - `expr < expr`, `expr > expr`, `expr <= expr`, `expr >= expr` - 数值比较
    /// - `+`, `-`, `*`, `/`, `%` - 算术运算
    /// - `(expr)` - 括号分组
    /// - 数字常量 (十进制, 0x十六进制, 0八进制)
    /// - 宏名 - 如果已定义则为其值，否则为0
    fn evaluate_condition(&self, expr: &str) -> bool {
        let trimmed = expr.trim();
        if trimmed.is_empty() {
            return false;
        }

        // 简单快速路径：检查是否是已定义的宏
        if self.defines.contains_key(trimmed) {
            return true;
        }

        // 简单快速路径：尝试解析为数字
        if let Ok(num) = parse_preprocessor_number(trimmed) {
            return num != 0;
        }

        // 使用递归下降解析器评估完整表达式
        let mut parser = ConditionParser::new(trimmed, &self.defines);
        match parser.parse_expression() {
            Ok(result) => result != 0,
            Err(_) => false, // 解析失败默认为 false
        }
    }

    /// 解析 #include 路径
    fn parse_include_path(
        &self,
        args: &str,
        line_num: usize,
        file_path: &str,
    ) -> CayResult<(String, bool)> {
        let trimmed = args.trim();

        if trimmed.is_empty() {
            return Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(file_path.to_string()),
                line: line_num,
                column: 1,
                message: "#include 缺少路径参数".to_string(),
                suggestion: "使用 #include \"path\" 或 #include <path>".to_string(),
            });
        }

        // 检查是系统路径 <path> 还是用户路径 "path"
        if trimmed.starts_with('<') && trimmed.ends_with('>') {
            // 系统路径
            let path = &trimmed[1..trimmed.len() - 1];
            Ok((path.to_string(), true))
        } else if trimmed.starts_with('"') && trimmed.ends_with('"') {
            // 用户路径
            let path = &trimmed[1..trimmed.len() - 1];
            Ok((path.to_string(), false))
        } else {
            Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(file_path.to_string()),
                line: line_num,
                column: 1,
                message: format!("无效的 #include 语法: {}", trimmed),
                suggestion: "使用 #include \"path\" 或 #include <path>".to_string(),
            })
        }
    }

    /// 读取包含文件
    fn read_include_file(
        &mut self,
        path: &str,
        is_system: bool,
        current_file: &str,
    ) -> CayResult<Option<(String, String)>> {
        // 解析完整路径
        let full_path = self.resolve_include_path(path, is_system, current_file)?;

        // 规范化路径：将相对路径转换为绝对路径，消除 .. 和符号链接
        // 确保不同路径字符串引用同一文件时被正确去重
        let full_path = std::fs::canonicalize(&full_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or(full_path);

        // 首先检测循环包含（基于当前处理链）- 使用完整路径
        if self.include_stack.contains(&full_path) {
            return Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(current_file.to_string()),
                line: 1,
                column: 1,
                message: format!("检测到循环包含: {}", full_path),
                suggestion: "检查头文件之间的循环依赖".to_string(),
            });
        }

        // 然后检查是否已经包含过（#pragma once 语义）
        if self.included_files.contains(&full_path) {
            return Ok(None); // 已经包含过，返回 None 表示跳过
        }

        // 读取文件内容
        let content = std::fs::read_to_string(&full_path).map_err(|e| CayError::Preprocessor {
            error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
            file: Some(current_file.to_string()),
            line: 1,
            column: 1,
            message: format!("无法读取包含文件 '{}': {}", full_path, e),
            suggestion: "检查文件路径是否正确".to_string(),
        })?;

        // 添加到已包含集合
        self.included_files.insert(full_path.clone());

        Ok(Some((content, full_path)))
    }

    /// 解析包含文件的完整路径
    fn resolve_include_path(
        &self,
        path: &str,
        is_system: bool,
        current_file: &str,
    ) -> CayResult<String> {
        if is_system {
            for sys_path in &self.system_include_paths {
                let sys_include_path = sys_path.join(path);
                if sys_include_path.exists() {
                    return Ok(sys_include_path.to_string_lossy().to_string());
                }
            }

            // 2. 可执行文件所在目录下的 caylibs
            // 使用 canonicalize 解析符号链接（Linux上current_exe可能返回/proc/self/exe）
            if let Ok(exe_path) = std::env::current_exe() {
                let exe_path = exe_path.canonicalize().unwrap_or(exe_path);
                if let Some(exe_dir) = exe_path.parent() {
                    let exe_caylibs = exe_dir.join("caylibs").join(path);
                    if exe_caylibs.exists() {
                        return Ok(exe_caylibs.to_string_lossy().to_string());
                    }
                }
            }

            // 3. 当前工作目录下的 caylibs
            let cwd_caylibs = std::env::current_dir()
                .map(|d| d.join("caylibs").join(path))
                .unwrap_or_else(|_| PathBuf::from(path));
            if cwd_caylibs.exists() {
                return Ok(cwd_caylibs.to_string_lossy().to_string());
            }

            Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(current_file.to_string()),
                line: 1,
                column: 1,
                message: format!("系统包含文件未找到: <{}>", path),
                suggestion: "检查系统包含路径配置".to_string(),
            })
        } else {
            // 4. 相对于当前文件目录
            let current_dir = Path::new(current_file)
                .parent()
                .unwrap_or_else(|| Path::new("."));
            let relative_path = current_dir.join(path);
            if relative_path.exists() {
                return Ok(relative_path.to_string_lossy().to_string());
            }

            // 5. 基础目录
            let base_path = self.base_dir.join(path);
            if base_path.exists() {
                return Ok(base_path.to_string_lossy().to_string());
            }

            // 6. 额外的包含路径（-I 指定的路径，最后尝试）
            for include_path in &self.system_include_paths {
                let include_full_path = include_path.join(path);
                if include_full_path.exists() {
                    return Ok(include_full_path.to_string_lossy().to_string());
                }
            }

            Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(current_file.to_string()),
                line: 1,
                column: 1,
                message: format!("包含文件未找到: \"{}\"", path),
                suggestion: "检查文件路径是否正确，或使用系统包含路径 <path>".to_string(),
            })
        }
    }

    /// 解析 #define 参数
    fn parse_define_args(
        &self,
        args: &str,
        line_num: usize,
        file_path: &str,
    ) -> CayResult<(String, String)> {
        let trimmed = args.trim();

        if trimmed.is_empty() {
            return Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(file_path.to_string()),
                line: line_num,
                column: 1,
                message: "#define 缺少宏名称".to_string(),
                suggestion: "使用 #define NAME value".to_string(),
            });
        }

        // 移除行尾注释 (// 和 /* */ 风格)
        let without_comments = Self::remove_line_comments(trimmed);

        // 分割名称和值
        let mut parts = without_comments.splitn(2, |c: char| c.is_whitespace());
        let name = parts.next().unwrap_or("").to_string();
        let value = parts.next().unwrap_or("").trim().to_string();

        if name.is_empty() {
            return Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(file_path.to_string()),
                line: line_num,
                column: 1,
                message: "#define 宏名称不能为空".to_string(),
                suggestion: "使用 #define NAME value".to_string(),
            });
        }

        Ok((name, value))
    }

    /// 移除行内注释（// 和 /* */ 风格）
    fn remove_line_comments(line: &str) -> String {
        // 处理 // 风格注释
        if let Some(pos) = line.find("//") {
            return line[..pos].trim_end().to_string();
        }

        // 处理 /* */ 风格注释（单行情况）
        if let Some(start) = line.find("/*") {
            if let Some(end) = line[start..].find("*/") {
                let before = &line[..start];
                let after = &line[start + end + 2..];
                return Self::remove_line_comments(&(before.to_string() + after));
            }
        }

        line.to_string()
    }

    /// 解析 #link 参数
    /// 支持 #link "libname" 或 #link <libname>
    fn parse_link_args(
        &self,
        args: &str,
        line_num: usize,
        file_path: &str,
    ) -> CayResult<(String, bool)> {
        let trimmed = args.trim();

        if trimmed.is_empty() {
            return Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(file_path.to_string()),
                line: line_num,
                column: 1,
                message: "#link 缺少库名称参数".to_string(),
                suggestion: "使用 #link \"libname\" 或 #link <libname>".to_string(),
            });
        }

        // 检查是系统库 <libname> 还是用户库 "libname"
        if trimmed.starts_with('<') && trimmed.ends_with('>') {
            // 系统库
            let lib_name = &trimmed[1..trimmed.len() - 1];
            if lib_name.is_empty() {
                return Err(CayError::Preprocessor {
                    error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                    file: Some(file_path.to_string()),
                    line: line_num,
                    column: 1,
                    message: "#link 库名称不能为空".to_string(),
                    suggestion: "使用 #link <libname>".to_string(),
                });
            }
            Ok((lib_name.to_string(), true))
        } else if trimmed.starts_with('"') && trimmed.ends_with('"') {
            // 用户库
            let lib_name = &trimmed[1..trimmed.len() - 1];
            if lib_name.is_empty() {
                return Err(CayError::Preprocessor {
                    error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                    file: Some(file_path.to_string()),
                    line: line_num,
                    column: 1,
                    message: "#link 库名称不能为空".to_string(),
                    suggestion: "使用 #link \"libname\"".to_string(),
                });
            }
            Ok((lib_name.to_string(), false))
        } else {
            Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(file_path.to_string()),
                line: line_num,
                column: 1,
                message: format!("无效的 #link 语法: {}", trimmed),
                suggestion: "使用 #link \"libname\" 或 #link <libname>".to_string(),
            })
        }
    }

    /// 解析标识符
    fn parse_identifier(&self, args: &str, line_num: usize, file_path: &str) -> CayResult<String> {
        let trimmed = args.trim();

        if trimmed.is_empty() {
            return Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(file_path.to_string()),
                line: line_num,
                column: 1,
                message: "缺少标识符参数".to_string(),
                suggestion: "提供标识符名称".to_string(),
            });
        }

        // 标识符只能包含字母、数字和下划线，且不能以数字开头
        let name = trimmed.split_whitespace().next().unwrap_or("").to_string();

        if name.is_empty() {
            return Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(file_path.to_string()),
                line: line_num,
                column: 1,
                message: "标识符不能为空".to_string(),
                suggestion: "提供有效的标识符名称".to_string(),
            });
        }

        Ok(name)
    }

    /// 解析字符串字面量
    fn parse_string_literal(
        &self,
        args: &str,
        line_num: usize,
        file_path: &str,
    ) -> CayResult<String> {
        let trimmed = args.trim();

        if trimmed.is_empty() {
            return Ok(String::new());
        }

        // 去除引号
        if (trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
        {
            let content = &trimmed[1..trimmed.len() - 1];
            Ok(content.to_string())
        } else {
            Ok(trimmed.to_string())
        }
    }

    /// 宏替换
    /// 时间复杂度: O(n * m) 其中 n 是行长度，m 是宏定义数量
    /// 空间复杂度: O(n) 用于存储结果字符串
    fn expand_macros(&self, line: &str) -> String {
        let mut result = String::with_capacity(line.len());
        let mut i = 0;
        let chars: Vec<char> = line.chars().collect();

        while i < chars.len() {
            // 尝试匹配最长的宏名称
            let mut matched = false;

            // 按长度降序排序宏定义，确保优先匹配更长的名称
            // 例如 FILE_MODE_READPLUS 应该在 FILE_MODE_READ 之前被检查
            let mut defines_sorted: Vec<_> = self.defines.iter().collect();
            defines_sorted.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()));

            for (name, value) in &defines_sorted {
                let name_len = name.len();
                if i + name_len <= chars.len() {
                    let candidate: String = chars[i..i + name_len].iter().collect();
                    if candidate == **name {
                        // 检查前后是否是标识符边界
                        let before_is_boundary = i == 0 || !self.is_identifier_char(chars[i - 1]);
                        let after_pos = i + name_len;
                        let after_is_boundary =
                            after_pos >= chars.len() || !self.is_identifier_char(chars[after_pos]);

                        if before_is_boundary && after_is_boundary {
                            result.push_str(value);
                            i += name_len;
                            matched = true;
                            break;
                        }
                    }
                }
            }

            if !matched {
                result.push(chars[i]);
                i += 1;
            }
        }

        result
    }

    /// 检查字符是否是标识符字符（字母、数字、下划线）
    /// 时间复杂度: O(1)
    fn is_identifier_char(&self, c: char) -> bool {
        c.is_alphanumeric() || c == '_'
    }

    /// 压入条件编译状态
    fn push_conditional(&mut self, condition: bool) {
        if self.skipping {
            // 如果已经在跳过状态，新条件也跳过
            self.conditional_stack.push(ConditionalState::Inactive);
        } else if condition {
            self.conditional_stack.push(ConditionalState::Active);
            self.skipping = false;
        } else {
            self.conditional_stack.push(ConditionalState::Inactive);
            self.skipping = true;
        }
    }

    /// 处理 #else
    fn handle_else(&mut self, file_path: &str) -> CayResult<()> {
        match self.conditional_stack.last() {
            Some(ConditionalState::Active) => {
                // 当前分支已执行，跳过后续
                *self.conditional_stack.last_mut().unwrap() = ConditionalState::Done;
                self.skipping = true;
            }
            Some(ConditionalState::Inactive) => {
                // 当前分支未执行，现在执行
                *self.conditional_stack.last_mut().unwrap() = ConditionalState::Active;
                self.skipping = false;
            }
            Some(ConditionalState::Done) => {
                // 已经有分支执行过了，继续跳过
                self.skipping = true;
            }
            None => {
                return Err(CayError::Preprocessor {
                    error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                    file: Some(file_path.to_string()),
                    line: 1,
                    column: 1,
                    message: "多余的 #else".to_string(),
                    suggestion: "确保每个 #else 都有对应的 #ifdef 或 #ifndef".to_string(),
                });
            }
        }
        Ok(())
    }

    /// 处理 #elif
    fn handle_elif(&mut self, condition: bool, file_path: &str) -> CayResult<()> {
        match self.conditional_stack.last() {
            Some(ConditionalState::Active) => {
                // 当前分支已执行，跳过后续
                *self.conditional_stack.last_mut().unwrap() = ConditionalState::Done;
                self.skipping = true;
            }
            Some(ConditionalState::Inactive) if condition => {
                // 当前分支未执行且条件为真，执行
                *self.conditional_stack.last_mut().unwrap() = ConditionalState::Active;
                self.skipping = false;
            }
            _ => {
                // 继续跳过
                self.skipping = true;
            }
        }
        Ok(())
    }

    /// 弹出条件编译状态
    fn pop_conditional(&mut self, file_path: &str) -> CayResult<()> {
        if self.conditional_stack.pop().is_none() {
            return Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                file: Some(file_path.to_string()),
                line: 1,
                column: 0,
                message: "多余的 #endif".to_string(),
                suggestion: "确保每个 #endif 都有对应的 #ifdef 或 #ifndef".to_string(),
            });
        }

        // 更新 skipping 状态
        self.skipping = self
            .conditional_stack
            .iter()
            .any(|&state| state != ConditionalState::Active);

        Ok(())
    }

    /// 兼容旧版本的简单预处理接口
    pub fn process(&mut self, source: &str, file_path: &str) -> CayResult<String> {
        let result = self.process_with_source_map(source, file_path)?;
        Ok(result.code)
    }
}

/// 独立的预处理函数接口（兼容旧版本调用）
///
/// # Arguments
/// * `source` - 原始源代码
/// * `file_path` - 源文件路径（用于错误报告）
/// * `base_dir` - 基础目录（用于解析相对路径）
///
/// # Returns
/// 预处理后的源代码字符串
///
/// # Errors
/// 当遇到预处理错误时返回错误
pub fn preprocess(source: &str, file_path: &str, base_dir: &str) -> CayResult<String> {
    let mut pp = Preprocessor::new(base_dir);
    pp.process(source, file_path)
}

/// 解析预处理器数字常量（支持十进制、十六进制、八进制、二进制）
fn parse_preprocessor_number(s: &str) -> Result<i64, ()> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(());
    }

    // 处理负号
    let (negative, num_str) = if trimmed.starts_with('-') {
        (true, &trimmed[1..])
    } else if trimmed.starts_with('+') {
        (false, &trimmed[1..])
    } else {
        (false, trimmed)
    };

    let value = if num_str.starts_with("0x") || num_str.starts_with("0X") {
        // 十六进制
        i64::from_str_radix(&num_str[2..], 16).map_err(|_| ())?
    } else if num_str.starts_with("0b") || num_str.starts_with("0B") {
        // 二进制
        i64::from_str_radix(&num_str[2..], 2).map_err(|_| ())?
    } else if num_str.starts_with('0') && num_str.len() > 1 {
        // 八进制
        i64::from_str_radix(&num_str[1..], 8).map_err(|_| ())?
    } else {
        // 十进制
        num_str.parse::<i64>().map_err(|_| ())?
    };

    Ok(if negative { -value } else { value })
}

/// 预处理器条件表达式解析器
///
/// 支持完整的 C 预处理器条件表达式语法
struct ConditionParser<'a> {
    input: &'a str,
    pos: usize,
    defines: &'a std::collections::HashMap<String, String>,
}

impl<'a> ConditionParser<'a> {
    fn new(input: &'a str, defines: &'a std::collections::HashMap<String, String>) -> Self {
        Self {
            input,
            pos: 0,
            defines,
        }
    }

    /// 跳过空白字符
    fn skip_whitespace(&mut self) {
        while self.pos < self.input.len() && self.input.as_bytes()[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    /// 查看当前字符
    fn peek(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    /// 消费指定字符
    fn consume_char(&mut self, expected: char) -> bool {
        self.skip_whitespace();
        if self.peek() == Some(expected) {
            self.pos += expected.len_utf8();
            true
        } else {
            false
        }
    }

    /// 消费关键字（后面不能跟字母数字）
    fn consume_keyword(&mut self, keyword: &str) -> bool {
        self.skip_whitespace();
        let remaining = &self.input[self.pos..];
        if remaining.starts_with(keyword) {
            let after = self.pos + keyword.len();
            if after >= self.input.len()
                || !self.input.as_bytes()[after].is_ascii_alphanumeric()
                    && self.input.as_bytes()[after] != b'_'
            {
                self.pos = after;
                return true;
            }
        }
        false
    }

    /// 读取标识符
    fn read_identifier(&mut self) -> Option<String> {
        self.skip_whitespace();
        let start = self.pos;
        while self.pos < self.input.len() {
            let ch = self.input.as_bytes()[self.pos];
            if ch.is_ascii_alphanumeric() || ch == b'_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        if start == self.pos {
            None
        } else {
            Some(self.input[start..self.pos].to_string())
        }
    }

    /// 读取数字
    fn read_number(&mut self) -> Option<i64> {
        self.skip_whitespace();
        let start = self.pos;

        // 处理符号
        if self.pos < self.input.len()
            && (self.input.as_bytes()[self.pos] == b'+' || self.input.as_bytes()[self.pos] == b'-')
        {
            self.pos += 1;
        }

        // 处理前缀
        if self.pos + 1 < self.input.len() {
            let prefix = &self.input[self.pos..self.pos + 2];
            if prefix == "0x" || prefix == "0X" || prefix == "0b" || prefix == "0B" {
                self.pos += 2;
            }
        }

        // 读取数字部分
        while self.pos < self.input.len() && self.input.as_bytes()[self.pos].is_ascii_hexdigit() {
            self.pos += 1;
        }

        if start == self.pos {
            None
        } else {
            parse_preprocessor_number(&self.input[start..self.pos]).ok()
        }
    }

    /// 读取操作符
    fn read_operator(&mut self) -> Option<&str> {
        self.skip_whitespace();
        let remaining = &self.input[self.pos..];

        // 两字符操作符优先
        for op in &["==", "!=", "<=", ">=", "&&", "||"] {
            if remaining.starts_with(op) {
                self.pos += op.len();
                return Some(op);
            }
        }

        // 单字符操作符
        if let Some(ch) = remaining.chars().next() {
            match ch {
                '!' | '<' | '>' | '+' | '-' | '*' | '/' | '%' | '(' | ')' => {
                    self.pos += ch.len_utf8();
                    return Some(&self.input[self.pos - ch.len_utf8()..self.pos]);
                }
                _ => {}
            }
        }

        None
    }

    /// 解析主表达式（处理 ||）
    fn parse_expression(&mut self) -> Result<i64, ()> {
        let mut result = self.parse_and_expr()?;

        loop {
            self.skip_whitespace();
            if self.consume_keyword("||") {
                let right = self.parse_and_expr()?;
                result = if result != 0 || right != 0 { 1 } else { 0 };
            } else {
                break;
            }
        }

        Ok(result)
    }

    /// 解析 AND 表达式（处理 &&）
    fn parse_and_expr(&mut self) -> Result<i64, ()> {
        let mut result = self.parse_comparison()?;

        loop {
            self.skip_whitespace();
            if self.consume_keyword("&&") {
                let right = self.parse_comparison()?;
                result = if result != 0 && right != 0 { 1 } else { 0 };
            } else {
                break;
            }
        }

        Ok(result)
    }

    /// 解析比较表达式
    fn parse_comparison(&mut self) -> Result<i64, ()> {
        let mut result = self.parse_additive()?;

        self.skip_whitespace();
        let remaining = &self.input[self.pos..];

        for op in &["==", "!=", "<=", ">=", "<", ">"] {
            if remaining.starts_with(op) {
                self.pos += op.len();
                let right = self.parse_additive()?;
                let cmp = match *op {
                    "==" => result == right,
                    "!=" => result != right,
                    "<=" => result <= right,
                    ">=" => result >= right,
                    "<" => result < right,
                    ">" => result > right,
                    _ => false,
                };
                return Ok(if cmp { 1 } else { 0 });
            }
        }

        Ok(result)
    }

    /// 解析加减表达式
    fn parse_additive(&mut self) -> Result<i64, ()> {
        let mut result = self.parse_multiplicative()?;

        loop {
            self.skip_whitespace();
            let remaining = &self.input[self.pos..];

            if remaining.starts_with('+') {
                self.pos += 1;
                let right = self.parse_multiplicative()?;
                result = result.wrapping_add(right);
            } else if remaining.starts_with('-') {
                self.pos += 1;
                let right = self.parse_multiplicative()?;
                result = result.wrapping_sub(right);
            } else {
                break;
            }
        }

        Ok(result)
    }

    /// 解析乘除表达式
    fn parse_multiplicative(&mut self) -> Result<i64, ()> {
        let mut result = self.parse_unary()?;

        loop {
            self.skip_whitespace();
            let remaining = &self.input[self.pos..];

            if remaining.starts_with('*') {
                self.pos += 1;
                let right = self.parse_unary()?;
                result = result.wrapping_mul(right);
            } else if remaining.starts_with('/') {
                self.pos += 1;
                let right = self.parse_unary()?;
                if right == 0 {
                    return Ok(0); // 除以零返回 0
                }
                result = result.wrapping_div(right);
            } else if remaining.starts_with('%') {
                self.pos += 1;
                let right = self.parse_unary()?;
                if right == 0 {
                    return Ok(0);
                }
                result = result.wrapping_rem(right);
            } else {
                break;
            }
        }

        Ok(result)
    }

    /// 解析一元表达式
    fn parse_unary(&mut self) -> Result<i64, ()> {
        self.skip_whitespace();

        // 逻辑非
        if self.consume_keyword("!") {
            let val = self.parse_primary()?;
            return Ok(if val == 0 { 1 } else { 0 });
        }

        // 按位取反
        if self.consume_keyword("~") {
            let val = self.parse_primary()?;
            return Ok(!val);
        }

        // 正号
        if self.consume_keyword("+") {
            return self.parse_primary();
        }

        // 负号
        if self.consume_keyword("-") {
            let val = self.parse_primary()?;
            return Ok(-val);
        }

        self.parse_primary()
    }

    /// 解析基本表达式（数字、标识符、括号、defined）
    fn parse_primary(&mut self) -> Result<i64, ()> {
        self.skip_whitespace();

        // 括号分组
        if self.consume_char('(') {
            let result = self.parse_expression()?;
            self.consume_char(')');
            return Ok(result);
        }

        // defined(MACRO) 或 defined MACRO
        if self.consume_keyword("defined") {
            self.skip_whitespace();
            if self.consume_char('(') {
                // defined(MACRO) 形式
                if let Some(name) = self.read_identifier() {
                    self.consume_char(')');
                    return Ok(if self.defines.contains_key(&name) {
                        1
                    } else {
                        0
                    });
                }
            } else if let Some(name) = self.read_identifier() {
                // defined MACRO 形式
                return Ok(if self.defines.contains_key(&name) {
                    1
                } else {
                    0
                });
            }
            return Ok(0);
        }

        // 数字常量
        if let Some(num) = self.read_number() {
            return Ok(num);
        }

        // 标识符（宏名）- 查找其值
        if let Some(name) = self.read_identifier() {
            if let Some(value_str) = self.defines.get(&name) {
                // 宏有值，尝试解析
                if let Ok(num) = parse_preprocessor_number(value_str) {
                    return Ok(num);
                }
                // 如果值不是数字，检查是否为空（仅定义无值）
                if value_str.trim().is_empty() {
                    return Ok(1); // 已定义但无值，视为 1
                }
            }
            return Ok(0); // 未定义的宏视为 0
        }

        Err(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_define() {
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process("#define PI 3.14\nconst double pi = PI;", "test.c")
            .unwrap();
        assert!(result.contains("const double pi = 3.14"));
    }

    #[test]
    fn test_conditional_compilation() {
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process(
                "#define DEBUG\n#ifdef DEBUG\nint debug = 1;\n#endif",
                "test.c",
            )
            .unwrap();
        assert!(result.contains("int debug = 1"));
    }

    #[test]
    fn test_ifndef() {
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process("#ifndef UNDEFINED\nint x = 1;\n#endif", "test.c")
            .unwrap();
        assert!(result.contains("int x = 1"));
    }

    #[test]
    fn test_else() {
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process(
                "#define RELEASE\n#ifdef DEBUG\nint mode = 0;\n#else\nint mode = 1;\n#endif",
                "test.c",
            )
            .unwrap();
        assert!(result.contains("int mode = 1"));
    }

    #[test]
    fn test_endif_with_comment() {
        // 测试 #endif 后面带注释的情况
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process(
                "#define TEST\n#ifdef TEST\nint x = 1;\n#endif /* TEST */",
                "test.c",
            )
            .unwrap();
        assert!(result.contains("int x = 1"));
    }
}
