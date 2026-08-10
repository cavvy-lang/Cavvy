//! C/C++ 头文件声明提取器（`#include_c` 的兜底路径）
//!
//! 当 `#include_c <header.h>` 找不到对应的 `.cay` 包装时，用本模块把磁盘上的
//! 真实头文件解析成 Cay `extern { ... }` 声明。这是一个**保守**的提取器：
//!
//! - 只产出能干净映射到 Cay FFI 类型集的函数原型；
//! - 无法表示的声明（函数体、static/inline、struct/class 按值、`long double`、
//!   未知值类型、函数指针 typedef 等）一律**跳过并告警**；
//! - 未知指针类型 / `struct X*` / class 指针/引用 / 函数指针统一映射为
//!   `c_void*`（不透明，FFI 安全）。
//!
//! C++ 支持（无模板头文件，扩展名 `.hpp/.hh/.hxx` 或出现 `class`/`template`/
//! `namespace`/`extern "C++"` 时进入 C++ 模式）：
//!
//! - class/struct 提取为 Cay `namespace ns { interop class Name { ... } }`：
//!   数据成员按声明顺序镜像为等尺寸 Cay 字段（布局与 C++ 一致），构造/析构/
//!   成员函数/静态成员函数渲染为 `native` 声明，由 Cay 编译器按 Itanium ABI
//!   （g++/clang/MinGW，不支持 MSVC）mangle 链接名；对象用 `new` 创建，
//!   离开作用域自动析构（RAII）；
//! - 含虚函数的类在字段最前补 `c_void* __cpp_vptr;` 并告警（Cay 侧为直接
//!   调用，不支持虚分派语义）；含基类/位域/按值类成员/模板类型成员/匿名
//!   union/未识别类型成员的类布局不完整，不生成构造/析构（`new` 自然被封死，
//!   需经 C++ 工厂函数创建对象）；union、嵌套类、静态数据成员跳过并告警；
//! - 运算符重载无法对应 Cay 方法语法，维持 `<Class>__operator_<op>` 自由
//!   函数别名形式（首参 `c_void*` 为 this）；仅 const 区分的方法重载对跳过
//!   const 版本并告警；
//! - C++ 模式下的顶层自由函数同样按 Itanium mangle；`extern "C"` 块内保持
//!   C 链接，`extern "C++"` 块内强制 C++ 链接；
//! - `template <...>` 声明与模板实参类型（如 `std::vector<int>`）无法离线
//!   实例化，跳过/降级并告警。
//!
//! 不变量：生成的 `extern {}` 始终是合法 Cay——只发干净映射，最坏情况是声明更少，
//! 绝不会产生错误声明。本模块不是完整 C/C++ 解析器，不处理模板实例化、宏函数、
//! 结构体/类布局等。

use crate::miette_diagnostic::{CayError, CayResult, ErrorCodes};
use crate::preprocessor::cpp_mangle::{mangle_function, MethodName};
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
    /// 已知 C++ 类名（`Foo*`/`Foo&` 参数/返回映射为 Cay 对象类型，传引用语义）
    Object(String),
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
            Base::Object(name) => {
                // C++ 对象引用语义：渲染为类名本身（Cay 侧传指针）
                name.clone()
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

#[derive(Debug, Clone, PartialEq)]
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
    /// C++ 模式的 Itanium 链接名；None = 原名 C 链接。
    /// 有值时渲染为 `<ret> <link_name>(params) as <name>;`（name 作 Cay 别名）。
    link_name: Option<String>,
    /// 非静态 C++ 成员函数：params[0] 为注入的 this（别名后缀生成时跳过）
    has_this: bool,
    /// 声明所在头文件行号；保留以供后续按行号精确告警。
    #[allow(dead_code)]
    line: usize,
}

/// C++ 模式收集的类成员方法（渲染为 interop class 内的 native 声明）
#[derive(Debug, Clone)]
struct InteropMethod {
    name: String,
    ret: CayType,
    params: Vec<Param>,
    is_static: bool,
    is_const: bool,
}

/// C++ 模式收集的类：渲染为 `namespace ... { interop class Name { ... } }`
#[derive(Debug, Clone)]
struct InteropClass {
    /// 外层命名空间路径（空 = 全局作用域）
    ns: Vec<String>,
    name: String,
    /// 镜像字段（声明顺序）：(Cay 类型文本, 字段名)
    fields: Vec<(String, String)>,
    /// 含虚函数（含纯虚）：渲染时字段最前补 `c_void* __cpp_vptr;`
    has_virtual: bool,
    /// 布局不完整原因（含基类/位域/按值类成员/模板类型成员/匿名 union/
    /// 未识别类型成员/union 整体）；Some 时不生成构造/析构（`new` 被封死）
    layout_incomplete: Option<String>,
    /// 构造函数参数列表集合（重载）
    ctors: Vec<Vec<Param>>,
    has_dtor: bool,
    methods: Vec<InteropMethod>,
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

/// 解析 C/C++ 头文件文本（测试与预处理共用）。
pub(crate) fn extract_c_header_text(
    name: &str,
    text: &str,
    platform_defines: &HashMap<String, String>,
) -> CayResult<CHeaderExtract> {
    // C++ 模式判定（一）：扩展名 .hpp/.hh/.hxx（大小写不敏感）。
    // 扩展名判为 C++ 时预先定义 __cplusplus，使头文件中的 C++ 条件块可见。
    let ext_cpp = is_cpp_header_name(name);
    // Stage A: 注释剥离 + 行续接
    let stripped = strip_comments_and_join(text);
    // Stage B: C 预处理子集（尝试递归 include，基于当前头文件目录）
    let mut macros = seed_c_macros(platform_defines);
    if ext_cpp {
        macros.object.insert("__cplusplus".to_string(), "201703L".to_string());
    }
    let mut included: HashSet<PathBuf> = HashSet::new();
    let mut pragma_once: HashSet<PathBuf> = HashSet::new();
    let base_dir = Path::new(name).parent().map(|p| p.to_path_buf());
    let include_paths = system_include_paths();
    let (pp_code, mut warnings, skipped_includes) = c_preprocess(
        &stripped,
        &mut macros,
        base_dir.as_deref(),
        Some(include_paths.as_slice()),
        &mut included,
        Some(Path::new(name)),
        &mut pragma_once,
    );
    if !skipped_includes.is_empty() {
        warnings.push(format!(
            "已跳过嵌套 #include（未找到文件）: {}",
            skipped_includes.join(", ")
        ));
    }
    // Stage D: 分词 + 顶层声明提取（含 Stage C 的对象宏展开）
    let toks = tokenize(&pp_code);
    // C++ 模式判定（二）：token 流预扫描命中 template/class/namespace/extern "C++"
    let cpp_mode = ext_cpp || prescan_cpp_tokens(&toks);
    // 预收集 class/struct 名：`Foo*`/`Foo&` 参数映射为 Cay 对象类型时需要
    let known_classes = if cpp_mode {
        prescan_class_names(&toks)
    } else {
        HashSet::new()
    };
    let (protos, classes, decl_warnings, _typedefs) =
        extract_declarations(&toks, &macros, name, cpp_mode, &known_classes);
    warnings.extend(decl_warnings);
    // Stage F: 渲染
    let extern_code = emit(&protos, &classes);
    Ok(CHeaderExtract {
        extern_code,
        link_libraries: Vec::new(),
        warnings,
    })
}

/// 扩展名是否表明这是 C++ 头文件（.hpp/.hh/.hxx，大小写不敏感）
fn is_cpp_header_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".hpp") || lower.ends_with(".hh") || lower.ends_with(".hxx")
}

