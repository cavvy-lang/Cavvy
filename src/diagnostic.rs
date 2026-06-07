//! Cavvy 编译器诊断系统
//!
//! 提供全面的错误、警告和提示信息管理系统。
//! 支持多错误收集、错误代码、详细的上下文信息和修复建议。

use std::collections::HashMap;
use std::fmt;

/// 错误严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    /// 提示信息，不影响编译
    Note,
    /// 警告，编译继续但可能有问题
    Warning,
    /// 错误，编译失败
    Error,
    /// 致命错误，立即停止编译
    Fatal,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Note => write!(f, "提示"),
            Severity::Warning => write!(f, "警告"),
            Severity::Error => write!(f, "错误"),
            Severity::Fatal => write!(f, "致命错误"),
        }
    }
}

/// 编译阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilationPhase {
    /// 预处理器
    Preprocessor,
    /// 词法分析
    Lexer,
    /// 语法分析
    Parser,
    /// 语义分析
    Semantic,
    /// 代码生成
    CodeGen,
    /// 链接
    Linker,
}

impl fmt::Display for CompilationPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompilationPhase::Preprocessor => write!(f, "预处理器"),
            CompilationPhase::Lexer => write!(f, "词法分析"),
            CompilationPhase::Parser => write!(f, "语法分析"),
            CompilationPhase::Semantic => write!(f, "语义分析"),
            CompilationPhase::CodeGen => write!(f, "代码生成"),
            CompilationPhase::Linker => write!(f, "链接器"),
        }
    }
}

/// 源代码位置 - 使用 error 模块中的定义
pub use crate::error::SourceLocation;

/// 源代码范围
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: SourceLocation,
    pub end: SourceLocation,
}

impl SourceSpan {
    pub fn new(start_line: usize, start_col: usize, end_line: usize, end_col: usize) -> Self {
        Self {
            start: SourceLocation::new(None, start_line, start_col),
            end: SourceLocation::new(None, end_line, end_col),
        }
    }

    pub fn single(line: usize, column: usize) -> Self {
        Self {
            start: SourceLocation::new(None, line, column),
            end: SourceLocation::new(None, line, column),
        }
    }
}

/// 修复建议
#[derive(Debug, Clone)]
pub struct FixSuggestion {
    /// 建议描述
    pub description: String,
    /// 替换的代码片段（如果有）
    pub replacement: Option<String>,
    /// 替换范围
    pub span: Option<SourceSpan>,
}

impl FixSuggestion {
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            replacement: None,
            span: None,
        }
    }

    pub fn with_replacement(mut self, replacement: impl Into<String>, span: SourceSpan) -> Self {
        self.replacement = Some(replacement.into());
        self.span = Some(span);
        self
    }
}

/// 诊断信息（错误、警告、提示）
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// 错误代码
    pub code: String,
    /// 严重程度
    pub severity: Severity,
    /// 编译阶段
    pub phase: CompilationPhase,
    /// 错误消息
    pub message: String,
    /// 详细说明
    pub details: Option<String>,
    /// 源代码位置
    pub location: SourceLocation,
    /// 源代码范围
    pub span: Option<SourceSpan>,
    /// 修复建议
    pub suggestions: Vec<FixSuggestion>,
    /// 相关上下文信息
    pub related_info: Vec<RelatedInfo>,
}

/// 相关信息（用于提供额外的上下文）
#[derive(Debug, Clone)]
pub struct RelatedInfo {
    pub message: String,
    pub location: SourceLocation,
}

impl Diagnostic {
    /// 创建新的诊断信息
    pub fn new(
        code: impl Into<String>,
        severity: Severity,
        phase: CompilationPhase,
        message: impl Into<String>,
        location: SourceLocation,
    ) -> Self {
        Self {
            code: code.into(),
            severity,
            phase,
            message: message.into(),
            details: None,
            location,
            span: None,
            suggestions: Vec::new(),
            related_info: Vec::new(),
        }
    }

    /// 创建错误级别的诊断
    pub fn error(
        code: impl Into<String>,
        phase: CompilationPhase,
        message: impl Into<String>,
        location: SourceLocation,
    ) -> Self {
        Self::new(code, Severity::Error, phase, message, location)
    }

    /// 创建警告级别的诊断
    pub fn warning(
        code: impl Into<String>,
        phase: CompilationPhase,
        message: impl Into<String>,
        location: SourceLocation,
    ) -> Self {
        Self::new(code, Severity::Warning, phase, message, location)
    }

