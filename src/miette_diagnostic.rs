//! Cavvy 统一诊断系统
//!
//! 基于 miette 的错误报告框架，提供：
//! - 美观、友好的彩色错误输出
//! - 源代码片段高亮
//! - 错误代码和链接
//! - 多错误收集与修复建议
//!
//! 本文件整合了原 error.rs 和 diagnostic.rs 的所有功能。

use miette::{NamedSource, SourceSpan as MietteSpan};
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

// ============================================================
// 原始 error.rs 类型定义
// ============================================================

#[derive(Error, Debug, Clone)]
pub enum cayError {
    #[error("词法错误 [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Lexer {
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
    },

    #[error("语法错误 [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Parser {
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
    },

    #[error("语义错误 [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Semantic {
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
    },

    #[error("{} [{}:{line}:{column}]: {message}", if *is_warning { "代码生成警告" } else { "代码生成错误" }, file.as_deref().unwrap_or("<unknown>"))]
    CodeGen {
        code: String,
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
        is_warning: bool,
    },

    #[error("IO错误 [{}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Io {
        file: Option<String>,
        message: String,
    },

    #[error("LLVM错误: {0}")]
    Llvm(String),

    #[error("类型错误 [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    TypeMismatch {
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        expected: String,
        actual: String,
        suggestion: String,
    },

    #[error("未定义标识符 [{}:{line}:{column}]: '{name}'", file.as_deref().unwrap_or("<unknown>"))]
    UndefinedIdentifier {
        file: Option<String>,
        line: usize,
        column: usize,
        name: String,
        suggestion: String,
    },

    #[error("重复定义 [{}:{line}:{column}]: '{name}'", file.as_deref().unwrap_or("<unknown>"))]
    DuplicateDefinition {
        file: Option<String>,
        line: usize,
        column: usize,
        name: String,
        suggestion: String,
    },

    #[error("预处理器错误 [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Preprocessor {
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
    },

    #[error("发现 {} 个错误", errors.len())]
    MultipleErrors { errors: Vec<cayError> },
}

pub type cayResult<T> = Result<T, cayError>;

// ============================================================
// SourceLocation 定义
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
pub struct SourceLocation {
    pub file: Option<String>,
    pub line: usize,
    pub column: usize,
}

impl SourceLocation {
    pub fn new(file: Option<String>, line: usize, column: usize) -> Self {
        Self { file, line, column }
    }

    pub fn from_token(token: &crate::lexer::TokenWithLocation) -> Self {
        Self {
            file: token.source_file.clone(),
            line: token.source_line.unwrap_or(token.loc.line),
            column: token.loc.column,
        }
    }

    pub fn file_str(&self) -> &str {
        self.file.as_deref().unwrap_or("")
    }
}

impl Default for SourceLocation {
    fn default() -> Self {
        Self {
            file: None,
            line: 0,
            column: 0,
        }
    }
}

impl fmt::Display for SourceLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ref file) = self.file {
            write!(f, "{}:{}:{}", file, self.line, self.column)
        } else {
            write!(f, "{}:{}", self.line, self.column)
        }
    }
}

pub type FullSourceLocation = SourceLocation;

// ============================================================
// 原始 diagnostic.rs 类型定义
// ============================================================

/// 错误严重程度
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Note,
    Warning,
    Error,
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
    Preprocessor,
    Lexer,
    Parser,
    Semantic,
    CodeGen,
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
    pub description: String,
    pub replacement: Option<String>,
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

/// 相关信息
#[derive(Debug, Clone)]
pub struct RelatedInfo {
    pub message: String,
    pub location: SourceLocation,
}

/// 诊断信息（错误、警告、提示）
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub phase: CompilationPhase,
    pub message: String,
    pub details: Option<String>,
    pub location: SourceLocation,
    pub span: Option<SourceSpan>,
    pub suggestions: Vec<FixSuggestion>,
    pub related_info: Vec<RelatedInfo>,
}

impl Diagnostic {
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

    pub fn error(
        code: impl Into<String>,
        phase: CompilationPhase,
        message: impl Into<String>,
        location: SourceLocation,
    ) -> Self {
        Self::new(code, Severity::Error, phase, message, location)
    }

    pub fn warning(
        code: impl Into<String>,
        phase: CompilationPhase,
        message: impl Into<String>,
        location: SourceLocation,
    ) -> Self {
        Self::new(code, Severity::Warning, phase, message, location)
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    pub fn with_span(mut self, span: SourceSpan) -> Self {
        self.span = Some(span);
        self
    }

    pub fn with_suggestion(mut self, suggestion: FixSuggestion) -> Self {
        self.suggestions.push(suggestion);
        self
    }

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
            max_errors: 100,
            error_count: 0,
            warning_count: 0,
        }
    }

    pub fn with_max_errors(mut self, max: usize) -> Self {
        self.max_errors = max;
        self
    }

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

    pub fn has_errors(&self) -> bool {
        self.error_count > 0
    }

    pub fn has_fatal_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Fatal)
    }

    pub fn is_max_errors_reached(&self) -> bool {
        self.error_count >= self.max_errors
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn error_count(&self) -> usize {
        self.error_count
    }

    pub fn warning_count(&self) -> usize {
        self.warning_count
    }

    pub fn clear(&mut self) {
        self.diagnostics.clear();
        self.error_count = 0;
        self.warning_count = 0;
    }

    pub fn merge(&mut self, other: DiagnosticCollector) {
        for diag in other.diagnostics {
            self.add(diag);
        }
    }

    /// 添加 cayError 到收集器
    pub fn add_cay_error(&mut self, error: &cayError) {
        let compiler_err = CompilerError::from(error.clone());
        self.add(compiler_err.0);
    }
}

