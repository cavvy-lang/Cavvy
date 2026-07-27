//! C 头文件声明提取器（`#include_c` 的兜底路径）
//!
//! 当 `#include_c <header.h>` 找不到对应的 `.cay` 包装时，用本模块把磁盘上的
//! 真实 C 头文件解析成 Cay `extern { ... }` 声明。这是一个**保守**的提取器：
//!
//! - 只产出能干净映射到 Cay FFI 类型集的函数原型；
//! - 无法表示的声明（函数体、static/inline、struct 按值、`long double`、未知值类型、
//!   函数指针 typedef 等）一律**跳过并告警**；
//! - 未知指针类型 / `struct X*` / 函数指针统一映射为 `c_void*`（不透明，FFI 安全）。
//!
//! 不变量：生成的 `extern {}` 始终是合法 Cay——只发干净映射，最坏情况是声明更少，
//! 绝不会产生错误声明。本模块不是完整 C 解析器，不处理 C++、宏函数、结构体布局等。

use crate::miette_diagnostic::{CayError, CayResult, ErrorCodes};
use crate::preprocessor::{parse_preprocessor_number, ConditionParser, LinkLibrary};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// C 头文件提取结果
#[derive(Debug, Clone)]
pub struct CHeaderExtract {
    /// 渲染好的 Cay 文本：`extern [stdcall] {\n    ...\n}\n`（无声明时为空串）
    pub extern_code: String,
    /// 提取器/头名映射推断出的自动链接库
    pub link_libraries: Vec<LinkLibrary>,
    /// 非致命诊断（每个跳过/猜测的声明一条）
    pub warnings: Vec<String>,
}

// ============================================================================
// 数据结构
// ============================================================================

/// 轻量 C token
#[derive(Debug, Clone, PartialEq)]
enum CTok {
    Ident(String),
    Num(String),
    Str(String),
    Punct(&'static str), // 单/双字符标点，来自固定表
    VarArgs,             // ...
}

#[derive(Debug, Clone)]
struct SpanTok {
    tok: CTok,
    line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CallConv {
    Cdecl,
    Stdcall,
}

/// Cay FFI 类型：基类型 + 指针层数
#[derive(Debug, Clone, PartialEq)]
enum Base {
    Void,
    Scalar(String),
}

#[derive(Debug, Clone, PartialEq)]
struct CayType {
    base: Base,
    stars: usize,
}

impl CayType {
    /// 渲染为 Cay 源码。单层 `c_char*` 渲染为 `c_string` 糖；更深层用 `c_char**`。
    fn render(&self) -> String {
        match &self.base {
            Base::Void => {
                if self.stars == 0 {
                    "void".to_string()
                } else {
                    format!("c_void{}", "*".repeat(self.stars))
                }
            }
            Base::Scalar(name) => {
                if self.stars == 0 {
                    name.clone()
                } else if name == "c_char" && self.stars == 1 {
                    "c_string".to_string()
                } else {
                    format!("{}{}", name, "*".repeat(self.stars))
                }
            }
        }
    }

