//! cay 语法分析器
//!
//! 本模块将词法分析器生成的令牌流解析为抽象语法树 (AST)。
//! 已重构为多个子模块以提高可维护性。

mod classes;
mod types;
mod statements;
mod expressions;
mod utils;

use crate::lexer::TokenWithLocation;
use crate::ast::Program;
use crate::error::cayResult;
use crate::diagnostic::DiagnosticCollector;

/// 语法分析器
pub struct Parser {
    /// 令牌流
    pub tokens: Vec<TokenWithLocation>,
    /// 当前解析位置
    pub pos: usize,
    /// 诊断收集器
    pub diagnostics: DiagnosticCollector,
    /// 源代码文本（用于内联IR等需要直接访问源码的场景）
    source: Option<String>,
    /// 类型别名映射: 别名名称 -> 目标类型
    type_aliases: std::collections::HashMap<String, crate::types::Type>,
}

impl Parser {
    /// 创建新的语法分析器
    pub fn new(tokens: Vec<TokenWithLocation>) -> Self {
        Self { 
            tokens, 
            pos: 0,
            diagnostics: DiagnosticCollector::new(),
            source: None,
            type_aliases: std::collections::HashMap::new(),
        }
    }

    /// 创建带源代码的语法分析器（用于内联IR解析）
    pub fn with_source(tokens: Vec<TokenWithLocation>, source: String) -> Self {
        Self { 
            tokens, 
            pos: 0,
            diagnostics: DiagnosticCollector::new(),
            source: Some(source),
            type_aliases: std::collections::HashMap::new(),
        }
    }

    /// 获取源代码
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// 获取诊断收集器
    pub fn diagnostics(&self) -> &DiagnosticCollector {
        &self.diagnostics
    }

