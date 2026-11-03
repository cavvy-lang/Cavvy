//! 文档分析流水线：预处理 → 词法 → 语法 → 语义 → 编辑器语义模型。
//!
//! 与编译器唯一的区别是：这里不生成代码，并且会额外构建 LSP 需要的符号/引用索引。
//!
//! 分析结果全部是 `Send + Sync` 的纯数据，可以安全地缓存到 `DashMap` 中供并发请求读取。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location, NumberOrString,
    Position, Range,
};

use crate::lexer::{Lexer, TokenWithLocation};
use crate::miette_diagnostic::CayError;
use crate::parser;
use crate::preprocessor::Preprocessor;
use crate::semantic::SemanticAnalyzer;
use crate::types::TypeRegistry;

use super::builtins::BuiltinIndex;
use super::model::{Model, build_model};
use super::util::{BytePos, Span, TextFile, path_to_uri};

#[derive(Debug)]
pub struct Analysis {
    pub uri: String,
    pub path: String,
    pub version: i32,
    pub content: Arc<String>,
    pub files: HashMap<String, Arc<TextFile>>,
    pub diagnostics: Vec<Diagnostic>,
    /// 每条诊断对应的源文件（用于把 include 文件错误发布到正确 URI）。
    pub diagnostic_records: Vec<(String, Diagnostic)>,
    pub model: Model,
    pub builtins: Arc<BuiltinIndex>,
    pub include_paths: Vec<PathBuf>,
    pub registry: TypeRegistry,
    /// 带源映射的 token（用于 semantic tokens；只保留主文件）。
    pub tokens: Vec<TokenWithLocation>,
    pub parse_ok: bool,
}

impl Analysis {
    pub fn empty(
        uri: &str,
        path: &str,
        version: i32,
        content: &str,
        diagnostics: Vec<Diagnostic>,
        builtins: Arc<BuiltinIndex>,
        include_paths: Vec<PathBuf>,
    ) -> Self {
        let text = Arc::new(TextFile::new(path.to_string(), content.to_string()));
        let mut files = HashMap::new();
        files.insert(path.to_string(), text);
        Self {
            uri: uri.to_string(),
            path: path.to_string(),
            version,
            content: Arc::new(content.to_string()),
            files,
            diagnostics,
            diagnostic_records: Vec::new(),
            model: Model {
                files: HashMap::new(),
                main_file: path.to_string(),
                ..Default::default()
            },
            builtins,
            include_paths,
            registry: TypeRegistry::new(),
            tokens: Vec::new(),
            parse_ok: false,
        }
    }

    pub fn text(&self, file: &str) -> Option<&TextFile> {
        self.files.get(file).map(|f| f.as_ref())
    }

    pub fn main_text(&self) -> Option<&TextFile> {
        self.text(&self.path)
    }

    pub fn diagnostics_for(&self, file: &str) -> Vec<Diagnostic> {
        self.diagnostic_records
            .iter()
            .filter(|(f, _)| same_file(f, file))
            .map(|(_, d)| d.clone())
            .collect()
    }

    pub fn diagnostics_for_main(&self) -> Vec<Diagnostic> {
        self.diagnostics_for(&self.path)
    }
}

