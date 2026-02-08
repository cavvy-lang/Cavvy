use logos::Logos;
use crate::error::{EolResult, lexer_error};
use crate::error::SourceLocation;

#[derive(Logos, Debug, Clone, PartialEq)]
#[logos(skip r"[ \t\f]+")]
#[logos(skip r"//[^\n]*")]
#[logos(skip r"/\*([^*]|\*[^/])*\*\*/")]
pub enum Token {
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
    
    // 标识符
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Identifier(String),
    
    // 字面量
    #[regex(r"-?(?:0[xX][0-9a-fA-F][0-9a-fA-F_]*|0[bB][01][01_]*|0[oO]?[0-7][0-7_]*|[0-9][0-9_]*)[Ll]?", |lex| {
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
            // 以0开头但不含字母的十进制数字？实际上，前导零的十进制数字，但我们将视为十进制（如Java中，前导零表示八进制？在Java中，前导零表示八进制，但为了兼容性，我们将其视为八进制？我们已匹配八进制模式，所以这里应该是十进制）
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
    
    #[regex(r"-?(?:[0-9][0-9_]*\.[0-9][0-9_]*|\.[0-9][0-9_]*|[0-9][0-9_]*\.)(?:[eE][+-]?[0-9][0-9_]*)?[FfDd]?", |lex| {
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
    
    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        s[1..s.len()-1].to_string()
    })]
    StringLiteral(String),
    
    #[regex(r"'([^'\\]|\\.)'", |lex| {
        let s = lex.slice();
        s.chars().nth(1)
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
    
    // 换行（用于跟踪行号）- 支持 Windows \r\n 和 Unix \n
    #[regex(r"\r?\n")]
    Newline,
}

#[derive(Debug, Clone)]
pub struct TokenWithLocation {
    pub token: Token,
    pub loc: SourceLocation,
}

pub struct Lexer<'a> {
    source: &'a str,
    inner: logos::Lexer<'a, Token>,
    line: usize,
    column: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            inner: Token::lexer(source),
            line: 1,
            column: 1,
        }
    }

    pub fn tokenize(&mut self) -> EolResult<Vec<TokenWithLocation>> {
        let mut tokens = Vec::new();
        
        while let Some(token_result) = self.inner.next() {
            match token_result {
                Ok(token) => {
                    let span = self.inner.span();
                    let loc = SourceLocation {
                        line: self.line,
                        column: self.column,
                    };
                    
                    // 更新行号和列号
                    if token == Token::Newline {
                        self.line += 1;
                        self.column = 1;
                        continue; // 不保留换行token
                    } else {
                        self.column += span.end - span.start;
                    }
                    
                    tokens.push(TokenWithLocation { token, loc });
                }
                Err(_) => {
                    let span = self.inner.span();
                    let error_char = &self.source[span.clone()];
                    return Err(lexer_error(
                        self.line,
                        self.column,
                        format!("Unexpected character: '{}'", error_char)
                    ));
                }
            }
        }
        
        // 添加EOF标记 - 使用Identifier作为哨兵值
        tokens.push(TokenWithLocation {
            token: Token::Identifier(String::new()), // 用作EOF标记
            loc: SourceLocation {
                line: self.line,
                column: self.column,
            },
        });
        
        Ok(tokens)
    }
}

pub fn lex(source: &str) -> EolResult<Vec<TokenWithLocation>> {
    let mut lexer = Lexer::new(source);
    lexer.tokenize()
}
