//! Cavvy 统一诊断系统 v3 — 干净架构
//!
//! CayError 直接实现 miette::Diagnostic。
//! 所有错误构造函数要求显式传入 error_code — 不再使用字符串匹配推断。
//!
//! 架构: CayError → CayDiagnostic(轻量包装) → miette 输出
//! 已移除: CompilerError, DisplayDiagnostic, 旧 Diagnostic, 旧 DiagnosticCollector，
//!         以及一组零引用的 miette-derive 死类型（原「保留供未来使用」的
//!         按阶段划分的错误/警告结构体与枚举）

use miette::{NamedSource, SourceSpan as MietteSpan};
use serde::Serialize;
use std::collections::HashMap;
use std::fmt;
use thiserror::Error;

// ============================================================
// ErrorCodes
// ============================================================

pub struct ErrorCodes;

impl ErrorCodes {
    pub const PREPROCESSOR_DEFINE_ERROR: &'static str = "E1001";
    pub const PREPROCESSOR_IFDEF_ERROR: &'static str = "E1002";
    pub const PREPROCESSOR_INCLUDE_ERROR: &'static str = "E1003";
    pub const PREPROCESSOR_UNCLOSED_DIRECTIVE: &'static str = "E1004";
    pub const PREPROCESSOR_CIRCULAR_INCLUDE: &'static str = "E1005";
    pub const PREPROCESSOR_INVALID_MACRO: &'static str = "E1006";
    pub const PREPROCESSOR_INCLUDE_C_ERROR: &'static str = "E1007";
    pub const PREPROCESSOR_INCLUDE_H_ERROR: &'static str = "E1008";

    pub const LEXER_INVALID_CHARACTER: &'static str = "E2001";
    pub const LEXER_UNTERMINATED_STRING: &'static str = "E2002";
    pub const LEXER_INVALID_ESCAPE_SEQUENCE: &'static str = "E2003";
    pub const LEXER_INVALID_NUMBER_LITERAL: &'static str = "E2004";
    pub const LEXER_UNTERMINATED_COMMENT: &'static str = "E2005";
    pub const LEXER_INVALID_IDENTIFIER: &'static str = "E2006";

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
    pub const SEMANTIC_UNUSED_VARIABLE_ERROR: &'static str = "E4029";

    pub const CODEGEN_UNSUPPORTED_FEATURE: &'static str = "E5001";
    pub const CODEGEN_TYPE_CONVERSION_ERROR: &'static str = "E5002";
    pub const CODEGEN_SYMBOL_NOT_FOUND: &'static str = "E5003";
    pub const CODEGEN_INVALID_OPERATION: &'static str = "E5004";
    pub const CODEGEN_LLVM_ERROR: &'static str = "E5005";

    pub const LINKER_SYMBOL_NOT_FOUND: &'static str = "E6001";
    pub const LINKER_MULTIPLE_DEFINITION: &'static str = "E6002";
    pub const LINKER_LIBRARY_NOT_FOUND: &'static str = "E6003";