// ============================================================
// 错误代码定义
// ============================================================

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

    // ===== 警告代码 (Wxxx) =====

    // 预处理器警告 (W1xxx)
    pub const PREPROCESSOR_WARNING: &'static str = "W1001";
    pub const PREPROCESSOR_DEPRECATED: &'static str = "W1002";

    // 词法警告 (W2xxx)
    pub const LEXER_DEPRECATED_SYNTAX: &'static str = "W2001";
    pub const LEXER_PORTABILITY: &'static str = "W2002";

    // 语法警告 (W3xxx)
    pub const PARSER_DEPRECATED_FEATURE: &'static str = "W3001";
    pub const PARSER_EXTENSION: &'static str = "W3002";

    // 语义警告 (W4xxx)
    pub const SEMANTIC_UNUSED_VARIABLE: &'static str = "W4001";
    pub const SEMANTIC_UNREACHABLE_CODE: &'static str = "W4002";
    pub const SEMANTIC_DEPRECATED: &'static str = "W4003";
    pub const SEMANTIC_NON_STANDARD: &'static str = "W4004";

    // 代码生成警告 (W5xxx)
    pub const CODEGEN_SUBOPTIMAL: &'static str = "W5001";
    pub const CODEGEN_DEPRECATED_FEATURE: &'static str = "W5002";

    pub fn get_description(code: &str) -> &'static str {
        match code {
            Self::PREPROCESSOR_DEFINE_ERROR => "宏定义错误",
            Self::PREPROCESSOR_IFDEF_ERROR => "条件编译指令错误",
            Self::PREPROCESSOR_INCLUDE_ERROR => "文件包含错误",
            Self::PREPROCESSOR_UNCLOSED_DIRECTIVE => "未闭合的预处理器指令",
            Self::PREPROCESSOR_CIRCULAR_INCLUDE => "循环包含错误",
            Self::PREPROCESSOR_INVALID_MACRO => "无效的宏定义",

            Self::LEXER_INVALID_CHARACTER => "非法字符",
            Self::LEXER_UNTERMINATED_STRING => "未闭合的字符串",
            Self::LEXER_INVALID_ESCAPE_SEQUENCE => "无效的转义序列",
            Self::LEXER_INVALID_NUMBER_LITERAL => "无效的数字字面量",
            Self::LEXER_UNTERMINATED_COMMENT => "未闭合的注释",
            Self::LEXER_INVALID_IDENTIFIER => "无效的标识符",

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

            Self::CODEGEN_UNSUPPORTED_FEATURE => "不支持的功能",
            Self::CODEGEN_TYPE_CONVERSION_ERROR => "类型转换错误",
            Self::CODEGEN_SYMBOL_NOT_FOUND => "符号未找到",
            Self::CODEGEN_INVALID_OPERATION => "无效的操作",
            Self::CODEGEN_LLVM_ERROR => "LLVM错误",

            Self::LINKER_SYMBOL_NOT_FOUND => "链接符号未找到",
            Self::LINKER_MULTIPLE_DEFINITION => "重复定义",
            Self::LINKER_LIBRARY_NOT_FOUND => "库未找到",

            // 警告代码描述
            Self::PREPROCESSOR_WARNING => "预处理警告",
            Self::PREPROCESSOR_DEPRECATED => "预处理已弃用特性",
            Self::LEXER_DEPRECATED_SYNTAX => "已弃用的语法",
            Self::LEXER_PORTABILITY => "可移植性警告",
            Self::PARSER_DEPRECATED_FEATURE => "已弃用的语言特性",
            Self::PARSER_EXTENSION => "扩展语法",
            Self::SEMANTIC_UNUSED_VARIABLE => "未使用的变量",
            Self::SEMANTIC_UNREACHABLE_CODE => "不可达代码",
            Self::SEMANTIC_DEPRECATED => "已弃用的语义特性",
            Self::SEMANTIC_NON_STANDARD => "非标准用法",
            Self::CODEGEN_SUBOPTIMAL => "代码生成次优",
            Self::CODEGEN_DEPRECATED_FEATURE => "已弃用的代码生成特性",

            _ => "未知错误",
        }
    }

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

// ============================================================
// CompilerError — 包装 Diagnostic 的统一错误类型
// ============================================================

#[derive(Debug, Clone)]
pub struct CompilerError(pub Diagnostic);

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.0.code, self.0.message)
    }
}

impl std::error::Error for CompilerError {}

impl miette::Diagnostic for CompilerError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new(&self.0.code))
    }

    fn severity(&self) -> Option<miette::Severity> {
        match self.0.severity {
            Severity::Error | Severity::Fatal => Some(miette::Severity::Error),
            Severity::Warning => Some(miette::Severity::Warning),
            Severity::Note => Some(miette::Severity::Advice),
        }
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.0
            .suggestions
            .first()
            .map(|s| Box::new(s.description.clone()) as Box<dyn fmt::Display>)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        let diag = &self.0;
        if diag.location.line == 0 {
            return None;
        }
        let offset = line_col_to_offset("", diag.location.line, diag.location.column);
        let len = 1usize;
        let label = diag.message.clone();
        let span = miette::LabeledSpan::new_with_span(
            Some(label),
            miette::SourceSpan::new(offset.into(), len),
        );
        Some(Box::new(std::iter::once(span)))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        None
    }
}

impl From<cayError> for CompilerError {
    fn from(e: cayError) -> Self {
        let diagnostic = match &e {
            cayError::Lexer { file, line, column, message, suggestion } => {
                let code = if message.contains("未闭合") || message.contains("Unterminated") {
                    ErrorCodes::LEXER_UNTERMINATED_STRING
                } else {
                    ErrorCodes::LEXER_INVALID_CHARACTER
                };
                Diagnostic::error(code, CompilationPhase::Lexer, message.clone(),
                    SourceLocation { file: file.clone(), line: *line, column: *column })
                    .with_suggestion(FixSuggestion::new(suggestion.clone()))
            }
            cayError::Parser { file, line, column, message, suggestion } => {
                let code = if message.contains("';'") || message.contains("分号") {
                    ErrorCodes::PARSER_EXPECTED_SEMICOLON
                } else if message.contains("'{'") || message.contains("'}'") || message.contains("大括号") {
                    ErrorCodes::PARSER_EXPECTED_BRACE
                } else if message.contains("'('") || message.contains("')'") || message.contains("括号") {
                    ErrorCodes::PARSER_EXPECTED_PAREN
                } else {
                    ErrorCodes::PARSER_UNEXPECTED_TOKEN
                };
                Diagnostic::error(code, CompilationPhase::Parser, message.clone(),
                    SourceLocation { file: file.clone(), line: *line, column: *column })
                    .with_suggestion(FixSuggestion::new(suggestion.clone()))
            }
            cayError::Semantic { file, line, column, message, suggestion } => {
                let code = if message.contains("Undefined") || message.contains("未定义") || message.contains("not found") {
                    ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER
                } else if message.contains("Duplicate") || message.contains("重复") {
                    ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION
                } else if message.contains("type") || message.contains("类型") || message.contains("assign") || message.contains("Cannot") {
                    ErrorCodes::SEMANTIC_TYPE_MISMATCH
                } else {
                    ErrorCodes::SEMANTIC_INVALID_OPERATION
                };
                Diagnostic::error(code, CompilationPhase::Semantic, message.clone(),
                    SourceLocation { file: file.clone(), line: *line, column: *column })
                    .with_suggestion(FixSuggestion::new(suggestion.clone()))
            }
            cayError::CodeGen { code, file, line, column, message, suggestion, is_warning } => {
                let display_line = if *line == 0 { 1 } else { *line };
                let display_column = if *column == 0 { 1 } else { *column };
                let severity = if *is_warning { Severity::Warning } else { Severity::Error };
                Diagnostic::new(code.clone(), severity, CompilationPhase::CodeGen, message.clone(),
                    SourceLocation { file: file.clone(), line: display_line, column: display_column })
                    .with_suggestion(FixSuggestion::new(suggestion.clone()))
            }
            cayError::Io { file, message } => Diagnostic::new(
                "I0001".to_string(), Severity::Error, CompilationPhase::Linker, message.clone(),
                SourceLocation { file: file.clone(), line: 1, column: 1 }
            ),
            cayError::Llvm(msg) => Diagnostic::error(
                ErrorCodes::CODEGEN_LLVM_ERROR, CompilationPhase::CodeGen, msg.clone(),
                SourceLocation::default()
            ),
            cayError::TypeMismatch { file, line, column, message, expected, actual, suggestion } => {
                Diagnostic::error(ErrorCodes::SEMANTIC_TYPE_MISMATCH, CompilationPhase::Semantic,
                    format!("{}: 期望 '{}', 实际 '{}'", message, expected, actual),
                    SourceLocation { file: file.clone(), line: *line, column: *column })
                    .with_suggestion(FixSuggestion::new(suggestion.clone()))
            }
            cayError::UndefinedIdentifier { file, line, column, name, suggestion } => {
                Diagnostic::error(ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER, CompilationPhase::Semantic,
                    format!("未定义的标识符: '{}'", name),
                    SourceLocation { file: file.clone(), line: *line, column: *column })
                    .with_suggestion(FixSuggestion::new(suggestion.clone()))
            }
            cayError::DuplicateDefinition { file, line, column, name, suggestion } => {
                Diagnostic::error(ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION, CompilationPhase::Semantic,
                    format!("重复定义: '{}'", name),
                    SourceLocation { file: file.clone(), line: *line, column: *column })
                    .with_suggestion(FixSuggestion::new(suggestion.clone()))
            }
            cayError::Preprocessor { file, line, column, message, suggestion } => {
                Diagnostic::error(ErrorCodes::PREPROCESSOR_DEFINE_ERROR, CompilationPhase::Preprocessor,
                    message.clone(),
                    SourceLocation { file: file.clone(), line: *line, column: *column })
                    .with_suggestion(FixSuggestion::new(suggestion.clone()))
            }
            cayError::MultipleErrors { errors } => {
                if let Some(first) = errors.first() {
                    return CompilerError::from(first.clone());
                }
                Diagnostic::error("E9999", CompilationPhase::Semantic,
                    format!("发现 {} 个错误", errors.len()),
                    SourceLocation::default())
            }
        };
        CompilerError(diagnostic)
    }
}