/// 分析入口。`overlays` 用于让尚未保存的已打开文件参与 include 解析（当前预处理器
/// 只从磁盘读取 include，overlays 主要用于源文件文本缓存）。
pub fn analyze_document(
    uri: &str,
    path: &str,
    content: &str,
    version: i32,
    builtins: Arc<BuiltinIndex>,
    include_paths: &[PathBuf],
    overlays: &HashMap<String, Arc<String>>,
) -> Analysis {
    let main_text = Arc::new(TextFile::new(path.to_string(), content.to_string()));
    let base_dir = Path::new(path)
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    // 系统 include 路径：调用方传入的 caylibs 等。
    let mut system_paths: Vec<PathBuf> = include_paths.to_vec();
    system_paths.retain(|p| p.is_dir());
    if !system_paths.contains(&base_dir) {
        system_paths.insert(0, base_dir.clone());
    }

    let mut pp = Preprocessor::with_include_paths(&base_dir, system_paths.clone());
    pp.set_suppress_warnings(true);
    let processed = match pp.process_with_source_map(content, path) {
        Ok(r) => r,
        Err(err) => {
            let mut files = HashMap::new();
            files.insert(path.to_string(), main_text.clone());
            let diagnostic_records = errors_to_diagnostic_records(&err, path, &files, path);
            let diagnostics = records_to_diagnostics(&diagnostic_records);
            return Analysis {
                uri: uri.to_string(),
                path: path.to_string(),
                version,
                content: Arc::new(content.to_string()),
                files,
                diagnostics,
                diagnostic_records,
                model: Model {
                    files: HashMap::new(),
                    main_file: path.to_string(),
                    ..Default::default()
                },
                builtins,
                include_paths: include_paths.to_vec(),
                registry: TypeRegistry::new(),
                tokens: Vec::new(),
                parse_ok: false,
            };
        }
    };

    // 收集主文件与所有 include 文件的文本（用于位置换算与定义跳转）。
    let preprocessor_warnings = pp.take_warnings();
    let mut files: HashMap<String, Arc<TextFile>> = HashMap::new();
    files.insert(path.to_string(), main_text.clone());
    for pos in &processed.source_map.mappings {
        if files.contains_key(&pos.file) {
            continue;
        }
        let text = if let Some(overlay) = overlays.get(&pos.file) {
            overlay.clone()
        } else if let Ok(read) = std::fs::read_to_string(&pos.file) {
            Arc::new(read)
        } else {
            Arc::new(String::new())
        };
        files.insert(
            pos.file.clone(),
            Arc::new(TextFile::new(pos.file.clone(), (*text).clone())),
        );
    }
    if !files.contains_key(path) {
        files.insert(path.to_string(), main_text.clone());
    }

    let source_map: HashMap<usize, (String, usize)> = processed
        .source_map
        .mappings
        .iter()
        .enumerate()
        .map(|(idx, pos)| (idx + 1, (pos.file.clone(), pos.line)))
        .collect();

    let mut lexer = Lexer::with_source_map_and_file(
        &processed.code,
        source_map.clone(),
        Some(path.to_string()),
    )
    .with_collect_all_errors();
    let tokens = match lexer.tokenize() {
        Ok(tokens) => tokens,
        Err(err) => {
            let mut diagnostic_records =
                preprocessor_warning_records(&preprocessor_warnings, &files);
            diagnostic_records.extend(errors_to_diagnostic_records(&err, path, &files, path));
            for e in lexer.diagnostics() {
                diagnostic_records.extend(errors_to_diagnostic_records(e, path, &files, path));
            }
            let diagnostics = records_to_diagnostics(&diagnostic_records);
            return Analysis {
                uri: uri.to_string(),
                path: path.to_string(),
                version,
                content: Arc::new(content.to_string()),
                files,
                diagnostics,
                diagnostic_records,
                model: Model {
                    files: HashMap::new(),
                    main_file: path.to_string(),
                    ..Default::default()
                },
                builtins,
                include_paths: include_paths.to_vec(),
                registry: TypeRegistry::new(),
                tokens: Vec::new(),
                parse_ok: false,
            };
        }
    };
    let mut diagnostic_records: Vec<(String, Diagnostic)> =
        preprocessor_warning_records(&preprocessor_warnings, &files);
    diagnostic_records.extend(
        lexer
            .diagnostics()
            .iter()
            .flat_map(|e| errors_to_diagnostic_records(e, path, &files, path)),
    );
    for warning in lexer.take_warnings() {
        diagnostic_records.extend(errors_to_diagnostic_records(&warning, path, &files, path));
    }

    let ast = match parser::parse_with_source(tokens.clone(), processed.code.clone()) {
        Ok(ast) => ast,
        Err(err) => {
            diagnostic_records.extend(errors_to_diagnostic_records(&err, path, &files, path));
            let diagnostics = records_to_diagnostics(&diagnostic_records);
            return Analysis {
                uri: uri.to_string(),
                path: path.to_string(),
                version,
                content: Arc::new(content.to_string()),
                files,
                diagnostics,
                diagnostic_records,
                model: Model {
                    files: HashMap::new(),
                    main_file: path.to_string(),
                    ..Default::default()
                },
                builtins,
                include_paths: include_paths.to_vec(),
                registry: TypeRegistry::new(),
                tokens,
                parse_ok: false,
            };
        }
    };

    // 语义分析。即使失败也保留已经构建出的类型注册表（用于补全/悬停）。
    let mut analyzer = SemanticAnalyzer::new();
    analyzer.set_current_file(Some(path.to_string()));
    analyzer.set_source_map(source_map);
    let analyzed = analyzer.analyze(ast.clone());
    if let Err(err) = &analyzed {
        diagnostic_records.extend(errors_to_diagnostic_records(err, path, &files, path));
    }
    let diagnostics = records_to_diagnostics(&diagnostic_records);
    let registry = analyzer.type_registry().clone();
    let program = match analyzed {
        Ok(program) => program,
        Err(_) => ast.flatten_namespaces(),
    };

    let model = build_model(
        &program,
        &registry,
        builtins.as_ref(),
        files.clone(),
        path.to_string(),
    );

    // 只保留主文件的 token 给 semantic tokens（include 文件由各自文档处理）。
    let main_tokens: Vec<TokenWithLocation> = tokens
        .into_iter()
        .filter(|t| {
            t.loc
                .file
                .as_deref()
                .map(|f| same_file(f, path))
                .unwrap_or(true)
        })
        .collect();

    Analysis {
        uri: uri.to_string(),
        path: path.to_string(),
        version,
        content: Arc::new(content.to_string()),
        files,
        diagnostics,
        diagnostic_records,
        model,
        builtins,
        include_paths: include_paths.to_vec(),
        registry,
        tokens: main_tokens,
        parse_ok: true,
    }
}

