//! `cay-lsp` 的 tower-lsp 服务实现。
//!
//! 设计目标：
//! - 请求处理只读取缓存好的 [`Analysis`]；写文档时做防抖后台分析；
//! - 所有编辑器功能都基于同一份语义模型（符号 / 引用 / 作用域 / 类型）；
//! - 内置类型与函数来自 `caylibs/__builtins__`，不在 LSP 中硬编码。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use dashmap::DashMap;
use tokio::time::sleep;
use tower_lsp::jsonrpc::{Error, Result};
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::types::Type;

use super::analysis::{Analysis, analyze_document, find_include_directives, same_file};
use super::builtins::{BuiltinIndex, BuiltinMember, BuiltinMemberKind, ParamSig, display_type};
use super::model::{OccurrenceKind, Symbol, SymbolKey, SymbolKind, base_type_name};
use super::util::{BytePos, Span, TextFile, caylibs_candidates, is_word_char, path_to_uri};
use super::workspace::{IndexedSymbol, WorkspaceIndex};

const VERSION: &str = env!("CAY_LSP_VERSION");

#[derive(Clone, Debug)]
pub struct Document {
    pub uri: Url,
    pub path: String,
    pub content: Arc<String>,
    pub version: i32,
    pub analysis: Option<Arc<Analysis>>,
}

#[derive(Clone, Debug)]
pub struct Settings {
    pub inlay_hints: bool,
    pub diagnostics: bool,
    pub workspace_index: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            inlay_hints: true,
            diagnostics: true,
            workspace_index: true,
        }
    }
}

pub struct ServerState {
    pub client: Client,
    pub documents: DashMap<String, Document>,
    pub builtins: RwLock<Arc<BuiltinIndex>>,
    pub workspace: Arc<WorkspaceIndex>,
    pub include_paths: RwLock<Vec<PathBuf>>,
    pub roots: RwLock<Vec<PathBuf>>,
    pub settings: RwLock<Settings>,
}

pub struct CavvyLanguageServer {
    pub state: Arc<ServerState>,
}

impl CavvyLanguageServer {
    pub fn new(client: Client) -> Self {
        let builtins = Arc::new(BuiltinIndex::load(&[]));
        let workspace = Arc::new(WorkspaceIndex::new(Vec::new(), builtins.clone()));
        Self {
            state: Arc::new(ServerState {
                client,
                documents: DashMap::new(),
                builtins: RwLock::new(builtins),
                workspace,
                include_paths: RwLock::new(Vec::new()),
                roots: RwLock::new(Vec::new()),
                settings: RwLock::new(Settings::default()),
            }),
        }
    }

    fn builtins(&self) -> Arc<BuiltinIndex> {
        self.state
            .builtins
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| Arc::new(BuiltinIndex::fallback()))
    }

    fn analysis(&self, uri: &str) -> Option<Arc<Analysis>> {
        analysis_for(&self.state, uri)
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for CavvyLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        let roots = workspace_roots(&params);
        if let Ok(mut guard) = self.state.roots.write() {
            *guard = roots.clone();
        }
        let builtins = Arc::new(BuiltinIndex::load(&roots));
        let include_paths = caylibs_candidates(&roots);
        let builtin_root = builtins.root.clone();
        let builtin_errors = builtins.errors.clone();
        if let Ok(mut guard) = self.state.builtins.write() {
            *guard = builtins.clone();
        }
        if let Ok(mut guard) = self.state.include_paths.write() {
            *guard = include_paths;
        }
        self.state.workspace.set_roots(roots.clone());
        self.apply_settings(params.initialization_options.as_ref());

        self.state
            .client
            .log_message(
                MessageType::INFO,
                format!(
                    "Cavvy LSP v{} 已启动；工作区 {} 个；内置存根: {}",
                    VERSION,
                    roots.len(),
                    builtin_root
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "<fallback>".to_string())
                ),
            )
            .await;
        for err in builtin_errors.iter().take(5) {
            self.state
                .client
                .log_message(MessageType::WARNING, format!("内置存根: {}", err))
                .await;
        }

        let capabilities = ServerCapabilities {
            position_encoding: Some(PositionEncodingKind::UTF16),
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
            selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec![
                    ".".to_string(),
                    ":".to_string(),
                    "(".to_string(),
                    "<".to_string(),
                    "\"".to_string(),
                    "/".to_string(),
                ]),
                ..Default::default()
            }),
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: Some(vec!["(".to_string(), ",".to_string()]),
                retrigger_characters: Some(vec![",".to_string()]),
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
            }),
            definition_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            document_highlight_provider: Some(OneOf::Left(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            workspace_symbol_provider: Some(OneOf::Left(true)),
            code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
                resolve_provider: Some(false),
            })),
            document_link_provider: Some(DocumentLinkOptions {
                resolve_provider: Some(false),
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
            }),
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: None,
                },
            })),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                    work_done_progress_options: WorkDoneProgressOptions {
                        work_done_progress: None,
                    },
                    legend: semantic_tokens_legend(),
                    range: Some(false),
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                }),
            ),
            inlay_hint_provider: Some(OneOf::Left(true)),
            workspace: Some(WorkspaceServerCapabilities {
                workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                    supported: Some(true),
                    change_notifications: Some(OneOf::Left(true)),
                }),
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
        self.state
            .client
            .log_message(MessageType::INFO, "Cavvy LSP 初始化完成")
            .await;
        let settings = self
            .state
            .settings
            .read()
            .map(|s| s.clone())
            .unwrap_or_default();
        if !settings.workspace_index {
            return;
        }
        let state = self.state.clone();
        tokio::spawn(async move {
            let workspace = state.workspace.clone();
            let _ = tokio::task::spawn_blocking(move || workspace.build()).await;
            let count = state.workspace.file_count();
            state
                .client
                .log_message(
                    MessageType::INFO,
                    format!("Cavvy 工作区索引完成：{} 个文件", count),
                )
                .await;
        });
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        let key = uri.to_string();
        let path = uri
            .to_file_path()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| uri.path().to_string());
        let content = Arc::new(params.text_document.text);
        let version = params.text_document.version;
        self.state.documents.insert(
            key.clone(),
            Document {
                uri,
                path,
                content,
                version,
                analysis: None,
            },
        );
        spawn_refresh(self.state.clone(), key, version, 0);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let key = params.text_document.uri.to_string();
        let version = params.text_document.version;
        if let Some(change) = params.content_changes.into_iter().last() {
            if let Some(mut doc) = self.state.documents.get_mut(&key) {
                doc.content = Arc::new(change.text);
                doc.version = version;
                doc.analysis = None;
            } else {
                let path = params
                    .text_document
                    .uri
                    .to_file_path()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| params.text_document.uri.path().to_string());
                self.state.documents.insert(
                    key.clone(),
                    Document {
                        uri: params.text_document.uri.clone(),
                        path,
                        content: Arc::new(change.text),
                        version,
                        analysis: None,
                    },
                );
            }
        }
        spawn_refresh(self.state.clone(), key, version, 140);
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let key = params.text_document.uri.to_string();
        let version = self
            .state
            .documents
            .get(&key)
            .map(|d| d.version)
            .unwrap_or(0);
        if let Some(doc) = self.state.documents.get(&key) {
            let path = PathBuf::from(&doc.path);
            let _ = self.state.workspace.index_file(&path);
        }
        spawn_refresh(self.state.clone(), key, version, 0);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let key = params.text_document.uri.to_string();
        self.state.documents.remove(&key);
        let _ = self
            .state
            .client
            .publish_diagnostics(params.text_document.uri, Vec::new(), None)
            .await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        for change in params.changes {
            let Ok(path) = change.uri.to_file_path() else {
                continue;
            };
            match change.typ {
                FileChangeType::DELETED => self.state.workspace.remove_file(&path),
                _ => {
                    let _ = self.state.workspace.index_file(&path);
                }
            }
            let key = change.uri.to_string();
            if let Some(doc) = self.state.documents.get(&key) {
                let version = doc.version;
                drop(doc);
                spawn_refresh(self.state.clone(), key, version, 0);
            }
        }
    }

    async fn did_change_configuration(&self, params: DidChangeConfigurationParams) {
        self.apply_settings(Some(&params.settings));
        self.state
            .client
            .log_message(MessageType::INFO, "Cavvy LSP 配置已更新")
            .await;
    }

    async fn did_change_workspace_folders(&self, params: DidChangeWorkspaceFoldersParams) {
        let mut roots = self
            .state
            .roots
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        for removed in params.event.removed {
            if let Ok(path) = removed.uri.to_file_path() {
                roots.retain(|root| root != &path);
            }
        }
        for added in params.event.added {
            if let Ok(path) = added.uri.to_file_path() {
                if !roots.contains(&path) {
                    roots.push(path);
                }
            }
        }
        if let Ok(mut guard) = self.state.roots.write() {
            *guard = roots.clone();
        }
        let builtins = Arc::new(BuiltinIndex::load(&roots));
        if let Ok(mut guard) = self.state.builtins.write() {
            *guard = builtins;
        }
        if let Ok(mut guard) = self.state.include_paths.write() {
            *guard = caylibs_candidates(&roots);
        }
        self.state.workspace.set_roots(roots);
        let state = self.state.clone();
        tokio::spawn(async move {
            let workspace = state.workspace.clone();
            let _ = tokio::task::spawn_blocking(move || workspace.build()).await;
            state
                .client
                .log_message(MessageType::INFO, "Cavvy 工作区索引已刷新")
                .await;
        });
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let Some(pos) = position_to_byte(&analysis, params.text_document_position_params.position)
        else {
            return Ok(None);
        };
        Ok(hover_impl(&analysis, pos))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let Some(pos) = position_to_byte(&analysis, params.text_document_position.position) else {
            return Ok(None);
        };
        let items = completion_impl(Some(&self.state), &analysis, pos);
        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let Some(pos) = position_to_byte(&analysis, params.text_document_position_params.position)
        else {
            return Ok(None);
        };
        Ok(signature_help_impl(&analysis, pos))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let Some(pos) = position_to_byte(&analysis, params.text_document_position_params.position)
        else {
            return Ok(None);
        };
        Ok(definition_impl(Some(&self.state), &analysis, pos))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let Some(pos) = position_to_byte(&analysis, params.text_document_position.position) else {
            return Ok(None);
        };
        Ok(Some(references_impl(
            &self.state,
            &analysis,
            pos,
            params.context.include_declaration,
        )))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let Some(pos) = position_to_byte(&analysis, params.position) else {
            return Ok(None);
        };
        let Some(occ) = analysis.model.occurrence_at_word(&analysis.path, pos) else {
            return Ok(None);
        };
        let Some(symbol) = occ.symbol.and_then(|id| analysis.model.symbol(id)) else {
            return Ok(None);
        };
        if is_builtin_key(&symbol.key) {
            return Err(Error::invalid_params("不能重命名语言内置符号"));
        }
        let text = analysis
            .main_text()
            .ok_or_else(|| Error::internal_error())?;
        Ok(Some(PrepareRenameResponse::RangeWithPlaceholder {
            range: text.range(occ.span),
            placeholder: symbol.name.clone(),
        }))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let Some(pos) = position_to_byte(&analysis, params.text_document_position.position) else {
            return Ok(None);
        };
        let Some(occ) = analysis.model.occurrence_at_word(&analysis.path, pos) else {
            return Ok(None);
        };
        let Some(symbol) = occ.symbol.and_then(|id| analysis.model.symbol(id)) else {
            return Ok(None);
        };
        if is_builtin_key(&symbol.key) {
            return Err(Error::invalid_params("不能重命名语言内置符号"));
        }
        if !is_valid_identifier(&params.new_name) {
            return Err(Error::invalid_params("新名称不是合法的 Cavvy 标识符"));
        }
        let locations = references_impl(&self.state, &analysis, pos, true);
        if locations.is_empty() {
            return Ok(None);
        }
        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        for loc in locations {
            changes
                .entry(loc.uri)
                .or_default()
                .push(TextEdit::new(loc.range, params.new_name.clone()));
        }
        Ok(Some(WorkspaceEdit {
            changes: Some(changes),
            document_changes: None,
            change_annotations: None,
        }))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params
            .text_document_position_params
            .text_document
            .uri
            .to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let Some(pos) = position_to_byte(&analysis, params.text_document_position_params.position)
        else {
            return Ok(None);
        };
        Ok(document_highlight_impl(&analysis, pos))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let symbols = analysis.model.document_symbols(&analysis.path);
        if !symbols.is_empty() {
            return Ok(Some(DocumentSymbolResponse::Nested(symbols)));
        }
        // 解析失败时仍尽量给出大纲（基于轻量 token 扫描）。
        if !analysis.parse_ok {
            let Some(text) = analysis.main_text() else {
                return Ok(None);
            };
            let indexed = super::workspace::scan_file_symbols(text, &analysis.content);
            let fallback: Vec<DocumentSymbol> = indexed
                .into_iter()
                .map(|s| DocumentSymbol {
                    name: s.name,
                    detail: Some(s.detail),
                    kind: s.kind.lsp(),
                    tags: None,
                    deprecated: None,
                    range: text.range(s.location.span),
                    selection_range: text.range(s.location.span),
                    children: None,
                })
                .collect();
            if !fallback.is_empty() {
                return Ok(Some(DocumentSymbolResponse::Nested(fallback)));
            }
        }
        Ok(None)
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query.trim().to_string();
        let mut out: Vec<SymbolInformation> = Vec::new();
        let mut seen = HashSet::new();

        let open_uris: Vec<String> = self
            .state
            .documents
            .iter()
            .map(|e| e.key().clone())
            .collect();
        for uri in open_uris {
            let Some(analysis) = self.analysis(&uri) else {
                continue;
            };
            for sym in &analysis.model.symbols {
                if !matches!(
                    sym.kind,
                    SymbolKind::Class
                        | SymbolKind::Interface
                        | SymbolKind::Struct
                        | SymbolKind::Enum
                        | SymbolKind::Function
                        | SymbolKind::Method
                ) {
                    continue;
                }
                if !query.is_empty() && !fuzzy_name_match(&sym.name, &query) {
                    continue;
                }
                let uri = path_to_uri(&sym.location.file);
                let key = (sym.name.clone(), uri.to_string(), sym.selection.span);
                if !seen.insert(key) {
                    continue;
                }
                if let Some(text) = analysis.model.files.get(&sym.location.file) {
                    out.push(SymbolInformation {
                        name: sym.name.clone(),
                        kind: sym.kind.lsp(),
                        tags: None,
                        deprecated: None,
                        location: Location {
                            uri,
                            range: text.range(sym.selection.span),
                        },
                        container_name: sym.owner.clone(),
                    });
                }
            }
        }

        for indexed in self.state.workspace.symbols_matching(&query) {
            let key = (
                indexed.name.clone(),
                indexed.location.file.clone(),
                indexed.location.span,
            );
            if !seen.insert(key) {
                continue;
            }
            out.push(symbol_information_from_index(&indexed));
        }

        let builtins = self.builtins();
        for (name, bt) in builtins.all_type_names() {
            if !query.is_empty() && !fuzzy_name_match(name, &query) {
                continue;
            }
            let key = (name.clone(), bt.file.clone(), bt.location.span);
            if !seen.insert(key) {
                continue;
            }
            let uri = path_to_uri(&bt.file);
            let range = range_for_file(&bt.file, bt.location.span);
            out.push(SymbolInformation {
                name: name.clone(),
                kind: SymbolKind::Class.lsp(),
                tags: None,
                deprecated: None,
                location: Location { uri, range },
                container_name: Some("__builtins__".to_string()),
            });
        }
        for func in builtins.global_functions() {
            if !query.is_empty() && !fuzzy_name_match(&func.name, &query) {
                continue;
            }
            let key = (
                func.name.clone(),
                func.location.file.clone(),
                func.location.span,
            );
            if !seen.insert(key) {
                continue;
            }
            out.push(SymbolInformation {
                name: func.name.clone(),
                kind: SymbolKind::Function.lsp(),
                tags: None,
                deprecated: None,
                location: Location {
                    uri: path_to_uri(&func.location.file),
                    range: range_for_file(&func.location.file, func.location.span),
                },
                container_name: Some("Builtins".to_string()),
            });
        }

        out.sort_by(|a, b| a.name.cmp(&b.name));
        out.truncate(500);
        Ok(Some(out))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        Ok(Some(SemanticTokensResult::Tokens(semantic_tokens_impl(
            &analysis,
        ))))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let uri = params.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        Ok(Some(folding_impl(&analysis)))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        Ok(code_action_impl(&analysis, &params.context.diagnostics))
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let settings = self
            .state
            .settings
            .read()
            .map(|s| s.clone())
            .unwrap_or_default();
        if !settings.inlay_hints {
            return Ok(None);
        }
        Ok(Some(inlay_hints_impl(&analysis, params.range)))
    }

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        Ok(Some(document_links_impl(&analysis)))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let uri = params.text_document.uri.to_string();
        let Some(analysis) = self.analysis(&uri) else {
            return Ok(None);
        };
        let mut out = Vec::new();
        for position in params.positions {
            if let Some(pos) = position_to_byte(&analysis, position) {
                out.push(selection_range_impl(&analysis, pos));
            }
        }
        if out.is_empty() {
            Ok(None)
        } else {
            Ok(Some(out))
        }
    }
}

