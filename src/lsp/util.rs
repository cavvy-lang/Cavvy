//! LSP 文本、位置与源码扫描工具。
//!
//! LSP 的 `Position.character` 以 UTF-16 代码单元计数，而编译器内部的
//! `SourceLocation.column` 是 1-based 的 **UTF-8 字节**列号。本模块集中处理
//! 两者之间的转换，并提供括号匹配、声明名定位、文档注释提取等编辑器常用操作。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::miette_diagnostic::SourceLocation;
use tower_lsp::lsp_types::{Position, Range, Url};

/// 0-based 行号 + 0-based UTF-8 字节列号。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BytePos {
    pub line: u32,
    pub col: u32,
}

impl BytePos {
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }

    pub fn from_source_location(loc: &SourceLocation) -> Self {
        Self {
            line: loc.line.saturating_sub(1) as u32,
            col: loc.column.saturating_sub(1) as u32,
        }
    }
}

/// 0-based 行号 + 0-based UTF-8 字节列号的范围（左闭右开）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Span {
    pub start: BytePos,
    pub end: BytePos,
}

impl Span {
    pub fn new(start: BytePos, end: BytePos) -> Self {
        Self { start, end }
    }

    pub fn point(pos: BytePos) -> Self {
        Self {
            start: pos,
            end: pos,
        }
    }

    pub fn contains(&self, pos: BytePos) -> bool {
        (self.start..self.end).contains(&pos) || (self.start == pos && self.start == self.end)
    }