    /// 解析整个程序
    pub fn parse(&mut self) -> cayResult<Program> {
        let mut classes = Vec::new();
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        let mut interfaces = Vec::new();
        let mut top_level_functions = Vec::new();
        let mut extern_declarations = Vec::new();
        let mut type_aliases = Vec::new();
        let mut namespace_path: Option<Vec<String>> = None;
        let mut namespace_decls = Vec::new();
        let mut using_decls = Vec::new();

        // 检查文件级 namespace 声明 (必须是文件第一个非注释/空行的语句)
        // 需要 lookahead 区分: namespace path; (文件级) vs namespace path { (块级)
        if self.check(&crate::lexer::Token::Namespace) {
            let saved_pos = self.pos;
            self.advance(); // 消费 namespace
            // 尝试解析路径
            if let Ok(_path) = self.parse_namespace_path() {
                // 检查路径后面是分号（文件级）还是花括号（块级）
                if self.check(&crate::lexer::Token::Semicolon) {
                    // 文件级: namespace path;
                    self.pos = saved_pos; // 回退
                    namespace_path = Some(self.parse_namespace_file_level()?);
                } else {
                    // 块级: namespace path { - 回退，由 while 循环处理
                    self.pos = saved_pos;
                }
            } else {
                // 解析失败，回退
                self.pos = saved_pos;
            }
        }

        while !self.is_at_end() {
            if self.check(&crate::lexer::Token::Namespace) {
                // 块级 namespace 声明
                namespace_decls.push(self.parse_namespace_block()?);
            } else if self.check(&crate::lexer::Token::Using) {
                using_decls.push(self.parse_using_declaration()?);
            } else if self.check(&crate::lexer::Token::Interface)
                || (self.check(&crate::lexer::Token::Public) && self.check_next(&crate::lexer::Token::Interface))
            {
                interfaces.push(self.parse_interface()?);
            } else if self.check(&crate::lexer::Token::Struct)
                || (self.check(&crate::lexer::Token::Public) && self.check_next(&crate::lexer::Token::Struct))
                || (self.check(&crate::lexer::Token::Private) && self.check_next(&crate::lexer::Token::Struct))
                || (self.check(&crate::lexer::Token::Protected) && self.check_next(&crate::lexer::Token::Struct))
            {
                structs.push(self.parse_struct()?);
            } else if self.check(&crate::lexer::Token::Enum)
                || (self.check(&crate::lexer::Token::Public) && self.check_next(&crate::lexer::Token::Enum))
                || (self.check(&crate::lexer::Token::Private) && self.check_next(&crate::lexer::Token::Enum))
                || (self.check(&crate::lexer::Token::Protected) && self.check_next(&crate::lexer::Token::Enum))
            {
                enums.push(self.parse_enum()?);
            } else if self.check(&crate::lexer::Token::Class)
                || self.check(&crate::lexer::Token::Private)
                || self.check(&crate::lexer::Token::Protected)
                || self.check(&crate::lexer::Token::AtMain)
            {
                classes.push(self.parse_class()?);
            } else if self.check(&crate::lexer::Token::Public) {
                // 检查是否是顶层函数: public 返回类型 函数名()
                if self.check_top_level_function() {
                    top_level_functions.push(self.parse_top_level_function()?);
                } else {
                    // 否则可能是 public class
                    classes.push(self.parse_class()?);
                }
            } else if self.check_top_level_function_return_type() {
                // 没有 public 修饰符的顶层函数
                top_level_functions.push(self.parse_top_level_function_without_public()?);
            } else if self.check(&crate::lexer::Token::Extern) {
                extern_declarations.push(self.parse_extern_declaration()?);
            } else if self.check(&crate::lexer::Token::Alias) {
                type_aliases.push(self.parse_type_alias()?);
            } else {
                let current_token = utils::current_token(self);
                let (token_desc, suggestion) = match current_token {
                    crate::lexer::Token::Semicolon => (
                        "分号(;)".to_string(),
                        "顶层声明不能是空语句。可能的问题:\n    - 多余的逗号或分号\n    - 缺少声明内容".to_string()
                    ),
                    crate::lexer::Token::LBrace => (
                        "左花括号({)".to_string(),
                        "顶层声明不能以代码块开始。可能的问题:\n    - 缺少类或函数声明\n    - 代码块应在函数或方法体内".to_string()
                    ),
                    crate::lexer::Token::RBrace => (
                        "右花括号(})".to_string(),
                        "文件提前结束或多余的右花括号。可能的问题:\n    - 前面的声明缺少匹配的左花括号\n    - 多余的右花括号".to_string()
                    ),
                    crate::lexer::Token::LParen => (
                        "左圆括号(()".to_string(),
                        "顶层声明不能以括号开始。可能的问题:\n    - 缺少函数声明\n    - Lambda 表达式不能作为顶层声明".to_string()
                    ),
                    crate::lexer::Token::If | crate::lexer::Token::While |
                    crate::lexer::Token::For | crate::lexer::Token::Do |
                    crate::lexer::Token::Switch | crate::lexer::Token::Return |
                    crate::lexer::Token::Break | crate::lexer::Token::Continue => {
                        let kw = format!("{:?}", current_token).to_lowercase();
                        (
                            format!("关键字({})", kw),
                            format!("{} 是控制流语句，不能作为顶层声明。可能的问题:\n    - 控制流语句只能在函数或方法体内使用\n    - 缺少函数声明", kw)
                        )
                    }
                    crate::lexer::Token::Int | crate::lexer::Token::Long |
                    crate::lexer::Token::Float | crate::lexer::Token::Double |
                    crate::lexer::Token::Bool | crate::lexer::Token::Char |
                    crate::lexer::Token::String => {
                        let kw = format!("{:?}", current_token).to_lowercase();
                        (
                            format!("关键字({})", kw),
                            format!("类型 '{}' 不能单独作为顶层声明。可能的问题:\n    - 缺少变量或函数声明，如: {} x; 或 {} main() {{ ... }}\n    - 类型后缺少标识符", kw, kw, kw)
                        )
                    }
                    crate::lexer::Token::Identifier(name) => (
                        format!("标识符('{}')", name),
                        format!("'{}' 不能作为顶层声明开始。可能的问题:\n    - 需要先声明类或函数\n    - 语句位置错误，应在函数体内\n    - 如果是方法调用，需要在函数或 main 函数中执行", name)
                    ),
                    crate::lexer::Token::IntegerLiteral(Some((val, _))) => (
                        format!("整数({})", val),
                        "整数字面量不能作为顶层声明。可能的问题:\n    - 缺少变量声明，如: int x = 10;\n    - 语句位置错误，应在函数体内".to_string()
                    ),
                    crate::lexer::Token::StringLiteral(Some(s)) => (
                        format!("字符串(\"{}\")", s),
                        "字符串字面量不能作为顶层声明。可能的问题:\n    - 缺少变量声明，如: String s = \"hello\";\n    - 语句位置错误，应在函数体内".to_string()
                    ),
                    crate::lexer::Token::Private | crate::lexer::Token::Protected |
                    crate::lexer::Token::Static | crate::lexer::Token::Final |
                    crate::lexer::Token::Abstract => {
                        let kw = format!("{:?}", current_token).to_lowercase();
                        (
                            format!("关键字({})", kw),
                            format!("修饰符 '{}' 不能单独作为顶层声明。可能的问题:\n    - 修饰符后缺少类或函数声明\n    - 顶层声明应以 class、interface 或 public 开始", kw)
                        )
                    }
                    _ => {
                        let token_name = utils::get_token_name(current_token);
                        (
                            token_name.clone(),
                            format!("{} 不能作为顶层声明。有效的顶层声明包括:\n    - 类: class MyClass {{ ... }}\n    - 接口: interface MyInterface {{ ... }}\n    - 外部函数: extern {{ ... }}\n    - 类型别名: type MyType = int;\n    - 主函数: public int main() {{ ... }}", token_name)
                        )
                    }
                };
                return Err(self.error(&format!(
                    "期望类、接口、extern 声明或顶层函数声明，但遇到了 {}\n提示: {}",
                    token_desc, suggestion
                )));
            }
        }

        Ok(Program { classes, structs, enums, interfaces, top_level_functions, extern_declarations, type_aliases, namespace_path, namespace_decls, using_decls })
    }