impl CavvyLanguageServer {
    fn apply_settings(&self, value: Option<&serde_json::Value>) {
        let Some(value) = value else {
            return;
        };
        let value = value.get("cavvy").unwrap_or(value);
        let mut settings = self
            .state
            .settings
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(v) = value.get("inlayHints").and_then(|v| v.as_bool()) {
            settings.inlay_hints = v;
        }
        if let Some(v) = value.get("diagnostics").and_then(|v| v.as_bool()) {
            settings.diagnostics = v;
        }
        if let Some(v) = value.get("workspaceIndex").and_then(|v| v.as_bool()) {
            settings.workspace_index = v;
        }
    }
}

// ---------------------------------------------------------------------------
// 文档缓存 / 分析
// ---------------------------------------------------------------------------

fn workspace_roots(params: &InitializeParams) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(folders) = &params.workspace_folders {
        for folder in folders {
            if let Ok(path) = folder.uri.to_file_path() {
                if !roots.contains(&path) {
                    roots.push(path);
                }
            }
        }
    }
    if roots.is_empty() {
        if let Some(root) = &params.root_uri {
            if let Ok(path) = root.to_file_path() {
                roots.push(path);
            }
        }
    }
    if roots.is_empty() {
        if let Ok(cwd) = std::env::current_dir() {
            roots.push(cwd);
        }
    }
    roots
}

fn collect_overlays(state: &Arc<ServerState>) -> HashMap<String, Arc<String>> {
    let mut overlays = HashMap::new();
    for entry in state.documents.iter() {
        overlays.insert(entry.path.clone(), entry.content.clone());
    }
    overlays
}

fn analysis_for(state: &Arc<ServerState>, uri: &str) -> Option<Arc<Analysis>> {
    let doc = state.documents.get(uri)?.clone();
    if let Some(existing) = &doc.analysis {
        if existing.version == doc.version && existing.content.as_str() == doc.content.as_str() {
            return Some(existing.clone());
        }
    }
    let builtins = state
        .builtins
        .read()
        .map(|g| g.clone())
        .unwrap_or_else(|_| Arc::new(BuiltinIndex::fallback()));
    let include_paths = state
        .include_paths
        .read()
        .map(|g| g.clone())
        .unwrap_or_default();
    let overlays = collect_overlays(state);
    let analysis = Arc::new(analyze_document(
        &doc.uri.to_string(),
        &doc.path,
        &doc.content,
        doc.version,
        builtins,
        &include_paths,
        &overlays,
    ));
    if let Some(mut current) = state.documents.get_mut(uri) {
        if current.version == doc.version {
            current.analysis = Some(analysis.clone());
        }
    }
    Some(analysis)
}

fn spawn_refresh(state: Arc<ServerState>, uri: String, version: i32, delay_ms: u64) {
    tokio::spawn(async move {
        if delay_ms > 0 {
            sleep(Duration::from_millis(delay_ms)).await;
        }
        let Some(doc) = state.documents.get(&uri).map(|d| d.clone()) else {
            return;
        };
        if doc.version != version {
            return;
        }
        let builtins = state
            .builtins
            .read()
            .map(|g| g.clone())
            .unwrap_or_else(|_| Arc::new(BuiltinIndex::fallback()));
        let include_paths = state
            .include_paths
            .read()
            .map(|g| g.clone())
            .unwrap_or_default();
        let overlays = collect_overlays(&state);
        let path = doc.path.clone();
        let content = (*doc.content).clone();
        let uri_for_analysis = uri.clone();
        let result = tokio::task::spawn_blocking(move || {
            analyze_document(
                &uri_for_analysis,
                &path,
                &content,
                version,
                builtins,
                &include_paths,
                &overlays,
            )
        })
        .await;
        let Ok(analysis) = result else {
            return;
        };
        let still_current = state
            .documents
            .get(&uri)
            .map(|d| d.version == version)
            .unwrap_or(false);
        if !still_current {
            return;
        }
        let analysis = Arc::new(analysis);
        if let Some(mut current) = state.documents.get_mut(&uri) {
            current.analysis = Some(analysis.clone());
        }
        let diagnostics_enabled = state.settings.read().map(|s| s.diagnostics).unwrap_or(true);
        if diagnostics_enabled {
            publish_analysis_diagnostics(&state, &analysis, version).await;
        }
    });
}

async fn publish_analysis_diagnostics(state: &Arc<ServerState>, analysis: &Analysis, version: i32) {
    let mut main_diagnostics = Vec::new();
    let mut other: HashMap<String, Vec<Diagnostic>> = HashMap::new();
    for (file, diagnostic) in &analysis.diagnostic_records {
        if same_file(file, &analysis.path) {
            main_diagnostics.push(diagnostic.clone());
        } else {
            other
                .entry(file.clone())
                .or_default()
                .push(diagnostic.clone());
        }
    }
    let main_uri = path_to_uri(&analysis.path);
    let _ = state
        .client
        .publish_diagnostics(main_uri, main_diagnostics, Some(version))
        .await;
    for (file, diagnostics) in other {
        // 只向已打开的文件推送诊断，避免污染未打开文档。
        let Some(doc) = state
            .documents
            .iter()
            .find(|d| same_file(&d.path, &file))
            .map(|d| d.uri.clone())
        else {
            continue;
        };
        let _ = state
            .client
            .publish_diagnostics(doc, diagnostics, None)
            .await;
    }
}

// ---------------------------------------------------------------------------
// 位置 / 文本工具
// ---------------------------------------------------------------------------

fn position_to_byte(analysis: &Analysis, position: Position) -> Option<BytePos> {
    let text = analysis.main_text()?;
    if text.line_count() == 0 {
        return None;
    }
    let line = position.line.min(text.line_count().saturating_sub(1));
    let col = text.utf16_to_byte_col(line, position.character);
    Some(BytePos::new(line, col))
}