    fn opaque() -> Self {
        CayType {
            base: Base::Void,
            stars: 1,
        }
    }
}

/// 收集系统/环境 include 路径（CPATH/C_INCLUDE_PATH/CPLUS_INCLUDE_PATH + 常见位置）
fn system_include_paths() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    // 用 std::env::split_paths 按平台分隔符拆分（Unix ':'，Windows ';'），自动跳过空段
    for var in ["CPATH", "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH"] {
        if let Some(value) = std::env::var_os(var) {
            paths.extend(std::env::split_paths(&value));
        }
    }
    // 常见系统位置
    let common = vec![
        "/usr/include",
        "/usr/local/include",
        "/usr/include/x86_64-linux-gnu",
    ];
    for c in common {
        let pb = PathBuf::from(c);
        if pb.exists() {
            paths.push(pb);
        }
    }
    paths
}

#[derive(Debug, Clone)]
enum Param {
    Typed(CayType),
    Varargs,
}

#[derive(Debug, Clone)]
struct ProtoFn {
    name: String,
    call_conv: CallConv,
    ret: CayType,
    params: Vec<Param>,
    /// 声明所在头文件行号；保留以供后续按行号精确告警。
    #[allow(dead_code)]
    line: usize,
}

fn make_mangled_name(base: &str, params: &[Param]) -> String {
    let mut parts: Vec<String> = Vec::new();
    for p in params {
        match p {
            Param::Varargs => parts.push("varargs".to_string()),
            Param::Typed(ty) => {
                let mut s = ty.render();
                s = s.replace('*', "p");
                s = s.replace(' ', "_");
                parts.push(s);
            }
        }
    }
    if parts.is_empty() {
        return format!("{}__v", base);
    }
    format!("{}__{}", base, parts.join("__"))
}

/// C 宏表：对象宏 + 函数宏名集合
#[derive(Debug, Clone)]
struct CMacros {
    object: HashMap<String, String>,
    func_like: HashSet<String>,
}

impl CMacros {
    fn new() -> Self {
        CMacros {
            object: HashMap::new(),
            func_like: HashSet::new(),
        }
    }
    fn is_defined(&self, n: &str) -> bool {
        self.object.contains_key(n) || self.func_like.contains(n)
    }
    /// 供 ConditionParser 使用的视图（函数宏名映射为空串以使 defined() 成立）
    fn eval_view(&self) -> HashMap<String, String> {
        let mut m = self.object.clone();
        for k in &self.func_like {
            m.insert(k.clone(), String::new());
        }
        m
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CCond {
    Active,
    Inactive,
    Done,
}

// ============================================================================
// 公共入口
// ============================================================================

/// 读取并解析真实 C 头文件。`search_paths` 当前未被递归使用（Stage B 不递归 include）。
/// 预处理器实际走 `extract_c_header_text`（已自行解析路径以复用规范路径做一次性去重）；
/// 本入口保留为对外公共 API，便于持有已解析路径的调用方直接使用。
#[allow(dead_code)]
pub fn extract_c_header(
    path: &Path,
    _search_paths: &[PathBuf],
    platform_defines: &HashMap<String, String>,
) -> CayResult<CHeaderExtract> {
    let content = std::fs::read_to_string(path).map_err(|e| CayError::Preprocessor {
        error_code: ErrorCodes::PREPROCESSOR_INCLUDE_C_ERROR,
        file: Some(path.to_string_lossy().to_string()),
        line: 1,
        column: 1,
        message: format!("#include_c: 无法读取头文件 '{}': {}", path.display(), e),
        suggestion: "检查文件路径与权限".to_string(),
    })?;
    extract_c_header_text(&path.to_string_lossy(), &content, platform_defines)
}

/// 解析 C 头文件文本（测试与预处理共用）。
pub(crate) fn extract_c_header_text(
    name: &str,
    text: &str,
    platform_defines: &HashMap<String, String>,
) -> CayResult<CHeaderExtract> {
    // Stage A: 注释剥离 + 行续接
    let stripped = strip_comments_and_join(text);
    // Stage B: C 预处理子集（尝试递归 include，基于当前头文件目录）
    let mut macros = seed_c_macros(platform_defines);
    let mut included: HashSet<PathBuf> = HashSet::new();
    let base_dir = Path::new(name).parent().map(|p| p.to_path_buf());
    let include_paths = system_include_paths();
    let (pp_code, mut warnings, skipped_includes) = c_preprocess(
        &stripped,
        &mut macros,
        base_dir.as_deref(),
        Some(include_paths.as_slice()),
        &mut included,
    );
    if !skipped_includes.is_empty() {
        warnings.push(format!(
            "已跳过嵌套 #include（未找到文件）: {}",
            skipped_includes.join(", ")
        ));
    }
    // Stage D: 分词 + 顶层声明提取（含 Stage C 的对象宏展开）
    let toks = tokenize(&pp_code);
    let (protos, decl_warnings, _typedefs) = extract_declarations(&toks, &macros, name);
    warnings.extend(decl_warnings);
    // Stage F: 渲染
    let extern_code = emit(&protos);
    Ok(CHeaderExtract {
        extern_code,
        link_libraries: Vec::new(),
        warnings,
    })
}

/// 头文件基名 → 自动链接库（平台条件）。纯 libc 头返回空（libc 默认已链接）。
pub(crate) fn c_header_link_libs(base: &str) -> Vec<LinkLibrary> {
    let mk = |n: &str| LinkLibrary {
        name: n.to_string(),
        is_system: true,
    };
    // 同时匹配完整 base（如 "sys/mman"）与末段
    let last = base.rsplit('/').next().unwrap_or(base);
    let win = cfg!(target_os = "windows");
    let posix = cfg!(any(target_os = "linux", target_os = "macos"));
    match base {
        "winsock2" | "ws2tcpip" | "mswsock" if win => vec![mk("ws2_32")],
        "windows" if win => vec![mk("user32"), mk("kernel32"), mk("gdi32")],
        "winmm" if win => vec![mk("winmm")],
        "iphlpapi" if win => vec![mk("iphlpapi")],
        "dbghelp" if win => vec![mk("dbghelp")],
        "setupapi" if win => vec![mk("setupapi")],
        "pthread" if posix => vec![mk("pthread")],
        "dlfcn" if cfg!(target_os = "linux") => vec![mk("dl")],
        "mqueue" | "aio" | "timer" | "rt" if cfg!(target_os = "linux") => vec![mk("rt")],
        "uuid" if posix => vec![mk("uuid")],
        _ => match last {
            "winsock2" | "ws2tcpip" | "mswsock" if win => vec![mk("ws2_32")],
            "windows" if win => vec![mk("user32"), mk("kernel32"), mk("gdi32")],
            "winmm" if win => vec![mk("winmm")],
            "iphlpapi" if win => vec![mk("iphlpapi")],
            "dbghelp" if win => vec![mk("dbghelp")],
            "setupapi" if win => vec![mk("setupapi")],
            "pthread" if posix => vec![mk("pthread")],
            "dlfcn" if cfg!(target_os = "linux") => vec![mk("dl")],
            _ => vec![],
        },
    }
}

// ============================================================================
// Stage A: 注释剥离 + 行续接
// ============================================================================

fn strip_comments_and_join(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        // 行续接 \<newline>
        if c == '\\' && i + 1 < chars.len() && chars[i + 1] == '\n' {
            i += 2;
            continue;
        }
        if c == '\\' && i + 2 < chars.len() && chars[i + 1] == '\r' && chars[i + 2] == '\n' {
            i += 3;
            continue;
        }
        // 块注释 /* ... */（保留其中的换行以维持行号）
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '*' {
            i += 2;
            while i < chars.len() {
                if chars[i] == '*' && i + 1 < chars.len() && chars[i + 1] == '/' {
                    i += 2;
                    break;
                }
                if chars[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            continue;
        }
        // 行注释 //
        if c == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            i += 2;
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        // 字符串/字符字面量（不在其中识别注释）
        if c == '"' || c == '\'' {
            let quote = c;
            out.push(c);
            i += 1;
            while i < chars.len() {
                let ch = chars[i];
                out.push(ch);
                if ch == '\\' && i + 1 < chars.len() {
                    out.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                i += 1;
                if ch == quote {
                    break;
                }
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

// ============================================================================
// Stage B: C 预处理子集（不递归 include）
// ============================================================================

fn c_preprocess(
    stripped: &str,
    macros: &mut CMacros,
    base_dir: Option<&Path>,
    include_paths: Option<&[PathBuf]>,
    included: &mut HashSet<PathBuf>,
) -> (String, Vec<String>, Vec<String>) {
    let mut out_lines: Vec<String> = Vec::new();
    let mut warnings = Vec::new();
    let mut skipped_includes = Vec::new();
    let mut stack: Vec<CCond> = Vec::new();
    let mut skipping = false;

    for line in stripped.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('#') {
            // 去掉 '#' 与后续空白
            let content = trimmed[1..].trim_start();
            // 指令名 + 参数
            let mut parts = content.splitn(2, |c: char| c.is_whitespace());
            let dname = parts.next().unwrap_or("").to_string();
            let args = parts.next().unwrap_or("").trim().to_string();

            let is_cond = matches!(
                dname.as_str(),
                "if" | "ifdef" | "ifndef" | "elif" | "else" | "endif"
            );

            if !is_cond && skipping {
                out_lines.push(String::new());
                continue;
            }

            match dname.as_str() {
                "define" => {
                    if !skipping {
                        if let Some((nm, is_func)) = parse_define_target(&args) {
                            let (n, v) = nm;
                            if is_func {
                                macros.func_like.insert(n);
                            } else {
                                macros.object.insert(n, v);
                            }
                        }
                    }
                    out_lines.push(String::new());
                }
                "undef" => {
                    let n = args.split_whitespace().next().unwrap_or("").to_string();
                    if !n.is_empty() {
                        macros.object.remove(&n);
                        macros.func_like.remove(&n);
                    }
                    out_lines.push(String::new());
                }
                "ifdef" => {
                    let cond = macros.is_defined(args.split_whitespace().next().unwrap_or(""));
                    push_cond(&mut stack, cond, &mut skipping);
                    out_lines.push(String::new());
                }
                "ifndef" => {
                    let cond = !macros.is_defined(args.split_whitespace().next().unwrap_or(""));
                    push_cond(&mut stack, cond, &mut skipping);
                    out_lines.push(String::new());
                }
                "if" => {
                    let cond = eval_c_condition(&args, &macros, &mut warnings);
                    push_cond(&mut stack, cond, &mut skipping);
                    out_lines.push(String::new());
                }
                "elif" => {
                    let cond = eval_c_condition(&args, &macros, &mut warnings);
                    handle_elif(&mut stack, cond, &mut skipping);
                    out_lines.push(String::new());
                }
                "else" => {
                    handle_else(&mut stack, &mut skipping);
                    out_lines.push(String::new());
                }
                "endif" => {
                    if stack.pop().is_none() {
                        warnings.push("多余的 #endif".to_string());
                    }
                    skipping = stack.iter().any(|s| *s != CCond::Active);
                    out_lines.push(String::new());
                }
                "include" => {
                    // 解析 include 名称（<...> 或 "...")
                    let inc = args.trim();
                    let inc_inner = if inc.starts_with('<') && inc.ends_with('>') {
                        inc[1..inc.len() - 1].to_string()
                    } else if inc.starts_with('"') && inc.ends_with('"') {
                        inc[1..inc.len() - 1].to_string()
                    } else {
                        inc.to_string()
                    };

                    // 尝试解析为本地文件（基于当前被处理文件的目录 base_dir），否则记录为跳过；
                    // 不提供相对 CWD 的候选路径，避免构建结果依赖工作目录
                    let mut found = false;
                    let mut candidates: Vec<PathBuf> = Vec::new();
                    if let Some(d) = base_dir {
                        candidates.push(d.join(&inc_inner));
                    }
                    if let Some(paths) = include_paths {
                        for inc_dir in paths {
                            candidates.push(inc_dir.join(&inc_inner));
                        }
                    }
                    for cand in candidates {
                        if cand.exists() && cand.is_file() {
                            // 规范路径用于循环检测
                            if let Ok(canon) = cand.canonicalize() {
                                if included.contains(&canon) {
                                    // 已包含，避免递归
                                    warnings.push(format!("跳过已包含的文件: {}", inc_inner));
                                    found = true;
                                    break;
                                }
                                // 读取并递归处理
                                match std::fs::read_to_string(&canon) {
                                    Ok(txt) => {
                                        included.insert(canon.clone());
                                        let sub = strip_comments_and_join(&txt);
                                        let sub_base = canon.parent().map(|p| p.to_path_buf());
                                        let (sub_out, sub_warns, sub_skipped) = c_preprocess(&sub, macros, sub_base.as_deref(), include_paths, included);
                                        warnings.extend(sub_warns);
                                        skipped_includes.extend(sub_skipped);
                                        // 把子文件的行加入当前输出
                                        for l in sub_out.lines() {
                                            out_lines.push(l.to_string());
                                        }
                                        found = true;
                                        break;
                                    }
                                    Err(_) => {
                                        // 不能读取，尝试下一个候选
                                        continue;
                                    }
                                }
                            }
                        }
                    }
                    if !found {
                        skipped_includes.push(args);
                        out_lines.push(String::new());
                    }
                }
                "error" => {
                    if !skipping {
                        warnings.push(format!("头文件 #error: {}", args));
                    }
                    out_lines.push(String::new());
                }
                "warning" => {
                    if !skipping {
                        warnings.push(format!("头文件 #warning: {}", args));
                    }
                    out_lines.push(String::new());
                }
                _ => {
                    // #pragma / #line / 未知指令 → 忽略
                    out_lines.push(String::new());
                }
            }
        } else if skipping {
            out_lines.push(String::new());
        } else {
            out_lines.push(line.to_string());
        }
    }

    if !stack.is_empty() {
        warnings.push("未闭合的 #if/#ifdef".to_string());
    }

    (out_lines.join("\n"), warnings, skipped_includes)
}

/// 解析 #define 目标，返回 ((name, value), is_func)；函数宏 is_func=true（仅记录名）。
fn parse_define_target(args: &str) -> Option<((String, String), bool)> {
    let trimmed = args.trim();
    let mut chars = trimmed.chars().peekable();
    let mut name = String::new();
    while let Some(&c) = chars.peek() {
        if c.is_alphanumeric() || c == '_' {
            name.push(c);
            chars.next();
        } else {
            break;
        }
    }
    if name.is_empty() {
        return None;
    }
    // 紧跟 '('（无空白）→ 函数宏
    if chars.peek() == Some(&'(') {
        return Some(((name, String::new()), true));
    }
    let value: String = chars.collect::<String>().trim().to_string();
    Some(((name, value), false))
}

fn push_cond(stack: &mut Vec<CCond>, cond: bool, skipping: &mut bool) {
    if *skipping {
        stack.push(CCond::Done);
    } else if cond {
        stack.push(CCond::Active);
        *skipping = false;
    } else {
        stack.push(CCond::Inactive);
        *skipping = true;
    }
}

fn handle_else(stack: &mut Vec<CCond>, skipping: &mut bool) {
    match stack.last_mut() {
        Some(CCond::Active) => {
            *stack.last_mut().unwrap() = CCond::Done;
            *skipping = true;
        }
        Some(CCond::Inactive) => {
            *stack.last_mut().unwrap() = CCond::Active;
            *skipping = false;
        }
        Some(CCond::Done) => {
            *skipping = true;
        }
        None => {}
    }
}

fn handle_elif(stack: &mut Vec<CCond>, cond: bool, skipping: &mut bool) {
    match stack.last_mut() {
        Some(CCond::Active) => {
            *stack.last_mut().unwrap() = CCond::Done;
            *skipping = true;
        }
        Some(CCond::Inactive) if cond => {
            *stack.last_mut().unwrap() = CCond::Active;
            *skipping = false;
        }
        _ => {
            *skipping = true;
        }
    }
}

fn eval_c_condition(expr: &str, macros: &CMacros, warnings: &mut Vec<String>) -> bool {
    let trimmed = expr.trim();
    if trimmed.is_empty() {
        return false;
    }
    if macros.is_defined(trimmed) {
        return true;
    }
    if let Ok(n) = parse_preprocessor_number(trimmed) {
        return n != 0;
    }
    let view = macros.eval_view();
    let mut parser = ConditionParser::new(trimmed, &view);
    match parser.parse_expression() {
        Ok(v) => v != 0,
        Err(_) => {
            // 解析失败按 C 预处理语义回退为 false，但记录警告而非静默吞错
            warnings.push(format!("#if 条件表达式 '{}' 解析失败，按 false 处理", trimmed));
            false
        }
    }
}

/// 用平台宏 + 常见 C 编译器宏初始化宏表
fn seed_c_macros(platform_defines: &HashMap<String, String>) -> CMacros {
    let mut m = CMacros::new();
    for (k, v) in platform_defines {
        m.object.insert(k.clone(), v.clone());
    }
    let mut mk = |k: &str, v: &str| m.object.insert(k.to_string(), v.to_string());
    mk("__STDC__", "1");
    mk("__STDC_VERSION__", "199901L");
    mk("__STDC_HOSTED__", "1");
    mk("__SIZEOF_INT__", "4");
    mk("__SIZEOF_SHORT__", "2");
    mk("__SIZEOF_LONG_LONG__", "8");
    mk("__SIZEOF_POINTER__", "8");
    mk("__SIZEOF_SIZE_T__", "8");
    mk("__GNUC__", "4");
    mk("__GNUC_MINOR__", "2");
    mk("__GNUC_PATCHLEVEL__", "1");
    // 派生宏严格依据调用方已声明的平台宏（`platform_defines`），而非本机 `cfg!(target_os)`：
    // 真正的预处理器已在 `self.defines` 中按目标平台设好 _WIN32/__linux__/__APPLE__
    // （见 preprocessor/mod.rs::with_include_paths），单测也据此模拟目标平台。
    // 若两者都不依赖本机编译期 cfg!，跨平台构建与单测行为才能保持一致。
    if platform_defines.contains_key("_WIN32") {
        mk("_WIN64", "");
        mk("__MINGW32__", "");
        mk("__MINGW64__", "");
        mk("__SIZEOF_LONG__", "4");
    } else if platform_defines.contains_key("__linux__") {
        mk("__linux", "");
        mk("__unix__", "");
        mk("__unix", "");
        mk("__ELF__", "");
        mk("__SIZEOF_LONG__", "8");
    } else if platform_defines.contains_key("__APPLE__") {
        mk("__MACH__", "");
        mk("__unix__", "");
        mk("__SIZEOF_LONG__", "8");
    }
    m
}

// ============================================================================
// 分词器
// ============================================================================

fn tokenize(src: &str) -> Vec<SpanTok> {
    let chars: Vec<char> = src.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    let mut line = 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        // 标识符
        if c.is_alphabetic() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            toks.push(SpanTok {
                tok: CTok::Ident(chars[start..i].iter().collect()),
                line,
            });
            continue;
        }
        // 数字
        if c.is_ascii_digit() || (c == '.' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit())
        {
            let start = i;
            while i < chars.len()
                && (chars[i].is_ascii_alphanumeric()
                    || chars[i] == '.'
                    || chars[i] == 'x'
                    || chars[i] == 'X'
                    || chars[i] == '\'')
            {
                i += 1;
            }
            toks.push(SpanTok {
                tok: CTok::Num(chars[start..i].iter().collect()),
                line,
            });
            continue;
        }
        // 字符串/字符字面量
        if c == '"' || c == '\'' {
            let quote = c;
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '\\' && i + 1 < chars.len() {
                    i += 2;
                    continue;
                }
                if chars[i] == quote {
                    i += 1;
                    break;
                }
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            let body: String = chars[start..i].iter().collect();
            toks.push(SpanTok {
                tok: CTok::Str(body),
                line,
            });
            continue;
        }
        // ...
        if c == '.' && i + 2 < chars.len() && chars[i + 1] == '.' && chars[i + 2] == '.' {
            toks.push(SpanTok {
                tok: CTok::VarArgs,
                line,
            });
            i += 3;
            continue;
        }
        // 标点：双字符优先
        if i + 1 < chars.len() {
            let two: String = chars[i..i + 2].iter().collect();
            let p: Option<&'static str> = match two.as_str() {
                "->" => Some("->"),
                "<<" => Some("<<"),
                ">>" => Some(">>"),
                "::" => Some("::"),
                _ => None,
            };
            if let Some(p) = p {
                toks.push(SpanTok {
                    tok: CTok::Punct(p),
                    line,
                });
                i += 2;
                continue;
            }
        }
        let p: &'static str = match c {
            '(' => "(",
            ')' => ")",
            ',' => ",",
            ';' => ";",
            '{' => "{",
            '}' => "}",
            '[' => "[",
            ']' => "]",
            '*' => "*",
            '&' => "&",
            '+' => "+",
            '-' => "-",
            '/' => "/",
            '%' => "%",
            '<' => "<",
            '>' => ">",
            '=' => "=",
            '!' => "!",
            '~' => "~",
            '|' => "|",
            '^' => "^",
            '.' => ".",
            '#' => "#",
            _ => {
                i += 1;
                continue;
            }
        };
        toks.push(SpanTok {
            tok: CTok::Punct(p),
            line,
        });
        i += 1;
    }
    toks
}

// ============================================================================
// Stage D: 顶层声明提取
// ============================================================================

/// C 类型关键字（用于识别参数名 vs 类型）
fn is_type_keyword(s: &str) -> bool {
    matches!(
        s,
        "void" | "char" | "short" | "int" | "long" | "float" | "double"
            | "signed" | "unsigned" | "_Bool" | "bool" | "const" | "volatile"
            | "restrict" | "__restrict" | "__restrict__" | "__const" | "__volatile"
            | "register" | "auto" | "struct" | "union" | "enum" | "_Complex"
            | "_Imaginary" | "size_t" | "ssize_t" | "intptr_t" | "uintptr_t"
            | "int8_t" | "int16_t" | "int32_t" | "int64_t" | "uint8_t" | "uint16_t"
            | "uint32_t" | "uint64_t" | "wchar_t" | "ptrdiff_t"
    )
}

/// 需要从声明中剥离的前缀/属性 token（调用约定单独记录）
fn is_qualifier_or_storage(s: &str) -> bool {
    matches!(
        s,
        "const" | "volatile" | "restrict" | "__restrict" | "__restrict__" | "__const"
            | "__volatile" | "register" | "auto" | "extern" | "static" | "inline"
            | "__inline" | "__inline__" | "__forceinline" | "_CRTIMP" | "__MINGW_IMPORT"
            | "friend"
    )
}

fn is_callconv_stdcall(s: &str) -> bool {
    matches!(
        s,
        "__stdcall" | "WINAPI" | "APIENTRY" | "CALLBACK" | "PASCAL"
    )
}

fn is_callconv_cdecl(s: &str) -> bool {
    matches!(s, "__cdecl" | "_cdecl" | "__cdecl__")
}

/// 跳过 `__attribute__`/`__declspec` 的参数组（含其后的成对括号）。
/// 调用时 i 指向 `__attribute__`/`__declspec` 标识符；返回跳过后新的索引。
fn skip_attribute(toks: &[SpanTok], i: usize) -> usize {
    let mut j = i + 1;
    // 跳过空白已被分词器消除
    if j < toks.len() && toks[j].tok == CTok::Punct("(") {
        let mut depth = 0;
        while j < toks.len() {
            match &toks[j].tok {
                CTok::Punct("(") => depth += 1,
                CTok::Punct(")") => {
                    depth -= 1;
                    if depth == 0 {
                        j += 1;
                        break;
                    }
                }
                _ => {}
            }
            j += 1;
        }
        // GCC 允许 __attribute__((...)) 双括号
        if j < toks.len() && toks[j].tok == CTok::Punct("(") {
            let mut depth = 0;
            while j < toks.len() {
                match &toks[j].tok {
                    CTok::Punct("(") => depth += 1,
                    CTok::Punct(")") => {
                        depth -= 1;
                        if depth == 0 {
                            j += 1;
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
        }
    }
    j
}

fn extract_declarations(
    toks: &[SpanTok],
    macros: &CMacros,
    _name: &str,
) -> (Vec<ProtoFn>, Vec<String>, HashMap<String, CayType>) {
    let mut protos: Vec<ProtoFn> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();
    let mut typedefs: HashMap<String, CayType> = HashMap::new();
    let mut ns_stack: Vec<String> = Vec::new();
    let mut i = 0;
    let n = toks.len();

    while i < n {
        // 顶层命名空间支持：namespace NAME { ... }
        if let CTok::Ident(s) = &toks[i].tok {
            if s == "namespace" && i + 2 < n {
                if let CTok::Ident(ns) = &toks[i + 1].tok {
                    if toks[i + 2].tok == CTok::Punct("{") {
                        let close = find_matching_brace(toks, i + 2).unwrap_or(n - 1);
                        let mut ns_stack: Vec<String> = Vec::new();
                        ns_stack.push(ns.clone());
                        process_range(
                            &toks[i + 3..close.min(n)],
                            macros,
                            &mut protos,
                            &mut warnings,
                            &mut typedefs,
                            &mut ns_stack,
                        );
                        i = close + 1;
                        continue;
                    }
                }
            }
        }
        // extern "C" { ... } / extern "C++" { ... }
        if let CTok::Ident(s) = &toks[i].tok {
            if s == "extern" && i + 2 < n {
                if let CTok::Str(lang) = &toks[i + 1].tok {
                    if toks[i + 2].tok == CTok::Punct("{") {
                        let cpp = lang.contains("C++");
                        let close = find_matching_brace(toks, i + 2);
                        // C / C++ 块均尝试解析内部声明；C++ 解析为 best-effort
                        let inner_start = i + 3;
                        let inner_end = close.unwrap_or(n - 1);
                        if cpp {
                            warnings.push(format!(
                                "解析 extern \"C++\" 块（受限） at line {}",
                                toks[i].line
                            ));
                        }
                        process_range(
                            &toks[inner_start..inner_end.min(n)],
                            macros,
                            &mut protos,
                            &mut warnings,
                            &mut typedefs,
                            &mut ns_stack,
                        );
                        i = close.map(|c| c + 1).unwrap_or(n);
                        continue;
                    }
                }
            }
        }

        // 顶层 { ... } → 定义/结构体定义，跳过
        if toks[i].tok == CTok::Punct("{") {
            let close = find_matching_brace(toks, i);
            if let Some(c) = close {
                // 跳过可选尾随 ;
                let mut nxt = c + 1;
                if nxt < n && toks[nxt].tok == CTok::Punct(";") {
                    nxt += 1;
                }
                warnings.push(format!("跳过定义/结构体定义 at line {}", toks[i].line));
                i = nxt;
                continue;
            } else {
                break;
            }
        }

        // 收集语句片段（到 ; 或 {）
        let (slice, next_i, terminated_by_brace) = collect_statement(toks, i);
        if slice.is_empty() {
            i = next_i;
            continue;
        }
        if terminated_by_brace {
            // 可能是 class/struct/union 的定义：我们希望从类体中提取 `friend` 声明
            if let Some(CTok::Ident(first)) = slice.first().map(|t| &t.tok) {
                if first == "class" || first == "struct" || first == "union" {
                    // next_i 指向 '{'
                    let brace_idx = next_i;
                    if let Some(close) = find_matching_brace(toks, brace_idx) {
                        // 遍历类体内的顶层语句，仅当包含 `friend` 时提取
                        let sub = &toks[brace_idx + 1..close];
                        let mut j = 0;
                        while j < sub.len() {
                            let (inner, inner_next, inner_term) = collect_statement(sub, j);
                            if inner.is_empty() {
                                j = inner_next;
                                continue;
                            }
                            if inner_term {
                                warnings.push(format!("跳过定义（含函数体） at line {}", inner[0].line));
                                j = inner_next;
                                continue;
                            }
                            let has_friend = inner.iter().any(|t| match &t.tok {
                                CTok::Ident(s) if s == "friend" => true,
                                _ => false,
                            });
                            if has_friend {
                                process_statement(&inner, macros, &mut protos, &mut warnings, &mut typedefs, &ns_stack);
                            } else {
                                warnings.push(format!("跳过类成员 at line {}", inner[0].line));
                            }
                            j = inner_next;
                        }
                        i = close + 1;
                        continue;
                    }
                }
            }
            warnings.push(format!("跳过定义（含函数体） at line {}", slice[0].line));
            i = next_i;
            continue;
        }
        // 处理片段
        process_statement(&slice, macros, &mut protos, &mut warnings, &mut typedefs, &ns_stack);
        i = next_i;
    }

    (protos, warnings, typedefs)
}

/// 处理 extern "C" 块内部的一段 token（顶层语义）
fn process_range(
    toks: &[SpanTok],
    macros: &CMacros,
    protos: &mut Vec<ProtoFn>,
    warnings: &mut Vec<String>,
    typedefs: &mut HashMap<String, CayType>,
    ns_stack: &mut Vec<String>,
) {
    let mut i = 0;
    let n = toks.len();
    while i < n {
        // 命名空间处理: namespace NAME { ... }
        if let CTok::Ident(s) = &toks[i].tok {
            if s == "namespace" && i + 2 < n {
                if let CTok::Ident(ns) = &toks[i + 1].tok {
                    if toks[i + 2].tok == CTok::Punct("{") {
                        let close = find_matching_brace(toks, i + 2).unwrap_or(n - 1);
                        ns_stack.push(ns.clone());
                        process_range(
                            &toks[i + 3..close.min(n)],
                            macros,
                            protos,
                            warnings,
                            typedefs,
                            ns_stack,
                        );
                        ns_stack.pop();
                        i = close + 1;
                        continue;
                    }
                }
            }
        }

        if toks[i].tok == CTok::Punct("{") {
            let close = find_matching_brace(toks, i);
            warnings.push(format!("跳过定义/结构体定义 at line {}", toks[i].line));
            i = close.map(|c| c + 1).unwrap_or(n);
            continue;
        }
        let (slice, next_i, terminated_by_brace) = collect_statement(toks, i);
        if slice.is_empty() {
            i = next_i;
            continue;
        }
        if terminated_by_brace {
            warnings.push(format!("跳过定义（含函数体） at line {}", slice[0].line));
            i = next_i;
            continue;
        }
        process_statement(&slice, macros, protos, warnings, typedefs, ns_stack);
        i = next_i;
    }
}

/// 从 i 开始收集语句片段到 `;` 或 `{`（顶层），返回 (片段, 下一个索引, 是否由{终止)
fn collect_statement(toks: &[SpanTok], i: usize) -> (Vec<SpanTok>, usize, bool) {
    let mut slice = Vec::new();
    let mut j = i;
    let n = toks.len();
    while j < n {
        match &toks[j].tok {
            CTok::Punct(";") => return (slice, j + 1, false),
            CTok::Punct("{") => return (slice, j, true),
            CTok::Punct("(") | CTok::Punct("[") => {
                // 收集括号内整体
                let close = find_matching(toks, j);
                if let Some(c) = close {
                    slice.extend_from_slice(&toks[j..=c.min(n - 1)]);
                    j = c + 1;
                } else {
                    slice.push(toks[j].clone());
                    j += 1;
                }
            }
            _ => {
                slice.push(toks[j].clone());
                j += 1;
            }
        }
    }
    (slice, j, false)
}

/// 找到与 `(` / `[` / `{` 配对的闭合括号索引
fn find_matching(toks: &[SpanTok], open: usize) -> Option<usize> {
    let (open_b, close_b): (u8, u8) = match &toks[open].tok {
        CTok::Punct("(") => (b'(', b')'),
        CTok::Punct("[") => (b'[', b']'),
        CTok::Punct("{") => (b'{', b'}'),
        _ => return None,
    };
    let mut depth = 0;
    for k in open..toks.len() {
        if let CTok::Punct(p) = &toks[k].tok {
            if p.len() == 1 {
                let b = p.as_bytes()[0];
                if b == open_b {
                    depth += 1;
                } else if b == close_b {
                    depth -= 1;
                    if depth == 0 {
                        return Some(k);
                    }
                }
            }
        }
    }
    None
}

fn find_matching_brace(toks: &[SpanTok], open: usize) -> Option<usize> {
    let mut depth = 0;
    for k in open..toks.len() {
        match &toks[k].tok {
            CTok::Punct("{") => depth += 1,
            CTok::Punct("}") => {
                depth -= 1;
                if depth == 0 {
                    return Some(k);
                }
            }
            _ => {}
        }
    }
    None
}

/// 处理单条语句片段：typedef / 函数原型 / 变量声明
fn process_statement(
    slice: &[SpanTok],
    macros: &CMacros,
    protos: &mut Vec<ProtoFn>,
    warnings: &mut Vec<String>,
    typedefs: &mut HashMap<String, CayType>,
    ns_stack: &Vec<String>,
) {
    if slice.is_empty() {
        return;
    }
    // 先剥离属性与限定/存储类，并记录调用约定
    let (clean, call_conv, skip_static_inline) = strip_attributes_and_qualifiers(slice);
    if clean.is_empty() {
        return;
    }

    // 对象宏展开（Stage C）
    let expanded = expand_object_macros(&clean, macros);

    // typedef
    if let Some(CTok::Ident(s)) = expanded.first().map(|t| &t.tok) {
        if s == "typedef" {
            process_typedef(&expanded[1..], typedefs, warnings);
            return;
        }
    }

    if skip_static_inline {
        // static/inline 不可链接，跳过
        if let Some(name) = first_ident_after_type(&expanded) {
            warnings.push(format!("跳过 static/inline 函数 '{}'", name));
        }
        return;
    }

    // 函数原型或变量声明
    process_prototype(&expanded, call_conv, protos, warnings, typedefs, ns_stack);
}

/// 剥离属性/限定/存储类，记录调用约定与 static/inline 标记
fn strip_attributes_and_qualifiers(slice: &[SpanTok]) -> (Vec<SpanTok>, CallConv, bool) {
    let mut out = Vec::new();
    let mut call_conv = CallConv::Cdecl;
    let mut skip_static_inline = false;
    let mut i = 0;
    let n = slice.len();
    while i < n {
        if let CTok::Ident(s) = &slice[i].tok {
            if s == "__attribute__" || s == "__declspec" || s == "__attribute" {
                i = skip_attribute(slice, i);
                continue;
            }
            if is_callconv_stdcall(s) {
                call_conv = CallConv::Stdcall;
                i += 1;
                continue;
            }
            if is_callconv_cdecl(s) {
                i += 1;
                continue;
            }
            if s == "__fastcall" || s == "__regparm" {
                // 保守降级为 cdecl
                i += 1;
                continue;
            }
            if is_qualifier_or_storage(s) {
                if matches!(s.as_str(), "static" | "inline" | "__inline" | "__inline__" | "__forceinline") {
                    skip_static_inline = true;
                }
                i += 1;
                continue;
            }
        }
        out.push(slice[i].clone());
        i += 1;
    }
    (out, call_conv, skip_static_inline)
}

/// Stage C: 对象宏展开（函数宏不展开），带循环保护
fn expand_object_macros(slice: &[SpanTok], macros: &CMacros) -> Vec<SpanTok> {
    let mut cur: Vec<SpanTok> = slice.to_vec();
    for _ in 0..16 {
        let mut changed = false;
        let mut next = Vec::with_capacity(cur.len());
        for t in cur.into_iter() {
            if let CTok::Ident(name) = &t.tok {
                if !macros.func_like.contains(name) {
                    if let Some(val) = macros.object.get(name) {
                        let mut sub = tokenize(val);
                        // 保持行号
                        for st in sub.iter_mut() {
                            st.line = t.line;
                        }
                        next.append(&mut sub);
                        changed = true;
                        continue;
                    }
                }
            }
            next.push(t);
        }
        cur = next;
        if !changed {
            break;
        }
    }
    cur
}

fn process_typedef(
    slice: &[SpanTok],
    typedefs: &mut HashMap<String, CayType>,
    warnings: &mut Vec<String>,
) {
    // 函数指针 typedef（含 (*））→ 跳过
    for t in slice {
        if t.tok == CTok::Punct("(") {
            warnings.push("跳过函数指针 typedef".to_string());
            return;
        }
    }
    // 按逗号切分多声明子
    let groups = split_top_commas(slice);
    for g in groups {
        // 函数指针 typedef（含 (*）→ 跳过
        if g.iter().any(|t| t.tok == CTok::Punct("(")) {
            warnings.push("跳过函数指针 typedef".to_string());
            continue;
        }
        // 数组 typedef → 跳过
        if g.iter().any(|t| t.tok == CTok::Punct("[")) {
            warnings.push("跳过数组 typedef".to_string());
            continue;
        }
        // 最后标识符为 typedef 名；其余为底层类型（含 *）
        let mut idents: Vec<(usize, String)> = Vec::new();
        for (idx, t) in g.iter().enumerate() {
            if let CTok::Ident(s) = &t.tok {
                idents.push((idx, s.clone()));
            }
        }
        if idents.is_empty() {
            continue;
        }
        let (name_idx, name) = idents.last().unwrap().clone();
        // base = 除名以外的 token（含 *，由 map_c_type 统计）
        let base_toks: Vec<SpanTok> = g
            .iter()
            .enumerate()
            .filter(|(idx, _)| *idx != name_idx)
            .map(|(_, t)| t.clone())
            .collect();
        match map_c_type(&base_toks, typedefs, false) {
            Some(ty) => {
                typedefs.insert(name, ty);
            }
            None => {
                warnings.push(format!("跳过 typedef '{}'（无法映射底层类型）", name));
            }
        }
    }
}

fn split_top_commas(slice: &[SpanTok]) -> Vec<Vec<SpanTok>> {
    let mut groups = Vec::new();
    let mut cur = Vec::new();
    let mut depth = 0;
    for t in slice {
        match &t.tok {
            CTok::Punct("(") | CTok::Punct("[") | CTok::Punct("{") => {
                depth += 1;
                cur.push(t.clone());
            }
            CTok::Punct(")") | CTok::Punct("]") | CTok::Punct("}") => {
                depth -= 1;
                cur.push(t.clone());
            }
            CTok::Punct(",") if depth == 0 => {
                groups.push(cur.clone());
                cur.clear();
            }
            _ => cur.push(t.clone()),
        }
    }
    if !cur.is_empty() {
        groups.push(cur);
    }
    groups
}

fn first_ident_after_type(slice: &[SpanTok]) -> Option<String> {
    for t in slice {
        if let CTok::Ident(s) = &t.tok {
            if !is_type_keyword(s) && s != "extern" {
                return Some(s.clone());
            }
        }
    }
    None
}

fn process_prototype(
    slice: &[SpanTok],
    call_conv: CallConv,
    protos: &mut Vec<ProtoFn>,
    warnings: &mut Vec<String>,
    typedefs: &HashMap<String, CayType>,
    ns_stack: &Vec<String>,
) {
    // 找第一个顶层 ( ；其前一个标识符为函数名，之前为返回类型
    let mut paren_open: Option<usize> = None;
    let mut depth = 0;
    for (idx, t) in slice.iter().enumerate() {
        match &t.tok {
            CTok::Punct("(") => {
                if depth == 0 {
                    paren_open = Some(idx);
                    break;
                }
                depth += 1;
            }
            CTok::Punct(")") => depth -= 1,
            _ => {}
        }
    }
    let open = match paren_open {
        Some(o) => o,
        None => {
            // 无括号 → 变量声明，跳过
            return;
        }
    };
    if open == 0 {
        return;
    }
    // 函数名 = ( 前的标识符
    let name_idx = open - 1;
    let base_name = match &slice[name_idx].tok {
        CTok::Ident(s) if !is_type_keyword(s) => s.clone(),
        _ => {
            // (*  函数指针变量 → 跳过
            warnings.push("跳过函数指针变量声明".to_string());
            return;
        }
    };
    let name = if ns_stack.is_empty() {
        base_name.clone()
    } else {
        let mut parts = ns_stack.clone();
        parts.push(base_name.clone());
        parts.join("__")
    };
    // ( 内首个 token 为 * → 函数指针变量
    if slice.get(open + 1).map_or(false, |t| t.tok == CTok::Punct("*")) {
        warnings.push(format!("跳过函数指针变量声明 '{}'", name));
        return;
    }
    // 返回类型 = name 之前的 token
    let ret_toks: Vec<SpanTok> = slice[..name_idx].to_vec();
    let ret = match map_c_type(&ret_toks, typedefs, true) {
        Some(r) => r,
        None => {
            warnings.push(format!(
                "跳过 '{}': 无法表示的返回类型 at line {}",
                name,
                slice[name_idx].line
            ));
            return;
        }
    };
    // 参数
    let close = match find_matching(slice, open) {
        Some(c) => c,
        None => return,
    };
    let param_toks = &slice[open + 1..close];
    let params = match parse_params(param_toks, typedefs, warnings, &name) {
        Some(p) => p,
        None => return,
    };
    // close 之后应无残留（属性已剥离）
    let trailing = &slice[close + 1..];
    if !trailing.is_empty() {
        warnings.push(format!("跳过 '{}': 参数列表后有残留 token", name));
        return;
    }
    // 检查是否存在同名（同 fully-qualified 名）函数，若有则启用 mangle
    let full_base = name.clone();
    let mut existing_same_base = Vec::new();
    for (idx, p) in protos.iter().enumerate() {
        if p.name == full_base || p.name.starts_with(&(full_base.clone() + "__")) {
            existing_same_base.push(idx);
        }
    }
    let final_name = if !existing_same_base.is_empty() {
        // 需要 mangle：先把已存在未 mangle 的条目改名
        for &idx in &existing_same_base {
            let existing = &protos[idx];
            // 若已有条目名恰好等于 base，则重命名为 mangled
            if existing.name == full_base {
                let new_name = make_mangled_name(&full_base, &existing.params);
                // 修改原型名
                // NOTE: we mutate in place
                // we need mutable access: create mutable borrow
                // but protos is &mut Vec<ProtoFn>, so we can modify
                protos[idx].name = new_name;
            }
        }
        make_mangled_name(&full_base, &params)
    } else {
        full_base.clone()
    };

    protos.push(ProtoFn {
        name: final_name,
        call_conv,
        ret,
        params,
        line: slice[name_idx].line,
    });
}

fn parse_params(
    toks: &[SpanTok],
    typedefs: &HashMap<String, CayType>,
    warnings: &mut Vec<String>,
    fn_name: &str,
) -> Option<Vec<Param>> {
    // void 单参 → 空
    if toks.len() == 1 {
        if let CTok::Ident(s) = &toks[0].tok {
            if s == "void" {
                return Some(Vec::new());
            }
        }
    }
    // 空 → (...) 变参（C 语义）
    if toks.is_empty() {
        warnings.push(format!(
            "'{}()' 在 C 中表示参数未指定，映射为可变参数 (...)",
            fn_name
        ));
        return Some(vec![Param::Varargs]);
    }
    let groups = split_top_commas(toks);
    let mut params = Vec::new();
    for g in groups {
        // 变参 ...
        if g.len() == 1 && g[0].tok == CTok::VarArgs {
            params.push(Param::Varargs);
            continue;
        }
        // 命名变参：TYPE ... name 形式罕见，仅当末 token 为 ... 时按变参处理
        if g.iter().any(|t| t.tok == CTok::VarArgs) {
            params.push(Param::Varargs);
            continue;
        }
        // 函数指针参数（含 (*）→ 不透明
        if g.iter().any(|t| t.tok == CTok::Punct("(")) {
            params.push(Param::Typed(CayType::opaque()));
            continue;
        }
        let stripped = strip_param_name(&g, typedefs);
        match map_c_type(&stripped, typedefs, false) {
            Some(ty) => params.push(Param::Typed(ty)),
            None => {
                warnings.push(format!(
                    "跳过 '{}': 无法表示的参数类型 at line {}",
                    fn_name,
                    g.first().map(|t| t.line).unwrap_or(0)
                ));
                return None;
            }
        }
    }
    Some(params)
}

/// 剥离参数名：若末标识符不是类型关键字/typedef/struct-tag，且前面有其他标识符，则去掉。
fn strip_param_name(g: &[SpanTok], typedefs: &HashMap<String, CayType>) -> Vec<SpanTok> {
    let idents: Vec<usize> = g
        .iter()
        .enumerate()
        .filter_map(|(i, t)| match &t.tok {
            CTok::Ident(_) => Some(i),
            _ => None,
        })
        .collect();
    if idents.len() < 2 {
        return g.to_vec();
    }
    let last = *idents.last().unwrap();
    let last_name = match &g[last].tok {
        CTok::Ident(s) => s.clone(),
        _ => return g.to_vec(),
    };
    if is_type_keyword(&last_name) || typedefs.contains_key(&last_name) {
        return g.to_vec();
    }
    // 前一个 token 是否为 struct/union/enum
    if last > 0 {
        if let CTok::Ident(prev) = &g[last - 1].tok {
            if prev == "struct" || prev == "union" || prev == "enum" {
                return g.to_vec();
            }
        }
    }
    // 去掉末标识符
    let mut out = g.to_vec();
    out.remove(last);
    out
}

// ============================================================================
// Stage E: C → Cay FFI 类型映射
// ============================================================================

fn map_c_type(
    spec: &[SpanTok],
    typedefs: &HashMap<String, CayType>,
    is_return: bool,
) -> Option<CayType> {
    if spec.is_empty() {
        return None;
    }
    // 函数指针模式（含 (*）→ 不透明指针
    if spec.iter().any(|t| t.tok == CTok::Punct("(")) {
        return Some(CayType::opaque());
    }
    // 统计 * 与数组维度，收集标识符（剥除限定符 const/volatile 等）
    let mut stars = 0;
    let mut arr = 0;
    let mut idents: Vec<String> = Vec::new();
    for t in spec {
        match &t.tok {
            CTok::Punct("*") => stars += 1,
            CTok::Punct("[") => arr += 1,
            CTok::Ident(s) => {
                if !is_c_qualifier(s) {
                    idents.push(s.clone());
                }
            }
            _ => {}
        }
    }
    if arr > 0 && is_return {
        return None; // 返回数组 → 跳过
    }
    let total_stars = stars + arr; // 参数数组退化为指针

    // 单 typedef 名 → 复用其已记录 CayType（叠加声明中的指针层）
    if idents.len() == 1 {
        if let Some(ty) = typedefs.get(&idents[0]) {
            return Some(CayType {
                base: ty.base.clone(),
                stars: ty.stars + total_stars,
            });
        }
    }

    match map_base(&idents) {
        Some(base) => Some(CayType {
            base,
            stars: total_stars,
        }),
        None => {
            // 未知/不可表示的基类型：指针 → 不透明，值 → None
            if total_stars >= 1 {
                Some(CayType::opaque())
            } else {
                None
            }
        }
    }
}

/// C 限定符（在类型映射时剥除）
fn is_c_qualifier(s: &str) -> bool {
    matches!(
        s,
        "const" | "volatile" | "restrict" | "__restrict" | "__restrict__" | "__const"
            | "__volatile" | "register" | "auto"
    )
}

fn map_base(idents: &[String]) -> Option<Base> {
    if idents.is_empty() {
        return None;
    }
    // typedef 由 map_c_type 的单标识符分支处理；此处只处理基类型关键字组合
    let joined = idents.join(" ");
    match joined.as_str() {
        "void" => Some(Base::Void),
        "int" | "signed" | "signed int" => Some(Base::Scalar("c_int".to_string())),
        "unsigned" | "unsigned int" => Some(Base::Scalar("c_uint".to_string())),
        "short" | "short int" | "signed short" | "signed short int" => {
            Some(Base::Scalar("c_short".to_string()))
        }
        "unsigned short" | "unsigned short int" => Some(Base::Scalar("c_ushort".to_string())),
        "long" | "long int" | "signed long" | "signed long int" => {
            Some(Base::Scalar("c_long".to_string()))
        }
        "unsigned long" | "unsigned long int" => Some(Base::Scalar("c_ulong".to_string())),
        "long long" | "long long int" | "signed long long" | "signed long long int" => {
            Some(Base::Scalar("c_int64_t".to_string()))
        }
        "unsigned long long" | "unsigned long long int" => {
            Some(Base::Scalar("c_uint64_t".to_string()))
        }
        "char" | "signed char" => Some(Base::Scalar("c_char".to_string())),
        "unsigned char" => Some(Base::Scalar("c_uchar".to_string())),
        "float" => Some(Base::Scalar("c_float".to_string())),
        "double" => Some(Base::Scalar("c_double".to_string())),
        "_Bool" | "bool" => Some(Base::Scalar("c_bool".to_string())),
        "size_t" => Some(Base::Scalar("size_t".to_string())),
        "ssize_t" => Some(Base::Scalar("ssize_t".to_string())),
        "intptr_t" => Some(Base::Scalar("intptr_t".to_string())),
        "uintptr_t" => Some(Base::Scalar("uintptr_t".to_string())),
        "ptrdiff_t" => Some(Base::Scalar("intptr_t".to_string())),
        "wchar_t" => Some(Base::Scalar("c_int".to_string())),
        "int8_t" => Some(Base::Scalar("c_char".to_string())),
        "int16_t" => Some(Base::Scalar("c_short".to_string())),
        "int32_t" => Some(Base::Scalar("c_int".to_string())),
        "int64_t" => Some(Base::Scalar("c_int64_t".to_string())),
        "uint8_t" => Some(Base::Scalar("c_uchar".to_string())),
        "uint16_t" => Some(Base::Scalar("c_ushort".to_string())),
        "uint32_t" => Some(Base::Scalar("c_uint".to_string())),
        "uint64_t" => Some(Base::Scalar("c_uint64_t".to_string())),
        _ => {
            let first = idents.first().map(|s| s.as_str()).unwrap_or("");
            match first {
                "enum" => Some(Base::Scalar("c_int".to_string())),
                "struct" | "union" => None,        // 按值不可表示
                "_Complex" | "_Imaginary" => None, // 不支持
                _ => {
                    // long double（含 long 与 double）→ 不可表示
                    if idents.iter().any(|s| s == "long") && idents.iter().any(|s| s == "double") {
                        None
                    } else {
                        None // 未知
                    }
                }
            }
        }
    }
}

// ============================================================================
// Stage F: 渲染
// ============================================================================

/// Cay 关键字/内置函数冲突集：命中则发 `as <name>_c` 别名
const C_NAME_COLLISIONS: &[&str] = &[
    "exit",
    "print",
    "println",
    "eprint",
    "eprintln",
    "readInt",
    "readLong",
    "readFloat",
    "readDouble",
    "readLine",
    "readChar",
    "readBool",
    "new",
    "this",
    "super",
    "class",
    "struct",
    "enum",
    "return",
    "break",
    "continue",
    "if",
    "else",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "default",
    "var",
    "let",
    "auto",
    "extern",
    "alias",
    "fn",
    "function",
    "scope",
    "extends",
    "implements",
    "interface",
    "instanceof",
    "using",
    "namespace",
    "as",
    "abs",
    "labs",
];

fn emit(protos: &[ProtoFn]) -> String {
    if protos.is_empty() {
        return String::new();
    }
    // 按 cdecl / stdcall 分块（Cay 每块调用约定一致）
    let cdecl: Vec<&ProtoFn> = protos.iter().filter(|p| p.call_conv == CallConv::Cdecl).collect();
    let stdcall: Vec<&ProtoFn> = protos
        .iter()
        .filter(|p| p.call_conv == CallConv::Stdcall)
        .collect();
    let mut out = String::new();
    if !cdecl.is_empty() {
        out.push_str("extern {\n");
        for p in &cdecl {
            out.push_str("    ");
            out.push_str(&render_proto(p));
            out.push('\n');
        }
        out.push_str("}\n");
    }
    if !stdcall.is_empty() {
        out.push_str("extern stdcall {\n");
        for p in &stdcall {
            out.push_str("    ");
            out.push_str(&render_proto(p));
            out.push('\n');
        }
        out.push_str("}\n");
    }
    out
}

fn render_proto(p: &ProtoFn) -> String {
    let mut s = String::new();
    s.push_str(&p.ret.render());
    s.push(' ');
    s.push_str(&p.name);
    s.push('(');
    let params: Vec<String> = p
        .params
        .iter()
        .map(|par| match par {
            Param::Typed(ty) => ty.render(),
            Param::Varargs => "...".to_string(),
        })
        .collect();
    s.push_str(&params.join(", "));
    s.push(')');
    if C_NAME_COLLISIONS.contains(&p.name.as_str()) {
        s.push_str(&format!(" as {}_c", p.name));
    }
    s.push(';');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extract(text: &str) -> CHeaderExtract {
        let defines = HashMap::new();
        extract_c_header_text("test.h", text, &defines).unwrap()
    }

    fn extract_with(text: &str, defines: &[&str]) -> CHeaderExtract {
        let mut m = HashMap::new();
        for d in defines {
            m.insert(d.to_string(), String::new());
        }
        extract_c_header_text("test.h", text, &m).unwrap()
    }

    #[test]
    fn test_basic_prototype() {
        let r = extract("int add(int a, int b);");
        assert!(r.extern_code.contains("c_int add(c_int, c_int);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_varargs() {
        let r = extract("int printf(const char *fmt, ...);");
        assert!(r.extern_code.contains("c_int printf(c_string, ...);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_typedef_substitution() {
        let r = extract("typedef long myoff;\nmyoff lseek(int, myoff, int);");
        assert!(r.extern_code.contains("c_long lseek(c_int, c_long, c_int);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_pointer_typedef() {
        let r = extract("typedef int *intptr2;\nvoid f(intptr2 p);");
        assert!(r.extern_code.contains("void f(c_int*);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_opaque_struct_pointer() {
        let r = extract("struct _IO_FILE;\nint fclose(struct _IO_FILE *fp);");
        assert!(r.extern_code.contains("c_int fclose(c_void*);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_skip_function_definition() {
        let r = extract("int foo(void) { return 0; }\nint bar(void);");
        assert!(!r.extern_code.contains("foo"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int bar();"), "got: {}", r.extern_code);
        assert!(r.warnings.iter().any(|w| w.contains("定义")), "warnings: {:?}", r.warnings);
    }

    #[test]
    fn test_skip_static_inline() {
        let r = extract("static inline int sq(int x) { return x*x; }\nint visible(void);");
        assert!(!r.extern_code.contains("sq"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int visible();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_ifdef_selection() {
        let r = extract_with(
            "#ifdef _WIN32\nint wfn(void);\n#else\nint lfn(void);\n#endif",
            &["_WIN32"],
        );
        assert!(r.extern_code.contains("wfn"), "got: {}", r.extern_code);
        assert!(!r.extern_code.contains("lfn"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_ifdef_else_branch() {
        let r = extract_with(
            "#ifdef _WIN32\nint wfn(void);\n#else\nint lfn(void);\n#endif",
            &[],
        );
        assert!(r.extern_code.contains("lfn"), "got: {}", r.extern_code);
        assert!(!r.extern_code.contains("wfn"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_stdcall_windows() {
        let r = extract_with("int __stdcall Foo(void);", &["_WIN32"]);
        assert!(r.extern_code.contains("extern stdcall {"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int Foo();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_winapi_macro_expands_to_stdcall() {
        let r = extract_with("#define WINAPI __stdcall\nint WINAPI Foo(void);", &["_WIN32"]);
        assert!(r.extern_code.contains("extern stdcall {"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_collision_alias_exit() {
        let r = extract("int exit(int code);");
        assert!(r.extern_code.contains("as exit_c"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_no_alias_atoi() {
        let r = extract("int atoi(const char *s);");
        assert!(r.extern_code.contains("c_int atoi(c_string);"), "got: {}", r.extern_code);
        assert!(!r.extern_code.contains("as "), "got: {}", r.extern_code);
    }

    #[test]
    fn test_void_param_empty() {
        let r = extract("int getchar(void);");
        assert!(r.extern_code.contains("c_int getchar();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_bare_paren_varargs() {
        let r = extract("int fclose();");
        assert!(r.extern_code.contains("c_int fclose(...);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_char_star_sugar() {
        let r = extract("char *a(const char *s);");
        assert!(r.extern_code.contains("c_string a(c_string);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_unsigned_char_star() {
        let r = extract("unsigned char *b(unsigned char *p);");
        assert!(r.extern_code.contains("c_uchar* b(c_uchar*);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_extern_c_unwrap() {
        let r = extract("extern \"C\" {\nint x(void);\nint y(void);\n}");
        assert!(r.extern_code.contains("c_int x();"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int y();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_extern_cpp_dropped() {
        let r = extract("extern \"C++\" {\nint cpponly(void);\n}\nint cfn(void);");
        assert!(r.extern_code.contains("cpponly"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int cfn();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_namespace_extraction() {
        let r = extract("namespace ns { int f(void); } int g(void);");
        assert!(r.extern_code.contains("ns__f"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int g();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_class_friend_extraction() {
        let r = extract("class C { friend int foo(void); int mem(void); }; int bar(void);");
        assert!(r.extern_code.contains("foo"), "got: {}", r.extern_code);
        assert!(!r.extern_code.contains("mem("), "member should be skipped: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int bar();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_overload_mangling() {
        let r = extract("int f(int); double f(double); int g(void);");
        // both overloads should be present and have mangled names
        assert!(r.extern_code.contains("f__c_int"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("f__c_double"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int g();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_int64_mapping() {
        let r = extract("int64_t f(uint64_t x);");
        assert!(r.extern_code.contains("c_int64_t f(c_uint64_t);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_unknown_value_type_skips() {
        let r = extract("struct Foo bar(void);");
        assert!(!r.extern_code.contains("bar"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_include_skipped() {
        let r = extract("#include <stdio.h>\nint mine(void);");
        assert!(r.extern_code.contains("c_int mine();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_array_decay() {
        let r = extract("void f(int a[4], char b[]);");
        assert!(r.extern_code.contains("void f(c_int*, c_string);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_link_libs_winsock() {
        let libs = c_header_link_libs("winsock2");
        if cfg!(target_os = "windows") {
            assert!(libs.iter().any(|l| l.name == "ws2_32"));
        } else {
            assert!(libs.is_empty());
        }
    }

    #[test]
    fn test_link_libs_stdio_empty() {
        let libs = c_header_link_libs("stdio");
        assert!(libs.is_empty(), "stdio should not need extra lib");
    }
}