    // 类解析方法
    fn parse_class(&mut self) -> cayResult<crate::ast::ClassDecl> {
        classes::parse_class(self)
    }

    fn parse_struct(&mut self) -> cayResult<crate::ast::StructDecl> {
        classes::parse_struct(self)
    }

    fn parse_enum(&mut self) -> cayResult<crate::ast::EnumDecl> {
        classes::parse_enum(self)
    }

    fn parse_interface(&mut self) -> cayResult<crate::ast::InterfaceDecl> {
        classes::parse_interface(self)
    }

    /// 解析泛型类型参数 <T, U, ...>
    fn parse_generic_type_params(&mut self) -> cayResult<Vec<String>> {
        classes::parse_generic_type_params(self)
    }

    fn parse_class_member(&mut self) -> cayResult<crate::ast::ClassMember> {
        classes::parse_class_member(self)
    }

    fn parse_field(&mut self) -> cayResult<crate::ast::FieldDecl> {
        classes::parse_field(self)
    }

    fn parse_method(&mut self) -> cayResult<crate::ast::MethodDecl> {
        classes::parse_method(self)
    }

    fn parse_modifiers(&mut self) -> cayResult<Vec<crate::ast::Modifier>> {
        classes::parse_modifiers(self)
    }

    fn parse_parameters(&mut self) -> cayResult<Vec<crate::types::ParameterInfo>> {
        classes::parse_parameters(self)
    }
    
    // 类型解析方法
    fn parse_type(&mut self) -> cayResult<crate::types::Type> {
        types::parse_type(self)
    }
    
    fn is_type_token(&self) -> bool {
        types::is_type_token(self)
    }
    
    fn is_primitive_type_token(&self) -> bool {
        types::is_primitive_type_token(self)
    }
    
    // 语句解析方法
    fn parse_block(&mut self) -> cayResult<crate::ast::Block> {
        statements::parse_block(self)
    }
    
    fn parse_statement(&mut self) -> cayResult<crate::ast::Stmt> {
        statements::parse_statement(self)
    }
    
    fn parse_var_decl(&mut self) -> cayResult<crate::ast::Stmt> {
        statements::parse_var_decl(self)
    }
    
    fn parse_if_statement(&mut self) -> cayResult<crate::ast::Stmt> {
        statements::parse_if_statement(self)
    }
    
    fn parse_while_statement(&mut self) -> cayResult<crate::ast::Stmt> {
        statements::parse_while_statement(self)
    }
    
    fn parse_for_statement(&mut self) -> cayResult<crate::ast::Stmt> {
        statements::parse_for_statement(self)
    }
    
    fn parse_do_while_statement(&mut self) -> cayResult<crate::ast::Stmt> {
        statements::parse_do_while_statement(self)
    }
    
    fn parse_switch_statement(&mut self) -> cayResult<crate::ast::Stmt> {
        statements::parse_switch_statement(self)
    }
    
    fn parse_return_statement(&mut self) -> cayResult<crate::ast::Stmt> {
        statements::parse_return_statement(self)
    }
    
    fn parse_expression_statement(&mut self) -> cayResult<crate::ast::Stmt> {
        statements::parse_expression_statement(self)
    }
    
    // 表达式解析方法
    fn parse_expression(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_expression(self)
    }
    
    fn parse_assignment(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_assignment(self)
    }
    
    fn parse_or(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_or(self)
    }
    
    fn parse_and(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_and(self)
    }
    
    fn parse_bitwise_or(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_bitwise_or(self)
    }
    
    fn parse_bitwise_xor(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_bitwise_xor(self)
    }
    
    fn parse_bitwise_and(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_bitwise_and(self)
    }
    
    fn parse_equality(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_equality(self)
    }
    
    fn parse_comparison(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_comparison(self)
    }
    
    fn parse_shift(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_shift(self)
    }
    
    fn parse_term(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_term(self)
    }
    
    fn parse_factor(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_factor(self)
    }
    