fn range_for_file(file: &str, span: Span) -> Range {
    // 没有文本时退化为零宽范围；调用方通常有文本。
    let _ = file;
    Range::new(
        Position::new(span.start.line, span.start.col),
        Position::new(span.end.line, span.end.col),
    )
}

fn range_in(analysis: &Analysis, file: &str, span: Span) -> Range {
    analysis
        .files
        .get(file)
        .map(|t| t.range(span))
        .unwrap_or_else(|| range_for_file(file, span))
}

fn location_in(analysis: &Analysis, file: &str, span: Span) -> Location {
    Location {
        uri: path_to_uri(file),
        range: range_in(analysis, file, span),
    }
}

fn safe_line_col(text: &TextFile, pos: BytePos) -> usize {
    let utf16 = text.byte_col_to_utf16(pos.line, pos.col);
    text.utf16_to_byte_col(pos.line, utf16) as usize
}

fn prefix_and_range(text: &TextFile, pos: BytePos) -> (String, Range, BytePos) {
    let line = text.line(pos.line);
    let end = safe_line_col(text, pos).min(line.len());
    let mut start = end;
    while start > 0 {
        let prev = prev_boundary(line, start);
        let ch = line[prev..start].chars().next().unwrap_or(' ');
        if is_word_char(ch) {
            start = prev;
        } else {
            break;
        }
    }
    let prefix = line[start..end].to_string();
    let range = text.range(Span::new(
        BytePos::new(pos.line, start as u32),
        BytePos::new(pos.line, end as u32),
    ));
    (prefix, range, BytePos::new(pos.line, start as u32))
}

fn prev_boundary(s: &str, mut idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    idx -= 1;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn is_in_comment_or_string(text: &TextFile, pos: BytePos) -> bool {
    let offset = text.offset(pos);
    let mut state = 0u8; // 0 normal, 1 line comment, 2 block comment, 3 string, 4 char
    let mut escaped = false;
    let mut chars = text.text[..offset].chars().peekable();
    while let Some(ch) = chars.next() {
        match state {
            0 => match ch {
                '/' if matches!(chars.peek(), Some('/')) => {
                    chars.next();
                    state = 1;
                }
                '/' if matches!(chars.peek(), Some('*')) => {
                    chars.next();
                    state = 2;
                }
                '"' => state = 3,
                '\'' => state = 4,
                _ => {}
            },
            1 => {
                if ch == '\n' {
                    state = 0;
                }
            }
            2 => {
                if ch == '*' && matches!(chars.peek(), Some('/')) {
                    chars.next();
                    state = 0;
                }
            }
            3 => {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '"' {
                    state = 0;
                }
            }
            4 => {
                if escaped {
                    escaped = false;
                } else if ch == '\\' {
                    escaped = true;
                } else if ch == '\'' {
                    state = 0;
                }
            }
            _ => {}
        }
    }
    state != 0
}

// ---------------------------------------------------------------------------
// Hover
// ---------------------------------------------------------------------------

fn hover_impl(analysis: &Analysis, pos: BytePos) -> Option<Hover> {
    let occ = analysis.model.occurrence_at_word(&analysis.path, pos);
    if let Some(occ) = occ {
        if let Some(symbol) = occ.symbol.and_then(|id| analysis.model.symbol(id)) {
            let value = render_symbol(analysis, symbol);
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value,
                }),
                range: analysis.main_text().map(|t| t.range(occ.span)),
            });
        }
        if let Some(ty) = &occ.result {
            let word = occ.name.clone();
            let value = format!("**`{}`**\n\n类型：`{}`", word, display_type(ty));
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value,
                }),
                range: analysis.main_text().map(|t| t.range(occ.span)),
            });
        }
    }

    // 没有符号时，尝试关键字 / 内置类型 / 内置函数文档。
    let (span, word) = analysis.main_text()?.word_at(pos)?;
    let mut docs = Vec::new();
    if let Some(doc) = keyword_doc(&word) {
        docs.push(doc.to_string());
    }
    let builtins = &analysis.builtins;
    if let Some(bt) = builtins.get_by_name(&word) {
        docs.push(format!(
            "**内建类型 `{}`**\n\n{}",
            bt.canonical,
            if bt.documentation.is_empty() {
                format!("对应存根 `{}`。", bt.stub_name)
            } else {
                bt.documentation.clone()
            }
        ));
    }
    if let Some(func) = builtins.global_function_symbol(&word) {
        docs.push(format!(
            "```cavvy\n{}\n```\n\n{}",
            func.label, func.documentation
        ));
    }
    if docs.is_empty() {
        return None;
    }
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: docs.join("\n\n---\n\n"),
        }),
        range: analysis.main_text().map(|t| t.range(span)),
    })
}

fn render_symbol(analysis: &Analysis, symbol: &Symbol) -> String {
    let mut out = String::new();
    let signature = if symbol.signature.is_empty() {
        symbol.detail.clone()
    } else {
        symbol.signature.clone()
    };
    out.push_str("```cavvy\n");
    out.push_str(signature.trim());
    out.push_str("\n```\n\n");
    let kind = match symbol.kind {
        SymbolKind::Class => "类",
        SymbolKind::Interface => "接口",
        SymbolKind::Struct => "结构体",
        SymbolKind::Enum => "枚举",
        SymbolKind::EnumVariant => "枚举成员",
        SymbolKind::Method => "方法",
        SymbolKind::Constructor => "构造函数",
        SymbolKind::Field => "字段",
        SymbolKind::Function => "函数",
        SymbolKind::Local => "局部变量",
        SymbolKind::Parameter => "参数",
        SymbolKind::BuiltinType => "内建类型",
        SymbolKind::BuiltinMember => "内建成员",
    };
    out.push_str(&format!("*{}*", kind));
    if let Some(owner) = &symbol.owner {
        out.push_str(&format!("，定义于 `{}`", owner));
    }
    if let Some(ty) = &symbol.ty {
        if !matches!(ty, Type::Function(_)) {
            out.push_str(&format!("\n\n类型：`{}`", display_type(ty)));
        }
    }
    if !symbol.documentation.trim().is_empty() {
        out.push_str("\n\n");
        out.push_str(symbol.documentation.trim());
    }
    let _ = analysis;
    out
}

fn keyword_doc(word: &str) -> Option<&'static str> {
    Some(match word {
        "public" => "访问修饰符：公开的，任何地方都可以访问。",
        "private" => "访问修饰符：私有的，只在类内部可访问。",
        "protected" => "访问修饰符：受保护的，类内部和子类可访问。",
        "static" => "修饰符：静态成员，属于类而不是实例。",
        "final" => "修饰符：不可修改/不可继承。",
        "abstract" => "修饰符：抽象类或抽象方法。",
        "native" => "修饰符：声明由外部或运行时实现的方法。",
        "class" => "定义一个类。",
        "interface" => "定义一个接口。",
        "struct" => "定义一个值类型结构体。",
        "enum" => "定义一个枚举（可携带 payload）。",
        "extends" => "继承：指定父类。",
        "implements" => "实现：指定接口。",
        "namespace" => "命名空间：组织代码的逻辑容器。",
        "using" => "导入命名空间中的符号。",
        "void" => "类型：无返回值。",
        "int" => "32 位有符号整数类型。",
        "long" => "64 位有符号整数类型。",
        "float" => "32 位单精度浮点类型。",
        "double" => "64 位双精度浮点类型。",
        "bool" => "布尔类型（true / false）。",
        "char" => "字符类型。",
        "string" | "String" => "内建字符串类型。",
        "var" | "let" | "auto" => "类型推断：由初始化表达式推断变量类型。",
        "if" => "条件语句 / 表达式。",
        "else" => "条件不成立时执行的分支。",
        "while" => "while 循环。",
        "for" => "for 循环。",
        "do" => "do-while 循环。",
        "switch" => "switch 分支语句。",
        "case" => "switch 的 case 分支。",
        "default" => "switch 的默认分支。",
        "break" => "跳出循环。",
        "continue" => "继续下一次循环。",
        "return" => "从函数/方法返回。",
        "new" => "创建对象或数组。",
        "this" => "当前实例引用。",
        "super" => "父类引用。",
        "instanceof" => "运行时类型检查。",
        "extern" => "声明外部（FFI）函数。",
        "true" => "布尔真值。",
        "false" => "布尔假值。",
        "null" => "空引用。",
        "scope" => "栈作用域分配块。",
        "typeof" => "编译期类型查询。",
        "sizeof" => "编译期大小查询。",
        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Completion
// ---------------------------------------------------------------------------

enum CompletionContext {
    Include {
        prefix: String,
        range: Range,
    },
    Member {
        prefix: String,
        range: Range,
        receiver: Option<Type>,
        static_name: Option<String>,
    },
    General {
        prefix: String,
        range: Range,
    },
}

fn completion_impl(
    state: Option<&Arc<ServerState>>,
    analysis: &Analysis,
    pos: BytePos,
) -> Vec<CompletionItem> {
    let Some(text) = analysis.main_text() else {
        return Vec::new();
    };
    if is_in_comment_or_string(text, pos) {
        // 字符串里的 `#include` 由 include 上下文单独处理。
        if !inside_include_directive(text, pos) {
            return Vec::new();
        }
    }
    let ctx = completion_context(analysis, text, pos);
    let mut items = match ctx {
        CompletionContext::Include { prefix, range } => {
            include_completion_items(analysis, &prefix, range)
        }
        CompletionContext::Member {
            prefix,
            range,
            receiver,
            static_name,
        } => {
            if let Some(name) = static_name {
                static_completion_items(analysis, &name, &prefix, range)
            } else if let Some(ty) = receiver {
                type_completion_items(analysis, &ty, &prefix, range)
            } else {
                let mut items = general_completion_items(analysis, &prefix, range, pos);
                if let Some(state) = state {
                    items.extend(workspace_completion_items(state, &prefix, range, 120));
                }
                items
            }
        }
        CompletionContext::General { prefix, range } => {
            let mut items = general_completion_items(analysis, &prefix, range, pos);
            if let Some(state) = state {
                items.extend(workspace_completion_items(state, &prefix, range, 200));
            }
            items
        }
    };
    dedup_completion_items(&mut items);
    items
}

fn workspace_completion_items(
    state: &Arc<ServerState>,
    prefix: &str,
    range: Range,
    limit: usize,
) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    for indexed in state
        .workspace
        .symbols_matching(prefix)
        .into_iter()
        .take(limit)
    {
        let kind = match indexed.kind {
            SymbolKind::Class => CompletionItemKind::CLASS,
            SymbolKind::Interface => CompletionItemKind::INTERFACE,
            SymbolKind::Struct => CompletionItemKind::STRUCT,
            SymbolKind::Enum => CompletionItemKind::ENUM,
            SymbolKind::EnumVariant => CompletionItemKind::ENUM_MEMBER,
            SymbolKind::Method | SymbolKind::Constructor => CompletionItemKind::METHOD,
            SymbolKind::Field => CompletionItemKind::FIELD,
            SymbolKind::Function => CompletionItemKind::FUNCTION,
            _ => CompletionItemKind::TEXT,
        };
        out.push(completion_item(
            indexed.name.clone(),
            kind,
            indexed.detail.clone(),
            String::new(),
            indexed.name.clone(),
            range,
            "3",
            false,
        ));
    }
    out
}

fn dedup_completion_items(items: &mut Vec<CompletionItem>) {
    let mut seen = HashSet::new();
    items.retain(|item| {
        let key = (item.label.clone(), item.kind.map(|k| format!("{:?}", k)));
        seen.insert(key)
    });
}

fn inside_include_directive(text: &TextFile, pos: BytePos) -> bool {
    let line = text.line(pos.line);
    let cursor = safe_line_col(text, pos).min(line.len());
    let before = &line[..cursor];
    if let Some(hash) = before.find('#') {
        let rest = &before[hash + 1..];
        return rest.trim_start().starts_with("include");
    }
    false
}

fn completion_context(analysis: &Analysis, text: &TextFile, pos: BytePos) -> CompletionContext {
    let line = text.line(pos.line);
    let cursor = safe_line_col(text, pos).min(line.len());
    let before = &line[..cursor];

    // #include 路径补全。
    if let Some(hash) = before.rfind('#') {
        let rest = &before[hash + 1..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with("include") {
            let after = hash + 1 + (rest.len() - trimmed.len()) + "include".len();
            let tail = &line[after..cursor];
            let tail_trimmed = tail.trim_start();
            if let Some(quote) = tail_trimmed.chars().next() {
                if quote == '"' || quote == '<' {
                    let qpos = after + (tail.len() - tail_trimmed.len());
                    let path_start = qpos + quote.len_utf8();
                    let prefix = line[path_start..cursor].to_string();
                    let range = text.range(Span::new(
                        BytePos::new(pos.line, path_start as u32),
                        BytePos::new(pos.line, cursor as u32),
                    ));
                    return CompletionContext::Include { prefix, range };
                }
            }
        }
    }

    let (prefix, range, prefix_start) = prefix_and_range(text, pos);
    // 找到前缀之前的非空白字符。
    let mut idx = (prefix_start.col as usize).min(line.len());
    while idx > 0 {
        let prev = prev_boundary(line, idx);
        let ch = line[prev..idx].chars().next().unwrap_or(' ');
        if ch.is_whitespace() {
            idx = prev;
        } else {
            break;
        }
    }
    if idx >= 2 {
        let two_start = prev_boundary(line, idx);
        let two = &line[two_start..idx];
        if two == "::" {
            let dot = BytePos::new(pos.line, two_start as u32);
            let static_name = identifier_before(text, dot);
            return CompletionContext::Member {
                prefix,
                range,
                receiver: None,
                static_name,
            };
        }
    }
    if idx > 0 {
        let dot_start = prev_boundary(line, idx);
        let dot_ch = line[dot_start..idx].chars().next().unwrap_or(' ');
        if dot_ch == '.' {
            let dot = BytePos::new(pos.line, dot_start as u32);
            let receiver = analysis.model.receiver_type_before(&analysis.path, dot);
            let static_name = if receiver.is_none() {
                identifier_before(text, dot)
            } else {
                None
            };
            return CompletionContext::Member {
                prefix,
                range,
                receiver,
                static_name,
            };
        }
    }

    CompletionContext::General { prefix, range }
}

fn identifier_before(text: &TextFile, pos: BytePos) -> Option<String> {
    let line = text.line(pos.line);
    let mut end = safe_line_col(text, pos).min(line.len());
    while end > 0 {
        let prev = prev_boundary(line, end);
        let ch = line[prev..end].chars().next().unwrap_or(' ');
        if ch.is_whitespace() {
            end = prev;
        } else {
            break;
        }
    }
    let mut start = end;
    while start > 0 {
        let prev = prev_boundary(line, start);
        let ch = line[prev..start].chars().next().unwrap_or(' ');
        if is_word_char(ch) {
            start = prev;
        } else {
            break;
        }
    }
    if start < end {
        Some(line[start..end].to_string())
    } else {
        None
    }
}

fn include_completion_items(
    analysis: &Analysis,
    prefix: &str,
    range: Range,
) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let base = Path::new(&analysis.path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_default();

    let mut dirs = vec![base];
    dirs.extend(analysis.include_paths.iter().cloned());
    for dir in dirs {
        for entry in walk_cay_files(&dir, 0) {
            let rel = entry
                .strip_prefix(&dir)
                .map(|p| p.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|_| entry.to_string_lossy().replace('\\', "/"));
            if !prefix.is_empty() && !rel.to_lowercase().starts_with(&prefix.to_lowercase()) {
                continue;
            }
            if !seen.insert(rel.clone()) {
                continue;
            }
            out.push(completion_item(
                rel.clone(),
                CompletionItemKind::FILE,
                format!("包含 {}", rel),
                String::new(),
                rel,
                range,
                "0",
                false,
            ));
        }
    }
    out
}

fn walk_cay_files(dir: &Path, depth: usize) -> Vec<PathBuf> {
    if depth > 12 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || name == "target" || name == "node_modules" {
                continue;
            }
            out.extend(walk_cay_files(&path, depth + 1));
        } else if path.extension().map(|e| e == "cay").unwrap_or(false) {
            out.push(path);
        }
    }
    out.sort();
    out
}

