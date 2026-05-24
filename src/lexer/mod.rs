use logos::Logos;
use crate::error::{cayResult};
use crate::error::SourceLocation;
use crate::diagnostic::{Diagnostic, DiagnosticCollector, ErrorCodes, CompilationPhase, SourceSpan, FixSuggestion};

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\f]+")]
#[logos(skip r"//.*")]
pub enum Token {
    // 多行注释 - 需要特殊处理以计数换行符
    #[regex(r"/\*([^*]|\*[^/])*\*/", |lex| {
        let slice = lex.slice();
        let newline_count = slice.chars().filter(|&c| c == '\n').count();
        Some(newline_count)
    })]
    BlockComment(Option<usize>),
    // 关键字
    #[token("public")]
    Public,
    #[token("private")]
    Private,
    #[token("protected")]
    Protected,
    #[token("static")]
    Static,
    #[token("final")]
    Final,
    #[token("abstract")]
    Abstract,
    #[token("native")]
    Native,
    // 注解 - 注意：@main 和 @Override 是完整的令牌，不是 @ + 标识符
    #[token("@main")]
    AtMain,
    #[token("@Override")]
    AtOverride,
    #[token("@")]
    At,
    #[token("class")]
    Class,
    #[token("void")]
    Void,
    #[token("int")]
    Int,
    #[token("long")]
    Long,
    #[token("float")]
    Float,
    #[token("double")]
    Double,
    #[token("bool")]
    #[token("boolean")]
    Bool,
    #[token("string")]
    #[token("String")]
    String,
    #[token("char")]
    Char,
    #[token("true")]
    True,
    #[token("false")]
    False,
    #[token("null")]
    Null,
    #[token("if")]
    If,
    #[token("else")]
    Else,
    #[token("while")]
    While,
    #[token("for")]
    For,
    #[token("do")]
    Do,
    #[token("switch")]
    Switch,
    #[token("case")]
    Case,
    #[token("default")]
    Default,
    #[token("return")]
    Return,
    #[token("break")]
    Break,
    #[token("continue")]
    Continue,
    #[token("new")]
    New,
    #[token("this")]
    This,
    #[token("super")]
    Super,
    #[token("extends")]
    Extends,
    #[token("implements")]
    Implements,
    #[token("interface")]
    Interface,
    #[token("instanceof")]
    InstanceOf,
    #[token("var")]
    Var,
    #[token("let")]
    Let,
    #[token("auto")]
    Auto,
    #[token("extern")]
    Extern,
    #[token("namespace")]
    Namespace,
    #[token("using")]
    Using,
    #[token("scope")]
    Scope,
    #[token("__ir")]
    InlineIr,

    // FFI 类型关键字
    #[token("c_int")]
    CInt,
    #[token("c_uint")]
    CUInt,
    #[token("c_long")]
    CLong,
    #[token("c_short")]
    CShort,
    #[token("c_ushort")]
    CUShort,
    #[token("c_char")]
    CChar,
    #[token("c_uchar")]
    CUChar,
    #[token("c_float")]
    CFloat,
    #[token("c_double")]
    CDouble,
    #[token("size_t")]
    SizeT,
    #[token("ssize_t")]
    SSizeT,
    #[token("uintptr_t")]
    UIntPtr,
    #[token("intptr_t")]
    IntPtr,
    #[token("c_void")]
    CVoid,
    #[token("c_bool")]
    CBool,
    #[token("c_string")]
    CString,
    #[token("c_int64_t")]
    CInt64,
    #[token("c_uint64_t")]
    CUInt64,

    // 调用约定关键字
    #[token("cdecl")]
    Cdecl,
    #[token("stdcall")]
    Stdcall,
    #[token("fastcall")]
    Fastcall,
    #[token("sysv64")]
    Sysv64,
    #[token("win64")]
    Win64,

    // 类型别名关键字
    #[token("alias")]
    Alias,
    #[token("fn")]
    Fn,

    // extern 别名关键字
    #[token("as")]
    As,

    // 标识符
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Identifier(String),
    
    // 字面量
    #[regex(r"(?:0[xX][0-9a-fA-F][0-9a-fA-F_]*|0[bB][01][01_]*|0[oO]?[0-7][0-7_]*|[0-9][0-9_]*)[Ll]?", |lex| {
        let slice = lex.slice();
        // 分离后缀
        let (num_str, suffix) = if slice.ends_with('L') || slice.ends_with('l') {
            (&slice[..slice.len()-1], Some(slice.chars().last().unwrap()))
        } else {
            (slice, None)
        };
        // 移除下划线
        let cleaned: String = num_str.chars().filter(|c| *c != '_').collect();
        // 解析数字
        let radix = if cleaned.starts_with("0x") || cleaned.starts_with("0X") {
            16
        } else if cleaned.starts_with("0b") || cleaned.starts_with("0B") {
            2
        } else if cleaned.starts_with("0o") || cleaned.starts_with("0O") {
            8
        } else if cleaned.starts_with("0") && cleaned.len() > 1 && cleaned.chars().nth(1).map(|c| c.is_digit(10)).unwrap_or(false) {
            // 以0开头但不含字母的十进制数字？实际上，前导零的十进制数字，但我们将视为十进制（如Java中，前导零表示八进制，但为了兼容性，我们将其视为八进制？我们已匹配八进制模式，所以这里应该是十进制）
            10
        } else {
            10
        };
        let num = if radix == 10 {
            cleaned.parse::<i64>().ok()
        } else {
            i64::from_str_radix(&cleaned[2..], radix).ok()
        };
        num.map(|val| (val, suffix))
    })]
    IntegerLiteral(Option<(i64, Option<char>)>),
    
    // 浮点数字面量 - 支持以下格式：
    // 1. 标准小数: 3.14, .5, 2.
    // 2. 科学计数法: 1e10, 1.5e-10, 2E+5
    // 3. 带后缀: 3.14f, 1e10d
    #[regex(r"(?:[0-9][0-9_]*\.[0-9][0-9_]*|\.[0-9][0-9_]*|[0-9][0-9_]*\.)(?:[eE][+-]?[0-9][0-9_]*)?[FfDd]?|[0-9][0-9_]*[eE][+-]?[0-9][0-9_]*[FfDd]?", |lex| {
        let slice = lex.slice();
        let (num_str, suffix) = if slice.ends_with('F') || slice.ends_with('f') {
            (&slice[..slice.len()-1], Some('f'))
        } else if slice.ends_with('D') || slice.ends_with('d') {
            (&slice[..slice.len()-1], Some('d'))
        } else {
            (slice, None)
        };
        // 移除下划线
        let cleaned: String = num_str.chars().filter(|c| *c != '_').collect();
        cleaned.parse::<f64>().ok().map(|val| (val, suffix))
    })]
    FloatLiteral(Option<(f64, Option<char>)>),
    
    #[regex(r#""([^"\\]|\\.)*""#, |lex| {
        let s = lex.slice();
        let content = &s[1..s.len()-1];
        Some(process_escape_sequences(content))
    })]
    StringLiteral(Option<String>),
    
    #[regex(r"'([^'\\]|\\.)'", |lex| {
        let s = lex.slice();
        let content = &s[1..s.len()-1];
        process_char_escape(content)
    })]
    CharLiteral(Option<char>),
    
    // 运算符
    #[token("+")]
    Plus,
    #[token("-")]
    Minus,
    #[token("*")]
    Star,
    #[token("/")]
    Slash,
    #[token("%")]
    Percent,
    #[token("==")]
    EqEq,
    #[token("!=")]
    NotEq,
    #[token("<")]
    Lt,
    #[token("<=")]
    Le,
    #[token(">")]
    Gt,
    #[token(">=")]
    Ge,
    #[token("&&")]
    AndAnd,
    #[token("||")]
    OrOr,
    #[token("!")]
    Bang,
    #[token("&")]
    Ampersand,
    #[token("|")]
    Pipe,
    #[token("^")]
    Caret,
    #[token("<<")]
    Shl,
    #[token(">>")]
    Shr,
    #[token(">>>")]
    UnsignedShr,
    #[token("~")]
    Tilde,
    
    // 赋值运算符
    #[token("=")]
    Assign,
    #[token("+=")]
    AddAssign,
    #[token("-=")]
    SubAssign,
    #[token("*=")]
    MulAssign,
    #[token("/=")]
    DivAssign,
    #[token("%=")]
    ModAssign,
    
    // 自增自减
    #[token("++")]
    Inc,
    #[token("--")]
    Dec,
    
    // 分隔符
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token(";")]
    Semicolon,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token("...")]
    DotDotDot,
    #[token(":")]
    Colon,
    #[token("::")]
    DoubleColon,
    #[token("->")]
    Arrow,
    #[token("?")]
    Question,

    // 换行（用于跟踪行号）- 支持 Windows \r\n 和 Unix \n
    #[regex(r"\r?\n")]
    Newline,
}

