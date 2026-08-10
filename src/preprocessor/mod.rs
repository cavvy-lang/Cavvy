//! Cavvy 预处理器模块
//!
//! 实现预处理指令系统（当前版本见 .verinfo，6.2.0）：
//! - #include "path"  - 文件包含（隐式 #pragma once）
//! - #include_c <header.h> / "header.h"  - 导入 C/C++ 头文件的 Cay FFI 声明 + 自动链接
//!   （仅 <...> 系统形式且命中 caylibs/c/<name>.cay 标准库包装时用包装；其余一律解析真实头文件）
//! - #include_h <header.cayh> / "header.cayh"  - 导入 Cavvy 声明文件（.cayh）
//!   与 .h/.hpp 不同，.cayh 是 Cavvy 源码，支持高级 ADT（enum 等）；约定只放
//!   "零符号"声明（enum 定义、纯 native 类声明、#define），实现在对应 .cay 中，
//!   多文件分别编译后链接（仅 <...> 系统形式且命中 caylibs/cayh/<name>.cayh 时优先命中）
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

mod c_header;
mod cpp_mangle;

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
    /// #include_c "header.h" 或 #include_c <header.h> — 导入 C 头文件的 Cay FFI 声明
    IncludeC(String, bool), // (路径, 是否系统路径)
    /// #include_h "header.cayh" 或 #include_h <header.cayh> — 导入 Cavvy 原生头文件
    IncludeH(String, bool), // (路径, 是否系统路径)
    /// #undef name
    Undef(String),
}

/// 指令处理结果
#[derive(Debug, Clone)]
enum DirectiveResult {
    /// 单行输出（普通指令）
    Single(Option<String>),
    /// 多行输出（包含文件 / 生成的 extern 块）
    /// `link_libraries` 为该包含/生成引入的链接库（含被包含文件内的 #link、
    /// 以及 #include_c 的自动链接）。修复历史 bug：此前仅回传 code/source_map，
    /// 导致被包含文件中的 #link 被丢弃。
    Multi {
        code: String,
        source_map: SourceMap,
        link_libraries: Vec<LinkLibrary>,
    },
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

    /// 从命令行风格的定义字符串中解析 (name, value)
    ///
    /// 支持 "NAME" 和 "NAME=value" 两种形式。
    fn parse_cli_define(s: &str) -> (String, String) {
        let trimmed = s.trim();
        if let Some(pos) = trimmed.find('=') {
            let name = trimmed[..pos].trim().to_string();
            let value = trimmed[pos + 1..].trim().to_string();
            (name, value)
        } else {
            (trimmed.to_string(), String::new())
        }
    }

    /// 预定义一组宏（通常来自命令行 -D/--define）
    ///
    /// 支持 "NAME" 和 "NAME=value" 形式；重复定义会覆盖之前的值。
    pub fn seed_defines(&mut self, defines: &[String]) {
        for d in defines {
            let (name, value) = Self::parse_cli_define(d);
            if !name.is_empty() {
                self.defines.insert(name, value);
            }
        }
    }