impl From<CompilerError> for cayError {
    fn from(e: CompilerError) -> Self {
        let d = &e.0;
        let message = d.message.clone();
        let suggestion = d.suggestions.first().map(|s| s.description.clone()).unwrap_or_else(|| "请检查代码".to_string());
        let file = d.location.file.clone();

        match d.phase {
            CompilationPhase::Lexer => cayError::Lexer { file, line: d.location.line, column: d.location.column, message, suggestion },
            CompilationPhase::Parser => cayError::Parser { file, line: d.location.line, column: d.location.column, message, suggestion },
            CompilationPhase::Semantic => cayError::Semantic { file, line: d.location.line, column: d.location.column, message, suggestion },
            CompilationPhase::Preprocessor => cayError::Preprocessor { file, line: d.location.line, column: d.location.column, message, suggestion },
            _ => cayError::CodeGen {
                code: d.code.clone(),
                file, line: d.location.line, column: d.location.column,
                message, suggestion,
                is_warning: d.severity == Severity::Warning,
            },
        }
    }
}

pub type CompilerResult<T> = Result<T, CompilerError>;

// ============================================================
// 便捷构造函数（CompilerError 级别）
// ============================================================

pub fn error(
    code: &str,
    phase: CompilationPhase,
    message: impl Into<String>,
    location: SourceLocation,
) -> CompilerError {
    CompilerError(Diagnostic::error(code, phase, message, location))
}

pub fn warning(
    code: &str,
    phase: CompilationPhase,
    message: impl Into<String>,
    location: SourceLocation,
) -> CompilerError {
    CompilerError(Diagnostic::warning(code, phase, message, location))
}

pub fn error_with_suggestion(
    code: &str,
    phase: CompilationPhase,
    message: impl Into<String>,
    location: SourceLocation,
    suggestion: impl Into<String>,
) -> CompilerError {
    CompilerError(
        Diagnostic::error(code, phase, message, location)
            .with_suggestion(FixSuggestion::new(suggestion)),
    )
}

pub fn warning_with_suggestion(
    code: &str,
    phase: CompilationPhase,
    message: impl Into<String>,
    location: SourceLocation,
    suggestion: impl Into<String>,
) -> CompilerError {
    CompilerError(
        Diagnostic::warning(code, phase, message, location)
            .with_suggestion(FixSuggestion::new(suggestion)),
    )
}

pub fn error_with_details(
    code: &str,
    phase: CompilationPhase,
    message: impl Into<String>,
    location: SourceLocation,
    details: impl Into<String>,
) -> CompilerError {
    CompilerError(
        Diagnostic::error(code, phase, message, location)
            .with_details(details),
    )
}

pub fn warning_with_details(
    code: &str,
    phase: CompilationPhase,
    message: impl Into<String>,
    location: SourceLocation,
    details: impl Into<String>,
) -> CompilerError {
    CompilerError(
        Diagnostic::warning(code, phase, message, location)
            .with_details(details),
    )
}

// ============================================================
// 旧错误构造函数（返回 cayError，保持向后兼容）
// ============================================================

pub fn lexer_error(line: usize, column: usize, message: impl Into<String>) -> cayError {
    lexer_error_with_file(None, line, column, message)
}