    pub const PREPROCESSOR_WARNING: &'static str = "W1001";
    pub const PREPROCESSOR_DEPRECATED: &'static str = "W1002";
    pub const LEXER_DEPRECATED_SYNTAX: &'static str = "W2001";
    pub const LEXER_PORTABILITY: &'static str = "W2002";
    pub const LEXER_STYLE_ALIAS_MIXING: &'static str = "W2003";
    pub const PARSER_DEPRECATED_FEATURE: &'static str = "W3001";
    pub const PARSER_EXTENSION: &'static str = "W3002";
    pub const SEMANTIC_WARN_UNUSED_VARIABLE: &'static str = "W4001";
    pub const SEMANTIC_WARN_UNREACHABLE_CODE: &'static str = "W4002";
    pub const SEMANTIC_DEPRECATED: &'static str = "W4003";
    pub const SEMANTIC_NON_STANDARD: &'static str = "W4004";
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
            Self::PREPROCESSOR_INCLUDE_C_ERROR => "C 头文件包含错误",
            Self::PREPROCESSOR_INCLUDE_H_ERROR => "Cavvy 头文件包含错误",
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
            Self::SEMANTIC_UNUSED_VARIABLE_ERROR => "未使用的变量",
            Self::CODEGEN_UNSUPPORTED_FEATURE => "不支持的功能",
            Self::CODEGEN_TYPE_CONVERSION_ERROR => "类型转换错误",
            Self::CODEGEN_SYMBOL_NOT_FOUND => "符号未找到",
            Self::CODEGEN_INVALID_OPERATION => "无效的操作",
            Self::CODEGEN_LLVM_ERROR => "LLVM错误",
            Self::LINKER_SYMBOL_NOT_FOUND => "链接符号未找到",
            Self::LINKER_MULTIPLE_DEFINITION => "重复定义",
            Self::LINKER_LIBRARY_NOT_FOUND => "库未找到",
            Self::PREPROCESSOR_WARNING => "预处理警告",
            Self::PREPROCESSOR_DEPRECATED => "预处理已弃用特性",
            Self::LEXER_DEPRECATED_SYNTAX => "已弃用的语法",
            Self::LEXER_PORTABILITY => "可移植性警告",
            Self::LEXER_STYLE_ALIAS_MIXING => "代码风格警告：别名混用",
            Self::PARSER_DEPRECATED_FEATURE => "已弃用的语言特性",
            Self::PARSER_EXTENSION => "扩展语法",
            Self::SEMANTIC_WARN_UNUSED_VARIABLE => "未使用的变量",
            Self::SEMANTIC_WARN_UNREACHABLE_CODE => "不可达代码",
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
            Self::LEXER_STYLE_ALIAS_MIXING => "请在同一源文件内统一使用同一种拼写",
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
// 基本类型
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Note,
    Warning,
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Note => write!(f, "提示"),
            Severity::Warning => write!(f, "警告"),
            Severity::Error => write!(f, "错误"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilationPhase {
    Preprocessor,
    Lexer,
    Parser,
    Semantic,
    CodeGen,
    Linker,
}

impl CompilationPhase {
    pub fn label(&self) -> &'static str {
        match self {
            CompilationPhase::Preprocessor => "预处理器错误",
            CompilationPhase::Lexer => "词法错误",
            CompilationPhase::Parser => "语法错误",
            CompilationPhase::Semantic => "类型错误",
            CompilationPhase::CodeGen => "代码生成错误",
            CompilationPhase::Linker => "链接错误",
        }
    }

    pub fn warning_label(&self) -> &'static str {
        match self {
            CompilationPhase::Preprocessor => "预处理器警告",
            CompilationPhase::Lexer => "词法警告",
            CompilationPhase::Parser => "语法警告",
            CompilationPhase::Semantic => "语义警告",
            CompilationPhase::CodeGen => "代码生成警告",
            CompilationPhase::Linker => "链接警告",
        }
    }
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

#[derive(Debug, Clone)]
pub struct RelatedInfo {
    pub message: String,
    pub location: SourceLocation,
}

// ============================================================
// CayError — 统一错误类型
// ============================================================
// 每个变体携带 error_code，在构造时由调用者显式指定。

#[derive(Error, Debug, Clone)]
pub enum CayError {
    #[error("词法错误 [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Lexer {
        error_code: &'static str,
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
    },

    #[error("语法错误 [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Parser {
        error_code: &'static str,
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
    },

    #[error("语义错误 [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Semantic {
        error_code: &'static str,
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
    },

    #[error("{kind} [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    CodeGen {
        error_code: &'static str,
        kind: String,
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
        is_warning: bool,
    },

    #[error("IO错误 [{}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Io {
        error_code: &'static str,
        file: Option<String>,
        message: String,
    },

    #[error("LLVM错误: {0}")]
    Llvm(String),

    #[error("类型错误 [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    TypeMismatch {
        error_code: &'static str,
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
        error_code: &'static str,
        file: Option<String>,
        line: usize,
        column: usize,
        name: String,
        suggestion: String,
    },

    #[error("重复定义 [{}:{line}:{column}]: '{name}'", file.as_deref().unwrap_or("<unknown>"))]
    DuplicateDefinition {
        error_code: &'static str,
        file: Option<String>,
        line: usize,
        column: usize,
        name: String,
        suggestion: String,
    },

    #[error("预处理器错误 [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Preprocessor {
        error_code: &'static str,
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
    },

    #[error("发现 {} 个错误", errors.len())]
    MultipleErrors { errors: Vec<CayError> },

    /// 统一的警告变体，可按阶段携带位置与建议。
    /// 与错误变体分离，避免在每个错误变体上重复 `is_warning` 字段。
    #[error("{phase_label} [{}:{line}:{column}]: {message}", file.as_deref().unwrap_or("<unknown>"))]
    Lint {
        error_code: &'static str,
        phase_label: CompilationPhase,
        file: Option<String>,
        line: usize,
        column: usize,
        message: String,
        suggestion: String,
    },
}

