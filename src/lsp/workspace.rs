//! 工作区索引：跨文件定义跳转、工作区符号与引用候选文件。
//!
//! 索引使用轻量解析（去掉预处理指令后做一次词法/语法分析），不运行语义分析，
//! 因此可以在后台快速扫描大型仓库。需要精确引用时由服务器对候选文件再做完整分析。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dashmap::DashMap;
use tower_lsp::lsp_types::{Location, Url};

use crate::ast::{ClassMember, Program};
use crate::lexer;
use crate::parser;

use super::builtins::BuiltinIndex;
use super::model::{SymbolKey, SymbolKind};
use super::util::{BytePos, LocatedSpan, Span, TextFile, path_to_uri};

const MAX_INDEXED_FILES: usize = 4000;

#[derive(Clone, Debug)]
pub struct IndexedSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub key: SymbolKey,
    pub location: LocatedSpan,
    pub detail: String,
    pub container: Option<String>,
}

#[derive(Clone, Debug)]
pub struct IndexedFile {
    pub path: String,
    pub uri: Url,
    pub content: Arc<String>,
    pub modified: Option<std::time::SystemTime>,
    pub symbols: Vec<IndexedSymbol>,
}

#[derive(Debug)]
pub struct WorkspaceIndex {
    roots: Mutex<Vec<PathBuf>>,
    builtins: Arc<BuiltinIndex>,
    files: DashMap<String, IndexedFile>,
    pub indexed: Mutex<bool>,
}

impl WorkspaceIndex {
    pub fn new(roots: Vec<PathBuf>, builtins: Arc<BuiltinIndex>) -> Self {
        Self {
            roots: Mutex::new(roots),
            builtins,
            files: DashMap::new(),
            indexed: Mutex::new(false),
        }
    }

    pub fn set_roots(&self, roots: Vec<PathBuf>) {
        if let Ok(mut guard) = self.roots.lock() {
            *guard = roots;
        }
    }