pub fn lexer_error_with_file(
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> cayError {
    let msg = message.into();
    let code = if msg.contains("未闭合") {
        ErrorCodes::LEXER_UNTERMINATED_STRING
    } else {
        ErrorCodes::LEXER_INVALID_CHARACTER
    };
    CompilerError(Diagnostic::error(code, CompilationPhase::Lexer, msg,
        SourceLocation::new(file, line, column)))
    .into()
}

pub fn parser_error(line: usize, column: usize, message: impl Into<String>) -> cayError {
    parser_error_with_file(None, line, column, message)
}

pub fn parser_error_with_file(
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> cayError {
    let msg = message.into();
    let code = if msg.contains("';'") {
        ErrorCodes::PARSER_EXPECTED_SEMICOLON
    } else if msg.contains("'{'") || msg.contains("'}'") {
        ErrorCodes::PARSER_EXPECTED_BRACE
    } else if msg.contains("'('") || msg.contains("')'") {
        ErrorCodes::PARSER_EXPECTED_PAREN
    } else {
        ErrorCodes::PARSER_UNEXPECTED_TOKEN
    };
    CompilerError(Diagnostic::error(code, CompilationPhase::Parser, msg,
        SourceLocation::new(file, line, column)))
    .into()
}

pub fn semantic_error(line: usize, column: usize, message: impl Into<String>) -> cayError {
    semantic_error_with_file(None, line, column, message)
}

pub fn semantic_error_with_file(
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> cayError {
    let msg = message.into();
    let code = if msg.contains("Undefined") || msg.contains("未定义") {
        ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER
    } else if msg.contains("Duplicate") || msg.contains("重复") {
        ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION
    } else if msg.contains("type") || msg.contains("类型") {
        ErrorCodes::SEMANTIC_TYPE_MISMATCH
    } else {
        ErrorCodes::SEMANTIC_INVALID_OPERATION
    };
    CompilerError(Diagnostic::error(code, CompilationPhase::Semantic, msg,
        SourceLocation::new(file, line, column)))
    .into()
}

pub fn codegen_error_at(loc: SourceLocation, message: impl Into<String>) -> cayError {
    let msg = message.into();
    let code = if msg.contains("Unsupported") {
        ErrorCodes::CODEGEN_UNSUPPORTED_FEATURE
    } else if msg.contains("not found") {
        ErrorCodes::CODEGEN_SYMBOL_NOT_FOUND
    } else {
        ErrorCodes::CODEGEN_INVALID_OPERATION
    };
    CompilerError(Diagnostic::error(code, CompilationPhase::CodeGen, msg, loc))
    .into()
}

pub fn codegen_warning_at(loc: SourceLocation, message: impl Into<String>) -> cayError {
    let msg = message.into();
    CompilerError(Diagnostic::warning(
        ErrorCodes::CODEGEN_INVALID_OPERATION,
        CompilationPhase::CodeGen, msg, loc))
    .into()
}

pub fn type_mismatch_error(
    line: usize,
    column: usize,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> cayError {
    type_mismatch_error_with_file(None, line, column, expected, actual)
}

pub fn type_mismatch_error_with_file(
    file: Option<String>,
    line: usize,
    column: usize,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> cayError {
    let expected_str = expected.into();
    let actual_str = actual.into();
    CompilerError(
        Diagnostic::error(ErrorCodes::SEMANTIC_TYPE_MISMATCH, CompilationPhase::Semantic,
            format!("类型不匹配: 期望 '{}', 实际 '{}'", expected_str, actual_str),
            SourceLocation::new(file, line, column))
            .with_suggestion(FixSuggestion::new(format!(
                "请确保表达式返回 '{}' 类型的值", expected_str
            ))),
    ).into()
}

pub fn undefined_identifier_error(line: usize, column: usize, name: impl Into<String>) -> cayError {
    undefined_identifier_error_with_file(None, line, column, name)
}

pub fn undefined_identifier_error_with_file(
    file: Option<String>,
    line: usize,
    column: usize,
    name: impl Into<String>,
) -> cayError {
    let name_str = name.into();
    CompilerError(
        Diagnostic::error(ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER, CompilationPhase::Semantic,
            format!("未定义的标识符: '{}'", name_str),
            SourceLocation::new(file, line, column))
            .with_suggestion(FixSuggestion::new(format!(
                "请检查 '{}' 的拼写，或在使用前声明该变量/函数", name_str
            ))),
    ).into()
}

pub fn duplicate_definition_error(line: usize, column: usize, name: impl Into<String>) -> cayError {
    duplicate_definition_error_with_file(None, line, column, name)
}

pub fn duplicate_definition_error_with_file(
    file: Option<String>,
    line: usize,
    column: usize,
    name: impl Into<String>,
) -> cayError {
    let name_str = name.into();
    CompilerError(
        Diagnostic::error(ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION, CompilationPhase::Semantic,
            format!("重复定义: '{}'", name_str),
            SourceLocation::new(file, line, column))
            .with_suggestion(FixSuggestion::new(format!(
                "'{}' 已被定义，请使用不同的名称", name_str
            ))),
    ).into()
}

// ============================================================
// 错误信息查询函数
// ============================================================

pub fn get_error_message(error: &cayError) -> String {
    match error {
        cayError::Lexer { message, .. } => message.clone(),
        cayError::Parser { message, .. } => message.clone(),
        cayError::Semantic { message, .. } => message.clone(),
        cayError::TypeMismatch { message, .. } => message.clone(),
        cayError::UndefinedIdentifier { name, .. } => format!("未定义的标识符 '{}'", name),
        cayError::DuplicateDefinition { name, .. } => format!("重复定义 '{}'", name),
        cayError::CodeGen { message, .. } => message.clone(),
        cayError::Io { message, .. } => message.clone(),
        cayError::Llvm(msg) => msg.clone(),
        cayError::Preprocessor { message, .. } => message.clone(),
        cayError::MultipleErrors { errors } => format!("发现 {} 个错误", errors.len()),
    }
}

pub fn get_error_help(error: &cayError) -> Option<String> {
    match error {
        cayError::Lexer { suggestion, .. } => Some(suggestion.clone()),
        cayError::Parser { suggestion, .. } => Some(suggestion.clone()),
        cayError::Semantic { suggestion, .. } => Some(suggestion.clone()),
        cayError::TypeMismatch { suggestion, .. } => Some(suggestion.clone()),
        cayError::UndefinedIdentifier { suggestion, .. } => Some(suggestion.clone()),
        cayError::DuplicateDefinition { suggestion, .. } => Some(suggestion.clone()),
        cayError::CodeGen { suggestion, .. } => Some(suggestion.clone()),
        cayError::Io { .. } => None,
        cayError::Llvm(_) => None,
        cayError::Preprocessor { suggestion, .. } => Some(suggestion.clone()),
        cayError::MultipleErrors { .. } => Some("请逐个修复上述错误".to_string()),
    }
}

pub fn get_error_location(error: &cayError) -> Option<(usize, usize)> {
    match error {
        cayError::Lexer { line, column, .. } => Some((*line, *column)),
        cayError::Parser { line, column, .. } => Some((*line, *column)),
        cayError::Semantic { line, column, .. } => Some((*line, *column)),
        cayError::TypeMismatch { line, column, .. } => Some((*line, *column)),
        cayError::UndefinedIdentifier { line, column, .. } => Some((*line, *column)),
        cayError::DuplicateDefinition { line, column, .. } => Some((*line, *column)),
        cayError::Preprocessor { line, column, .. } => Some((*line, *column)),
        cayError::MultipleErrors { .. } => None,
        _ => None,
    }
}

pub fn get_error_file(error: &cayError) -> Option<String> {
    match error {
        cayError::Lexer { file, .. } => file.clone(),
        cayError::Parser { file, .. } => file.clone(),
        cayError::Semantic { file, .. } => file.clone(),
        cayError::TypeMismatch { file, .. } => file.clone(),
        cayError::UndefinedIdentifier { file, .. } => file.clone(),
        cayError::DuplicateDefinition { file, .. } => file.clone(),
        cayError::Preprocessor { file, .. } => file.clone(),
        cayError::MultipleErrors { .. } => None,
        _ => None,
    }
}

// ============================================================
// 打印函数
// ============================================================

pub fn print_miette_error(error_type: &str, message: &str, help: Option<&str>) {
    eprintln!("\n  × {}: {}", error_type, message);
    if let Some(help_text) = help {
        if !help_text.is_empty() {
            eprintln!("  help: {}", help_text);
        }
    }
    eprintln!();
}

pub fn print_miette_warning(warning_type: &str, message: &str, help: Option<&str>) {
    eprintln!("\n  ⚠ {}: {}", warning_type, message);
    if let Some(help_text) = help {
        if !help_text.is_empty() {
            eprintln!("  help: {}", help_text);
        }
    }
    eprintln!();
}

#[deprecated()]
pub fn print_compile_error(stage: &str, error: &str, source_path: &str, help: Option<&str>) {
    eprintln!("\n  × cavvy::compile_error: {}阶段错误", stage);
    eprintln!("   ╭─[{}]", source_path);
    eprintln!("   │");
    eprintln!("   │ {}", error);
    eprintln!("   ╰────");
    if let Some(help_text) = help {
        if !help_text.is_empty() {
            eprintln!("  help: {}", help_text);
        }
    }
    eprintln!();
}

pub fn print_tool_error(tool: &str, message: &str, help: Option<&str>) {
    eprintln!("\n  × cavvy::tool_error: {} 执行失败", tool);
    eprintln!("   │");
    eprintln!("   │ {}", message);
    if let Some(help_text) = help {
        if !help_text.is_empty() {
            eprintln!("   │");
            eprintln!("  help: {}", help_text);
        }
    }
    eprintln!();
}

pub fn print_warning(message: &str) {
    eprintln!("  ⚠ cavvy::warning: {}", message);
}

#[deprecated()]
pub fn print_warning_with_location(message: &str, filename: &str, line: usize, column: usize) {
    eprintln!("  ⚠ cavvy::warning: {}", message);
    eprintln!("     位置: {}:{}:{}", filename, line, column);
}

// ============================================================
// print_error_with_context — 使用 miette 展示错误
// ============================================================

pub fn print_error_with_context(error: &cayError, source: &str, filename: &str) {
    let mut collector = DiagnosticCollector::new();

    match error {
        cayError::MultipleErrors { errors } => {
            for err in errors {
                collector.add(CompilerError::from(err.clone()).0);
            }
        }
        single => {
            collector.add(CompilerError::from(single.clone()).0);
        }
    }

    print_diagnostics_per_file(&collector, source, filename);
}

fn print_diagnostics_per_file(
    collector: &DiagnosticCollector,
    default_source: &str,
    default_filename: &str,
) {
    let diagnostics = collector.diagnostics();
    if diagnostics.is_empty() {
        return;
    }

    let mut by_file: HashMap<String, (String, Vec<&Diagnostic>)> = HashMap::new();
    let mut no_file_diags: Vec<&Diagnostic> = Vec::new();

    for diag in diagnostics {
        if let Some(ref file) = diag.location.file {
            if !file.is_empty() {
                let entry = by_file.entry(file.clone()).or_insert_with(|| {
                    let content = std::fs::read_to_string(file)
                        .unwrap_or_else(|_| default_source.to_string());
                    (content, Vec::new())
                });
                entry.1.push(diag);
                continue;
            }
        }
        no_file_diags.push(diag);
    }

    for (file, (content, diags)) in &by_file {
        let mut sub_collector = DiagnosticCollector::new();
        for d in diags {
            sub_collector.add((*d).clone());
        }
        print_diagnostics(&sub_collector, content, file);
    }

    if !no_file_diags.is_empty() {
        let mut sub_collector = DiagnosticCollector::new();
        for d in no_file_diags {
            sub_collector.add(d.clone());
        }
        print_diagnostics(&sub_collector, default_source, default_filename);
    }
}

// ============================================================
// miette 诊断输出
// ============================================================

/// 用于 miette 展示的临时诊断包装
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
    phase_label: String,
    token_name: Option<String>,
}

impl fmt::Display for DisplayDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for DisplayDiagnostic {}

impl miette::Diagnostic for DisplayDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        None
    }

    fn severity(&self) -> Option<miette::Severity> {
        Some(self.severity)
    }

    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.help
            .as_ref()
            .map(|h| Box::new(h.as_str()) as Box<dyn fmt::Display>)
    }

    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        if self.line == 0 {
            return None;
        }
        let offset = line_col_to_offset(&self.source, self.line, self.column);
        if offset >= self.source.len() {
            return None;
        }

        let source_len = self.source.len();
        let span_len = if let Some(ref name) = self.token_name {
            name.len().max(1)
        } else {
            let rest = &self.source[offset..];
            rest.chars()
                .take_while(|c| !c.is_whitespace())
                .count()
                .max(1)
        };
        let span_len = span_len.min(source_len - offset).max(1);

        let label = miette::LabeledSpan::new_with_span(
            Some(self.phase_label.clone()),
            MietteSpan::new(offset.into(), span_len),
        );
        Some(Box::new(std::iter::once(label)))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        None
    }
}

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