pub type CayResult<T> = Result<T, CayError>;

impl CayError {
    pub fn error_code(&self) -> &'static str {
        match self {
            CayError::Lexer { error_code, .. }
            | CayError::Parser { error_code, .. }
            | CayError::Semantic { error_code, .. }
            | CayError::CodeGen { error_code, .. }
            | CayError::Io { error_code, .. }
            | CayError::TypeMismatch { error_code, .. }
            | CayError::UndefinedIdentifier { error_code, .. }
            | CayError::DuplicateDefinition { error_code, .. }
            | CayError::Preprocessor { error_code, .. } => error_code,
            CayError::Lint { error_code, .. } => error_code,
            CayError::Llvm(_) => ErrorCodes::CODEGEN_LLVM_ERROR,
            CayError::MultipleErrors { .. } => "E9999",
        }
    }
    pub fn phase(&self) -> CompilationPhase {
        match self {
            CayError::Lexer { .. } => CompilationPhase::Lexer,
            CayError::Parser { .. } => CompilationPhase::Parser,
            CayError::Semantic { .. }
            | CayError::TypeMismatch { .. }
            | CayError::UndefinedIdentifier { .. }
            | CayError::DuplicateDefinition { .. } => CompilationPhase::Semantic,
            CayError::CodeGen { .. } => CompilationPhase::CodeGen,
            CayError::Preprocessor { .. } => CompilationPhase::Preprocessor,
            CayError::Io { .. } | CayError::Llvm(_) => CompilationPhase::Linker,
            CayError::MultipleErrors { .. } => CompilationPhase::Semantic,
            CayError::Lint { phase_label, .. } => *phase_label,
        }
    }
    pub fn severity(&self) -> Severity {
        match self {
            // 严重级别由结构化的 is_warning 标志决定，
            // 不再解析 kind 字段中的中文魔法字符串。
            // 注: 当前所有 CodeGen 构造点的 kind 仅为显示用标签
            // （"代码生成错误"/"代码生成警告"）；非警告即错误，
            // 编译失败语义由是否返回 Err 表达，不设独立 Fatal 级别。
            CayError::CodeGen {
                is_warning: true, ..
            }
            | CayError::Lint { .. } => Severity::Warning,
            _ => Severity::Error,
        }
    }
    pub fn suggestion_text(&self) -> Option<&str> {
        match self {
            CayError::Lexer { suggestion, .. }
            | CayError::Parser { suggestion, .. }
            | CayError::Semantic { suggestion, .. }
            | CayError::CodeGen { suggestion, .. }
            | CayError::TypeMismatch { suggestion, .. }
            | CayError::UndefinedIdentifier { suggestion, .. }
            | CayError::DuplicateDefinition { suggestion, .. }
            | CayError::Preprocessor { suggestion, .. }
            | CayError::Lint { suggestion, .. } => {
                if suggestion.is_empty() {
                    None
                } else {
                    Some(suggestion.as_str())
                }
            }
            _ => None,
        }
    }
    pub fn location(&self) -> Option<(usize, usize)> {
        match self {
            CayError::Lexer { line, column, .. }
            | CayError::Parser { line, column, .. }
            | CayError::Semantic { line, column, .. }
            | CayError::CodeGen { line, column, .. }
            | CayError::TypeMismatch { line, column, .. }
            | CayError::UndefinedIdentifier { line, column, .. }
            | CayError::DuplicateDefinition { line, column, .. }
            | CayError::Preprocessor { line, column, .. }
            | CayError::Lint { line, column, .. } => Some((*line, *column)),
            _ => None,
        }
    }
    pub fn file(&self) -> Option<&str> {
        match self {
            CayError::Lexer { file, .. }
            | CayError::Parser { file, .. }
            | CayError::Semantic { file, .. }
            | CayError::CodeGen { file, .. }
            | CayError::Io { file, .. }
            | CayError::TypeMismatch { file, .. }
            | CayError::UndefinedIdentifier { file, .. }
            | CayError::DuplicateDefinition { file, .. }
            | CayError::Preprocessor { file, .. }
            | CayError::Lint { file, .. } => file.as_deref(),
            _ => None,
        }
    }
    pub fn message(&self) -> String {
        match self {
            CayError::Lexer { message, .. }
            | CayError::Parser { message, .. }
            | CayError::Semantic { message, .. }
            | CayError::CodeGen { message, .. }
            | CayError::Io { message, .. }
            | CayError::Preprocessor { message, .. }
            | CayError::Lint { message, .. } => message.clone(),
            CayError::TypeMismatch { message, .. } => message.clone(),
            CayError::UndefinedIdentifier { name, .. } => format!("未定义的标识符: '{}'", name),
            CayError::DuplicateDefinition { name, .. } => format!("重复定义: '{}'", name),
            CayError::Llvm(msg) => msg.clone(),
            CayError::MultipleErrors { errors } => format!("发现 {} 个错误", errors.len()),
        }
    }
}