    /// 添加详细说明
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    /// 添加源代码范围
    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// 添加修复建议
    pub fn with_suggestion(mut self, suggestion: FixSuggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

    /// 添加相关信息
    pub fn with_related_info(
        mut self,
        message: impl Into<String>,
        location: SourceLocation,
    ) -> Self {
        self.related_info.push(RelatedInfo {
            message: message.into(),
            location,
        });
        self
    }
}

/// 诊断收集器
#[derive(Debug, Clone, Default)]
pub struct DiagnosticCollector {
    diagnostics: Vec<Diagnostic>,
    max_errors: usize,
    error_count: usize,
    warning_count: usize,
}

impl DiagnosticCollector {
    pub fn new() -> Self {
        Self {
            diagnostics: Vec::new(),
            max_errors: 100, // 默认最多收集100个错误
            error_count: 0,
            warning_count: 0,
        }
    }

    pub fn with_max_errors(mut self, max: usize) -> Self {
        self.max_errors = max;
        self
    }

    /// 添加诊断信息
    pub fn add(&mut self, diagnostic: Diagnostic) {
        match diagnostic.severity {
            Severity::Error | Severity::Fatal => {
                if self.error_count >= self.max_errors {
                    return;
                }
                self.error_count += 1;
            }
            Severity::Warning => {
                self.warning_count += 1;
            }
            _ => {}
        }
        self.diagnostics.push(diagnostic);
    }

    /// 检查是否有错误（不包括警告）
    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    /// 检查是否有致命错误
    pub fn has_fatal_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Fatal)
    }

    /// 检查是否达到最大错误数
    pub fn is_max_errors_reached(&self) -> bool {
        self.error_count >= self.max_errors
    }

    /// 获取所有诊断信息
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 获取错误数量
    pub fn error_count(&self) -> usize {
        self.error_count
    }

    /// 获取警告数量
    pub fn warning_count(&self) -> usize {
        self.warning_count
    }

    /// 清空所有诊断信息
    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.error_count = 0;
        self.warning_count = 0;
    }

    /// 合并另一个收集器的诊断信息
    pub fn merge(&mut self, other: DiagnosticCollector) {
        for diag in other.diagnostics {
            self.add(diag);
        }
    }
}

/// 错误代码定义
pub struct ErrorCodes;

impl ErrorCodes {
    // 预处理器错误 (E1xxx)
    pub const PREPROCESSOR_DEFINE_ERROR: &'static str = "E1001";
    pub const PREPROCESSOR_IFDEF_ERROR: &'static str = "E1002";
    pub const PREPROCESSOR_INCLUDE_ERROR: &'static str = "E1003";
    pub const PREPROCESSOR_UNCLOSED_DIRECTIVE: &'static str = "E1004";
    pub const PREPROCESSOR_CIRCULAR_INCLUDE: &'static str = "E1005";
    pub const PREPROCESSOR_INVALID_MACRO: &'static str = "E1006";

    // 词法错误 (E2xxx)
    pub const LEXER_INVALID_CHARACTER: &'static str = "E2001";
    pub const LEXER_UNTERMINATED_STRING: &'static str = "E2002";
    pub const LEXER_INVALID_ESCAPE_SEQUENCE: &'static str = "E2003";
    pub const LEXER_INVALID_NUMBER_LITERAL: &'static str = "E2004";
    pub const LEXER_UNTERMINATED_COMMENT: &'static str = "E2005";
    pub const LEXER_INVALID_IDENTIFIER: &'static str = "E2006";

    // 语法错误 (E3xxx)
    pub const PARSER_UNEXPECTED_TOKEN: &'static str = "E3001";
    pub const PARSER_EXPECTED_SEMICOLON: &'static str = "E3002";
    pub const PARSER_EXPECTED_BRACE: &'static str = "E3003";
    pub const PARSER_EXPECTED_PAREN: &'static str = "E3004";
    pub const PARSER_EXPECTED_IDENTIFIER: &'static str = "E3005";
    pub const PARSER_EXPECTED_TYPE: &'static str = "E3006";
    pub const PARSER_INVALID_STATEMENT: &'static str = "E3007";
    pub const PARSER_INVALID_EXPRESSION: &'static str = "E3008";
    pub const PARSER_MISSING_MAIN: &'static str = "E3009";
    pub const PARSER_MULTIPLE_MAIN: &'static str = "E3010";