/// token 流预扫描：命中 C++ 关键字/extern "C++" 则整个头按 C++ 模式处理
fn prescan_cpp_tokens(toks: &[SpanTok]) -> bool {
    for (i, t) in toks.iter().enumerate() {
        if let CTok::Ident(s) = &t.tok {
            match s.as_str() {
                "template" | "class" | "namespace" => return true,
                "extern" => {
                    if let Some(CTok::Str(lang)) = toks.get(i + 1).map(|t| &t.tok) {
                        if lang.contains("C++") {
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    false
}

/// 预扫描收集 class/struct 名（不含 union）：用于把 `Foo*`/`Foo&` 参数/返回
/// 映射为 Cay 对象类型。前向引用（先使用后定义）也能命中。
fn prescan_class_names(toks: &[SpanTok]) -> HashSet<String> {
    let mut set = HashSet::new();
    for w in toks.windows(2) {
        if let (CTok::Ident(a), CTok::Ident(b)) = (&w[0].tok, &w[1].tok) {
            if a == "class" || a == "struct" {
                set.insert(b.clone());
            }
        }
    }
    set
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
    current_path: Option<&Path>,
    pragma_once: &mut HashSet<PathBuf>,
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
                                if pragma_once.contains(&canon) {
                                    // 文件带 #pragma once 且已处理过
                                    warnings.push(format!(
                                        "跳过 #pragma once 文件: {}",
                                        inc_inner
                                    ));
                                    found = true;
                                    break;
                                }
                                // 读取并递归处理
                                match std::fs::read_to_string(&canon) {
                                    Ok(txt) => {
                                        included.insert(canon.clone());
                                        let sub = strip_comments_and_join(&txt);
                                        let sub_base = canon.parent().map(|p| p.to_path_buf());
                                        let (sub_out, sub_warns, sub_skipped) = c_preprocess(
                                            &sub,
                                            macros,
                                            sub_base.as_deref(),
                                            include_paths,
                                            included,
                                            Some(canon.as_path()),
                                            pragma_once,
                                        );
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
                "pragma" => {
                    // #pragma once：登记当前文件规范路径，重复包含时跳过
                    if !skipping && args.split_whitespace().next() == Some("once") {
                        if let Some(p) = current_path {
                            if let Ok(canon) = p.canonicalize() {
                                pragma_once.insert(canon);
                            }
                        }
                    }
                    // 其余 #pragma 忽略
                    out_lines.push(String::new());
                }
                _ => {
                    // #line / 未知指令 → 忽略
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
    cpp_mode: bool,
    known_classes: &HashSet<String>,
) -> (
    Vec<ProtoFn>,
    Vec<InteropClass>,
    Vec<String>,
    HashMap<String, CayType>,
) {
    let mut protos: Vec<ProtoFn> = Vec::new();
    let mut classes: Vec<InteropClass> = Vec::new();
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
                            &mut classes,
                            &mut warnings,
                            &mut typedefs,
                            &mut ns_stack,
                            cpp_mode,
                            known_classes,
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
                        // extern "C" 块内强制 C 链接；extern "C++" 块内强制 C++ 链接
                        let block_cpp = lang.contains("C++");
                        let close = find_matching_brace(toks, i + 2);
                        let inner_start = i + 3;
                        let inner_end = close.unwrap_or(n - 1);
                        process_range(
                            &toks[inner_start..inner_end.min(n)],
                            macros,
                            &mut protos,
                            &mut classes,
                            &mut warnings,
                            &mut typedefs,
                            &mut ns_stack,
                            block_cpp,
                            known_classes,
                        );
                        i = close.map(|c| c + 1).unwrap_or(n);
                        continue;
                    }
                }
            }
        }

        // 模板声明：跳过并告警（C++ 模式）
        if cpp_mode {
            if let Some(next) = skip_template_decl(toks, i, &mut warnings) {
                i = next;
                continue;
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
            if !cpp_mode {
                // C 路径：从类体中仅提取 `friend` 声明（行为保持不变）
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
                                    process_statement(&inner, macros, &mut protos, &mut warnings, &mut typedefs, &ns_stack, false);
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
            } else {
                // C++ 路径：class/struct/union 定义 → 收集为 interop class
                if let Some((name, has_base, is_union)) = parse_class_head(&slice) {
                    let brace_idx = next_i;
                    if let Some(close) = find_matching_brace(toks, brace_idx) {
                        let sub = &toks[brace_idx + 1..close];
                        process_class_body_cpp(
                            sub,
                            &ns_stack,
                            &name,
                            has_base,
                            is_union,
                            macros,
                            &mut protos,
                            &mut classes,
                            &mut warnings,
                            &mut typedefs,
                            known_classes,
                        );
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
        process_statement(&slice, macros, &mut protos, &mut warnings, &mut typedefs, &ns_stack, cpp_mode);
        i = next_i;
    }

    (protos, classes, warnings, typedefs)
}

/// 处理 extern "C"/命名空间块内部的一段 token（顶层语义）
fn process_range(
    toks: &[SpanTok],
    macros: &CMacros,
    protos: &mut Vec<ProtoFn>,
    classes: &mut Vec<InteropClass>,
    warnings: &mut Vec<String>,
    typedefs: &mut HashMap<String, CayType>,
    ns_stack: &mut Vec<String>,
    cpp_mode: bool,
    known_classes: &HashSet<String>,
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
                            classes,
                            warnings,
                            typedefs,
                            ns_stack,
                            cpp_mode,
                            known_classes,
                        );
                        ns_stack.pop();
                        i = close + 1;
                        continue;
                    }
                }
            }
        }

        // 块内嵌套 extern "C" / extern "C++"
        if let CTok::Ident(s) = &toks[i].tok {
            if s == "extern" && i + 2 < n {
                if let CTok::Str(lang) = &toks[i + 1].tok {
                    if toks[i + 2].tok == CTok::Punct("{") {
                        let block_cpp = lang.contains("C++");
                        let close = find_matching_brace(toks, i + 2).unwrap_or(n - 1);
                        process_range(
                            &toks[i + 3..close.min(n)],
                            macros,
                            protos,
                            classes,
                            warnings,
                            typedefs,
                            ns_stack,
                            block_cpp,
                            known_classes,
                        );
                        i = close + 1;
                        continue;
                    }
                }
            }
        }

        // 模板声明：跳过并告警（C++ 模式）
        if cpp_mode {
            if let Some(next) = skip_template_decl(toks, i, warnings) {
                i = next;
                continue;
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
            // C++ 模式：块内的 class/struct 定义同样解析成员函数
            if cpp_mode {
                if let Some((name, has_base, is_union)) = parse_class_head(&slice) {
                    let brace_idx = next_i;
                    if let Some(close) = find_matching_brace(toks, brace_idx) {
                        let sub = &toks[brace_idx + 1..close];
                        process_class_body_cpp(
                            sub,
                            ns_stack,
                            &name,
                            has_base,
                            is_union,
                            macros,
                            protos,
                            classes,
                            warnings,
                            typedefs,
                            known_classes,
                        );
                        i = close + 1;
                        continue;
                    }
                }
            }
            warnings.push(format!("跳过定义（含函数体） at line {}", slice[0].line));
            i = next_i;
            continue;
        }
        process_statement(&slice, macros, protos, warnings, typedefs, ns_stack, cpp_mode);
        i = next_i;
    }
}

/// 检测 `template <...>` 前缀并跳过其后的整条模板声明（到 `;` 或 `{...}` 函数体）。
/// 命中时 push 告警并返回跳过后的索引；未命中返回 None。
fn skip_template_decl(toks: &[SpanTok], i: usize, warnings: &mut Vec<String>) -> Option<usize> {
    let n = toks.len();
    if i + 1 >= n {
        return None;
    }
    match &toks[i].tok {
        CTok::Ident(s) if s == "template" => {}
        _ => return None,
    }
    if toks[i + 1].tok != CTok::Punct("<") {
        return None;
    }
    // 配对 < ... >（">>" 计为两层闭合）
    let mut depth = 0i32;
    let mut close_gt: Option<usize> = None;
    for k in i + 1..n {
        match &toks[k].tok {
            CTok::Punct("<") => depth += 1,
            CTok::Punct(">") => {
                depth -= 1;
                if depth == 0 {
                    close_gt = Some(k);
                    break;
                }
            }
            CTok::Punct(">>") => {
                depth -= 2;
                if depth <= 0 {
                    close_gt = Some(k);
                    break;
                }
            }
            _ => {}
        }
    }
    let gt = close_gt?;
    // 模板名：class/struct 模板取关键字后的名字；函数模板取首个 ( 前的标识符；
    // 都取不到就退化为 "template"
    let mut name: Option<String> = None;
    let mut k = gt + 1;
    while k < n {
        match &toks[k].tok {
            CTok::Ident(s) if s == "class" || s == "struct" || s == "union" || s == "enum" => {
                if let Some(CTok::Ident(n2)) = toks.get(k + 1).map(|t| &t.tok) {
                    name = Some(n2.clone());
                }
                break;
            }
            CTok::Punct("(") => {
                if k > gt + 1 {
                    if let CTok::Ident(s) = &toks[k - 1].tok {
                        name = Some(s.clone());
                    }
                }
                break;
            }
            CTok::Punct(";") | CTok::Punct("{") => break,
            _ => {}
        }
        k += 1;
    }
    // 退化：<...> 后第一个标识符
    if name.is_none() {
        for k in gt + 1..n {
            if let CTok::Ident(s) = &toks[k].tok {
                name = Some(s.clone());
                break;
            }
            if toks[k].tok == CTok::Punct(";") || toks[k].tok == CTok::Punct("{") {
                break;
            }
        }
    }
    let name = name.unwrap_or_else(|| "template".to_string());
    // 跳过 <...> 后的声明：到 ; 或由 {...} 结束的函数体
    let (_slice, mut next, term) = collect_statement(toks, gt + 1);
    if term {
        // next 指向 '{'，跳过函数体与可选尾随 ';'
        if let Some(c) = find_matching_brace(toks, next) {
            next = c + 1;
            if next < n && toks[next].tok == CTok::Punct(";") {
                next += 1;
            }
        }
    }
    warnings.push(format!(
        "C++ 模板 '{}' 需要 C++ 编译器实例化展开后才能使用，已跳过 at line {}",
        name, toks[i].line
    ));
    Some(next)
}

// ============================================================================
// C++ class/struct 成员解析（仅 C++ 模式）
// ============================================================================

/// 解析类/结构定义头（`{` 前的 token 片段）。
/// 返回 (类名, 是否含基类列表, 是否 union)；非类定义头返回 None。
fn parse_class_head(slice: &[SpanTok]) -> Option<(String, bool, bool)> {
    let mut i = 0;
    while i < slice.len() {
        if let CTok::Ident(s) = &slice[i].tok {
            match s.as_str() {
                kw @ ("class" | "struct" | "union") => {
                    let mut j = i + 1;
                    // 跳过类名前的属性/宏（__attribute__((...))、alignas(...) 等）
                    while j < slice.len() {
                        if let CTok::Ident(a) = &slice[j].tok {
                            if a == "__attribute__" || a == "__declspec" || a == "__attribute" {
                                j = skip_attribute(slice, j);
                                continue;
                            }
                            if a == "alignas" && j + 1 < slice.len()
                                && slice[j + 1].tok == CTok::Punct("(")
                            {
                                if let Some(c) = find_matching(slice, j + 1) {
                                    j = c + 1;
                                    continue;
                                }
                            }
                            if a == "final" {
                                j += 1;
                                continue;
                            }
                        }
                        break;
                    }
                    let name = match slice.get(j).map(|t| &t.tok) {
                        Some(CTok::Ident(n)) => n.clone(),
                        _ => return None,
                    };
                    // 类名后还有其它标识符（基类列表；':' 已被分词器丢弃）→ 有继承
                    let has_base = slice[j + 1..]
                        .iter()
                        .any(|t| matches!(t.tok, CTok::Ident(_)));
                    return Some((name, has_base, kw == "union"));
                }
                "__attribute__" | "__declspec" | "__attribute" => {
                    i = skip_attribute(slice, i);
                    continue;
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

/// 成员运算符符号 → (Itanium 代码, Cay 别名后缀) 白名单
fn cpp_op_mapping(symbols: &[&str]) -> Option<(&'static str, &'static str)> {
    Some(match symbols {
        ["+"] => ("pl", "plus"),
        ["-"] => ("mi", "minus"),
        ["*"] => ("ml", "mul"),
        ["/"] => ("dv", "div"),
        ["=", "="] => ("eq", "eq"),
        ["!", "="] => ("ne", "ne"),
        ["<"] => ("lt", "lt"),
        ["<", "="] => ("le", "le"),
        [">"] => ("gt", "gt"),
        [">", "="] => ("ge", "ge"),
        ["="] => ("aS", "assign"),
        ["[", "]"] => ("ix", "index"),
        ["(", ")"] => ("cl", "call"),
        ["<<"] => ("ls", "shl"),
        [">>"] => ("rs", "shr"),
        ["+", "+"] => ("pp", "inc"),
        ["-", "-"] => ("mm", "dec"),
        _ => return None,
    })
}

/// C++ 模式：解析 class/struct 定义体，收集为 InteropClass（渲染为 Cay
/// `interop class`：字段镜像 + native 构造/析构/方法）。运算符与 friend
/// 维持自由函数别名形式（protos）。`ns` 为外层命名空间路径。
#[allow(clippy::too_many_arguments)]
fn process_class_body_cpp(
    sub: &[SpanTok],
    ns: &[String],
    class_name: &str,
    has_base: bool,
    is_union: bool,
    macros: &CMacros,
    protos: &mut Vec<ProtoFn>,
    classes: &mut Vec<InteropClass>,
    warnings: &mut Vec<String>,
    typedefs: &mut HashMap<String, CayType>,
    known_classes: &HashSet<String>,
) {
    let mut cls = InteropClass {
        ns: ns.to_vec(),
        name: class_name.to_string(),
        fields: Vec::new(),
        has_virtual: false,
        layout_incomplete: if is_union {
            Some("union 布局不可表示".to_string())
        } else if has_base {
            Some("含基类".to_string())
        } else {
            None
        },
        ctors: Vec::new(),
        has_dtor: false,
        methods: Vec::new(),
    };
    let class_line = sub.first().map(|t| t.line).unwrap_or(0);
    let mut warned_static_member = false;
    let mut j = 0;
    while j < sub.len() {
        // 模板成员：跳过并告警
        if let Some(next) = skip_template_decl(sub, j, warnings) {
            j = next;
            continue;
        }
        let (inner, inner_next, inner_term) = collect_statement(sub, j);
        if inner.is_empty() {
            j = inner_next;
            continue;
        }
        if inner_term {
            // inner_next 指向 '{'：嵌套类型定义、匿名 union 或内联函数体
            let brace_idx = inner_next;
            let close = find_matching_brace(sub, brace_idx).unwrap_or(sub.len() - 1);
            let mut after = close + 1;
            // 匿名 union 成员（union { ... } name;）：布局不可表示
            let head = strip_access_labels(&inner);
            let is_anon_union = head.first().map_or(false, |t| matches!(&t.tok, CTok::Ident(s) if s == "union"))
                && parse_class_head(head).is_none();
            if is_anon_union {
                // 匿名 union 的声明名在 } 之后（`} field_name;`），一并跳过
                while after < sub.len() && sub[after].tok != CTok::Punct(";") {
                    after += 1;
                }
                if after < sub.len() {
                    after += 1;
                }
            } else if after < sub.len() && sub[after].tok == CTok::Punct(";") {
                // 内联函数体/嵌套类型后的可选尾随 ;
                after += 1;
            }
            if is_anon_union {
                if cls.layout_incomplete.is_none() {
                    cls.layout_incomplete = Some("含匿名 union 成员".to_string());
                }
            } else if let Some((nested_name, _, _)) = parse_class_head(head) {
                // 嵌套类：Cay 无嵌套类语法，跳过
                warnings.push(format!(
                    "跳过嵌套类 '{}'（Cay 不支持嵌套类） at line {}",
                    nested_name, inner[0].line
                ));
            } else if head.first().map_or(false, |t| matches!(&t.tok, CTok::Ident(s) if s == "enum")) {
                warnings.push(format!("跳过嵌套 enum 定义 at line {}", inner[0].line));
            } else {
                // 内联成员函数定义：无独立的可链接符号
                warnings.push(format!(
                    "内联函数体无独立符号，已跳过 at line {}",
                    inner[0].line
                ));
            }
            j = after;
            continue;
        }
        // 剥离访问标签（public:/private:/protected:，Cay 侧统一渲染为 public）
        let inner = strip_access_labels(&inner);
        if inner.is_empty() {
            j = inner_next;
            continue;
        }
        // friend 声明：按自由函数提取（C++ 模式下同样 mangle）
        let has_friend = inner
            .iter()
            .any(|t| matches!(&t.tok, CTok::Ident(s) if s == "friend"));
        if has_friend {
            process_statement(inner, macros, protos, warnings, typedefs, &ns.to_vec(), true);
            j = inner_next;
            continue;
        }
        // using 声明 / 类内 typedef：跳过（别名无法表示）
        let first_is_using = matches!(&inner[0].tok, CTok::Ident(s) if s == "using");
        if first_is_using {
            j = inner_next;
            continue;
        }
        let first_is_typedef = matches!(&inner[0].tok, CTok::Ident(s) if s == "typedef");
        if first_is_typedef {
            process_typedef(&inner[1..], typedefs, warnings);
            j = inner_next;
            continue;
        }
        // 无顶层 ( → 数据成员：镜像为 Cay 字段
        let has_paren = inner.iter().any(|t| t.tok == CTok::Punct("("));
        if !has_paren {
            // 静态数据成员：Cay 无法声明其外部存储，跳过
            let is_static_member = inner
                .iter()
                .any(|t| matches!(&t.tok, CTok::Ident(s) if s == "static"));
            if is_static_member {
                if !warned_static_member {
                    warnings.push(format!(
                        "跳过类 '{}' 的静态数据成员（Cay 无法表示） at line {}",
                        class_name, inner[0].line
                    ));
                    warned_static_member = true;
                }
                j = inner_next;
                continue;
            }
            if !is_union {
                match mirror_data_member(inner, typedefs, known_classes) {
                    Ok((ty, name)) => cls.fields.push((ty, name)),
                    Err(reason) => {
                        if cls.layout_incomplete.is_none() {
                            cls.layout_incomplete = Some(reason);
                        }
                    }
                }
            }
            j = inner_next;
            continue;
        }
        parse_cpp_member(inner, ns, class_name, &mut cls, protos, warnings, typedefs, known_classes);
        j = inner_next;
    }
    if cls.has_virtual {
        warnings.push(format!(
            "类 '{}' 含虚函数，Cay 侧为直接调用，不支持虚分派语义 at line {}",
            class_name, class_line
        ));
    }
    if let Some(reason) = &cls.layout_incomplete {
        warnings.push(format!(
            "类 '{}' 对象布局不完整（{}），未生成构造函数；请通过 C++ 工厂函数创建对象 at line {}",
            class_name, reason, class_line
        ));
    }
    classes.push(cls);
}

/// 数据成员镜像：把 `TYPE name;` 片段映射为等尺寸 Cay 字段。
/// 返回 (Cay 类型文本, 字段名)；失败返回布局不完整原因。
fn mirror_data_member(
    inner: &[SpanTok],
    typedefs: &HashMap<String, CayType>,
    known_classes: &HashSet<String>,
) -> Result<(String, String), String> {
    // 位域（`int x : 3;`）：':' 已被分词器丢弃，残余为顶层数字字面量
    let has_eq = inner.iter().any(|t| t.tok == CTok::Punct("="));
    if !has_eq && inner.iter().any(|t| matches!(t.tok, CTok::Num(_))) {
        return Err("含位域成员".to_string());
    }
    // 数组维度：Cay 字段无定长数组，布局不可表示
    if inner.iter().any(|t| t.tok == CTok::Punct("[")) {
        return Err("含数组类型成员".to_string());
    }
    // 模板类型成员（std::vector<int> 等）
    if has_template_args(inner) {
        return Err("含模板类型成员".to_string());
    }
    // 去掉初值部分（= expr），避免初值中的标识符被误认为字段名
    let inner = strip_default_value(inner);
    // 字段名 = 末标识符；其前为类型 token
    let idents: Vec<usize> = inner
        .iter()
        .enumerate()
        .filter_map(|(i, t)| match &t.tok {
            CTok::Ident(_) => Some(i),
            _ => None,
        })
        .collect();
    if idents.len() < 2 {
        return Err("未识别的成员类型".to_string());
    }
    let name_idx = *idents.last().unwrap();
    let field_name = match &inner[name_idx].tok {
        CTok::Ident(s) => s.clone(),
        _ => return Err("未识别的成员类型".to_string()),
    };
    let type_toks: Vec<SpanTok> = inner[..name_idx].to_vec();
    // 按值类/struct/union 成员：Cay 字段无法表达其布局
    let first_ident = type_toks.iter().find_map(|t| match &t.tok {
        CTok::Ident(s) => Some(s.as_str()),
        _ => None,
    });
    if matches!(first_ident, Some("struct") | Some("union")) {
        return Err("含按值类成员".to_string());
    }
    // 去掉初值部分（= expr）后映射类型
    let type_toks = strip_default_value(&type_toks);
    let ty = match map_c_type(type_toks, typedefs, false) {
        Some(ty) => ty,
        None => {
            // 区分按值类成员与未识别类型
            let idents: Vec<&str> = type_toks
                .iter()
                .filter_map(|t| match &t.tok {
                    CTok::Ident(s) if !is_c_qualifier(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect();
            if idents.len() == 1 && known_classes.contains(idents[0]) {
                return Err("含按值类成员".to_string());
            }
            return Err("未识别的成员类型".to_string());
        }
    };
    match &ty.base {
        // 指针/引用成员：统一镜像为 c_void*（指针大小）
        _ if ty.stars > 0 => Ok(("c_void*".to_string(), field_name)),
        Base::Scalar(name) => Ok((name.clone(), field_name)),
        Base::Object(name) => {
            // map_c_type 不产生 Object；防御：按值类成员
            let _ = name;
            Err("含按值类成员".to_string())
        }
        Base::Void => Err("未识别的成员类型".to_string()),
    }
}

/// 剥离成员片段前导的访问标签（public/private/protected；':' 已被分词器丢弃）
fn strip_access_labels(inner: &[SpanTok]) -> &[SpanTok] {
    let mut k = 0;
    while k < inner.len() {
        match &inner[k].tok {
            CTok::Ident(s) if s == "public" || s == "private" || s == "protected" => k += 1,
            _ => break,
        }
    }
    &inner[k..]
}

/// 解析单条成员函数/构造/析构/运算符声明：
/// 普通方法/构造/析构收集进 `cls`（渲染为 interop class 的 native 声明）；
/// 运算符无法对应 Cay 方法语法，维持 `<Class>__operator_<op>` 自由函数
/// 别名形式（首参 c_void* 为 this）。
#[allow(clippy::too_many_arguments)]
fn parse_cpp_member(
    slice: &[SpanTok],
    ns: &[String],
    class_name: &str,
    cls: &mut InteropClass,
    protos: &mut Vec<ProtoFn>,
    warnings: &mut Vec<String>,
    typedefs: &HashMap<String, CayType>,
    known_classes: &HashSet<String>,
) {
    // 1. 剥离前导限定/说明符（static 记录为无 this，virtual 记入类标记）
    let mut is_static = false;
    let mut is_virtual = false;
    let mut k = 0;
    while k < slice.len() {
        match &slice[k].tok {
            CTok::Ident(s) => match s.as_str() {
                "static" => {
                    is_static = true;
                    k += 1;
                }
                "virtual" => {
                    is_virtual = true;
                    k += 1;
                }
                "inline" | "__inline" | "__inline__" | "constexpr" | "consteval"
                | "explicit" | "mutable" | "register" | "extern" => {
                    k += 1;
                }
                "__attribute__" | "__declspec" | "__attribute" => {
                    k = skip_attribute(slice, k);
                }
                _ => break,
            },
            _ => break,
        }
    }
    let slice = &slice[k..];
    if slice.is_empty() {
        return;
    }
    let decl_line = slice[0].line;

    // 2. 定位名称与参数列表括号（运算符函数单独处理括号对）
    let op_pos = slice
        .iter()
        .position(|t| matches!(&t.tok, CTok::Ident(s) if s == "operator"));
    let (ret_toks, method, alias_leaf, open): (&[SpanTok], MethodName, String, usize) =
        if let Some(op) = op_pos {
            // 运算符：op 后收集符号（operator() 的括号对特判）
            let mut syms: Vec<&str> = Vec::new();
            let mut p = op + 1;
            if p + 1 < slice.len()
                && slice[p].tok == CTok::Punct("(")
                && slice[p + 1].tok == CTok::Punct(")")
            {
                syms.push("(");
                syms.push(")");
                p += 2;
            } else {
                while p < slice.len() {
                    if let CTok::Punct(sym) = &slice[p].tok {
                        if *sym == "(" {
                            break;
                        }
                        syms.push(sym);
                        p += 1;
                    } else {
                        break;
                    }
                }
            }
            if syms.is_empty() {
                warnings.push(format!(
                    "跳过转换运算符（无法表示） at line {}",
                    decl_line
                ));
                return;
            }
            let (code, alias) = match cpp_op_mapping(&syms) {
                Some(v) => v,
                None => {
                    warnings.push(format!(
                        "跳过不支持的运算符重载 'operator{}' at line {}",
                        syms.join(""),
                        decl_line
                    ));
                    return;
                }
            };
            if p >= slice.len() || slice[p].tok != CTok::Punct("(") {
                return;
            }
            (
                &slice[..op],
                MethodName::Operator(code),
                format!("operator_{}", alias),
                p,
            )
        } else {
            // 普通方法/构造/析构：首个顶层 ( 前为名字
            let mut open: Option<usize> = None;
            for (idx, t) in slice.iter().enumerate() {
                if t.tok == CTok::Punct("(") {
                    open = Some(idx);
                    break;
                }
            }
            let open = match open {
                Some(o) => o,
                None => return,
            };
            if open == 0 {
                return;
            }
            let name_tok = &slice[open - 1].tok;
            let name = match name_tok {
                CTok::Ident(s) => s.clone(),
                _ => return,
            };
            let is_dtor = name == class_name
                && open >= 2
                && slice[open - 2].tok == CTok::Punct("~");
            if name == class_name {
                // 构造/析构：无返回类型
                let ret_end = if is_dtor { open - 2 } else { open - 1 };
                if ret_end > 0 {
                    // 构造/析构前还有残余 token（未识别的说明符）→ 保守跳过
                    warnings.push(format!(
                        "跳过类 '{}' 的构造/析构声明（无法解析说明符） at line {}",
                        class_name, decl_line
                    ));
                    return;
                }
                (
                    &slice[..0],
                    if is_dtor { MethodName::Dtor } else { MethodName::Ctor },
                    if is_dtor { "dtor".to_string() } else { "ctor".to_string() },
                    open,
                )
            } else {
                (
                    &slice[..open - 1],
                    MethodName::Named(name.clone()),
                    name,
                    open,
                )
            }
        };
    // 3. 参数列表
    let close = match find_matching(slice, open) {
        Some(c) => c,
        None => return,
    };
    let param_toks = &slice[open + 1..close];
    let alias_base = {
        let mut parts: Vec<String> = ns.to_vec();
        parts.push(class_name.to_string());
        parts.push(alias_leaf.clone());
        parts.join("__")
    };
    let pairs = match parse_params(param_toks, typedefs, warnings, &alias_base, true) {
        Some(p) => p,
        None => return,
    };
    // 4. 参数列表后的尾限定：const / noexcept / override / final / = 0 / = delete / = default
    let mut is_const = false;
    let mut m = close + 1;
    let mut pure_virtual = false;
    let mut deleted = false;
    while m < slice.len() {
        match &slice[m].tok {
            CTok::Ident(s) => match s.as_str() {
                "const" => {
                    is_const = true;
                    m += 1;
                }
                "noexcept" => {
                    m += 1;
                    if m < slice.len() && slice[m].tok == CTok::Punct("(") {
                        if let Some(c) = find_matching(slice, m) {
                            m = c + 1;
                        }
                    }
                }
                "volatile" | "override" | "final" => m += 1,
                "requires" => break, // requires 子句截断（概念约束无法表示）
                _ => m += 1,
            },
            CTok::Punct("&") => m += 1, // ref 限定
            CTok::Punct("=") => {
                match slice.get(m + 1).map(|t| &t.tok) {
                    Some(CTok::Num(n)) if n == "0" => pure_virtual = true,
                    Some(CTok::Ident(id)) if id == "delete" => deleted = true,
                    // = default 按普通声明处理
                    _ => {}
                }
                break;
            }
            _ => m += 1,
        }
    }
    if deleted {
        // = delete 函数不可调用，静默跳过
        return;
    }
    if is_virtual || pure_virtual {
        cls.has_virtual = true;
    }
    if pure_virtual {
        warnings.push(format!(
            "纯虚函数 '{}' 无独立符号，已跳过 at line {}",
            alias_base, decl_line
        ));
        return;
    }
    // 5. 返回类型（构造/析构为 void）
    let ret: CayType = match &method {
        MethodName::Ctor | MethodName::Dtor => CayType {
            base: Base::Void,
            stars: 0,
        },
        _ => {
            if has_template_args(ret_toks) {
                warn_template_type(ret_toks, warnings);
                CayType::opaque()
            } else {
                warn_enum_class(ret_toks, warnings);
                match map_c_type(ret_toks, typedefs, true) {
                    Some(r) => r,
                    None => {
                        warnings.push(format!(
                            "跳过 '{}': 无法表示的返回类型 at line {}",
                            alias_base, decl_line
                        ));
                        return;
                    }
                }
            }
        }
    };
    let mut user_params: Vec<Param> = pairs.iter().map(|(p, _)| p.clone()).collect();
    let cpp_params: Vec<String> = pairs.iter().map(|(_, s)| s.clone()).collect();

    // 运算符：维持自由函数别名形式（显式 this）
    if matches!(method, MethodName::Operator(_)) {
        let mangled = match mangle_function(
            ns,
            Some(class_name),
            method,
            &cpp_params,
            is_const && !is_static,
        ) {
            Some(m) => m,
            None => {
                warnings.push(format!(
                    "跳过 '{}': 参数类型无法生成 Itanium 链接名 at line {}",
                    alias_base, decl_line
                ));
                return;
            }
        };
        let mut params: Vec<Param> = Vec::new();
        if !is_static {
            params.push(Param::Typed(CayType::opaque()));
        }
        params.extend(user_params.iter().cloned());
        let alias = unique_alias(protos, &alias_base, &user_params);
        protos.push(ProtoFn {
            name: alias,
            call_conv: CallConv::Cdecl,
            ret,
            params,
            link_name: Some(mangled),
            has_this: !is_static,
            line: decl_line,
        });
        return;
    }

    // 普通方法/构造/析构：收集进 interop class（Cay 编译器负责 mangle）
    // 可变参数无法写入 Cay native 方法声明，跳过
    if user_params.iter().any(|p| matches!(p, Param::Varargs)) {
        warnings.push(format!(
            "跳过 '{}': 可变参数成员函数无法表示 at line {}",
            alias_base, decl_line
        ));
        return;
    }
    // 已知类的指针/引用参数（`Foo*`/`const Foo&`）映射为 Cay 对象类型
    for (par, cpp) in user_params.iter_mut().zip(cpp_params.iter()) {
        if matches!(par, Param::Typed(t) if *t == CayType::opaque()) {
            if let Some(name) = known_class_pointee(cpp, known_classes) {
                *par = Param::Typed(CayType {
                    base: Base::Object(name),
                    stars: 0,
                });
            }
        }
    }
    // 返回类型同理
    let ret = if ret == CayType::opaque() {
        let cpp_ret = tokens_to_cpp_type(ret_toks);
        match known_class_pointee(&cpp_ret, known_classes) {
            Some(name) => CayType {
                base: Base::Object(name),
                stars: 0,
            },
            None => ret,
        }
    } else {
        ret
    };
    match method {
        MethodName::Ctor => {
            cls.ctors.push(user_params);
        }
        MethodName::Dtor => {
            cls.has_dtor = true;
        }
        MethodName::Named(name) => {
            // 仅 const 区分的重载对：跳过 const 版本
            let same_sig = |m: &InteropMethod| {
                m.name == name && m.params == user_params && m.is_static == is_static
            };
            if is_const {
                if cls.methods.iter().any(|m| same_sig(m) && !m.is_const) {
                    warnings.push(format!(
                        "跳过 '{}': 与非常量重载仅 const 不同 at line {}",
                        alias_base, decl_line
                    ));
                    return;
                }
            } else if let Some(pos) = cls
                .methods
                .iter()
                .position(|m| same_sig(m) && m.is_const)
            {
                warnings.push(format!(
                    "跳过 '{}': 与非常量重载仅 const 不同（保留非常量版本） at line {}",
                    alias_base, decl_line
                ));
                cls.methods.remove(pos);
            }
            cls.methods.push(InteropMethod {
                name,
                ret,
                params: user_params,
                is_static,
                is_const,
            });
        }
        MethodName::Operator(_) => unreachable!(),
    }
}

/// 参数/返回为已知类指针/引用（`Foo*`/`const Foo&`，单层）时返回类名
fn known_class_pointee(cpp: &str, known_classes: &HashSet<String>) -> Option<String> {
    let s = cpp.trim();
    let s = s.strip_prefix("const ").unwrap_or(s).trim();
    if !(s.ends_with('*') || s.ends_with('&')) {
        return None;
    }
    let core = s[..s.len() - 1].trim();
    // 多级指针/限定名保持不透明
    if core.is_empty()
        || core.ends_with('*')
        || core.ends_with('&')
        || core.contains(' ')
        || core.contains("::")
    {
        return None;
    }
    if known_classes.contains(core) {
        Some(core.to_string())
    } else {
        None
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
    cpp_mode: bool,
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
    let mut expanded = expand_object_macros(&clean, macros);

    // 单条声明形式的 extern "C" int f(void); / extern "C++" ...
    // （extern 已被限定符剥离，字符串字面量还留在片段首部）
    let mut stmt_cpp = cpp_mode;
    if let Some(CTok::Str(lang)) = expanded.first().map(|t| &t.tok) {
        stmt_cpp = lang.contains("C++");
        expanded.remove(0);
        if expanded.is_empty() {
            return;
        }
    }

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
    process_prototype(&expanded, call_conv, protos, warnings, typedefs, ns_stack, stmt_cpp);
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
    cpp_mode: bool,
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
    if cpp_mode {
        warn_enum_class(&ret_toks, warnings);
    }
    let ret = if cpp_mode && has_template_args(&ret_toks) {
        // 模板实参返回类型（如 std::vector<int>）→ 告警并降级为不透明指针
        warn_template_type(&ret_toks, warnings);
        CayType::opaque()
    } else {
        match map_c_type(&ret_toks, typedefs, true) {
            Some(r) => r,
            None => {
                warnings.push(format!(
                    "跳过 '{}': 无法表示的返回类型 at line {}",
                    name,
                    slice[name_idx].line
                ));
                return;
            }
        }
    };
    // 参数
    let close = match find_matching(slice, open) {
        Some(c) => c,
        None => return,
    };
    let param_toks = &slice[open + 1..close];
    let pairs = match parse_params(param_toks, typedefs, warnings, &name, cpp_mode) {
        Some(p) => p,
        None => return,
    };
    // close 之后应无残留（属性已剥离）
    let trailing = &slice[close + 1..];
    if !trailing.is_empty() {
        warnings.push(format!("跳过 '{}': 参数列表后有残留 token", name));
        return;
    }
    // C++ 模式：自由函数按 Itanium ABI 生成链接名
    let mut link_name: Option<String> = None;
    if cpp_mode {
        let cpp_params: Vec<String> = pairs.iter().map(|(_, s)| s.clone()).collect();
        match mangle_function(ns_stack, None, MethodName::Named(base_name.clone()), &cpp_params, false) {
            Some(ln) => link_name = Some(ln),
            None => {
                warnings.push(format!(
                    "跳过 '{}': 参数类型无法生成 Itanium 链接名 at line {}",
                    name,
                    slice[name_idx].line
                ));
                return;
            }
        }
    }
    let params: Vec<Param> = pairs.into_iter().map(|(p, _)| p).collect();
    // 检查是否存在同名（同 fully-qualified 名）函数，若有则启用 mangle 后缀去重
    let final_name = unique_alias(protos, &name, &params);

    protos.push(ProtoFn {
        name: final_name,
        call_conv,
        ret,
        params,
        link_name,
        has_this: false,
        line: slice[name_idx].line,
    });
}

/// 生成冲突避免的 Cay 别名：与既有原型同名（或同 `base__` 前缀）时，
/// 先把恰好等于 base 的旧条目回填为参数后缀名，再返回带参数后缀的新名；
/// 后缀名仍冲突（如仅 const 不同的重载）时追加序号。
fn unique_alias(protos: &mut Vec<ProtoFn>, base: &str, params: &[Param]) -> String {
    let has_conflict = protos
        .iter()
        .any(|p| p.name == base || p.name.starts_with(&format!("{}__", base)));
    if !has_conflict {
        return base.to_string();
    }
    // 回填：把已存在未 mangle 的条目改名（成员函数跳过注入的 this 参数）
    for p in protos.iter_mut() {
        if p.name == base {
            let suffix_params: &[Param] = if p.has_this { &p.params[1..] } else { &p.params };
            let new_name = make_mangled_name(base, suffix_params);
            p.name = new_name;
        }
    }
    let mut cand = make_mangled_name(base, params);
    let mut seq = 2;
    while protos.iter().any(|p| p.name == cand) {
        cand = format!("{}_{}", make_mangled_name(base, params), seq);
        seq += 1;
    }
    cand
}

/// 剥离参数列表组中的顶层默认值 `= expr`（含括号/花括号嵌套）
fn strip_default_value(g: &[SpanTok]) -> &[SpanTok] {
    let mut depth = 0;
    for (idx, t) in g.iter().enumerate() {
        match &t.tok {
            CTok::Punct("(") | CTok::Punct("[") | CTok::Punct("{") => depth += 1,
            CTok::Punct(")") | CTok::Punct("]") | CTok::Punct("}") => depth -= 1,
            CTok::Punct("=") if depth == 0 => return &g[..idx],
            _ => {}
        }
    }
    g
}

/// token 序列是否含模板实参（顶层 `<`，如 `std::vector<int>`）
fn has_template_args(toks: &[SpanTok]) -> bool {
    toks.iter().any(|t| t.tok == CTok::Punct("<"))
}

/// 取 `<` 前的限定名（`std::vector<int>` → `std::vector`），取不到用 "template"
fn template_type_name(toks: &[SpanTok]) -> String {
    let lt = match toks.iter().position(|t| t.tok == CTok::Punct("<")) {
        Some(p) => p,
        None => return "template".to_string(),
    };
    let mut parts: Vec<String> = Vec::new();
    let mut k = lt;
    while k > 0 {
        k -= 1;
        match &toks[k].tok {
            CTok::Ident(s) => parts.push(s.clone()),
            CTok::Punct("::") => parts.push("::".to_string()),
            _ => break,
        }
    }
    parts.reverse();
    let joined: String = parts
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join("")
        .trim_start_matches("::")
        .to_string();
    if joined.is_empty() {
        "template".to_string()
    } else {
        joined
    }
}

/// 模板实参类型告警（类型位置：降级而非跳过整条声明）
fn warn_template_type(toks: &[SpanTok], warnings: &mut Vec<String>) {
    let name = template_type_name(toks);
    let line = toks.first().map(|t| t.line).unwrap_or(0);
    warnings.push(format!(
        "C++ 模板 '{}' 需要 C++ 编译器实例化展开后才能使用，类型已降级为 c_void* at line {}",
        name, line
    ));
}

/// `enum class` 按 c_int 映射的提示（C++ 模式）
fn warn_enum_class(toks: &[SpanTok], warnings: &mut Vec<String>) {
    for w in toks.windows(2) {
        if let (CTok::Ident(a), CTok::Ident(b)) = (&w[0].tok, &w[1].tok) {
            if a == "enum" && b == "class" {
                warnings.push(format!(
                    "enum class 按 c_int 处理 at line {}",
                    w[0].line
                ));
                return;
            }
        }
    }
}

/// 把类型 token 序列还原为 C++ 类型字符串（供 Itanium mangling）：
/// 标识符/`::` 组合为限定名，`*`/`&` 紧贴，数组维度退化为 `*`。
fn tokens_to_cpp_type(toks: &[SpanTok]) -> String {
    let mut s = String::new();
    let mut in_bracket = false;
    for t in toks {
        if in_bracket {
            if t.tok == CTok::Punct("]") {
                in_bracket = false;
            }
            continue;
        }
        match &t.tok {
            CTok::Ident(id) => {
                // 前一个字符是字母/数字/下划线时需空格分隔（多词基类型）
                if s.chars().last().map_or(false, |c| c.is_alphanumeric() || c == '_') {
                    s.push(' ');
                }
                s.push_str(id);
            }
            CTok::Punct("[") => {
                s.push('*'); // 函数参数中的数组按退化指针编码
                in_bracket = true;
            }
            CTok::Punct("]") => {}
            CTok::Punct(p) => s.push_str(p),
            CTok::Num(_) | CTok::Str(_) => {}
            CTok::VarArgs => s.push_str("..."),
        }
    }
    s
}

fn parse_params(
    toks: &[SpanTok],
    typedefs: &HashMap<String, CayType>,
    warnings: &mut Vec<String>,
    fn_name: &str,
    cpp_mode: bool,
) -> Option<Vec<(Param, String)>> {
    // void 单参 → 空
    if toks.len() == 1 {
        if let CTok::Ident(s) = &toks[0].tok {
            if s == "void" {
                return Some(Vec::new());
            }
        }
    }
    // 空参数列表：C++ 中 () 即无参；C 中表示参数未指定，按可变参数处理
    if toks.is_empty() {
        if cpp_mode {
            return Some(Vec::new());
        }
        warnings.push(format!(
            "'{}()' 在 C 中表示参数未指定，映射为可变参数 (...)",
            fn_name
        ));
        return Some(vec![(Param::Varargs, "...".to_string())]);
    }
    let groups = split_top_commas(toks);
    let mut params = Vec::new();
    for g in groups {
        // 剥离顶层默认值 = expr（C++ 默认参数）
        let g = strip_default_value(&g);
        if g.is_empty() {
            continue;
        }
        // 变参 ...
        if g.len() == 1 && g[0].tok == CTok::VarArgs {
            params.push((Param::Varargs, "...".to_string()));
            continue;
        }
        // 命名变参：TYPE ... name 形式罕见，仅当末 token 为 ... 时按变参处理
        if g.iter().any(|t| t.tok == CTok::VarArgs) {
            params.push((Param::Varargs, "...".to_string()));
            continue;
        }
        // 函数指针参数（含 (*）→ 不透明
        if g.iter().any(|t| t.tok == CTok::Punct("(")) {
            params.push((Param::Typed(CayType::opaque()), String::new()));
            continue;
        }
        // C++ 模式：模板实参类型（std::vector<int> 等）→ 告警并降级为不透明指针
        if cpp_mode && has_template_args(&g) {
            warn_template_type(&g, warnings);
            params.push((Param::Typed(CayType::opaque()), tokens_to_cpp_type(&g)));
            continue;
        }
        if cpp_mode {
            warn_enum_class(&g, warnings);
        }
        let stripped = strip_param_name(&g, typedefs);
        match map_c_type(&stripped, typedefs, false) {
            Some(ty) => params.push((Param::Typed(ty), tokens_to_cpp_type(&stripped))),
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
    // 统计 * / &（引用按等价指针层映射）与数组维度，收集标识符（剥除限定符 const/volatile 等）
    let mut stars = 0;
    let mut arr = 0;
    let mut idents: Vec<String> = Vec::new();
    for t in spec {
        match &t.tok {
            CTok::Punct("*") => stars += 1,
            CTok::Punct("&") => stars += 1,
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

fn emit(protos: &[ProtoFn], classes: &[InteropClass]) -> String {
    let mut out = String::new();
    if !protos.is_empty() {
        // 按 cdecl / stdcall 分块（Cay 每块调用约定一致）
        let cdecl: Vec<&ProtoFn> = protos.iter().filter(|p| p.call_conv == CallConv::Cdecl).collect();
        let stdcall: Vec<&ProtoFn> = protos
            .iter()
            .filter(|p| p.call_conv == CallConv::Stdcall)
            .collect();
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
    }
    // interop class：按命名空间分组（保持首次出现顺序）
    let mut ns_order: Vec<&Vec<String>> = Vec::new();
    for c in classes {
        if !ns_order.contains(&&c.ns) {
            ns_order.push(&c.ns);
        }
    }
    for ns in ns_order {
        let body: String = classes
            .iter()
            .filter(|c| &c.ns == ns)
            .map(|c| render_interop_class(c, ns.len()))
            .collect();
        if ns.is_empty() {
            out.push_str(&body);
        } else {
            // 嵌套 namespace 块：namespace a { namespace b { ... } }
            for (depth, name) in ns.iter().enumerate() {
                out.push_str(&"    ".repeat(depth));
                out.push_str(&format!("namespace {} {{\n", name));
            }
            out.push_str(&body);
            for depth in (0..ns.len()).rev() {
                out.push_str(&"    ".repeat(depth));
                out.push_str("}\n");
            }
        }
    }
    out
}

/// 渲染单个 interop class（indent 为 namespace 嵌套深度）
fn render_interop_class(c: &InteropClass, indent: usize) -> String {
    let pad = "    ".repeat(indent);
    let mpad = "    ".repeat(indent + 1);
    let mut out = format!("{}interop class {} {{\n", pad, c.name);
    // 虚表指针镜像（含虚函数的类首字段）
    if c.has_virtual {
        out.push_str(&format!("{}public c_void* __cpp_vptr;\n", mpad));
    }
    for (ty, name) in &c.fields {
        out.push_str(&format!("{}public {} {};\n", mpad, ty, name));
    }
    // 布局不完整时不生成构造/析构：interop 类无默认构造，`new` 自然被封死
    if c.layout_incomplete.is_none() {
        for params in &c.ctors {
            out.push_str(&format!(
                "{}public native {}({});\n",
                mpad,
                c.name,
                render_params(params)
            ));
        }
        if c.has_dtor {
            out.push_str(&format!("{}public native ~{}();\n", mpad, c.name));
        }
    }
    for m in &c.methods {
        let mut decl = format!("{}public native ", mpad);
        if m.is_static {
            decl.push_str("static ");
        }
        decl.push_str(&format!("{} {}({})", m.ret.render(), m.name, render_params(&m.params)));
        if m.is_const {
            decl.push_str(" const");
        }
        decl.push_str(";\n");
        out.push_str(&decl);
    }
    out.push_str(&format!("{}}}\n", pad));
    out
}

/// interop class 成员声明的参数列表：Cay 类内方法声明要求参数名，
/// 合成 p0/p1/...（extern 块原型允许无名参数，不走此路径）
fn render_params(params: &[Param]) -> String {
    params
        .iter()
        .enumerate()
        .map(|(i, par)| match par {
            Param::Typed(ty) => format!("{} p{}", ty.render(), i),
            Param::Varargs => "...".to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn render_proto(p: &ProtoFn) -> String {
    let params: Vec<String> = p
        .params
        .iter()
        .map(|par| match par {
            Param::Typed(ty) => ty.render(),
            Param::Varargs => "...".to_string(),
        })
        .collect();
    let params = params.join(", ");
    // Cay 别名：与关键字/内置冲突时加 _c 后缀
    let alias = if C_NAME_COLLISIONS.contains(&p.name.as_str()) {
        format!("{}_c", p.name)
    } else {
        p.name.clone()
    };
    match &p.link_name {
        // C++ 链接名：符号用 mangled 名，Cay 侧用别名
        Some(ln) => format!("{} {}({}) as {};", p.ret.render(), ln, params, alias),
        None => {
            let mut s = String::new();
            s.push_str(&p.ret.render());
            s.push(' ');
            s.push_str(&p.name);
            s.push('(');
            s.push_str(&params);
            s.push(')');
            if alias != p.name {
                s.push_str(&format!(" as {}", alias));
            }
            s.push(';');
            s
        }
    }
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
        // extern "C++" 触发 C++ 模式：块内外自由函数都按 Itanium mangle
        let r = extract("extern \"C++\" {\nint cpponly(void);\n}\nint cfn(void);");
        assert!(r.extern_code.contains("c_int _Z7cpponlyv() as cpponly;"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int _Z3cfnv() as cfn;"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_namespace_extraction() {
        // namespace 触发 C++ 模式：块内函数按命名空间 mangle，别名沿用 ns__ 前缀
        let r = extract("namespace ns { int f(void); } int g(void);");
        assert!(r.extern_code.contains("c_int _ZN2ns1fEv() as ns__f;"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int _Z1gv() as g;"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_class_friend_extraction() {
        // class 触发 C++ 模式：friend 按自由函数 mangle，成员函数进 interop class
        let r = extract("class C { friend int foo(void); int mem(void); }; int bar(void);");
        assert!(r.extern_code.contains("c_int _Z3foov() as foo;"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int _Z3barv() as bar;"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("interop class C {"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("public native c_int mem();"), "got: {}", r.extern_code);
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

    // ==================== C++ 模式测试 ====================

    /// 以 .hpp 文件名提取（扩展名触发 C++ 模式）
    fn extract_hpp(text: &str) -> CHeaderExtract {
        let defines = HashMap::new();
        extract_c_header_text("test.hpp", text, &defines).unwrap()
    }

    #[test]
    fn test_cpp_class_members() {
        // interop class 完整输出：字段镜像 + native 构造/析构/方法（const/static），
        // 运算符维持自由函数别名形式
        let r = extract(
            "class Foo {\n\
             public:\n\
             Foo();\n\
             ~Foo();\n\
             void bar(int x);\n\
             int get() const;\n\
             static int ver();\n\
             int operator+(const Foo& o);\n\
             private:\n\
             int v_;\n\
             };",
        );
        let expected = "interop class Foo {\n\
            \x20   public c_int v_;\n\
            \x20   public native Foo();\n\
            \x20   public native ~Foo();\n\
            \x20   public native void bar(c_int p0);\n\
            \x20   public native c_int get() const;\n\
            \x20   public native static c_int ver();\n\
            }";
        assert!(r.extern_code.contains(expected), "class: got: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int _ZN3FooplERK3Foo(c_void*, c_void*) as Foo__operator_plus;"), "operator+: {}", r.extern_code);
        // 构造/析构/方法不再以自由函数形式出现
        assert!(!r.extern_code.contains("Foo__ctor"), "ctor: {}", r.extern_code);
        assert!(!r.extern_code.contains("Foo__bar"), "bar: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_member_overload_suffix() {
        // 方法重载由 Cay 原生重载决议处理，按参数类型区分
        let r = extract("class Foo {\npublic:\nvoid bar(int);\nvoid bar(double);\n};");
        assert!(r.extern_code.contains("public native void bar(c_int p0);"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("public native void bar(c_double p0);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_template_function_skipped() {
        let r = extract("template <typename T> T max(T a, T b);\nint ok(void);");
        assert!(r.warnings.iter().any(|w| w.contains("C++ 模板 'max'")), "warnings: {:?}", r.warnings);
        assert!(!r.extern_code.contains("max"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("ok"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_template_class_skipped() {
        let r = extract("template <typename T> class Box {\npublic:\nT get();\n};\nint ok(void);");
        assert!(r.warnings.iter().any(|w| w.contains("C++ 模板 'Box'")), "warnings: {:?}", r.warnings);
        assert!(!r.extern_code.contains("Box"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("ok"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_template_type_param_skipped() {
        // 参数类型带模板实参：告警降级，但 C++ 模式下无法 mangle → 整条跳过
        let r = extract("class X;\nvoid process(std::vector<int> v);\nint ok(void);");
        assert!(r.warnings.iter().any(|w| w.contains("C++ 模板 'std::vector'")), "warnings: {:?}", r.warnings);
        assert!(!r.extern_code.contains("process"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("ok"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_free_function_mangle() {
        let r = extract_hpp("int add(int a, int b);");
        assert!(r.extern_code.contains("c_int _Z3addii(c_int, c_int) as add;"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_extern_c_block_not_mangled() {
        let r = extract_hpp("extern \"C\" {\nint cfn(void);\n}\nint cppfn(void);");
        assert!(r.extern_code.contains("c_int cfn();"), "extern C: {}", r.extern_code);
        assert!(r.extern_code.contains("c_int _Z5cppfnv() as cppfn;"), "cpp fn: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_default_arg_stripped() {
        let r = extract("class Foo {\npublic:\nvoid bar(int x = 5, const char* s = \"hi\");\n};");
        assert!(r.extern_code.contains("public native void bar(c_int p0, c_string p1);"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_namespace_class_mangle() {
        // 命名空间内的类渲染为 namespace 块包裹的 interop class
        let r = extract("namespace ns {\nclass Foo {\npublic:\nvoid bar(int);\n};\n}");
        let expected = "namespace ns {\n\
            \x20   interop class Foo {\n\
            \x20       public native void bar(c_int p0);\n\
            \x20   }\n\
            }";
        assert!(r.extern_code.contains(expected), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_pure_virtual_skipped() {
        // 纯虚跳过+告警；含虚函数的类补 __cpp_vptr 首字段并告警
        let r = extract("class Foo {\npublic:\nvirtual int pv() = 0;\nint ok();\n};");
        assert!(r.warnings.iter().any(|w| w.contains("纯虚函数")), "warnings: {:?}", r.warnings);
        assert!(r.warnings.iter().any(|w| w.contains("虚函数")), "warnings: {:?}", r.warnings);
        assert!(r.extern_code.contains("public c_void* __cpp_vptr;"), "vptr: {}", r.extern_code);
        assert!(!r.extern_code.contains("pv()"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("public native c_int ok();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_inline_body_skipped() {
        let r = extract("class Foo {\npublic:\nvoid inl() { }\nvoid decl();\n};");
        assert!(r.warnings.iter().any(|w| w.contains("内联函数体")), "warnings: {:?}", r.warnings);
        assert!(!r.extern_code.contains("inl"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("public native void decl();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_deleted_and_defaulted() {
        let r = extract("class Foo {\npublic:\nFoo();\nFoo(const Foo& o) = delete;\nvoid reset() = default;\n};");
        assert!(r.extern_code.contains("public native Foo();"), "default ctor: {}", r.extern_code);
        assert!(!r.extern_code.contains("Foo(Foo"), "copy ctor deleted: {}", r.extern_code);
        assert!(r.extern_code.contains("public native void reset();"), "=default kept: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_virtual_vptr_field() {
        // 含虚函数（非纯虚）：方法保留，字段最前补 __cpp_vptr，告警一次
        let r = extract("class Foo {\npublic:\nvirtual void hook();\nint x;\n};");
        let expected = "interop class Foo {\n\
            \x20   public c_void* __cpp_vptr;\n\
            \x20   public c_int x;\n\
            \x20   public native void hook();";
        assert!(r.extern_code.contains(expected), "got: {}", r.extern_code);
        assert_eq!(
            r.warnings.iter().filter(|w| w.contains("不支持虚分派")).count(),
            1,
            "warnings: {:?}",
            r.warnings
        );
    }

    #[test]
    fn test_cpp_layout_incomplete_no_ctor() {
        // 含基类：布局不完整，不生成构造/析构，方法照常
        let r = extract("class Base { public: int x; };\nclass Derived : public Base {\npublic:\nDerived();\n~Derived();\nvoid hello();\n};");
        assert!(
            r.warnings.iter().any(|w| w.contains("类 'Derived' 对象布局不完整（含基类）")),
            "warnings: {:?}", r.warnings
        );
        let derived_start = r.extern_code.find("interop class Derived").expect("Derived class");
        let derived_block = &r.extern_code[derived_start..];
        assert!(!derived_block.contains("native Derived("), "ctor: {}", derived_block);
        assert!(!derived_block.contains("~Derived"), "dtor: {}", derived_block);
        assert!(derived_block.contains("public native void hello();"), "method: {}", derived_block);
    }

    #[test]
    fn test_cpp_bitfield_layout_incomplete() {
        let r = extract("class Foo {\npublic:\nFoo();\nvoid ok();\nunsigned flags : 3;\n};");
        assert!(
            r.warnings.iter().any(|w| w.contains("位域")),
            "warnings: {:?}", r.warnings
        );
        assert!(!r.extern_code.contains("native Foo("), "ctor: {}", r.extern_code);
        assert!(r.extern_code.contains("public native void ok();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_by_value_class_member_incomplete() {
        let r = extract("class Inner { public: int v; };\nclass Outer {\npublic:\nOuter();\nInner in_;\n};");
        assert!(
            r.warnings.iter().any(|w| w.contains("按值类成员")),
            "warnings: {:?}", r.warnings
        );
        assert!(!r.extern_code.contains("native Outer("), "ctor: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_const_only_overload_skipped() {
        // 仅 const 区分的重载对：跳过 const 版本并告警
        let r = extract("class Foo {\npublic:\nint get();\nint get() const;\n};");
        assert!(
            r.warnings.iter().any(|w| w.contains("仅 const 不同")),
            "warnings: {:?}", r.warnings
        );
        assert!(r.extern_code.contains("public native c_int get();"), "got: {}", r.extern_code);
        assert!(!r.extern_code.contains("get() const"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_nested_class_skipped() {
        let r = extract("class Outer {\npublic:\nvoid ok();\nclass Inner {\npublic:\nvoid bad();\n};\n};");
        assert!(
            r.warnings.iter().any(|w| w.contains("跳过嵌套类 'Inner'")),
            "warnings: {:?}", r.warnings
        );
        assert!(!r.extern_code.contains("Inner"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("public native void ok();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_static_data_member_skipped() {
        let r = extract("class Foo {\npublic:\nvoid ok();\nstatic int count_;\nint v_;\n};");
        assert!(
            r.warnings.iter().any(|w| w.contains("静态数据成员")),
            "warnings: {:?}", r.warnings
        );
        assert!(!r.extern_code.contains("count_"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("public c_int v_;"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_class_ptr_param_object() {
        // 已知类指针/引用参数与返回映射为 Cay 对象类型（mangle 自动加 P）
        let r = extract("class Foo {\npublic:\nvoid take(Foo* other);\nFoo& self();\nint v_;\n};");
        assert!(r.extern_code.contains("public native void take(Foo p0);"), "got: {}", r.extern_code);
        assert!(r.extern_code.contains("public native Foo self();"), "got: {}", r.extern_code);
    }

    #[test]
    fn test_cpp_field_type_mirror() {
        // 标量字段按声明顺序镜像为等尺寸 Cay 类型；指针成员为 c_void*
        let r = extract("class Foo {\npublic:\nchar c;\nshort s;\nlong l;\nlong long ll;\nfloat f;\ndouble d;\nbool b;\nunsigned u;\nvoid* p;\n};");
        for field in [
            "public c_char c;",
            "public c_short s;",
            "public c_long l;",
            "public c_int64_t ll;",
            "public c_float f;",
            "public c_double d;",
            "public c_bool b;",
            "public c_uint u;",
            "public c_void* p;",
        ] {
            assert!(r.extern_code.contains(field), "{}: got: {}", field, r.extern_code);
        }
    }

    #[test]
    fn test_cpp_pragma_once() {
        // 带 #pragma once 的头被包含两次只展开一次
        let dir = std::env::temp_dir().join(format!("cay_pragma_once_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let sub = dir.join("once_hdr.h");
        std::fs::write(&sub, "#pragma once\nint once_fn(void);\n").unwrap();
        let top = dir.join("top.h");
        let top_text = "#include \"once_hdr.h\"\n#include \"once_hdr.h\"\nint top_fn(void);";
        let defines = HashMap::new();
        let r = extract_c_header_text(&top.to_string_lossy(), top_text, &defines).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(r.extern_code.matches("once_fn").count(), 1, "got: {}", r.extern_code);
        assert!(r.extern_code.contains("top_fn"), "got: {}", r.extern_code);
    }
}