pub(crate) fn same_file(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    let a = Path::new(a);
    let b = Path::new(b);
    a == b || a.canonicalize().ok() == b.canonicalize().ok()
}

/// 将任意 `CayError`（包含 `MultipleErrors`）展开成 LSP 诊断。
pub fn errors_to_diagnostics(
    error: &CayError,
    main_file: &str,
    files: &HashMap<String, Arc<TextFile>>,
    default_file: &str,
) -> Vec<Diagnostic> {
    records_to_diagnostics(&errors_to_diagnostic_records(
        error,
        main_file,
        files,
        default_file,
    ))
}

/// 与 [`errors_to_diagnostics`] 相同，但保留每条诊断所属的源文件。
pub fn errors_to_diagnostic_records(
    error: &CayError,
    main_file: &str,
    files: &HashMap<String, Arc<TextFile>>,
    default_file: &str,
) -> Vec<(String, Diagnostic)> {
    match error {
        CayError::MultipleErrors { errors } => errors
            .iter()
            .flat_map(|e| errors_to_diagnostic_records(e, main_file, files, default_file))
            .collect(),
        other => {
            let file = describe_error(other)
                .3
                .unwrap_or_else(|| default_file.to_string());
            vec![(
                file,
                error_to_diagnostic(other, main_file, files, default_file),
            )]
        }
    }
}

pub fn records_to_diagnostics(records: &[(String, Diagnostic)]) -> Vec<Diagnostic> {
    records.iter().map(|(_, d)| d.clone()).collect()
}

fn preprocessor_warning_records(
    warnings: &[(String, usize, String)],
    files: &HashMap<String, Arc<TextFile>>,
) -> Vec<(String, Diagnostic)> {
    let mut out = Vec::new();
    for (file, line, message) in warnings {
        let text = files.get(file);
        let line0 = line.saturating_sub(1) as u32;
        let range = text
            .map(|t| {
                let start = BytePos::new(line0, 0);
                let end = BytePos::new(line0, t.line(line0).len() as u32);
                t.range(Span::new(start, end))
            })
            .unwrap_or_else(|| Range::new(Position::new(line0, 0), Position::new(line0, 0)));
        out.push((
            file.clone(),
            Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::WARNING),
                code: Some(NumberOrString::String("W0001".to_string())),
                code_description: None,
                source: Some("cavvy".to_string()),
                message: format!("#warning: {}", message),
                related_information: None,
                tags: None,
                data: None,
            },
        ));
    }
    out
}