    /// 判断两个范围是否重叠。
    pub fn intersects(&self, other: &Span) -> bool {
        self.start < other.end && other.start < self.end
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// 带文件路径的位置范围。
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LocatedSpan {
    pub file: String,
    pub span: Span,
}

impl LocatedSpan {
    pub fn new(file: impl Into<String>, span: Span) -> Self {
        Self {
            file: file.into(),
            span,
        }
    }

    pub fn to_lsp_range(&self, files: &std::collections::HashMap<String, Arc<TextFile>>) -> Range {
        files
            .get(&self.file)
            .map(|f| f.range(self.span))
            .unwrap_or_else(|| Range::new(Position::new(0, 0), Position::new(0, 0)))
    }
}

/// 一个源文件的文本与行索引。
#[derive(Debug)]
pub struct TextFile {
    pub path: String,
    pub text: Arc<String>,
    line_starts: Vec<usize>,
}

impl TextFile {
    pub fn new(path: impl Into<String>, text: impl Into<String>) -> Self {
        let text = Arc::new(text.into());
        let mut line_starts = vec![0usize];
        for (idx, b) in text.bytes().enumerate() {
            if b == b'\n' {
                line_starts.push(idx + 1);
            }
        }
        Self {
            path: path.into(),
            text,
            line_starts,
        }
    }

    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// 返回不含行尾 `\r\n`/`\n` 的行文本。
    pub fn line(&self, line: u32) -> &str {
        let idx = line as usize;
        if idx >= self.line_starts.len() {
            return "";
        }
        let start = self.line_starts[idx];
        let end = self
            .line_starts
            .get(idx + 1)
            .copied()
            .unwrap_or(self.text.len());
        let mut s = &self.text[start..end];
        if let Some(stripped) = s.strip_suffix('\n') {
            s = stripped;
        }
        if let Some(stripped) = s.strip_suffix('\r') {
            s = stripped;
        }
        s
    }

    pub fn line_start(&self, line: u32) -> usize {
        self.line_starts
            .get(line as usize)
            .copied()
            .unwrap_or(self.text.len())
    }

    pub fn line_len_bytes(&self, line: u32) -> u32 {
        self.line(line).len() as u32
    }

    pub fn line_len_utf16(&self, line: u32) -> u32 {
        self.line(line).encode_utf16().count() as u32
    }

    /// 字节列 -> UTF-16 列。
    pub fn byte_col_to_utf16(&self, line: u32, byte_col: u32) -> u32 {
        let text = self.line(line);
        let mut b = (byte_col as usize).min(text.len());
        while b > 0 && !text.is_char_boundary(b) {
            b -= 1;
        }
        text[..b].encode_utf16().count() as u32
    }

    /// UTF-16 列 -> 字节列。
    pub fn utf16_to_byte_col(&self, line: u32, utf16_col: u32) -> u32 {
        let text = self.line(line);
        let mut units = 0u32;
        for (byte_idx, ch) in text.char_indices() {
            if units >= utf16_col {
                return byte_idx as u32;
            }
            units += ch.len_utf16() as u32;
            if units > utf16_col {
                // 光标落在 surrogate pair 中间：取该字符起点。
                return byte_idx as u32;
            }
        }
        text.len() as u32
    }

    /// 字节偏移（相对整个文件）-> BytePos。
    pub fn pos_at_offset(&self, offset: usize) -> BytePos {
        let offset = floor_char_boundary(&self.text, offset.min(self.text.len()));
        let line = match self.line_starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts[line];
        BytePos::new(line as u32, (offset - line_start) as u32)
    }

    /// BytePos -> 整个文件中的字节偏移。
    pub fn offset(&self, pos: BytePos) -> usize {
        let start = self.line_start(pos.line);
        let line = self.line(pos.line);
        start + floor_char_boundary(line, (pos.col as usize).min(line.len()))
    }

    pub fn position(&self, pos: BytePos) -> Position {
        Position::new(pos.line, self.byte_col_to_utf16(pos.line, pos.col))
    }

    pub fn range(&self, span: Span) -> Range {
        Range::new(self.position(span.start), self.position(span.end))
    }

    pub fn location(&self, span: Span) -> tower_lsp::lsp_types::Location {
        tower_lsp::lsp_types::Location {
            uri: Url::from_file_path(&self.path)
                .unwrap_or_else(|_| Url::parse("file:///").expect("file:/// 是有效 URL")),
            range: self.range(span),
        }
    }

    /// 提取指定 UTF-16 位置上的单词（标识符字符）。
    pub fn word_at(&self, pos: BytePos) -> Option<(Span, String)> {
        let line = self.line(pos.line);
        let byte_col = (pos.col as usize).min(line.len());
        if line.is_empty() {
            return None;
        }
        let byte_col = floor_char_boundary(line, byte_col);
        let mut start = byte_col;
        while start > 0 {
            let prev = prev_char_boundary(line, start);
            let ch = line[prev..start].chars().next()?;
            if !is_word_char(ch) {
                break;
            }
            start = prev;
        }
        let mut end = byte_col;
        while end < line.len() {
            let ch = line[end..].chars().next()?;
            if !is_word_char(ch) {
                break;
            }
            end += ch.len_utf8();
        }
        if start == end {
            return None;
        }
        Some((
            Span::new(
                BytePos::new(pos.line, start as u32),
                BytePos::new(pos.line, end as u32),
            ),
            line[start..end].to_string(),
        ))
    }

    /// 从 `from` 开始查找字符 `needle`（不处理字符串/注释）。
    pub fn find_char(&self, from: BytePos, needle: char) -> Option<BytePos> {
        let start = self.offset(from);
        for (idx, ch) in self.text[start..].char_indices() {
            if ch == needle {
                return Some(self.pos_at_offset(start + idx));
            }
        }
        None
    }

    /// 从 `from` 开始查找字符 `needle`，在遇到换行前停止。
    pub fn find_char_on_line(&self, from: BytePos, needle: char) -> Option<BytePos> {
        let line = self.line(from.line);
        let start = floor_char_boundary(line, (from.col as usize).min(line.len()));
        for (idx, ch) in line[start..].char_indices() {
            if ch == needle {
                return Some(BytePos::new(from.line, (start + idx) as u32));
            }
        }
        None
    }

    /// 从开括号位置起找到匹配的闭括号（支持 (), [], {}，跳过字符串/字符/注释）。
    pub fn matching_close(&self, open: BytePos) -> Option<BytePos> {
        let open_ch = self.char_at(open)?;
        let close_ch = match open_ch {
            '(' => ')',
            '[' => ']',
            '{' => '}',
            '<' => '>',
            _ => return None,
        };
        let mut depth = 0usize;
        let start = self.offset(open);
        let mut chars = self.text[start..].char_indices().peekable();
        while let Some((idx, ch)) = chars.next() {
            match ch {
                '\n' => {
                    continue;
                }
                '/' if matches!(chars.peek(), Some((_, '/'))) => {
                    // 行注释
                    for (_, c) in chars.by_ref() {
                        if c == '\n' {
                            break;
                        }
                    }
                    continue;
                }
                '/' if matches!(chars.peek(), Some((_, '*'))) => {
                    chars.next();
                    let mut prev = '\0';
                    for (_, c) in chars.by_ref() {
                        if prev == '*' && c == '/' {
                            break;
                        }
                        prev = c;
                    }
                    continue;
                }
                '"' | '\'' => {
                    let quote = ch;
                    let mut escaped = false;
                    for (_, c) in chars.by_ref() {
                        if escaped {
                            escaped = false;
                            continue;
                        }
                        if c == '\\' {
                            escaped = true;
                        } else if c == quote {
                            break;
                        }
                    }
                    continue;
                }
                _ => {}
            }
            if ch == open_ch {
                depth += 1;
            } else if ch == close_ch {
                depth -= 1;
                if depth == 0 {
                    let pos = self.pos_at_offset(start + idx);
                    return Some(pos);
                }
            }
        }
        None
    }

    pub fn char_at(&self, pos: BytePos) -> Option<char> {
        let line = self.line(pos.line);
        let col = floor_char_boundary(line, (pos.col as usize).min(line.len()));
        if !line.is_char_boundary(col) {
            return None;
        }
        line[col..].chars().next()
    }

    /// 提取紧邻声明行之前的文档注释。
    pub fn doc_comment_before(&self, line: u32) -> String {
        if line == 0 {
            return String::new();
        }
        let mut collected: Vec<String> = Vec::new();
        let mut idx = line as i64 - 1;
        // 先跳过空行：文档注释和声明之间允许空行（IDE 常见习惯）。
        let mut saw_comment = false;
        while idx >= 0 {
            let text = self.line(idx as u32).trim();
            if text.is_empty() {
                if saw_comment {
                    break;
                }
                idx -= 1;
                continue;
            }
            if text.starts_with("///") || text.starts_with("//!") {
                saw_comment = true;
                collected.push(
                    text.trim_start_matches("///")
                        .trim_start_matches("//!")
                        .trim()
                        .to_string(),
                );
                idx -= 1;
                continue;
            }
            if text.starts_with("//") {
                saw_comment = true;
                collected.push(text.trim_start_matches("//").trim().to_string());
                idx -= 1;
                continue;
            }
            if text.ends_with("*/") || text.starts_with('*') || text.starts_with("/*") {
                saw_comment = true;
                collected.push(
                    text.trim_start_matches("/**")
                        .trim_start_matches("/*")
                        .trim_start_matches('*')
                        .trim_end_matches("*/")
                        .trim()
                        .to_string(),
                );
                idx -= 1;
                continue;
            }
            break;
        }
        collected.reverse();
        collected
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// 从位置开始读取一个标识符（用于 `class Foo` / `new Foo`）。
    pub fn identifier_at(&self, pos: BytePos) -> Option<(Span, String)> {
        let line = self.line(pos.line);
        let mut start = floor_char_boundary(line, (pos.col as usize).min(line.len()));
        while start < line.len() {
            let ch = line[start..].chars().next()?;
            if !ch.is_whitespace() && !is_word_char(ch) && ch != '@' {
                return None;
            }
            if is_word_char(ch) {
                break;
            }
            start += ch.len_utf8();
        }
        if start >= line.len() {
            return None;
        }
        let mut end = start;
        while end < line.len() {
            let ch = line[end..].chars().next()?;
            if !is_word_char(ch) {
                break;
            }
            end += ch.len_utf8();
        }
        if start == end {
            return None;
        }
        Some((
            Span::new(
                BytePos::new(pos.line, start as u32),
                BytePos::new(pos.line, end as u32),
            ),
            line[start..end].to_string(),
        ))
    }

    /// 在 `from` 之后查找关键字 `keyword` 后面的标识符。
    pub fn name_after_keyword(&self, from: BytePos, keyword: &str) -> Option<(Span, String)> {
        let start = self.offset(from);
        let text = &self.text[start..];
        let mut search_from = 0usize;
        while let Some(rel) = text[search_from..].find(keyword) {
            let abs = search_from + rel;
            // 关键字需要是完整单词。
            let before_ok = abs == 0
                || text[..abs]
                    .chars()
                    .next_back()
                    .map(|c| !is_word_char(c))
                    .unwrap_or(true);
            let after = abs + keyword.len();
            let after_ok = text[after..]
                .chars()
                .next()
                .map(|c| !is_word_char(c))
                .unwrap_or(true);
            if before_ok && after_ok {
                let pos = self.pos_at_offset(start + after);
                if let Some(found) = self.identifier_at(pos) {
                    return Some(found);
                }
            }
            search_from = abs + keyword.len();
            if search_from >= text.len() {
                break;
            }
        }
        None
    }

    /// 在 `from` 到行尾（或 `stop` 字符）之间查找 `name` 的完整单词。
    pub fn find_word_after(&self, from: BytePos, name: &str, stop_at: &[char]) -> Option<Span> {
        let line = self.line(from.line);
        let mut start = floor_char_boundary(line, (from.col as usize).min(line.len()));
        while start < line.len() {
            let ch = line[start..].chars().next()?;
            if stop_at.contains(&ch) {
                return None;
            }
            if is_word_char(ch) {
                let mut end = start;
                while end < line.len() {
                    let c = line[end..].chars().next()?;
                    if !is_word_char(c) {
                        break;
                    }
                    end += c.len_utf8();
                }
                if &line[start..end] == name {
                    return Some(Span::new(
                        BytePos::new(from.line, start as u32),
                        BytePos::new(from.line, end as u32),
                    ));
                }
                start = end;
                continue;
            }
            start += ch.len_utf8();
        }
        None
    }

    /// 查找声明名称：从 `from` 开始，取 `stop_at` 之前最后一次出现的 `name`。
    pub fn find_last_word_before(
        &self,
        from: BytePos,
        name: &str,
        stop_at: &[char],
    ) -> Option<Span> {
        let line = self.line(from.line);
        let mut start = floor_char_boundary(line, (from.col as usize).min(line.len()));
        let mut last = None;
        while start < line.len() {
            let ch = line[start..].chars().next()?;
            if stop_at.contains(&ch) {
                break;
            }
            if is_word_char(ch) {
                let mut end = start;
                while end < line.len() {
                    let c = line[end..].chars().next()?;
                    if !is_word_char(c) {
                        break;
                    }
                    end += c.len_utf8();
                }
                if &line[start..end] == name {
                    last = Some(Span::new(
                        BytePos::new(from.line, start as u32),
                        BytePos::new(from.line, end as u32),
                    ));
                }
                start = end;
                continue;
            }
            start += ch.len_utf8();
        }
        last
    }

    /// 向前查找接收者表达式的起始位置（用于 `foo.bar.` 的 `bar` / `foo()` 等）。
    pub fn receiver_start_before(&self, dot: BytePos) -> Option<BytePos> {
        let offset = self.offset(dot);
        if offset == 0 {
            return None;
        }
        let mut idx = offset;
        // 跳过空白
        loop {
            if idx == 0 {
                return None;
            }
            let prev = prev_char_boundary(&self.text, idx);
            let ch = self.text[prev..idx].chars().next()?;
            if ch.is_whitespace() {
                idx = prev;
                continue;
            }
            break;
        }
        // 跳过类型实参 `>`：简单回退到匹配的 `<` 之前，再递归。
        let prev = prev_char_boundary(&self.text, idx);
        let ch = self.text[prev..idx].chars().next()?;
        match ch {
            ')' | ']' => {
                let open = self.matching_open(idx)?;
                let open_pos = self.pos_at_offset(open);
                if ch == ')' {
                    // 调用：接收者是 `(` 之前的表达式。
                    self.receiver_start_before(open_pos)
                } else {
                    // 索引：接收者是 `[` 之前的表达式。
                    self.receiver_start_before(open_pos)
                }
            }
            '>' => {
                // 可能是 `foo<T>(...)` 或 `foo<T>.`；向前找 `<`，再取它前面的表达式。
                let mut depth = 1i32;
                let mut cur = prev;
                while cur > 0 {
                    let p = prev_char_boundary(&self.text, cur);
                    let c = self.text[p..cur].chars().next()?;
                    if c == '>' {
                        depth += 1;
                    } else if c == '<' {
                        depth -= 1;
                        if depth == 0 {
                            return self.receiver_start_before(self.pos_at_offset(p));
                        }
                    }
                    cur = p;
                }
                None
            }
            _ if is_word_char(ch) => {
                let mut start = prev;
                loop {
                    if start == 0 {
                        break;
                    }
                    let p = prev_char_boundary(&self.text, start);
                    let c = self.text[p..start].chars().next()?;
                    if !is_word_char(c) {
                        break;
                    }
                    start = p;
                }
                // 处理链式 `a.b` 或命名空间 `a::b`：继续向前。
                let before = self.text[..start].trim_end();
                if let Some(prefix) = before.strip_suffix("new") {
                    if prefix
                        .chars()
                        .next_back()
                        .map(|c| !is_word_char(c))
                        .unwrap_or(true)
                    {
                        return Some(self.pos_at_offset(prefix.len()));
                    }
                }
                if before.ends_with('.') || before.ends_with("::") {
                    let dot_pos = self.pos_at_offset(before.len().saturating_sub(1));
                    return self.receiver_start_before(dot_pos);
                }
                Some(self.pos_at_offset(start))
            }
            '"' | '\'' => {
                // 字符串/字符字面量：向前找未转义的引号。
                let quote = ch;
                let mut cur = prev;
                while cur > 0 {
                    let p = prev_char_boundary(&self.text, cur);
                    let c = self.text[p..cur].chars().next()?;
                    if c == quote {
                        // 检查是否被转义。
                        let mut backslashes = 0usize;
                        let mut q = p;
                        while q > 0 {
                            let pp = prev_char_boundary(&self.text, q);
                            if self.text[pp..q].chars().next() == Some('\\') {
                                backslashes += 1;
                                q = pp;
                            } else {
                                break;
                            }
                        }
                        if backslashes % 2 == 0 {
                            return Some(self.pos_at_offset(p));
                        }
                    }
                    cur = p;
                }
                None
            }
            _ if ch.is_ascii_digit() => {
                let mut start = prev;
                while start > 0 {
                    let p = prev_char_boundary(&self.text, start);
                    let c = self.text[p..start].chars().next()?;
                    if c.is_ascii_alphanumeric() || c == '.' || c == '_' {
                        start = p;
                    } else {
                        break;
                    }
                }
                Some(self.pos_at_offset(start))
            }
            _ => None,
        }
    }

    /// 从闭括号之后的字节偏移找到匹配的开括号字节偏移。
    fn matching_open(&self, close_end_offset: usize) -> Option<usize> {
        let close_end_offset = floor_char_boundary(&self.text, close_end_offset);
        let ch = self.text[..close_end_offset].chars().next_back()?;
        let open_ch = match ch {
            ')' => '(',
            ']' => '[',
            '}' => '{',
            _ => return None,
        };
        let mut depth = 0usize;
        let mut idx = close_end_offset;
        while idx > 0 {
            let prev = prev_char_boundary(&self.text, idx);
            let c = self.text[prev..idx].chars().next()?;
            if c == ch {
                depth += 1;
            } else if c == open_ch {
                depth -= 1;
                if depth == 0 {
                    return Some(prev);
                }
            }
            idx = prev;
        }
        None
    }
}

fn floor_char_boundary(s: &str, mut idx: usize) -> usize {
    if idx >= s.len() {
        return s.len();
    }
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    idx
}

fn prev_char_boundary(s: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    let mut i = idx - 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '$'
}

/// 规范化文件路径，用于跨平台比较。不要求文件存在。
pub fn normalize_path(path: &Path) -> String {
    let mut out = path.to_path_buf();
    // 去掉 `.` 与 `..`（不解析符号链接）。
    let mut parts: Vec<std::ffi::OsString> = Vec::new();
    for comp in out.components() {
        use std::path::Component;
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                if !parts.is_empty() {
                    parts.pop();
                }
            }
            other => parts.push(other.as_os_str().to_os_string()),
        }
    }
    out = parts.iter().collect();
    out.to_string_lossy().replace('/', "\\")
}

/// 由文件路径构造 `file://` URI。
pub fn path_to_uri(path: impl AsRef<Path>) -> Url {
    Url::from_file_path(path.as_ref())
        .unwrap_or_else(|_| Url::parse("file:///").expect("file:/// 是有效 URL"))
}

/// 由 URI 得到文件系统路径；无法转换时返回 URI 字符串。
pub fn uri_to_path(uri: &Url) -> String {
    uri.to_file_path()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| uri.path().to_string())
}