    // 语义错误 (E4xxx)
    pub const SEMANTIC_UNDEFINED_IDENTIFIER: &'static str = "E4001";
    pub const SEMANTIC_DUPLICATE_DEFINITION: &'static str = "E4002";
    pub const SEMANTIC_TYPE_MISMATCH: &'static str = "E4003";
    pub const SEMANTIC_INVALID_CAST: &'static str = "E4004";
    pub const SEMANTIC_INCOMPATIBLE_TYPES: &'static str = "E4005";
    pub const SEMANTIC_UNINITIALIZED_VARIABLE: &'static str = "E4006";
    pub const SEMANTIC_INVALID_OPERATION: &'static str = "E4007";
    pub const SEMANTIC_ACCESS_VIOLATION: &'static str = "E4008";
    pub const SEMANTIC_STATIC_CONTEXT: &'static str = "E4009";
    pub const SEMANTIC_FINAL_REASSIGNMENT: &'static str = "E4010";
    pub const SEMANTIC_MISSING_RETURN: &'static str = "E4011";
    pub const SEMANTIC_RETURN_TYPE_MISMATCH: &'static str = "E4012";
    pub const SEMANTIC_BREAK_OUTSIDE_LOOP: &'static str = "E4013";
    pub const SEMANTIC_CONTINUE_OUTSIDE_LOOP: &'static str = "E4014";
    pub const SEMANTIC_INVALID_ARRAY_SIZE: &'static str = "E4015";
    pub const SEMANTIC_ARRAY_INDEX_TYPE: &'static str = "E4016";
    pub const SEMANTIC_METHOD_NOT_FOUND: &'static str = "E4017";
    pub const SEMANTIC_WRONG_ARGUMENT_COUNT: &'static str = "E4018";
    pub const SEMANTIC_ARGUMENT_TYPE_MISMATCH: &'static str = "E4019";
    pub const SEMANTIC_ABSTRACT_CLASS_INSTANCE: &'static str = "E4020";
    pub const SEMANTIC_OVERRIDE_ERROR: &'static str = "E4021";
    pub const SEMANTIC_INHERITANCE_ERROR: &'static str = "E4022";
    pub const SEMANTIC_CIRCULAR_INHERITANCE: &'static str = "E4023";
    pub const SEMANTIC_FINAL_CLASS_INHERITANCE: &'static str = "E4024";
    pub const SEMANTIC_INTERFACE_IMPL_ERROR: &'static str = "E4025";
    pub const SEMANTIC_VOID_ASSIGNMENT: &'static str = "E4026";
    pub const SEMANTIC_DIVISION_BY_ZERO: &'static str = "E4027";
    pub const SEMANTIC_UNREACHABLE_CODE: &'static str = "E4028";
    pub const SEMANTIC_UNUSED_VARIABLE: &'static str = "E4029";

    // 代码生成错误 (E5xxx)
    pub const CODEGEN_UNSUPPORTED_FEATURE: &'static str = "E5001";
    pub const CODEGEN_TYPE_CONVERSION_ERROR: &'static str = "E5002";
    pub const CODEGEN_SYMBOL_NOT_FOUND: &'static str = "E5003";
    pub const CODEGEN_INVALID_OPERATION: &'static str = "E5004";
    pub const CODEGEN_LLVM_ERROR: &'static str = "E5005";

    // 链接错误 (E6xxx)
    pub const LINKER_SYMBOL_NOT_FOUND: &'static str = "E6001";
    pub const LINKER_MULTIPLE_DEFINITION: &'static str = "E6002";
    pub const LINKER_LIBRARY_NOT_FOUND: &'static str = "E6003";