pub fn error_to_diagnostic(
    error: &CayError,
    main_file: &str,
    files: &HashMap<String, Arc<TextFile>>,
    default_file: &str,
) -> Diagnostic {
    let (code, message, suggestion, file, line, column, severity) = describe_error(error);
    let file = file.unwrap_or_else(|| default_file.to_string());
    let text = files.get(&file);
    let line0 = line.saturating_sub(1) as u32;
    let col0 = column.saturating_sub(1) as u32;
    let (range, end_pos) = if let Some(text) = text {
        if line == 0 {
            (
                Range::new(Position::new(0, 0), Position::new(0, 0)),
                BytePos::new(0, 0),
            )
        } else {
            let start = BytePos::new(line0, col0);
            let end = text
                .word_at(start)
                .map(|(span, _)| span.end)
                .unwrap_or_else(|| BytePos::new(line0, text.line(start.line).len() as u32));
            (text.range(Span::new(start, end)), end)
        }
    } else {
        (
            Range::new(Position::new(line0, col0), Position::new(line0, col0)),
            BytePos::new(line0, col0),
        )
    };

    let related = if !same_file(&file, main_file) {
        text.map(|t| {
            vec![DiagnosticRelatedInformation {
                location: Location {
                    uri: path_to_uri(&file),
                    range: t.range(Span::new(BytePos::new(line0, col0), end_pos)),
                },
                message: format!("错误位于包含文件: {}", file),
            }]
        })
    } else {
        None
    };

    let full_message = if suggestion.trim().is_empty() || message.contains(&suggestion) {
        message
    } else {
        format!("{}\n\n建议: {}", message, suggestion)
    };

    Diagnostic {
        range,
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_string())),
        code_description: None,
        source: Some("cavvy".to_string()),
        message: full_message,
        related_information: related,
        tags: None,
        data: None,
    }
}

#[allow(clippy::type_complexity)]
fn describe_error(
    error: &CayError,
) -> (
    &'static str,
    String,
    String,
    Option<String>,
    usize,
    usize,
    DiagnosticSeverity,
) {
    match error {
        CayError::Lexer {
            error_code,
            file,
            line,
            column,
            message,
            suggestion,
        } => (
            error_code,
            message.clone(),
            suggestion.clone(),
            file.clone(),
            *line,
            *column,
            DiagnosticSeverity::ERROR,
        ),
        CayError::Parser {
            error_code,
            file,
            line,
            column,
            message,
            suggestion,
        } => (
            error_code,
            message.clone(),
            suggestion.clone(),
            file.clone(),
            *line,
            *column,
            DiagnosticSeverity::ERROR,
        ),
        CayError::Semantic {
            error_code,
            file,
            line,
            column,
            message,
            suggestion,
        } => (
            error_code,
            message.clone(),
            suggestion.clone(),
            file.clone(),
            *line,
            *column,
            DiagnosticSeverity::ERROR,
        ),
        CayError::CodeGen {
            error_code,
            kind,
            file,
            line,
            column,
            message,
            suggestion,
            is_warning,
        } => (
            error_code,
            if kind.is_empty() {
                message.clone()
            } else {
                format!("{}: {}", kind, message)
            },
            suggestion.clone(),
            file.clone(),
            *line,
            *column,
            if *is_warning {
                DiagnosticSeverity::WARNING
            } else {
                DiagnosticSeverity::ERROR
            },
        ),
        CayError::Io {
            error_code,
            file,
            message,
        } => (
            error_code,
            message.clone(),
            String::new(),
            file.clone(),
            0,
            0,
            DiagnosticSeverity::ERROR,
        ),
        CayError::Llvm(message) => (
            "E9001",
            message.clone(),
            String::new(),
            None,
            0,
            0,
            DiagnosticSeverity::ERROR,
        ),
        CayError::TypeMismatch {
            error_code,
            file,
            line,
            column,
            message,
            expected,
            actual,
            suggestion,
        } => (
            error_code,
            format!("{}（期望 {}，实际 {}）", message, expected, actual),
            suggestion.clone(),
            file.clone(),
            *line,
            *column,
            DiagnosticSeverity::ERROR,
        ),
        CayError::UndefinedIdentifier {
            error_code,
            file,
            line,
            column,
            name,
            suggestion,
        } => (
            error_code,
            format!("未定义标识符 '{}'", name),
            suggestion.clone(),
            file.clone(),
            *line,
            *column,
            DiagnosticSeverity::ERROR,
        ),
        CayError::DuplicateDefinition {
            error_code,
            file,
            line,
            column,
            name,
            suggestion,
        } => (
            error_code,
            format!("重复定义 '{}'", name),
            suggestion.clone(),
            file.clone(),
            *line,
            *column,
            DiagnosticSeverity::ERROR,
        ),
        CayError::Preprocessor {
            error_code,
            file,
            line,
            column,
            message,
            suggestion,
        } => (
            error_code,
            message.clone(),
            suggestion.clone(),
            file.clone(),
            *line,
            *column,
            DiagnosticSeverity::ERROR,
        ),
        CayError::Lint {
            error_code,
            phase_label,
            file,
            line,
            column,
            message,
            suggestion,
        } => (
            error_code,
            format!("{}: {}", phase_label, message),
            suggestion.clone(),
            file.clone(),
            *line,
            *column,
            DiagnosticSeverity::WARNING,
        ),
        CayError::MultipleErrors { .. } => (
            "E9999",
            "多个错误".to_string(),
            String::new(),
            None,
            0,
            0,
            DiagnosticSeverity::ERROR,
        ),
    }
}