/// 生成 `file://` URI 的字符串形式。
pub fn uri_string(uri: &Url) -> String {
    uri.as_str().to_string()
}

/// 在目录中查找可用的 `caylibs` 目录候选。
pub fn caylibs_candidates(workspace_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut push = |p: PathBuf| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    for root in workspace_roots {
        push(root.join("caylibs"));
        // 某些项目把 caylibs 放在工作区上一级。
        if let Some(parent) = root.parent() {
            push(parent.join("caylibs"));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        push(cwd.join("caylibs"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            push(dir.join("caylibs"));
        }
    }
    push(PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/caylibs"
    )));
    out.retain(|p| p.is_dir());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_roundtrip_with_cjk_and_emoji() {
        let f = TextFile::new("t.cay", "int 变量 = 1; // 😀\nString s = \"你好\";");
        assert_eq!(f.byte_col_to_utf16(0, 4), 4);
        assert_eq!(f.byte_col_to_utf16(0, 7), 5);
        assert_eq!(f.utf16_to_byte_col(0, 5), 7);
        // 行 0 的 😀 在 UTF-16 中占两个单元。
        let utf16_len = f.line(0).encode_utf16().count() as u32;
        assert_eq!(f.byte_col_to_utf16(0, f.line(0).len() as u32), utf16_len);
        assert_eq!(f.utf16_to_byte_col(0, utf16_len), f.line(0).len() as u32);
    }

    #[test]
    fn word_at_handles_cjk_identifier() {
        let f = TextFile::new("t.cay", "int 变量名 = 1;");
        let (span, word) = f.word_at(BytePos::new(0, 5)).expect("应找到中文标识符");
        assert_eq!(word, "变量名");
        assert_eq!(span.start.col, 4);
        assert_eq!(span.end.col, 13);
    }

    #[test]
    fn matching_close_skips_strings_and_comments() {
        let f = TextFile::new("t.cay", "foo(\"a)\", /* ) */ bar(1))");
        let open = BytePos::new(0, 3);
        let close = f.matching_close(open).expect("应匹配右括号");
        assert_eq!(close.col, 24);
    }

    #[test]
    fn doc_comment_before_stops_at_code() {
        let f = TextFile::new("t.cay", "/// 第一行\n/// 第二行\npublic class A {\n}\n");
        assert_eq!(f.doc_comment_before(2), "第一行\n第二行");
    }

    #[test]
    fn receiver_start_handles_chains() {
        let f = TextFile::new("t.cay", "foo.bar.baz.");
        let dot = BytePos::new(0, 11);
        let start = f.receiver_start_before(dot).expect("应找到接收者");
        assert_eq!(start.col, 0);
    }

    #[test]
    fn receiver_start_handles_new_expression() {
        let f = TextFile::new("t.cay", "new Foo().");
        let dot = BytePos::new(0, 9);
        let start = f.receiver_start_before(dot).expect("应找到 new 表达式");
        assert_eq!(start.col, 0);
    }
}