    fn parse_unary(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_unary(self)
    }
    
    fn parse_postfix(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_postfix(self)
    }
    
    fn parse_primary(&mut self) -> cayResult<crate::ast::Expr> {
        expressions::parse_primary(self)
    }
    
    fn parse_arguments(&mut self) -> cayResult<Vec<crate::ast::Expr>> {
        expressions::parse_arguments(self)
    }
    
    fn match_assignment_op(&mut self) -> Option<crate::ast::AssignOp> {
        expressions::match_assignment_op(self)
    }
    
    // 辅助方法
    fn is_at_end(&self) -> bool {
        utils::is_at_end(self)
    }
    
    fn current_token(&self) -> &crate::lexer::Token {
        utils::current_token(self)
    }
    
    fn current_loc(&self) -> crate::error::SourceLocation {
        utils::current_loc(self)
    }
    
    fn previous_loc(&self) -> crate::error::SourceLocation {
        utils::previous_loc(self)
    }
    
    fn advance(&mut self) -> &crate::lexer::Token {
        utils::advance(self)
    }
    
    fn check(&self, token: &crate::lexer::Token) -> bool {
        utils::check(self, token)
    }

    fn check_next(&self, token: &crate::lexer::Token) -> bool {
        utils::check_next(self, token)
    }

    fn match_token(&mut self, token: &crate::lexer::Token) -> bool {
        utils::match_token(self, token)
    }
    
    fn consume(&mut self, token: &crate::lexer::Token, message: &str) -> cayResult<&crate::lexer::Token> {
        utils::consume(self, token, message)
    }
    
    fn consume_identifier(&mut self, message: &str) -> cayResult<String> {
        utils::consume_identifier(self, message)
    }
    
    fn error(&self, message: &str) -> crate::error::cayError {
        utils::error(self, message)
    }

    /// 检查是否是顶层函数（public 返回类型 函数名()）
    fn check_top_level_function(&self) -> bool {
        // 需要 lookahead: public 返回类型 函数名(
        let mut pos = self.pos;
        // 跳过 public
        if pos >= self.tokens.len() {
            return false;
        }
        pos += 1;

        // 检查是否是返回类型
        if pos >= self.tokens.len() {
            return false;
        }
        match &self.tokens[pos].token {
            crate::lexer::Token::Int | crate::lexer::Token::Void |
            crate::lexer::Token::Long | crate::lexer::Token::Float |
            crate::lexer::Token::Double | crate::lexer::Token::Bool |
            crate::lexer::Token::Char | crate::lexer::Token::String |
            crate::lexer::Token::Identifier(_) => {}
            _ => return false,
        }
        pos += 1;

        // 检查是否是函数名（标识符后跟左括号）
        if pos >= self.tokens.len() {
            return false;
        }
        match &self.tokens[pos].token {
            crate::lexer::Token::Identifier(_) => {}
            _ => return false,
        }
        pos += 1;

        // 检查后面是否是左括号
        if pos >= self.tokens.len() {
            return false;
        }
        matches!(&self.tokens[pos].token, crate::lexer::Token::LParen)
    }

    /// 检查当前位置是否是一个顶层函数的返回类型（用于没有 public 修饰符的情况）
    fn check_top_level_function_return_type(&self) -> bool {
        // 需要 lookahead: 返回类型 函数名(
        let mut pos = self.pos;

        // 检查是否是返回类型
        if pos >= self.tokens.len() {
            return false;
        }
        match &self.tokens[pos].token {
            crate::lexer::Token::Int | crate::lexer::Token::Void |
            crate::lexer::Token::Long | crate::lexer::Token::Float |
            crate::lexer::Token::Double | crate::lexer::Token::Bool |
            crate::lexer::Token::Char | crate::lexer::Token::String => {}
            _ => return false,
        }
        pos += 1;

        // 检查是否是函数名（标识符后跟左括号）
        if pos >= self.tokens.len() {
            return false;
        }
        match &self.tokens[pos].token {
            crate::lexer::Token::Identifier(_) => {}
            _ => return false,
        }
        pos += 1;

        // 检查后面是否是左括号
        if pos >= self.tokens.len() {
            return false;
        }
        matches!(&self.tokens[pos].token, crate::lexer::Token::LParen)
    }

    /// 解析顶层函数（带 public 修饰符）
    fn parse_top_level_function(&mut self) -> cayResult<crate::ast::TopLevelFunction> {
        let loc = self.current_loc();

        // 必须是以 public 开始
        self.consume(&crate::lexer::Token::Public, "期望 'public'\n提示: 顶层函数应以 public 开头，例如: public int main() { ... }")?;

        self.parse_top_level_function_body(loc, vec![crate::ast::Modifier::Public])
    }

