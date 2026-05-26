use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use cavvy::lexer;
use cavvy::parser;
use cavvy::preprocessor;
use cavvy::semantic;

const VERSION: &str = env!("CAY_LSP_VERSION");

/// 文档状态
#[derive(Debug, Clone)]
struct DocumentState {
    uri: Url,
    content: String,
    version: i32,
    diagnostics: Vec<Diagnostic>,
}

/// Cavvy 语言服务器
struct CavvyLanguageServer {
    client: Client,
    documents: Arc<DashMap<String, DocumentState>>,
}

/// 服务器配置
#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    #[serde(default)]
    enable_preprocessing: bool,
    #[serde(default)]
    enable_semantic_tokens: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enable_preprocessing: true,
            enable_semantic_tokens: false,
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for CavvyLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        self.client
            .log_message(MessageType::INFO, format!("Cavvy LSP v{} 初始化中...", VERSION))
            .await;

        let capabilities = ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::FULL),
                    will_save: None,
                    will_save_wait_until: None,
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(false),
                    })),
                },
            )),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
                DiagnosticOptions {
                    identifier: Some("cavvy".to_string()),
                    inter_file_dependencies: true,
                    workspace_diagnostics: false,
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                },
            )),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![
                    ".".to_string(),
                    "::".to_string(),
                    " ".to_string(),
                ]),
                ..Default::default()
            }),
            ..Default::default()
        };

        Ok(InitializeResult {
            capabilities,
            server_info: Some(ServerInfo {
                name: "cay-lsp".to_string(),
                version: Some(VERSION.to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Cavvy LSP 已初始化完成")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        self.client
            .log_message(MessageType::INFO, "Cavvy LSP 正在关闭...")
            .await;
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let content = params.text_document.text;
        let version = params.text_document.version;

        self.client
            .log_message(MessageType::INFO, format!("文档打开: {}", uri))
            .await;

        let state = DocumentState {
            uri: params.text_document.uri.clone(),
            content: content.clone(),
            version,
            diagnostics: Vec::new(),
        };

        self.documents.insert(uri.clone(), state);
        self.validate_document(&uri, &content).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        
        // 获取最新的内容（FULL sync 模式下只有最后一个变化）
        if let Some(change) = params.content_changes.last() {
            let content = change.text.clone();
            let version = params.text_document.version;

            // 更新文档状态
            if let Some(mut state) = self.documents.get_mut(&uri) {
                state.content = content.clone();
                state.version = version;
            } else {
                let state = DocumentState {
                    uri: params.text_document.uri.clone(),
                    content: content.clone(),
                    version,
                    diagnostics: Vec::new(),
                };
                self.documents.insert(uri.clone(), state);
            }

            self.validate_document(&uri, &content).await;
        }
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        
        self.client
            .log_message(MessageType::INFO, format!("文档保存: {}", uri))
            .await;

        // 重新验证
        if let Some(state) = self.documents.get(&uri) {
            let content = state.content.clone();
            drop(state);
            self.validate_document(&uri, &content).await;
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        
        self.client
            .log_message(MessageType::INFO, format!("文档关闭: {}", uri))
            .await;

        self.documents.remove(&uri);
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let position = params.text_document_position_params.position;

        if let Some(state) = self.documents.get(&uri) {
            let content = &state.content;
            
            // 获取当前行的内容
            let lines: Vec<&str> = content.lines().collect();
            if let Some(line) = lines.get(position.line as usize) {
                // 简单实现：提取当前位置的单词并提供基本信息
                let word = extract_word_at_position(line, position.character as usize);
                
                if !word.is_empty() {
                    let contents = HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: format!("**`{}`**\n\nCavvy 标识符", word),
                    });
                    
                    return Ok(Some(Hover {
                        contents,
                        range: None,
                    }));
                }
            }
        }

        Ok(None)
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();

        if let Some(state) = self.documents.get(&uri) {
            let content = &state.content;
            
            // 解析文档获取符号
            match self.parse_symbols(content, &uri).await {
                Ok(symbols) => {
                    return Ok(Some(DocumentSymbolResponse::Nested(symbols)));
                }
                Err(e) => {
                    self.client
                        .log_message(MessageType::WARNING, format!("解析符号失败: {}", e))
                        .await;
                }
            }
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let position = params.text_document_position.position;

        if let Some(state) = self.documents.get(&uri) {
            let content = &state.content;
            
            // 获取当前行的内容
            let lines: Vec<&str> = content.lines().collect();
            if let Some(line) = lines.get(position.line as usize) {
                let _line_before_cursor = &line[..position.character.min(line.len() as u32) as usize];
                
                // 简单的关键字补全
                let mut items = Vec::new();
                
                // Cavvy 关键字
                let keywords = vec![
                    ("class", "定义类"),
                    ("public", "公开访问修饰符"),
                    ("private", "私有访问修饰符"),
                    ("protected", "保护访问修饰符"),
                    ("static", "静态修饰符"),
                    ("final", "最终修饰符"),
                    ("abstract", "抽象修饰符"),
                    ("extends", "继承"),
                    ("implements", "实现接口"),
                    ("interface", "定义接口"),
                    ("void", "无返回值类型"),
                    ("int", "整数类型"),
                    ("long", "长整数类型"),
                    ("float", "单精度浮点类型"),
                    ("double", "双精度浮点类型"),
                    ("boolean", "布尔类型"),
                    ("char", "字符类型"),
                    ("String", "字符串类型"),
                    ("if", "条件语句"),
                    ("else", "否则分支"),
                    ("while", "while循环"),
                    ("for", "for循环"),
                    ("do", "do-while循环"),
                    ("switch", "switch语句"),
                    ("case", "case分支"),
                    ("default", "默认分支"),
                    ("break", "跳出循环"),
                    ("continue", "继续下一次循环"),
                    ("return", "返回语句"),
                    ("new", "创建实例"),
                    ("this", "当前实例引用"),
                    ("super", "父类引用"),
                    ("instanceof", "类型检查"),
                    ("var", "变量声明"),
                    ("let", "变量声明"),
                    ("auto", "自动类型推断"),
                    ("extern", "外部函数声明"),
                    ("true", "真值"),
                    ("false", "假值"),
                    ("null", "空值"),
                ];
                
                for (keyword, desc) in keywords {
                    items.push(CompletionItem {
                        label: keyword.to_string(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some(desc.to_string()),
                        insert_text: Some(keyword.to_string()),
                        ..Default::default()
                    });
                }
                
                // 内置函数
                let builtins = vec![
                    ("println", "println(内容)", "输出并换行"),
                    ("print", "print(内容)", "输出不换行"),
                    ("readInt", "readInt()", "读取整数"),
                    ("readFloat", "readFloat()", "读取浮点数"),
                    ("readLine", "readLine()", "读取一行字符串"),
                ];
                
                for (name, insert, desc) in builtins {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some(desc.to_string()),
                        insert_text: Some(insert.to_string()),
                        ..Default::default()
                    });
                }
                
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        Ok(None)
    }
}

impl CavvyLanguageServer {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(DashMap::new()),
        }
    }

    /// 验证文档并发送诊断信息
    async fn validate_document(&self, uri: &str, content: &str) {
        let diagnostics = self.analyze_document(uri, content).await;
        
        // 更新文档状态中的诊断信息
        if let Some(mut state) = self.documents.get_mut(uri) {
            state.diagnostics = diagnostics.clone();
        }

        // 发送诊断信息给客户端
        let url = match Url::parse(uri) {
            Ok(url) => url,
            Err(_) => {
                self.client
                    .log_message(MessageType::ERROR, format!("无效的URI: {}", uri))
                    .await;
                return;
            }
        };

        self.client
            .publish_diagnostics(url, diagnostics, None)
            .await;
    }

    /// 分析文档，返回诊断信息
    async fn analyze_document(&self, uri: &str, content: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let file_path = uri.strip_prefix("file://").unwrap_or(uri);

        // 1. 预处理
        let processed_content = match self.preprocess_content(content, file_path).await {
            Ok(processed) => processed,
            Err(e) => {
                if let Some(diagnostic) = error_to_diagnostic(&e, content) {
                    diagnostics.push(diagnostic);
                }
                return diagnostics;
            }
        };

        // 2. 词法分析
        let tokens = match lexer::lex(&processed_content) {
            Ok(tokens) => tokens,
            Err(e) => {
                if let Some(diagnostic) = error_to_diagnostic(&e, &processed_content) {
                    diagnostics.push(diagnostic);
                }
                return diagnostics;
            }
        };

        // 3. 语法分析
        let ast = match parser::parse(tokens) {
            Ok(ast) => ast,
            Err(e) => {
                if let Some(diagnostic) = error_to_diagnostic(&e, &processed_content) {
                    diagnostics.push(diagnostic);
                }
                return diagnostics;
            }
        };

        // 4. 语义分析
        let mut analyzer = semantic::SemanticAnalyzer::new();
        if let Err(e) = analyzer.analyze(&ast) {
            if let Some(diagnostic) = error_to_diagnostic(&e, &processed_content) {
                diagnostics.push(diagnostic);
            }
        }

        diagnostics
    }

    /// 预处理文档内容
    async fn preprocess_content(&self, content: &str, file_path: &str) -> std::result::Result<String, cavvy::error::cayError> {
        let base_dir = Path::new(file_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf()));

        preprocessor::preprocess(content, file_path, base_dir.to_str().unwrap_or("."))
    }

    /// 解析文档符号
    async fn parse_symbols(&self, content: &str, uri: &str) -> std::result::Result<Vec<DocumentSymbol>, String> {
        let file_path = uri.strip_prefix("file://").unwrap_or(uri);
        
        // 预处理
        let processed = match self.preprocess_content(content, file_path).await {
            Ok(p) => p,
            Err(e) => return Err(format!("预处理失败: {:?}", e)),
        };
        
        // 词法分析
        let tokens = match lexer::lex(&processed) {
            Ok(t) => t,
            Err(e) => return Err(format!("词法分析失败: {:?}", e)),
        };
        
        // 语法分析
        let ast = match parser::parse(tokens) {
            Ok(a) => a,
            Err(e) => return Err(format!("语法分析失败: {:?}", e)),
        };
        
        let mut symbols = Vec::new();
        
        // 提取类定义
        for class in &ast.classes {
            let class_symbol = DocumentSymbol {
                name: class.name.clone(),
                detail: Some(format!("class {}", class.name)),
                kind: SymbolKind::CLASS,
                tags: None,
                deprecated: None,
                range: Range {
                    start: Position::new(class.loc.line as u32, 0),
                    end: Position::new(class.loc.line as u32, 0),
                },
                selection_range: Range {
                    start: Position::new(class.loc.line as u32, 0),
                    end: Position::new(class.loc.line as u32, 0),
                },
                children: Some(extract_class_members(class)),
            };
            symbols.push(class_symbol);
        }
        
        // 提取顶层函数（如果有的话）
        // TODO: 当 AST 支持顶层函数时添加
        
        Ok(symbols)
    }
}

/// 将错误转换为 LSP 诊断信息
fn error_to_diagnostic(error: &cavvy::error::cayError, source: &str) -> Option<Diagnostic> {
    use cavvy::error::cayError;

    let (message, line, column) = match error {
        cayError::Lexer { message, line, column, .. } => {
            (message.clone(), *line, *column)
        }
        cayError::Parser { message, line, column, .. } => {
            (message.clone(), *line, *column)
        }
        cayError::Semantic { message, line, column, .. } => {
            (message.clone(), *line, *column)
        }
        cayError::Preprocessor { message, line, column, .. } => {
            (message.clone(), *line, *column)
        }
        cayError::Io { file: _, message } => {
            return Some(Diagnostic {
                range: Range {
                    start: Position::new(0, 0),
                    end: Position::new(0, 0),
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("cavvy".to_string()),
                message: message.clone(),
                related_information: None,
                tags: None,
                data: None,
            });
        }
        _ => {
            return Some(Diagnostic {
                range: Range {
                    start: Position::new(0, 0),
                    end: Position::new(0, 0),
                },
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("cavvy".to_string()),
                message: format!("{:?}", error),
                related_information: None,
                tags: None,
                data: None,
            });
        }
    };

    // 计算行的长度
    let lines: Vec<&str> = source.lines().collect();
    let line_len = lines.get(line.saturating_sub(1)).map(|l| l.len()).unwrap_or(0) as u32;

    Some(Diagnostic {
        range: Range {
            start: Position::new(line.saturating_sub(1) as u32, column.saturating_sub(1) as u32),
            end: Position::new(line.saturating_sub(1) as u32, line_len),
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("cavvy".to_string()),
        message,
        related_information: None,
        tags: None,
        data: None,
    })
}

/// 提取类成员符号
fn extract_class_members(class: &cavvy::ast::ClassDecl) -> Vec<DocumentSymbol> {
    let mut members = Vec::new();
    use cavvy::ast::ClassMember;

    // 遍历类成员
    for member in &class.members {
        match member {
            ClassMember::Field(field) => {
                let symbol = DocumentSymbol {
                    name: field.name.clone(),
                    detail: Some(format!("{:?} {}", field.field_type, field.name)),
                    kind: SymbolKind::FIELD,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position::new(field.loc.line as u32, 0),
                        end: Position::new(field.loc.line as u32, 0),
                    },
                    selection_range: Range {
                        start: Position::new(field.loc.line as u32, 0),
                        end: Position::new(field.loc.line as u32, 0),
                    },
                    children: None,
                };
                members.push(symbol);
            }
            ClassMember::Method(method) => {
                let _is_static = method.modifiers.contains(&cavvy::ast::Modifier::Static);
                let symbol = DocumentSymbol {
                    name: method.name.clone(),
                    detail: Some(format!("{:?} {}", method.return_type, method.name)),
                    kind: SymbolKind::METHOD,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position::new(method.loc.line as u32, 0),
                        end: Position::new(method.loc.line as u32, 0),
                    },
                    selection_range: Range {
                        start: Position::new(method.loc.line as u32, 0),
                        end: Position::new(method.loc.line as u32, 0),
                    },
                    children: None,
                };
                members.push(symbol);
            }
            ClassMember::Constructor(ctor) => {
                let symbol = DocumentSymbol {
                    name: "<constructor>".to_string(),
                    detail: Some("构造函数".to_string()),
                    kind: SymbolKind::CONSTRUCTOR,
                    tags: None,
                    deprecated: None,
                    range: Range {
                        start: Position::new(ctor.loc.line as u32, 0),
                        end: Position::new(ctor.loc.line as u32, 0),
                    },
                    selection_range: Range {
                        start: Position::new(ctor.loc.line as u32, 0),
                        end: Position::new(ctor.loc.line as u32, 0),
                    },
                    children: None,
                };
                members.push(symbol);
            }
            _ => {} // 其他成员类型暂时忽略
        }
    }

    members
}

/// 从行中提取指定位置的单词
fn extract_word_at_position(line: &str, position: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.is_empty() || position >= chars.len() {
        return String::new();
    }

    // 找到单词的起始位置
    let mut start = position;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }

    // 找到单词的结束位置
    let mut end = position;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }

    chars[start..end].iter().collect()
}

/// 判断字符是否是单词字符
fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[tokio::main]
async fn main() {
    // 设置日志
    let _ = env_logger::try_init();

    // 创建 LSP 服务
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(|client| CavvyLanguageServer::new(client));

    // 运行服务器
    Server::new(stdin, stdout, socket).serve(service).await;
}