impl miette::Diagnostic for CayError {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new(self.error_code()))
    }
    fn severity(&self) -> Option<miette::Severity> {
        match self.severity() {
            Severity::Error => Some(miette::Severity::Error),
            Severity::Warning => Some(miette::Severity::Warning),
            _ => Some(miette::Severity::Advice),
        }
    }
    fn help<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        self.suggestion_text()
            .map(|s| Box::new(s) as Box<dyn fmt::Display>)
    }
    fn labels(&self) -> Option<Box<dyn Iterator<Item = miette::LabeledSpan> + '_>> {
        None
    }
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        None
    }
}

// ============================================================
// 便捷构造函数 — 显式 error_code（不再使用字符串匹配）
// ============================================================

pub fn lexer_error(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    CayError::Lexer {
        error_code: code,
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
    }
}

pub fn lexer_error_with_suggestion(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) -> CayError {
    CayError::Lexer {
        error_code: code,
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: suggestion.into(),
    }
}

pub fn lexer_error_with_file(
    code: &'static str,
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    CayError::Lexer {
        error_code: code,
        file,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
    }
}

pub fn lexer_error_with_file_and_suggestion(
    code: &'static str,
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) -> CayError {
    CayError::Lexer {
        error_code: code,
        file,
        line,
        column,
        message: message.into(),
        suggestion: suggestion.into(),
    }
}

pub fn parser_error(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    CayError::Parser {
        error_code: code,
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
    }
}

pub fn parser_error_with_suggestion(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) -> CayError {
    CayError::Parser {
        error_code: code,
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: suggestion.into(),
    }
}

pub fn parser_error_with_file(
    code: &'static str,
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    CayError::Parser {
        error_code: code,
        file,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
    }
}

pub fn parser_error_with_file_and_suggestion(
    code: &'static str,
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) -> CayError {
    CayError::Parser {
        error_code: code,
        file,
        line,
        column,
        message: message.into(),
        suggestion: suggestion.into(),
    }
}

pub fn semantic_error(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    CayError::Semantic {
        error_code: code,
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
    }
}

pub fn semantic_error_with_suggestion(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) -> CayError {
    CayError::Semantic {
        error_code: code,
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: suggestion.into(),
    }
}

pub fn semantic_error_with_file(
    code: &'static str,
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    CayError::Semantic {
        error_code: code,
        file,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
    }
}

pub fn semantic_error_with_file_and_suggestion(
    code: &'static str,
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) -> CayError {
    CayError::Semantic {
        error_code: code,
        file,
        line,
        column,
        message: message.into(),
        suggestion: suggestion.into(),
    }
}

pub fn codegen_error(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    CayError::CodeGen {
        error_code: code,
        kind: "代码生成错误".to_string(),
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
        is_warning: false,
    }
}

pub fn codegen_error_at(
    code: &'static str,
    loc: SourceLocation,
    message: impl Into<String>,
) -> CayError {
    CayError::CodeGen {
        error_code: code,
        kind: "代码生成错误".to_string(),
        file: loc.file,
        line: loc.line,
        column: loc.column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
        is_warning: false,
    }
}

pub fn codegen_warning(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    CayError::CodeGen {
        error_code: code,
        kind: "代码生成警告".to_string(),
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
        is_warning: true,
    }
}

