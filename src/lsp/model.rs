//! LSP 语义模型：从 AST 构建符号表、词法作用域、引用/声明位置与类型信息。
//!
//! 语义分析器负责诊断与类型注册表；本模块在同一个 AST 上做一次面向编辑器的
//! “索引化”遍历，产出的都是 `Send + Sync` 的纯数据，便于缓存与跨请求共享。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::ast::*;
use crate::miette_diagnostic::SourceLocation;
use crate::types::{
    ClassInfo, FieldInfo, MethodInfo, ParameterInfo, Type, TypeRegistry, substitute_type_params,
};

use super::builtins::{BuiltinIndex, BuiltinMember, BuiltinMemberKind, ParamSig, display_type};
use super::util::{BytePos, LocatedSpan, Span, TextFile, is_word_char};

pub type SymbolId = usize;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKey {
    Class(String),
    Interface(String),
    Struct(String),
    Enum(String),
    EnumVariant(String, String),
    Constructor {
        owner: String,
        arity: usize,
    },
    Method {
        owner: String,
        name: String,
        arity: usize,
    },
    Field {
        owner: String,
        name: String,
    },
    Function(String),
    Local {
        file: String,
        line: u32,
        col: u32,
    },
    Parameter {
        file: String,
        line: u32,
        col: u32,
    },
    BuiltinType(String),
    BuiltinMethod {
        owner: String,
        name: String,
        arity: usize,
    },
    BuiltinField {
        owner: String,
        name: String,
    },
    BuiltinFunction {
        name: String,
        arity: usize,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Class,
    Interface,
    Struct,
    Enum,
    EnumVariant,
    Method,
    Constructor,
    Field,
    Function,
    Local,
    Parameter,
    BuiltinType,
    BuiltinMember,
}

impl SymbolKind {
    pub fn is_type_like(self) -> bool {
        matches!(
            self,
            SymbolKind::Class
                | SymbolKind::Interface
                | SymbolKind::Struct
                | SymbolKind::Enum
                | SymbolKind::BuiltinType
        )
    }

    pub fn lsp(self) -> tower_lsp::lsp_types::SymbolKind {
        use tower_lsp::lsp_types::SymbolKind as K;
        match self {
            SymbolKind::Class => K::CLASS,
            SymbolKind::Interface => K::INTERFACE,
            SymbolKind::Struct => K::STRUCT,
            SymbolKind::Enum => K::ENUM,
            SymbolKind::EnumVariant => K::ENUM_MEMBER,
            SymbolKind::Method => K::METHOD,
            SymbolKind::Constructor => K::CONSTRUCTOR,
            SymbolKind::Field => K::FIELD,
            SymbolKind::Function => K::FUNCTION,
            SymbolKind::Local => K::VARIABLE,
            SymbolKind::Parameter => K::VARIABLE,
            SymbolKind::BuiltinType => K::CLASS,
            SymbolKind::BuiltinMember => K::METHOD,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

impl Visibility {
    pub fn from_modifiers(modifiers: &[Modifier]) -> Self {
        if modifiers.contains(&Modifier::Private) {
            Visibility::Private
        } else if modifiers.contains(&Modifier::Protected) {
            Visibility::Protected
        } else {
            Visibility::Public
        }
    }
}

#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: SymbolId,
    pub key: SymbolKey,
    pub name: String,
    pub kind: SymbolKind,
    pub detail: String,
    pub documentation: String,
    pub ty: Option<Type>,
    pub signature: String,
    pub location: LocatedSpan,
    pub selection: LocatedSpan,
    pub owner: Option<String>,
    pub is_static: bool,
    pub params: Vec<ParamSig>,
    pub return_type: Option<Type>,
    pub children: Vec<SymbolId>,
    pub visibility: Visibility,
}

impl Symbol {
    pub fn type_like(&self) -> bool {
        self.kind.is_type_like()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OccurrenceKind {
    Value,
    Type,
    Member,
    MemberCall,
    Call,
    Declaration,
    Namespace,
}

#[derive(Clone, Debug)]
pub struct Occurrence {
    pub file: String,
    pub span: Span,
    pub name: String,
    pub kind: OccurrenceKind,
    pub symbol: Option<SymbolId>,
    pub receiver: Option<Type>,
    pub result: Option<Type>,
    pub is_definition: bool,
}

#[derive(Clone, Debug)]
pub struct Scope {
    pub file: String,
    pub span: Span,
    pub parent: Option<usize>,
    pub names: HashMap<String, SymbolId>,
    pub class: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Signature {
    pub label: String,
    pub params: Vec<ParamSig>,
    pub return_type: Type,
    pub documentation: String,
    pub location: Option<LocatedSpan>,
    pub symbol: Option<SymbolId>,
    pub is_static: bool,
}

#[derive(Clone, Debug)]
pub struct CallSite {
    pub file: String,
    pub call_span: Span,
    pub open: BytePos,
    pub close: BytePos,
    pub callee: Span,
    pub callee_name: String,
    pub receiver: Option<Type>,
    pub signatures: Vec<Signature>,
    pub arg_spans: Vec<Span>,
}

#[derive(Clone, Debug)]
pub struct Folding {
    pub file: String,
    pub span: Span,
    pub kind: &'static str,
}

#[derive(Clone, Debug)]
pub struct InlayHintData {
    pub file: String,
    pub pos: BytePos,
    pub label: String,
}

#[derive(Clone, Debug, Default)]
pub struct Model {
    pub symbols: Vec<Symbol>,
    pub by_key: HashMap<SymbolKey, Vec<SymbolId>>,
    pub occurrences: Vec<Occurrence>,
    pub scopes: Vec<Scope>,
    pub calls: Vec<CallSite>,
    pub type_at_start: HashMap<(String, usize), Type>,
    pub root_symbols: Vec<SymbolId>,
    pub class_ranges: Vec<(String, Span, String)>,
    pub folding: Vec<Folding>,
    pub inlay_hints: Vec<InlayHintData>,
    pub files: HashMap<String, Arc<TextFile>>,
    pub main_file: String,
}

impl Model {
    pub fn symbol(&self, id: SymbolId) -> Option<&Symbol> {
        self.symbols.get(id)
    }

    pub fn symbol_for_key(&self, key: &SymbolKey) -> Option<&Symbol> {
        self.by_key
            .get(key)
            .and_then(|ids| ids.first())
            .and_then(|id| self.symbols.get(*id))
    }

    pub fn occurrence_at(&self, file: &str, pos: BytePos) -> Option<&Occurrence> {
        let mut best: Option<&Occurrence> = None;
        for occ in &self.occurrences {
            if occ.file != file {
                continue;
            }
            if occ.span.contains(pos) {
                let better = best
                    .map(|b| {
                        span_len(occ.span) < span_len(b.span)
                            || (span_len(occ.span) == span_len(b.span)
                                && occ.span.start > b.span.start)
                    })
                    .unwrap_or(true);
                if better {
                    best = Some(occ);
                }
            }
        }
        best
    }

    pub fn occurrence_at_word(&self, file: &str, pos: BytePos) -> Option<&Occurrence> {
        if let Some(occ) = self.occurrence_at(file, pos) {
            return Some(occ);
        }
        let text = self.files.get(file)?;
        let (span, _) = text.word_at(pos)?;
        self.occurrence_at(file, span.start)
    }

    pub fn occurrence_for_symbol(&self, id: SymbolId) -> Option<&Occurrence> {
        self.occurrences
            .iter()
            .find(|o| o.symbol == Some(id) && o.is_definition)
    }

    pub fn symbol_at(&self, file: &str, pos: BytePos) -> Option<&Symbol> {
        self.occurrence_at_word(file, pos)
            .and_then(|o| o.symbol)
            .and_then(|id| self.symbol(id))
    }

    pub fn scope_at(&self, file: &str, pos: BytePos) -> Option<usize> {
        let mut best: Option<(usize, usize)> = None; // (start, index)
        for (idx, scope) in self.scopes.iter().enumerate() {
            if scope.file != file {
                continue;
            }
            if scope.span.contains(pos) {
                let start = absolute_pos(scope.file.as_str(), scope.span.start, &self.files);
                if best.map(|(s, _)| start >= s).unwrap_or(true) {
                    best = Some((start, idx));
                }
            }
        }
        best.map(|(_, idx)| idx)
    }

    pub fn visible_locals(&self, file: &str, pos: BytePos) -> Vec<SymbolId> {
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut current = self.scope_at(file, pos);
        while let Some(idx) = current {
            let Some(scope) = self.scopes.get(idx) else {
                break;
            };
            let mut names: Vec<(&String, &SymbolId)> = scope.names.iter().collect();
            names.sort_by(|a, b| a.0.cmp(b.0));
            for (name, id) in names {
                if seen.insert(name.clone()) {
                    out.push(*id);
                }
            }
            current = scope.parent;
        }
        out
    }

    pub fn class_at(&self, file: &str, pos: BytePos) -> Option<String> {
        let mut best: Option<(usize, String)> = None;
        for (name, span, f) in &self.class_ranges {
            if f != file || !span.contains(pos) {
                continue;
            }
            let start = absolute_pos(file, span.start, &self.files);
            if best.as_ref().map(|(s, _)| start >= *s).unwrap_or(true) {
                best = Some((start, name.clone()));
            }
        }
        best.map(|(_, name)| name)
    }

    pub fn call_at(&self, file: &str, pos: BytePos) -> Option<&CallSite> {
        let mut best: Option<&CallSite> = None;
        for call in &self.calls {
            if call.file != file {
                continue;
            }
            if pos >= call.open && pos <= call.close {
                let better = best
                    .map(|b| span_len(call.call_span) < span_len(b.call_span))
                    .unwrap_or(true);
                if better {
                    best = Some(call);
                }
            }
        }
        best
    }

    pub fn type_at(&self, file: &str, start: BytePos) -> Option<&Type> {
        let offset = self.files.get(file)?.offset(start);
        self.type_at_start.get(&(file.to_string(), offset))
    }

    /// 从某个位置向前扫描接收者表达式，并返回其类型（用于 `foo.` / `foo().`）。
    pub fn receiver_type_before(&self, file: &str, dot: BytePos) -> Option<Type> {
        let text = self.files.get(file)?;
        let start = text.receiver_start_before(dot)?;
        if let Some(ty) = self.type_at(file, start) {
            return Some(ty.clone());
        }
        // 退化：如果接收者是一个标识符，查 occurrence 的 result。
        if let Some(occ) = self.occurrence_at(file, start) {
            if let Some(ty) = &occ.result {
                return Some(ty.clone());
            }
        }
        None
    }

    pub fn class_symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbol_for_key(&SymbolKey::Class(name.to_string()))
    }

    pub fn enum_symbol(&self, name: &str) -> Option<&Symbol> {
        self.symbol_for_key(&SymbolKey::Enum(name.to_string()))
    }

    pub fn method_symbols(&self, owner: &str, name: &str) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| {
                matches!(
                    &s.key,
                    SymbolKey::Method { owner: o, name: n, .. } if o == owner && n == name
                )
            })
            .collect()
    }

    pub fn field_symbols(&self, owner: &str, name: &str) -> Vec<&Symbol> {
        self.symbols
            .iter()
            .filter(|s| {
                matches!(
                    &s.key,
                    SymbolKey::Field { owner: o, name: n } if o == owner && n == name
                )
            })
            .collect()
    }

    pub fn document_symbols(&self, file: &str) -> Vec<tower_lsp::lsp_types::DocumentSymbol> {
        let mut out = Vec::new();
        for id in &self.root_symbols {
            let Some(sym) = self.symbol(*id) else {
                continue;
            };
            if sym.location.file != file {
                continue;
            }
            out.push(self.to_document_symbol(sym, file));
        }
        out
    }

    fn to_document_symbol(&self, sym: &Symbol, file: &str) -> tower_lsp::lsp_types::DocumentSymbol {
        use tower_lsp::lsp_types::{DocumentSymbol, Position, Range};
        let text = self.files.get(file);
        let range = text
            .map(|t| t.range(sym.location.span))
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
        let selection_range = text
            .map(|t| t.range(sym.selection.span))
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)));
        let children: Vec<DocumentSymbol> = sym
            .children
            .iter()
            .filter_map(|id| self.symbol(*id))
            .filter(|s| s.location.file == file)
            .map(|s| self.to_document_symbol(s, file))
            .collect();
        DocumentSymbol {
            name: sym.name.clone(),
            detail: Some(sym.detail.clone()),
            kind: sym.kind.lsp(),
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
        }
    }
}

fn span_len(span: Span) -> usize {
    if span.start.line == span.end.line {
        span.end.col.saturating_sub(span.start.col) as usize
    } else {
        ((span.end.line - span.start.line) as usize) * 1024
            + span.end.col.saturating_sub(span.start.col) as usize
    }
}

fn absolute_pos(file: &str, pos: BytePos, files: &HashMap<String, Arc<TextFile>>) -> usize {
    files
        .get(file)
        .map(|f| f.offset(pos))
        .unwrap_or(pos.line as usize * 1024 + pos.col as usize)
}

/// 构建模型。
pub fn build_model(
    ast: &Program,
    registry: &TypeRegistry,
    builtins: &BuiltinIndex,
    files: HashMap<String, Arc<TextFile>>,
    main_file: String,
) -> Model {
    let mut builder = ModelBuilder {
        main_file,
        files,
        registry,
        builtins,
        symbols: Vec::new(),
        by_key: HashMap::new(),
        occurrences: Vec::new(),
        scopes: Vec::new(),
        scope_stack: Vec::new(),
        calls: Vec::new(),
        type_at_start: HashMap::new(),
        root_symbols: Vec::new(),
        class_ranges: Vec::new(),
        folding: Vec::new(),
        inlay_hints: Vec::new(),
        class_symbols: HashMap::new(),
        interface_symbols: HashMap::new(),
        struct_symbols: HashMap::new(),
        enum_symbols: HashMap::new(),
        constructor_symbols: HashMap::new(),
        variant_symbols: HashMap::new(),
        method_symbols: HashMap::new(),
        field_symbols: HashMap::new(),
        function_symbols: HashMap::new(),
        builtin_symbols: HashMap::new(),
        current_class_type_params: Vec::new(),
    };
    builder.collect_program(ast);
    builder.resolve_program(ast);
    builder.finish()
}

struct ModelBuilder<'a> {
    main_file: String,
    files: HashMap<String, Arc<TextFile>>,
    registry: &'a TypeRegistry,
    builtins: &'a BuiltinIndex,
    symbols: Vec<Symbol>,
    by_key: HashMap<SymbolKey, Vec<SymbolId>>,
    occurrences: Vec<Occurrence>,
    scopes: Vec<Scope>,
    scope_stack: Vec<usize>,
    calls: Vec<CallSite>,
    type_at_start: HashMap<(String, usize), Type>,
    root_symbols: Vec<SymbolId>,
    class_ranges: Vec<(String, Span, String)>,
    folding: Vec<Folding>,
    inlay_hints: Vec<InlayHintData>,
    class_symbols: HashMap<String, SymbolId>,
    interface_symbols: HashMap<String, SymbolId>,
    struct_symbols: HashMap<String, SymbolId>,
    enum_symbols: HashMap<String, SymbolId>,
    constructor_symbols: HashMap<(String, usize), SymbolId>,
    variant_symbols: HashMap<(String, String), SymbolId>,
    method_symbols: HashMap<(String, String, usize), SymbolId>,
    field_symbols: HashMap<(String, String), SymbolId>,
    function_symbols: HashMap<String, SymbolId>,
    builtin_symbols: HashMap<String, SymbolId>,
    current_class_type_params: Vec<String>,
}