fn extract_token_from_message(msg: &str) -> Option<String> {
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

/// 使用 miette 打印诊断信息到 stderr
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
        let line_count = source.lines().count();
        if diag.location.line == 0 || diag.location.line > line_count {
            use std::io::Write;
            use std::time::SystemTime;

            let timestamp = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let debug_filename = format!("debug_{}.txt", timestamp);
            let invalid_reason = if diag.location.line == 0 {
                "行号为0"
            } else {
                &format!("行号超出范围(文件共{}行)", line_count)
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
                diag.code, diag.message, diag.phase, filename,
                diag.location.line, invalid_reason, diag.location.column,
                source.len(), line_count, source
            );
            if let Ok(mut file) = std::fs::File::create(&debug_filename) {
                let _ = file.write_all(debug_content.as_bytes());
            }
            eprintln!(
                "\n  [!] 检测到Cavvy报错系统出现严重问题，请立刻向 https://github.com/cavvy-lang/cavvy/issues 提出Bug报告，以下是版本信息："
            );
            eprintln!("      Cavvy v{} ", env!("CARGO_PKG_VERSION"));
            eprintln!("      报错文件的源代码、Token解析、Parser解析已保存：{}\n", debug_filename);
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

        let report = miette::Report::new(display).with_source_code(src.clone());
        let mut handler = miette::GraphicalReportHandler::new();
        let mut output = String::new();
        handler.render_report(&mut output, report.as_ref()).unwrap();
        eprintln!("{}", output);
    }

    let summary = match (error_count, warning_count) {
        (e, 0) if e > 0 => format!("{} 个错误", e),
        (0, w) if w > 0 => format!("{} 个警告", w),
        (e, w) => format!("{} 个错误, {} 个警告", e, w),
    };
    eprintln!("  编译结果: {}\n", summary);
}

/// 格式化诊断信息为字符串
pub fn format_diagnostic(diagnostic: &Diagnostic, source: &str, filename: &str) -> String {
    let mut output = String::new();

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

    if let Some(details) = &diagnostic.details {
        output.push_str(&format!("\n\n详细说明: {}", details));
    }

    if !diagnostic.suggestions.is_empty() {
        output.push_str("\n\n修复建议:");
        for (i, suggestion) in diagnostic.suggestions.iter().enumerate() {
            output.push_str(&format!("\n  {}. {}", i + 1, suggestion.description));
            if let Some(replacement) = &suggestion.replacement {
                output.push_str(&format!("\n     建议代码: {}", replacement));
            }
        }
    }

    if !diagnostic.related_info.is_empty() {
        output.push_str("\n\n相关信息:");
        for info in &diagnostic.related_info {
            output.push_str(&format!("\n  第 {} 行: {}", info.location.line, info.message));
        }
    }

    output.push('\n');
    output
}