pub fn codegen_warning_at(
    code: &'static str,
    loc: SourceLocation,
    message: impl Into<String>,
) -> CayError {
    CayError::CodeGen {
        error_code: code,
        kind: "代码生成警告".to_string(),
        file: loc.file,
        line: loc.line,
        column: loc.column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
        is_warning: true,
    }
}

pub fn type_mismatch_error(
    line: usize,
    column: usize,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> CayError {
    type_mismatch_error_with_file(None, line, column, expected, actual)
}

pub fn type_mismatch_error_with_file(
    file: Option<String>,
    line: usize,
    column: usize,
    expected: impl Into<String>,
    actual: impl Into<String>,
) -> CayError {
    let expected_str = expected.into();
    let actual_str = actual.into();
    CayError::TypeMismatch {
        error_code: ErrorCodes::SEMANTIC_TYPE_MISMATCH,
        file,
        line,
        column,
        message: format!("类型不匹配: 期望 '{}', 实际 '{}'", expected_str, actual_str),
        expected: expected_str.clone(),
        actual: actual_str,
        suggestion: format!("请确保表达式返回 '{}' 类型的值", expected_str),
    }
}

pub fn undefined_identifier_error(line: usize, column: usize, name: impl Into<String>) -> CayError {
    undefined_identifier_error_with_file(None, line, column, name)
}

pub fn undefined_identifier_error_with_file(
    file: Option<String>,
    line: usize,
    column: usize,
    name: impl Into<String>,
) -> CayError {
    let name_str = name.into();
    CayError::UndefinedIdentifier {
        error_code: ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER,
        file,
        line,
        column,
        name: name_str.clone(),
        suggestion: format!("请检查 '{}' 的拼写，或在使用前声明该变量/函数", name_str),
    }
}

pub fn duplicate_definition_error(line: usize, column: usize, name: impl Into<String>) -> CayError {
    duplicate_definition_error_with_file(None, line, column, name)
}

pub fn duplicate_definition_error_with_file(
    file: Option<String>,
    line: usize,
    column: usize,
    name: impl Into<String>,
) -> CayError {
    let name_str = name.into();
    CayError::DuplicateDefinition {
        error_code: ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
        file,
        line,
        column,
        name: name_str.clone(),
        suggestion: format!("'{}' 已被定义，请使用不同的名称", name_str),
    }
}

pub fn preprocessor_error(
    code: &'static str,
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
    suggestion: impl Into<String>,
) -> CayError {
    CayError::Preprocessor {
        error_code: code,
        file,
        line,
        column,
        message: message.into(),
        suggestion: suggestion.into(),
    }
}

// ============================================================
// 统一警告构造函数
// ============================================================

pub fn lint_warning(
    phase: CompilationPhase,
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    CayError::Lint {
        error_code: code,
        phase_label: phase,
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
    }
}

pub fn lint_warning_at(
    phase: CompilationPhase,
    code: &'static str,
    loc: SourceLocation,
    message: impl Into<String>,
) -> CayError {
    CayError::Lint {
        error_code: code,
        phase_label: phase,
        file: loc.file,
        line: loc.line,
        column: loc.column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
    }
}

pub fn lexer_warning(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    lint_warning(CompilationPhase::Lexer, code, line, column, message)
}

pub fn lexer_warning_at(
    code: &'static str,
    loc: SourceLocation,
    message: impl Into<String>,
) -> CayError {
    lint_warning_at(CompilationPhase::Lexer, code, loc, message)
}

pub fn parser_warning(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    lint_warning(CompilationPhase::Parser, code, line, column, message)
}

pub fn parser_warning_at(
    code: &'static str,
    loc: SourceLocation,
    message: impl Into<String>,
) -> CayError {
    lint_warning_at(CompilationPhase::Parser, code, loc, message)
}

pub fn semantic_warning(
    code: &'static str,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    lint_warning(CompilationPhase::Semantic, code, line, column, message)
}

pub fn semantic_warning_at(
    code: &'static str,
    loc: SourceLocation,
    message: impl Into<String>,
) -> CayError {
    lint_warning_at(CompilationPhase::Semantic, code, loc, message)
}

pub fn preprocessor_warning(
    code: &'static str,
    file: Option<String>,
    line: usize,
    column: usize,
    message: impl Into<String>,
) -> CayError {
    CayError::Lint {
        error_code: code,
        phase_label: CompilationPhase::Preprocessor,
        file,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
    }
}

pub fn io_error(file: Option<String>, message: impl Into<String>) -> CayError {
    CayError::Io {
        error_code: "I0001",
        file,
        message: message.into(),
    }
}

// ============================================================
// 查询函数
// ============================================================

pub fn get_error_message(error: &CayError) -> String {
    error.message()
}
pub fn get_error_help(error: &CayError) -> Option<String> {
    error.suggestion_text().map(|s| s.to_string())
}
pub fn get_error_location(error: &CayError) -> Option<(usize, usize)> {
    error.location()
}
pub fn get_error_file(error: &CayError) -> Option<String> {
    error.file().map(|s| s.to_string())
}

// ============================================================
// 打印函数
// ============================================================

pub fn print_miette_error(error_type: &str, message: &str, help: Option<&str>) {
    eprintln!("\n  × {}: {}", error_type, message);
    if let Some(h) = help {
        if !h.is_empty() {
            eprintln!("  help: {}", h);
        }
    }
    eprintln!();
}

pub fn print_miette_warning(warning_type: &str, message: &str, help: Option<&str>) {
    eprintln!("\n  ⚠ {}: {}", warning_type, message);
    if let Some(h) = help {
        if !h.is_empty() {
            eprintln!("  help: {}", h);
        }
    }
    eprintln!();
}

pub fn print_tool_error(tool: &str, message: &str, help: Option<&str>) {
    eprintln!("\n  × cavvy::tool_error: {} 执行失败", tool);
    eprintln!("   │\n   │ {}", message);
    if let Some(h) = help {
        eprintln!("   │\n  help: {}", h);
    }
    eprintln!();
}

pub fn print_warning(message: &str) {
    eprintln!("  ⚠ cavvy::warning: {}", message);
}

// ============================================================
// CayDiagnostic — 轻量渲染包装
// ============================================================

#[derive(Debug, Clone)]
enum HighlightKind {
    Fixed(usize),
    ToWhitespace,
}

#[derive(Debug)]
struct CayDiagnostic {
    error_code: &'static str,
    severity: miette::Severity,
    message: String,
    help: Option<String>,
    source: String,
    filename: String,
    phase_label: String,
    line: usize,
    column: usize,
    highlight_kind: HighlightKind,
}

impl CayDiagnostic {
    fn new(error: &CayError, source: &str, filename: &str) -> Self {
        let (line, column) = error.location().unwrap_or((0, 0));
        let highlight_kind = match error {
            CayError::UndefinedIdentifier { name, .. }
            | CayError::DuplicateDefinition { name, .. } => HighlightKind::Fixed(name.len()),
            _ => HighlightKind::ToWhitespace,
        };
        Self {
            error_code: error.error_code(),
            severity: match error.severity() {
                Severity::Error => miette::Severity::Error,
                Severity::Warning => miette::Severity::Warning,
                _ => miette::Severity::Advice,
            },
            message: error.message(),
            help: error.suggestion_text().map(|s| s.to_string()),
            source: source.to_string(),
            filename: filename.to_string(),
            phase_label: if error.severity() == Severity::Warning {
                error.phase().warning_label().to_string()
            } else {
                error.phase().label().to_string()
            },
            line,
            column,
            highlight_kind,
        }
    }
    fn compute_highlight_len(&self, offset: usize) -> usize {
        match &self.highlight_kind {
            // Fixed 存的是标识符字节长度（String::len 即 UTF-8 字节数）
            HighlightKind::Fixed(len) => *len,
            // LabeledSpan 使用字节长度：累加字符的 UTF-8 字节数，而非字符个数，
            // 否则多字节字符（如中文标识符）会导致高亮错位
            HighlightKind::ToWhitespace => self.source[offset..]
                .chars()
                .take_while(|c| !c.is_whitespace())
                .map(char::len_utf8)
                .sum::<usize>()
                .max(1),
        }
    }
    fn compute_labels(&self) -> Vec<miette::LabeledSpan> {
        if self.line == 0 {
            return Vec::new();
        }
        let offset = line_col_to_offset(&self.source, self.line, self.column);
        if offset >= self.source.len() {
            return Vec::new();
        }
        let span_len = self
            .compute_highlight_len(offset)
            .min(self.source.len() - offset)
            .max(1);
        vec![miette::LabeledSpan::new_with_span(
            Some(self.phase_label.clone()),
            MietteSpan::new(offset.into(), span_len),
        )]
    }
}

impl fmt::Display for CayDiagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.error_code, self.message)
    }
}

