use miette::{Diagnostic, NamedSource, SourceSpan};
use std::fmt;
use std::sync::Arc;
use thiserror::Error;

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
// 新错误系统：统一 CompilerError（包装 Diagnostic）
// ============================================================

/// 新的统一编译器错误类型 —— 所有错误最终都表达为一个 Diagnostic。
/// Phase 2 迁移完成后，cayError 将被废弃，CompilerError 成为唯一错误类型。
#[derive(Debug, Clone)]
pub struct CompilerError(pub crate::diagnostic::Diagnostic);

impl fmt::Display for CompilerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.0.code, self.0.message)
    }
}

impl std::error::Error for CompilerError {}

// ============================================================
// miette::Diagnostic 实现 —— 提供漂亮的终端错误展示
// ============================================================

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
        // 取第一个修复建议作为 help
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
        let len = 1usize; // 默认高亮1个字符
        let label = diag.message.clone();
        let span = miette::LabeledSpan::new_with_span(
            Some(label),
            miette::SourceSpan::new(offset.into(), len),
        );
        Some(Box::new(std::iter::once(span)))
    }

    fn source_code(&self) -> Option<&dyn miette::SourceCode> {
        // 源文件内容由调用方通过 Report::with_source_code() 注入
        None
    }
}

impl From<cayError> for CompilerError {
    fn from(e: cayError) -> Self {
        let diagnostic = match &e {
            cayError::Lexer {
                file,
                line,
                column,
                message,
                suggestion,
            } => {
                let code = if message.contains("未闭合") || message.contains("Unterminated") {
                    crate::diagnostic::ErrorCodes::LEXER_UNTERMINATED_STRING
                } else {
                    crate::diagnostic::ErrorCodes::LEXER_INVALID_CHARACTER
                };
                crate::diagnostic::Diagnostic::error(
                    code,
                    crate::diagnostic::CompilationPhase::Lexer,
                    message.clone(),
                    SourceLocation {
                        file: file.clone(),
                        line: *line,
                        column: *column,
                    },
                )
                .with_suggestion(crate::diagnostic::FixSuggestion::new(suggestion.clone()))
            }
            cayError::Parser {
                file,
                line,
                column,
                message,
                suggestion,
            } => {
                let code = if message.contains("';'") || message.contains("分号") {
                    crate::diagnostic::ErrorCodes::PARSER_EXPECTED_SEMICOLON
                } else if message.contains("'{'")
                    || message.contains("'}'")
                    || message.contains("大括号")
                {
                    crate::diagnostic::ErrorCodes::PARSER_EXPECTED_BRACE
                } else if message.contains("'('")
                    || message.contains("')'")
                    || message.contains("括号")
                {
                    crate::diagnostic::ErrorCodes::PARSER_EXPECTED_PAREN
                } else {
                    crate::diagnostic::ErrorCodes::PARSER_UNEXPECTED_TOKEN
                };
                crate::diagnostic::Diagnostic::error(
                    code,
                    crate::diagnostic::CompilationPhase::Parser,
                    message.clone(),
                    SourceLocation {
                        file: file.clone(),
                        line: *line,
                        column: *column,
                    },
                )
                .with_suggestion(crate::diagnostic::FixSuggestion::new(suggestion.clone()))
            }
            cayError::Semantic {
                file,
                line,
                column,
                message,
                suggestion,
            } => {
                let code = if message.contains("Undefined")
                    || message.contains("未定义")
                    || message.contains("not found")
                {
                    crate::diagnostic::ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER
                } else if message.contains("Duplicate") || message.contains("重复") {
                    crate::diagnostic::ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION
                } else if message.contains("type")
                    || message.contains("类型")
                    || message.contains("assign")
                    || message.contains("Cannot")
                {
                    crate::diagnostic::ErrorCodes::SEMANTIC_TYPE_MISMATCH
                } else {
                    crate::diagnostic::ErrorCodes::SEMANTIC_INVALID_OPERATION
                };
                crate::diagnostic::Diagnostic::error(
                    code,
                    crate::diagnostic::CompilationPhase::Semantic,
                    message.clone(),
                    SourceLocation {
                        file: file.clone(),
                        line: *line,
                        column: *column,
                    },
                )
                .with_suggestion(crate::diagnostic::FixSuggestion::new(suggestion.clone()))
            }
            cayError::CodeGen {
                code,
                file,
                line,
                column,
                message,
                suggestion,
                is_warning,
            } => {
                // line 为 0 时退回到 1，确保诊断显示有源码上下文
                let display_line = if *line == 0 { 1 } else { *line };
                let display_column = if *column == 0 { 1 } else { *column };
                let severity = if *is_warning {
                    crate::diagnostic::Severity::Warning
                } else {
                    crate::diagnostic::Severity::Error
                };
                crate::diagnostic::Diagnostic::new(
                    code.clone(),
                    severity,
                    crate::diagnostic::CompilationPhase::CodeGen,
                    message.clone(),
                    SourceLocation {
                        file: file.clone(),
                        line: display_line,
                        column: display_column,
                    },
                )
                .with_suggestion(crate::diagnostic::FixSuggestion::new(suggestion.clone()))
            }
            cayError::Io { file, message } => crate::diagnostic::Diagnostic::new(
                "I0001".to_string(),
                crate::diagnostic::Severity::Error,
                crate::diagnostic::CompilationPhase::Linker,
                message.clone(),
                SourceLocation {
                    file: file.clone(),
                    line: 1,
                    column: 1,
                },
            ),
            cayError::Llvm(msg) => crate::diagnostic::Diagnostic::error(
                crate::diagnostic::ErrorCodes::CODEGEN_LLVM_ERROR,
                crate::diagnostic::CompilationPhase::CodeGen,
                msg.clone(),
                SourceLocation::default(),
            ),
            cayError::TypeMismatch {
                file,
                line,
                column,
                message,
                expected,
                actual,
                suggestion,
            } => crate::diagnostic::Diagnostic::error(
                crate::diagnostic::ErrorCodes::SEMANTIC_TYPE_MISMATCH,
                crate::diagnostic::CompilationPhase::Semantic,
                format!("{}: 期望 '{}', 实际 '{}'", message, expected, actual),
                SourceLocation {
                    file: file.clone(),
                    line: *line,
                    column: *column,
                },
            )
            .with_suggestion(crate::diagnostic::FixSuggestion::new(suggestion.clone())),
            cayError::UndefinedIdentifier {
                file,
                line,
                column,
                name,
                suggestion,
            } => crate::diagnostic::Diagnostic::error(
                crate::diagnostic::ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER,
                crate::diagnostic::CompilationPhase::Semantic,
                format!("未定义的标识符: '{}'", name),
                SourceLocation {
                    file: file.clone(),
                    line: *line,
                    column: *column,
                },
            )
            .with_suggestion(crate::diagnostic::FixSuggestion::new(suggestion.clone())),
            cayError::DuplicateDefinition {
                file,
                line,
                column,
                name,
                suggestion,
            } => crate::diagnostic::Diagnostic::error(
                crate::diagnostic::ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
                crate::diagnostic::CompilationPhase::Semantic,
                format!("重复定义: '{}'", name),
                SourceLocation {
                    file: file.clone(),
                    line: *line,
                    column: *column,
                },
            )
            .with_suggestion(crate::diagnostic::FixSuggestion::new(suggestion.clone())),
            cayError::Preprocessor {
                file,
                line,
                column,
                message,
                suggestion,
            } => crate::diagnostic::Diagnostic::error(
                crate::diagnostic::ErrorCodes::PREPROCESSOR_DEFINE_ERROR,
                crate::diagnostic::CompilationPhase::Preprocessor,
                message.clone(),
                SourceLocation {
                    file: file.clone(),
                    line: *line,
                    column: *column,
                },
            )
            .with_suggestion(crate::diagnostic::FixSuggestion::new(suggestion.clone())),
            cayError::MultipleErrors { errors } => {
                // 取第一个错误的 Diagnostic
                if let Some(first) = errors.first() {
                    return CompilerError::from(first.clone());
                }
                crate::diagnostic::Diagnostic::error(
                    "E9999",
                    crate::diagnostic::CompilationPhase::Semantic,
                    format!("发现 {} 个错误", errors.len()),
                    SourceLocation::default(),
                )
            }
        };
        CompilerError(diagnostic)
    }
}