pub fn format_all_diagnostics(
    collector: &DiagnosticCollector,
    source: &str,
    filename: &str,
) -> String {
    let mut output = String::new();

    for diagnostic in collector.diagnostics() {
        output.push_str(&format_diagnostic(diagnostic, source, filename));
    }

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
// 行/列 → 偏移量 辅助函数
// ============================================================

pub fn line_col_to_offset(source: &str, line: usize, column: usize) -> usize {
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

pub fn line_range(source: &str, line: usize) -> (usize, usize) {
    let mut current_line = 1;

    for (offset, ch) in source.char_indices() {
        if current_line == line {
            for (end_offset, end_ch) in source[offset..].char_indices() {
                if end_ch == '\n' {
                    return (offset, offset + end_offset);
                }
            }
            return (offset, source.len());
        }

        if ch == '\n' {
            current_line += 1;
        }
    }

    (source.len(), source.len())
}

// ============================================================
// miette 基础错误类型（新生代错误系统）
// ============================================================

/// 通用编译错误 — 基于 miette derive 的简洁错误类型
#[derive(Error, Debug, miette::Diagnostic)]
#[error("{message}")]
#[diagnostic()]
pub struct CavvyError {
    message: String,
    #[diagnostic(code)]
    code: String,
    #[source_code]
    src: NamedSource<String>,
    #[label("{label_text}")]
    span: MietteSpan,
    #[diagnostic(transparent)]
    label_text: String,
    #[help]
    help: Option<String>,
}

/// 通用编译警告 — 基于 miette derive 的警告类型
#[derive(Error, Debug, miette::Diagnostic)]
#[error("{message}")]
#[diagnostic(severity(warning))]
pub struct CavvyWarning {
    message: String,
    #[diagnostic(code)]
    code: String,
    #[source_code]
    src: NamedSource<String>,
    #[label("{label_text}")]
    span: MietteSpan,
    #[diagnostic(transparent)]
    label_text: String,
    #[help]
    help: Option<String>,
}

impl CavvyWarning {
    pub fn new(
        message: impl Into<String>,
        code: impl Into<String>,
        source: impl Into<String>,
        source_name: impl AsRef<str>,
        span: (usize, usize),
        label: impl Into<String>,
    ) -> Self {
        let label_text = label.into();
        Self {
            message: message.into(),
            code: code.into(),
            src: NamedSource::new(source_name, source.into()),
            span: span.into(),
            label_text,
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

impl CavvyError {
    pub fn new(
        message: impl Into<String>,
        code: impl Into<String>,
        source: impl Into<String>,
        source_name: impl AsRef<str>,
        span: (usize, usize),
        label: impl Into<String>,
    ) -> Self {
        let label_text = label.into();
        Self {
            message: message.into(),
            code: code.into(),
            src: NamedSource::new(source_name, source.into()),
            span: span.into(),
            label_text,
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// 词法错误 — 基于 miette derive 的细粒度错误类型
#[derive(Error, Debug, miette::Diagnostic)]
pub enum LexerError {
    #[error("非法字符: {ch}")]
    #[diagnostic(code(lexer::invalid_character), help("请删除非法字符或使用支持的字符替换"))]
    InvalidCharacter {
        ch: char,
        #[source_code]
        src: NamedSource<String>,
        #[label("非法字符在这里")]
        span: MietteSpan,
    },

    #[error("未闭合的字符串字面量")]
    #[diagnostic(code(lexer::unterminated_string), help("请在字符串末尾添加双引号"))]
    UnterminatedString {
        #[source_code]
        src: NamedSource<String>,
        #[label("字符串从这里开始")]
        span: MietteSpan,
    },

    #[error("无效的转义序列: {sequence}")]
    #[diagnostic(code(lexer::invalid_escape), help("有效的转义序列: \\n, \\t, \\\", \\\\"))]
    InvalidEscapeSequence {
        sequence: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("无效的转义序列")]
        span: MietteSpan,
    },

    #[error("无效的数字字面量")]
    #[diagnostic(code(lexer::invalid_number), help("支持的格式: 十进制(123), 十六进制(0xFF), 二进制(0b101)"))]
    InvalidNumberLiteral {
        #[source_code]
        src: NamedSource<String>,
        #[label("无效的数字格式")]
        span: MietteSpan,
    },

    /// 词法警告 — 不会阻止编译，但提醒潜在问题
    #[error("词法警告: {message}")]
    #[diagnostic(code(lexer::warning), severity(warning))]
    LexerWarning {
        message: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("{message}")]
        span: MietteSpan,
        #[help]
        help: Option<String>,
    },
}

impl LexerError {
    pub fn invalid_character(ch: char, source: &str, source_name: &str, offset: usize) -> Self {
        Self::InvalidCharacter {
            ch,
            src: NamedSource::new(source_name, source.to_string()),
            span: (offset, ch.len_utf8()).into(),
        }
    }

    pub fn unterminated_string(source: &str, source_name: &str, start: usize) -> Self {
        Self::UnterminatedString {
            src: NamedSource::new(source_name, source.to_string()),
            span: (start, 1).into(),
        }
    }

    pub fn invalid_escape(sequence: &str, source: &str, source_name: &str, offset: usize) -> Self {
        Self::InvalidEscapeSequence {
            sequence: sequence.to_string(),
            src: NamedSource::new(source_name, source.to_string()),
            span: (offset, sequence.len()).into(),
        }
    }

    pub fn invalid_number(source: &str, source_name: &str, offset: usize, len: usize) -> Self {
        Self::InvalidNumberLiteral {
            src: NamedSource::new(source_name, source.to_string()),
            span: (offset, len).into(),
        }
    }
}

/// 语法错误 — 基于 miette derive 的细粒度错误类型
#[derive(Error, Debug, miette::Diagnostic)]
pub enum ParserError {
    #[error("期望 {expected}，但找到 {found}")]
    #[diagnostic(code(parser::unexpected_token))]
    UnexpectedToken {
        expected: String,
        found: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("这里")]
        span: MietteSpan,
        #[help]
        help: Option<String>,
    },

    #[error("缺少分号")]
    #[diagnostic(code(parser::missing_semicolon), help("在语句末尾添加分号 ';'"))]
    MissingSemicolon {
        #[source_code]
        src: NamedSource<String>,
        #[label("这里应该有一个分号")]
        span: MietteSpan,
    },

    #[error("期望标识符")]
    #[diagnostic(code(parser::expected_identifier), help("使用有效的标识符名称（以字母或下划线开头）"))]
    ExpectedIdentifier {
        #[source_code]
        src: NamedSource<String>,
        #[label("这里")]
        span: MietteSpan,
    },

    #[error("未闭合的括号")]
    #[diagnostic(code(parser::unmatched_brace), help("确保所有括号都正确配对"))]
    UnmatchedBrace {
        brace: char,
        #[source_code]
        src: NamedSource<String>,
        #[label("未闭合的括号")]
        span: MietteSpan,
    },

    #[error("无效的表达式")]
    #[diagnostic(code(parser::invalid_expression))]
    InvalidExpression {
        #[source_code]
        src: NamedSource<String>,
        #[label("无效的表达式")]
        span: MietteSpan,
        #[help]
        help: Option<String>,
    },

    /// 语法警告 — 不会阻止编译，但提醒潜在问题
    #[error("语法警告: {message}")]
    #[diagnostic(code(parser::warning), severity(warning))]
    ParserWarning {
        message: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("{message}")]
        span: MietteSpan,
        #[help]
        help: Option<String>,
    },
}

impl ParserError {
    pub fn unexpected_token(
        expected: impl Into<String>,
        found: impl Into<String>,
        source: &str,
        source_name: &str,
        offset: usize,
        len: usize,
    ) -> Self {
        Self::UnexpectedToken {
            expected: expected.into(),
            found: found.into(),
            src: NamedSource::new(source_name, source.to_string()),
            span: (offset, len).into(),
            help: None,
        }
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        if let Self::UnexpectedToken { help: h, .. } = &mut self {
            *h = Some(help.into());
        }
        self
    }
}

/// 语义错误 — 基于 miette derive 的细粒度错误类型
#[derive(Error, Debug, miette::Diagnostic)]
pub enum SemanticError {
    #[error("未定义的标识符: {name}")]
    #[diagnostic(code(semantic::undefined_identifier), help("请检查拼写或声明该变量/函数"))]
    UndefinedIdentifier {
        name: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("未定义的标识符")]
        span: MietteSpan,
    },

    #[error("类型不匹配: 期望 {expected}，但找到 {found}")]
    #[diagnostic(code(semantic::type_mismatch), help("确保类型兼容或进行显式转换"))]
    TypeMismatch {
        expected: String,
        found: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("类型不匹配")]
        span: MietteSpan,
    },

    #[error("重复定义: {name}")]
    #[diagnostic(code(semantic::duplicate_definition), help("该名称已在作用域中定义，请使用不同的名称"))]
    DuplicateDefinition {
        name: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("重复定义")]
        span: MietteSpan,
    },

    #[error("'break' 只能在循环或switch中使用")]
    #[diagnostic(code(semantic::break_outside_loop), help("break只能在循环或switch语句内部使用"))]
    BreakOutsideLoop {
        #[source_code]
        src: NamedSource<String>,
        #[label("这里的break无效")]
        span: MietteSpan,
    },

    #[error("'continue' 只能在循环中使用")]
    #[diagnostic(code(semantic::continue_outside_loop), help("continue只能在循环内部使用"))]
    ContinueOutsideLoop {
        #[source_code]
        src: NamedSource<String>,
        #[label("这里的continue无效")]
        span: MietteSpan,
    },

    #[error("函数调用参数数量不匹配: 期望 {expected} 个，但找到 {found} 个")]
    #[diagnostic(code(semantic::arg_count_mismatch))]
    ArgCountMismatch {
        expected: usize,
        found: usize,
        #[source_code]
        src: NamedSource<String>,
        #[label("函数调用")]
        span: MietteSpan,
    },

    /// 语义警告 — 不会阻止编译，但提醒潜在问题
    #[error("语义警告: {message}")]
    #[diagnostic(code(semantic::warning), severity(warning))]
    SemanticWarning {
        message: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("{message}")]
        span: MietteSpan,
        #[help]
        help: Option<String>,
    },
}