fn type_completion_items(
    analysis: &Analysis,
    ty: &Type,
    prefix: &str,
    range: Range,
) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    // 内建成员（String / array / 基本类型 / Object）。
    for member in analysis.builtins.members_for(ty) {
        if member.is_static {
            continue;
        }
        if !member
            .name
            .to_lowercase()
            .starts_with(&prefix.to_lowercase())
        {
            continue;
        }
        if !member.is_method() && member.kind == BuiltinMemberKind::Method {
            continue;
        }
        if !seen.insert((member.name.clone(), member.is_method())) {
            continue;
        }
        let item = builtin_completion_item(&member, range);
        out.push(item);
    }

    // 用户类 / struct / enum。
    match ty {
        Type::Object(name) | Type::Generic(name, _) => {
            let base = name.split('<').next().unwrap_or(name).trim();
            let chain = class_chain(&analysis.registry, base);
            for info in chain {
                for (fname, field) in &info.fields {
                    if field.is_static {
                        continue;
                    }
                    if !fname.to_lowercase().starts_with(&prefix.to_lowercase()) {
                        continue;
                    }
                    if !seen.insert((fname.clone(), false)) {
                        continue;
                    }
                    let symbol = analysis
                        .model
                        .field_symbols(&info.name, fname)
                        .into_iter()
                        .next();
                    let doc = symbol.map(|s| s.documentation.clone()).unwrap_or_default();
                    out.push(completion_item(
                        fname.clone(),
                        CompletionItemKind::FIELD,
                        format!("{} {}", display_type(&field.field_type), fname),
                        doc,
                        fname.clone(),
                        range,
                        "1",
                        false,
                    ));
                }
                for (mname, methods) in &info.methods {
                    if !mname.to_lowercase().starts_with(&prefix.to_lowercase()) {
                        continue;
                    }
                    for method in methods.iter() {
                        if method.is_static {
                            continue;
                        }
                        if !seen.insert((mname.clone(), true)) {
                            continue;
                        }
                        let params: Vec<ParamSig> = method
                            .params
                            .iter()
                            .map(|p| ParamSig {
                                name: p.name.clone(),
                                ty: p.param_type.clone(),
                                is_varargs: p.is_varargs,
                            })
                            .collect();
                        let signature = format!(
                            "{} {}({})",
                            display_type(&method.return_type),
                            mname,
                            params
                                .iter()
                                .map(|p| p.display())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        let symbol = analysis
                            .model
                            .method_symbols(&info.name, mname)
                            .into_iter()
                            .next();
                        let doc = symbol.map(|s| s.documentation.clone()).unwrap_or_default();
                        let insert = method_snippet(mname, &params);
                        out.push(completion_item(
                            mname.clone(),
                            CompletionItemKind::METHOD,
                            signature,
                            doc,
                            insert,
                            range,
                            "2",
                            true,
                        ));
                    }
                }
            }
        }
        Type::Struct(name) => {
            if let Some(info) = analysis.registry.get_struct(name) {
                for (fname, field) in &info.fields {
                    if field.is_static {
                        continue;
                    }
                    if !fname.to_lowercase().starts_with(&prefix.to_lowercase()) {
                        continue;
                    }
                    if !seen.insert((fname.clone(), false)) {
                        continue;
                    }
                    out.push(completion_item(
                        fname.clone(),
                        CompletionItemKind::FIELD,
                        format!("{} {}", display_type(&field.field_type), fname),
                        String::new(),
                        fname.clone(),
                        range,
                        "1",
                        false,
                    ));
                }
                for (mname, methods) in &info.methods {
                    for method in methods {
                        if method.is_static {
                            continue;
                        }
                        if !mname.to_lowercase().starts_with(&prefix.to_lowercase()) {
                            continue;
                        }
                        if !seen.insert((mname.clone(), true)) {
                            continue;
                        }
                        let params: Vec<ParamSig> = method
                            .params
                            .iter()
                            .map(|p| ParamSig {
                                name: p.name.clone(),
                                ty: p.param_type.clone(),
                                is_varargs: p.is_varargs,
                            })
                            .collect();
                        let signature = format!(
                            "{} {}({})",
                            display_type(&method.return_type),
                            mname,
                            params
                                .iter()
                                .map(|p| p.display())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        out.push(completion_item(
                            mname.clone(),
                            CompletionItemKind::METHOD,
                            signature,
                            String::new(),
                            method_snippet(mname, &params),
                            range,
                            "2",
                            true,
                        ));
                    }
                }
            }
        }
        Type::GenericParam(bound) => {
            out.extend(type_completion_items(
                analysis,
                &Type::Object(bound.clone()),
                prefix,
                range,
            ));
        }
        _ => {}
    }
    out
}

fn static_completion_items(
    analysis: &Analysis,
    class_name: &str,
    prefix: &str,
    range: Range,
) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let base = class_name.split('<').next().unwrap_or(class_name).trim();

    if let Some(info) = analysis.registry.get_class(base) {
        for (fname, field) in &info.fields {
            if !field.is_static || !fname.to_lowercase().starts_with(&prefix.to_lowercase()) {
                continue;
            }
            if !seen.insert(fname.clone()) {
                continue;
            }
            out.push(completion_item(
                fname.clone(),
                CompletionItemKind::FIELD,
                format!("static {} {}", display_type(&field.field_type), fname),
                String::new(),
                fname.clone(),
                range,
                "1",
                false,
            ));
        }
        for (mname, methods) in &info.methods {
            for method in methods {
                if !method.is_static || !mname.to_lowercase().starts_with(&prefix.to_lowercase()) {
                    continue;
                }
                if !seen.insert(mname.clone()) {
                    continue;
                }
                let params: Vec<ParamSig> = method
                    .params
                    .iter()
                    .map(|p| ParamSig {
                        name: p.name.clone(),
                        ty: p.param_type.clone(),
                        is_varargs: p.is_varargs,
                    })
                    .collect();
                let signature = format!(
                    "static {} {}({})",
                    display_type(&method.return_type),
                    mname,
                    params
                        .iter()
                        .map(|p| p.display())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                out.push(completion_item(
                    mname.clone(),
                    CompletionItemKind::METHOD,
                    signature,
                    String::new(),
                    method_snippet(mname, &params),
                    range,
                    "2",
                    true,
                ));
            }
        }
    }
    if let Some(en) = analysis.registry.get_enum(base) {
        for variant in &en.variants {
            if !variant
                .name
                .to_lowercase()
                .starts_with(&prefix.to_lowercase())
            {
                continue;
            }
            if !seen.insert(variant.name.clone()) {
                continue;
            }
            out.push(completion_item(
                variant.name.clone(),
                CompletionItemKind::ENUM_MEMBER,
                format!("{}.{}", base, variant.name),
                String::new(),
                variant.name.clone(),
                range,
                "1",
                false,
            ));
        }
    }
    for member in analysis.builtins.static_members_for(base) {
        if !member
            .name
            .to_lowercase()
            .starts_with(&prefix.to_lowercase())
        {
            continue;
        }
        if !seen.insert(member.name.clone()) {
            continue;
        }
        out.push(builtin_completion_item(&member, range));
    }
    out
}

fn builtin_completion_item(member: &BuiltinMember, range: Range) -> CompletionItem {
    let kind = if member.is_method() {
        CompletionItemKind::METHOD
    } else {
        CompletionItemKind::FIELD
    };
    let insert = if member.is_method() {
        method_snippet(&member.name, &member.params)
    } else {
        member.name.clone()
    };
    completion_item(
        member.name.clone(),
        kind,
        member.label.clone(),
        member.documentation.clone(),
        insert,
        range,
        if member.is_method() { "2" } else { "1" },
        member.is_method(),
    )
}

fn method_snippet(name: &str, params: &[ParamSig]) -> String {
    if params.is_empty() {
        return format!("{}()", name);
    }
    let placeholders: Vec<String> = params
        .iter()
        .enumerate()
        .map(|(i, p)| format!("${{{}:{}}}", i + 1, p.name))
        .collect();
    format!("{}({})", name, placeholders.join(", "))
}

fn general_completion_items(
    analysis: &Analysis,
    prefix: &str,
    range: Range,
    pos: BytePos,
) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let lower = prefix.to_lowercase();

    // 局部变量 / 参数
    for id in analysis.model.visible_locals(&analysis.path, pos) {
        let Some(sym) = analysis.model.symbol(id) else {
            continue;
        };
        if !sym.name.to_lowercase().starts_with(&lower) {
            continue;
        }
        out.push(completion_item(
            sym.name.clone(),
            CompletionItemKind::VARIABLE,
            sym.detail.clone(),
            sym.documentation.clone(),
            sym.name.clone(),
            range,
            "0",
            false,
        ));
    }

    // 当前类字段
    if let Some(class) = analysis.model.class_at(&analysis.path, pos) {
        if let Some(info) = analysis.registry.get_class(&class) {
            for (fname, field) in &info.fields {
                if !fname.to_lowercase().starts_with(&lower) {
                    continue;
                }
                out.push(completion_item(
                    fname.clone(),
                    CompletionItemKind::FIELD,
                    format!("{} {}", display_type(&field.field_type), fname),
                    String::new(),
                    fname.clone(),
                    range,
                    "1",
                    false,
                ));
            }
            for (mname, methods) in &info.methods {
                if !mname.to_lowercase().starts_with(&lower) {
                    continue;
                }
                for method in methods {
                    let params: Vec<ParamSig> = method
                        .params
                        .iter()
                        .map(|p| ParamSig {
                            name: p.name.clone(),
                            ty: p.param_type.clone(),
                            is_varargs: p.is_varargs,
                        })
                        .collect();
                    let signature = format!(
                        "{} {}({})",
                        display_type(&method.return_type),
                        mname,
                        params
                            .iter()
                            .map(|p| p.display())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    out.push(completion_item(
                        mname.clone(),
                        CompletionItemKind::METHOD,
                        signature,
                        String::new(),
                        method_snippet(mname, &params),
                        range,
                        "2",
                        true,
                    ));
                    break; // 同名重载只需展示一个补全项
                }
            }
        }
    }

    // 文档内类型 / 函数
    for sym in &analysis.model.symbols {
        if !matches!(
            sym.kind,
            SymbolKind::Class
                | SymbolKind::Interface
                | SymbolKind::Struct
                | SymbolKind::Enum
                | SymbolKind::Function
        ) {
            continue;
        }
        if !sym.name.to_lowercase().starts_with(&lower) {
            continue;
        }
        let kind = match sym.kind {
            SymbolKind::Class => CompletionItemKind::CLASS,
            SymbolKind::Interface => CompletionItemKind::INTERFACE,
            SymbolKind::Struct => CompletionItemKind::STRUCT,
            SymbolKind::Enum => CompletionItemKind::ENUM,
            _ => CompletionItemKind::FUNCTION,
        };
        out.push(completion_item(
            sym.name.clone(),
            kind,
            sym.detail.clone(),
            sym.documentation.clone(),
            sym.name.clone(),
            range,
            "3",
            false,
        ));
    }

    // 内置类型
    for (name, bt) in analysis.builtins.all_type_names() {
        if !name.to_lowercase().starts_with(&lower) {
            continue;
        }
        out.push(completion_item(
            name.clone(),
            CompletionItemKind::CLASS,
            format!("内建类型 {}（存根 {}）", name, bt.stub_name),
            bt.documentation.clone(),
            name.clone(),
            range,
            "4",
            false,
        ));
    }

    // 内置函数
    for func in analysis.builtins.global_functions() {
        if !func.name.to_lowercase().starts_with(&lower) {
            continue;
        }
        out.push(completion_item(
            func.name.clone(),
            CompletionItemKind::FUNCTION,
            func.label.clone(),
            func.documentation.clone(),
            method_snippet(&func.name, &func.params),
            range,
            "2",
            true,
        ));
    }

    // 关键字
    for keyword in KEYWORDS {
        if !keyword.to_lowercase().starts_with(&lower) {
            continue;
        }
        out.push(completion_item(
            (*keyword).to_string(),
            CompletionItemKind::KEYWORD,
            keyword_doc(keyword).unwrap_or("Cavvy 关键字").to_string(),
            String::new(),
            (*keyword).to_string(),
            range,
            "5",
            false,
        ));
    }

    // 常用代码片段
    for (label, detail, snippet) in SNIPPETS {
        if !label.to_lowercase().starts_with(&lower) {
            continue;
        }
        out.push(completion_item(
            (*label).to_string(),
            CompletionItemKind::SNIPPET,
            (*detail).to_string(),
            String::new(),
            (*snippet).to_string(),
            range,
            "6",
            true,
        ));
    }

    out
}

fn completion_item(
    label: String,
    kind: CompletionItemKind,
    detail: String,
    documentation: String,
    insert_text: String,
    range: Range,
    sort_prefix: &str,
    snippet: bool,
) -> CompletionItem {
    CompletionItem {
        label: label.clone(),
        label_details: None,
        kind: Some(kind),
        detail: Some(detail),
        documentation: if documentation.is_empty() {
            None
        } else {
            Some(Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: documentation,
            }))
        },
        deprecated: None,
        preselect: None,
        sort_text: Some(format!("{}_{}", sort_prefix, label)),
        filter_text: Some(label.clone()),
        insert_text: Some(insert_text.clone()),
        insert_text_format: Some(if snippet {
            InsertTextFormat::SNIPPET
        } else {
            InsertTextFormat::PLAIN_TEXT
        }),
        text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(range, insert_text))),
        additional_text_edits: None,
        command: None,
        commit_characters: None,
        data: None,
        tags: None,
        insert_text_mode: None,
    }
}