impl std::error::Error for CayDiagnostic {}

impl miette::Diagnostic for CayDiagnostic {
    fn code<'a>(&'a self) -> Option<Box<dyn fmt::Display + 'a>> {
        Some(Box::new(self.error_code))
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
        let labels = self.compute_labels();
        if labels.is_empty() {
            None
        } else {
            Some(Box::new(labels.into_iter()))
        }
    }
    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        None
    }
}

// ============================================================
// 行/列辅助函数
// ============================================================

pub fn line_col_to_offset(source: &str, line: usize, column: usize) -> usize {
    let mut current_line: usize = 1;
    let mut current_col: usize = 1;
    for (offset, ch) in source.char_indices() {
        if current_line == line && current_col == column {
            return offset;
        }
        if ch == '\n' {
            current_line = current_line.saturating_add(1);
            current_col = 1;
        } else {
            current_col = current_col.saturating_add(1);
        }
    }
    source.len()
}

pub fn line_range(source: &str, line: usize) -> (usize, usize) {
    let mut current_line: usize = 1;
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
            current_line = current_line.saturating_add(1);
        }
    }
    (source.len(), source.len())
}

// ============================================================
// print_diagnostics
// ============================================================

pub fn print_diagnostics(errors: &[CayError], source: &str, filename: &str) {
    if errors.is_empty() {
        return;
    }
    let error_count = errors
        .iter()
        .filter(|e| e.severity() == Severity::Error)
        .count();
    let warning_count = errors
        .iter()
        .filter(|e| e.severity() == Severity::Warning)
        .count();
    eprintln!();
    for error in errors {
        let diag = CayDiagnostic::new(error, source, filename);
        let report = miette::Report::new(diag)
            .with_source_code(NamedSource::new(filename, source.to_string()));
        let mut handler = miette::GraphicalReportHandler::new();
        let mut output = String::new();
        if handler.render_report(&mut output, report.as_ref()).is_ok() {
            eprintln!("{}", output);
        } else {
            eprintln!(
                "  × [{}] {}: {}",
                error.error_code(),
                error.phase().label(),
                error.message()
            );
        }
    }
    eprintln!(
        "  编译结果: {}\n",
        match (error_count, warning_count) {
            (e, 0) if e > 0 => format!("{} 个错误", e),
            (0, w) if w > 0 => format!("{} 个警告", w),
            (e, w) => format!("{} 个错误, {} 个警告", e, w),
        }
    );
}