// 反向转换：CompilerError → cayError（迁移期间桥接）
impl From<CompilerError> for cayError {
    fn from(e: CompilerError) -> Self {
        let d = &e.0;
        let message = d.message.clone();
        let suggestion = d
            .suggestions
            .first()
            .map(|s| s.description.clone())
            .unwrap_or_else(|| "请检查代码".to_string());
        let file = d.location.file.clone();

        match d.phase {
            CompilationPhase::Lexer => cayError::Lexer {
                file,
                line: d.location.line,
                column: d.location.column,
                message,
                suggestion,
            },
            CompilationPhase::Parser => cayError::Parser {
                file,
                line: d.location.line,
                column: d.location.column,
                message,
                suggestion,
            },
            CompilationPhase::Semantic => cayError::Semantic {
                file,
                line: d.location.line,
                column: d.location.column,
                message,
                suggestion,
            },
            CompilationPhase::Preprocessor => cayError::Preprocessor {
                file,
                line: d.location.line,
                column: d.location.column,
                message,
                suggestion,
            },
            _ => cayError::CodeGen {
                code: d.code.clone(),
                file,
                line: d.location.line,
                column: d.location.column,
                message,
                suggestion,
                is_warning: d.severity == crate::diagnostic::Severity::Warning,
            },
        }
    }
}