    /// 预取消定义一组宏（通常来自命令行 -U/--undefine）
    ///
    /// 在构建种子定义之后调用，用于移除不应存在的宏。
    pub fn seed_undefines(&mut self, undefines: &[String]) {
        for u in undefines {
            let name = u.trim();
            if !name.is_empty() {
                self.defines.remove(name);
            }
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
                                link_libraries: included_link_libraries,
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
                                // 合并被包含文件/生成块引入的链接库（按 name 去重）
                                for lib in included_link_libraries {
                                    if !link_libraries
                                        .iter()
                                        .any(|l: &LinkLibrary| l.name == lib.name)
                                    {
                                        link_libraries.push(lib);
                                    }
                                }
                            }
                            DirectiveResult::Link {
                                lib_name,
                                is_system,
                            } => {
                                // 收集链接库信息（按 name 去重）
                                if !link_libraries.iter().any(|l| l.name == lib_name) {
                                    link_libraries.push(LinkLibrary {
                                        name: lib_name,
                                        is_system,
                                    });
                                }
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
            "undef" => {
                // 解析 #undef name
                let name = self.parse_identifier(args, line_num, file_path)?;
                Ok(Some(Directive::Undef(name)))
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
            "include_c" => {
                // 解析 #include_c "header.h" 或 #include_c <header.h>
                // 语义与 #include 的路径解析一致，但优先映射到 .cay 包装并自动链接
                let (path, is_system) = self.parse_include_path(args, line_num, file_path)?;
                Ok(Some(Directive::IncludeC(path, is_system)))
            }
            "include_h" => {
                // 解析 #include_h "header.cayh" 或 #include_h <header.cayh>
                // 与 #include_c 结构一致，但目标是 Cavvy 原生头文件（支持 enum 等高级 ADT）
                let (path, is_system) = self.parse_include_path(args, line_num, file_path)?;
                Ok(Some(Directive::IncludeH(path, is_system)))
            }
            _ => {
                Err(CayError::Preprocessor {
                    error_code: ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                    file: Some(file_path.to_string()),
                    line: line_num,
                    column: 1,
                    message: format!("未知的预处理指令: {}", directive_name),
                    suggestion: "支持的指令: #include, #include_c, #include_h, #define, #undef, #ifdef, #ifndef, #else, #elif, #endif, #error, #warning, #link".to_string(),
                })
            }
        }
    }

    /// 移除 C 风格块注释 /* ... */
    /// 感知字符串/字符字面量：字面量内的 `/*` 不视为注释起始。
    fn remove_block_comments(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        let mut in_literal: Option<char> = None; // 当前字面量的引号（" 或 '）

        while let Some(c) = chars.next() {
            if let Some(quote) = in_literal {
                result.push(c);
                if c == '\\' {
                    // 转义字符：下一个字符原样保留
                    if let Some(next) = chars.next() {
                        result.push(next);
                    }
                } else if c == quote {
                    in_literal = None;
                }
                continue;
            }

            if c == '"' || c == '\'' {
                in_literal = Some(c);
                result.push(c);
            } else if c == '/' && chars.peek() == Some(&'*') {
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

                        // 返回处理后的内容和源映射（同时回传被包含文件引入的链接库，
                        // 修复历史 bug：被包含文件中的 #link 此前会被丢弃）
                        Ok(DirectiveResult::Multi {
                            code: included_result.code,
                            source_map: included_result.source_map,
                            link_libraries: included_result.link_libraries,
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
            Directive::Undef(name) => {
                if self.skipping {
                    return Ok(DirectiveResult::Single(None));
                }
                self.defines.remove(&name);
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
                // 完整条件表达式求值（递归下降，见 ConditionParser）
                let condition = self.evaluate_condition(&expr, file_path, line_num);
                self.push_conditional(condition);
                Ok(DirectiveResult::Single(None))
            }
            Directive::Else => {
                self.handle_else(file_path)?;
                Ok(DirectiveResult::Single(None))
            }
            Directive::Elif(expr) => {
                let condition = self.evaluate_condition(&expr, file_path, line_num);
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
                    emit_preprocessor_warning(
                        Some(file_path),
                        line_num,
                        format_args!("#warning: {}", message),
                    );
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
            Directive::IncludeC(path, is_system) => {
                self.process_include_c(&path, is_system, file_path, line_num)
            }
            Directive::IncludeH(path, is_system) => {
                self.process_include_h(&path, is_system, file_path, line_num)
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
    fn evaluate_condition(&self, expr: &str, file_path: &str, line_num: usize) -> bool {
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
            Err(_) => {
                // 解析失败按 C 预处理语义回退为 false，但必须告知用户而非静默吞错
                emit_preprocessor_warning(
                    Some(file_path),
                    line_num,
                    format_args!("#if 条件表达式 '{}' 解析失败，按 false 处理", trimmed),
                );
                false
            }
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

    /// 处理 #include_c 指令：导入 C/C++ 头文件的 Cay FFI 声明并自动链接。
    ///
    /// 解析顺序：
    /// 1. **标准库白名单包装**（仅 <...> 系统形式，且命中 caylibs/c/<base>.cay）——
    ///    包装是手写、类型正确的 Cay 声明，命中即递归包含（复用 #pragma once / 循环检测）。
    /// 2. **真实头文件解析**—— 其余所有情况（含全部 "..." 形式）：定位磁盘上的真实
    ///    头文件，用提取器（c_header 模块）把声明转成 Cay 源码（C：extern 块；
    ///    C++：interop class + native 方法），只产出能干净映射的声明。
    /// 3. 两层都按"头名→库"映射（c_header::c_header_link_libs）自动声明链接。
    fn process_include_c(
        &mut self,
        path: &str,
        is_system: bool,
        current_file: &str,
        line_num: usize,
    ) -> CayResult<DirectiveResult> {
        if self.skipping {
            return Ok(DirectiveResult::Single(None));
        }

        // (a) 规范化头名：去掉 .h / .cay 后缀，保留子目录（如 sys/socket）
        let base = Self::strip_header_ext(path);

        // (b) 仅 <...> 系统形式允许映射到标准库白名单包装（caylibs/c/<base>.cay）；
        //     "..." 形式与所有其他头名一律解析真实头文件，同名 .cay 不参与匹配。
        if is_system {
            if let Some(cay_path) = self.resolve_cay_wrapper(&base, current_file) {
                return self.process_resolved_include(&cay_path, current_file, &base);
            }
        }

        // (c) 兜底：定位真实 .h 并解析
        if let Some(header_path) = self.resolve_real_header(path, is_system, current_file) {
            let canon = std::fs::canonicalize(&header_path)
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or(header_path);
            // 一次性去重（与 #pragma once / 包装路径共享 included_files）
            if self.included_files.contains(&canon) {
                return Ok(DirectiveResult::Single(Some(String::new())));
            }
            let content = std::fs::read_to_string(&canon).map_err(|e| CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_INCLUDE_C_ERROR,
                file: Some(canon.clone()),
                line: 1,
                column: 1,
                message: format!("#include_c: 无法读取头文件 '{}': {}", canon, e),
                suggestion: "检查文件路径与权限".to_string(),
            })?;
            let extract =
                c_header::extract_c_header_text(&canon, &content, &self.defines)?;
            for w in &extract.warnings {
                emit_preprocessor_warning(
                    Some(&canon),
                    1,
                    format_args!("#include_c {}: {}", path, w),
                );
            }
            // 自动链接：提取器推断 + 头名映射（去重）
            let mut libs = extract.link_libraries;
            for lib in c_header::c_header_link_libs(&base) {
                if !libs.iter().any(|l| l.name == lib.name) {
                    libs.push(lib);
                }
            }
            // 合成 extern 块 + 每行源映射（指向头文件）
            let (code, source_map) = synthesize_extern_block(&extract.extern_code, &canon);
            self.included_files.insert(canon);
            return Ok(DirectiveResult::Multi {
                code,
                source_map,
                link_libraries: libs,
            });
        }

        // 全找不到
        let (target, suggestion) = if is_system {
            (
                format!("#include_c: 找不到 C 头文件 '{}'（无标准库包装，也未找到真实头）", path),
                "用 -I 指定头文件搜索路径，或在 caylibs/c/ 下放置 <name>.cay 包装".to_string(),
            )
        } else {
            (
                format!("#include_c: 找不到头文件 '{}'", path),
                "检查相对路径，或用 -I 指定头文件搜索路径".to_string(),
            )
        };
        Err(CayError::Preprocessor {
            error_code: ErrorCodes::PREPROCESSOR_INCLUDE_C_ERROR,
            file: Some(current_file.to_string()),
            line: line_num,
            column: 1,
            message: target,
            suggestion,
        })
    }

    /// 处理 #include_h 指令：导入 Cavvy 声明文件（.cayh）。
    ///
    /// .cayh 是 C 式 .h/.c 分离模型中的声明文件：与 .h/.hpp 不同，它是 Cavvy
    /// 源码，可以直接声明高级 ADT（enum 等）。约定 .cayh 只放"零符号"声明——
    /// enum 定义、纯 native 方法/构造的类声明、#define 常量；实现放在对应的
    /// .cay 中，经 `cayc helper.cay main.cay` 分别编译后链接解析。
    /// 实现文件可以像 C 一样 #include_h 自己的头文件：纯声明类会与同 TU 的
    /// 同名实现类合并（semantic 阶段），不产生重复定义。
    ///
    /// 解析顺序（镜像 #include_c）：
    /// 1. **标准库白名单**（仅 <...> 系统形式，且命中 caylibs/cayh/<base>.cayh）——
    ///    命中即用该声明文件；
    /// 2. **真实文件解析**—— 其余所有情况（含全部 "..." 形式）：按 #include 的搜索
    ///    顺序定位磁盘上的 .cayh 并递归预处理；
    /// 3. 按"头名→库"映射（c_header::c_header_link_libs）自动声明链接。
    fn process_include_h(
        &mut self,
        path: &str,
        is_system: bool,
        current_file: &str,
        line_num: usize,
    ) -> CayResult<DirectiveResult> {
        if self.skipping {
            return Ok(DirectiveResult::Single(None));
        }

        // (a) 规范化头名：去掉 .cayh 等后缀，保留子目录
        let base = Self::strip_header_ext(path);

        // (b) 仅 <...> 系统形式允许映射到标准库白名单头（caylibs/cayh/<base>.cayh）
        if is_system {
            if let Some(cayh_path) = self.resolve_cayh_wrapper(&base, current_file) {
                return self.process_resolved_include(&cayh_path, current_file, &base);
            }
        }

        // (c) 定位真实 .cayh 并按 Cavvy 源码递归包含（支持 enum 等高级 ADT）
        if let Ok(header_path) = self.resolve_include_path(path, is_system, current_file) {
            return self.process_resolved_include(&header_path, current_file, &base);
        }

        // 全找不到
        let (target, suggestion) = if is_system {
            (
                format!("#include_h: 找不到 Cavvy 头文件 '{}'（无标准库头，也未找到真实文件）", path),
                "用 -I 指定头文件搜索路径，或在 caylibs/cayh/ 下放置 <name>.cayh".to_string(),
            )
        } else {
            (
                format!("#include_h: 找不到头文件 '{}'", path),
                "检查相对路径，或用 -I 指定头文件搜索路径".to_string(),
            )
        };
        Err(CayError::Preprocessor {
            error_code: ErrorCodes::PREPROCESSOR_INCLUDE_H_ERROR,
            file: Some(current_file.to_string()),
            line: line_num,
            column: 1,
            message: target,
            suggestion,
        })
    }

    /// 解析 #include_h 对应的标准库头路径（仅 <...> 系统形式可达，见 process_include_h）。
    /// 只匹配 `cayh/<base>.cayh`（caylibs/cayh/ 白名单命名空间），复用
    /// `resolve_include_path` 的搜索根（exe-dir/caylibs、cwd/caylibs、-I 等）。
    fn resolve_cayh_wrapper(&self, base: &str, current_file: &str) -> Option<String> {
        let cand = format!("cayh/{}.cayh", base);
        self.resolve_include_path(&cand, true, current_file).ok()
    }

    /// 处理已解析路径的 .cay 包装 / .cayh 原生头：读取 + 一次性/循环检测 + 递归预处理 + 自动链接。
    /// 不再走 `read_include_file` 的二次解析（避免对相对路径重复 join 当前目录）。
    fn process_resolved_include(
        &mut self,
        resolved_path: &str,
        current_file: &str,
        base: &str,
    ) -> CayResult<DirectiveResult> {
        let canon = std::fs::canonicalize(resolved_path)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|_| resolved_path.to_string());

        // 循环包含检测
        if self.include_stack.contains(&canon) {
            return Err(CayError::Preprocessor {
                error_code: ErrorCodes::PREPROCESSOR_CIRCULAR_INCLUDE,
                file: Some(current_file.to_string()),
                line: 1,
                column: 1,
                message: format!("检测到循环包含: {}", canon),
                suggestion: "检查头文件之间的循环依赖".to_string(),
            });
        }
        // 一次性（#pragma once 语义）
        if self.included_files.contains(&canon) {
            return Ok(DirectiveResult::Single(Some(String::new())));
        }

        let content = std::fs::read_to_string(&canon).map_err(|e| CayError::Preprocessor {
            error_code: ErrorCodes::PREPROCESSOR_INCLUDE_ERROR,
            file: Some(current_file.to_string()),
            line: 1,
            column: 1,
            message: format!("无法读取包含文件 '{}': {}", canon, e),
            suggestion: "检查文件路径是否正确".to_string(),
        })?;
        self.included_files.insert(canon.clone());

        // 保存/重置条件编译状态，递归处理
        self.include_stack.push(canon.clone());
        let saved_conditional_stack = self.conditional_stack.clone();
        let saved_skipping = self.skipping;
        self.conditional_stack = Vec::new();
        self.skipping = false;
        let included_result = self.process_with_source_map(&content, &canon)?;
        self.conditional_stack = saved_conditional_stack;
        self.skipping = saved_skipping;
        self.include_stack.pop();

        // 合并被包含文件引入的链接库 + 头名自动链接（去重）
        let mut libs = included_result.link_libraries;
        for lib in c_header::c_header_link_libs(base) {
            if !libs.iter().any(|l| l.name == lib.name) {
                libs.push(lib);
            }
        }
        Ok(DirectiveResult::Multi {
            code: included_result.code,
            source_map: included_result.source_map,
            link_libraries: libs,
        })
    }

    /// 解析 #include_c 对应的标准库包装路径（仅 <...> 系统形式可达，见 process_include_c）。
    /// 只匹配 `c/<base>.cay`（caylibs/c/ 白名单命名空间），不匹配任意同名 `<base>.cay`——
    /// 后者会被用户源文件/第三方库中的同名 .cay 劫持，遮蔽真实头文件。
    /// 复用 `resolve_include_path` 的搜索根（exe-dir/caylibs、cwd/caylibs、-I 等）。
    fn resolve_cay_wrapper(&self, base: &str, current_file: &str) -> Option<String> {
        let cand = format!("c/{}.cay", base);
        self.resolve_include_path(&cand, true, current_file).ok()
    }

    /// 兜底：定位磁盘上的真实 .h。用户形式相对当前文件/基础目录；系统形式查 -I 与
    /// 捆绑的 freestanding C 头目录。返回 None 表示找不到（不报错，由调用方决定）。
    fn resolve_real_header(
        &self,
        path: &str,
        is_system: bool,
        current_file: &str,
    ) -> Option<String> {
        // 用户形式：相对当前文件目录、基础目录
        if !is_system {
            let current_dir = Path::new(current_file)
                .parent()
                .unwrap_or_else(|| Path::new("."));
            let p = current_dir.join(path);
            if p.exists() {
                return Some(p.to_string_lossy().to_string());
            }
            let bp = self.base_dir.join(path);
            if bp.exists() {
                return Some(bp.to_string_lossy().to_string());
            }
        }
        // -I 路径（两种形式都查）
        for ip in &self.system_include_paths {
            let p = ip.join(path);
            if p.exists() {
                return Some(p.to_string_lossy().to_string());
            }
        }
        // 系统形式：捆绑的 freestanding C 头目录
        if is_system {
            for bp in bundled_c_include_paths() {
                let p = bp.join(path);
                if p.exists() {
                    return Some(p.to_string_lossy().to_string());
                }
            }
        }
        None
    }

    /// 去掉头文件路径的单个 `.h`/`.hpp`/`.hh`/`.hxx`/`.cay`/`.cayh` 后缀，保留其余（含子目录）。
    fn strip_header_ext(p: &str) -> String {
        let p = p.trim();
        p.strip_suffix(".cayh")
            .or_else(|| p.strip_suffix(".hpp"))
            .or_else(|| p.strip_suffix(".hxx"))
            .or_else(|| p.strip_suffix(".hh"))
            .or_else(|| p.strip_suffix(".h"))
            .or_else(|| p.strip_suffix(".cay"))
            .unwrap_or(p)
            .to_string()
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
    /// 感知字符串/字符字面量：字面量内的 `//`、`/*` 不视为注释；
    /// 且 `//` 仅在位于行首或前一个字符是空白时才视为注释起始，
    /// 避免截断 `#define URL http://x.com` 这类包含 `//` 的值。
    fn remove_line_comments(line: &str) -> String {
        let chars: Vec<char> = line.chars().collect();
        let mut result = String::with_capacity(line.len());
        let mut in_literal: Option<char> = None; // 当前字面量的引号（" 或 '）
        let mut i = 0;

        while i < chars.len() {
            let c = chars[i];
            if let Some(quote) = in_literal {
                result.push(c);
                if c == '\\' && i + 1 < chars.len() {
                    // 转义字符：下一个字符原样保留
                    result.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == quote {
                    in_literal = None;
                }
                i += 1;
                continue;
            }

            if c == '"' || c == '\'' {
                in_literal = Some(c);
                result.push(c);
                i += 1;
            } else if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
                // // 仅在行首或前置空白时视为注释（保护 URL 等值中的 //）
                let prev_is_boundary = result
                    .chars()
                    .last()
                    .map(|p| p.is_whitespace())
                    .unwrap_or(true);
                if prev_is_boundary {
                    break; // 丢弃行尾注释
                }
                result.push(c);
                i += 1;
            } else if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
                // 单行块注释：跳过到 */；未闭合则按普通字符保留（交由后续解析报错）
                let mut j = i + 2;
                let mut closed = false;
                while j + 1 < chars.len() {
                    if chars[j] == '*' && chars[j + 1] == '/' {
                        closed = true;
                        j += 2;
                        break;
                    }
                    j += 1;
                }
                if closed {
                    i = j;
                } else {
                    result.push(c);
                    i += 1;
                }
            } else {
                result.push(c);
                i += 1;
            }
        }

        result.trim_end().to_string()
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
    /// 宏表按名称长度降序预排序一次（最长匹配优先），不随逐字符扫描重复排序；
    /// 字符串/字符字面量内容原样保留，不做替换（如 `"PI"` 中的 PI 不展开）。
    fn expand_macros(&self, line: &str) -> String {
        // 按长度降序排序宏定义，确保优先匹配更长的名称
        // 例如 FILE_MODE_READPLUS 应该在 FILE_MODE_READ 之前被检查
        let mut defines_sorted: Vec<_> = self.defines.iter().collect();
        defines_sorted.sort_by(|(a, _), (b, _)| b.len().cmp(&a.len()));

        let mut result = String::with_capacity(line.len());
        let mut i = 0;
        let chars: Vec<char> = line.chars().collect();
        let mut in_literal: Option<char> = None; // 当前字面量的引号（" 或 '）

        while i < chars.len() {
            // 字面量内部：原样复制，跳过宏替换
            if let Some(quote) = in_literal {
                let c = chars[i];
                result.push(c);
                if c == '\\' && i + 1 < chars.len() {
                    // 转义字符：下一个字符原样保留
                    result.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if c == quote {
                    in_literal = None;
                }
                i += 1;
                continue;
            }
            if chars[i] == '"' || chars[i] == '\'' {
                in_literal = Some(chars[i]);
                result.push(chars[i]);
                i += 1;
                continue;
            }

            // 尝试匹配最长的宏名称
            let mut matched = false;

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
            // 如果已经在跳过状态（外层条件为真），内层条件也应完全跳过。
            // 使用 Done 状态确保 #else/#elif 不会错误激活内层分支。
            self.conditional_stack.push(ConditionalState::Done);
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

/// 通过项目警告体系输出预处理警告
///
/// 构造 `CayError` 警告（`preprocessor_warning`），并经项目统一的
/// `print_warning` 显示；消息中带 `文件:行号` 前缀便于定位。
fn emit_preprocessor_warning(file: Option<&str>, line: usize, message: impl std::fmt::Display) {
    let text = match file {
        Some(f) => format!("{}:{}: {}", f, line, message),
        None => format!("{}", message),
    };
    let warning = crate::miette_diagnostic::preprocessor_warning(
        ErrorCodes::PREPROCESSOR_WARNING,
        file.map(|f| f.to_string()),
        line,
        1,
        text,
    );
    crate::miette_diagnostic::print_warning(&warning.message());
}

/// 将生成的 `extern {}` 块文本拆分为多行，并构造每行指向头文件的源映射。
/// 用于 `#include_c` 兜底解析真实 `.h` 的产物（每行映射到头文件路径，行号统一为 1）。
fn synthesize_extern_block(extern_code: &str, header_path: &str) -> (String, SourceMap) {
    let mut source_map = SourceMap::new();
    let lines: Vec<&str> = extern_code.lines().collect();
    for _ in &lines {
        source_map.add_mapping(header_path.to_string(), 1);
    }
    (lines.join("\n"), source_map)
}

/// 捆绑的 freestanding C 头文件搜索目录（相对可执行文件）。
/// bundled MinGW 不含完整 libc 头（如 stdio.h），仅 GCC freestanding 头与 clang 资源头；
/// 这些目录主要用于 `<stdint.h>`/`<stddef.h>` 等的兜底解析。
/// GCC/clang 的版本号目录按磁盘实际情况动态探测，不存在的候选直接过滤，不做硬编码。
fn bundled_c_include_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        let exe = exe.canonicalize().unwrap_or(exe);
        if let Some(exe_dir) = exe.parent() {
            // GCC freestanding 头：lib/mingw64/lib/gcc/<triplet>/<version>/include
            let gcc_root = exe_dir.join("lib/mingw64/lib/gcc");
            if let Ok(triplets) = std::fs::read_dir(&gcc_root) {
                for triplet in triplets.flatten() {
                    if let Ok(versions) = std::fs::read_dir(triplet.path()) {
                        for ver in versions.flatten() {
                            let inc = ver.path().join("include");
                            if inc.is_dir() {
                                paths.push(inc);
                            }
                        }
                    }
                }
            }
            // clang 资源头：llvm-minimal/lib/clang/<version>/include
            // （捆绑的 llvm-minimal 为扁平布局时该目录不存在，自动被过滤）
            let clang_root = exe_dir.join("llvm-minimal/lib/clang");
            if let Ok(versions) = std::fs::read_dir(&clang_root) {
                for ver in versions.flatten() {
                    let inc = ver.path().join("include");
                    if inc.is_dir() {
                        paths.push(inc);
                    }
                }
            }
        }
    }
    paths
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
pub(crate) fn parse_preprocessor_number(s: &str) -> Result<i64, ()> {
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
pub(crate) struct ConditionParser<'a> {
    input: &'a str,
    pos: usize,
    defines: &'a std::collections::HashMap<String, String>,
}

impl<'a> ConditionParser<'a> {
    pub(crate) fn new(
        input: &'a str,
        defines: &'a std::collections::HashMap<String, String>,
    ) -> Self {
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

    /// 消费操作符（不做后随字符检查）
    ///
    /// 与 `consume_keyword` 不同，操作符后面可以紧跟标识符/数字，
    /// 以支持 `#if !defined(FOO)`、`#if A&&B` 这类无空格写法。
    fn consume_operator(&mut self, op: &str) -> bool {
        self.skip_whitespace();
        if self.input[self.pos..].starts_with(op) {
            self.pos += op.len();
            return true;
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
        let len = self.input.len();
        let bytes = self.input.as_bytes();

        // 处理符号
        if self.pos < len && (bytes[self.pos] == b'+' || bytes[self.pos] == b'-') {
            self.pos += 1;
        }

        // 数字必须以数字字符开头，否则回退到起始位置：
        // 避免把 A-F 开头的标识符（如 FOO）误吞为“十六进制数字”
        if self.pos >= len || !bytes[self.pos].is_ascii_digit() {
            self.pos = start;
            return None;
        }

        // 处理前缀
        if self.pos + 1 < len {
            let prefix = &self.input[self.pos..self.pos + 2];
            if prefix == "0x" || prefix == "0X" || prefix == "0b" || prefix == "0B" {
                self.pos += 2;
            }
        }

        // 读取数字部分
        while self.pos < len && bytes[self.pos].is_ascii_hexdigit() {
            self.pos += 1;
        }

        parse_preprocessor_number(&self.input[start..self.pos]).ok()
    }

    /// 解析主表达式（处理 ||）
    pub(crate) fn parse_expression(&mut self) -> Result<i64, ()> {
        let mut result = self.parse_and_expr()?;

        loop {
            self.skip_whitespace();
            if self.consume_operator("||") {
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
            if self.consume_operator("&&") {
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
        if self.consume_operator("!") {
            let val = self.parse_primary()?;
            return Ok(if val == 0 { 1 } else { 0 });
        }

        // 按位取反
        if self.consume_operator("~") {
            let val = self.parse_primary()?;
            return Ok(!val);
        }

        // 正号
        if self.consume_operator("+") {
            return self.parse_primary();
        }

        // 负号
        if self.consume_operator("-") {
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

    #[test]
    fn test_seed_define() {
        let mut pp = Preprocessor::new(".");
        pp.seed_defines(&vec!["FEATURE".to_string(), "VALUE=42".to_string()]);
        let result = pp
            .process("#ifdef FEATURE\nint feature = 1;\n#endif\nint x = VALUE;", "test.c")
            .unwrap();
        assert!(result.contains("int feature = 1;"));
        assert!(result.contains("int x = 42;"));
    }

    #[test]
    fn test_seed_undefine() {
        let mut pp = Preprocessor::new(".");
        pp.seed_defines(&vec!["FEATURE".to_string()]);
        pp.seed_undefines(&vec!["FEATURE".to_string()]);
        let result = pp
            .process("#ifdef FEATURE\nint feature = 1;\n#endif\nint x = 2;", "test.c")
            .unwrap();
        assert!(!result.contains("int feature = 1;"));
        assert!(result.contains("int x = 2;"));
    }

    #[test]
    fn test_undef_directive() {
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process(
                "#define FEATURE\n#ifdef FEATURE\nint a = 1;\n#endif\n#undef FEATURE\n#ifdef FEATURE\nint b = 2;\n#endif",
                "test.c",
            )
            .unwrap();
        assert!(result.contains("int a = 1;"));
        assert!(!result.contains("int b = 2;"));
    }

    #[test]
    fn test_nested_conditional_in_else() {
        // 验证 #else 分支中嵌套的 #ifdef 不会错误激活
        let mut pp = Preprocessor::new(".");
        pp.seed_defines(&vec!["OUTER".to_string()]);
        let result = pp
            .process(
                "#ifdef OUTER\nint a = 1;\n#else\n#ifdef INNER\nint a = 2;\n#else\nint a = 3;\n#endif\n#endif",
                "test.c",
            )
            .unwrap();
        assert!(result.contains("int a = 1;"));
        assert!(!result.contains("int a = 2;"));
        assert!(!result.contains("int a = 3;"));
    }

    #[test]
    fn test_if_not_defined_no_space() {
        // 操作符后紧跟标识符的无空格写法：#if !defined(X)
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process("#if !defined(FOO)\nint a = 1;\n#endif", "test.c")
            .unwrap();
        assert!(result.contains("int a = 1"));

        // 已定义时应跳过
        let mut pp2 = Preprocessor::new(".");
        pp2.seed_defines(&vec!["FOO".to_string()]);
        let result2 = pp2
            .process("#if !defined(FOO)\nint a = 1;\n#endif\nint b = 2;", "test.c")
            .unwrap();
        assert!(!result2.contains("int a = 1"));
        assert!(result2.contains("int b = 2"));
    }

    #[test]
    fn test_if_and_no_space() {
        // #if A&&B 无空格写法
        let mut pp = Preprocessor::new(".");
        pp.seed_defines(&vec!["A=1".to_string(), "B=1".to_string()]);
        let result = pp
            .process("#if A&&B\nint x = 1;\n#endif", "test.c")
            .unwrap();
        assert!(result.contains("int x = 1"));

        // B 为 0 时整体为假
        let mut pp2 = Preprocessor::new(".");
        pp2.seed_defines(&vec!["A=1".to_string(), "B=0".to_string()]);
        let result2 = pp2
            .process("#if A&&B\nint x = 1;\n#endif\nint y = 2;", "test.c")
            .unwrap();
        assert!(!result2.contains("int x = 1"));
        assert!(result2.contains("int y = 2"));
    }

    #[test]
    fn test_if_or_no_space() {
        // #if A||B 无空格写法
        let mut pp = Preprocessor::new(".");
        pp.seed_defines(&vec!["A=1".to_string()]);
        let result = pp
            .process("#if A||B\nint x = 1;\n#endif", "test.c")
            .unwrap();
        assert!(result.contains("int x = 1"));

        // 两者都未定义/为 0 时整体为假
        let mut pp2 = Preprocessor::new(".");
        let result2 = pp2
            .process("#if A||B\nint x = 1;\n#endif\nint y = 2;", "test.c")
            .unwrap();
        assert!(!result2.contains("int x = 1"));
        assert!(result2.contains("int y = 2"));
    }

    #[test]
    fn test_if_parse_failure_falls_back_to_false() {
        // 无法解析的条件表达式回退为 false（并发出警告），不终止编译
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process("#if @@bad@@\nint a = 1;\n#endif\nint b = 2;", "test.c")
            .unwrap();
        assert!(!result.contains("int a = 1"));
        assert!(result.contains("int b = 2"));
    }

    #[test]
    fn test_macro_not_expanded_in_string_literal() {
        // 字符串/字符字面量内的宏名不得替换
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process(
                "#define PI 3.14\nconst string s = \"PI\";\nconst string t = \"value is PI ok\";",
                "test.c",
            )
            .unwrap();
        assert!(result.contains("\"PI\""));
        assert!(result.contains("\"value is PI ok\""));
    }

    #[test]
    fn test_define_value_with_url_not_truncated() {
        // #define 值中的 // （如 URL）不得被当作行注释截断
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process(
                "#define URL http://x.com\nstring url = URL;",
                "test.c",
            )
            .unwrap();
        assert!(result.contains("http://x.com"));
    }

    #[test]
    fn test_define_trailing_line_comment_still_stripped() {
        // 前置空白的 // 注释仍应被移除
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process("#define X 1 // trailing comment\nint x = X;", "test.c")
            .unwrap();
        assert!(result.contains("int x = 1;"));
    }

    #[test]
    fn test_block_comment_inside_string_preserved() {
        // 字符串字面量内的 /* */ 不得被当作注释移除
        let mut pp = Preprocessor::new(".");
        let result = pp
            .process("#define S \"a /* b\"\nstring s = S;", "test.c")
            .unwrap();
        assert!(result.contains("\"a /* b\""));
    }

    #[test]
    fn test_bundled_c_include_paths_all_exist() {
        // 探测出的候选路径必须真实存在（不存在的已被过滤）
        for p in bundled_c_include_paths() {
            assert!(p.is_dir(), "候选路径不存在: {}", p.display());
        }
    }

    #[test]
    fn test_include_h_native_cayh_header() {
        // .cayh 按 Cavvy 源码包含：enum 等高级 ADT 原样展开（区别于 #include_c 的 FFI 提取）
        let dir = std::env::temp_dir().join(format!("cay_include_h_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let header = dir.join("shape.cayh");
        std::fs::write(
            &header,
            "public enum Shape {\n    Circle(double),\n    Point\n}\n",
        )
        .unwrap();
        let main = dir.join("main.cay");

        let mut pp = Preprocessor::new(&dir);
        let result = pp
            .process("#include_h \"shape.cayh\"\nint x = 1;", main.to_str().unwrap())
            .unwrap();
        assert!(result.contains("public enum Shape"));
        assert!(result.contains("Circle(double)"));
        assert!(result.contains("int x = 1;"));

        // pragma once 语义：第二次包含同一 .cayh 不再展开
        let result2 = pp
            .process("#include_h \"shape.cayh\"\nint y = 2;", main.to_str().unwrap())
            .unwrap();
        assert!(!result2.contains("public enum Shape"));
        assert!(result2.contains("int y = 2;"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_include_h_missing_header_error() {
        let mut pp = Preprocessor::new(".");
        let err = pp
            .process("#include_h \"no_such_header.cayh\"\nint x = 1;", "test.c")
            .unwrap_err();
        let msg = format!("{:?}", err);
        assert!(msg.contains("E1008"), "应报告 E1008，实际: {}", msg);
        assert!(msg.contains("#include_h"), "错误信息应含 #include_h，实际: {}", msg);
    }
}