    pub fn roots(&self) -> Vec<PathBuf> {
        self.roots.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn builtins(&self) -> &Arc<BuiltinIndex> {
        &self.builtins
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn indexed_files(&self) -> Vec<IndexedFile> {
        self.files.iter().map(|e| e.value().clone()).collect()
    }

    /// 扫描所有工作区根目录下的 `.cay` 文件。同步执行，调用方可在后台线程运行。
    pub fn build(&self) {
        let roots = self.roots();
        let mut paths = Vec::new();
        for root in &roots {
            collect_cay_files(root, &mut paths, 0);
        }
        paths.sort();
        paths.dedup();
        self.files.clear();
        for path in paths.into_iter().take(MAX_INDEXED_FILES) {
            let _ = self.index_file(&path);
        }
        if let Ok(mut guard) = self.indexed.lock() {
            *guard = true;
        }
    }

    /// 重新索引单个文件（保存后调用）。
    pub fn index_file(&self, path: &Path) -> Option<()> {
        if !path.is_file() {
            self.files.remove(&path.to_string_lossy().to_string());
            return None;
        }
        let path_string = path.to_string_lossy().into_owned();
        let modified = std::fs::metadata(path).ok().and_then(|m| m.modified().ok());
        if let Some(existing) = self.files.get(&path_string) {
            if existing.modified == modified && modified.is_some() {
                return Some(());
            }
        }
        let content = std::fs::read_to_string(path).ok()?;
        let text = TextFile::new(path_string.clone(), content.clone());
        let symbols = scan_file_symbols(&text, &content);
        let uri = path_to_uri(path);
        self.files.insert(
            path_string.clone(),
            IndexedFile {
                path: path_string,
                uri,
                content: Arc::new(content),
                modified,
                symbols,
            },
        );
        Some(())
    }

    pub fn remove_file(&self, path: &Path) {
        self.files.remove(&path.to_string_lossy().to_string());
    }

    /// 按符号键查找工作区内的定义。
    pub fn find_definition(&self, key: &SymbolKey) -> Option<LocatedSpan> {
        for entry in self.files.iter() {
            for symbol in &entry.symbols {
                if &symbol.key == key {
                    return Some(symbol.location.clone());
                }
            }
        }
        None
    }

    /// 按名称模糊搜索工作区符号。
    pub fn symbols_matching(&self, query: &str) -> Vec<IndexedSymbol> {
        let mut out = Vec::new();
        let query = query.trim().to_lowercase();
        for entry in self.files.iter() {
            for symbol in &entry.symbols {
                if query.is_empty()
                    || symbol.name.to_lowercase().contains(&query)
                    || subsequence(&symbol.name.to_lowercase(), &query)
                {
                    out.push(symbol.clone());
                }
            }
        }
        out.sort_by(|a, b| {
            let a_exact = a.name.to_lowercase() == query;
            let b_exact = b.name.to_lowercase() == query;
            b_exact
                .cmp(&a_exact)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.location.file.cmp(&b.location.file))
        });
        out.truncate(500);
        out
    }

    /// 文本中包含某个标识符的文件（引用搜索候选）。
    pub fn files_containing(&self, name: &str, limit: usize) -> Vec<IndexedFile> {
        let mut out = Vec::new();
        let needle = name.to_string();
        for entry in self.files.iter() {
            if entry.content.contains(&needle) {
                out.push(entry.value().clone());
                if out.len() >= limit {
                    break;
                }
            }
        }
        out
    }

    pub fn location_for(span: &LocatedSpan) -> Location {
        let text_uri = path_to_uri(&span.file);
        Location {
            uri: text_uri,
            range: tower_lsp::lsp_types::Range::new(
                tower_lsp::lsp_types::Position::new(span.span.start.line, span.span.start.col),
                tower_lsp::lsp_types::Position::new(span.span.end.line, span.span.end.col),
            ),
        }
    }
}

fn subsequence(haystack: &str, needle: &str) -> bool {
    let mut chars = haystack.chars();
    for n in needle.chars() {
        if !chars.any(|c| c == n) {
            return false;
        }
    }
    true
}

fn collect_cay_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 24 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.')
            || name == "target"
            || name == "node_modules"
            || name == "bin"
            || name == "__builtins__"
        {
            continue;
        }
        if path.is_dir() {
            collect_cay_files(&path, out, depth + 1);
        } else if path
            .extension()
            .map(|e| e == "cay" || e == "eol")
            .unwrap_or(false)
        {
            out.push(path);
        }
    }
}