const KEYWORDS: &[&str] = &[
    "public",
    "private",
    "protected",
    "static",
    "final",
    "abstract",
    "native",
    "class",
    "interface",
    "struct",
    "enum",
    "extends",
    "implements",
    "namespace",
    "using",
    "void",
    "int",
    "long",
    "float",
    "double",
    "bool",
    "char",
    "String",
    "var",
    "let",
    "auto",
    "if",
    "else",
    "while",
    "for",
    "do",
    "switch",
    "case",
    "default",
    "break",
    "continue",
    "return",
    "new",
    "this",
    "super",
    "instanceof",
    "extern",
    "true",
    "false",
    "null",
    "scope",
    "typeof",
    "sizeof",
    "fn",
];

const SNIPPETS: &[(&str, &str, &str)] = &[
    (
        "class",
        "定义类",
        "public class ${1:Name} {\n    public ${2:void} ${3:run}() {\n        $0\n    }\n}",
    ),
    (
        "interface",
        "定义接口",
        "public interface ${1:Name} {\n    ${2:void} ${3:run}();\n}",
    ),
    (
        "struct",
        "定义结构体",
        "public struct ${1:Name} {\n    ${2:int} ${3:value};\n}",
    ),
    (
        "enum",
        "定义枚举",
        "public enum ${1:Name} {\n    ${2:A},\n    ${3:B}\n}",
    ),
    (
        "main",
        "主类 + main 方法",
        "@main\npublic class ${1:Main} {\n    public static void main() {\n        $0\n    }\n}",
    ),
    (
        "for",
        "for 循环",
        "for (int ${1:i} = 0; ${1:i} < ${2:n}; ${1:i}++) {\n    $0\n}",
    ),
    (
        "foreach",
        "增强 for 循环",
        "for (${1:int} ${2:item} : ${3:items}) {\n    $0\n}",
    ),
    ("if", "if 语句", "if (${1:condition}) {\n    $0\n}"),
    ("while", "while 循环", "while (${1:condition}) {\n    $0\n}"),
    (
        "switch",
        "switch 语句",
        "switch (${1:value}) {\n    case ${2:1}:\n        $0\n        break;\n    default:\n        break;\n}",
    ),
];

fn class_chain(registry: &crate::types::TypeRegistry, name: &str) -> Vec<crate::types::ClassInfo> {
    let mut out = Vec::new();
    let mut current = Some(name.to_string());
    let mut seen = HashSet::new();
    while let Some(name) = current {
        if !seen.insert(name.clone()) {
            break;
        }
        let Some(info) = registry.get_class(&name) else {
            break;
        };
        out.push(info.clone());
        current = info.parent.clone();
    }
    out
}

// ---------------------------------------------------------------------------
// Signature help
// ---------------------------------------------------------------------------

fn signature_help_impl(analysis: &Analysis, pos: BytePos) -> Option<SignatureHelp> {
    let call = analysis.model.call_at(&analysis.path, pos)?;
    if call.signatures.is_empty() {
        return None;
    }
    let signatures: Vec<SignatureInformation> = call
        .signatures
        .iter()
        .map(|sig| {
            let params: Vec<ParameterInformation> = sig
                .params
                .iter()
                .map(|p| ParameterInformation {
                    label: ParameterLabel::Simple(p.display()),
                    documentation: None,
                })
                .collect();
            SignatureInformation {
                label: sig.label.clone(),
                documentation: if sig.documentation.is_empty() {
                    None
                } else {
                    Some(Documentation::MarkupContent(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: sig.documentation.clone(),
                    }))
                },
                parameters: Some(params),
                active_parameter: None,
            }
        })
        .collect();
    let active_parameter = active_parameter_index(analysis, call, pos);
    Some(SignatureHelp {
        signatures,
        active_signature: Some(0),
        active_parameter: Some(active_parameter),
    })
}

fn active_parameter_index(analysis: &Analysis, call: &super::model::CallSite, pos: BytePos) -> u32 {
    let Some(text) = analysis.main_text() else {
        return 0;
    };
    let offset = text.offset(pos);
    let open_offset = text.offset(call.open);
    let close_offset = text.offset(call.close);
    if offset <= open_offset || offset > close_offset {
        return 0;
    }
    let mut index = 0u32;
    let mut depth = 0i32;
    let mut in_string = None;
    let mut escaped = false;
    let bytes = text.text.as_bytes();
    let mut i = open_offset + 1;
    while i < offset {
        let ch = text.text[i..].chars().next().unwrap_or('\0');
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = None;
            }
            i += ch.len_utf8();
            continue;
        }
        match ch {
            '"' | '\'' => in_string = Some(ch),
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            ',' if depth == 0 => index += 1,
            _ => {}
        }
        i += ch.len_utf8();
        let _ = bytes;
    }
    index
}