    /// 获取错误代码的详细说明
    pub fn get_description(code: &str) -> &'static str {
        match code {
            // 预处理器
            Self::PREPROCESSOR_DEFINE_ERROR => "宏定义错误",
            Self::PREPROCESSOR_IFDEF_ERROR => "条件编译指令错误",
            Self::PREPROCESSOR_INCLUDE_ERROR => "文件包含错误",
            Self::PREPROCESSOR_UNCLOSED_DIRECTIVE => "未闭合的预处理器指令",
            Self::PREPROCESSOR_CIRCULAR_INCLUDE => "循环包含错误",
            Self::PREPROCESSOR_INVALID_MACRO => "无效的宏定义",

            // 词法
            Self::LEXER_INVALID_CHARACTER => "非法字符",
            Self::LEXER_UNTERMINATED_STRING => "未闭合的字符串",
            Self::LEXER_INVALID_ESCAPE_SEQUENCE => "无效的转义序列",
            Self::LEXER_INVALID_NUMBER_LITERAL => "无效的数字字面量",
            Self::LEXER_UNTERMINATED_COMMENT => "未闭合的注释",
            Self::LEXER_INVALID_IDENTIFIER => "无效的标识符",

            // 语法
            Self::PARSER_UNEXPECTED_TOKEN => "意外的标记",
            Self::PARSER_EXPECTED_SEMICOLON => "缺少分号",
            Self::PARSER_EXPECTED_BRACE => "缺少大括号",
            Self::PARSER_EXPECTED_PAREN => "缺少括号",
            Self::PARSER_EXPECTED_IDENTIFIER => "缺少标识符",
            Self::PARSER_EXPECTED_TYPE => "缺少类型",
            Self::PARSER_INVALID_STATEMENT => "无效的语句",
            Self::PARSER_INVALID_EXPRESSION => "无效的表达式",
            Self::PARSER_MISSING_MAIN => "缺少主函数",
            Self::PARSER_MULTIPLE_MAIN => "多个主函数",

            // 语义
            Self::SEMANTIC_UNDEFINED_IDENTIFIER => "未定义的标识符",
            Self::SEMANTIC_DUPLICATE_DEFINITION => "重复定义",
            Self::SEMANTIC_TYPE_MISMATCH => "类型不匹配",
            Self::SEMANTIC_INVALID_CAST => "无效的类型转换",
            Self::SEMANTIC_INCOMPATIBLE_TYPES => "不兼容的类型",
            Self::SEMANTIC_UNINITIALIZED_VARIABLE => "未初始化的变量",
            Self::SEMANTIC_INVALID_OPERATION => "无效的操作",
            Self::SEMANTIC_ACCESS_VIOLATION => "访问权限错误",
            Self::SEMANTIC_STATIC_CONTEXT => "静态上下文错误",
            Self::SEMANTIC_FINAL_REASSIGNMENT => "final变量重新赋值",
            Self::SEMANTIC_MISSING_RETURN => "缺少返回值",
            Self::SEMANTIC_RETURN_TYPE_MISMATCH => "返回值类型不匹配",
            Self::SEMANTIC_BREAK_OUTSIDE_LOOP => "break在循环外",
            Self::SEMANTIC_CONTINUE_OUTSIDE_LOOP => "continue在循环外",
            Self::SEMANTIC_INVALID_ARRAY_SIZE => "无效的数组大小",
            Self::SEMANTIC_ARRAY_INDEX_TYPE => "数组索引类型错误",
            Self::SEMANTIC_METHOD_NOT_FOUND => "方法未找到",
            Self::SEMANTIC_WRONG_ARGUMENT_COUNT => "参数数量错误",
            Self::SEMANTIC_ARGUMENT_TYPE_MISMATCH => "参数类型不匹配",
            Self::SEMANTIC_ABSTRACT_CLASS_INSTANCE => "抽象类实例化",
            Self::SEMANTIC_OVERRIDE_ERROR => "重写错误",
            Self::SEMANTIC_INHERITANCE_ERROR => "继承错误",
            Self::SEMANTIC_CIRCULAR_INHERITANCE => "循环继承",
            Self::SEMANTIC_FINAL_CLASS_INHERITANCE => "final类继承错误",
            Self::SEMANTIC_INTERFACE_IMPL_ERROR => "接口实现错误",
            Self::SEMANTIC_VOID_ASSIGNMENT => "void赋值错误",
            Self::SEMANTIC_DIVISION_BY_ZERO => "除零错误",
            Self::SEMANTIC_UNREACHABLE_CODE => "不可达代码",
            Self::SEMANTIC_UNUSED_VARIABLE => "未使用的变量",

            // 代码生成
            Self::CODEGEN_UNSUPPORTED_FEATURE => "不支持的功能",
            Self::CODEGEN_TYPE_CONVERSION_ERROR => "类型转换错误",
            Self::CODEGEN_SYMBOL_NOT_FOUND => "符号未找到",
            Self::CODEGEN_INVALID_OPERATION => "无效的操作",
            Self::CODEGEN_LLVM_ERROR => "LLVM错误",

            // 链接
            Self::LINKER_SYMBOL_NOT_FOUND => "链接符号未找到",
            Self::LINKER_MULTIPLE_DEFINITION => "重复定义",
            Self::LINKER_LIBRARY_NOT_FOUND => "库未找到",

            _ => "未知错误",
        }
    }

    /// 获取错误代码的修复建议
    pub fn get_suggestion(code: &str) -> &'static str {
        match code {
            Self::LEXER_INVALID_CHARACTER => "请删除非法字符或使用支持的字符",
            Self::LEXER_UNTERMINATED_STRING => "请在字符串末尾添加双引号",
            Self::LEXER_INVALID_ESCAPE_SEQUENCE => "请使用有效的转义序列: \\n, \\t, \\\", \\\\",
            Self::PARSER_EXPECTED_SEMICOLON => "请在语句末尾添加分号 ';'",
            Self::PARSER_EXPECTED_BRACE => "请添加大括号 '{' 或 '}'",
            Self::PARSER_EXPECTED_PAREN => "请添加括号 '(' 或 ')'",
            Self::SEMANTIC_UNDEFINED_IDENTIFIER => "请检查拼写或声明该标识符",
            Self::SEMANTIC_TYPE_MISMATCH => "请确保类型兼容或进行显式转换",
            Self::SEMANTIC_BREAK_OUTSIDE_LOOP => "break只能在循环或switch中使用",
            Self::SEMANTIC_CONTINUE_OUTSIDE_LOOP => "continue只能在循环中使用",
            _ => "请检查代码并修复错误",
        }
    }
}