    /// 解析顶层函数（不带 public 修饰符）
    fn parse_top_level_function_without_public(&mut self) -> cayResult<crate::ast::TopLevelFunction> {
        let loc = self.current_loc();
        self.parse_top_level_function_body(loc, vec![])
    }

    /// 解析顶层函数的主体部分
    fn parse_top_level_function_body(&mut self, loc: crate::error::SourceLocation, modifiers: Vec<crate::ast::Modifier>) -> cayResult<crate::ast::TopLevelFunction> {
        // 解析返回类型
        let return_type = self.parse_type()?;

        // 解析函数名
        let name = self.consume_identifier("期望函数名\n提示: 返回类型后应跟函数名，例如: int add(int a, int b) { ... }")?;

        // 解析参数列表
        self.consume(&crate::lexer::Token::LParen, "期望 '('\n提示: 函数名后应跟 '(' 开始参数列表，例如: add(int a, int b)")?;
        let params = self.parse_parameters()?;
        self.consume(&crate::lexer::Token::RParen, "期望 ')'\n提示: 参数列表应以 ')' 结束")?;

        // 解析函数体
        let body = self.parse_block()?;

        Ok(crate::ast::TopLevelFunction {
            name,
            modifiers,
            return_type,
            params,
            body,
            namespace_path: Vec::new(),
            loc,
        })
    }

    /// 解析 extern 声明
    fn parse_extern_declaration(&mut self) -> cayResult<crate::ast::ExternDecl> {
        let loc = self.current_loc();

        // 消费 extern 关键字
        self.consume(&crate::lexer::Token::Extern, "期望 'extern'\n提示: 外部函数声明应以 extern 开头，例如: extern { ... }")?;

        // 解析调用约定（可选）
        let calling_convention = self.parse_calling_convention()?;

        // 解析函数声明列表
        let mut functions = Vec::new();

        // 支持两种语法:
        // 1. extern "C" { type func(params); ... }
        // 2. extern type func(params);

        if self.check(&crate::lexer::Token::StringLiteral(None)) ||
           matches!(self.current_token(), crate::lexer::Token::StringLiteral(Some(_))) {
            // 字符串字面量指定调用约定，如 extern "C" { ... }
            self.advance(); // 消费字符串字面量
            self.consume(&crate::lexer::Token::LBrace, "期望 '{'\n提示: 调用约定后应跟 '{' 开始外部函数块，例如: extern \"C\" { ... }")?;

            while !self.check(&crate::lexer::Token::RBrace) && !self.is_at_end() {
                functions.push(self.parse_extern_function()?);
            }

            self.consume(&crate::lexer::Token::RBrace, "期望 '}'\n提示: 外部函数块应以 '}' 结束")?;
        } else if self.check(&crate::lexer::Token::LBrace) {
            // extern { ... } - 默认 C 调用约定
            self.advance(); // 消费 {

            while !self.check(&crate::lexer::Token::RBrace) && !self.is_at_end() {
                functions.push(self.parse_extern_function()?);
            }

            self.consume(&crate::lexer::Token::RBrace, "期望 '}'\n提示: 外部函数块应以 '}' 结束")?;
        } else {
            // 单个函数声明: extern type func(params);
            functions.push(self.parse_extern_function()?);
        }

        Ok(crate::ast::ExternDecl {
            calling_convention,
            functions,
            namespace_path: Vec::new(),
            loc,
        })
    }

    /// 解析调用约定
    fn parse_calling_convention(&mut self) -> cayResult<crate::ast::CallingConvention> {
        match self.current_token() {
            crate::lexer::Token::Cdecl => {
                self.advance();
                Ok(crate::ast::CallingConvention::Cdecl)
            }
            crate::lexer::Token::Stdcall => {
                self.advance();
                Ok(crate::ast::CallingConvention::Stdcall)
            }
            crate::lexer::Token::Fastcall => {
                self.advance();
                Ok(crate::ast::CallingConvention::Fastcall)
            }
            crate::lexer::Token::Sysv64 => {
                self.advance();
                Ok(crate::ast::CallingConvention::Sysv64)
            }
            crate::lexer::Token::Win64 => {
                self.advance();
                Ok(crate::ast::CallingConvention::Win64)
            }
            _ => Ok(crate::ast::CallingConvention::Cdecl), // 默认 C 调用约定
        }
    }