// ---------------------------------------------------------------------------
// Definition / references
// ---------------------------------------------------------------------------

fn definition_impl(
    state: Option<&Arc<ServerState>>,
    analysis: &Analysis,
    pos: BytePos,
) -> Option<GotoDefinitionResponse> {
    if let Some(occ) = analysis.model.occurrence_at_word(&analysis.path, pos) {
        if let Some(symbol) = occ.symbol.and_then(|id| analysis.model.symbol(id)) {
            if let Some(location) = symbol_location(analysis, symbol) {
                return Some(GotoDefinitionResponse::Scalar(location));
            }
        }
    }

    // #include 路径
    if let Some(link) = include_at(analysis, pos) {
        return Some(GotoDefinitionResponse::Scalar(link));
    }

    // 工作区中的类型 / 函数
    if let Some(state) = state {
        if let Some(text) = analysis.main_text() {
            if let Some((_, word)) = text.word_at(pos) {
                for key in [
                    SymbolKey::Class(word.clone()),
                    SymbolKey::Interface(word.clone()),
                    SymbolKey::Struct(word.clone()),
                    SymbolKey::Enum(word.clone()),
                    SymbolKey::Function(word.clone()),
                ] {
                    if let Some(span) = state.workspace.find_definition(&key) {
                        return Some(GotoDefinitionResponse::Scalar(Location {
                            uri: path_to_uri(&span.file),
                            range: range_for_file(&span.file, span.span),
                        }));
                    }
                }
            }
        }
    }
    None
}

fn symbol_location(analysis: &Analysis, symbol: &Symbol) -> Option<Location> {
    if symbol.location.file.starts_with('<') {
        return None;
    }
    if let Some(text) = analysis.files.get(&symbol.location.file) {
        Some(Location {
            uri: path_to_uri(&symbol.location.file),
            range: text.range(symbol.selection.span),
        })
    } else {
        // include / 内置存根文件可能不在当前分析的文件表里；直接按字节列返回，
        // 存根与 include 文件通常以 ASCII 为主，UTF-16 与字节列一致。
        Some(Location {
            uri: path_to_uri(&symbol.location.file),
            range: range_for_file(&symbol.location.file, symbol.selection.span),
        })
    }
}

fn include_at(analysis: &Analysis, pos: BytePos) -> Option<Location> {
    for (rel, span) in find_include_directives(&analysis.content) {
        if span.contains(pos) {
            let path = resolve_include_path(analysis, &rel)?;
            return Some(Location {
                uri: path_to_uri(&path),
                range: Range::new(Position::new(0, 0), Position::new(0, 0)),
            });
        }
    }
    None
}

fn resolve_include_path(analysis: &Analysis, rel: &str) -> Option<PathBuf> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() && rel_path.exists() {
        return Some(rel_path.to_path_buf());
    }
    let base = Path::new(&analysis.path).parent()?;
    let candidate = base.join(rel);
    if candidate.exists() {
        return Some(candidate);
    }
    for root in &analysis.include_paths {
        let candidate = root.join(rel);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

fn references_impl(
    state: &Arc<ServerState>,
    analysis: &Analysis,
    pos: BytePos,
    include_declaration: bool,
) -> Vec<Location> {
    let Some(occ) = analysis.model.occurrence_at_word(&analysis.path, pos) else {
        return Vec::new();
    };
    let Some(symbol) = occ.symbol.and_then(|id| analysis.model.symbol(id)) else {
        return Vec::new();
    };
    let key = symbol.key.clone();
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    collect_key_locations(analysis, &key, include_declaration, &mut out, &mut seen);

    if !matches!(key, SymbolKey::Local { .. } | SymbolKey::Parameter { .. }) {
        // 其他已打开文档
        let open_uris: Vec<String> = state.documents.iter().map(|e| e.key().clone()).collect();
        for uri in open_uris {
            if &uri == &analysis.uri {
                continue;
            }
            let Some(other) = analysis_for(state, &uri) else {
                continue;
            };
            collect_key_locations(&other, &key, include_declaration, &mut out, &mut seen);
        }

        // 工作区候选文件（包含同名文本的文件才做完整分析）
        if !is_builtin_key(&key) {
            let candidates = state.workspace.files_containing(&symbol.name, 60);
            for file in candidates {
                if file.path == analysis.path {
                    continue;
                }
                if file.content.len() > 2 * 1024 * 1024 {
                    continue;
                }
                let builtins = state
                    .builtins
                    .read()
                    .map(|g| g.clone())
                    .unwrap_or_else(|_| Arc::new(BuiltinIndex::fallback()));
                let include_paths = state
                    .include_paths
                    .read()
                    .map(|g| g.clone())
                    .unwrap_or_default();
                let other = analyze_document(
                    &file.uri.to_string(),
                    &file.path,
                    &file.content,
                    0,
                    builtins,
                    &include_paths,
                    &HashMap::new(),
                );
                collect_key_locations(&other, &key, include_declaration, &mut out, &mut seen);
            }
        }
    }
    out
}

fn collect_key_locations(
    analysis: &Analysis,
    key: &SymbolKey,
    include_declaration: bool,
    out: &mut Vec<Location>,
    seen: &mut HashSet<(String, Span)>,
) {
    for occ in &analysis.model.occurrences {
        if occ.is_definition && !include_declaration {
            continue;
        }
        let matches = occ
            .symbol
            .and_then(|id| analysis.model.symbol(id))
            .map(|sym| &sym.key == key)
            .unwrap_or(false);
        if !matches {
            continue;
        }
        if !seen.insert((occ.file.clone(), occ.span)) {
            continue;
        }
        out.push(location_in(analysis, &occ.file, occ.span));
    }
    // 声明本身如果没有对应的出现记录，补上。
    if include_declaration {
        if let Some(symbol) = analysis.model.symbol_for_key(key) {
            if !symbol.location.file.starts_with('<') {
                if seen.insert((symbol.location.file.clone(), symbol.selection.span)) {
                    out.push(location_in(
                        analysis,
                        &symbol.location.file,
                        symbol.selection.span,
                    ));
                }
            }
        }
    }
}

fn is_builtin_key(key: &SymbolKey) -> bool {
    matches!(
        key,
        SymbolKey::BuiltinType(_)
            | SymbolKey::BuiltinMethod { .. }
            | SymbolKey::BuiltinField { .. }
            | SymbolKey::BuiltinFunction { .. }
    )
}

// ---------------------------------------------------------------------------
// Document highlight
// ---------------------------------------------------------------------------

fn document_highlight_impl(analysis: &Analysis, pos: BytePos) -> Option<Vec<DocumentHighlight>> {
    let occ = analysis.model.occurrence_at_word(&analysis.path, pos)?;
    let key = occ
        .symbol
        .and_then(|id| analysis.model.symbol(id))
        .map(|s| s.key.clone())?;
    let mut out = Vec::new();
    for other in &analysis.model.occurrences {
        if other.file != analysis.path {
            continue;
        }
        let matches = other
            .symbol
            .and_then(|id| analysis.model.symbol(id))
            .map(|s| s.key == key)
            .unwrap_or(false);
        if matches {
            out.push(DocumentHighlight {
                range: range_in(analysis, &other.file, other.span),
                kind: Some(if other.is_definition {
                    DocumentHighlightKind::WRITE
                } else {
                    DocumentHighlightKind::READ
                }),
            });
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

// ---------------------------------------------------------------------------
// Semantic tokens
// ---------------------------------------------------------------------------

const TT_TYPE: u32 = 1;
const TT_CLASS: u32 = 2;
const TT_ENUM: u32 = 3;
const TT_INTERFACE: u32 = 4;
const TT_STRUCT: u32 = 5;

const TT_PARAMETER: u32 = 7;
const TT_VARIABLE: u32 = 8;
const TT_PROPERTY: u32 = 9;
const TT_ENUM_MEMBER: u32 = 10;
const TT_FUNCTION: u32 = 11;
const TT_METHOD: u32 = 12;
const TT_KEYWORD: u32 = 13;
const TT_STRING: u32 = 14;
const TT_NUMBER: u32 = 15;
const TT_OPERATOR: u32 = 16;

fn semantic_tokens_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::new("namespace"),
            SemanticTokenType::new("type"),
            SemanticTokenType::new("class"),
            SemanticTokenType::new("enum"),
            SemanticTokenType::new("interface"),
            SemanticTokenType::new("struct"),
            SemanticTokenType::new("typeParameter"),
            SemanticTokenType::new("parameter"),
            SemanticTokenType::new("variable"),
            SemanticTokenType::new("property"),
            SemanticTokenType::new("enumMember"),
            SemanticTokenType::new("function"),
            SemanticTokenType::new("method"),
            SemanticTokenType::new("keyword"),
            SemanticTokenType::new("string"),
            SemanticTokenType::new("number"),
            SemanticTokenType::new("operator"),
            SemanticTokenType::new("comment"),
        ],
        token_modifiers: vec![],
    }
}

fn semantic_tokens_impl(analysis: &Analysis) -> SemanticTokens {
    let Some(text) = analysis.main_text() else {
        return SemanticTokens {
            result_id: None,
            data: Vec::new(),
        };
    };
    let mut entries: Vec<(u32, u32, u32, u32)> = Vec::new();
    for token in &analysis.tokens {
        let file = token.loc.file.as_deref().unwrap_or(&analysis.path);
        if file != analysis.path {
            continue;
        }
        let line = token.loc.line.saturating_sub(1) as u32;
        let byte_col = token.loc.column.saturating_sub(1) as u32;
        if line >= text.line_count() {
            continue;
        }
        let start = text.byte_col_to_utf16(line, byte_col);
        let lexeme = &token.lexeme;
        if lexeme.contains('\n') || lexeme.is_empty() {
            continue;
        }
        let len = lexeme.encode_utf16().count() as u32;
        if len == 0 {
            continue;
        }
        let ty = classify_token(analysis, token, BytePos::new(line, byte_col));
        if let Some(ty) = ty {
            entries.push((line, start, len, ty));
        }
    }
    entries.sort_by_key(|e| (e.0, e.1));
    let mut data = Vec::with_capacity(entries.len() * 5);
    let mut prev_line = 0u32;
    let mut prev_start = 0u32;
    for (line, start, len, ty) in entries {
        let delta_line = line - prev_line;
        let delta_start = if delta_line == 0 {
            start - prev_start
        } else {
            start
        };
        data.push(SemanticToken {
            delta_line,
            delta_start,
            length: len,
            token_type: ty,
            token_modifiers_bitset: 0,
        });
        prev_line = line;
        prev_start = start;
    }
    SemanticTokens {
        result_id: None,
        data,
    }
}

fn classify_token(
    analysis: &Analysis,
    token: &crate::lexer::TokenWithLocation,
    pos: BytePos,
) -> Option<u32> {
    use crate::lexer::Token as T;
    match &token.token {
        T::Identifier(_) => {
            if let Some(occ) = analysis.model.occurrence_at(&analysis.path, pos) {
                if let Some(symbol) = occ.symbol.and_then(|id| analysis.model.symbol(id)) {
                    return Some(match symbol.kind {
                        SymbolKind::Class => TT_CLASS,
                        SymbolKind::Interface => TT_INTERFACE,
                        SymbolKind::Struct => TT_STRUCT,
                        SymbolKind::Enum => TT_ENUM,
                        SymbolKind::EnumVariant => TT_ENUM_MEMBER,
                        SymbolKind::Method | SymbolKind::Constructor => TT_METHOD,
                        SymbolKind::Field => TT_PROPERTY,
                        SymbolKind::Function => TT_FUNCTION,
                        SymbolKind::Local => TT_VARIABLE,
                        SymbolKind::Parameter => TT_PARAMETER,
                        SymbolKind::BuiltinType => TT_CLASS,
                        SymbolKind::BuiltinMember => TT_METHOD,
                    });
                }
            }
            Some(match occ_kind(&analysis.model, &analysis.path, pos) {
                Some(OccurrenceKind::Member) => TT_PROPERTY,
                Some(OccurrenceKind::MemberCall) | Some(OccurrenceKind::Call) => TT_METHOD,
                Some(OccurrenceKind::Type) => TT_TYPE,
                _ => TT_VARIABLE,
            })
        }
        T::StringLiteral(_) | T::CharLiteral(_) => Some(TT_STRING),
        T::IntegerLiteral(_) | T::FloatLiteral(_) => Some(TT_NUMBER),
        T::Int | T::Long | T::Float | T::Double | T::Bool | T::String | T::Char | T::Void => {
            Some(TT_TYPE)
        }
        T::True | T::False | T::Null => Some(TT_KEYWORD),
        t if is_keyword_token(t) => Some(TT_KEYWORD),
        t if is_operator_token(t) => Some(TT_OPERATOR),
        _ => None,
    }
}

fn occ_kind(model: &super::model::Model, file: &str, pos: BytePos) -> Option<OccurrenceKind> {
    model.occurrence_at(file, pos).map(|o| o.kind)
}

fn is_keyword_token(token: &crate::lexer::Token) -> bool {
    use crate::lexer::Token as T;
    matches!(
        token,
        T::Public
            | T::Private
            | T::Protected
            | T::Static
            | T::Final
            | T::Abstract
            | T::Native
            | T::Interop
            | T::Class
            | T::Interface
            | T::Struct
            | T::Enum
            | T::Extends
            | T::Implements
            | T::Namespace
            | T::Using
            | T::If
            | T::Else
            | T::While
            | T::For
            | T::Do
            | T::Switch
            | T::Case
            | T::Default
            | T::Break
            | T::Continue
            | T::Return
            | T::New
            | T::This
            | T::Super
            | T::InstanceOf
            | T::Var
            | T::Let
            | T::Auto
            | T::Extern
            | T::Scope
            | T::TypeOf
            | T::SizeOf
            | T::Fn
            | T::Impl
    )
}

fn is_operator_token(token: &crate::lexer::Token) -> bool {
    use crate::lexer::Token as T;
    matches!(
        token,
        T::Plus
            | T::Minus
            | T::Star
            | T::Slash
            | T::Percent
            | T::Assign
            | T::EqEq
            | T::NotEq
            | T::Lt
            | T::Le
            | T::Gt
            | T::Ge
            | T::AndAnd
            | T::OrOr
            | T::Bang
            | T::Ampersand
            | T::Pipe
            | T::Caret
            | T::Tilde
            | T::Shl
            | T::Shr
            | T::AddAssign
            | T::SubAssign
            | T::MulAssign
            | T::DivAssign
            | T::ModAssign
            | T::Arrow
            | T::Question
    )
}

// ---------------------------------------------------------------------------
// Folding / inlay hints / links / selection
// ---------------------------------------------------------------------------

fn folding_impl(analysis: &Analysis) -> Vec<FoldingRange> {
    let mut out = Vec::new();
    for folding in &analysis.model.folding {
        if folding.file != analysis.path {
            continue;
        }
        let start = folding.span.start.line;
        let end = folding.span.end.line;
        if end <= start {
            continue;
        }
        let kind = match folding.kind {
            "class" | "interface" | "struct" | "enum" => Some(FoldingRangeKind::Region),
            "method" | "constructor" | "function" => Some(FoldingRangeKind::Region),
            _ => None,
        };
        out.push(FoldingRange {
            start_line: start,
            start_character: None,
            end_line: end,
            end_character: None,
            kind,
            collapsed_text: None,
        });
    }
    // include 块折叠
    let includes = find_include_directives(&analysis.content);
    if includes.len() > 1 {
        let first = includes.first().map(|(_, s)| s.start.line).unwrap_or(0);
        let last = includes.last().map(|(_, s)| s.end.line).unwrap_or(0);
        if last > first {
            out.push(FoldingRange {
                start_line: first,
                start_character: None,
                end_line: last,
                end_character: None,
                kind: Some(FoldingRangeKind::Imports),
                collapsed_text: None,
            });
        }
    }
    out.sort_by_key(|f| (f.start_line, f.end_line));
    out
}

fn inlay_hints_impl(analysis: &Analysis, range: Range) -> Vec<InlayHint> {
    let Some(text) = analysis.main_text() else {
        return Vec::new();
    };
    let start = BytePos::new(
        range.start.line.min(text.line_count().saturating_sub(1)),
        text.utf16_to_byte_col(
            range.start.line.min(text.line_count().saturating_sub(1)),
            range.start.character,
        ),
    );
    let end = BytePos::new(
        range.end.line.min(text.line_count().saturating_sub(1)),
        text.utf16_to_byte_col(
            range.end.line.min(text.line_count().saturating_sub(1)),
            range.end.character,
        ),
    );
    analysis
        .model
        .inlay_hints
        .iter()
        .filter(|h| h.file == analysis.path && h.pos >= start && h.pos <= end)
        .map(|h| InlayHint {
            position: text.position(h.pos),
            label: InlayHintLabel::String(h.label.clone()),
            kind: Some(InlayHintKind::TYPE),
            text_edits: None,
            tooltip: Some(InlayHintTooltip::String(
                "由初始化表达式推断出的类型".to_string(),
            )),
            padding_left: Some(false),
            padding_right: Some(true),
            data: None,
        })
        .collect()
}

fn document_links_impl(analysis: &Analysis) -> Vec<DocumentLink> {
    let Some(text) = analysis.main_text() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (rel, span) in find_include_directives(&analysis.content) {
        if let Some(path) = resolve_include_path(analysis, &rel) {
            out.push(DocumentLink {
                range: text.range(span),
                target: Some(path_to_uri(path)),
                tooltip: Some(format!("打开包含文件 {}", rel)),
                data: None,
            });
        }
    }
    out
}

fn selection_range_impl(analysis: &Analysis, pos: BytePos) -> SelectionRange {
    let text = analysis.main_text();
    let word_span = text
        .and_then(|t| t.word_at(pos))
        .map(|(s, _)| s)
        .unwrap_or(Span::point(pos));
    let mut current = SelectionRange {
        range: text
            .map(|t| t.range(word_span))
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0))),
        parent: None,
    };

    let mut spans = Vec::new();
    if let Some(occ) = analysis.model.occurrence_at(&analysis.path, pos) {
        spans.push(occ.span);
        if let Some(symbol) = occ.symbol.and_then(|id| analysis.model.symbol(id)) {
            spans.push(symbol.location.span);
        }
    }
    if let Some(class) = analysis.model.class_at(&analysis.path, pos) {
        if let Some(sym) = analysis.model.class_symbol(&class) {
            spans.push(sym.location.span);
        }
    }
    if let Some(text) = text {
        spans.push(Span::new(
            BytePos::new(0, 0),
            BytePos::new(text.line_count().saturating_sub(1), 0),
        ));
    }
    spans.sort_by_key(|s| std::cmp::Reverse((s.end.line, s.end.col, s.start.line, s.start.col)));
    for span in spans {
        if span == word_span {
            continue;
        }
        let parent = current;
        current = SelectionRange {
            range: text
                .map(|t| t.range(span))
                .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0))),
            parent: Some(Box::new(parent)),
        };
    }
    current
}