impl SemanticError {
    pub fn undefined_identifier(
        name: &str,
        source: &str,
        source_name: &str,
        offset: usize,
        len: usize,
    ) -> Self {
        Self::UndefinedIdentifier {
            name: name.to_string(),
            src: NamedSource::new(source_name, source.to_string()),
            span: (offset, len).into(),
        }
    }

    pub fn type_mismatch(
        expected: impl Into<String>,
        found: impl Into<String>,
        source: &str,
        source_name: &str,
        offset: usize,
        len: usize,
    ) -> Self {
        Self::TypeMismatch {
            expected: expected.into(),
            found: found.into(),
            src: NamedSource::new(source_name, source.to_string()),
            span: (offset, len).into(),
        }
    }
}

/// 代码生成错误 — 基于 miette derive
#[derive(Error, Debug, miette::Diagnostic)]
pub enum CodeGenError {
    #[error("不支持的特性: {feature}")]
    #[diagnostic(code(codegen::unsupported_feature), help("该特性在当前版本中不受支持"))]
    UnsupportedFeature {
        feature: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("不支持的特性")]
        span: MietteSpan,
    },

    #[error("内部编译错误: {message}")]
    #[diagnostic(code(codegen::internal_error))]
    InternalError {
        message: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("错误位置")]
        span: MietteSpan,
    },

    /// 代码生成警告 — 不会阻止编译，但提醒潜在问题
    #[error("代码生成警告: {message}")]
    #[diagnostic(code(codegen::warning), severity(warning))]
    CodeGenWarning {
        message: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("{message}")]
        span: MietteSpan,
        #[help]
        help: Option<String>,
    },
}

/// 基于 miette 的通用编译结果类型
pub type MietteResult<T> = miette::Result<T>;

// ============================================================
// 向后兼容的 get_error_code / get_highlight_length / get_error_span
// ============================================================

fn get_error_code(error: &cayError) -> &'static str {
    match error {
        cayError::Lexer { .. } => "cavvy::lexer_error",
        cayError::Parser { .. } => "cavvy::parser_error",
        cayError::Semantic { .. } => "cavvy::semantic_error",
        cayError::TypeMismatch { .. } => "cavvy::type_mismatch",
        cayError::UndefinedIdentifier { .. } => "cavvy::undefined_identifier",
        cayError::DuplicateDefinition { .. } => "cavvy::duplicate_definition",
        cayError::CodeGen { .. } => "cavvy::codegen_error",
        cayError::Io { .. } => "cavvy::io_error",
        cayError::Llvm(_) => "cavvy::llvm_error",
        cayError::Preprocessor { .. } => "cavvy::preprocessor_error",
        cayError::MultipleErrors { .. } => "cavvy::multiple_errors",
    }
}

fn get_highlight_length(error: &cayError) -> usize {
    match error {
        cayError::UndefinedIdentifier { name, .. } => name.len(),
        cayError::DuplicateDefinition { name, .. } => name.len(),
        _ => 1,
    }
}

fn get_error_span(source: &str, line: usize, column: usize, error: &cayError) -> MietteSpan {
    let offset = line_col_to_offset(source, line, column);
    let length = match error {
        cayError::UndefinedIdentifier { name, .. } => name.len(),
        cayError::DuplicateDefinition { name, .. } => name.len(),
        cayError::TypeMismatch { .. } => {
            let rest = &source[offset..];
            rest.split_whitespace().next().map(|s| s.len()).unwrap_or(1)
        }
        _ => 1,
    };
    (offset, length).into()
}