/// 格式化诊断信息为字符串
pub fn format_diagnostic(diagnostic: &Diagnostic, source: &str, filename: &str) -> String {
    let mut output = String::new();

    // 标题
    output.push_str(&format!(
        "\n[{}] {} ({})",
        diagnostic.severity,
        diagnostic.code,
        ErrorCodes::get_description(&diagnostic.code)
    ));
    output.push_str(&format!("\n文件: {}", filename));
    output.push_str(&format!(
        "\n位置: 第 {} 行, 第 {} 列",
        diagnostic.location.line, diagnostic.location.column
    ));

    // 源代码上下文
    if diagnostic.location.line > 0 {
        output.push_str("\n\n源代码上下文:");
        let lines: Vec<&str> = source.lines().collect();
        let start = diagnostic.location.line.saturating_sub(3).max(1);
        let end = (diagnostic.location.line + 1).min(lines.len());

        for i in start..=end {
            if i <= lines.len() {
                output.push_str(&format!("\n{:4} | {}", i, lines[i - 1]));
                if i == diagnostic.location.line {
                    let spaces = " ".repeat(diagnostic.location.column.saturating_sub(1) + 6);
                    output.push_str(&format!("\n{}^ {}", spaces, diagnostic.message));
                }
            }
        }
    }

    // 详细说明
    if let Some(details) = &diagnostic.details {
        output.push_str(&format!("\n\n详细说明: {}", details));
    }

    // 修复建议
    if !diagnostic.suggestions.is_empty() {
        output.push_str("\n\n修复建议:");
        for (i, suggestion) in diagnostic.suggestions.iter().enumerate() {
            output.push_str(&format!("\n  {}. {}", i + 1, suggestion.description));
            if let Some(replacement) = &suggestion.replacement {
                output.push_str(&format!("\n     建议代码: {}", replacement));
            }
        }
    }

    // 相关信息
    if !diagnostic.related_info.is_empty() {
        output.push_str("\n\n相关信息:");
        for info in &diagnostic.related_info {
            output.push_str(&format!(
                "\n  第 {} 行: {}",
                info.location.line, info.message
            ));
        }
    }

    output.push('\n');
    output
}

/// 格式化所有诊断信息
pub fn format_all_diagnostics(
    collector: &DiagnosticCollector,
    source: &str,
    filename: &str,
) -> String {
    let mut output = String::new();

    for diagnostic in collector.diagnostics() {
        output.push_str(&format_diagnostic(diagnostic, source, filename));
    }

    // 统计信息
    if collector.error_count() > 0 || collector.warning_count() > 0 {
        output.push_str(&format!(
            "\n编译结果: {} 个错误, {} 个警告\n",
            collector.error_count(),
            collector.warning_count()
        ));
    }

    output
}

// ============================================================
// 统一诊断输出函数（基于 miette 的漂亮终端展示）
// ============================================================

use miette::{GraphicalReportHandler, LabeledSpan, NamedSource, Report, SourceSpan as MietteSpan};