    /// 解析单个外部函数声明
    /// 支持语法: type func(params); 或 type func(params) as alias;
    fn parse_extern_function(&mut self) -> cayResult<crate::ast::ExternFunction> {
        let loc = self.current_loc();

        // 解析返回类型
        let return_type = self.parse_type()?;

        // 解析函数名（外部C函数名）
        let name = self.consume_identifier("Expected function name in extern declaration")?;

        // 解析参数列表
        self.consume(&crate::lexer::Token::LParen, "Expected '(' after extern function name")?;
        let params = self.parse_extern_parameters()?;
        self.consume(&crate::lexer::Token::RParen, "Expected ')' after extern function parameters")?;

        // 解析可选的别名: as alias_name
        let alias = if self.check(&crate::lexer::Token::As) {
            self.advance(); // 消费 'as'
            Some(self.consume_identifier("Expected alias name after 'as' in extern declaration")?)
        } else {
            None
        };

        // 消费分号
        self.consume(&crate::lexer::Token::Semicolon, "Expected ';' after extern function declaration")?;

        Ok(crate::ast::ExternFunction {
            name,
            alias,
            return_type,
            params,
            loc,
        })
    }

    /// 解析extern函数参数列表（支持可选参数名，兼容C风格声明）
    fn parse_extern_parameters(&mut self) -> cayResult<Vec<crate::types::ParameterInfo>> {
        use crate::lexer::Token;
        let mut params = Vec::new();
        let mut param_index = 0;

        if !self.check(&Token::RParen) {
            loop {
                // 检查是否是裸可变参数 ...
                if self.check(&Token::DotDotDot) {
                    self.advance(); // 消费 ...
                    params.push(crate::types::ParameterInfo::new_varargs("...".to_string(), crate::types::Type::CVoid));
                    if self.check(&Token::Comma) {
                        return Err(self.error("可变参数必须是最后一个参数\n提示: 可变参数(...)必须放在参数列表的最后"));
                    }
                    break;
                }

                // 解析参数类型
                let param_type = self.parse_type()?;

                // 检查是否是可变参数类型（type...）
                let is_varargs = self.check(&Token::DotDotDot);

                if is_varargs {
                    self.advance(); // 消费 ...
                    // type... 形式的可变参数，需要一个名称
                    let name = self.consume_identifier("期望参数名\n提示: 可变参数需要名称，例如: int... args")?;
                    params.push(crate::types::ParameterInfo::new_varargs(name, param_type));
                    // 可变参数之后可以有更多参数（通过命名参数指定）
                    if self.match_token(&Token::Comma) {
                        continue;
                    }
                    break;
                } else {
                    // 检查后面是否是标识符或关键字（表示这是参数名）
                    // 在extern声明中，参数名是可选的，且可以是关键字（如type, fn等）
                    let param_name = if self.is_potential_param_name() {
                        // 获取参数名（可能是标识符或关键字）
                        let name = self.current_token_as_param_name();
                        self.advance();
                        name
                    } else if self.check(&Token::Comma) || self.check(&Token::RParen) {
                        // 没有参数名，生成一个默认名称
                        param_index += 1;
                        format!("arg{}", param_index)
                    } else {
                        // 其他情况也生成默认名称
                        param_index += 1;
                        format!("arg{}", param_index)
                    };
                    params.push(crate::types::ParameterInfo::new(param_name, param_type));
                }

                if !self.match_token(&Token::Comma) {
                    break;
                }
            }
        }

        Ok(params)
    }

    /// 解析类型别名声明: alias Name = Type;
    /// 支持函数指针类型: alias CompareFn = fn(i32, i32) -> i32;
    fn parse_type_alias(&mut self) -> cayResult<crate::ast::TypeAliasDecl> {
        let loc = self.current_loc();

        // 消费 alias 关键字
        self.consume(&crate::lexer::Token::Alias, "期望 'alias'\n提示: 类型别名声明应以 alias 开头，例如: alias MyInt = int;")?;

        // 解析类型名称
        let name = self.consume_identifier("期望类型别名名称\n提示: alias 后应跟类型名称，例如: alias MyInt = int;")?;

        // 消费 =
        self.consume(&crate::lexer::Token::Assign, "期望 '='\n提示: 类型别名格式为 alias Name = Type;")?;

        // 解析目标类型（支持函数指针类型 fn(...) -> ReturnType）
        let target_type = self.parse_type_or_fn_ptr()?;

        // 消费分号
        self.consume(&crate::lexer::Token::Semicolon, "期望 ';'\n提示: 类型别名声明应以 ';' 结束")?;

        // 注册类型别名以便后续解析使用
        self.register_type_alias(name.clone(), target_type.clone());

        Ok(crate::ast::TypeAliasDecl {
            name,
            target_type,
            namespace_path: Vec::new(),
            loc,
        })
    }

