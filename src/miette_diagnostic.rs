//! Cavvy 诊断系统 - 基于 miette 的实现
//!
//! 提供美观、友好的错误报告，包括：
//! - 彩色错误输出
//! - 源代码片段高亮
//! - 错误代码和链接
//! - 多错误收集

use miette::{Diagnostic, NamedSource, SourceSpan};
use thiserror::Error;

/// Cavvy 编译错误类型
#[derive(Error, Debug, Diagnostic)]
#[error("{message}")]
#[diagnostic()]
pub struct CavvyError {
    /// 错误消息
    message: String,

    /// 错误代码
    #[diagnostic(code)]
    code: String,

    /// 源代码
    #[source_code]
    src: NamedSource<String>,

    /// 错误位置标签
    #[label("{label_text}")]
    span: SourceSpan,

    /// 标签文本（存储用）
    #[diagnostic(transparent)]
    label_text: String,

    /// 帮助信息
    #[help]
    help: Option<String>,
}

impl CavvyError {
    /// 创建新的编译错误
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

    /// 添加帮助信息
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }
}

/// 词法错误
#[derive(Error, Debug, Diagnostic)]
pub enum LexerError {
    #[error("非法字符: {ch}")]
    #[diagnostic(
        code(lexer::invalid_character),
        help("请删除非法字符或使用支持的字符替换")
    )]
    InvalidCharacter {
        ch: char,
        #[source_code]
        src: NamedSource<String>,
        #[label("非法字符在这里")]
        span: SourceSpan,
    },

    #[error("未闭合的字符串字面量")]
    #[diagnostic(code(lexer::unterminated_string), help("请在字符串末尾添加双引号"))]
    UnterminatedString {
        #[source_code]
        src: NamedSource<String>,
        #[label("字符串从这里开始")]
        span: SourceSpan,
    },

    #[error("无效的转义序列: {sequence}")]
    #[diagnostic(
        code(lexer::invalid_escape),
        help("有效的转义序列: \\n, \\t, \\\", \\\\")
    )]
    InvalidEscapeSequence {
        sequence: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("无效的转义序列")]
        span: SourceSpan,
    },

    #[error("无效的数字字面量")]
    #[diagnostic(
        code(lexer::invalid_number),
        help("支持的格式: 十进制(123), 十六进制(0xFF), 二进制(0b101)")
    )]
    InvalidNumberLiteral {
        #[source_code]
        src: NamedSource<String>,
        #[label("无效的数字格式")]
        span: SourceSpan,
    },
}

impl LexerError {
    /// 创建非法字符错误
    pub fn invalid_character(ch: char, source: &str, source_name: &str, offset: usize) -> Self {
        Self::InvalidCharacter {
            ch,
            src: NamedSource::new(source_name, source.to_string()),
            span: (offset, ch.len_utf8()).into(),
        }
    }

    /// 创建未闭合字符串错误
    pub fn unterminated_string(source: &str, source_name: &str, start: usize) -> Self {
        Self::UnterminatedString {
            src: NamedSource::new(source_name, source.to_string()),
            span: (start, 1).into(),
        }
    }

    /// 创建无效转义序列错误
    pub fn invalid_escape(sequence: &str, source: &str, source_name: &str, offset: usize) -> Self {
        Self::InvalidEscapeSequence {
            sequence: sequence.to_string(),
            src: NamedSource::new(source_name, source.to_string()),
            span: (offset, sequence.len()).into(),
        }
    }

    /// 创建无效数字错误
    pub fn invalid_number(source: &str, source_name: &str, offset: usize, len: usize) -> Self {
        Self::InvalidNumberLiteral {
            src: NamedSource::new(source_name, source.to_string()),
            span: (offset, len).into(),
        }
    }
}

/// 语法错误
#[derive(Error, Debug, Diagnostic)]
pub enum ParserError {
    #[error("期望 {expected}，但找到 {found}")]
    #[diagnostic(code(parser::unexpected_token))]
    UnexpectedToken {
        expected: String,
        found: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("这里")]
        span: SourceSpan,
        #[help]
        help: Option<String>,
    },

    #[error("缺少分号")]
    #[diagnostic(code(parser::missing_semicolon), help("在语句末尾添加分号 ';'"))]
    MissingSemicolon {
        #[source_code]
        src: NamedSource<String>,
        #[label("这里应该有一个分号")]
        span: SourceSpan,
    },

    #[error("期望标识符")]
    #[diagnostic(
        code(parser::expected_identifier),
        help("使用有效的标识符名称（以字母或下划线开头）")
    )]
    ExpectedIdentifier {
        #[source_code]
        src: NamedSource<String>,
        #[label("这里")]
        span: SourceSpan,
    },

    #[error("未闭合的括号")]
    #[diagnostic(code(parser::unmatched_brace), help("确保所有括号都正确配对"))]
    UnmatchedBrace {
        brace: char,
        #[source_code]
        src: NamedSource<String>,
        #[label("未闭合的括号")]
        span: SourceSpan,
    },

    #[error("无效的表达式")]
    #[diagnostic(code(parser::invalid_expression))]
    InvalidExpression {
        #[source_code]
        src: NamedSource<String>,
        #[label("无效的表达式")]
        span: SourceSpan,
        #[help]
        help: Option<String>,
    },
}