struct Ctx {
    class: Option<String>,
}

impl Ctx {
    fn top() -> Self {
        Self { class: None }
    }

    fn for_class(class: &str) -> Self {
        Self {
            class: Some(class.to_string()),
        }
    }

    fn for_method(class: &str, _method: &MethodDecl) -> Self {
        Self {
            class: Some(class.to_string()),
        }
    }
}

impl<'a> ModelBuilder<'a> {
    fn finish(mut self) -> Model {
        self.folding.sort_by_key(|f| (f.file.clone(), f.span.start));
        self.folding
            .dedup_by(|a, b| a.file == b.file && a.span == b.span);
        Model {
            symbols: self.symbols,
            by_key: self.by_key,
            occurrences: self.occurrences,
            scopes: self.scopes,
            calls: self.calls,
            type_at_start: self.type_at_start,
            root_symbols: self.root_symbols,
            class_ranges: self.class_ranges,
            folding: self.folding,
            inlay_hints: self.inlay_hints,
            files: self.files,
            main_file: self.main_file,
        }
    }

    fn text(&self, file: &str) -> Option<&TextFile> {
        self.files.get(file).map(|f| f.as_ref())
    }

    fn loc_file(&self, loc: &SourceLocation) -> String {
        loc.file.clone().unwrap_or_else(|| self.main_file.clone())
    }

    fn add_symbol(&mut self, mut symbol: Symbol) -> SymbolId {
        let id = self.symbols.len();
        symbol.id = id;
        self.by_key.entry(symbol.key.clone()).or_default().push(id);
        self.symbols.push(symbol);
        id
    }

    fn add_occurrence(&mut self, occurrence: Occurrence) {
        self.occurrences.push(occurrence);
    }

    fn add_decl_occurrence(&mut self, file: &str, span: Span, name: &str, symbol: SymbolId) {
        self.add_occurrence(Occurrence {
            file: file.to_string(),
            span,
            name: name.to_string(),
            kind: OccurrenceKind::Declaration,
            symbol: Some(symbol),
            receiver: None,
            result: None,
            is_definition: true,
        });
    }

    fn span_from_loc(&self, file: &str, loc: &SourceLocation, len_bytes: usize) -> Span {
        let start = BytePos::from_source_location(loc);
        let text = self.text(file);
        let line_len = text
            .map(|t| t.line(start.line).len())
            .unwrap_or(start.col as usize);
        let end_col = (start.col as usize + len_bytes).min(line_len.max(start.col as usize));
        Span::new(start, BytePos::new(start.line, end_col as u32))
    }

    fn semi_decl_span(&self, file: &str, loc: &SourceLocation) -> Span {
        let start = BytePos::from_source_location(loc);
        let Some(text) = self.text(file) else {
            return Span::point(start);
        };
        if let Some(semi) = text.find_char_on_line(start, ';') {
            Span::new(start, BytePos::new(semi.line, semi.col + 1))
        } else {
            let len = text.line(start.line).len() as u32;
            Span::new(start, BytePos::new(start.line, len))
        }
    }

    fn full_decl_span(&self, file: &str, loc: &SourceLocation) -> Span {
        let start = BytePos::from_source_location(loc);
        let Some(text) = self.text(file) else {
            return Span::point(start);
        };
        let brace = text.find_char(start, '{');
        let semi = text.find_char(start, ';');
        let block_span = |open: BytePos| {
            text.matching_close(open)
                .map(|close| Span::new(start, BytePos::new(close.line, close.col + 1)))
                .unwrap_or_else(|| Span::new(start, BytePos::new(open.line, open.col + 1)))
        };
        match (brace, semi) {
            (Some(b), Some(s)) if b < s => block_span(b),
            (Some(b), None) => block_span(b),
            (_, Some(s)) => Span::new(start, BytePos::new(s.line, s.col + 1)),
            _ => {
                let len = text.line(start.line).len() as u32;
                Span::new(start, BytePos::new(start.line, len))
            }
        }
    }

    fn field_name_span(&self, file: &str, loc: &SourceLocation, name: &str) -> Span {
        let start = BytePos::from_source_location(loc);
        self.text(file)
            .and_then(|t| {
                t.find_last_word_before(start, name, &[';', '=', ','])
                    .or_else(|| t.find_word_after(start, name, &[';', '=', ',']))
            })
            .unwrap_or_else(|| self.span_from_loc(file, loc, name.len()))
    }

    fn method_name_span(&self, file: &str, loc: &SourceLocation, name: &str) -> Span {
        let start = BytePos::from_source_location(loc);
        let Some(text) = self.text(file) else {
            return self.span_from_loc(file, loc, name.len());
        };
        // 优先找 `(` 前紧邻的名字，兼容返回类型与名字同名、泛型方法等。
        if let Some(open) = text.find_char(start, '(') {
            let mut before = text.text[..text.offset(open)].trim_end().to_string();
            // 去掉方法级泛型参数 <T>
            if before.ends_with('>') {
                let mut depth = 0i32;
                let mut cut = before.len();
                for (idx, ch) in before.char_indices().rev() {
                    if ch == '>' {
                        depth += 1;
                    } else if ch == '<' {
                        depth -= 1;
                        if depth == 0 {
                            cut = idx;
                            break;
                        }
                    }
                }
                before.truncate(cut);
                before = before.trim_end().to_string();
            }
            let mut start_idx = before.len();
            let bytes = before.as_bytes();
            while start_idx > 0 {
                let ch = before[..start_idx].chars().next_back().unwrap_or(' ');
                if is_word_char(ch) {
                    start_idx -= ch.len_utf8();
                } else {
                    break;
                }
            }
            if start_idx < bytes.len() {
                let s = text.pos_at_offset(start_idx);
                let e = text.pos_at_offset(before.len());
                return Span::new(s, e);
            }
        }
        text.find_word_after(start, name, &['(', ';'])
            .unwrap_or_else(|| self.span_from_loc(file, loc, name.len()))
    }

    fn doc_for(&self, file: &str, loc: &SourceLocation) -> String {
        self.text(file)
            .map(|t| t.doc_comment_before(loc.line.saturating_sub(1) as u32))
            .unwrap_or_default()
    }

    fn class_display_name(class: &ClassDecl) -> String {
        let mut s = class.name.clone();
        if !class.type_params.is_empty() {
            s.push('<');
            s.push_str(
                &class
                    .type_params
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(", "),
            );
            s.push('>');
        }
        s
    }

    // ------------------------------------------------------------------
    // 第一遍：声明收集
    // ------------------------------------------------------------------

    fn collect_program(&mut self, program: &Program) {
        for class in &program.classes {
            self.collect_class(class);
        }
        for interface in &program.interfaces {
            self.collect_interface(interface);
        }
        for strukt in &program.structs {
            self.collect_struct(strukt);
        }
        for en in &program.enums {
            self.collect_enum(en);
        }
        for func in &program.top_level_functions {
            self.collect_function(func);
        }
    }