/// 新的统一编译结果类型
pub type CompilerResult<T> = Result<T, CompilerError>;

/// 为 DiagnosticCollector 添加便捷方法：收集 cayError
impl crate::diagnostic::DiagnosticCollector {
    /// 将一个 cayError 转换为 Diagnostic 并添加到收集器
    pub fn add_cay_error(&mut self, error: &cayError) {
        let compiler_err = CompilerError::from(error.clone());
        self.add(compiler_err.0);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceLocation {
    pub file: Option<String>, // 源文件路径（用于多文件include场景）
    pub line: usize,
    pub column: usize,
}

impl SourceLocation {
    /// 创建新的源位置
    pub fn new(file: Option<String>, line: usize, column: usize) -> Self {
        Self { file, line, column }
    }

    /// 从token创建源位置
    pub fn from_token(token: &crate::lexer::TokenWithLocation) -> Self {
        Self {
            file: token.source_file.clone(),
            line: token.source_line.unwrap_or(token.loc.line),
            column: token.loc.column,
        }
    }

    /// 获取文件路径，如果为None则返回默认空字符串
    pub fn file_str(&self) -> &str {
        self.file.as_deref().unwrap_or("")
    }
}

impl Default for SourceLocation {
    fn default() -> Self {
        Self {
            file: None,
            line: 0,
            column: 0, // 让Cavvy显示严重问题报错，到时候用户用的时候可以直接提issue不至于一脸懵逼
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

// FullSourceLocation 已合并到 SourceLocation，保留类型别名以便迁移
pub type FullSourceLocation = SourceLocation;

// ============================================================
// 新错误系统便捷构造函数（使用错误代码，Phase 2 迁移目标）
// ============================================================

use crate::diagnostic::{
    CompilationPhase, Diagnostic as CavvyDiagnostic, ErrorCodes, FixSuggestion, Severity,
};

/// 创建一个 CompilerError（错误级别）
pub fn error(
    code: &str,
    phase: CompilationPhase,
    message: impl Into<String>,
    location: SourceLocation,
) -> CompilerError {
    CompilerError(CavvyDiagnostic::error(code, phase, message, location))
}

/// 创建一个 CompilerError（警告级别）
pub fn warning(
    code: &str,
    phase: CompilationPhase,
    message: impl Into<String>,
    location: SourceLocation,
) -> CompilerError {
    CompilerError(CavvyDiagnostic::warning(code, phase, message, location))
}

/// 创建一个 CompilerError 并附带修复建议
pub fn error_with_suggestion(
    code: &str,
    phase: CompilationPhase,
    message: impl Into<String>,
    location: SourceLocation,
    suggestion: impl Into<String>,
) -> CompilerError {
    CompilerError(
        CavvyDiagnostic::error(code, phase, message, location)
            .with_suggestion(crate::diagnostic::FixSuggestion::new(suggestion)),
    )
}

// ============================================================
// 旧错误构造函数（现在内部使用 CompilerError + 错误代码）
// 迁移完成后，这些函数将逐步替换为直接使用 error()/warning()
// ============================================================

// 词法错误
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
    CompilerError(CavvyDiagnostic::error(
        code,
        CompilationPhase::Lexer,
        msg,
        SourceLocation::new(file, line, column),
    ))
    .into()
}

// 语法错误
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
    CompilerError(CavvyDiagnostic::error(
        code,
        CompilationPhase::Parser,
        msg,
        SourceLocation::new(file, line, column),
    ))
    .into()
}

// 语义错误
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
    CompilerError(CavvyDiagnostic::error(
        code,
        CompilationPhase::Semantic,
        msg,
        SourceLocation::new(file, line, column),
    ))
    .into()
}

// 代码生成错误（带源码位置）
pub fn codegen_error_at(loc: SourceLocation, message: impl Into<String>) -> cayError {
    let msg = message.into();
    let code = if msg.contains("Unsupported") {
        ErrorCodes::CODEGEN_UNSUPPORTED_FEATURE
    } else if msg.contains("not found") {
        ErrorCodes::CODEGEN_SYMBOL_NOT_FOUND
    } else {
        ErrorCodes::CODEGEN_INVALID_OPERATION
    };
    CompilerError(CavvyDiagnostic::error(
        code,
        CompilationPhase::CodeGen,
        msg,
        loc,
    ))
    .into()
}

// 代码生成警告（带源码位置）
pub fn codegen_warning_at(loc: SourceLocation, message: impl Into<String>) -> cayError {
    let msg = message.into();
    CompilerError(CavvyDiagnostic::warning(
        crate::diagnostic::ErrorCodes::CODEGEN_INVALID_OPERATION,
        CompilationPhase::CodeGen,
        msg,
        loc,
    ))
    .into()
}

// 代码生成错误（无源码位置 — 用于无法获取 AST 节点位置的场景）
// 不能使用了，因为没有源码位置
// pub fn codegen_error(message: impl Into<String>) -> cayError {
//     codegen_error_at(SourceLocation::default(), message)
// }

// 类型不匹配错误
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
        CavvyDiagnostic::error(
            ErrorCodes::SEMANTIC_TYPE_MISMATCH,
            CompilationPhase::Semantic,
            format!("类型不匹配: 期望 '{}', 实际 '{}'", expected_str, actual_str),
            SourceLocation::new(file, line, column),
        )
        .with_suggestion(FixSuggestion::new(format!(
            "请确保表达式返回 '{}' 类型的值",
            expected_str
        ))),
    )
    .into()
}