fn strip_preprocessor_lines(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    for line in content.lines() {
        if line.trim_start().starts_with('#') {
            out.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

pub(crate) fn scan_file_symbols(text: &TextFile, content: &str) -> Vec<IndexedSymbol> {
    let stripped = strip_preprocessor_lines(content);
    let ast = lexer::lex(&stripped)
        .ok()
        .and_then(|tokens| parser::parse(tokens).ok());
    if let Some(ast) = ast {
        return symbols_from_ast(text, &ast);
    }
    // 回退：只扫描 class/interface/struct/enum 关键字。
    let mut out = Vec::new();
    let Ok(tokens) = lexer::lex(&stripped) else {
        return out;
    };
    let mut i = 0;
    while i + 1 < tokens.len() {
        let kind = match &tokens[i].token {
            lexer::Token::Class => Some(SymbolKind::Class),
            lexer::Token::Interface => Some(SymbolKind::Interface),
            lexer::Token::Struct => Some(SymbolKind::Struct),
            lexer::Token::Enum => Some(SymbolKind::Enum),
            _ => None,
        };
        if let Some(kind) = kind {
            if let lexer::Token::Identifier(name) = &tokens[i + 1].token {
                let span = Span::new(
                    BytePos::from_source_location(&tokens[i + 1].loc),
                    BytePos::new(
                        tokens[i + 1].loc.line.saturating_sub(1) as u32,
                        (tokens[i + 1].loc.column.saturating_sub(1) + name.len()) as u32,
                    ),
                );
                let key = match kind {
                    SymbolKind::Class => SymbolKey::Class(name.clone()),
                    SymbolKind::Interface => SymbolKey::Interface(name.clone()),
                    SymbolKind::Struct => SymbolKey::Struct(name.clone()),
                    SymbolKind::Enum => SymbolKey::Enum(name.clone()),
                    _ => SymbolKey::Class(name.clone()),
                };
                out.push(IndexedSymbol {
                    name: name.clone(),
                    kind,
                    key,
                    location: LocatedSpan::new(text.path.clone(), span),
                    detail: format!("{:?} {}", kind, name),
                    container: None,
                });
            }
        }
        i += 1;
    }
    out
}

fn symbols_from_ast(text: &TextFile, ast: &Program) -> Vec<IndexedSymbol> {
    let mut out = Vec::new();
    for class in &ast.classes {
        let name_span = declaration_name_span(text, &class.loc, "class", &class.name);
        out.push(IndexedSymbol {
            name: class.name.clone(),
            kind: SymbolKind::Class,
            key: SymbolKey::Class(class.name.clone()),
            location: LocatedSpan::new(text.path.clone(), name_span),
            detail: format!("class {}", class.name),
            container: None,
        });
        for member in &class.members {
            match member {
                ClassMember::Method(method) => {
                    let span = method_name_span(text, &method.loc, &method.name);
                    out.push(IndexedSymbol {
                        name: method.name.clone(),
                        kind: SymbolKind::Method,
                        key: SymbolKey::Method {
                            owner: class.name.clone(),
                            name: method.name.clone(),
                            arity: method.params.len(),
                        },
                        location: LocatedSpan::new(text.path.clone(), span),
                        detail: format!(
                            "{} {}({})",
                            method.return_type.display_name(),
                            method.name,
                            method.params.len()
                        ),
                        container: Some(class.name.clone()),
                    });
                }
                ClassMember::Field(field) => {
                    let span = field_name_span(text, &field.loc, &field.name);
                    out.push(IndexedSymbol {
                        name: field.name.clone(),
                        kind: SymbolKind::Field,
                        key: SymbolKey::Field {
                            owner: class.name.clone(),
                            name: field.name.clone(),
                        },
                        location: LocatedSpan::new(text.path.clone(), span),
                        detail: format!("{} {}", field.field_type.display_name(), field.name),
                        container: Some(class.name.clone()),
                    });
                }
                ClassMember::Constructor(ctor) => {
                    let span = declaration_name_span(text, &ctor.loc, "", &class.name);
                    out.push(IndexedSymbol {
                        name: format!("{}({})", class.name, ctor.params.len()),
                        kind: SymbolKind::Constructor,
                        key: SymbolKey::Constructor {
                            owner: class.name.clone(),
                            arity: ctor.params.len(),
                        },
                        location: LocatedSpan::new(text.path.clone(), span),
                        detail: format!("{}({} params)", class.name, ctor.params.len()),
                        container: Some(class.name.clone()),
                    });
                }
                _ => {}
            }
        }
    }
    for interface in &ast.interfaces {
        out.push(IndexedSymbol {
            name: interface.name.clone(),
            kind: SymbolKind::Interface,
            key: SymbolKey::Interface(interface.name.clone()),
            location: LocatedSpan::new(
                text.path.clone(),
                declaration_name_span(text, &interface.loc, "interface", &interface.name),
            ),
            detail: format!("interface {}", interface.name),
            container: None,
        });
    }
    for strukt in &ast.structs {
        out.push(IndexedSymbol {
            name: strukt.name.clone(),
            kind: SymbolKind::Struct,
            key: SymbolKey::Struct(strukt.name.clone()),
            location: LocatedSpan::new(
                text.path.clone(),
                declaration_name_span(text, &strukt.loc, "struct", &strukt.name),
            ),
            detail: format!("struct {}", strukt.name),
            container: None,
        });
    }
    for en in &ast.enums {
        out.push(IndexedSymbol {
            name: en.name.clone(),
            kind: SymbolKind::Enum,
            key: SymbolKey::Enum(en.name.clone()),
            location: LocatedSpan::new(
                text.path.clone(),
                declaration_name_span(text, &en.loc, "enum", &en.name),
            ),
            detail: format!("enum {}", en.name),
            container: None,
        });
        for variant in &en.variants {
            out.push(IndexedSymbol {
                name: variant.name.clone(),
                kind: SymbolKind::EnumVariant,
                key: SymbolKey::EnumVariant(en.name.clone(), variant.name.clone()),
                location: LocatedSpan::new(
                    text.path.clone(),
                    field_name_span(text, &variant.loc, &variant.name),
                ),
                detail: format!("{}.{}", en.name, variant.name),
                container: Some(en.name.clone()),
            });
        }
    }
    for func in &ast.top_level_functions {
        out.push(IndexedSymbol {
            name: func.name.clone(),
            kind: SymbolKind::Function,
            key: SymbolKey::Function(func.name.clone()),
            location: LocatedSpan::new(
                text.path.clone(),
                method_name_span(text, &func.loc, &func.name),
            ),
            detail: format!("{} {}(...)", func.return_type.display_name(), func.name),
            container: None,
        });
    }
    out
}

fn declaration_name_span(
    text: &TextFile,
    loc: &crate::miette_diagnostic::SourceLocation,
    keyword: &str,
    name: &str,
) -> Span {
    let start = BytePos::from_source_location(loc);
    if !keyword.is_empty() {
        if let Some((span, _)) = text.name_after_keyword(start, keyword) {
            return Span::new(span.start, span.end);
        }
    }
    text.find_word_after(start, name, &['{', ';', '(', ','])
        .unwrap_or_else(|| {
            Span::new(
                start,
                BytePos::new(start.line, start.col + name.len() as u32),
            )
        })
}

fn method_name_span(
    text: &TextFile,
    loc: &crate::miette_diagnostic::SourceLocation,
    name: &str,
) -> Span {
    let start = BytePos::from_source_location(loc);
    text.find_word_after(start, name, &['(', ';'])
        .unwrap_or_else(|| {
            Span::new(
                start,
                BytePos::new(start.line, start.col + name.len() as u32),
            )
        })
}

fn field_name_span(
    text: &TextFile,
    loc: &crate::miette_diagnostic::SourceLocation,
    name: &str,
) -> Span {
    let start = BytePos::from_source_location(loc);
    text.find_last_word_before(start, name, &[';', '=', ','])
        .or_else(|| text.find_word_after(start, name, &[';', '=', ',']))
        .unwrap_or_else(|| {
            Span::new(
                start,
                BytePos::new(start.line, start.col + name.len() as u32),
            )
        })
}

/// 判断路径是否应被工作区索引忽略（例如内置存根目录）。
pub fn should_skip_path(path: &Path) -> bool {
    path.components().any(|c| {
        let s = c.as_os_str().to_string_lossy();
        s == "__builtins__" || s == "target" || s == ".git"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_text_finds_classes_and_methods() {
        let text = TextFile::new(
            "a.cay",
            "public class Foo {\n public int bar() { return 1; }\n}\n",
        );
        let symbols = scan_file_symbols(&text, &*text.text);
        assert!(symbols.iter().any(|s| s.name == "Foo"));
        assert!(symbols.iter().any(|s| s.name == "bar"));
    }

    #[test]
    fn strip_preprocessor_preserves_lines() {
        let src = "#include \"a.cay\"\nint x;\n";
        let out = strip_preprocessor_lines(src);
        assert_eq!(out.lines().count(), 2);
        assert!(out.contains("int x;"));
    }

    #[test]
    fn workspace_index_finds_cross_file_class() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("a.cay"),
            "public class Alpha {\n    public int value;\n}\n",
        )
        .expect("write");
        let builtins = Arc::new(BuiltinIndex::load_from_root(&PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/caylibs/__builtins__"
        ))));
        let index = WorkspaceIndex::new(vec![dir.path().to_path_buf()], builtins);
        index.build();
        let definition = index
            .find_definition(&SymbolKey::Class("Alpha".to_string()))
            .expect("应找到 Alpha");
        assert!(definition.file.ends_with("a.cay"));
        assert!(!index.symbols_matching("Alpha").is_empty());
    }
}