    fn collect_class(&mut self, class: &ClassDecl) {
        if self.class_symbols.contains_key(&class.name) {
            return;
        }
        let file = self.loc_file(&class.loc);
        let full = self.full_decl_span(&file, &class.loc);
        let name_span = self
            .text(&file)
            .and_then(|t| t.name_after_keyword(BytePos::from_source_location(&class.loc), "class"))
            .map(|(s, _)| s)
            .unwrap_or_else(|| self.span_from_loc(&file, &class.loc, class.name.len()));
        let mut detail = format!("class {}", Self::class_display_name(class));
        if let Some(parent) = &class.parent {
            detail.push_str(&format!(" extends {}", parent));
        }
        if !class.interfaces.is_empty() {
            detail.push_str(&format!(
                " implements {}",
                class
                    .interfaces
                    .iter()
                    .map(display_type)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        let doc = self.doc_for(&file, &class.loc);
        let symbol = Symbol {
            id: 0,
            key: SymbolKey::Class(class.name.clone()),
            name: class.name.clone(),
            kind: SymbolKind::Class,
            detail: detail.clone(),
            documentation: doc,
            ty: None,
            signature: detail,
            location: LocatedSpan::new(&file, full),
            selection: LocatedSpan::new(&file, name_span),
            owner: None,
            is_static: false,
            params: Vec::new(),
            return_type: None,
            children: Vec::new(),
            visibility: Visibility::from_modifiers(&class.modifiers),
        };
        let class_id = self.add_symbol(symbol);
        self.class_symbols
            .entry(class.name.clone())
            .or_insert(class_id);
        self.root_symbols.push(class_id);
        self.add_decl_occurrence(&file, name_span, &class.name, class_id);
        self.class_ranges
            .push((class.name.clone(), full, file.clone()));
        self.folding.push(Folding {
            file: file.clone(),
            span: full,
            kind: "class",
        });

        let old_type_params = std::mem::replace(
            &mut self.current_class_type_params,
            class.type_params.iter().map(|p| p.name.clone()).collect(),
        );

        for member in &class.members {
            match member {
                ClassMember::Field(field) => {
                    let id = self.collect_field(&file, &class.name, field);
                    self.symbols[class_id].children.push(id);
                }
                ClassMember::Method(method) => {
                    let id = self.collect_method(&file, &class.name, method);
                    self.symbols[class_id].children.push(id);
                }
                ClassMember::Constructor(ctor) => {
                    let id = self.collect_constructor(&file, &class.name, ctor);
                    self.symbols[class_id].children.push(id);
                }
                _ => {}
            }
        }

        self.current_class_type_params = old_type_params;
    }

    fn collect_field(&mut self, file: &str, owner: &str, field: &FieldDecl) -> SymbolId {
        let full = self.semi_decl_span(file, &field.loc);
        let name_span = self.field_name_span(file, &field.loc, &field.name);
        let ty = field.field_type.clone();
        let detail = format!("{} {}", display_type(&ty), field.name);
        let symbol = Symbol {
            id: 0,
            key: SymbolKey::Field {
                owner: owner.to_string(),
                name: field.name.clone(),
            },
            name: field.name.clone(),
            kind: SymbolKind::Field,
            detail: detail.clone(),
            documentation: self.doc_for(file, &field.loc),
            ty: Some(ty),
            signature: detail,
            location: LocatedSpan::new(file, full),
            selection: LocatedSpan::new(file, name_span),
            owner: Some(owner.to_string()),
            is_static: field.modifiers.contains(&Modifier::Static),
            params: Vec::new(),
            return_type: None,
            children: Vec::new(),
            visibility: Visibility::from_modifiers(&field.modifiers),
        };
        let id = self.add_symbol(symbol);
        self.field_symbols
            .entry((owner.to_string(), field.name.clone()))
            .or_insert(id);
        self.add_decl_occurrence(file, name_span, &field.name, id);
        self.folding.push(Folding {
            file: file.to_string(),
            span: full,
            kind: "field",
        });
        id
    }

    fn collect_method(&mut self, file: &str, owner: &str, method: &MethodDecl) -> SymbolId {
        let full = self.full_decl_span(file, &method.loc);
        let name_span = self.method_name_span(file, &method.loc, &method.name);
        let params: Vec<ParamSig> = method
            .params
            .iter()
            .map(|p| ParamSig {
                name: p.name.clone(),
                ty: p.param_type.clone(),
                is_varargs: p.is_varargs,
            })
            .collect();
        let label = signature_label(
            &method.return_type,
            &method.name,
            &params,
            method.modifiers.contains(&Modifier::Static),
        );
        let symbol = Symbol {
            id: 0,
            key: SymbolKey::Method {
                owner: owner.to_string(),
                name: method.name.clone(),
                arity: params.len(),
            },
            name: method.name.clone(),
            kind: SymbolKind::Method,
            detail: label.clone(),
            documentation: self.doc_for(file, &method.loc),
            ty: Some(Type::Function(Box::new(crate::types::FunctionType {
                params: method.params.iter().map(|p| p.param_type.clone()).collect(),
                return_type: Box::new(method.return_type.clone()),
                is_static: method.modifiers.contains(&Modifier::Static),
                is_closure: false,
            }))),
            signature: label,
            location: LocatedSpan::new(file, full),
            selection: LocatedSpan::new(file, name_span),
            owner: Some(owner.to_string()),
            is_static: method.modifiers.contains(&Modifier::Static),
            params,
            return_type: Some(method.return_type.clone()),
            children: Vec::new(),
            visibility: Visibility::from_modifiers(&method.modifiers),
        };
        let id = self.add_symbol(symbol);
        self.method_symbols
            .entry((owner.to_string(), method.name.clone(), method.params.len()))
            .or_insert(id);
        self.add_decl_occurrence(file, name_span, &method.name, id);
        self.folding.push(Folding {
            file: file.to_string(),
            span: full,
            kind: "method",
        });
        id
    }

    fn collect_constructor(&mut self, file: &str, owner: &str, ctor: &ConstructorDecl) -> SymbolId {
        let full = self.full_decl_span(file, &ctor.loc);
        let name_span = self.method_name_span(file, &ctor.loc, owner);
        let params: Vec<ParamSig> = ctor
            .params
            .iter()
            .map(|p| ParamSig {
                name: p.name.clone(),
                ty: p.param_type.clone(),
                is_varargs: p.is_varargs,
            })
            .collect();
        let label = format!(
            "{}({})",
            owner,
            params
                .iter()
                .map(|p| p.display())
                .collect::<Vec<_>>()
                .join(", ")
        );
        let symbol = Symbol {
            id: 0,
            key: SymbolKey::Constructor {
                owner: owner.to_string(),
                arity: params.len(),
            },
            name: owner.to_string(),
            kind: SymbolKind::Constructor,
            detail: label.clone(),
            documentation: self.doc_for(file, &ctor.loc),
            ty: None,
            signature: label,
            location: LocatedSpan::new(file, full),
            selection: LocatedSpan::new(file, name_span),
            owner: Some(owner.to_string()),
            is_static: false,
            params,
            return_type: Some(Type::Object(owner.to_string())),
            children: Vec::new(),
            visibility: Visibility::from_modifiers(&ctor.modifiers),
        };
        let id = self.add_symbol(symbol);
        self.constructor_symbols
            .entry((owner.to_string(), ctor.params.len()))
            .or_insert(id);
        self.add_decl_occurrence(file, name_span, owner, id);
        self.folding.push(Folding {
            file: file.to_string(),
            span: full,
            kind: "constructor",
        });
        id
    }

    fn collect_interface(&mut self, interface: &InterfaceDecl) {
        let file = self.loc_file(&interface.loc);
        let full = self.full_decl_span(&file, &interface.loc);
        let name_span = self
            .text(&file)
            .and_then(|t| {
                t.name_after_keyword(BytePos::from_source_location(&interface.loc), "interface")
            })
            .map(|(s, _)| s)
            .unwrap_or_else(|| self.span_from_loc(&file, &interface.loc, interface.name.len()));
        let detail = format!(
            "interface {}{}",
            interface.name,
            if interface.type_params.is_empty() {
                String::new()
            } else {
                format!(
                    "<{}>",
                    interface
                        .type_params
                        .iter()
                        .map(|p| p.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        );
        let symbol = Symbol {
            id: 0,
            key: SymbolKey::Interface(interface.name.clone()),
            name: interface.name.clone(),
            kind: SymbolKind::Interface,
            detail: detail.clone(),
            documentation: self.doc_for(&file, &interface.loc),
            ty: None,
            signature: detail,
            location: LocatedSpan::new(&file, full),
            selection: LocatedSpan::new(&file, name_span),
            owner: None,
            is_static: false,
            params: Vec::new(),
            return_type: None,
            children: Vec::new(),
            visibility: Visibility::from_modifiers(&interface.modifiers),
        };
        let id = self.add_symbol(symbol);
        self.interface_symbols
            .entry(interface.name.clone())
            .or_insert(id);
        self.root_symbols.push(id);
        self.add_decl_occurrence(&file, name_span, &interface.name, id);
        self.folding.push(Folding {
            file: file.clone(),
            span: full,
            kind: "interface",
        });

        for method in &interface.methods {
            let mfull = self.full_decl_span(&file, &method.loc);
            let mspan = self.method_name_span(&file, &method.loc, &method.name);
            let params: Vec<ParamSig> = method
                .params
                .iter()
                .map(|p| ParamSig {
                    name: p.name.clone(),
                    ty: p.param_type.clone(),
                    is_varargs: p.is_varargs,
                })
                .collect();
            let label = signature_label(
                &method.return_type,
                &method.name,
                &params,
                method.modifiers.contains(&Modifier::Static),
            );
            let symbol = Symbol {
                id: 0,
                key: SymbolKey::Method {
                    owner: interface.name.clone(),
                    name: method.name.clone(),
                    arity: params.len(),
                },
                name: method.name.clone(),
                kind: SymbolKind::Method,
                detail: label.clone(),
                documentation: self.doc_for(&file, &method.loc),
                ty: None,
                signature: label,
                location: LocatedSpan::new(&file, mfull),
                selection: LocatedSpan::new(&file, mspan),
                owner: Some(interface.name.clone()),
                is_static: method.modifiers.contains(&Modifier::Static),
                params,
                return_type: Some(method.return_type.clone()),
                children: Vec::new(),
                visibility: Visibility::Public,
            };
            let mid = self.add_symbol(symbol);
            self.symbols[id].children.push(mid);
            self.add_decl_occurrence(&file, mspan, &method.name, mid);
        }
    }

    fn collect_struct(&mut self, strukt: &StructDecl) {
        let file = self.loc_file(&strukt.loc);
        let full = self.full_decl_span(&file, &strukt.loc);
        let name_span = self
            .text(&file)
            .and_then(|t| {
                t.name_after_keyword(BytePos::from_source_location(&strukt.loc), "struct")
            })
            .map(|(s, _)| s)
            .unwrap_or_else(|| self.span_from_loc(&file, &strukt.loc, strukt.name.len()));
        let detail = format!("struct {}", strukt.name);
        let symbol = Symbol {
            id: 0,
            key: SymbolKey::Struct(strukt.name.clone()),
            name: strukt.name.clone(),
            kind: SymbolKind::Struct,
            detail: detail.clone(),
            documentation: self.doc_for(&file, &strukt.loc),
            ty: None,
            signature: detail,
            location: LocatedSpan::new(&file, full),
            selection: LocatedSpan::new(&file, name_span),
            owner: None,
            is_static: false,
            params: Vec::new(),
            return_type: None,
            children: Vec::new(),
            visibility: Visibility::from_modifiers(&strukt.modifiers),
        };
        let id = self.add_symbol(symbol);
        self.struct_symbols.entry(strukt.name.clone()).or_insert(id);
        self.root_symbols.push(id);
        self.add_decl_occurrence(&file, name_span, &strukt.name, id);
        self.folding.push(Folding {
            file: file.clone(),
            span: full,
            kind: "struct",
        });

        for field in &strukt.fields {
            let ffull = self.semi_decl_span(&file, &field.loc);
            let fspan = self.field_name_span(&file, &field.loc, &field.name);
            let ty = field.field_type.clone();
            let fsym = Symbol {
                id: 0,
                key: SymbolKey::Field {
                    owner: strukt.name.clone(),
                    name: field.name.clone(),
                },
                name: field.name.clone(),
                kind: SymbolKind::Field,
                detail: format!("{} {}", display_type(&ty), field.name),
                documentation: self.doc_for(&file, &field.loc),
                ty: Some(ty.clone()),
                signature: format!("{} {}", display_type(&ty), field.name),
                location: LocatedSpan::new(&file, ffull),
                selection: LocatedSpan::new(&file, fspan),
                owner: Some(strukt.name.clone()),
                is_static: false,
                params: Vec::new(),
                return_type: None,
                children: Vec::new(),
                visibility: Visibility::from_modifiers(&field.modifiers),
            };
            let fid = self.add_symbol(fsym);
            self.field_symbols
                .entry((strukt.name.clone(), field.name.clone()))
                .or_insert(fid);
            self.symbols[id].children.push(fid);
            self.add_decl_occurrence(&file, fspan, &field.name, fid);
        }
        for method in &strukt.methods {
            let mid = self.collect_method(&file, &strukt.name, method);
            self.symbols[id].children.push(mid);
        }
        for ctor in &strukt.constructors {
            let cid = self.collect_constructor(&file, &strukt.name, ctor);
            self.symbols[id].children.push(cid);
        }
    }

    fn collect_enum(&mut self, en: &EnumDecl) {
        let file = self.loc_file(&en.loc);
        let full = self.full_decl_span(&file, &en.loc);
        let name_span = self
            .text(&file)
            .and_then(|t| t.name_after_keyword(BytePos::from_source_location(&en.loc), "enum"))
            .map(|(s, _)| s)
            .unwrap_or_else(|| self.span_from_loc(&file, &en.loc, en.name.len()));
        let detail = format!("enum {}", en.name);
        let symbol = Symbol {
            id: 0,
            key: SymbolKey::Enum(en.name.clone()),
            name: en.name.clone(),
            kind: SymbolKind::Enum,
            detail: detail.clone(),
            documentation: self.doc_for(&file, &en.loc),
            ty: None,
            signature: detail,
            location: LocatedSpan::new(&file, full),
            selection: LocatedSpan::new(&file, name_span),
            owner: None,
            is_static: false,
            params: Vec::new(),
            return_type: None,
            children: Vec::new(),
            visibility: Visibility::from_modifiers(&en.modifiers),
        };
        let id = self.add_symbol(symbol);
        self.enum_symbols.entry(en.name.clone()).or_insert(id);
        self.root_symbols.push(id);
        self.add_decl_occurrence(&file, name_span, &en.name, id);
        self.folding.push(Folding {
            file: file.clone(),
            span: full,
            kind: "enum",
        });

        for variant in &en.variants {
            let vspan = self
                .text(&file)
                .and_then(|t| {
                    t.find_word_after(
                        BytePos::from_source_location(&variant.loc),
                        &variant.name,
                        &[',', '(', '}', '{'],
                    )
                })
                .unwrap_or_else(|| self.span_from_loc(&file, &variant.loc, variant.name.len()));
            let vsym = Symbol {
                id: 0,
                key: SymbolKey::EnumVariant(en.name.clone(), variant.name.clone()),
                name: variant.name.clone(),
                kind: SymbolKind::EnumVariant,
                detail: format!("{}.{}", en.name, variant.name),
                documentation: self.doc_for(&file, &variant.loc),
                ty: Some(Type::Object(en.name.clone())),
                signature: format!("{}.{}", en.name, variant.name),
                location: LocatedSpan::new(&file, vspan),
                selection: LocatedSpan::new(&file, vspan),
                owner: Some(en.name.clone()),
                is_static: true,
                params: Vec::new(),
                return_type: None,
                children: Vec::new(),
                visibility: Visibility::Public,
            };
            let vid = self.add_symbol(vsym);
            self.variant_symbols
                .entry((en.name.clone(), variant.name.clone()))
                .or_insert(vid);
            self.symbols[id].children.push(vid);
            self.add_decl_occurrence(&file, vspan, &variant.name, vid);
        }
        for method in &en.methods {
            let mid = self.collect_method(&file, &en.name, method);
            self.symbols[id].children.push(mid);
        }
    }

    fn collect_function(&mut self, func: &TopLevelFunction) {
        let file = self.loc_file(&func.loc);
        let full = self.full_decl_span(&file, &func.loc);
        let name_span = self.method_name_span(&file, &func.loc, &func.name);
        let params: Vec<ParamSig> = func
            .params
            .iter()
            .map(|p| ParamSig {
                name: p.name.clone(),
                ty: p.param_type.clone(),
                is_varargs: p.is_varargs,
            })
            .collect();
        let label = signature_label(
            &func.return_type,
            &func.name,
            &params,
            func.modifiers.contains(&Modifier::Static),
        );
        let symbol = Symbol {
            id: 0,
            key: SymbolKey::Function(func.name.clone()),
            name: func.name.clone(),
            kind: SymbolKind::Function,
            detail: label.clone(),
            documentation: self.doc_for(&file, &func.loc),
            ty: Some(Type::Function(Box::new(crate::types::FunctionType {
                params: func.params.iter().map(|p| p.param_type.clone()).collect(),
                return_type: Box::new(func.return_type.clone()),
                is_static: true,
                is_closure: false,
            }))),
            signature: label,
            location: LocatedSpan::new(&file, full),
            selection: LocatedSpan::new(&file, name_span),
            owner: None,
            is_static: true,
            params,
            return_type: Some(func.return_type.clone()),
            children: Vec::new(),
            visibility: Visibility::from_modifiers(&func.modifiers),
        };
        let id = self.add_symbol(symbol);
        self.function_symbols.entry(func.name.clone()).or_insert(id);
        self.root_symbols.push(id);
        self.add_decl_occurrence(&file, name_span, &func.name, id);
        self.folding.push(Folding {
            file,
            span: full,
            kind: "function",
        });
    }

    // ------------------------------------------------------------------
    // 第二遍：引用、作用域、类型
    // ------------------------------------------------------------------

    fn resolve_program(&mut self, program: &Program) {
        for class in &program.classes {
            self.resolve_class(class);
        }
        for interface in &program.interfaces {
            self.resolve_interface(interface);
        }
        for strukt in &program.structs {
            self.resolve_struct(strukt);
        }
        for en in &program.enums {
            self.resolve_enum(en);
        }
        for func in &program.top_level_functions {
            self.resolve_function(func);
        }
    }

    fn resolve_class(&mut self, class: &ClassDecl) {
        let file = self.loc_file(&class.loc);
        let ctx = Ctx::for_class(&class.name);
        let old_type_params = std::mem::replace(
            &mut self.current_class_type_params,
            class.type_params.iter().map(|p| p.name.clone()).collect(),
        );

        if let Some(parent) = &class.parent {
            let span = self
                .text(&file)
                .and_then(|t| {
                    t.name_after_keyword(BytePos::from_source_location(&class.loc), "extends")
                })
                .map(|(s, _)| s)
                .unwrap_or_else(|| self.span_from_loc(&file, &class.loc, parent.len()));
            self.add_type_occurrence(&file, span, parent);
        }
        for interface in &class.interfaces {
            let base = base_type_name(interface);
            let span = self
                .text(&file)
                .and_then(|t| {
                    t.name_after_keyword(BytePos::from_source_location(&class.loc), "implements")
                })
                .and_then(|(s, _)| {
                    self.text(&file)
                        .and_then(|t| t.find_word_after(s.start, &base, &['{', ',']))
                })
                .unwrap_or_else(|| {
                    self.text(&file)
                        .and_then(|t| {
                            t.find_word_after(
                                BytePos::from_source_location(&class.loc),
                                &base,
                                &['{', ','],
                            )
                        })
                        .unwrap_or_else(|| self.span_from_loc(&file, &class.loc, base.len()))
                });
            self.add_type_occurrence(&file, span, &base);
        }

        for member in &class.members {
            match member {
                ClassMember::Field(field) => {
                    self.visit_type_ref(&file, &field.field_type, &field.loc);
                    if let Some(init) = &field.initializer {
                        self.visit_expr(&file, init, &ctx);
                    }
                }
                ClassMember::Method(method) => {
                    self.resolve_method(&file, &class.name, method, &ctx)
                }
                ClassMember::Constructor(ctor) => {
                    for p in &ctor.params {
                        self.add_param_type_occurrence(&file, &ctor.loc, p);
                    }
                    let scope = self.push_scope(
                        &file,
                        Span::point(BytePos::from_source_location(&ctor.loc)),
                        ctx.class.clone(),
                    );
                    for p in &ctor.params {
                        self.declare_param(
                            &file,
                            &ctor.loc,
                            p,
                            scope,
                            Span::point(BytePos::from_source_location(&ctor.loc)),
                        );
                    }
                    self.visit_block_in_scope(&file, &ctor.body, &ctx, scope);
                    self.pop_scope(scope, self.block_span(&file, Some(&ctor.body)));
                }
                ClassMember::InstanceInitializer(block) | ClassMember::StaticInitializer(block) => {
                    self.visit_block(&file, block, &ctx);
                }
                _ => {}
            }
        }

        self.current_class_type_params = old_type_params;
    }

    fn resolve_interface(&mut self, interface: &InterfaceDecl) {
        let file = self.loc_file(&interface.loc);
        for method in &interface.methods {
            self.visit_type_ref(&file, &method.return_type, &method.loc);
            for p in &method.params {
                self.add_param_type_occurrence(&file, &method.loc, p);
            }
        }
    }

    fn resolve_struct(&mut self, strukt: &StructDecl) {
        let file = self.loc_file(&strukt.loc);
        let ctx = Ctx::for_class(&strukt.name);
        for field in &strukt.fields {
            self.visit_type_ref(&file, &field.field_type, &field.loc);
            if let Some(init) = &field.initializer {
                self.visit_expr(&file, init, &ctx);
            }
        }
        for method in &strukt.methods {
            self.resolve_method(&file, &strukt.name, method, &ctx);
        }
        for ctor in &strukt.constructors {
            let scope = self.push_scope(
                &file,
                Span::point(BytePos::from_source_location(&ctor.loc)),
                ctx.class.clone(),
            );
            for p in &ctor.params {
                self.add_param_type_occurrence(&file, &ctor.loc, p);
                self.declare_param(
                    &file,
                    &ctor.loc,
                    p,
                    scope,
                    Span::point(BytePos::from_source_location(&ctor.loc)),
                );
            }
            self.visit_block_in_scope(&file, &ctor.body, &ctx, scope);
            self.pop_scope(scope, self.block_span(&file, Some(&ctor.body)));
        }
    }

    fn resolve_enum(&mut self, en: &EnumDecl) {
        let file = self.loc_file(&en.loc);
        let ctx = Ctx::for_class(&en.name);
        for method in &en.methods {
            self.resolve_method(&file, &en.name, method, &ctx);
        }
    }

    fn resolve_function(&mut self, func: &TopLevelFunction) {
        let file = self.loc_file(&func.loc);
        self.visit_type_ref(&file, &func.return_type, &func.loc);
        let ctx = Ctx { class: None };
        let scope = self.push_scope(
            &file,
            Span::point(BytePos::from_source_location(&func.loc)),
            None,
        );
        for p in &func.params {
            self.add_param_type_occurrence(&file, &func.loc, p);
            self.declare_param(
                &file,
                &func.loc,
                p,
                scope,
                Span::point(BytePos::from_source_location(&func.loc)),
            );
        }
        self.visit_block_in_scope(&file, &func.body, &ctx, scope);
        self.pop_scope(scope, self.block_span(&file, Some(&func.body)));
    }

    fn resolve_method(&mut self, file: &str, class: &str, method: &MethodDecl, parent_ctx: &Ctx) {
        self.visit_type_ref(file, &method.return_type, &method.loc);
        for p in &method.params {
            self.add_param_type_occurrence(file, &method.loc, p);
        }
        let ctx = Ctx::for_method(class, method);
        let scope = self.push_scope(
            file,
            Span::point(BytePos::from_source_location(&method.loc)),
            parent_ctx.class.clone(),
        );
        for p in &method.params {
            self.declare_param(
                file,
                &method.loc,
                p,
                scope,
                Span::point(BytePos::from_source_location(&method.loc)),
            );
        }
        if let Some(body) = &method.body {
            self.visit_block_in_scope(file, body, &ctx, scope);
        }
        self.pop_scope(scope, self.block_span(file, method.body.as_ref()));
    }

    fn add_param_type_occurrence(&mut self, file: &str, loc: &SourceLocation, p: &ParameterInfo) {
        let base = base_type_name(&p.param_type);
        if base.is_empty() {
            return;
        }
        let search_start = self.param_search_start(file, loc);
        let span = self
            .text(file)
            .and_then(|t| t.find_word_after(search_start, &base, &[',', ')']))
            .or_else(|| {
                self.text(file)
                    .and_then(|t| t.find_word_after(search_start, &base, &[';']))
            });
        if let Some(span) = span {
            self.add_type_occurrence(file, span, &base);
        }
    }

    fn push_scope(&mut self, file: &str, span: Span, class: Option<String>) -> usize {
        let parent = self.scope_stack.last().copied();
        let idx = self.scopes.len();
        self.scopes.push(Scope {
            file: file.to_string(),
            span,
            parent,
            names: HashMap::new(),
            class,
        });
        self.scope_stack.push(idx);
        idx
    }

    fn pop_scope(&mut self, idx: usize, end: Span) {
        if let Some(scope) = self.scopes.get_mut(idx) {
            if scope.span.end == scope.span.start {
                scope.span.end = end.end;
            } else if end.end > scope.span.end {
                scope.span.end = end.end;
            }
        }
        if self.scope_stack.last().copied() == Some(idx) {
            self.scope_stack.pop();
        } else if let Some(pos) = self.scope_stack.iter().rposition(|s| *s == idx) {
            self.scope_stack.remove(pos);
        }
    }

    fn declare_local(
        &mut self,
        file: &str,
        name: &str,
        ty: Option<Type>,
        kind: SymbolKind,
        full: Span,
        selection: Span,
        scope: usize,
        ctx: &Ctx,
        documentation: String,
    ) -> SymbolId {
        let key = match kind {
            SymbolKind::Parameter => SymbolKey::Parameter {
                file: file.to_string(),
                line: selection.start.line,
                col: selection.start.col,
            },
            _ => SymbolKey::Local {
                file: file.to_string(),
                line: selection.start.line,
                col: selection.start.col,
            },
        };
        let detail = match &ty {
            Some(t) => format!("{} {}", display_type(t), name),
            None => name.to_string(),
        };
        let symbol = Symbol {
            id: 0,
            key,
            name: name.to_string(),
            kind,
            detail: detail.clone(),
            documentation,
            ty,
            signature: detail,
            location: LocatedSpan::new(file, full),
            selection: LocatedSpan::new(file, selection),
            owner: ctx.class.clone(),
            is_static: false,
            params: Vec::new(),
            return_type: None,
            children: Vec::new(),
            visibility: Visibility::Public,
        };
        let id = self.add_symbol(symbol);
        if let Some(scope) = self.scopes.get_mut(scope) {
            scope.names.insert(name.to_string(), id);
        }
        self.add_decl_occurrence(file, selection, name, id);
        id
    }

    fn param_search_start(&self, file: &str, loc: &SourceLocation) -> BytePos {
        let start = BytePos::from_source_location(loc);
        self.text(file)
            .and_then(|t| t.find_char(start, '('))
            .unwrap_or(start)
    }

    fn declare_param(
        &mut self,
        file: &str,
        loc: &SourceLocation,
        p: &ParameterInfo,
        scope: usize,
        fallback_span: Span,
    ) -> SymbolId {
        let search_start = self.param_search_start(file, loc);
        let span = self
            .text(file)
            .and_then(|t| t.find_word_after(search_start, &p.name, &[',', ')']))
            .unwrap_or(fallback_span);
        self.declare_local(
            file,
            &p.name,
            Some(p.param_type.clone()),
            SymbolKind::Parameter,
            Span::new(span.start, span.end),
            span,
            scope,
            &Ctx::top(),
            String::new(),
        )
    }

    fn add_type_occurrence(&mut self, file: &str, span: Span, name: &str) {
        let base = name.split('<').next().unwrap_or(name).trim();
        if base.is_empty() || base == "void" || base == "auto" {
            return;
        }
        let symbol = self.resolve_type_symbol(base);
        self.add_occurrence(Occurrence {
            file: file.to_string(),
            span: span_for_name(span, base),
            name: base.to_string(),
            kind: OccurrenceKind::Type,
            symbol,
            receiver: None,
            result: None,
            is_definition: false,
        });
    }

    fn resolve_type_symbol(&mut self, name: &str) -> Option<SymbolId> {
        let base = name.split('<').next().unwrap_or(name).trim();
        if let Some(id) = self.class_symbols.get(base) {
            return Some(*id);
        }
        if let Some(id) = self.interface_symbols.get(base) {
            return Some(*id);
        }
        if let Some(id) = self.struct_symbols.get(base) {
            return Some(*id);
        }
        if let Some(id) = self.enum_symbols.get(base) {
            return Some(*id);
        }
        if let Some(id) = self.ensure_builtin_type(base) {
            return Some(id);
        }
        // 限定名 / using 别名
        if let Some(qualified) = self.registry.find_qualified_class(base) {
            if let Some(id) = self.class_symbols.get(&qualified) {
                return Some(*id);
            }
        }
        None
    }

    fn ensure_builtin_type(&mut self, name: &str) -> Option<SymbolId> {
        let canonical = self.builtins.canonical_name(name)?;
        if let Some(id) = self.builtin_symbols.get(&format!("type:{}", canonical)) {
            return Some(*id);
        }
        let bt = self.builtins.get_by_name(&canonical)?.clone();
        let symbol = Symbol {
            id: 0,
            key: SymbolKey::BuiltinType(canonical.clone()),
            name: canonical.clone(),
            kind: SymbolKind::BuiltinType,
            detail: format!("class {}（内建 {}）", bt.stub_name, canonical),
            documentation: bt.documentation.clone(),
            ty: None,
            signature: format!("class {}", bt.stub_name),
            location: bt.location.clone(),
            selection: bt.location.clone(),
            owner: None,
            is_static: false,
            params: Vec::new(),
            return_type: None,
            children: Vec::new(),
            visibility: Visibility::Public,
        };
        let id = self.add_symbol(symbol);
        self.builtin_symbols
            .insert(format!("type:{}", canonical), id);
        Some(id)
    }

    fn ensure_builtin_member(&mut self, member: &BuiltinMember) -> SymbolId {
        let key_str = match member.kind {
            BuiltinMemberKind::Field => format!("field:{}:{}", member.owner, member.name),
            BuiltinMemberKind::Method => {
                format!(
                    "method:{}:{}:{}",
                    member.owner,
                    member.name,
                    member.params.len()
                )
            }
        };
        if let Some(id) = self.builtin_symbols.get(&key_str) {
            return *id;
        }
        let key = match member.kind {
            BuiltinMemberKind::Field => SymbolKey::BuiltinField {
                owner: member.owner.clone(),
                name: member.name.clone(),
            },
            BuiltinMemberKind::Method => SymbolKey::BuiltinMethod {
                owner: member.owner.clone(),
                name: member.name.clone(),
                arity: member.params.len(),
            },
        };
        let symbol = Symbol {
            id: 0,
            key,
            name: member.name.clone(),
            kind: SymbolKind::BuiltinMember,
            detail: member.label.clone(),
            documentation: member.documentation.clone(),
            ty: Some(member.ty.clone()),
            signature: member.label.clone(),
            location: member.location.clone(),
            selection: member.location.clone(),
            owner: Some(member.owner.clone()),
            is_static: member.is_static,
            params: member.params.clone(),
            return_type: Some(member.ty.clone()),
            children: Vec::new(),
            visibility: Visibility::Public,
        };
        let id = self.add_symbol(symbol);
        self.builtin_symbols.insert(key_str, id);
        id
    }

    fn ensure_builtin_function(&mut self, method: &super::builtins::BuiltinMethod) -> SymbolId {
        let key_str = format!("fn:{}:{}", method.name, method.params.len());
        if let Some(id) = self.builtin_symbols.get(&key_str) {
            return *id;
        }
        let params = method.params.clone();
        let symbol = Symbol {
            id: 0,
            key: SymbolKey::BuiltinFunction {
                name: method.name.clone(),
                arity: params.len(),
            },
            name: method.name.clone(),
            kind: SymbolKind::Function,
            detail: method.label.clone(),
            documentation: method.documentation.clone(),
            ty: Some(Type::Function(Box::new(crate::types::FunctionType {
                params: params.iter().map(|p| p.ty.clone()).collect(),
                return_type: Box::new(method.return_type.clone()),
                is_static: true,
                is_closure: false,
            }))),
            signature: method.label.clone(),
            location: method.location.clone(),
            selection: method.location.clone(),
            owner: Some("Builtins".to_string()),
            is_static: true,
            params,
            return_type: Some(method.return_type.clone()),
            children: Vec::new(),
            visibility: Visibility::Public,
        };
        let id = self.add_symbol(symbol);
        self.builtin_symbols.insert(key_str, id);
        id
    }

    // The rest of the visitor (expressions, blocks, member lookup) is in model_visit.rs.
}

fn span_for_name(span: Span, name: &str) -> Span {
    Span::new(
        span.start,
        BytePos::new(span.start.line, span.start.col + name.len() as u32),
    )
}

pub fn signature_label(
    return_type: &Type,
    name: &str,
    params: &[ParamSig],
    is_static: bool,
) -> String {
    format!(
        "{}{} {}({})",
        if is_static { "static " } else { "" },
        display_type(return_type),
        name,
        params
            .iter()
            .map(|p| p.display())
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub fn primitive_type_name(ty: &Type) -> Option<&'static str> {
    Some(match ty {
        Type::Int32 => "int",
        Type::Int64 => "long",
        Type::Float32 => "float",
        Type::Float64 => "double",
        Type::Bool => "bool",
        Type::Char => "char",
        Type::String => "String",
        _ => return None,
    })
}

pub fn base_type_name(ty: &Type) -> String {
    match ty {
        Type::Object(name) => name.split('<').next().unwrap_or(name).to_string(),
        Type::Generic(name, _) => name.clone(),
        Type::Struct(name) => name.clone(),
        Type::Array(inner) => base_type_name(inner),
        Type::Pointer(inner) => base_type_name(inner),
        _ => String::new(),
    }
}

fn parse_generic_args(s: &str) -> Vec<Type> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for ch in s.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                out.push(parse_type_name(current.trim()));
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        out.push(parse_type_name(current.trim()));
    }
    out
}

pub fn parse_type_name(name: &str) -> Type {
    let name = name.trim();
    match name {
        "int" | "i32" => return Type::Int32,
        "long" | "i64" => return Type::Int64,
        "float" | "f32" => return Type::Float32,
        "double" | "f64" => return Type::Float64,
        "bool" | "boolean" => return Type::Bool,
        "char" => return Type::Char,
        "String" | "string" => return Type::String,
        "void" => return Type::Void,
        "auto" => return Type::Auto,
        // 解析器在 `new Foo<T>()` 的 class_name 中使用 Debug 拼写（如 Int32）。
        "Int32" => return Type::Int32,
        "Int64" => return Type::Int64,
        "Float32" => return Type::Float32,
        "Float64" => return Type::Float64,
        "Bool" => return Type::Bool,
        "Char" => return Type::Char,
        _ => {}
    }
    if let Some(pos) = name.find('<') {
        if name.ends_with('>') {
            let base = name[..pos].trim().to_string();
            let args = parse_generic_args(&name[pos + 1..name.len() - 1]);
            return Type::Generic(base, args);
        }
    }
    if name.ends_with("[]") {
        return Type::Array(Box::new(parse_type_name(&name[..name.len() - 2])));
    }
    Type::Object(name.to_string())
}

fn substitute_receiver(ty: &Type, field: &FieldInfo, class: &ClassInfo) -> Type {
    let args = type_args_of(ty);
    if args.is_empty() || class.type_params.is_empty() {
        return field.field_type.clone();
    }
    let mapping: HashMap<String, Type> = class
        .type_params
        .iter()
        .map(|p| p.name.clone())
        .zip(args)
        .collect();
    substitute_type_params(&field.field_type, &mapping)
}

fn substitute_receiver_method(ty: &Type, method: &MethodInfo, class: &ClassInfo) -> MethodInfo {
    let args = type_args_of(ty);
    if args.is_empty() || class.type_params.is_empty() {
        return method.clone();
    }
    let mapping: HashMap<String, Type> = class
        .type_params
        .iter()
        .map(|p| p.name.clone())
        .zip(args)
        .collect();
    let mut cloned = method.clone();
    cloned.return_type = substitute_type_params(&method.return_type, &mapping);
    for p in &mut cloned.params {
        p.param_type = substitute_type_params(&p.param_type, &mapping);
    }
    cloned
}

fn type_args_of(ty: &Type) -> Vec<Type> {
    match ty {
        Type::Generic(_, args) => args.clone(),
        Type::Array(inner) => vec![(**inner).clone()],
        Type::Object(name) => {
            if let Some(pos) = name.find('<') {
                if name.ends_with('>') {
                    return parse_generic_args(&name[pos + 1..name.len() - 1]);
                }
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

fn method_to_signature(
    method: &MethodInfo,
    symbol: Option<SymbolId>,
    location: Option<LocatedSpan>,
    documentation: String,
) -> Signature {
    let params: Vec<ParamSig> = method
        .params
        .iter()
        .map(|p| ParamSig {
            name: p.name.clone(),
            ty: p.param_type.clone(),
            is_varargs: p.is_varargs,
        })
        .collect();
    Signature {
        label: signature_label(&method.return_type, &method.name, &params, method.is_static),
        params,
        return_type: method.return_type.clone(),
        documentation,
        location,
        symbol,
        is_static: method.is_static,
    }
}

fn method_accepts(method: &MethodInfo, arity: usize) -> bool {
    if method.params.last().map(|p| p.is_varargs).unwrap_or(false) {
        arity + 1 >= method.params.len()
    } else {
        arity == method.params.len()
    }
}

impl<'a> ModelBuilder<'a> {
    fn block_span(&self, file: &str, block: Option<&Block>) -> Span {
        match block {
            Some(b) => self.expr_block_span(file, b),
            None => Span::point(BytePos::new(0, 0)),
        }
    }

    fn expr_block_span(&self, file: &str, block: &Block) -> Span {
        let start = BytePos::from_source_location(&block.loc);
        let Some(text) = self.text(file) else {
            return Span::point(start);
        };
        let open = text.find_char(start, '{').unwrap_or(start);
        if let Some(close) = text.matching_close(open) {
            Span::new(start, BytePos::new(close.line, close.col + 1))
        } else {
            Span::new(start, start)
        }
    }

    fn visit_block(&mut self, file: &str, block: &Block, ctx: &Ctx) {
        let span = self.expr_block_span(file, block);
        let scope = self.push_scope(file, span, ctx.class.clone());
        self.visit_block_in_scope(file, block, ctx, scope);
        self.pop_scope(scope, span);
    }

    fn visit_block_in_scope(&mut self, file: &str, block: &Block, ctx: &Ctx, scope: usize) {
        for stmt in &block.statements {
            self.visit_stmt(file, stmt, ctx, scope);
        }
        if let Some(tail) = &block.tail_expr {
            self.visit_expr(file, tail, ctx);
        }
    }

    fn visit_stmt(&mut self, file: &str, stmt: &Stmt, ctx: &Ctx, scope: usize) {
        match stmt {
            Stmt::Expr(expr) => {
                self.visit_expr(file, expr, ctx);
            }
            Stmt::VarDecl(decl) => {
                let full = self.semi_decl_span(file, &decl.loc);
                let name_span = self.field_name_span(file, &decl.loc, &decl.name);
                let mut ty = decl.var_type.clone();
                let init_ty = decl
                    .initializer
                    .as_ref()
                    .and_then(|e| self.visit_expr(file, e, ctx));
                if matches!(ty, Type::Auto) || ty == Type::Auto {
                    if let Some(t) = init_ty.clone() {
                        ty = t;
                    }
                }
                self.visit_type_ref(file, &decl.var_type, &decl.loc);
                let id = self.declare_local(
                    file,
                    &decl.name,
                    if matches!(ty, Type::Auto) {
                        None
                    } else {
                        Some(ty.clone())
                    },
                    SymbolKind::Local,
                    full,
                    name_span,
                    scope,
                    ctx,
                    self.doc_for(file, &decl.loc),
                );
                if matches!(decl.var_type, Type::Auto) {
                    if let Some(t) = init_ty {
                        if let Some(sym) = self.symbols.get_mut(id) {
                            sym.ty = Some(t.clone());
                            sym.detail = format!("{} {}", display_type(&t), decl.name);
                            sym.signature = sym.detail.clone();
                        }
                        self.inlay_hints.push(InlayHintData {
                            file: file.to_string(),
                            pos: BytePos::new(name_span.end.line, name_span.end.col),
                            label: format!(": {}", display_type(&t)),
                        });
                    }
                }
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    self.visit_expr(file, expr, ctx);
                }
            }
            Stmt::If(if_stmt) => {
                self.visit_expr(file, &if_stmt.condition, ctx);
                self.visit_stmt(file, &if_stmt.then_branch, ctx, scope);
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.visit_stmt(file, else_branch, ctx, scope);
                }
            }
            Stmt::While(while_stmt) => {
                self.visit_expr(file, &while_stmt.condition, ctx);
                self.visit_stmt(file, &while_stmt.body, ctx, scope);
            }
            Stmt::DoWhile(do_stmt) => {
                self.visit_expr(file, &do_stmt.condition, ctx);
                self.visit_stmt(file, &do_stmt.body, ctx, scope);
            }
            Stmt::For(for_stmt) => {
                let span = self
                    .text(file)
                    .map(|t| {
                        let start = BytePos::from_source_location(&for_stmt.loc);
                        let open = t.find_char(start, '{').unwrap_or(start);
                        t.matching_close(open)
                            .map(|close| Span::new(start, BytePos::new(close.line, close.col + 1)))
                            .unwrap_or(Span::point(start))
                    })
                    .unwrap_or_else(|| Span::point(BytePos::from_source_location(&for_stmt.loc)));
                let for_scope = self.push_scope(file, span, ctx.class.clone());
                if let Some(init) = &for_stmt.init {
                    self.visit_stmt(file, init, ctx, for_scope);
                }
                if let Some(cond) = &for_stmt.condition {
                    self.visit_expr(file, cond, ctx);
                }
                if let Some(update) = &for_stmt.update {
                    self.visit_expr(file, update, ctx);
                }
                self.visit_stmt(file, &for_stmt.body, ctx, for_scope);
                self.pop_scope(for_scope, span);
            }
            Stmt::ForEach(foreach) => {
                let iterable_ty = self.visit_expr(file, &foreach.iterable, ctx);
                let elem_ty = match &iterable_ty {
                    Some(Type::Array(inner)) => Some((**inner).clone()),
                    Some(Type::String) => Some(Type::Char),
                    Some(Type::Generic(name, args)) if name == "array" => args.first().cloned(),
                    _ => None,
                };
                let span = self.expr_block_span(
                    file,
                    &Block {
                        statements: Vec::new(),
                        tail_expr: None,
                        loc: foreach.loc.clone(),
                    },
                );
                let for_scope = self.push_scope(file, span, ctx.class.clone());
                let ty = if matches!(foreach.var_type, Type::Auto) {
                    elem_ty
                } else {
                    Some(foreach.var_type.clone())
                };
                let name_span = self
                    .text(file)
                    .map(|t| {
                        t.find_word_after(
                            BytePos::from_source_location(&foreach.loc),
                            &foreach.var_name,
                            &[':', '(', ')'],
                        )
                    })
                    .flatten()
                    .unwrap_or_else(|| {
                        self.span_from_loc(file, &foreach.loc, foreach.var_name.len())
                    });
                self.declare_local(
                    file,
                    &foreach.var_name,
                    ty,
                    SymbolKind::Local,
                    name_span,
                    name_span,
                    for_scope,
                    ctx,
                    String::new(),
                );
                self.visit_stmt(file, &foreach.body, ctx, for_scope);
                self.pop_scope(for_scope, span);
            }
            Stmt::Switch(switch_stmt) => {
                self.visit_expr(file, &switch_stmt.expr, ctx);
                for case in &switch_stmt.cases {
                    let span = self
                        .text(file)
                        .map(|t| {
                            let start = BytePos::from_source_location(&case.loc);
                            t.find_char(start, ':')
                                .map(|colon| Span::new(start, colon))
                                .unwrap_or(Span::point(start))
                        })
                        .unwrap_or_else(|| Span::point(BytePos::from_source_location(&case.loc)));
                    let case_scope = self.push_scope(file, span, ctx.class.clone());
                    if let Some(binding) = &case.payload_binding {
                        let bspan = self
                            .text(file)
                            .and_then(|t| {
                                t.find_word_after(
                                    BytePos::from_source_location(&case.loc),
                                    &binding.var_name,
                                    &[':', ')'],
                                )
                            })
                            .unwrap_or_else(|| {
                                self.span_from_loc(file, &case.loc, binding.var_name.len())
                            });
                        self.declare_local(
                            file,
                            &binding.var_name,
                            Some(binding.var_type.clone()),
                            SymbolKind::Local,
                            bspan,
                            bspan,
                            case_scope,
                            ctx,
                            String::new(),
                        );
                    }
                    for stmt in &case.body {
                        self.visit_stmt(file, stmt, ctx, case_scope);
                    }
                    self.pop_scope(case_scope, span);
                }
                if let Some(default) = &switch_stmt.default {
                    for stmt in default {
                        self.visit_stmt(file, stmt, ctx, scope);
                    }
                }
            }
            Stmt::Block(block) => self.visit_block(file, block, ctx),
            Stmt::Scope(scope_stmt) => self.visit_block(file, &scope_stmt.body, ctx),
            Stmt::Break(_, _) | Stmt::Continue(_, _) | Stmt::InlineIr(_) => {}
        }
    }

    fn visit_type_ref(&mut self, file: &str, ty: &Type, loc: &SourceLocation) {
        if let Type::Array(inner) = ty {
            self.visit_type_ref(file, inner, loc);
            return;
        }
        let base = base_type_name(ty);
        if base.is_empty() {
            if let Some(primitive) = primitive_type_name(ty) {
                if let Some(text) = self.text(file) {
                    if let Some(span) = text.find_word_after(
                        BytePos::from_source_location(loc),
                        primitive,
                        &[';', ',', ')', '=', '{', '>', '['],
                    ) {
                        self.add_type_occurrence(file, span, primitive);
                    }
                }
            }
            return;
        }
        let span = self
            .text(file)
            .and_then(|t| {
                t.find_word_after(
                    BytePos::from_source_location(loc),
                    &base,
                    &[';', ',', ')', '=', '{', '>', '['],
                )
            })
            .unwrap_or_else(|| self.span_from_loc(file, loc, base.len()));
        self.add_type_occurrence(file, span, &base);
    }

    fn visit_expr(&mut self, file: &str, expr: &Expr, ctx: &Ctx) -> Option<Type> {
        let start = BytePos::from_source_location(expr.location());
        let result = self.visit_expr_inner(file, expr, ctx);
        if let Some(ty) = &result {
            if let Some(text) = self.text(file) {
                let offset = text.offset(start);
                self.type_at_start
                    .insert((file.to_string(), offset), ty.clone());
            }
        }
        result
    }

    fn visit_expr_inner(&mut self, file: &str, expr: &Expr, ctx: &Ctx) -> Option<Type> {
        match expr {
            Expr::Literal(lit) => Some(match &lit.value {
                LiteralValue::Int32(_) => Type::Int32,
                LiteralValue::Int64(_) => Type::Int64,
                LiteralValue::Float32(_) => Type::Float32,
                LiteralValue::Float64(_) => Type::Float64,
                LiteralValue::String(_) => Type::String,
                LiteralValue::Bool(_) => Type::Bool,
                LiteralValue::Char(_) => Type::Char,
                LiteralValue::Null => Type::Object("null".to_string()),
            }),
            Expr::Identifier(id) => self.visit_identifier(file, id, ctx),
            Expr::MemberAccess(member) => self.visit_member_access(file, member, ctx),
            Expr::Call(call) => self.visit_call(file, call, ctx),
            Expr::Binary(binary) => {
                let left = self.visit_expr(file, &binary.left, ctx);
                let right = self.visit_expr(file, &binary.right, ctx);
                Some(infer_binary_type(&binary.op, left.as_ref(), right.as_ref()))
            }
            Expr::Unary(unary) => {
                let operand = self.visit_expr(file, &unary.operand, ctx);
                Some(match unary.op {
                    UnaryOp::Not => Type::Bool,
                    UnaryOp::AddressOf => Type::Pointer(Box::new(operand.unwrap_or(Type::CVoid))),
                    UnaryOp::Deref => match operand {
                        Some(Type::Pointer(inner)) => *inner,
                        _ => Type::Auto,
                    },
                    _ => operand.unwrap_or(Type::Auto),
                })
            }
            Expr::Cast(cast) => {
                self.visit_type_ref(file, &cast.target_type, &cast.loc);
                self.visit_expr(file, &cast.expr, ctx);
                Some(cast.target_type.clone())
            }
            Expr::ArrayCreation(creation) => {
                for size in &creation.sizes {
                    self.visit_expr(file, size, ctx);
                }
                Some(Type::Array(Box::new(creation.element_type.clone())))
            }
            Expr::ArrayAccess(access) => {
                let array_ty = self.visit_expr(file, &access.array, ctx);
                self.visit_expr(file, &access.index, ctx);
                match array_ty {
                    Some(Type::Array(inner)) => Some(*inner),
                    Some(Type::String) => Some(Type::Char),
                    Some(Type::Generic(name, args)) if name == "array" => args.first().cloned(),
                    _ => None,
                }
            }
            Expr::ArrayInit(init) => {
                let mut element_ty = None;
                for elem in &init.elements {
                    let ty = self.visit_expr(file, elem, ctx);
                    if element_ty.is_none() {
                        element_ty = ty;
                    }
                }
                Some(Type::Array(Box::new(element_ty.unwrap_or(Type::Auto))))
            }
            Expr::New(new_expr) => {
                let ty = parse_type_name(&new_expr.class_name);
                self.visit_type_ref(file, &ty, &new_expr.loc);
                for arg in &new_expr.args {
                    self.visit_expr(file, arg, ctx);
                }
                // 为 `new Foo(...)` 记录调用点，使签名帮助/参数高亮可用。
                let base = base_type_name(&ty);
                if !base.is_empty() {
                    let start = BytePos::from_source_location(&new_expr.loc);
                    let callee_span = self
                        .text(file)
                        .and_then(|t| t.find_word_after(start, &base, &['(', '<']))
                        .unwrap_or_else(|| self.span_from_loc(file, &new_expr.loc, base.len()));
                    let signatures =
                        self.constructor_signatures(&new_expr.class_name, new_expr.args.len());
                    let synthetic = CallExpr {
                        callee: Box::new(Expr::Identifier(IdentifierExpr {
                            name: base.clone(),
                            loc: new_expr.loc.clone(),
                        })),
                        args: new_expr.args.clone(),
                        loc: new_expr.loc.clone(),
                    };
                    self.record_call(
                        file,
                        &synthetic,
                        callee_span,
                        &base,
                        Some(ty.clone()),
                        signatures,
                    );
                }
                Some(ty)
            }
            Expr::Assignment(assign) => {
                let target_ty = self.visit_expr(file, &assign.target, ctx);
                self.visit_expr(file, &assign.value, ctx);
                target_ty
            }
            Expr::Ternary(ternary) => {
                self.visit_expr(file, &ternary.condition, ctx);
                let a = self.visit_expr(file, &ternary.true_branch, ctx);
                let b = self.visit_expr(file, &ternary.false_branch, ctx);
                Some(common_type(a.as_ref(), b.as_ref()))
            }
            Expr::If(if_expr) => {
                self.visit_expr(file, &if_expr.condition, ctx);
                let a = self.visit_block_expr(file, &if_expr.then_branch, ctx);
                let b = self.visit_block_expr(file, &if_expr.else_branch, ctx);
                Some(common_type(a.as_ref(), b.as_ref()))
            }
            Expr::Try(try_expr) => {
                let ty = self.visit_expr(file, &try_expr.expr, ctx);
                match ty {
                    Some(Type::Generic(name, args))
                        if (name == "Result" || name == "Optional") && !args.is_empty() =>
                    {
                        Some(args[0].clone())
                    }
                    other => other,
                }
            }
            Expr::InstanceOf(instance) => {
                self.visit_expr(file, &instance.expr, ctx);
                self.visit_type_ref(file, &instance.target_type, &instance.loc);
                Some(Type::Bool)
            }
            Expr::MethodRef(method_ref) => {
                if let Some(class_name) = &method_ref.class_name {
                    let ty = parse_type_name(class_name);
                    self.visit_type_ref(file, &ty, &method_ref.loc);
                    self.resolve_static_member(
                        file,
                        &ty,
                        &method_ref.method_name,
                        None,
                        &method_ref.loc,
                    )
                    .and_then(|m| m.ty)
                    .or(Some(Type::Auto))
                } else if let Some(object) = &method_ref.object {
                    let obj_ty = self.visit_expr(file, object, ctx)?;
                    self.resolve_instance_member(
                        file,
                        &obj_ty,
                        &method_ref.method_name,
                        None,
                        &method_ref.loc,
                    )
                    .and_then(|m| m.ty)
                } else {
                    None
                }
            }
            Expr::Lambda(lambda) => {
                let span = self
                    .text(file)
                    .map(|_t| {
                        let start = BytePos::from_source_location(&lambda.loc);
                        Span::new(start, start)
                    })
                    .unwrap_or_else(|| Span::point(BytePos::from_source_location(&lambda.loc)));
                let scope = self.push_scope(file, span, ctx.class.clone());
                let mut param_tys = Vec::new();
                for p in &lambda.params {
                    let ty = p.param_type.clone().unwrap_or(Type::Auto);
                    let name_span = self
                        .text(file)
                        .and_then(|t| {
                            t.find_word_after(
                                BytePos::from_source_location(&lambda.loc),
                                &p.name,
                                &[')', ','],
                            )
                        })
                        .unwrap_or_else(|| self.span_from_loc(file, &lambda.loc, p.name.len()));
                    self.declare_local(
                        file,
                        &p.name,
                        Some(ty.clone()),
                        SymbolKind::Parameter,
                        name_span,
                        name_span,
                        scope,
                        ctx,
                        String::new(),
                    );
                    param_tys.push(ty);
                }
                let ret = match &lambda.body {
                    LambdaBody::Expr(expr) => self.visit_expr(file, expr, ctx),
                    LambdaBody::Block(block) => self.visit_block_expr(file, block, ctx),
                };
                self.pop_scope(scope, span);
                Some(Type::Function(Box::new(crate::types::FunctionType {
                    params: param_tys,
                    return_type: Box::new(ret.unwrap_or(Type::Auto)),
                    is_static: false,
                    is_closure: true,
                })))
            }
            Expr::Alloc(alloc) => {
                self.visit_expr(file, &alloc.size, ctx);
                if let Some(align) = &alloc.align {
                    self.visit_expr(file, align, ctx);
                }
                Some(Type::Pointer(Box::new(Type::CVoid)))
            }
            Expr::Dealloc(dealloc) => {
                self.visit_expr(file, &dealloc.ptr, ctx);
                Some(Type::Void)
            }
            Expr::AllocArray(alloc) => {
                self.visit_expr(file, &alloc.allocator, ctx);
                self.visit_expr(file, &alloc.size, ctx);
                Some(Type::Array(Box::new(alloc.element_type.clone())))
            }
            Expr::NamedArg(named) => self.visit_expr(file, &named.value, ctx),
            Expr::TypeOf(type_of) => {
                self.visit_expr(file, &type_of.expr, ctx);
                Some(Type::Object("Type".to_string()))
            }
            Expr::SizeOf(size_of) => {
                match &size_of.target {
                    SizeOfTarget::Type(ty) => self.visit_type_ref(file, ty, &size_of.loc),
                    SizeOfTarget::Expr(expr) => {
                        self.visit_expr(file, expr, ctx);
                    }
                }
                Some(Type::Int64)
            }
        }
    }

    fn visit_block_expr(&mut self, file: &str, block: &Block, ctx: &Ctx) -> Option<Type> {
        let span = self.expr_block_span(file, block);
        let scope = self.push_scope(file, span, ctx.class.clone());
        for stmt in &block.statements {
            self.visit_stmt(file, stmt, ctx, scope);
        }
        let tail = block
            .tail_expr
            .as_ref()
            .and_then(|e| self.visit_expr(file, e, ctx));
        self.pop_scope(scope, span);
        tail
    }

    // ------------------------------------------------------------------
    // 标识符 / 成员 / 调用解析
    // ------------------------------------------------------------------

    fn lookup_local(&self, name: &str) -> Option<SymbolId> {
        for scope in self.scope_stack.iter().rev() {
            if let Some(id) = self.scopes.get(*scope).and_then(|s| s.names.get(name)) {
                return Some(*id);
            }
        }
        None
    }

    fn class_chain(&self, name: &str) -> Vec<ClassInfo> {
        let mut out = Vec::new();
        let mut current = Some(name.to_string());
        let mut seen = HashSet::new();
        while let Some(name) = current {
            if !seen.insert(name.clone()) {
                break;
            }
            let Some(class) = self.registry.get_class(&name) else {
                break;
            };
            out.push(class.clone());
            current = class.parent.clone();
        }
        out
    }

    fn lookup_field_in_class(
        &self,
        class: &str,
        name: &str,
        receiver: &Type,
    ) -> Option<(String, FieldInfo, Type)> {
        for info in self.class_chain(class) {
            if let Some(field) = info.fields.get(name) {
                let ty = substitute_receiver(receiver, field, &info);
                return Some((info.name.clone(), field.clone(), ty));
            }
        }
        None
    }

    fn lookup_field_in_struct(
        &self,
        strukt: &str,
        name: &str,
        receiver: &Type,
    ) -> Option<(String, FieldInfo, Type)> {
        let info = self.registry.get_struct(strukt)?.clone();
        let field = info.fields.get(name)?.clone();
        let args = type_args_of(receiver);
        let ty = if info.type_params.is_empty() || args.is_empty() {
            field.field_type.clone()
        } else {
            let mapping: HashMap<String, Type> = info
                .type_params
                .iter()
                .map(|p| p.name.clone())
                .zip(args)
                .collect();
            substitute_type_params(&field.field_type, &mapping)
        };
        Some((info.name.clone(), field, ty))
    }

    fn signatures_from_class_methods(
        &mut self,
        receiver: &Type,
        class_name: &str,
        name: &str,
        arity: Option<usize>,
        want_static: bool,
    ) -> Vec<Signature> {
        let mut out = Vec::new();
        for info in self.class_chain(class_name) {
            let Some(methods) = info.methods.get(name) else {
                continue;
            };
            for method in methods {
                if method.is_static != want_static {
                    continue;
                }
                if let Some(arity) = arity {
                    if !method_accepts(method, arity) {
                        continue;
                    }
                }
                let specialized = substitute_receiver_method(receiver, method, &info);
                let symbol = self
                    .method_symbols
                    .get(&(info.name.clone(), name.to_string(), method.params.len()))
                    .copied();
                let (doc, loc) = symbol
                    .and_then(|id| self.symbols.get(id))
                    .map(|s| (s.documentation.clone(), Some(s.location.clone())))
                    .unwrap_or_default();
                out.push(method_to_signature(&specialized, symbol, loc, doc));
            }
        }
        out
    }

    fn signatures_from_struct_methods(
        &mut self,
        receiver: &Type,
        strukt: &str,
        name: &str,
        arity: Option<usize>,
        want_static: bool,
    ) -> Vec<Signature> {
        let Some(info) = self.registry.get_struct(strukt).cloned() else {
            return Vec::new();
        };
        let Some(methods) = info.methods.get(name) else {
            return Vec::new();
        };
        let pseudo_class = ClassInfo {
            name: info.name.clone(),
            type_params: info.type_params.clone(),
            methods: HashMap::new(),
            fields: HashMap::new(),
            constructors: Vec::new(),
            has_destructor: false,
            destructor_is_native: false,
            parent: None,
            interfaces: Vec::new(),
            is_abstract: false,
            is_final: false,
            is_interop: false,
            is_stack_only: false,
            vtable_layout: None,
        };
        let mut out = Vec::new();
        for method in methods {
            if method.is_static != want_static {
                continue;
            }
            if let Some(arity) = arity {
                if !method_accepts(method, arity) {
                    continue;
                }
            }
            let specialized = substitute_receiver_method(receiver, method, &pseudo_class);
            let symbol = self
                .method_symbols
                .get(&(info.name.clone(), name.to_string(), method.params.len()))
                .copied();
            let (doc, loc) = symbol
                .and_then(|id| self.symbols.get(id))
                .map(|s| (s.documentation.clone(), Some(s.location.clone())))
                .unwrap_or_default();
            out.push(method_to_signature(&specialized, symbol, loc, doc));
        }
        out
    }

    fn signatures_from_enum_methods(
        &mut self,
        _receiver: &Type,
        en: &str,
        name: &str,
        arity: Option<usize>,
        want_static: bool,
    ) -> Vec<Signature> {
        let Some(info) = self.registry.get_enum(en).cloned() else {
            return Vec::new();
        };
        let Some(methods) = info.methods.get(name) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for method in methods {
            if method.is_static != want_static {
                continue;
            }
            if let Some(arity) = arity {
                if !method_accepts(method, arity) {
                    continue;
                }
            }
            let symbol = self
                .method_symbols
                .get(&(info.name.clone(), name.to_string(), method.params.len()))
                .copied();
            let (doc, loc) = symbol
                .and_then(|id| self.symbols.get(id))
                .map(|s| (s.documentation.clone(), Some(s.location.clone())))
                .unwrap_or_default();
            out.push(method_to_signature(method, symbol, loc, doc));
        }
        out
    }

    fn builtin_member_signatures(&mut self, member: &BuiltinMember) -> Vec<Signature> {
        if member.is_method() {
            vec![Signature {
                label: member.label.clone(),
                params: member.params.clone(),
                return_type: member.ty.clone(),
                documentation: member.documentation.clone(),
                location: Some(member.location.clone()),
                symbol: Some(self.ensure_builtin_member(member)),
                is_static: member.is_static,
            }]
        } else {
            Vec::new()
        }
    }

    fn member_from_signatures(
        signatures: Vec<Signature>,
        call_arity: Option<usize>,
    ) -> Option<MemberRes> {
        let first = signatures.first()?;
        let ty = if call_arity.is_some() {
            Some(first.return_type.clone())
        } else {
            Some(Type::Function(Box::new(crate::types::FunctionType {
                params: first.params.iter().map(|p| p.ty.clone()).collect(),
                return_type: Box::new(first.return_type.clone()),
                is_static: first.is_static,
                is_closure: false,
            })))
        };
        Some(MemberRes {
            ty,
            symbol: first.symbol,
            signatures,
        })
    }

    fn resolve_instance_member(
        &mut self,
        file: &str,
        receiver: &Type,
        member_name: &str,
        call_arity: Option<usize>,
        _loc: &SourceLocation,
    ) -> Option<MemberRes> {
        let _ = file;
        // 1) 数组 / String / 基本类型 / Object 等内建成员。
        if matches!(
            receiver,
            Type::Array(_)
                | Type::String
                | Type::Int32
                | Type::Int64
                | Type::Float32
                | Type::Float64
                | Type::Bool
                | Type::Char
        ) || matches!(receiver, Type::Object(n) if n == "Object" || n == "Integer")
        {
            let members = self.builtins.members_for(receiver);
            if !members.is_empty() {
                if let Some(member) =
                    select_builtin_member(&members, member_name, call_arity, false)
                {
                    if member.is_method() {
                        let signatures = self.builtin_member_signatures(&member);
                        return Self::member_from_signatures(signatures, call_arity);
                    }
                    let id = self.ensure_builtin_member(&member);
                    return Some(MemberRes {
                        ty: Some(member.ty.clone()),
                        symbol: Some(id),
                        signatures: Vec::new(),
                    });
                }
            }
        }

        // 2) 泛型参数：使用其 bound。
        let receiver = match receiver {
            Type::GenericParam(name) => {
                let bound = self
                    .current_class_type_params
                    .iter()
                    .find(|p| *p == name)
                    .map(|_| "Object".to_string())
                    .unwrap_or_else(|| "Object".to_string());
                Type::Object(bound)
            }
            other => other.clone(),
        };

        // 3) 用户类 / struct / enum / 接口。
        match &receiver {
            Type::Object(class_name) => {
                let base = class_name.split('<').next().unwrap_or(class_name).trim();
                if base == "null" {
                    return None;
                }
                if let Some((owner, _field, ty)) =
                    self.lookup_field_in_class(base, member_name, &receiver)
                {
                    let symbol = self
                        .field_symbols
                        .get(&(owner.clone(), member_name.to_string()))
                        .copied();
                    return Some(MemberRes {
                        ty: Some(ty),
                        symbol,
                        signatures: Vec::new(),
                    });
                }
                let signatures = self.signatures_from_class_methods(
                    &receiver,
                    base,
                    member_name,
                    call_arity,
                    false,
                );
                if !signatures.is_empty() {
                    return Self::member_from_signatures(signatures, call_arity);
                }
                // Object 根类方法（hashCode/equals）作为最后回退。
                if base != "Object" {
                    let object_ty = Type::Object("Object".to_string());
                    let members = self.builtins.members_for(&object_ty);
                    if let Some(member) =
                        select_builtin_member(&members, member_name, call_arity, false)
                    {
                        if member.is_method() {
                            let signatures = self.builtin_member_signatures(&member);
                            return Self::member_from_signatures(signatures, call_arity);
                        }
                    }
                }
                None
            }
            Type::Generic(base, _) => {
                if let Some((owner, _field, ty)) =
                    self.lookup_field_in_class(base, member_name, &receiver)
                {
                    let symbol = self
                        .field_symbols
                        .get(&(owner.clone(), member_name.to_string()))
                        .copied();
                    return Some(MemberRes {
                        ty: Some(ty),
                        symbol,
                        signatures: Vec::new(),
                    });
                }
                let signatures = self.signatures_from_class_methods(
                    &receiver,
                    base,
                    member_name,
                    call_arity,
                    false,
                );
                if !signatures.is_empty() {
                    return Self::member_from_signatures(signatures, call_arity);
                }
                None
            }
            Type::Struct(name) => {
                if let Some((owner, _field, ty)) =
                    self.lookup_field_in_struct(name, member_name, &receiver)
                {
                    let symbol = self
                        .field_symbols
                        .get(&(owner.clone(), member_name.to_string()))
                        .copied();
                    return Some(MemberRes {
                        ty: Some(ty),
                        symbol,
                        signatures: Vec::new(),
                    });
                }
                let signatures = self.signatures_from_struct_methods(
                    &receiver,
                    name,
                    member_name,
                    call_arity,
                    false,
                );
                if !signatures.is_empty() {
                    return Self::member_from_signatures(signatures, call_arity);
                }
                None
            }
            _ => None,
        }
    }

    fn resolve_static_member(
        &mut self,
        file: &str,
        ty: &Type,
        name: &str,
        call_arity: Option<usize>,
        loc: &SourceLocation,
    ) -> Option<MemberRes> {
        let _ = (file, loc);
        let base = base_type_name(ty);
        if base.is_empty() {
            return None;
        }

        // enum variant
        if let Some(variant) = self
            .variant_symbols
            .get(&(base.clone(), name.to_string()))
            .copied()
        {
            return Some(MemberRes {
                ty: Some(Type::Object(base.clone())),
                symbol: Some(variant),
                signatures: Vec::new(),
            });
        }

        // 用户类 / struct / enum 的静态成员
        if let Some(class) = self.registry.get_class(&base).cloned() {
            if let Some(field) = class.fields.get(name).filter(|f| f.is_static) {
                let symbol = self
                    .field_symbols
                    .get(&(class.name.clone(), name.to_string()))
                    .copied();
                return Some(MemberRes {
                    ty: Some(field.field_type.clone()),
                    symbol,
                    signatures: Vec::new(),
                });
            }
            let mut signatures = self.signatures_from_class_methods(
                &Type::Object(base.clone()),
                &base,
                name,
                call_arity,
                true,
            );
            if !signatures.is_empty() {
                if let Some(first) = signatures.first_mut() {
                    if first.symbol.is_none() {
                        let statics = self.builtins.static_members_for(&base);
                        if let Some(member) =
                            select_builtin_member(&statics, name, call_arity, true)
                        {
                            let id = self.ensure_builtin_member(&member);
                            first.symbol = Some(id);
                            first.label = member.label.clone();
                            first.documentation = member.documentation.clone();
                            first.location = Some(member.location.clone());
                        } else {
                            first.symbol = self.class_symbols.get(&base).copied();
                        }
                    }
                }
                return Self::member_from_signatures(signatures, call_arity);
            }
        }
        if let Some(_strukt) = self.registry.get_struct(&base) {
            let signatures = self.signatures_from_struct_methods(
                &Type::Struct(base.clone()),
                &base,
                name,
                call_arity,
                true,
            );
            if !signatures.is_empty() {
                return Self::member_from_signatures(signatures, call_arity);
            }
        }
        if let Some(_) = self.registry.get_enum(&base) {
            let signatures = self.signatures_from_enum_methods(
                &Type::Object(base.clone()),
                &base,
                name,
                call_arity,
                true,
            );
            if !signatures.is_empty() {
                return Self::member_from_signatures(signatures, call_arity);
            }
        }

        // 内建静态成员（String.valueOf / Integer.parseInt 等）。
        let statics = self.builtins.static_members_for(&base);
        if let Some(member) = select_builtin_member(&statics, name, call_arity, true) {
            if member.is_method() {
                let signatures = self.builtin_member_signatures(&member);
                return Self::member_from_signatures(signatures, call_arity);
            }
            let id = self.ensure_builtin_member(&member);
            return Some(MemberRes {
                ty: Some(member.ty.clone()),
                symbol: Some(id),
                signatures: Vec::new(),
            });
        }

        None
    }

    fn visit_identifier(&mut self, file: &str, id: &IdentifierExpr, ctx: &Ctx) -> Option<Type> {
        let name = id.name.as_str();
        let span = span_for_name(Span::point(BytePos::from_source_location(&id.loc)), name);

        if name == "this" {
            if let Some(class) = &ctx.class {
                let symbol = self.class_symbols.get(class).copied();
                let ty = Type::Object(class.clone());
                self.add_occurrence(Occurrence {
                    file: file.to_string(),
                    span,
                    name: name.to_string(),
                    kind: OccurrenceKind::Value,
                    symbol,
                    receiver: None,
                    result: Some(ty.clone()),
                    is_definition: false,
                });
                return Some(ty);
            }
            return None;
        }
        if name == "super" {
            if let Some(class) = &ctx.class {
                let parent = self
                    .registry
                    .get_class(class)
                    .and_then(|c| c.parent.clone());
                if let Some(parent) = parent {
                    let symbol = self.class_symbols.get(&parent).copied();
                    let ty = Type::Object(parent.clone());
                    self.add_occurrence(Occurrence {
                        file: file.to_string(),
                        span,
                        name: name.to_string(),
                        kind: OccurrenceKind::Value,
                        symbol,
                        receiver: None,
                        result: Some(ty.clone()),
                        is_definition: false,
                    });
                    return Some(ty);
                }
            }
            return None;
        }

        // 局部变量 / 参数
        if let Some(symbol) = self.lookup_local(name) {
            let ty = self.symbols.get(symbol).and_then(|s| s.ty.clone());
            self.add_occurrence(Occurrence {
                file: file.to_string(),
                span,
                name: name.to_string(),
                kind: OccurrenceKind::Value,
                symbol: Some(symbol),
                receiver: None,
                result: ty.clone(),
                is_definition: false,
            });
            return ty;
        }

        // 当前类字段
        if let Some(class) = &ctx.class {
            if let Some((owner, _field, ty)) =
                self.lookup_field_in_class(class, name, &Type::Object(class.clone()))
            {
                let symbol = self
                    .field_symbols
                    .get(&(owner.clone(), name.to_string()))
                    .copied();
                self.add_occurrence(Occurrence {
                    file: file.to_string(),
                    span,
                    name: name.to_string(),
                    kind: OccurrenceKind::Value,
                    symbol,
                    receiver: None,
                    result: Some(ty.clone()),
                    is_definition: false,
                });
                return Some(ty);
            }
        }

        // 顶层函数
        if let Some(symbol) = self.function_symbols.get(name).copied() {
            let ty = self.symbols.get(symbol).and_then(|s| s.ty.clone());
            self.add_occurrence(Occurrence {
                file: file.to_string(),
                span,
                name: name.to_string(),
                kind: OccurrenceKind::Value,
                symbol: Some(symbol),
                receiver: None,
                result: ty.clone(),
                is_definition: false,
            });
            return ty;
        }

        // 类型名
        if let Some(symbol) = self.resolve_type_symbol(name) {
            let ty = match &self.symbols[symbol].key {
                SymbolKey::Struct(name) => Type::Struct(name.clone()),
                SymbolKey::BuiltinType(canonical) => match canonical.as_str() {
                    "String" => Type::String,
                    "int" => Type::Int32,
                    "long" => Type::Int64,
                    "float" => Type::Float32,
                    "double" => Type::Float64,
                    "bool" => Type::Bool,
                    "char" => Type::Char,
                    other => Type::Object(other.to_string()),
                },
                SymbolKey::Class(name) => Type::Object(name.clone()),
                SymbolKey::Interface(name) => Type::Object(name.clone()),
                SymbolKey::Enum(name) => Type::Object(name.clone()),
                _ => Type::Object(name.to_string()),
            };
            self.add_occurrence(Occurrence {
                file: file.to_string(),
                span,
                name: name.to_string(),
                kind: OccurrenceKind::Type,
                symbol: Some(symbol),
                receiver: None,
                result: Some(ty.clone()),
                is_definition: false,
            });
            return Some(ty);
        }

        // 全局内建函数
        if let Some(method) = self.builtins.global_function_symbol(name) {
            let method = method.clone();
            let symbol = self.ensure_builtin_function(&method);
            let ty = self.symbols.get(symbol).and_then(|s| s.ty.clone());
            self.add_occurrence(Occurrence {
                file: file.to_string(),
                span,
                name: name.to_string(),
                kind: OccurrenceKind::Value,
                symbol: Some(symbol),
                receiver: None,
                result: ty.clone(),
                is_definition: false,
            });
            return ty;
        }

        self.add_occurrence(Occurrence {
            file: file.to_string(),
            span,
            name: name.to_string(),
            kind: OccurrenceKind::Value,
            symbol: None,
            receiver: None,
            result: None,
            is_definition: false,
        });
        None
    }

    fn identifier_has_value_binding(&self, name: &str, ctx: &Ctx) -> bool {
        if self.lookup_local(name).is_some() {
            return true;
        }
        if let Some(class) = &ctx.class {
            if self
                .lookup_field_in_class(class, name, &Type::Object(class.clone()))
                .is_some()
            {
                return true;
            }
        }
        self.function_symbols.contains_key(name)
            || self.builtins.global_function_symbol(name).is_some()
    }

    fn visit_member_access(
        &mut self,
        file: &str,
        member: &MemberAccessExpr,
        ctx: &Ctx,
    ) -> Option<Type> {
        let member_span = self
            .text(file)
            .and_then(|t| {
                t.find_word_after(
                    BytePos::from_source_location(&member.object.location()),
                    &member.member,
                    &['(', ';', ',', ')', '='],
                )
            })
            .unwrap_or_else(|| self.span_from_loc(file, &member.loc, member.member.len()));

        // 静态访问：ClassName.member / EnumName.Variant / String.valueOf
        if let Expr::Identifier(obj) = member.object.as_ref() {
            if !self.identifier_has_value_binding(&obj.name, ctx) {
                if let Some(ty) = self.static_receiver_type(&obj.name) {
                    if let Some(res) =
                        self.resolve_static_member(file, &ty, &member.member, None, &member.loc)
                    {
                        self.add_occurrence(Occurrence {
                            file: file.to_string(),
                            span: member_span,
                            name: member.member.clone(),
                            kind: OccurrenceKind::Member,
                            symbol: res.symbol,
                            receiver: Some(ty.clone()),
                            result: res.ty.clone(),
                            is_definition: false,
                        });
                        return res.ty;
                    }
                    // 静态成员解析失败也记录一次引用，便于诊断/快速修复。
                    self.add_occurrence(Occurrence {
                        file: file.to_string(),
                        span: member_span,
                        name: member.member.clone(),
                        kind: OccurrenceKind::Member,
                        symbol: None,
                        receiver: Some(ty),
                        result: None,
                        is_definition: false,
                    });
                    return None;
                }
            }
        }

        let obj_ty = self.visit_expr(file, &member.object, ctx)?;
        match self.resolve_instance_member(file, &obj_ty, &member.member, None, &member.loc) {
            Some(res) => {
                self.add_occurrence(Occurrence {
                    file: file.to_string(),
                    span: member_span,
                    name: member.member.clone(),
                    kind: OccurrenceKind::Member,
                    symbol: res.symbol,
                    receiver: Some(obj_ty),
                    result: res.ty.clone(),
                    is_definition: false,
                });
                res.ty
            }
            None => {
                // 未知成员也要保留 occurrence，否则诊断位置上的 quick fix / hover
                // 无法定位接收者类型。
                self.add_occurrence(Occurrence {
                    file: file.to_string(),
                    span: member_span,
                    name: member.member.clone(),
                    kind: OccurrenceKind::Member,
                    symbol: None,
                    receiver: Some(obj_ty),
                    result: None,
                    is_definition: false,
                });
                None
            }
        }
    }

    fn static_receiver_type(&mut self, name: &str) -> Option<Type> {
        let base = name.split('<').next().unwrap_or(name).trim();
        if self.class_symbols.contains_key(base) {
            return Some(Type::Object(base.to_string()));
        }
        if self.struct_symbols.contains_key(base) {
            return Some(Type::Struct(base.to_string()));
        }
        if self.enum_symbols.contains_key(base) {
            return Some(Type::Object(base.to_string()));
        }
        if self.resolve_type_symbol(base).is_some() {
            return match base {
                "String" => Some(Type::String),
                "int" => Some(Type::Int32),
                "long" => Some(Type::Int64),
                "float" => Some(Type::Float32),
                "double" => Some(Type::Float64),
                "bool" => Some(Type::Bool),
                "char" => Some(Type::Char),
                other => Some(Type::Object(other.to_string())),
            };
        }
        None
    }

    fn visit_call(&mut self, file: &str, call: &CallExpr, ctx: &Ctx) -> Option<Type> {
        match call.callee.as_ref() {
            Expr::MemberAccess(member) => self.visit_member_call(file, member, call, ctx),
            Expr::Identifier(id) => self.visit_identifier_call(file, id, call, ctx),
            Expr::MethodRef(method_ref) => {
                let ty = match (&method_ref.class_name, &method_ref.object) {
                    (Some(class_name), _) => {
                        let recv = parse_type_name(class_name);
                        self.visit_type_ref(file, &recv, &method_ref.loc);
                        self.resolve_static_member(
                            file,
                            &recv,
                            &method_ref.method_name,
                            Some(call.args.len()),
                            &method_ref.loc,
                        )
                    }
                    (None, Some(object)) => {
                        let recv = self.visit_expr(file, object, ctx);
                        recv.and_then(|recv| {
                            self.resolve_instance_member(
                                file,
                                &recv,
                                &method_ref.method_name,
                                Some(call.args.len()),
                                &method_ref.loc,
                            )
                        })
                    }
                    _ => None,
                };
                for arg in &call.args {
                    self.visit_expr(file, arg, ctx);
                }
                let (callee_span, signatures) = match ty {
                    Some(res) => (
                        self.text(file)
                            .map(|t| {
                                let s = BytePos::from_source_location(&method_ref.loc);
                                let e = t
                                    .find_word_after(s, &method_ref.method_name, &['(', ';'])
                                    .map(|sp| sp.end)
                                    .unwrap_or(s);
                                Span::new(s, e)
                            })
                            .unwrap_or_else(|| {
                                Span::point(BytePos::from_source_location(&method_ref.loc))
                            }),
                        res.signatures,
                    ),
                    None => (
                        Span::point(BytePos::from_source_location(&method_ref.loc)),
                        Vec::new(),
                    ),
                };
                self.record_call(
                    file,
                    call,
                    callee_span,
                    &method_ref.method_name,
                    None,
                    signatures,
                );
                Some(Type::Auto)
            }
            other => {
                let callee_ty = self.visit_expr(file, other, ctx);
                for arg in &call.args {
                    self.visit_expr(file, arg, ctx);
                }
                match callee_ty {
                    Some(Type::Function(f)) => Some(*f.return_type),
                    _ => None,
                }
            }
        }
    }

    fn visit_member_call(
        &mut self,
        file: &str,
        member: &MemberAccessExpr,
        call: &CallExpr,
        ctx: &Ctx,
    ) -> Option<Type> {
        let member_span = self.callee_member_span(file, call, &member.member);
        let arity = call.args.len();

        let mut static_receiver = None;
        if let Expr::Identifier(obj) = member.object.as_ref() {
            if !self.identifier_has_value_binding(&obj.name, ctx) {
                static_receiver = self.static_receiver_type(&obj.name);
            }
        }

        let mut instance_receiver: Option<Type> = None;
        let res = if let Some(recv) = &static_receiver {
            self.resolve_static_member(file, recv, &member.member, Some(arity), &member.loc)
        } else {
            let obj_ty = self.visit_expr(file, &member.object, ctx);
            instance_receiver = obj_ty.clone();
            obj_ty.and_then(|obj_ty| {
                self.resolve_instance_member(
                    file,
                    &obj_ty,
                    &member.member,
                    Some(arity),
                    &member.loc,
                )
            })
        };

        if static_receiver.is_some() || instance_receiver.is_some() {
            self.add_occurrence(Occurrence {
                file: file.to_string(),
                span: member_span,
                name: member.member.clone(),
                kind: OccurrenceKind::MemberCall,
                symbol: res.as_ref().and_then(|r| r.symbol),
                receiver: static_receiver
                    .clone()
                    .or_else(|| instance_receiver.clone()),
                result: res.as_ref().and_then(|r| r.ty.clone()),
                is_definition: false,
            });
        }

        for arg in &call.args {
            self.visit_expr(file, arg, ctx);
        }

        let signatures = res
            .as_ref()
            .map(|r| r.signatures.clone())
            .unwrap_or_default();
        let result_ty = res.as_ref().and_then(|r| r.ty.clone());
        self.record_call(
            file,
            call,
            member_span,
            &member.member,
            static_receiver
                .clone()
                .or_else(|| instance_receiver.clone()),
            signatures,
        );
        result_ty
    }

    fn visit_identifier_call(
        &mut self,
        file: &str,
        id: &IdentifierExpr,
        call: &CallExpr,
        ctx: &Ctx,
    ) -> Option<Type> {
        let name = id.name.as_str();
        let span = span_for_name(Span::point(BytePos::from_source_location(&id.loc)), name);
        let arity = call.args.len();

        // 先访问参数，得到类型用于重载选择。
        let mut arg_types = Vec::new();
        for arg in &call.args {
            arg_types.push(self.visit_expr(file, arg, ctx));
        }

        // 类构造 / 省略 new 的实例化
        if let Some(class_name) = self.type_name_for_value(name) {
            let ty = parse_type_name(&class_name);
            let signatures = self.constructor_signatures(&class_name, arity);
            let symbol = self
                .constructor_symbols
                .get(&(base_type_name(&ty), arity))
                .copied()
                .or_else(|| self.class_symbols.get(&base_type_name(&ty)).copied());
            self.add_occurrence(Occurrence {
                file: file.to_string(),
                span,
                name: name.to_string(),
                kind: OccurrenceKind::Call,
                symbol,
                receiver: None,
                result: Some(ty.clone()),
                is_definition: false,
            });
            self.record_call(file, call, span, name, None, signatures);
            return Some(ty);
        }

        // 局部函数指针
        if let Some(symbol) = self.lookup_local(name) {
            let ty = self.symbols.get(symbol).and_then(|s| s.ty.clone());
            if let Some(Type::Function(f)) = &ty {
                self.add_occurrence(Occurrence {
                    file: file.to_string(),
                    span,
                    name: name.to_string(),
                    kind: OccurrenceKind::Call,
                    symbol: Some(symbol),
                    receiver: None,
                    result: ty.clone(),
                    is_definition: false,
                });
                self.record_call(file, call, span, name, None, Vec::new());
                return Some((*f.return_type).clone());
            }
        }

        // 顶层函数
        if let Some(symbol) = self.function_symbols.get(name).copied() {
            if let Some(sym) = self.symbols.get(symbol).cloned() {
                let signatures = vec![Signature {
                    label: sym.signature.clone(),
                    params: sym.params.clone(),
                    return_type: sym.return_type.clone().unwrap_or(Type::Auto),
                    documentation: sym.documentation.clone(),
                    location: Some(sym.location.clone()),
                    symbol: Some(symbol),
                    is_static: true,
                }];
                self.add_occurrence(Occurrence {
                    file: file.to_string(),
                    span,
                    name: name.to_string(),
                    kind: OccurrenceKind::Call,
                    symbol: Some(symbol),
                    receiver: None,
                    result: sym.return_type.clone(),
                    is_definition: false,
                });
                self.record_call(file, call, span, name, None, signatures);
                return sym.return_type;
            }
        }

        // 当前类方法（未限定调用）
        if let Some(class) = &ctx.class {
            let signatures = self.signatures_from_class_methods(
                &Type::Object(class.clone()),
                class,
                name,
                Some(arity),
                false,
            );
            let static_signatures = self.signatures_from_class_methods(
                &Type::Object(class.clone()),
                class,
                name,
                Some(arity),
                true,
            );
            let signatures = if !signatures.is_empty() {
                signatures
            } else {
                static_signatures
            };
            if !signatures.is_empty() {
                let symbol = signatures.first().and_then(|s| s.symbol);
                let result = signatures.first().map(|s| s.return_type.clone());
                self.add_occurrence(Occurrence {
                    file: file.to_string(),
                    span,
                    name: name.to_string(),
                    kind: OccurrenceKind::Call,
                    symbol,
                    receiver: None,
                    result: result.clone(),
                    is_definition: false,
                });
                self.record_call(file, call, span, name, None, signatures);
                return result;
            }
        }

        // 全局内建函数
        if let Some(method) = self.builtins.find_global_function(name, arity) {
            let method = method.clone();
            let symbol = self.ensure_builtin_function(&method);
            let signatures = vec![Signature {
                label: method.label.clone(),
                params: method.params.clone(),
                return_type: method.return_type.clone(),
                documentation: method.documentation.clone(),
                location: Some(method.location.clone()),
                symbol: Some(symbol),
                is_static: true,
            }];
            self.add_occurrence(Occurrence {
                file: file.to_string(),
                span,
                name: name.to_string(),
                kind: OccurrenceKind::Call,
                symbol: Some(symbol),
                receiver: None,
                result: Some(method.return_type.clone()),
                is_definition: false,
            });
            self.record_call(file, call, span, name, None, signatures);
            return Some(method.return_type);
        }

        // 未解析
        self.add_occurrence(Occurrence {
            file: file.to_string(),
            span,
            name: name.to_string(),
            kind: OccurrenceKind::Call,
            symbol: None,
            receiver: None,
            result: None,
            is_definition: false,
        });
        self.record_call(file, call, span, name, None, Vec::new());
        None
    }

    fn type_name_for_value(&self, name: &str) -> Option<String> {
        if self.class_symbols.contains_key(name)
            || self.registry.get_class(name).is_some()
            || self.builtins.get_by_name(name).is_some()
        {
            return Some(name.to_string());
        }
        None
    }

    fn constructor_signatures(&mut self, class_name: &str, arity: usize) -> Vec<Signature> {
        let base = class_name.split('<').next().unwrap_or(class_name).trim();
        let ty = parse_type_name(class_name);
        let mut out = Vec::new();
        if let Some(class) = self.registry.get_class(base).cloned() {
            for ctor in &class.constructors {
                if !method_accepts(
                    &MethodInfo {
                        name: base.to_string(),
                        class_name: base.to_string(),
                        type_params: Vec::new(),
                        params: ctor.params.clone(),
                        return_type: ty.clone(),
                        is_static: false,
                        is_public: ctor.is_public,
                        is_private: ctor.is_private,
                        is_protected: ctor.is_protected,
                        is_native: ctor.is_native,
                        is_const: false,
                        is_abstract: false,
                        is_override: false,
                        is_final: false,
                        is_test: false,
                        vtable_slot: None,
                        loc: ctor.loc.clone(),
                    },
                    arity,
                ) {
                    continue;
                }
                let symbol = self
                    .constructor_symbols
                    .get(&(base.to_string(), ctor.params.len()))
                    .copied();
                let params: Vec<ParamSig> = ctor
                    .params
                    .iter()
                    .map(|p| ParamSig {
                        name: p.name.clone(),
                        ty: p.param_type.clone(),
                        is_varargs: p.is_varargs,
                    })
                    .collect();
                let label = format!(
                    "{}({})",
                    base,
                    params
                        .iter()
                        .map(|p| p.display())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                out.push(Signature {
                    label,
                    params,
                    return_type: ty.clone(),
                    documentation: String::new(),
                    location: self
                        .constructor_symbols
                        .get(&(base.to_string(), ctor.params.len()))
                        .and_then(|id| self.symbols.get(*id))
                        .map(|s| s.location.clone()),
                    symbol,
                    is_static: false,
                });
            }
        }
        if out.is_empty() && arity == 0 {
            let symbol = self
                .constructor_symbols
                .get(&(base.to_string(), 0))
                .copied()
                .or_else(|| self.class_symbols.get(base).copied());
            out.push(Signature {
                label: format!("{}()", base),
                params: Vec::new(),
                return_type: ty,
                documentation: String::new(),
                location: symbol
                    .and_then(|id| self.symbols.get(id))
                    .map(|s| s.location.clone()),
                symbol,
                is_static: false,
            });
        }
        out
    }

    fn callee_member_span(&self, file: &str, call: &CallExpr, member: &str) -> Span {
        let start = BytePos::from_source_location(&call.loc);
        let Some(text) = self.text(file) else {
            return self.span_from_loc(file, &call.loc, member.len());
        };
        let Some(open) = text.find_char(start, '(') else {
            return self.span_from_loc(file, &call.loc, member.len());
        };
        let cut = text.offset(open);
        let bytes = text.text[..cut].as_bytes();
        let mut end = cut;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        // 跳过泛型实参 <T>
        if end > 0 && bytes[end - 1] == b'>' {
            let mut depth = 0i32;
            let mut i = end;
            while i > 0 {
                let b = bytes[i - 1];
                if b == b'>' {
                    depth += 1;
                } else if b == b'<' {
                    depth -= 1;
                    if depth == 0 {
                        end = i - 1;
                        break;
                    }
                }
                i -= 1;
            }
            while end > 0 && bytes[end - 1].is_ascii_whitespace() {
                end -= 1;
            }
        }
        let mut start_idx = end;
        while start_idx > 0 {
            let b = bytes[start_idx - 1];
            if b.is_ascii_alphanumeric() || b == b'_' || b == b'$' {
                start_idx -= 1;
            } else {
                break;
            }
        }
        if start_idx >= end {
            return self.span_from_loc(file, &call.loc, member.len());
        }
        let s = text.pos_at_offset(start_idx);
        let e = text.pos_at_offset(end);
        Span::new(s, e)
    }

    fn record_call(
        &mut self,
        file: &str,
        call: &CallExpr,
        callee: Span,
        callee_name: &str,
        receiver: Option<Type>,
        signatures: Vec<Signature>,
    ) {
        let Some(text) = self.text(file) else {
            return;
        };
        let Some(open) = text.find_char(callee.end, '(') else {
            return;
        };
        let Some(close) = text.matching_close(open) else {
            return;
        };
        let arg_spans = split_call_args(text, open, close);
        self.calls.push(CallSite {
            file: file.to_string(),
            call_span: Span::new(callee.start, BytePos::new(close.line, close.col + 1)),
            open,
            close,
            callee,
            callee_name: callee_name.to_string(),
            receiver,
            signatures,
            arg_spans,
        });
        let _ = call;
    }
}

#[derive(Clone, Debug)]
struct MemberRes {
    ty: Option<Type>,
    symbol: Option<SymbolId>,
    signatures: Vec<Signature>,
}

fn select_builtin_member(
    members: &[BuiltinMember],
    name: &str,
    call_arity: Option<usize>,
    want_static: bool,
) -> Option<BuiltinMember> {
    let mut candidates: Vec<&BuiltinMember> = members
        .iter()
        .filter(|m| m.name == name && m.is_static == want_static)
        .collect();
    if candidates.is_empty() {
        return None;
    }
    if let Some(arity) = call_arity {
        let methods: Vec<&BuiltinMember> = candidates
            .iter()
            .copied()
            .filter(|m| m.is_method() && builtin_accepts(m, arity))
            .collect();
        if let Some(found) = methods.first() {
            return Some((*found).clone());
        }
        // 允许把方法的字段视图用于 `arr.length`。
        if let Some(field) = candidates.iter().find(|m| !m.is_method()) {
            return Some((*field).clone());
        }
        return None;
    }
    if let Some(field) = candidates.iter().find(|m| !m.is_method()) {
        return Some((*field).clone());
    }
    candidates.sort_by_key(|m| m.params.len());
    candidates.first().map(|m| (*m).clone())
}

fn builtin_accepts(member: &BuiltinMember, arity: usize) -> bool {
    if member.is_varargs {
        arity + 1 >= member.params.len()
    } else {
        arity == member.params.len()
    }
}

fn split_call_args(text: &TextFile, open: BytePos, close: BytePos) -> Vec<Span> {
    let start = text.offset(open) + 1;
    let end = text.offset(close);
    if start >= end {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut segment_start = start;
    let mut in_string: Option<char> = None;
    let mut escaped = false;
    let mut i = start;
    while i < end {
        let ch = text.text[i..].chars().next().unwrap_or('\0');
        let len = ch.len_utf8();
        if let Some(quote) = in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == quote {
                in_string = None;
            }
            i += len;
            continue;
        }
        match ch {
            '"' | '\'' => in_string = Some(ch),
            '(' | '[' | '{' | '<' => depth += 1,
            ')' | ']' | '}' | '>' => depth -= 1,
            ',' if depth == 0 => {
                push_arg_span(text, segment_start, i, &mut out);
                segment_start = i + len;
            }
            _ => {}
        }
        i += len;
    }
    push_arg_span(text, segment_start, end, &mut out);
    out
}

fn push_arg_span(text: &TextFile, start: usize, end: usize, out: &mut Vec<Span>) {
    if start >= end || end > text.text.len() {
        return;
    }
    let slice = &text.text[start..end];
    let trimmed_start = slice.len() - slice.trim_start().len();
    let trimmed = slice.trim_end();
    if trimmed.trim().is_empty() {
        return;
    }
    let s = start + trimmed_start;
    let e = start + trimmed.len();
    if s >= e {
        return;
    }
    out.push(Span::new(text.pos_at_offset(s), text.pos_at_offset(e)));
}

fn infer_binary_type(op: &BinaryOp, left: Option<&Type>, right: Option<&Type>) -> Type {
    use BinaryOp::*;
    match op {
        Eq | Ne | Lt | Le | Gt | Ge | And | Or => Type::Bool,
        Add => {
            if matches!(left, Some(Type::String)) || matches!(right, Some(Type::String)) {
                Type::String
            } else {
                promote_numeric(left, right)
            }
        }
        _ => promote_numeric(left, right),
    }
}

fn promote_numeric(left: Option<&Type>, right: Option<&Type>) -> Type {
    let is_f64 = matches!(left, Some(Type::Float64)) || matches!(right, Some(Type::Float64));
    if is_f64 {
        return Type::Float64;
    }
    let is_f32 = matches!(left, Some(Type::Float32)) || matches!(right, Some(Type::Float32));
    if is_f32 {
        return Type::Float32;
    }
    let is_i64 = matches!(left, Some(Type::Int64)) || matches!(right, Some(Type::Int64));
    if is_i64 {
        return Type::Int64;
    }
    if matches!(left, Some(Type::Char)) || matches!(right, Some(Type::Char)) {
        return Type::Int32;
    }
    left.cloned()
        .or_else(|| right.cloned())
        .unwrap_or(Type::Int32)
}

fn common_type(a: Option<&Type>, b: Option<&Type>) -> Type {
    match (a, b) {
        (Some(a), Some(b)) if a == b => a.clone(),
        (Some(Type::Object(n)), Some(_)) if n == "null" => b.cloned().unwrap_or(Type::Auto),
        (Some(_), Some(Type::Object(n))) if n == "null" => a.cloned().unwrap_or(Type::Auto),
        (Some(a), Some(b)) => promote_numeric(Some(a), Some(b)),
        (Some(a), None) => a.clone(),
        (None, Some(b)) => b.clone(),
        _ => Type::Auto,
    }
}