/// 用于 miette 展示的临时诊断包装结构体
/// 结合了 Diagnostic 数据和源代码，以正确计算字节偏移量
#[derive(Debug)]
struct DisplayDiagnostic {
    code: String,
    severity: miette::Severity,
    message: String,
    help: Option<String>,
    source: String,
    filename: String,
    line: usize,
    column: usize,
    /// 错误阶段描述（用于标签文本）
    phase_label: String,
    /// 错误相关的标识符名称（用于计算高亮宽度）
    token_name: Option<String>,
}

impl std::fmt::Display for DisplayDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for DisplayDiagnostic {}

impl miette::Diagnostic for DisplayDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        // 错误代码已包含在 Display 消息中 [E4003]，不重复显示
        None
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(self.severity)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn std::fmt::Display + 'a>> {
        self.help
            .as_ref()
            .map(|h| Box::new(h.as_str()) as Box<dyn std::fmt::Display>)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = LabeledSpan> + '_>> {
        if self.line == 0 {
            return None;
        }
        let offset = line_col_to_offset(&self.source, self.line, self.column);

        // 防御性检查：当 line_col_to_offset 找不到对应位置时会返回 source.len()，
        // 此时 offset 超出或等于源码长度，miette 渲染 span 会导致 OutOfBounds，
        // 直接返回 None 避免崩溃。
        if offset >= self.source.len() {
            return None;
        }

        let source_len = self.source.len();
        // 计算高亮宽度：如果有 token 名，用其长度；否则取到行尾或下一个空白
        let span_len = if let Some(ref name) = self.token_name {
            name.len().max(1)
        } else {
            // 取当前位置到下一个空白/行尾的长度
            let rest = &self.source[offset..];
            rest.chars()
                .take_while(|c| !c.is_whitespace())
                .count()
                .max(1)
        };

        // 防止 span_len 超出源码边界（比如 token 名较长但源码已结束）
        let span_len = span_len.min(source_len - offset).max(1);

        let label = LabeledSpan::new_with_span(
            Some(self.phase_label.clone()),
            MietteSpan::new(offset.into(), span_len),
        );
        Some(Box::new(std::iter::once(label)))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        None
    }
}

/// 获取错误阶段的简短中文描述
fn phase_short_label(phase: &CompilationPhase) -> String {
    match phase {
        CompilationPhase::Lexer => "词法错误".to_string(),
        CompilationPhase::Parser => "语法错误".to_string(),
        CompilationPhase::Semantic => "类型错误".to_string(),
        CompilationPhase::CodeGen => "代码生成错误".to_string(),
        CompilationPhase::Preprocessor => "预处理错误".to_string(),
        CompilationPhase::Linker => "链接错误".to_string(),
    }
}