// ---------------------------------------------------------------------------
// Code actions
// ---------------------------------------------------------------------------

fn code_action_impl(analysis: &Analysis, diagnostics: &[Diagnostic]) -> Option<CodeActionResponse> {
    let mut actions = Vec::new();
    let text = analysis.main_text()?;
    for diagnostic in diagnostics {
        let Some(start) = position_to_byte(analysis, diagnostic.range.start) else {
            continue;
        };
        let Some((unknown, kind)) = parse_unknown_member(&diagnostic.message) else {
            continue;
        };
        let Some(occ) = analysis.model.occurrence_at_word(&analysis.path, start) else {
            continue;
        };
        let receiver = occ.receiver.clone();
        let candidates = match kind {
            UnknownKind::String => analysis
                .builtins
                .members_for(&Type::String)
                .into_iter()
                .map(|m| m.name)
                .collect::<Vec<_>>(),
            UnknownKind::Member => receiver
                .as_ref()
                .map(|ty| member_candidates(analysis, ty))
                .unwrap_or_default(),
            UnknownKind::Static => {
                if let Some(recv) = &occ.receiver {
                    static_candidates(analysis, &base_type_name(recv))
                } else if let Some(text) = analysis.main_text() {
                    if let Some((_, word)) = text.word_at(start) {
                        static_candidates(analysis, &word)
                    } else {
                        Vec::new()
                    }
                } else {
                    Vec::new()
                }
            }
        };
        if candidates.is_empty() {
            continue;
        }
        let Some(best) = fuzzy_best(&unknown, &candidates) else {
            continue;
        };
        let range = text.range(occ.span);
        let edit = WorkspaceEdit {
            changes: Some(HashMap::from([(
                path_to_uri(&analysis.path),
                vec![TextEdit::new(range, best.clone())],
            )])),
            document_changes: None,
            change_annotations: None,
        };
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: format!("将 '{}' 改为 '{}'", unknown, best),
            kind: Some(CodeActionKind::QUICKFIX),
            diagnostics: Some(vec![diagnostic.clone()]),
            edit: Some(edit),
            command: None,
            is_preferred: Some(true),
            disabled: None,
            data: None,
        }));
    }
    if actions.is_empty() {
        None
    } else {
        Some(actions)
    }
}

enum UnknownKind {
    String,
    Member,
    Static,
}

fn parse_unknown_member(message: &str) -> Option<(String, UnknownKind)> {
    let patterns = [
        ("Unknown String method '", UnknownKind::String),
        ("Unknown member '", UnknownKind::Member),
        ("Unknown static member '", UnknownKind::Static),
        ("Unknown method '", UnknownKind::Member),
    ];
    for (pattern, kind) in patterns {
        if let Some(start) = message.find(pattern) {
            let rest = &message[start + pattern.len()..];
            if let Some(end) = rest.find('\'') {
                return Some((rest[..end].to_string(), kind));
            }
        }
    }
    None
}

fn member_candidates(analysis: &Analysis, ty: &Type) -> Vec<String> {
    let mut out = Vec::new();
    for member in analysis.builtins.members_for(ty) {
        out.push(member.name);
    }
    match ty {
        Type::Object(name) | Type::Generic(name, _) => {
            let base = name.split('<').next().unwrap_or(name).trim();
            for info in class_chain(&analysis.registry, base) {
                out.extend(info.fields.keys().cloned());
                out.extend(info.methods.keys().cloned());
            }
        }
        Type::Struct(name) => {
            if let Some(info) = analysis.registry.get_struct(name) {
                out.extend(info.fields.keys().cloned());
                out.extend(info.methods.keys().cloned());
            }
        }
        _ => {}
    }
    out.sort();
    out.dedup();
    out
}

fn static_candidates(analysis: &Analysis, class_name: &str) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(info) = analysis.registry.get_class(class_name) {
        for (name, field) in &info.fields {
            if field.is_static {
                out.push(name.clone());
            }
        }
        for (name, methods) in &info.methods {
            if methods.iter().any(|m| m.is_static) {
                out.push(name.clone());
            }
        }
    }
    if let Some(en) = analysis.registry.get_enum(class_name) {
        out.extend(en.variants.iter().map(|v| v.name.clone()));
    }
    for member in analysis.builtins.static_members_for(class_name) {
        out.push(member.name);
    }
    out.sort();
    out.dedup();
    out
}