/// 从 `#include "..."` / `#include <...>` 行中提取路径及其所在范围。
pub fn find_include_directives(content: &str) -> Vec<(String, Span)> {
    let mut out = Vec::new();
    for (line_idx, line) in content.lines().enumerate() {
        let Some(hash) = line.find('#') else {
            continue;
        };
        let rest = &line[hash + 1..];
        let trimmed = rest.trim_start();
        if !trimmed.starts_with("include") {
            continue;
        }
        let after_include = hash + 1 + (rest.len() - trimmed.len()) + "include".len();
        let rest = &line[after_include..];
        let Some(quote_rel) = rest.find(['"', '<']) else {
            continue;
        };
        let quote_idx = after_include + quote_rel;
        let Some(quote) = line[quote_idx..].chars().next() else {
            continue;
        };
        let close = if quote == '<' { '>' } else { '"' };
        let path_start = quote_idx + quote.len_utf8();
        let Some(close_rel) = line[path_start..].find(close) else {
            continue;
        };
        let path = line[path_start..path_start + close_rel].to_string();
        if path.is_empty() {
            continue;
        }
        out.push((
            path.clone(),
            Span::new(
                BytePos::new(line_idx as u32, path_start as u32),
                BytePos::new(line_idx as u32, (path_start + path.len()) as u32),
            ),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

    #[test]
    fn analyzes_simple_document_and_collects_symbols() {
        let analysis = analyze(
            r#"
            /// 文档注释
            public class Foo {
                public int count;
                public int getCount() {
                    return this.count;
                }
            }
            "#,
        );
        assert!(analysis.parse_ok, "diagnostics: {:?}", analysis.diagnostics);
        assert!(
            analysis
                .model
                .symbols
                .iter()
                .any(|s| s.name == "Foo" && s.documentation.contains("文档注释"))
        );
        assert!(analysis.model.symbols.iter().any(|s| s.name == "count"));
        assert!(analysis.model.symbols.iter().any(|s| s.name == "getCount"));
    }

    #[test]
    fn reports_semantic_diagnostics() {
        let analysis = analyze(
            r#"
            public class Foo {
                public void run() {
                    int x = "not an int";
                }
            }
            "#,
        );
        assert!(!analysis.diagnostics.is_empty());
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        );
    }

    #[test]
    fn finds_include_directives() {
        let inc = find_include_directives("#include \"foo.cay\"\n#include <std/vector.cay>\n");
        assert_eq!(inc.len(), 2);
        assert_eq!(inc[0].0, "foo.cay");
        assert_eq!(inc[1].0, "std/vector.cay");
    }

    #[test]
    fn preprocessor_warning_becomes_lsp_warning() {
        let analysis = analyze("#warning \"deprecated API\"\npublic class Foo {}\n");
        assert!(
            analysis.diagnostics.iter().any(|d| {
                d.severity == Some(DiagnosticSeverity::WARNING)
                    && d.message.contains("deprecated API")
            }),
            "诊断: {:?}",
            analysis.diagnostics
        );
    }

    #[test]
    fn diagnostics_records_keep_included_file_origin() {
        let dir = tempfile::tempdir().expect("tempdir");
        let main_path = dir.path().join("main.cay");
        let inc_path = dir.path().join("inc.cay");
        std::fs::write(
            &inc_path,
            "public class Inc {\n    public void bad() {\n        int x = \"oops\";\n    }\n}\n",
        )
        .expect("write inc");
        let source = "#include \"inc.cay\"\npublic class Main {\n    public void ok() {}\n}\n";
        std::fs::write(&main_path, source).expect("write main");
        let analysis = analyze_document(
            "file:///main.cay",
            &main_path.to_string_lossy(),
            source,
            1,
            builtins(),
            &[],
            &HashMap::new(),
        );
        assert!(
            !analysis.diagnostics.is_empty(),
            "include 文件中的类型错误应产生诊断"
        );
        assert!(
            analysis
                .diagnostic_records
                .iter()
                .any(|(file, _)| file.ends_with("inc.cay")),
            "诊断记录应保留 inc.cay 来源: {:?}",
            analysis.diagnostic_records
        );
        assert!(analysis.diagnostics_for_main().is_empty());
    }
}