    /// 检查当前token是否可能是参数名（标识符或某些关键字）
    fn is_potential_param_name(&self) -> bool {
        use crate::lexer::Token;
        match self.current_token() {
            Token::Identifier(_) => true,
            // 在extern声明中，某些关键字也可以作为参数名（如alias, fn等）
            Token::Alias | Token::Fn => true,
            _ => false,
        }
    }

    /// 将当前token转换为参数名字符串
    fn current_token_as_param_name(&self) -> String {
        use crate::lexer::Token;
        match self.current_token() {
            Token::Identifier(name) => name.clone(),
            Token::Alias => "alias".to_string(),
            Token::Fn => "fn".to_string(),
            _ => "arg".to_string(),
        }
    }

    /// 注册类型别名
    fn register_type_alias(&mut self, name: String, target_type: crate::types::Type) {
        self.type_aliases.insert(name, target_type);
    }

    /// 获取类型别名
    pub fn get_type_alias(&self, name: &str) -> Option<crate::types::Type> {
        self.type_aliases.get(name).cloned()
    }

    /// 解析类型或函数指针类型
    fn parse_type_or_fn_ptr(&mut self) -> cayResult<crate::types::Type> {
        // 检查是否是函数指针类型: fn(...) -> ReturnType
        if self.check(&crate::lexer::Token::Fn) {
            self.parse_fn_ptr_type()
        } else {
            // 普通类型
            self.parse_type()
        }
    }

    /// 解析函数指针类型: fn(ParamType1, ParamType2, ...) -> ReturnType
    /// 支持可选参数名: fn(param1: Type1, param2: Type2, ...) -> ReturnType
    fn parse_fn_ptr_type(&mut self) -> cayResult<crate::types::Type> {
        use crate::lexer::Token;

        // 消费 fn 关键字
        self.consume(&Token::Fn, "期望 'fn'")?;

        // 消费 (
        self.consume(&Token::LParen, "期望 '('\n提示: 函数指针类型格式为 fn(ParamTypes...) -> ReturnType")?;

        // 解析参数类型列表（支持可选参数名）
        let mut param_types = Vec::new();
        if !self.check(&Token::RParen) {
            loop {
                // 检查是否是命名参数格式: name: Type
                // 通过预读判断：如果是标识符后面跟着冒号，则是命名参数
                let param_type = if matches!(self.current_token(), Token::Identifier(_)) {
                    // 预读：保存当前位置，尝试解析标识符和冒号
                    let saved_pos = self.pos;
                    self.advance(); // 消费标识符

                    if self.check(&Token::Colon) {
                        // 是命名参数格式: name: Type
                        self.advance(); // 消费冒号
                        let ty = self.parse_type()?;
                        ty
                    } else {
                        // 不是命名参数格式，回退并解析为类型
                        self.pos = saved_pos;
                        self.parse_type()?
                    }
                } else {
                    // 普通类型
                    self.parse_type()?
                };
                param_types.push(param_type);

                if !self.match_token(&Token::Comma) {
                    break;
                }
            }
        }

        // 消费 )
        self.consume(&crate::lexer::Token::RParen, "期望 ')'\n提示: 函数指针参数列表应以 ')' 结束")?;

        // 消费 ->
        self.consume(&crate::lexer::Token::Arrow, "期望 '->'\n提示: 函数指针类型需要指定返回类型，格式为 fn(...) -> ReturnType")?;

        // 解析返回类型
        let return_type = self.parse_type()?;

        // 创建函数类型
        Ok(crate::types::Type::Function(Box::new(crate::types::FunctionType {
            params: param_types,
            return_type: Box::new(return_type),
            is_static: true,
        })))
    }

    // ==================== Namespace 和 Using 解析 ====================

    /// 解析文件级 namespace 声明: namespace std;
    fn parse_namespace_file_level(&mut self) -> cayResult<Vec<String>> {
        // 消费 namespace 关键字
        self.advance();
        // 解析路径
        let path = self.parse_namespace_path()?;
        // 消费分号
        self.consume(&crate::lexer::Token::Semicolon, "期望 ';' 结束文件级 namespace 声明\n提示: 文件级 namespace 格式为 'namespace 路径;'")?;
        Ok(path)
    }