// ============================================================
// 单元测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================
    // CompilerError 创建测试
    // ============================================================

    #[test]
    fn test_compiler_error_creation() {
        let loc = SourceLocation::new(Some("test.cay".into()), 10, 5);
        let err = error(ErrorCodes::SEMANTIC_TYPE_MISMATCH, CompilationPhase::Semantic, "类型不匹配", loc);
        assert_eq!(err.0.code, "E4003");
        assert_eq!(err.0.severity, Severity::Error);
        assert_eq!(err.0.message, "类型不匹配");
        assert_eq!(err.0.location.line, 10);
        assert_eq!(err.0.location.column, 5);
    }

    #[test]
    fn test_warning_creation() {
        let loc = SourceLocation::new(None, 1, 1);
        let warn = warning("W4001", CompilationPhase::Semantic, "未使用的变量", loc);
        assert_eq!(warn.0.severity, Severity::Warning);
        assert_eq!(warn.0.code, "W4001");
    }

    #[test]
    fn test_error_with_suggestion() {
        let loc = SourceLocation::new(None, 3, 8);
        let err = error_with_suggestion(
            ErrorCodes::SEMANTIC_TYPE_MISMATCH, CompilationPhase::Semantic,
            "类型不匹配: 期望 int, 实际 String", loc,
            "请使用 Integer.parseInt() 转换",
        );
        assert_eq!(err.0.suggestions.len(), 1);
        assert_eq!(err.0.suggestions[0].description, "请使用 Integer.parseInt() 转换");
    }

    // ============================================================
    // 双向转换测试
    // ============================================================

    #[test]
    fn test_cay_error_to_compiler_error() {
        let cay = lexer_error(1, 3, "Unexpected character: '@'");
        let compiler: CompilerError = cay.into();
        assert!(compiler.0.code.starts_with('E'));
        assert_eq!(compiler.0.phase, CompilationPhase::Lexer);
        assert!(compiler.0.message.contains("Unexpected character"));
    }

    #[test]
    fn test_compiler_error_to_cay_error() {
        let loc = SourceLocation::new(None, 5, 2);
        let compiler = error(ErrorCodes::SEMANTIC_TYPE_MISMATCH, CompilationPhase::Semantic, "类型错误", loc);
        let cay: cayError = compiler.into();
        match cay {
            cayError::Semantic { line, column, .. } => {
                assert_eq!(line, 5);
                assert_eq!(column, 2);
            }
            _ => panic!("Expected Semantic error"),
        }
    }

    #[test]
    fn test_roundtrip_cay_compiler_cay() {
        let original = lexer_error(2, 7, "未闭合的字符串字面量");
        let compiler: CompilerError = original.clone().into();
        let roundtrip: cayError = compiler.into();
        let msg = format!("{}", roundtrip);
        assert!(msg.contains("未闭合") || msg.contains("2"));
    }

    // ============================================================
    // 构造函数兼容性测试
    // ============================================================

    #[test]
    fn test_semantic_error_uses_error_code() {
        let err = semantic_error(10, 5, "Undefined variable 'x'");
        let compiler: CompilerError = err.into();
        assert_eq!(compiler.0.code, "E4001");
    }

    #[test]
    fn test_type_mismatch_error_uses_error_code() {
        let err = type_mismatch_error(3, 1, "int", "String");
        let compiler: CompilerError = err.into();
        assert_eq!(compiler.0.code, "E4003");
    }

    #[test]
    fn test_parser_error_uses_error_code() {
        let err = parser_error(7, 1, "Expected ';' after expression");
        let compiler: CompilerError = err.into();
        assert_eq!(compiler.0.code, "E3002");
    }

    #[test]
    fn test_undefined_identifier_error_uses_error_code() {
        let err = undefined_identifier_error(4, 2, "foo");
        let compiler: CompilerError = err.into();
        assert_eq!(compiler.0.code, "E4001");
    }

    #[test]
    fn test_duplicate_definition_error_uses_error_code() {
        let err = duplicate_definition_error(6, 3, "MyClass");
        let compiler: CompilerError = err.into();
        assert_eq!(compiler.0.code, "E4002");
    }

    // ============================================================
    // SourceLocation 测试
    // ============================================================

    #[test]
    fn test_source_location_default() {
        let loc = SourceLocation::default();
        assert_eq!(loc.line, 0);
        assert_eq!(loc.column, 0);
        assert_eq!(loc.file, None);
    }

    #[test]
    fn test_source_location_display() {
        let loc = SourceLocation::new(Some("main.cay".into()), 42, 7);
        let display = format!("{}", loc);
        assert!(display.contains("main.cay"));
        assert!(display.contains("42"));
        assert!(display.contains("7"));
    }

    // ============================================================
    // DiagnosticCollector 测试
    // ============================================================

    #[test]
    fn test_diagnostic_collector_add_cay_error() {
        let mut collector = DiagnosticCollector::new();
        let cay = lexer_error(1, 1, "Unexpected character");
        collector.add_cay_error(&cay);
        assert!(collector.has_errors());
        assert_eq!(collector.error_count(), 1);
    }

    #[test]
    fn test_diagnostic_collector_with_warnings() {
        let mut collector = DiagnosticCollector::new();
        let loc = SourceLocation::default();
        let w = warning("W4001", CompilationPhase::Semantic, "未使用的变量 'x'", loc);
        collector.add(w.0);
        assert!(!collector.has_errors());
        assert_eq!(collector.warning_count(), 1);
        assert_eq!(collector.error_count(), 0);
    }

    #[test]
    fn test_diagnostic_collector_multiple_errors() {
        let mut collector = DiagnosticCollector::new();
        let loc = SourceLocation::default();
        collector.add(Diagnostic::error("E4001", CompilationPhase::Semantic, "err1", loc.clone()));
        collector.add(Diagnostic::error("E4002", CompilationPhase::Semantic, "err2", loc.clone()));
        collector.add(Diagnostic::warning("W4001", CompilationPhase::Semantic, "warn1", loc));
        assert!(collector.has_errors());
        assert_eq!(collector.error_count(), 2);
        assert_eq!(collector.warning_count(), 1);
        assert_eq!(collector.diagnostics().len(), 3);
    }

    // ============================================================
    // Diagnostic builder 测试
    // ============================================================

    #[test]
    fn test_diagnostic_builder_chain() {
        let diag = Diagnostic::error("E4001", CompilationPhase::Semantic, "错误", SourceLocation::default())
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

    // ============================================================
    // ErrorCodes 测试
    // ============================================================

    #[test]
    fn test_error_codes() {
        assert_eq!(ErrorCodes::get_description("E4001"), "未定义的标识符");
        assert_eq!(ErrorCodes::get_description("E9999"), "未知错误");
    }

    // ============================================================
    // line_col_to_offset 测试
    // ============================================================

    #[test]
    fn test_line_col_to_offset() {
        let source = "line1\nline2\nline3";
        assert_eq!(line_col_to_offset(source, 1, 1), 0);
        assert_eq!(line_col_to_offset(source, 2, 1), 6);
        assert_eq!(line_col_to_offset(source, 3, 1), 12);
    }

    #[test]
    fn test_line_col_to_offset_multibyte() {
        let source = "你好\n世界";
        assert_eq!(line_col_to_offset(source, 1, 1), 0);
        assert_eq!(line_col_to_offset(source, 2, 1), 7);
    }

    // ============================================================
    // miette 错误测试
    // ============================================================

    #[test]
    fn test_lexer_error_display() {
        let err = LexerError::invalid_character('@', "int x = @;", "test.cay", 8);
        assert!(err.to_string().contains('@'));
    }

    // ============================================================
    // print_diagnostics 测试
    // ============================================================

    #[test]
    fn test_print_diagnostics_single_error() {
        let mut collector = DiagnosticCollector::new();
        let diag = Diagnostic::error(
            ErrorCodes::SEMANTIC_TYPE_MISMATCH, CompilationPhase::Semantic,
            "类型不匹配: 期望 int, 实际 String",
            SourceLocation::new(None, 3, 8),
        ).with_suggestion(FixSuggestion::new("请使用 Integer.parseInt() 转换"));
        collector.add(diag);

        let source = "int x = \"hello\";\nint y = 42;\n";
        print_diagnostics(&collector, source, "test.cay");
        assert!(collector.has_errors());
        assert_eq!(collector.error_count(), 1);
    }

    #[test]
    fn test_print_diagnostics_empty_collector() {
        let collector = DiagnosticCollector::new();
        print_diagnostics(&collector, "", "empty.cay");
        assert!(!collector.has_errors());
    }

    #[test]
    fn test_print_diagnostics_with_warnings() {
        let mut collector = DiagnosticCollector::new();
        let loc = SourceLocation::new(None, 1, 1);
        collector.add(Diagnostic::warning("W4001", CompilationPhase::Semantic, "未使用的变量", loc));
        assert!(!collector.has_errors());
        assert_eq!(collector.warning_count(), 1);
    }
}