pub fn print_error_with_context(error: &CayError, source: &str, filename: &str) {
    match error {
        CayError::MultipleErrors { errors } => {
            print_diagnostics_by_file(errors, source, filename);
        }
        single => {
            print_diagnostics_by_file(&[single.clone()], source, filename);
        }
    }
}

pub fn print_diagnostics_by_file(
    errors: &[CayError],
    default_source: &str,
    default_filename: &str,
) {
    if errors.is_empty() {
        return;
    }
    let mut by_file: HashMap<String, (String, Vec<&CayError>)> = HashMap::new();
    let mut no_file_errors: Vec<&CayError> = Vec::new();
    for error in errors {
        if let Some(file) = error.file() {
            if !file.is_empty() {
                by_file
                    .entry(file.to_string())
                    .or_insert_with(|| {
                        (
                            std::fs::read_to_string(file)
                                .unwrap_or_else(|_| default_source.to_string()),
                            Vec::new(),
                        )
                    })
                    .1
                    .push(error);
                continue;
            }
        }
        no_file_errors.push(error);
    }
    for (filename, (content, file_errors)) in &by_file {
        let owned: Vec<CayError> = file_errors.iter().map(|e| (*e).clone()).collect();
        print_diagnostics(&owned, content, filename);
    }
    if !no_file_errors.is_empty() {
        let owned: Vec<CayError> = no_file_errors.iter().map(|e| (*e).clone()).collect();
        print_diagnostics(&owned, default_source, default_filename);
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lexer_error() {
        let err = lexer_error(ErrorCodes::LEXER_INVALID_CHARACTER, 1, 3, "Unexpected '@'");
        assert_eq!(err.error_code(), ErrorCodes::LEXER_INVALID_CHARACTER);
    }
    #[test]
    fn test_parser_error() {
        let err = parser_error(ErrorCodes::PARSER_EXPECTED_SEMICOLON, 7, 1, "Expected ';'");
        assert_eq!(err.error_code(), ErrorCodes::PARSER_EXPECTED_SEMICOLON);
    }
    #[test]
    fn test_semantic_error() {
        let err = semantic_error(
            ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER,
            10,
            5,
            "Undefined 'x'",
        );
        assert_eq!(err.error_code(), ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER);
    }
    #[test]
    fn test_type_mismatch() {
        let err = type_mismatch_error(3, 1, "int", "String");
        assert_eq!(err.error_code(), ErrorCodes::SEMANTIC_TYPE_MISMATCH);
    }
    #[test]
    fn test_undefined_identifier() {
        let err = undefined_identifier_error(4, 2, "foo");
        assert_eq!(err.error_code(), ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER);
    }
    #[test]
    fn test_duplicate_definition() {
        let err = duplicate_definition_error(6, 3, "MyClass");
        assert_eq!(err.error_code(), ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION);
    }
    #[test]
    fn test_codegen_error() {
        let err = codegen_error(ErrorCodes::CODEGEN_INVALID_OPERATION, 8, 4, "bad");
        assert_eq!(err.error_code(), ErrorCodes::CODEGEN_INVALID_OPERATION);
    }
    #[test]
    fn test_codegen_warning() {
        let err = codegen_warning(ErrorCodes::CODEGEN_SUBOPTIMAL, 5, 2, "suboptimal");
        assert_eq!(err.severity(), Severity::Warning);
    }
    #[test]
    fn test_io_error() {
        let err = io_error(Some("t.cay".into()), "not found");
        assert_eq!(err.error_code(), "I0001");
    }
    #[test]
    fn test_preprocessor_error() {
        let err = preprocessor_error(
            ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
            Some("t.cay".into()),
            1,
            1,
            "bad",
            "fix",
        );
        assert_eq!(err.error_code(), ErrorCodes::PREPROCESSOR_DEFINE_ERROR);
    }
    #[test]
    fn test_source_location() {
        let loc = SourceLocation::new(Some("m.cay".into()), 42, 7);
        assert!(format!("{}", loc).contains("m.cay"));
    }
    #[test]
    fn test_error_codes() {
        assert_eq!(ErrorCodes::get_description("E4001"), "未定义的标识符");
    }
    #[test]
    fn test_line_col_to_offset() {
        assert_eq!(line_col_to_offset("line1\nline2\nline3", 1, 1), 0);
        assert_eq!(line_col_to_offset("你好\n世界", 2, 1), 7);
    }
    #[test]
    fn test_print_diagnostics_empty() {
        print_diagnostics(&[], "", "e.cay");
    }
    #[test]
    fn test_print_diagnostics_single() {
        let e = semantic_error(ErrorCodes::SEMANTIC_TYPE_MISMATCH, 3, 8, "bad");
        print_diagnostics(&[e], "x\ny\n", "t.cay");
    }
    #[test]
    fn test_print_diagnostics_warning() {
        let e = codegen_warning(ErrorCodes::CODEGEN_SUBOPTIMAL, 1, 1, "w");
        print_diagnostics(&[e], "x", "t.cay");
    }
    #[test]
    fn test_miette_diag() {
        let e = lexer_error(ErrorCodes::LEXER_UNTERMINATED_STRING, 2, 5, "unclosed");
        assert!(miette::Diagnostic::code(&e).is_some());
    }
    #[test]
    fn test_cay_diag_render() {
        let e = lexer_error(ErrorCodes::LEXER_INVALID_CHARACTER, 1, 3, "bad");
        let d = CayDiagnostic::new(&e, "abc@def", "t.cay");
        let r = miette::Report::new(d)
            .with_source_code(NamedSource::new("t.cay", "abc@def".to_string()));
        let mut h = miette::GraphicalReportHandler::new();
        let mut o = String::new();
        assert!(h.render_report(&mut o, r.as_ref()).is_ok());
        assert!(!o.is_empty());
    }
}