impl ParserError {
    /// 创建意外令牌错误
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

    /// 添加帮助信息
    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        if let Self::UnexpectedToken { help: h, .. } = &mut self {
            *h = Some(help.into());
        }
        self
    }
}

/// 语义错误
#[derive(Error, Debug, Diagnostic)]
pub enum SemanticError {
    #[error("未定义的标识符: {name}")]
    #[diagnostic(
        code(semantic::undefined_identifier),
        help("请检查拼写或声明该变量/函数")
    )]
    UndefinedIdentifier {
        name: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("未定义的标识符")]
        span: SourceSpan,
    },

    #[error("类型不匹配: 期望 {expected}，但找到 {found}")]
    #[diagnostic(code(semantic::type_mismatch), help("确保类型兼容或进行显式转换"))]
    TypeMismatch {
        expected: String,
        found: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("类型不匹配")]
        span: SourceSpan,
    },

    #[error("重复定义: {name}")]
    #[diagnostic(
        code(semantic::duplicate_definition),
        help("该名称已在作用域中定义，请使用不同的名称")
    )]
    DuplicateDefinition {
        name: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("重复定义")]
        span: SourceSpan,
    },

    #[error("'break' 只能在循环或switch中使用")]
    #[diagnostic(
        code(semantic::break_outside_loop),
        help("break只能在循环或switch语句内部使用")
    )]
    BreakOutsideLoop {
        #[source_code]
        src: NamedSource<String>,
        #[label("这里的break无效")]
        span: SourceSpan,
    },

    #[error("'continue' 只能在循环中使用")]
    #[diagnostic(
        code(semantic::continue_outside_loop),
        help("continue只能在循环内部使用")
    )]
    ContinueOutsideLoop {
        #[source_code]
        src: NamedSource<String>,
        #[label("这里的continue无效")]
        span: SourceSpan,
    },

    #[error("函数调用参数数量不匹配: 期望 {expected} 个，但找到 {found} 个")]
    #[diagnostic(code(semantic::arg_count_mismatch))]
    ArgCountMismatch {
        expected: usize,
        found: usize,
        #[source_code]
        src: NamedSource<String>,
        #[label("函数调用")]
        span: SourceSpan,
    },
}

impl SemanticError {
    /// 创建未定义标识符错误
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

    /// 创建类型不匹配错误
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

/// 代码生成错误
#[derive(Error, Debug, Diagnostic)]
pub enum CodeGenError {
    #[error("不支持的特性: {feature}")]
    #[diagnostic(code(codegen::unsupported_feature), help("该特性在当前版本中不受支持"))]
    UnsupportedFeature {
        feature: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("不支持的特性")]
        span: SourceSpan,
    },

    #[error("内部编译错误: {message}")]
    #[diagnostic(code(codegen::internal_error))]
    InternalError {
        message: String,
        #[source_code]
        src: NamedSource<String>,
        #[label("错误位置")]
        span: SourceSpan,
    },
}

/// 编译结果类型
pub type Result<T> = miette::Result<T>;

/// 将行号和列号转换为字节偏移量
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

/// 计算行的字节偏移范围
pub fn line_range(source: &str, line: usize) -> (usize, usize) {
    let mut current_line = 1;

    for (offset, ch) in source.char_indices() {
        if current_line == line {
            // 找到行尾
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_line_col_to_offset() {
        let source = "line1\nline2\nline3";
        assert_eq!(line_col_to_offset(source, 1, 1), 0);
        assert_eq!(line_col_to_offset(source, 2, 1), 6);
        assert_eq!(line_col_to_offset(source, 3, 1), 12);
    }

    #[test]
    fn test_lexer_error_display() {
        let err = LexerError::invalid_character('@', "int x = @;", "test.cay", 8);
        assert!(err.to_string().contains('@'));
    }
}