#[derive(Debug, Clone)]
pub struct TokenWithLocation {
    pub token: Token,
    pub loc: SourceLocation,
    /// 原始源文件路径（用于支持#include后的错误定位）
    pub source_file: Option<String>,
    /// 原始源文件行号
    pub source_line: Option<usize>,
}

impl TokenWithLocation {
    /// 创建带源映射的token
    pub fn with_source(token: Token, loc: SourceLocation, file: Option<String>, line: Option<usize>) -> Self {
        Self {
            token,
            loc,
            source_file: file,
            source_line: line,
        }
    }

    /// 获取用于错误报告的文件路径
    pub fn get_file(&self) -> Option<&str> {
        self.source_file.as_deref()
    }

    /// 获取用于错误报告的行号
    pub fn get_line(&self) -> usize {
        self.source_line.unwrap_or(self.loc.line)
    }
}

/// 词法分析错误类型
#[derive(Debug, Clone)]
pub enum LexerErrorType {
    InvalidCharacter,
    UnterminatedString,
    InvalidEscapeSequence,
    InvalidNumberLiteral,
    UnterminatedComment,
    InvalidIdentifier,
}

/// 增强的词法分析器，支持诊断收集和源映射
pub struct Lexer<'a> {
    source: &'a str,
    inner: logos::Lexer<'a, Token>,
    line: usize,
    column: usize,
    diagnostics: DiagnosticCollector,
    collect_all_errors: bool,
    /// 当前源文件路径（用于#include后的错误定位）
    current_source_file: Option<String>,
    /// 源映射表：输出行号 -> (原始文件, 原始行号)
    source_map: std::collections::HashMap<usize, (String, usize)>,
    /// 是否保留换行token（用于内联IR等需要行分隔的场景）
    preserve_newlines: bool,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            inner: Token::lexer(source),
            line: 1,
            column: 1,
            diagnostics: DiagnosticCollector::new(),
            collect_all_errors: false,
            current_source_file: None,
            source_map: std::collections::HashMap::new(),
            preserve_newlines: false,
        }
    }

    /// 创建保留换行token的词法分析器（用于内联IR解析）
    pub fn with_preserve_newlines(source: &'a str) -> Self {
        Self {
            source,
            inner: Token::lexer(source),
            line: 1,
            column: 1,
            diagnostics: DiagnosticCollector::new(),
            collect_all_errors: false,
            current_source_file: None,
            source_map: std::collections::HashMap::new(),
            preserve_newlines: true,
        }
    }

    /// 创建带源映射的词法分析器
    pub fn with_source_map(source: &'a str, source_map: std::collections::HashMap<usize, (String, usize)>) -> Self {
        Self::with_source_map_and_file(source, source_map, None)
    }

    /// 创建带源映射和当前文件路径的词法分析器
    pub fn with_source_map_and_file(source: &'a str, source_map: std::collections::HashMap<usize, (String, usize)>, current_file: Option<String>) -> Self {
        Self {
            source,
            inner: Token::lexer(source),
            line: 1,
            column: 1,
            diagnostics: DiagnosticCollector::new(),
            collect_all_errors: false,
            current_source_file: current_file,
            source_map,
            preserve_newlines: false,
        }
    }

    /// 启用多错误收集模式
    pub fn with_collect_all_errors(mut self) -> Self {
        self.collect_all_errors = true;
        self
    }

    /// 获取诊断收集器
    pub fn diagnostics(&self) -> &DiagnosticCollector {
        &self.diagnostics
    }

    /// 检查指定位置是否在__ir块内
    /// 通过向前查找最近的__ir {，并检查是否有匹配的}
    fn is_inside_inline_ir_block(&self, pos: usize) -> bool {
        // 查找最近的__ir {
        let source_before = &self.source[..pos];
        
        // 从后向前查找__ir
        if let Some(ir_pos) = source_before.rfind("__ir") {
            // 检查__ir后面是否有{
            let after_ir = &source_before[ir_pos..];
            if let Some(lbrace_pos) = after_ir.find('{') {
                let lbrace_global_pos = ir_pos + lbrace_pos;
                
                // 计算从{到当前位置的{和}的数量
                let block_content = &self.source[lbrace_global_pos..pos];
                let open_braces = block_content.chars().filter(|&c| c == '{').count();
                let close_braces = block_content.chars().filter(|&c| c == '}').count();
                
                // 如果开放的{数量大于关闭的}数量，说明在块内
                return open_braces > close_braces;
            }
        }
        
        false
    }

    /// 检查当前位置是否在未闭合的字符串中
    fn is_inside_unterminated_string(&self, pos: usize) -> bool {
        let source_before = &self.source[..pos];
        
        // 从后向前查找双引号
        let mut in_string = false;
        let mut escaped = false;
        
        for c in source_before.chars().rev() {
            if escaped {
                escaped = false;
                continue;
            }
            if c == '\\' {
                escaped = true;
                continue;
            }
            if c == '"' {
                in_string = !in_string;
            }
        }
        
        in_string
    }

    /// 创建详细的词法错误诊断
    fn create_lexer_diagnostic(&self, error_type: LexerErrorType, span: std::ops::Range<usize>) -> Diagnostic {
        let error_char = &self.source[span.clone()];
        let location = crate::diagnostic::SourceLocation::new(self.current_source_file.clone(), self.line, self.column);

        match error_type {
            LexerErrorType::InvalidCharacter => {
                Diagnostic::error(
                    ErrorCodes::LEXER_INVALID_CHARACTER,
                    CompilationPhase::Lexer,
                    format!("非法字符: '{}'", error_char),
                    location,
                )
                .with_details(format!(
                    "字符 '{}' 不是Cavvy语言支持的有效字符。Cavvy只支持标准ASCII字符集。",
                    error_char
                ))
                .with_suggestion(FixSuggestion::new("删除该字符或使用支持的字符替换"))
            }
            LexerErrorType::UnterminatedString => {
                Diagnostic::error(
                    ErrorCodes::LEXER_UNTERMINATED_STRING,
                    CompilationPhase::Lexer,
                    "未闭合的字符串字面量",
                    location,
                )
                .with_details("字符串字面量以双引号开始，但没有找到配对的结束双引号。")
                .with_suggestion(FixSuggestion::new("在字符串末尾添加双引号 (\")"))
            }
            LexerErrorType::InvalidEscapeSequence => {
                Diagnostic::error(
                    ErrorCodes::LEXER_INVALID_ESCAPE_SEQUENCE,
                    CompilationPhase::Lexer,
                    format!("无效的转义序列: '{}'", error_char),
                    location,
                )
                .with_details("Cavvy支持以下转义序列: \\n (换行), \\t (制表符), \\\" (双引号), \\\\ (反斜杠), \\' (单引号), \\0 (空字符)")
                .with_suggestion(FixSuggestion::new("使用有效的转义序列替换"))
            }
            LexerErrorType::InvalidNumberLiteral => {
                Diagnostic::error(
                    ErrorCodes::LEXER_INVALID_NUMBER_LITERAL,
                    CompilationPhase::Lexer,
                    format!("无效的数字字面量: '{}'", error_char),
                    location,
                )
                .with_details("数字字面量格式不正确。支持的格式: 十进制(123), 十六进制(0xFF), 二进制(0b101), 八进制(0o77)")
                .with_suggestion(FixSuggestion::new("检查数字格式，确保使用正确的进制前缀"))
            }
            LexerErrorType::UnterminatedComment => {
                Diagnostic::error(
                    ErrorCodes::LEXER_UNTERMINATED_COMMENT,
                    CompilationPhase::Lexer,
                    "未闭合的注释",
                    location,
                )
                .with_details("块注释以 /* 开始，但没有找到配对的结束标记 */。")
                .with_suggestion(FixSuggestion::new("添加 */ 结束注释，或将块注释改为行注释 //"))
            }
            LexerErrorType::InvalidIdentifier => {
                Diagnostic::error(
                    ErrorCodes::LEXER_INVALID_IDENTIFIER,
                    CompilationPhase::Lexer,
                    format!("无效的标识符: '{}'", error_char),
                    location,
                )
                .with_details("标识符必须以字母或下划线开头，后面可以跟字母、数字或下划线。")
                .with_suggestion(FixSuggestion::new("使用有效的标识符名称"))
            }
        }
    }

    pub fn tokenize(&mut self) -> cayResult<Vec<TokenWithLocation>> {
        let mut tokens = Vec::new();
        let mut token_count = 0;

        while let Some(token_result) = self.inner.next() {
            match token_result {
                Ok(token) => {
                    let span = self.inner.span();
                    token_count += 1;
                    
                    // 从字节偏移量计算真实列号（计入被logos跳过的空白字符）
                    let column = compute_column(self.source, span.start);
                    
                    let loc = SourceLocation {
                        file: None,  // 将由source_map填充
                        line: self.line,
                        column,
                    };

                    // 处理多行注释 - 更新行号但不保留token
                    if let Token::BlockComment(Some(newline_count)) = &token {
                        self.line += newline_count;
                        self.column = 1;
                        continue;
                    }

                    // 更新行号和列号
                    if token == Token::Newline {
                        self.line += 1;
                        self.column = 1;
                        // 根据配置决定是否保留换行token
                        if !self.preserve_newlines {
                            continue; // 不保留换行token
                        }
                        // 保留换行token，继续处理
                    } else {
                        self.column += span.end - span.start;
                    }

                    // 检查源映射 - 使用原始源位置
                    // 注意：loc.line 保持为预处理后的行号，让语义分析器来映射
                    // 这样可以避免双重映射问题
                    let (source_file, source_line) = if let Some((file, line)) = self.source_map.get(&self.line) {
                        (Some(file.clone()), Some(*line))
                    } else {
                        (self.current_source_file.clone(), Some(self.line))
                    };

                    // 更新loc中的file和line为原始源文件信息
                    // source_line 来自 source_map（原始行号），回退到预处理行号
                    let loc = SourceLocation {
                        file: source_file.clone(),
                        line: source_line.unwrap_or(self.line),
                        column: loc.column,
                    };

                    tokens.push(TokenWithLocation {
                        token,
                        loc,
                        source_file,
                        source_line,
                    });
                }
                Err(_) => {
                    let span = self.inner.span();
                    let error_char = &self.source[span.clone()];

                    // 检查源映射以获取正确的错误位置
                    let (error_line, error_file) = if let Some((file, line)) = self.source_map.get(&self.line) {
                        (*line, Some(file.clone()))
                    } else {
                        (self.line, self.current_source_file.clone())
                    };

                    // 检查是否在__ir块内（通过检查前面是否有__ir {）
                    let pos = span.start;
                    if self.is_inside_inline_ir_block(pos) {
                        // 在__ir块内，跳过错误字符
                        self.column += span.end - span.start;
                        continue;
                    }

                    // 检查是否是未闭合的字符串
                    // 如果错误字符以"开头，说明是未闭合的字符串
                    let is_unterminated_string = error_char.starts_with('"') || self.is_inside_unterminated_string(pos);

                    if self.collect_all_errors {
                        // 收集错误但继续分析
                        let error_type = if is_unterminated_string {
                            LexerErrorType::UnterminatedString
                        } else {
                            LexerErrorType::InvalidCharacter
                        };
                        let diagnostic = self.create_lexer_diagnostic(
                            error_type,
                            span.clone()
                        );
                        self.diagnostics.add(diagnostic);

                        // 跳过这个字符继续
                        self.column += span.end - span.start;
                    } else {
                        // 未启用错误收集模式时，也尝试收集诊断信息后再继续
                        let error_type = if is_unterminated_string {
                            LexerErrorType::UnterminatedString
                        } else {
                            LexerErrorType::InvalidCharacter
                        };
                        let diagnostic = self.create_lexer_diagnostic(
                            error_type,
                            span.clone()
                        );
                        self.diagnostics.add(diagnostic);
                        self.column += span.end - span.start;
                    }
                }
            }
        }

        // 检查是否有收集到的错误
        if self.diagnostics.has_errors() {
            let diagnostics = self.diagnostics.clone();
            let first = diagnostics.diagnostics().first()
                .map(|d| d.clone())
                .unwrap_or_else(|| {
                    crate::diagnostic::Diagnostic::error(
                        crate::diagnostic::ErrorCodes::LEXER_INVALID_CHARACTER,
                        crate::diagnostic::CompilationPhase::Lexer,
                        format!("词法分析发现 {} 个错误", diagnostics.error_count()),
                        SourceLocation::new(None, self.line, self.column),
                    )
                });
            return Err(crate::error::CompilerError(first).into());
        }

        Ok(tokens)
    }

    /// 获取下一个token（用于迭代器风格）
    pub fn next_token(&mut self) -> Option<cayResult<TokenWithLocation>> {
        match self.inner.next() {
            Some(Ok(token)) => {
                let span = self.inner.span();
                let loc = SourceLocation {
                    file: None,
                    line: self.line,
                    column: self.column,
                };

                // 处理多行注释
                if let Token::BlockComment(Some(newline_count)) = &token {
                    self.line += newline_count;
                    self.column = 1;
                    return self.next_token();
                }

                // 更新行号和列号
                if token == Token::Newline {
                    self.line += 1;
                    self.column = 1;
                    if !self.preserve_newlines {
                        return self.next_token();
                    }
                } else {
                    self.column += span.end - span.start;
                }

                // 检查源映射 - 使用原始源位置
                // 注意：loc.line 保持为预处理后的行号，让语义分析器来映射
                let (source_file, source_line) = if let Some((file, line)) = self.source_map.get(&self.line) {
                    (Some(file.clone()), Some(*line))
                } else {
                    (self.current_source_file.clone(), Some(self.line))
                };

                // 更新loc中的file和line为原始源文件信息
                let loc = SourceLocation {
                    file: source_file.clone(),
                    line: source_line.unwrap_or(self.line),
                    column: loc.column,
                };

                Some(Ok(TokenWithLocation {
                    token,
                    loc,
                    source_file,
                    source_line,
                }))
            }
            Some(Err(_)) => {
                let span = self.inner.span();
                let error_char = &self.source[span.clone()];

                // 检查源映射以获取正确的错误位置
                let (error_line, error_file) = if let Some((file, line)) = self.source_map.get(&self.line) {
                    (*line, Some(file.clone()))
                } else {
                    (self.line, self.current_source_file.clone())
                };

                let error_msg = if let Some(ref file) = error_file {
                    format!("Unexpected character: '{}' in {}:{}", error_char, file, error_line)
                } else {
                    format!("Unexpected character: '{}' at line {}", error_char, error_line)
                };

                let diagnostic = crate::diagnostic::Diagnostic::error(
                    crate::diagnostic::ErrorCodes::LEXER_INVALID_CHARACTER,
                    crate::diagnostic::CompilationPhase::Lexer,
                    error_msg,
                    SourceLocation::new(error_file.clone(), error_line, self.column),
                );
                self.diagnostics.add(diagnostic.clone());
                Some(Err(crate::error::CompilerError(diagnostic).into()))
            }
            None => None,
        }
    }
}