fn fuzzy_best(name: &str, candidates: &[String]) -> Option<String> {
    let lower = name.to_lowercase();
    let mut best: Option<(usize, String)> = None;
    for candidate in candidates {
        if candidate == name {
            continue;
        }
        let cl = candidate.to_lowercase();
        let distance = if cl.starts_with(&lower) || lower.starts_with(&cl) {
            0
        } else {
            edit_distance(&lower, &cl)
        };
        let threshold = (name.len() / 3).max(2);
        if distance <= threshold {
            if best.as_ref().map(|(d, _)| distance < *d).unwrap_or(true) {
                best = Some((distance, candidate.clone()));
            }
        }
    }
    best.map(|(_, name)| name)
}

fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for i in 1..=a.len() {
        cur[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            cur[j] = (cur[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}

fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn fuzzy_name_match(name: &str, query: &str) -> bool {
    name.to_lowercase().contains(&query.to_lowercase())
}

fn symbol_information_from_index(indexed: &IndexedSymbol) -> SymbolInformation {
    SymbolInformation {
        name: indexed.name.clone(),
        kind: indexed.kind.lsp(),
        tags: None,
        deprecated: None,
        location: Location {
            uri: path_to_uri(&indexed.location.file),
            range: Range::new(
                Position::new(
                    indexed.location.span.start.line,
                    indexed.location.span.start.col,
                ),
                Position::new(
                    indexed.location.span.end.line,
                    indexed.location.span.end.col,
                ),
            ),
        },
        container_name: indexed.container.clone(),
    }
}

// ---------------------------------------------------------------------------
// 入口
// ---------------------------------------------------------------------------

pub async fn run_stdio() {
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(CavvyLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}

/// 保留一个未使用的导入检查点，避免 workspace 类型在重构中被误删。
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn builtins() -> Arc<BuiltinIndex> {
        Arc::new(BuiltinIndex::load_from_root(&PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/caylibs/__builtins__"
        ))))
    }

    fn analyze(source: &str) -> Analysis {
        analyze_document(
            "file:///test.cay",
            "test.cay",
            source,
            1,
            builtins(),
            &[],
            &HashMap::new(),
        )
    }

    fn pos_of(text: &TextFile, needle: &str, offset: usize) -> BytePos {
        let idx = text.text.find(needle).expect("needle 应存在");
        text.pos_at_offset(idx + offset)
    }

    #[test]
    fn completion_after_array_dot_lists_length_from_stub() {
        let source = "public class Foo {\n    public void run() {\n        int[] v = new int[3];\n        int n = v.length;\n    }\n}";
        let analysis = analyze(source);
        let text = analysis.main_text().unwrap();
        let pos = pos_of(text, "v.length", 2);
        let items = completion_impl(None, &analysis, pos);
        assert!(
            items.iter().any(|i| i.label == "length"),
            "array 的 .length 应来自 __builtins__/array.cay，实际: {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn hover_array_length_shows_builtin_doc() {
        let source = "public class Foo {\n    public void run() {\n        int[] v = new int[3];\n        int n = v.length;\n    }\n}";
        let analysis = analyze(source);
        let text = analysis.main_text().unwrap();
        let pos = pos_of(text, "v.length", 2);
        let hover = hover_impl(&analysis, pos).expect("应有 hover");
        let HoverContents::Markup(markup) = hover.contents else {
            panic!("期望 markdown hover");
        };
        assert!(markup.value.contains("int length"), "{}", markup.value);
    }

    #[test]
    fn definition_array_length_jumps_to_stub_file() {
        let source = "public class Foo {\n    public void run() {\n        int[] v = new int[3];\n        int n = v.length;\n    }\n}";
        let analysis = analyze(source);
        let text = analysis.main_text().unwrap();
        let pos = pos_of(text, "v.length", 2);
        let def = definition_impl(None, &analysis, pos).expect("应有定义");
        let GotoDefinitionResponse::Scalar(location) = def else {
            panic!("期望单一定义");
        };
        assert!(
            location.uri.as_str().contains("array.cay"),
            "{}",
            location.uri
        );
    }

    #[test]
    fn signature_help_for_string_substring() {
        let source = "public class Foo {\n    public void run() {\n        String s = \"hello\";\n        String t = s.substring(1);\n    }\n}";
        let analysis = analyze(source);
        let text = analysis.main_text().unwrap();
        let pos = pos_of(text, "s.substring(", "s.substring(".len());
        let help = signature_help_impl(&analysis, pos).expect("应有签名帮助");
        assert!(
            help.signatures
                .iter()
                .any(|s| s.label.contains("substring")),
            "{:?}",
            help.signatures
        );
    }

    #[test]
    fn completion_general_lists_local_variable() {
        let source = "public class Foo {\n    public void run() {\n        int count = 1;\n        co;\n    }\n}";
        let analysis = analyze(source);
        let text = analysis.main_text().unwrap();
        let pos = pos_of(text, "co;", 2);
        let items = completion_impl(None, &analysis, pos);
        assert!(
            items.iter().any(|i| i.label == "count"),
            "局部变量 count 应出现在补全中"
        );
    }

    #[test]
    fn document_symbols_include_members() {
        let source = "public class Foo {\n    public int count;\n    public int getCount() { return this.count; }\n}";
        let analysis = analyze(source);
        let symbols = analysis.model.document_symbols(&analysis.path);
        let class = symbols.iter().find(|s| s.name == "Foo").expect("Foo");
        let children = class.children.as_ref().expect("成员");
        assert!(children.iter().any(|c| c.name == "count"));
        assert!(children.iter().any(|c| c.name == "getCount"));
    }

    #[test]
    fn code_action_fixes_unknown_member_typo() {
        let source = "public class Foo {\n    public void run() {\n        int[] v = new int[3];\n        int n = v.lenght;\n    }\n}";
        let analysis = analyze(source);
        let text = analysis.main_text().unwrap();
        let pos = pos_of(text, "v.lenght", 2);
        let range = text.range(Span::new(pos, BytePos::new(pos.line, pos.col + 6)));
        let diagnostic = Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("cavvy".to_string()),
            message: "Unknown member 'lenght' for class array".to_string(),
            related_information: None,
            tags: None,
            data: None,
        };
        let actions = code_action_impl(&analysis, &[diagnostic]).expect("应有 quick fix");
        let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
            panic!("期望 CodeAction");
        };
        assert!(action.title.contains("length"), "{}", action.title);
    }

    #[test]
    fn semantic_tokens_have_data() {
        let source = "public class Foo {\n    public int count;\n}";
        let analysis = analyze(source);
        let tokens = semantic_tokens_impl(&analysis);
        assert!(!tokens.data.is_empty());
    }

    #[test]
    fn definition_of_parameter_uses_parameter_name_span() {
        let source = "public class Foo {\n    public int add(int a, int b) {\n        return a + b;\n    }\n}";
        let analysis = analyze(source);
        let text = analysis.main_text().unwrap();
        let pos = pos_of(text, "a + b", 0);
        let def = definition_impl(None, &analysis, pos).expect("参数应有定义");
        let GotoDefinitionResponse::Scalar(location) = def else {
            panic!("期望单一定义");
        };
        assert_eq!(location.range.start.line, 1);
        assert!(location.range.start.character < location.range.end.character);
    }

    #[test]
    fn inlay_hint_for_var_declaration() {
        let source = "public class Foo {\n    public void run() {\n        var x = 1;\n    }\n}";
        let analysis = analyze(source);
        assert!(
            !analysis.model.inlay_hints.is_empty(),
            "var x = 1 应产生类型 inlay hint"
        );
        let hint = &analysis.model.inlay_hints[0];
        assert!(hint.label.contains("int"), "{}", hint.label);
    }

    #[test]
    fn document_symbol_ranges_do_not_swallow_class_body() {
        let source = "public class Foo {\n    public int count;\n    public int getCount() { return this.count; }\n}";
        let analysis = analyze(source);
        let symbols = analysis.model.document_symbols(&analysis.path);
        let class = symbols.iter().find(|s| s.name == "Foo").unwrap();
        let field = class
            .children
            .as_ref()
            .unwrap()
            .iter()
            .find(|s| s.name == "count")
            .unwrap();
        assert_eq!(field.range.start.line, 1);
        assert_eq!(field.range.end.line, 1);
    }

    #[test]
    fn completion_after_partial_member_name() {
        let source = "public class Foo {\n    public void run() {\n        int[] v = new int[3];\n        int n = v.length;\n    }\n}";
        let analysis = analyze(source);
        let text = analysis.main_text().unwrap();
        // 光标位于 `v.le` 之后。
        let pos = pos_of(text, "v.length", 4);
        let items = completion_impl(None, &analysis, pos);
        assert!(items.iter().any(|i| i.label == "length"));
    }

    #[test]
    fn include_document_link_resolves_relative_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let main_path = dir.path().join("main.cay");
        let inc_path = dir.path().join("inc.cay");
        std::fs::write(&inc_path, "public class Inc {}\n").expect("write inc");
        let source = "#include \"inc.cay\"\npublic class Main {}\n";
        std::fs::write(&main_path, source).expect("write main");
        let analysis = analyze_document(
            &path_to_uri(&main_path).to_string(),
            &main_path.to_string_lossy(),
            source,
            1,
            builtins(),
            &[],
            &HashMap::new(),
        );
        let links = document_links_impl(&analysis);
        assert_eq!(links.len(), 1);
        assert!(links[0].target.is_some());
        assert!(
            links[0]
                .target
                .as_ref()
                .unwrap()
                .as_str()
                .ends_with("inc.cay")
        );
    }

    #[test]
    fn code_action_fixes_unknown_method_typo() {
        let source = "public class Foo {\n    public void run() {\n        String s = \"x\";\n        s.substing(1);\n    }\n}";
        let analysis = analyze(source);
        let text = analysis.main_text().unwrap();
        let pos = pos_of(text, "s.substing", 2);
        let range = text.range(Span::new(pos, BytePos::new(pos.line, pos.col + 8)));
        let diagnostic = Diagnostic {
            range,
            severity: Some(DiagnosticSeverity::ERROR),
            code: None,
            code_description: None,
            source: Some("cavvy".to_string()),
            message: "Unknown String method 'substing'".to_string(),
            related_information: None,
            tags: None,
            data: None,
        };
        let actions = code_action_impl(&analysis, &[diagnostic]).expect("应有 quick fix");
        let CodeActionOrCommand::CodeAction(action) = &actions[0] else {
            panic!("期望 CodeAction");
        };
        assert!(action.title.contains("substring"), "{}", action.title);
    }

    #[test]
    fn static_completion_lists_string_value_of_and_instance_does_not() {
        let source = "public class Foo {\n    public void run() {\n        String s = \"x\";\n        String t = String.valueOf(1);\n        int n = s.length();\n    }\n}";
        let analysis = analyze(source);
        let text = analysis.main_text().unwrap();
        let static_items = completion_impl(
            None,
            &analysis,
            pos_of(text, "String.valueOf", "String.".len()),
        );
        assert!(
            static_items.iter().any(|i| i.label == "valueOf"),
            "String. 应提供静态 valueOf"
        );
        let instance_items = completion_impl(None, &analysis, pos_of(text, "s.length", 2));
        assert!(
            !instance_items.iter().any(|i| i.label == "valueOf"),
            "实例补全不应包含静态 valueOf"
        );
    }
}