// 未定义标识符错误
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
        CavvyDiagnostic::error(
            ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER,
            CompilationPhase::Semantic,
            format!("未定义的标识符: '{}'", name_str),
            SourceLocation::new(file, line, column),
        )
        .with_suggestion(FixSuggestion::new(format!(
            "请检查 '{}' 的拼写，或在使用前声明该变量/函数",
            name_str
        ))),
    )
    .into()
}

// 重复定义错误
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
        CavvyDiagnostic::error(
            ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
            CompilationPhase::Semantic,
            format!("重复定义: '{}'", name_str),
            SourceLocation::new(file, line, column),
        )
        .with_suggestion(FixSuggestion::new(format!(
            "'{}' 已被定义，请使用不同的名称",
            name_str
        ))),
    )
    .into()
}

/// 将行号列号转换为字节偏移量
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

/// 计算错误位置的跨度
fn get_error_span(source: &str, line: usize, column: usize, error: &cayError) -> SourceSpan {
    let offset = line_col_to_offset(source, line, column);

    // 根据错误类型确定跨度长度
    let length = match error {
        cayError::UndefinedIdentifier { name, .. } => name.len(),
        cayError::DuplicateDefinition { name, .. } => name.len(),
        cayError::TypeMismatch { .. } => {
            // 尝试找到该位置的token长度
            let rest = &source[offset..];
            rest.split_whitespace().next().map(|s| s.len()).unwrap_or(1)
        }
        _ => 1,
    };

    (offset, length).into()
}

/// 获取错误代码
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

/// 获取错误消息（不含建议）
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

/// 获取帮助信息
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

/// 获取错误位置
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

/// 获取错误文件路径
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

/// 打印带有上下文的错误信息 - 使用miette格式
///
/// # Arguments
/// * `error` - 错误对象
/// * `source` - 源代码内容
/// * `filename` - 源文件名
///
/// # Example
/// ```
/// use cavvy::error::{lexer_error, print_error_with_context};
/// let error = lexer_error(1, 1, "无效的字符");
/// print_error_with_context(&error, "let x = @", "test.cay");
/// ```
pub fn print_error_with_context(error: &cayError, source: &str, filename: &str) {
    use crate::diagnostic::DiagnosticCollector;

    let mut collector = DiagnosticCollector::new();

    // 将所有错误收集到一个 DiagnosticCollector
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

    // 对于多文件场景，每个诊断可能引用不同的源文件。
    // print_diagnostics_per_file 会按文件分组展示。
    print_diagnostics_per_file(&collector, source, filename);
}