/// 处理字符串中的转义序列
fn process_escape_sequences(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('t') => result.push('\t'),
                Some('r') => result.push('\r'),
                Some('f') => result.push('\x0C'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('\'') => result.push('\''),
                Some('0') => result.push('\0'),
                Some(c) => {
                    // 未知的转义序列，保留原样
                    result.push('\\');
                    result.push(c);
                }
                None => {
                    // 末尾的反斜杠
                    result.push('\\');
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// 处理字符转义序列
fn process_char_escape(s: &str) -> Option<char> {
    if s.starts_with('\\') {
        match s.chars().nth(1) {
            Some('n') => Some('\n'),
            Some('t') => Some('\t'),
            Some('r') => Some('\r'),
            Some('f') => Some('\x0C'),
            Some('\\') => Some('\\'),
            Some('"') => Some('"'),
            Some('\'') => Some('\''),
            Some('0') => Some('\0'),
            _ => None,
        }
    } else {
        s.chars().next()
    }
}

/// 便捷的tokenize函数
pub fn tokenize(source: &str) -> cayResult<Vec<TokenWithLocation>> {
    let mut lexer = Lexer::new(source);
    lexer.tokenize()
}

/// 便捷的tokenize函数（保留换行）
pub fn tokenize_with_newlines(source: &str) -> cayResult<Vec<TokenWithLocation>> {
    let mut lexer = Lexer::with_preserve_newlines(source);
    lexer.tokenize()
}

/// 收集所有词法错误的tokenize函数
pub fn tokenize_collect_errors(source: &str) -> (Vec<TokenWithLocation>, DiagnosticCollector) {
    let mut lexer = Lexer::new(source).with_collect_all_errors();
    match lexer.tokenize() {
        Ok(tokens) => (tokens, lexer.diagnostics().clone()),
        Err(_) => (Vec::new(), lexer.diagnostics().clone()),
    }
}

/// 带诊断的词法分析函数（别名）
pub fn lex_with_diagnostics(source: &str) -> (Vec<TokenWithLocation>, DiagnosticCollector) {
    tokenize_collect_errors(source)
}

/// 检查源字符串是否包含有效的Cavvy代码（无词法错误）
pub fn is_valid_source(source: &str) -> bool {
    tokenize(source).is_ok()
}

/// 从字节偏移量计算列号（计入空白字符）
/// 列号从 1 开始计数
fn compute_column(source: &str, byte_offset: usize) -> usize {
    // 从 byte_offset 往前找最近的一个换行符
    let line_start = source[..byte_offset.min(source.len())]
        .rfind('\n')
        .map(|pos| pos + 1)  // 换行符之后的位置
        .unwrap_or(0);        // 第一行从位置 0 开始
    
    // 列号 = 偏移量 - 行起始位置 + 1（1-indexed）
    byte_offset.saturating_sub(line_start) + 1
}

/// 获取token的显示名称
pub fn token_name(token: &Token) -> &'static str {
    match token {
        Token::Public => "public",
        Token::Private => "private",
        Token::Protected => "protected",
        Token::Static => "static",
        Token::Final => "final",
        Token::Abstract => "abstract",
        Token::Native => "native",
        Token::AtMain => "@main",
        Token::AtOverride => "@Override",
        Token::At => "@",
        Token::Class => "class",
        Token::Void => "void",
        Token::Int => "int",
        Token::Long => "long",
        Token::Float => "float",
        Token::Double => "double",
        Token::Bool => "boolean",
        Token::String => "String",
        Token::Char => "char",
        Token::True => "true",
        Token::False => "false",
        Token::Null => "null",
        Token::If => "if",
        Token::Else => "else",
        Token::While => "while",
        Token::For => "for",
        Token::Do => "do",
        Token::Switch => "switch",
        Token::Case => "case",
        Token::Default => "default",
        Token::Return => "return",
        Token::Break => "break",
        Token::Continue => "continue",
        Token::New => "new",
        Token::This => "this",
        Token::Super => "super",
        Token::Extends => "extends",
        Token::Implements => "implements",
        Token::Interface => "interface",
        Token::InstanceOf => "instanceof",
        Token::Var => "var",
        Token::Let => "let",
        Token::Auto => "auto",
        Token::Extern => "extern",
        Token::Namespace => "namespace",
        Token::Using => "using",
        Token::Scope => "scope",
        Token::InlineIr => "__ir",
        Token::CInt => "c_int",
        Token::CUInt => "c_uint",
        Token::CLong => "c_long",
        Token::CShort => "c_short",
        Token::CUShort => "c_ushort",
        Token::CChar => "c_char",
        Token::CUChar => "c_uchar",
        Token::CFloat => "c_float",
        Token::CDouble => "c_double",
        Token::SizeT => "size_t",
        Token::SSizeT => "ssize_t",
        Token::UIntPtr => "uintptr_t",
        Token::IntPtr => "intptr_t",
        Token::CVoid => "c_void",
        Token::CBool => "c_bool",
        Token::CString => "c_string",
        Token::CInt64 => "c_int64_t",
        Token::CUInt64 => "c_uint64_t",
        Token::Cdecl => "cdecl",
        Token::Stdcall => "stdcall",
        Token::Fastcall => "fastcall",
        Token::Sysv64 => "sysv64",
        Token::Win64 => "win64",
        Token::Alias => "alias",
        Token::Fn => "fn",
        Token::As => "as",
        Token::Identifier(_) => "identifier",
        Token::IntegerLiteral(_) => "integer literal",
        Token::FloatLiteral(_) => "float literal",
        Token::StringLiteral(_) => "string literal",
        Token::CharLiteral(_) => "char literal",
        Token::Plus => "+",
        Token::Minus => "-",
        Token::Star => "*",
        Token::Slash => "/",
        Token::Percent => "%",
        Token::EqEq => "==",
        Token::NotEq => "!=",
        Token::Lt => "<",
        Token::Le => "<=",
        Token::Gt => ">",
        Token::Ge => ">=",
        Token::AndAnd => "&&",
        Token::OrOr => "||",
        Token::Bang => "!",
        Token::Ampersand => "&",
        Token::Pipe => "|",
        Token::Caret => "^",
        Token::Shl => "<<",
        Token::Shr => ">>",
        Token::UnsignedShr => ">>>",
        Token::Tilde => "~",
        Token::Assign => "=",
        Token::AddAssign => "+=",
        Token::SubAssign => "-=",
        Token::MulAssign => "*=",
        Token::DivAssign => "/=",
        Token::ModAssign => "%=",
        Token::Inc => "++",
        Token::Dec => "--",
        Token::LParen => "(",
        Token::RParen => ")",
        Token::LBrace => "{",
        Token::RBrace => "}",
        Token::LBracket => "[",
        Token::RBracket => "]",
        Token::Semicolon => ";",
        Token::Comma => ",",
        Token::Dot => ".",
        Token::DotDotDot => "...",
        Token::Colon => ":",
        Token::DoubleColon => "::",
        Token::Arrow => "->",
        Token::Question => "?",
        Token::Newline => "newline",
        Token::BlockComment(_) => "block comment",
    }
}

/// 检查token是否为关键字
pub fn is_keyword(token: &Token) -> bool {
    matches!(token,
        Token::Public | Token::Private | Token::Protected |
        Token::Static | Token::Final | Token::Abstract | Token::Native |
        Token::Class | Token::Void | Token::Int | Token::Long |
        Token::Float | Token::Double | Token::Bool | Token::String |
        Token::Char | Token::True | Token::False | Token::Null |
        Token::If | Token::Else | Token::While | Token::For |
        Token::Do | Token::Switch | Token::Case | Token::Default |
        Token::Return | Token::Break | Token::Continue |
        Token::New | Token::This | Token::Super |
        Token::Extends | Token::Implements | Token::Interface | Token::InstanceOf |
        Token::Var | Token::Let | Token::Auto | Token::Extern | Token::Scope |
        Token::InlineIr | Token::Alias | Token::Fn | Token::As
    )
}

/// 获取关键字的优先级（用于错误恢复建议）
pub fn keyword_priority(token: &Token) -> u8 {
    match token {
        Token::If | Token::Else | Token::While | Token::For | Token::Return => 10,
        Token::Class | Token::Interface | Token::Extends | Token::Implements => 9,
        Token::Public | Token::Private | Token::Protected | Token::Static | Token::Final => 8,
        Token::Int | Token::Long | Token::Float | Token::Double | Token::Bool | Token::String | Token::Void => 7,
        Token::New | Token::This | Token::Super => 6,
        Token::True | Token::False | Token::Null => 5,
        _ => 0,
    }
}

/// 便捷的词法分析函数（别名）
pub fn lex(source: &str) -> cayResult<Vec<TokenWithLocation>> {
    tokenize(source)
}

/// 带源映射的词法分析函数
pub fn lex_with_source_map(source: &str, source_map: std::collections::HashMap<usize, (String, usize)>) -> cayResult<Vec<TokenWithLocation>> {
    lex_with_source_map_and_file(source, source_map, None)
}

/// 词法分析（带源映射和当前文件路径）
pub fn lex_with_source_map_and_file(source: &str, source_map: std::collections::HashMap<usize, (String, usize)>, current_file: Option<String>) -> cayResult<Vec<TokenWithLocation>> {
    let mut lexer = Lexer::with_source_map_and_file(source, source_map, current_file);
    lexer.tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_tokens() {
        let source = r#"int x = 42;"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 5);
        assert!(matches!(tokens[0].token, Token::Int));
        assert!(matches!(tokens[1].token, Token::Identifier(_)));
        assert!(matches!(tokens[2].token, Token::Assign));
        assert!(matches!(tokens[3].token, Token::IntegerLiteral(_)));
        assert!(matches!(tokens[4].token, Token::Semicolon));
    }

    #[test]
    fn test_line_comment() {
        let source = r#"int x = 42; // this is a comment"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 5);
    }

    #[test]
    fn test_block_comment() {
        let source = r#"int /* comment */ x = 42;"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 5);
    }

    #[test]
    fn test_multiline_comment() {
        let source = r#"int /* 
        multi-line 
        comment */ x = 42;"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 5);
    }

    #[test]
    fn test_string_literal() {
        let source = r#"String s = "hello";"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 5);
        if let Token::StringLiteral(Some(s)) = &tokens[3].token {
            assert_eq!(s, "hello");
        } else {
            panic!("Expected string literal");
        }
    }

    #[test]
    fn test_escape_sequences() {
        let source = r#""hello\nworld\t!""#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 1);
        if let Token::StringLiteral(Some(s)) = &tokens[0].token {
            assert_eq!(s, "hello\nworld\t!");
        } else {
            panic!("Expected string literal with escapes");
        }
    }

    #[test]
    fn test_string_escape_sequences_all() {
        // 测试所有支持的字符串转义序列
        let source = r#""\n\t\r\f\\\"\'\0""#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 1);
        if let Token::StringLiteral(Some(s)) = &tokens[0].token {
            assert_eq!(s, "\n\t\r\x0C\\\"'\0");
        } else {
            panic!("Expected string literal with all escapes");
        }
    }

    #[test]
    fn test_char_literal() {
        let source = r#"'a'"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 1);
        if let Token::CharLiteral(Some(c)) = &tokens[0].token {
            assert_eq!(*c, 'a');
        } else {
            panic!("Expected char literal");
        }
    }

    #[test]
    fn test_char_literal_with_escapes() {
        // 测试所有支持的字符转义序列
        let test_cases = vec![
            (r#"'\n'"#, '\n'),
            (r#"'\t'"#, '\t'),
            (r#"'\r'"#, '\r'),
            (r#"'\f'"#, '\x0C'),  // 换页符
            (r#"'\\'"#, '\\'),
            (r#"'\"'"#, '"'),
            (r#"'\''"#, '\''),
            (r#"'\0'"#, '\0'),
        ];

        for (source, expected) in test_cases {
            let tokens = tokenize(source).unwrap();
            assert_eq!(tokens.len(), 1, "Failed for source: {}", source);
            if let Token::CharLiteral(Some(c)) = &tokens[0].token {
                assert_eq!(*c, expected, "Failed for source: {}", source);
            } else {
                panic!("Expected char literal for source: {}", source);
            }
        }
    }

    #[test]
    fn test_operators() {
        let source = r#"+ - * / % == != < <= > >= && ||"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 13);  // 13个操作符token（空格被跳过）
    }

    #[test]
    fn test_keywords() {
        let source = r#"public static void main"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 4);
        assert!(matches!(tokens[0].token, Token::Public));
        assert!(matches!(tokens[1].token, Token::Static));
        assert!(matches!(tokens[2].token, Token::Void));
        assert!(matches!(tokens[3].token, Token::Identifier(_)));
    }

    #[test]
    fn test_invalid_character() {
        let source = "int x = 42 #;";
        let result = tokenize(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_unterminated_string() {
        let source = r#"String s = "hello;"#;
        let result = tokenize(source);
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_number() {
        let source = r#"0xFF 0X1a 0xDEADBEEF"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 3);
    }

    #[test]
    fn test_binary_number() {
        let source = r#"0b1010 0B1111"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_octal_number() {
        let source = r#"0o777 0O123"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 2);
    }

    #[test]
    fn test_ffi_types() {
        let source = r#"c_int c_float size_t c_void"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 4);
        assert!(matches!(tokens[0].token, Token::CInt));
        assert!(matches!(tokens[1].token, Token::CFloat));
        assert!(matches!(tokens[2].token, Token::SizeT));
        assert!(matches!(tokens[3].token, Token::CVoid));
    }

    #[test]
    fn test_calling_conventions() {
        let source = r#"cdecl stdcall fastcall"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(matches!(tokens[0].token, Token::Cdecl));
        assert!(matches!(tokens[1].token, Token::Stdcall));
        assert!(matches!(tokens[2].token, Token::Fastcall));
    }

    #[test]
    fn test_annotations() {
        let source = r#"@main @Override"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 2);
        assert!(matches!(tokens[0].token, Token::AtMain));
        assert!(matches!(tokens[1].token, Token::AtOverride));
    }

    #[test]
    fn test_inline_ir_token() {
        let source = r#"__ir"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 1);
        assert!(matches!(tokens[0].token, Token::InlineIr));
    }

    #[test]
    fn test_newline_tracking() {
        let source = "line1\nline2\nline3";
        let tokens = tokenize_with_newlines(source).unwrap();
        // Should have 3 identifiers and 2 newlines
        assert!(tokens.iter().any(|t| matches!(t.token, Token::Newline)));
    }

    #[test]
    fn test_source_location() {
        let source = r#"int
x"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens[0].loc.line, 1);  // int
        assert_eq!(tokens[1].loc.line, 2);  // x
    }

    #[test]
    fn test_underscore_in_number() {
        let source = r#"1_000_000 0xFF_FF 0b1010_1010"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 3);
        
        if let Token::IntegerLiteral(Some((val, _))) = &tokens[0].token {
            assert_eq!(*val, 1000000);
        } else {
            panic!("Expected integer literal");
        }
    }

    #[test]
    fn test_chinese_comment() {
        let source = "// 这是中文注释\nint x;";
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 3);
        assert!(matches!(tokens[0].token, Token::Int));
        assert!(matches!(tokens[1].token, Token::Identifier(_)));
        assert!(matches!(tokens[2].token, Token::Semicolon));
    }

    #[test]
    fn test_chinese_in_inline_ir_comment() {
        let source = r#"__ir {
            ; 这是IR注释
            %x = add i32 1, 2
        }"#;
        let tokens = tokenize_with_newlines(source).unwrap();
        // 应该能成功tokenize，不会报错
        assert!(tokens.len() > 0);
    }

    #[test]
    fn test_scientific_notation() {
        // 测试科学计数法
        let source = r#"1e10 1e-10 1e+10 1.5e10 1.5e-10 2E5 3.14e0"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 7, "Should have 7 float literals");
        
        // 验证所有都是浮点数字面量
        for token in &tokens {
            assert!(matches!(token.token, Token::FloatLiteral(_)), 
                "Expected FloatLiteral, got {:?}", token.token);
        }
        
        // 验证具体值
        if let Token::FloatLiteral(Some((val, _))) = &tokens[0].token {
            assert!((*val - 1e10).abs() < 1e-5, "1e10 should be 10000000000, got {}", val);
        }
        if let Token::FloatLiteral(Some((val, _))) = &tokens[1].token {
            assert!((*val - 1e-10).abs() < 1e-15, "1e-10 should be 0.0000000001, got {}", val);
        }
        if let Token::FloatLiteral(Some((val, _))) = &tokens[3].token {
            assert!((*val - 1.5e10).abs() < 1e-5, "1.5e10 should be 15000000000, got {}", val);
        }
    }

    #[test]
    fn test_scientific_notation_with_suffix() {
        // 测试带后缀的科学计数法
        let source = r#"1e10f 1.5e-10d 2E5F"#;
        let tokens = tokenize(source).unwrap();
        assert_eq!(tokens.len(), 3, "Should have 3 float literals");
        
        // 验证后缀
        if let Token::FloatLiteral(Some((_, suffix))) = &tokens[0].token {
            assert_eq!(*suffix, Some('f'), "Should have 'f' suffix");
        }
        if let Token::FloatLiteral(Some((_, suffix))) = &tokens[1].token {
            assert_eq!(*suffix, Some('d'), "Should have 'd' suffix");
        }
    }
}
