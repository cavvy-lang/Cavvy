use std::env;
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

/// 补全符号信息
#[derive(Debug, Clone)]
struct CompletionSymbol {
    name: String,
    kind: String,
    detail: String,
    documentation: Option<String>,
    insert_text: String,
}

/// Hover 信息
#[derive(Debug, Clone)]
struct HoverInfo {
    name: String,
    kind: String,
    signature: String,
    documentation: String,
}

#[tower_lsp::async_trait]
impl LanguageServer for CavvyLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        self.client
            .log_message(
                MessageType::INFO,
                format!("Cavvy LSP v{} 初始化中...", VERSION),
            )
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
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: Some("cavvy".to_string()),
                inter_file_dependencies: true,
                workspace_diagnostics: false,
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
            })),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![".".to_string(), "::".to_string(), " ".to_string()]),
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
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let position = params.text_document_position_params.position;

        if let Some(state) = self.documents.get(&uri) {
            let content = &state.content;
            let file_path = uri.strip_prefix("file://").unwrap_or(&uri);

            // 获取当前行的内容
            let lines: Vec<&str> = content.lines().collect();
            if let Some(line) = lines.get(position.line as usize) {
                // 提取当前位置的单词
                let word = extract_word_at_position(line, position.character as usize);

                if !word.is_empty() {
                    // 尝试从文档和 include 文件获取详细的 hover 信息
                    let hover_info = self.get_hover_info(content, file_path, &word).await;

                    let contents = HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: hover_info,
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
            let file_path = uri.strip_prefix("file://").unwrap_or(&uri);

            // 获取当前行的内容
            let lines: Vec<&str> = content.lines().collect();
            if let Some(line) = lines.get(position.line as usize) {
                let line_before_cursor =
                    &line[..position.character.min(line.len() as u32) as usize];

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
                    ("namespace", "命名空间"),
                    ("using", "使用命名空间"),
                    ("void", "无返回值类型"),
                    ("int", "整数类型"),
                    ("long", "长整数类型"),
                    ("float", "单精度浮点类型"),
                    ("double", "双精度浮点类型"),
                    ("bool", "布尔类型"),
                    ("char", "字符类型"),
                    ("string", "字符串类型"),
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
                    ("try", "异常捕获"),
                    ("catch", "捕获异常"),
                    ("finally", "最终执行"),
                    ("throw", "抛出异常"),
                    ("import", "导入包"),
                    ("package", "声明包"),
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
                    ("println", "println($1)", "输出并换行"),
                    ("print", "print($1)", "输出不换行"),
                    ("printf", "printf($1)", "格式化输出"),
                    ("readInt", "readInt()", "读取整数"),
                    ("readLong", "readLong()", "读取长整数"),
                    ("readFloat", "readFloat()", "读取浮点数"),
                    ("readDouble", "readDouble()", "读取双精度浮点数"),
                    ("readLine", "readLine()", "读取一行字符串"),
                    ("readChar", "readChar()", "读取字符"),
                    ("parseInt", "parseInt($1)", "字符串转整数"),
                    ("parseFloat", "parseFloat($1)", "字符串转浮点数"),
                ];

                for (name, insert, desc) in builtins {
                    items.push(CompletionItem {
                        label: name.to_string(),
                        kind: Some(CompletionItemKind::FUNCTION),
                        detail: Some(desc.to_string()),
                        insert_text: Some(insert.to_string()),
                        insert_text_format: Some(InsertTextFormat::SNIPPET),
                        ..Default::default()
                    });
                }

                // 从当前文档和 include 文件提取符号
                let symbols = self.extract_completion_symbols_sync(content, file_path);
                for symbol in symbols {
                    let kind = match symbol.kind.as_str() {
                        "class" => CompletionItemKind::CLASS,
                        "interface" => CompletionItemKind::INTERFACE,
                        "method" => CompletionItemKind::METHOD,
                        "field" => CompletionItemKind::FIELD,
                        "variable" => CompletionItemKind::VARIABLE,
                        "namespace" => CompletionItemKind::MODULE,
                        _ => CompletionItemKind::TEXT,
                    };

                    items.push(CompletionItem {
                        label: symbol.name,
                        kind: Some(kind),
                        detail: Some(symbol.detail),
                        documentation: symbol.documentation.map(|d| Documentation::String(d)),
                        insert_text: Some(symbol.insert_text),
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
        // 先清除旧诊断，避免错误残留
        let url = match Url::parse(uri) {
            Ok(url) => url.clone(),
            Err(_) => {
                self.client
                    .log_message(MessageType::ERROR, format!("无效的URI: {}", uri))
                    .await;
                return;
            }
        };

        // 立即发送空诊断清除旧错误
        self.client
            .publish_diagnostics(url.clone(), Vec::new(), None)
            .await;

        // 分析文档获取新诊断
        let diagnostics = self.analyze_document(uri, content).await;

        // 更新文档状态中的诊断信息
        if let Some(mut state) = self.documents.get_mut(uri) {
            state.diagnostics = diagnostics.clone();
        }

        // 发送新的诊断信息
        self.client
            .publish_diagnostics(url, diagnostics, None)
            .await;
    }

    /// 分析文档，返回诊断信息
    async fn analyze_document(&self, uri: &str, content: &str) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let file_path = uri.strip_prefix("file://").unwrap_or(uri);

        // 1. 预处理（带源映射）
        let (processed_content, source_map) = match self
            .preprocess_content_with_source_map(content, file_path)
            .await
        {
            Ok(result) => result,
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
                if let Some(diagnostic) =
                    error_to_diagnostic_with_source_map(&e, content, &source_map, file_path)
                {
                    diagnostics.push(diagnostic);
                }
                return diagnostics;
            }
        };

        // 3. 语法分析
        let ast = match parser::parse(tokens) {
            Ok(ast) => ast,
            Err(e) => {
                if let Some(diagnostic) =
                    error_to_diagnostic_with_source_map(&e, content, &source_map, file_path)
                {
                    diagnostics.push(diagnostic);
                }
                return diagnostics;
            }
        };

        // 4. 语义分析
        let mut analyzer = semantic::SemanticAnalyzer::new();
        if let Err(e) = analyzer.analyze(ast) {
            if let Some(diagnostic) =
                error_to_diagnostic_with_source_map(&e, content, &source_map, file_path)
            {
                diagnostics.push(diagnostic);
            }
        }

        diagnostics
    }

    /// 预处理文档内容（带源映射）
    async fn preprocess_content_with_source_map(
        &self,
        content: &str,
        file_path: &str,
    ) -> std::result::Result<(String, preprocessor::SourceMap), cavvy::miette_diagnostic::cayError> {
        let base_dir = Path::new(file_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())
            });

        let mut pp = preprocessor::Preprocessor::new(base_dir.to_str().unwrap_or("."));
        let result = pp.process_with_source_map(content, file_path)?;
        Ok((result.code, result.source_map))
    }

    /// 提取补全符号（从当前文档和 include 文件）
    fn extract_completion_symbols_sync(
        &self,
        content: &str,
        file_path: &str,
    ) -> Vec<CompletionSymbol> {
        let mut symbols = Vec::new();
        let mut processed_files = std::collections::HashSet::new();

        // 提取当前文档的符号
        self.extract_symbols_from_content_sync(
            content,
            file_path,
            &mut symbols,
            &mut processed_files,
        );

        symbols
    }

    /// 从内容提取符号（同步版本，使用语义分析器的 TypeRegistry）
    fn extract_symbols_from_content_sync(
        &self,
        content: &str,
        file_path: &str,
        symbols: &mut Vec<CompletionSymbol>,
        processed_files: &mut std::collections::HashSet<String>,
    ) {
        if processed_files.contains(file_path) {
            return;
        }
        processed_files.insert(file_path.to_string());

        // 预处理（同步版本）
        let base_dir = Path::new(file_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf())
            });

        let mut pp = preprocessor::Preprocessor::new(base_dir.to_str().unwrap_or("."));
        let (processed, _) = match pp.process_with_source_map(content, file_path) {
            Ok(r) => (r.code, r.source_map),
            Err(_) => return,
        };

        // 词法分析
        let tokens = match lexer::lex(&processed) {
            Ok(t) => t,
            Err(_) => return,
        };

        // 语法分析
        let ast = match parser::parse(tokens) {
            Ok(a) => a,
            Err(_) => return,
        };

        // 使用语义分析器获取完整的类型信息（包括 include 文件中的符号）
        let mut analyzer = semantic::SemanticAnalyzer::new();

        // 运行语义分析（忽略错误，我们只关心 type_registry 中的符号）
        let _ = analyzer.analyze(ast);

        // 从 TypeRegistry 提取类和方法
        for (class_name, class_info) in analyzer.type_registry().classes.iter() {
            // 添加类名
            if !symbols.iter().any(|s| s.name == *class_name) {
                symbols.push(CompletionSymbol {
                    name: class_name.clone(),
                    kind: "class".to_string(),
                    detail: format!("class {}", class_name),
                    documentation: Some(format!("类 {}", class_name)),
                    insert_text: class_name.clone(),
                });
            }

            // 添加类的方法
            for (method_name, methods) in class_info.methods.iter() {
                for method in methods.iter() {
                    if !symbols.iter().any(|s| s.name == *method_name) {
                        let params: Vec<String> = method
                            .params
                            .iter()
                            .map(|p| format!("{} {}", p.param_type, p.name))
                            .collect();
                        let signature = format!(
                            "{} {}({})",
                            method.return_type,
                            method.name,
                            params.join(", ")
                        );

                        symbols.push(CompletionSymbol {
                            name: method_name.clone(),
                            kind: "method".to_string(),
                            detail: signature,
                            documentation: Some(format!("{} 类的方法", class_name)),
                            insert_text: format!("{}(${{1}})", method_name),
                        });
                    }
                }
            }

            // 添加类的字段
            for (field_name, field_info) in class_info.fields.iter() {
                if !symbols.iter().any(|s| s.name == *field_name) {
                    symbols.push(CompletionSymbol {
                        name: field_name.clone(),
                        kind: "field".to_string(),
                        detail: format!("{} {}", field_info.field_type, field_name),
                        documentation: Some(format!("{} 类的字段", class_name)),
                        insert_text: field_name.clone(),
                    });
                }
            }
        }

        // 从 TypeRegistry 提取接口
        for (interface_name, _interface_info) in analyzer.type_registry().interfaces.iter() {
            if !symbols.iter().any(|s| s.name == *interface_name) {
                symbols.push(CompletionSymbol {
                    name: interface_name.clone(),
                    kind: "interface".to_string(),
                    detail: format!("interface {}", interface_name),
                    documentation: Some(format!("接口 {}", interface_name)),
                    insert_text: interface_name.clone(),
                });
            }
        }

        // 从 TypeRegistry 提取 struct
        for (struct_name, _struct_info) in analyzer.type_registry().structs.iter() {
            if !symbols.iter().any(|s| s.name == *struct_name) {
                symbols.push(CompletionSymbol {
                    name: struct_name.clone(),
                    kind: "struct".to_string(),
                    detail: format!("struct {}", struct_name),
                    documentation: Some(format!("结构体 {}", struct_name)),
                    insert_text: struct_name.clone(),
                });
            }
        }

        // 从 TypeRegistry 提取 enum
        for (enum_name, enum_info) in analyzer.type_registry().enums.iter() {
            if !symbols.iter().any(|s| s.name == *enum_name) {
                symbols.push(CompletionSymbol {
                    name: enum_name.clone(),
                    kind: "enum".to_string(),
                    detail: format!("enum {}", enum_name),
                    documentation: Some(format!("枚举 {}", enum_name)),
                    insert_text: enum_name.clone(),
                });
            }

            // 添加枚举变体
            for variant in enum_info.variants.iter() {
                let variant_name = format!("{}::{}", enum_name, variant.name);
                if !symbols.iter().any(|s| s.name == variant_name) {
                    symbols.push(CompletionSymbol {
                        name: variant_name.clone(),
                        kind: "enumMember".to_string(),
                        detail: format!("{}::{}", enum_name, variant.name),
                        documentation: Some(format!("{} 枚举的变体", enum_name)),
                        insert_text: variant_name.clone(),
                    });
                }
            }
        }

        // 提取 namespace 别名（来自 using 声明）
        for (alias, qualified) in analyzer.type_registry().namespace_aliases.iter() {
            if !symbols.iter().any(|s| s.name == *alias) {
                symbols.push(CompletionSymbol {
                    name: alias.clone(),
                    kind: "namespace".to_string(),
                    detail: format!("namespace alias {} -> {}", alias, qualified),
                    documentation: Some(format!("命名空间别名: {} = {}", alias, qualified)),
                    insert_text: alias.clone(),
                });
            }
        }

        // 提取 namespace 路径信息
        for (_qualified_name, ns_path) in analyzer.type_registry().class_namespace_paths.iter() {
            if !ns_path.is_empty() {
                let ns_name = ns_path[0].clone();
                if !symbols.iter().any(|s| s.name == ns_name) {
                    symbols.push(CompletionSymbol {
                        name: ns_name.clone(),
                        kind: "namespace".to_string(),
                        detail: format!("namespace {}", ns_path.join("::")),
                        documentation: Some(format!("命名空间 {}", ns_path.join("::"))),
                        insert_text: ns_name.clone(),
                    });
                }
            }
        }

        // 处理 include 的文件（递归提取）
        let include_pattern =
            regex::Regex::new(r#"#include\s+["<]([^">]+)[">]"#).expect("正则表达式应始终有效");
        for cap in include_pattern.captures_iter(content) {
            if let Some(include_file) = cap.get(1) {
                let include_path = include_file.as_str();
                let base_dir = Path::new(file_path).parent().unwrap_or(Path::new("."));
                let full_path = base_dir.join(include_path);

                if let Ok(include_content) = std::fs::read_to_string(&full_path) {
                    self.extract_symbols_from_content_sync(
                        &include_content,
                        full_path.to_str().unwrap_or(include_path),
                        symbols,
                        processed_files,
                    );
                }
            }
        }
    }

    /// 获取 Hover 信息
    async fn get_hover_info(&self, content: &str, file_path: &str, word: &str) -> String {
        // 内置关键字文档
        let keyword_docs: std::collections::HashMap<&str, &str> = [
            ("public", "访问修饰符 - 公开的，任何地方都可以访问"),
            ("private", "访问修饰符 - 私有的，只在类内部可访问"),
            ("protected", "访问修饰符 - 保护的，类内部和子类可访问"),
            ("static", "修饰符 - 静态的，属于类而不是实例"),
            ("final", "修饰符 - 最终的，不可修改或继承"),
            ("class", "定义一个类"),
            ("interface", "定义一个接口"),
            ("extends", "继承 - 指定父类"),
            ("implements", "实现 - 指定实现的接口"),
            ("namespace", "命名空间 - 组织代码的逻辑容器"),
            ("using", "使用命名空间 - 导入命名空间中的符号"),
            ("void", "类型 - 无返回值"),
            ("int", "类型 - 32位整数"),
            ("long", "类型 - 64位整数"),
            ("float", "类型 - 32位浮点数"),
            ("double", "类型 - 64位浮点数"),
            ("bool", "类型 - 布尔值 (true/false)"),
            ("char", "类型 - 字符"),
            ("string", "类型 - 字符串"),
            ("var", "类型推断 - 自动推断变量类型"),
            ("auto", "类型推断 - 自动推断变量类型"),
            ("if", "条件语句 - 如果条件为真则执行"),
            ("else", "条件语句 - 如果条件为假则执行"),
            ("while", "循环语句 - 当条件为真时循环"),
            ("for", "循环语句 - 遍历或计数循环"),
            ("return", "返回语句 - 从方法返回值"),
            ("new", "创建实例 - 分配新对象"),
            ("this", "当前实例引用"),
            ("super", "父类引用"),
            ("null", "空值"),
            ("true", "布尔真值"),
            ("false", "布尔假值"),
        ]
        .iter()
        .cloned()
        .collect();

        if let Some(doc) = keyword_docs.get(word) {
            return format!("**`{}`**\n\n{}", word, doc);
        }

        // 内置函数文档
        let builtin_docs: std::collections::HashMap<&str, &str> = [
            ("println", "**`println(value)`**\n\n输出值到控制台并换行"),
            ("print", "**`print(value)`**\n\n输出值到控制台不换行"),
            (
                "printf",
                "**`printf(format, ...)`**\n\n格式化输出，类似 C 语言 printf",
            ),
            ("readInt", "**`readInt()`**\n\n从标准输入读取一个整数"),
            ("readLong", "**`readLong()`**\n\n从标准输入读取一个长整数"),
            ("readFloat", "**`readFloat()`**\n\n从标准输入读取一个浮点数"),
            (
                "readDouble",
                "**`readDouble()`**\n\n从标准输入读取一个双精度浮点数",
            ),
            ("readLine", "**`readLine()`**\n\n从标准输入读取一行字符串"),
            ("readChar", "**`readChar()`**\n\n从标准输入读取一个字符"),
            ("parseInt", "**`parseInt(string)`**\n\n将字符串转换为整数"),
            (
                "parseFloat",
                "**`parseFloat(string)`**\n\n将字符串转换为浮点数",
            ),
        ]
        .iter()
        .cloned()
        .collect();

        if let Some(doc) = builtin_docs.get(word) {
            return doc.to_string();
        }

        // 尝试从文档和 include 文件查找符号
        if let Some(info) = self.find_symbol_info(content, file_path, word).await {
            return format!(
                "**`{}`** *{}*\n\n```cavvy\n{}\n```\n\n{}",
                info.name, info.kind, info.signature, info.documentation
            );
        }

        // 默认返回
        format!("**`{}`**\n\nCavvy 标识符", word)
    }

    /// 查找符号信息
    async fn find_symbol_info(
        &self,
        content: &str,
        file_path: &str,
        word: &str,
    ) -> Option<HoverInfo> {
        // 预处理
        let (processed, _) = match self
            .preprocess_content_with_source_map(content, file_path)
            .await
        {
            Ok(p) => p,
            Err(_) => return None,
        };

        // 词法分析
        let tokens = match lexer::lex(&processed) {
            Ok(t) => t,
            Err(_) => return None,
        };

        // 语法分析
        let ast = match parser::parse(tokens) {
            Ok(a) => a,
            Err(_) => return None,
        };

        // 查找类
        for class in &ast.classes {
            if class.name == word {
                return Some(HoverInfo {
                    name: class.name.clone(),
                    kind: "class".to_string(),
                    signature: format!("class {}", class.name),
                    documentation: "用户定义的类".to_string(),
                });
            }

            // 查找类成员
            use cavvy::ast::ClassMember;
            for member in &class.members {
                match member {
                    ClassMember::Method(method) if method.name == word => {
                        let params: Vec<String> = method
                            .params
                            .iter()
                            .map(|p| format!("{:?} {}", p.param_type, p.name))
                            .collect();
                        return Some(HoverInfo {
                            name: method.name.clone(),
                            kind: "method".to_string(),
                            signature: format!(
                                "{:?} {}({})",
                                method.return_type,
                                method.name,
                                params.join(", ")
                            ),
                            documentation: "类方法".to_string(),
                        });
                    }
                    ClassMember::Field(field) if field.name == word => {
                        return Some(HoverInfo {
                            name: field.name.clone(),
                            kind: "field".to_string(),
                            signature: format!("{:?} {}", field.field_type, field.name),
                            documentation: "类字段".to_string(),
                        });
                    }
                    _ => {}
                }
            }
        }

        // 查找接口
        for interface in &ast.interfaces {
            if interface.name == word {
                return Some(HoverInfo {
                    name: interface.name.clone(),
                    kind: "interface".to_string(),
                    signature: format!("interface {}", interface.name),
                    documentation: "接口定义".to_string(),
                });
            }
        }

        None
    }

    /// 解析文档符号
    async fn parse_symbols(
        &self,
        content: &str,
        uri: &str,
    ) -> std::result::Result<Vec<DocumentSymbol>, String> {
        let file_path = uri.strip_prefix("file://").unwrap_or(uri);

        // 预处理
        let (processed, _) = match self
            .preprocess_content_with_source_map(content, file_path)
            .await
        {
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
fn error_to_diagnostic(error: &cavvy::miette_diagnostic::cayError, source: &str) -> Option<Diagnostic> {
    use cavvy::miette_diagnostic::cayError;

    let (message, line, column) = match error {
        cayError::Lexer {
            message,
            line,
            column,
            ..
        } => (message.clone(), *line, *column),
        cayError::Parser {
            message,
            line,
            column,
            ..
        } => (message.clone(), *line, *column),
        cayError::Semantic {
            message,
            line,
            column,
            ..
        } => (message.clone(), *line, *column),
        cayError::Preprocessor {
            message,
            line,
            column,
            ..
        } => (message.clone(), *line, *column),
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
    let line_len = lines
        .get(line.saturating_sub(1))
        .map(|l| l.len())
        .unwrap_or(0) as u32;

    Some(Diagnostic {
        range: Range {
            start: Position::new(
                line.saturating_sub(1) as u32,
                column.saturating_sub(1) as u32,
            ),
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

/// 将错误转换为 LSP 诊断信息（带源映射支持）
fn error_to_diagnostic_with_source_map(
    error: &cavvy::miette_diagnostic::cayError,
    _source: &str,
    source_map: &preprocessor::SourceMap,
    default_file: &str,
) -> Option<Diagnostic> {
    use cavvy::miette_diagnostic::cayError;

    let (message, line, column) = match error {
        cayError::Lexer {
            message,
            line,
            column,
            ..
        } => (message.clone(), *line, *column),
        cayError::Parser {
            message,
            line,
            column,
            ..
        } => (message.clone(), *line, *column),
        cayError::Semantic {
            message,
            line,
            column,
            ..
        } => (message.clone(), *line, *column),
        cayError::Preprocessor {
            message,
            line,
            column,
            ..
        } => (message.clone(), *line, *column),
        cayError::MultipleErrors { errors } => {
            // 对于 MultipleErrors，我们只返回第一个错误的诊断
            if let Some(first_error) = errors.first() {
                return error_to_diagnostic_with_source_map(
                    first_error,
                    _source,
                    source_map,
                    default_file,
                );
            }
            return None;
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

    // 使用源映射查找原始位置
    let (orig_file, orig_line, orig_column) =
        if let Some(pos) = source_map.get_source_position(line) {
            // 如果源映射指向不同的文件，创建相关诊断信息
            (pos.file.clone(), pos.line, column)
        } else {
            (default_file.to_string(), line, column)
        };

    // 尝试读取原始源文件以获取行长度
    let line_len = if let Ok(file_content) = std::fs::read_to_string(&orig_file) {
        let lines: Vec<&str> = file_content.lines().collect();
        lines
            .get(orig_line.saturating_sub(1))
            .map(|l| l.len())
            .unwrap_or(0) as u32
    } else {
        0u32
    };

    // 如果错误来自包含的文件，添加相关诊断信息
    let related_info = if orig_file != default_file {
        Some(vec![DiagnosticRelatedInformation {
            location: Location {
                uri: Url::from_file_path(&orig_file)
                    .unwrap_or_else(|_| Url::parse("file:///").expect("file:/// 是有效URL")),
                range: Range {
                    start: Position::new(
                        orig_line.saturating_sub(1) as u32,
                        orig_column.saturating_sub(1) as u32,
                    ),
                    end: Position::new(orig_line.saturating_sub(1) as u32, line_len),
                },
            },
            message: format!("在包含的文件中: {}", orig_file),
        }])
    } else {
        None
    };

    Some(Diagnostic {
        range: Range {
            start: Position::new(
                orig_line.saturating_sub(1) as u32,
                orig_column.saturating_sub(1) as u32,
            ),
            end: Position::new(orig_line.saturating_sub(1) as u32, line_len),
        },
        severity: Some(DiagnosticSeverity::ERROR),
        code: None,
        code_description: None,
        source: Some("cavvy".to_string()),
        message,
        related_information: related_info,
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

    // 处理命令行参数
    let args: Vec<String> = env::args().collect();

    // 处理 --version 参数
    if args.len() > 1 && (args[1] == "--version" || args[1] == "-v") {
        println!("cay-lsp {}", VERSION);
        return;
    }

    // 处理 --help 参数
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        println!("Cavvy Language Server Protocol (LSP) implementation");
        println!();
        println!("Usage: cay-lsp [OPTIONS]");
        println!();
        println!("Options:");
        println!("  -h, --help       Print help");
        println!("  -v, --version    Print version");
        println!();
        println!("This LSP server communicates via stdin/stdout using JSON-RPC protocol.");
        return;
    }

    // 创建 LSP 服务
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(|client| CavvyLanguageServer::new(client));

    // 运行服务器
    Server::new(stdin, stdout, socket).serve(service).await;
}