/// 按文件分组打印诊断（支持多文件 include 场景）
fn print_diagnostics_per_file(
    collector: &crate::diagnostic::DiagnosticCollector,
    default_source: &str,
    default_filename: &str,
) {
    use crate::diagnostic::print_diagnostics;
    use std::collections::HashMap;

    let diagnostics = collector.diagnostics();
    if diagnostics.is_empty() {
        return;
    }

    // 按文件分组
    let mut by_file: HashMap<String, (String, Vec<&crate::diagnostic::Diagnostic>)> =
        HashMap::new();
    let mut no_file_diags: Vec<&crate::diagnostic::Diagnostic> = Vec::new();

    for diag in diagnostics {
        if let Some(ref file) = diag.location.file {
            if !file.is_empty() {
                let entry = by_file.entry(file.clone()).or_insert_with(|| {
                    // 尝试读取该文件内容
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

    // 打印有明确文件的诊断
    for (file, (content, diags)) in &by_file {
        let mut sub_collector = crate::diagnostic::DiagnosticCollector::new();
        for d in diags {
            sub_collector.add((*d).clone());
        }
        print_diagnostics(&sub_collector, content, file);
    }

    // 打印无文件的诊断（使用默认源文件）
    if !no_file_diags.is_empty() {
        let mut sub_collector = crate::diagnostic::DiagnosticCollector::new();
        for d in no_file_diags {
            sub_collector.add(d.clone());
        }
        print_diagnostics(&sub_collector, default_source, default_filename);
    }
}

/// 获取高亮长度
fn get_highlight_length(error: &cayError) -> usize {
    match error {
        cayError::UndefinedIdentifier { name, .. } => name.len(),
        cayError::DuplicateDefinition { name, .. } => name.len(),
        _ => 1,
    }
}

/// 通用错误打印函数 - 用于非编译错误（如IO错误、配置错误等）
///
/// # Arguments
/// * `error_type` - 错误类型标识
/// * `message` - 错误消息
/// * `help` - 可选的帮助信息
///
/// # Example
/// ```
/// use cavvy::error::print_miette_error;
/// print_miette_error("cavvy::io_error", "无法读取文件", Some("请检查文件路径是否正确"));
/// ```
pub fn print_miette_error(error_type: &str, message: &str, help: Option<&str>) {
    eprintln!("\n  × {}: {}", error_type, message);

    if let Some(help_text) = help {
        if !help_text.is_empty() {
            eprintln!("  help: {}", help_text);
        }
    }

    eprintln!();
}

/// 编译阶段错误打印函数
///
/// # Arguments
/// * `stage` - 编译阶段（如 "词法分析", "语法分析" 等）
/// * `error` - 错误消息
/// * `source_path` - 源文件路径
/// * `help` - 可选的帮助信息
///
/// # Example
/// ```
/// use cavvy::error::print_compile_error;
/// print_compile_error("词法分析", "无效的字符", "test.cay", Some("请检查字符编码"));
/// ```
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

/// 外部工具错误打印函数
///
/// # Arguments
/// * `tool` - 工具名称（如 "clang", "ir2exe" 等）
/// * `message` - 错误消息
/// * `help` - 可选的帮助信息
///
/// # Example
/// ```
/// use cavvy::error::print_tool_error;
/// print_tool_error("clang", "编译失败", Some("请检查 LLVM 安装"));
/// ```
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

/// 警告信息打印函数
///
/// # Arguments
/// * `message` - 警告消息
///
/// # Example
/// ```
/// use cavvy::error::print_warning;
/// print_warning("未使用的变量 'x'");
/// ```
pub fn print_warning(message: &str) {
    eprintln!("  ⚠ cavvy::warning: {}", message);
}

/// 警告信息打印函数（带位置）
///
/// # Arguments
/// * `message` - 警告消息
/// * `filename` - 文件名
/// * `line` - 行号
/// * `column` - 列号
///
/// # Example
/// ```
/// use cavvy::error::print_warning_with_location;
/// print_warning_with_location("未使用的变量", "test.cay", 10, 5);
/// ```
#[deprecated()]
pub fn print_warning_with_location(message: &str, filename: &str, line: usize, column: usize) {
    eprintln!("  ⚠ cavvy::warning: {}", message);
    eprintln!("     位置: {}:{}:{}", filename, line, column);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::{
        CompilationPhase, Diagnostic, DiagnosticCollector, ErrorCodes, Severity,
    };

    // ============================================================
    // CompilerError 创建测试
    // ============================================================

    #[test]
    fn test_compiler_error_creation() {
        let loc = SourceLocation::new(Some("test.cay".into()), 10, 5);
        let err = error(
            ErrorCodes::SEMANTIC_TYPE_MISMATCH,
            CompilationPhase::Semantic,
            "类型不匹配",
            loc,
        );
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
            ErrorCodes::SEMANTIC_TYPE_MISMATCH,
            CompilationPhase::Semantic,
            "类型不匹配: 期望 int, 实际 String",
            loc,
            "请使用 Integer.parseInt() 转换",
        );
        assert_eq!(err.0.suggestions.len(), 1);
        assert_eq!(
            err.0.suggestions[0].description,
            "请使用 Integer.parseInt() 转换"
        );
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
        let compiler = error(
            ErrorCodes::SEMANTIC_TYPE_MISMATCH,
            CompilationPhase::Semantic,
            "类型错误",
            loc,
        );
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
        // 验证关键信息保留
        let msg = format!("{}", roundtrip);
        assert!(msg.contains("未闭合") || msg.contains("2"));
    }

    // ============================================================
    // 旧构造函数兼容性测试
    // ============================================================

    #[test]
    fn test_semantic_error_uses_error_code() {
        let err = semantic_error(10, 5, "Undefined variable 'x'");
        let compiler: CompilerError = err.into();
        assert_eq!(compiler.0.code, "E4001"); // SEMANTIC_UNDEFINED_IDENTIFIER
    }

    #[test]
    fn test_type_mismatch_error_uses_error_code() {
        let err = type_mismatch_error(3, 1, "int", "String");
        let compiler: CompilerError = err.into();
        assert_eq!(compiler.0.code, "E4003"); // SEMANTIC_TYPE_MISMATCH
    }

    #[test]
    fn test_parser_error_uses_error_code() {
        let err = parser_error(7, 1, "Expected ';' after expression");
        let compiler: CompilerError = err.into();
        assert_eq!(compiler.0.code, "E3002"); // PARSER_EXPECTED_SEMICOLON
    }

    #[test]
    fn test_undefined_identifier_error_uses_error_code() {
        let err = undefined_identifier_error(4, 2, "foo");
        let compiler: CompilerError = err.into();
        assert_eq!(compiler.0.code, "E4001"); // SEMANTIC_UNDEFINED_IDENTIFIER
    }

    #[test]
    fn test_duplicate_definition_error_uses_error_code() {
        let err = duplicate_definition_error(6, 3, "MyClass");
        let compiler: CompilerError = err.into();
        assert_eq!(compiler.0.code, "E4002"); // SEMANTIC_DUPLICATE_DEFINITION
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
    // DiagnosticCollector 集成测试
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

        // 添加警告
        let loc = SourceLocation::default();
        let warning = warning("W4001", CompilationPhase::Semantic, "未使用的变量 'x'", loc);
        collector.add(warning.0);

        assert!(!collector.has_errors());
        assert_eq!(collector.warning_count(), 1);
        assert_eq!(collector.error_count(), 0);
    }

    #[test]
    fn test_diagnostic_collector_multiple_errors() {
        let mut collector = DiagnosticCollector::new();
        let loc = SourceLocation::default();

        collector.add(Diagnostic::error(
            "E4001",
            CompilationPhase::Semantic,
            "err1",
            loc.clone(),
        ));
        collector.add(Diagnostic::error(
            "E4002",
            CompilationPhase::Semantic,
            "err2",
            loc.clone(),
        ));
        collector.add(Diagnostic::warning(
            "W4001",
            CompilationPhase::Semantic,
            "warn1",
            loc,
        ));

        assert!(collector.has_errors());
        assert_eq!(collector.error_count(), 2);
        assert_eq!(collector.warning_count(), 1);
        assert_eq!(collector.diagnostics().len(), 3);
    }
}