/// 尝试从错误消息中提取相关的标识符/标记名
fn extract_token_from_message(msg: &str) -> Option<String> {
    // 提取单引号中的名称: 'foo', 'MyClass' 等
    if let Some(start) = msg.find('\'') {
        let after_quote = &msg[start + 1..];
        if let Some(end) = after_quote.find('\'') {
            let name = &after_quote[..end];
            if !name.is_empty() && name.len() < 50 {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// 使用 miette 打印所有诊断信息到 stderr
pub fn print_diagnostics(collector: &DiagnosticCollector, source: &str, filename: &str) {
    if collector.diagnostics().is_empty() {
        return;
    }

    let src = NamedSource::new(filename, source.to_string());
    let diagnostics = collector.diagnostics();

    let error_count = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error || d.severity == Severity::Fatal)
        .count();
    let warning_count = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();

    eprintln!();

    for diag in diagnostics {
        // 检测到行号为0或行号超出范围时输出详细调试信息并保存到文件
        let line_count = source.lines().count();
        if diag.location.line == 0 || diag.location.line > line_count {
            use std::io::Write;
            use std::time::SystemTime;

            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let debug_filename = format!("debug_{}.txt", timestamp);

            let line_count = source.lines().count();
            let invalid_reason = if diag.location.line == 0 {
                "行号为0"
            } else if diag.location.line > line_count {
                &format!("行号超出范围(文件共{}行)", line_count)
            } else {
                "未知原因"
            };

            let debug_content = format!(
                r#"Cavvy Bug Report
================
版本: {}
错误代码: {}
错误消息: {}
编译阶段: {:?}
文件名: {}
行号: {} ({})
列号: {}
源码长度: {} 字节
文件行数: {}

=== 源代码 ===
{}"#,
                env!("CARGO_PKG_VERSION"),
                diag.code,
                diag.message,
                diag.phase,
                filename,
                diag.location.line,
                invalid_reason,
                diag.location.column,
                source.len(),
                line_count,
                source
            );

            if let Ok(mut file) = std::fs::File::create(&debug_filename) {
                let _ = file.write_all(debug_content.as_bytes());
            }

            eprintln!(
                "\n  [!] 检测到Cavvy报错系统出现严重问题，请立刻向 https://github.com/cavvy-lang/cavvy/issues 提出Bug报告，以下是版本信息："
            );
            eprintln!("      Cavvy v{} ", env!("CARGO_PKG_VERSION"));
            eprintln!(
                "      报错文件的源代码、Token解析、Parser解析已保存：{}\n",
                debug_filename
            );
        }

        let severity = match diag.severity {
            Severity::Error | Severity::Fatal => miette::Severity::Error,
            Severity::Warning => miette::Severity::Warning,
            _ => miette::Severity::Advice,
        };

        let display = DisplayDiagnostic {
            code: diag.code.clone(),
            severity,
            message: diag.message.clone(),
            help: diag.suggestions.first().map(|s| s.description.clone()),
            source: source.to_string(),
            filename: filename.to_string(),
            line: diag.location.line,
            column: diag.location.column,
            phase_label: phase_short_label(&diag.phase),
            token_name: extract_token_from_message(&diag.message),
        };

        let report = Report::new(display).with_source_code(src.clone());
        let mut handler = GraphicalReportHandler::new();
        let mut output = String::new();
        handler.render_report(&mut output, report.as_ref()).unwrap();
        eprintln!("{}", output);
    }

    // 统计
    let summary = match (error_count, warning_count) {
        (e, 0) if e > 0 => format!("{} 个错误", e),
        (0, w) if w > 0 => format!("{} 个警告", w),
        (e, w) => format!("{} 个错误, {} 个警告", e, w),
    };
    eprintln!("  编译结果: {}\n", summary);
}

/// 将行号列号转换为字节偏移量（用于 miette SourceSpan）
fn line_col_to_offset(source: &str, line: usize, column: usize) -> usize {
    let mut current_line = 1;
    let mut current_col = 1;

    for (offset, ch) in source.char_indices() {
        if current_line == line && current_col == column {
            return offset;
        }
        if ch == '\n' {
            current_line += 1;
            current_col = 1;
        } else {
            current_col += 1;
        }
    }
    source.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_collector() {
        let mut collector = DiagnosticCollector::new();

        let diag = Diagnostic::error(
            ErrorCodes::SEMANTIC_TYPE_MISMATCH,
            CompilationPhase::Semantic,
            "类型不匹配",
            SourceLocation::new(None, 10, 5),
        );

        collector.add(diag);
        assert!(collector.has_errors());
        assert_eq!(collector.error_count(), 1);
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(ErrorCodes::get_description("E4001"), "未定义的标识符");
        assert_eq!(ErrorCodes::get_description("E9999"), "未知错误");
    }

    // ============================================================
    // print_diagnostics 输出格式测试
    // ============================================================

    #[test]
    fn test_print_diagnostics_single_error() {
        let mut collector = DiagnosticCollector::new();
        let diag = Diagnostic::error(
            ErrorCodes::SEMANTIC_TYPE_MISMATCH,
            CompilationPhase::Semantic,
            "类型不匹配: 期望 int, 实际 String",
            SourceLocation::new(None, 3, 8),
        )
        .with_suggestion(FixSuggestion::new("请使用 Integer.parseInt() 转换"));
        collector.add(diag);

        let source = "int x = \"hello\";\nint y = 42;\n";

        // print_diagnostics 直接输出到 stderr，验证不崩溃
        print_diagnostics(&collector, source, "test.cay");
        assert!(collector.has_errors());
        assert_eq!(collector.error_count(), 1);
    }

    #[test]
    fn test_print_diagnostics_multiple_errors() {
        let mut collector = DiagnosticCollector::new();
        let loc1 = SourceLocation::new(None, 1, 1);
        let loc2 = SourceLocation::new(None, 2, 1);

        collector.add(Diagnostic::error(
            "E4001",
            CompilationPhase::Semantic,
            "未定义变量 'x'",
            loc1,
        ));
        collector.add(Diagnostic::error(
            "E4002",
            CompilationPhase::Semantic,
            "重复定义 'y'",
            loc2,
        ));

        assert_eq!(collector.error_count(), 2);
        assert_eq!(collector.diagnostics().len(), 2);
    }

    #[test]
    fn test_print_diagnostics_empty_collector() {
        let collector = DiagnosticCollector::new();
        // 空收集器不应该 panic
        print_diagnostics(&collector, "", "empty.cay");
        assert!(!collector.has_errors());
    }

    #[test]
    fn test_print_diagnostics_with_warnings() {
        let mut collector = DiagnosticCollector::new();
        let loc = SourceLocation::new(None, 1, 1);

        collector.add(Diagnostic::warning(
            "W4001",
            CompilationPhase::Semantic,
            "未使用的变量",
            loc,
        ));

        assert!(!collector.has_errors());
        assert_eq!(collector.warning_count(), 1);
    }

    // ============================================================
    // line_col_to_offset 测试
    // ============================================================

    #[test]
    fn test_line_col_to_offset_basic() {
        let source = "abc\ndef\nghi";
        // line 1, col 1 -> 'a' at offset 0
        assert_eq!(line_col_to_offset(source, 1, 1), 0);
        // line 2, col 1 -> 'd' at offset 4
        assert_eq!(line_col_to_offset(source, 2, 1), 4);
        // line 3, col 2 -> 'h' at offset 9
        assert_eq!(line_col_to_offset(source, 3, 2), 9);
    }

    #[test]
    fn test_line_col_to_offset_multibyte() {
        let source = "你好\n世界";
        // line 1, col 1 -> '你' (3 bytes)
        assert_eq!(line_col_to_offset(source, 1, 1), 0);
        // line 2, col 1 -> '世' after newline
        assert_eq!(line_col_to_offset(source, 2, 1), 7); // "你好\n" = 7 bytes
    }

    // ============================================================
    // Diagnostic builder 测试
    // ============================================================

    #[test]
    fn test_diagnostic_builder_chain() {
        let diag = Diagnostic::error(
            "E4001",
            CompilationPhase::Semantic,
            "错误",
            SourceLocation::default(),
        )
        .with_details("详细说明")
        .with_suggestion(FixSuggestion::new("建议1"))
        .with_suggestion(FixSuggestion::new("建议2"))
        .with_span(SourceSpan::single(1, 5));

        assert_eq!(diag.code, "E4001");
        assert_eq!(diag.severity, Severity::Error);
        assert_eq!(diag.details, Some("详细说明".into()));
        assert_eq!(diag.suggestions.len(), 2);
        assert!(diag.span.is_some());
    }

    #[test]
    fn test_diagnostic_related_info() {
        let diag = Diagnostic::error(
            "E4001",
            CompilationPhase::Semantic,
            "主错误",
            SourceLocation::new(None, 5, 1),
        )
        .with_related_info("在这里定义", SourceLocation::new(None, 2, 1))
        .with_related_info("这里使用", SourceLocation::new(None, 5, 1));

        assert_eq!(diag.related_info.len(), 2);
    }

    // ============================================================
    // ErrorCodes 完整性测试
    // ============================================================

    #[test]
    fn test_all_error_codes_have_descriptions() {
        // 验证所有预定义的错误代码都有描述
        let codes = [
            ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
            ErrorCodes::LEXER_INVALID_CHARACTER,
            ErrorCodes::PARSER_UNEXPECTED_TOKEN,
            ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER,
            ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
            ErrorCodes::SEMANTIC_TYPE_MISMATCH,
            ErrorCodes::CODEGEN_UNSUPPORTED_FEATURE,
            ErrorCodes::CODEGEN_LLVM_ERROR,
            ErrorCodes::LINKER_SYMBOL_NOT_FOUND,
        ];
        for code in &codes {
            let desc = ErrorCodes::get_description(code);
            assert!(!desc.is_empty(), "Missing description for {}", code);
            assert_ne!(desc, "未知错误", "Unknown error for {}", code);
        }
    }

    #[test]
    fn test_all_error_codes_have_suggestions() {
        // 验证关键错误代码有修复建议
        let codes_with_suggestions = [
            ErrorCodes::LEXER_INVALID_CHARACTER,
            ErrorCodes::PARSER_EXPECTED_SEMICOLON,
            ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER,
            ErrorCodes::SEMANTIC_TYPE_MISMATCH,
        ];
        for code in &codes_with_suggestions {
            let suggestion = ErrorCodes::get_suggestion(code);
            assert!(!suggestion.is_empty(), "Missing suggestion for {}", code);
        }
    }
}