    /// 解析块级 namespace 声明: namespace std { ... }
    fn parse_namespace_block(&mut self) -> cayResult<crate::ast::NamespaceDecl> {
        let loc = utils::current_loc(self);
        // 消费 namespace 关键字
        self.advance();
        // 解析路径
        let path = self.parse_namespace_path()?;
        // 消费 {
        self.consume(&crate::lexer::Token::LBrace, "期望 '{' 开始 namespace 块\n提示: 块级 namespace 格式为 'namespace 路径 { ... }'")?;

        let mut classes = Vec::new();
        let mut interfaces = Vec::new();
        let mut top_level_functions = Vec::new();
        let mut extern_declarations = Vec::new();
        let mut type_aliases = Vec::new();
        let mut nested_namespaces = Vec::new();

        while !self.check(&crate::lexer::Token::RBrace) && !self.is_at_end() {
            if self.check(&crate::lexer::Token::Namespace) {
                nested_namespaces.push(self.parse_namespace_block()?);
            } else if self.check(&crate::lexer::Token::Interface)
                || (self.check(&crate::lexer::Token::Public) && self.check_next(&crate::lexer::Token::Interface))
            {
                interfaces.push(self.parse_interface()?);
            } else if self.check(&crate::lexer::Token::Class)
                || self.check(&crate::lexer::Token::Private)
                || self.check(&crate::lexer::Token::Protected)
                || self.check(&crate::lexer::Token::AtMain)
            {
                classes.push(self.parse_class()?);
            } else if self.check(&crate::lexer::Token::Public) {
                if self.check_top_level_function() {
                    top_level_functions.push(self.parse_top_level_function()?);
                } else {
                    classes.push(self.parse_class()?);
                }
            } else if self.check_top_level_function_return_type() {
                top_level_functions.push(self.parse_top_level_function_without_public()?);
            } else if self.check(&crate::lexer::Token::Extern) {
                extern_declarations.push(self.parse_extern_declaration()?);
            } else if self.check(&crate::lexer::Token::Alias) {
                type_aliases.push(self.parse_type_alias()?);
            } else {
                let token_name = utils::get_token_name(utils::current_token(self));
                return Err(self.error(&format!(
                    "namespace 块内出现意外的令牌: {}\n提示: namespace 块内只能包含类、接口、函数、extern 和嵌套 namespace 声明",
                    token_name
                )));
            }
        }

        // 消费 }
        self.consume(&crate::lexer::Token::RBrace, "期望 '}' 结束 namespace 块\n提示: namespace 块必须以 '}' 结束")?;

        Ok(crate::ast::NamespaceDecl {
            path,
            classes,
            structs: Vec::new(),
            enums: Vec::new(),
            interfaces,
            top_level_functions,
            extern_declarations,
            type_aliases,
            nested_namespaces,
            loc,
        })
    }

    /// 解析 namespace 路径: Identifier (:: Identifier)*
    fn parse_namespace_path(&mut self) -> cayResult<Vec<String>> {
        let mut path = Vec::new();
        // 第一个标识符
        let first = self.consume_identifier("期望命名空间标识符\n提示: namespace 路径格式为 'Identifier' 或 'Identifier::Identifier'")?;
        path.push(first);

        // 后续 ::Identifier
        while self.match_token(&crate::lexer::Token::DoubleColon) {
            let next = self.consume_identifier("期望命名空间标识符\n提示: '::' 后应跟标识符")?;
            path.push(next);
        }

        Ok(path)
    }

    /// 解析 using 声明: using std::StringBuilder;
    fn parse_using_declaration(&mut self) -> cayResult<crate::ast::UsingDecl> {
        let loc = utils::current_loc(self);
        // 消费 using 关键字
        self.advance();

        // 检查是否被禁止的 using namespace 形式
        if self.check(&crate::lexer::Token::Namespace) {
            return Err(self.error(
                "不允许使用 'using namespace' 语法\n提示: Cavvy 只支持 'using 路径::名称;' 的单名导入，不支持 'using namespace'")
            );
        }

        // 解析路径（包括最后的名称）
        let path = self.parse_namespace_path()?;

        // 路径不能为空
        if path.is_empty() {
            return Err(self.error("using 声明缺少名称\n提示: using 声明格式为 'using 路径::名称;'"));
        }

        // 检查通配符导入（路径最后一部分是 *）
        if path.last().map(|s| s.as_str()) == Some("*") {
            return Err(self.error(
                "不允许使用通配符 'using path::*'\n提示: Cavvy 只支持 'using 路径::名称;' 的单名导入")
            );
        }

        // 消费分号
        self.consume(&crate::lexer::Token::Semicolon, "期望 ';' 结束 using 声明\n提示: using 声明格式为 'using 路径::名称;'")?;

        Ok(crate::ast::UsingDecl { path, loc })
    }
}

/// 解析令牌流生成 AST
pub fn parse(tokens: Vec<TokenWithLocation>) -> cayResult<Program> {
    let mut parser = Parser::new(tokens);
    parser.parse()
}

/// 解析令牌流生成 AST（带源代码，用于内联IR解析）
pub fn parse_with_source(tokens: Vec<TokenWithLocation>, source: String) -> cayResult<Program> {
    let mut parser = Parser::with_source(tokens, source);
    parser.parse()
}
