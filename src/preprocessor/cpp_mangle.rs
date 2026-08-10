//! 字符串级 Itanium ABI mangler（C++ 链接名生成）
//!
//! 供 `#include_c` 的 C++ 头文件提取路径（`c_header.rs`）使用：把解析出的
//! 命名空间 / 类 / 方法 / 参数类型字符串编码为 Itanium ABI（g++ / clang / MinGW）
//! 的 mangled 符号名。不支持 MSVC mangling。
//!
//! 设计约束：
//! - 不依赖 `crate::types`，输入全是字符串（预处理器文本层无法复用
//!   `codegen/itanium_mangle.rs`）；
//! - 不做 Itanium `S_` 替换压缩——替换只是可选压缩，重复展开写出的名字同样合法；
//! - 无法编码的类型（模板实参、`long double`、未知多词类型等）返回 `None`，
//!   由调用方决定降级或跳过并告警。

/// 方法名：普通名 / 构造 / 析构 / 运算符
#[derive(Debug, Clone, PartialEq)]
pub enum MethodName {
    Named(String),
    Ctor,
    Dtor,
    /// Itanium 运算符代码（如 `+` → `"pl"`），符号到代码的白名单映射由调用方完成
    Operator(&'static str),
}

/// 追加 `<len><name>` 形式的源名编码
fn push_len_name(out: &mut String, name: &str) {
    out.push_str(&name.chars().count().to_string());
    out.push_str(name);
}

/// 嵌套限定名 `N[K]<ns...><class><unqualified>E`（const 成员函数在 N 后加 K）
fn build_nested(ns: &[String], class: Option<&str>, method: &MethodName, is_const_method: bool) -> String {
    let mut s = String::from("N");
    if is_const_method {
        s.push('K');
    }
    for n in ns {
        push_len_name(&mut s, n);
    }
    if let Some(c) = class {
        push_len_name(&mut s, c);
    }
    match method {
        MethodName::Named(m) => push_len_name(&mut s, m),
        MethodName::Ctor => s.push_str("C1"),
        MethodName::Dtor => s.push_str("D1"),
        MethodName::Operator(code) => s.push_str(code),
    }
    s.push('E');
    s
}

/// 生成不带参数编码的嵌套名（`N...E`），如 `ns::Foo::bar` → `N2ns3Foo3barE`。
/// 公开 API 的完整性保留项（mangle_function 内部走 build_nested），当前仅单测使用。
#[allow(dead_code)]
pub fn mangle_qualified_name(ns: &[String], class: Option<&str>, method: MethodName) -> String {
    build_nested(ns, class, &method, false)
}

/// 生成完整函数符号 `_Z<name><params>`；空参编码为 `v`。
/// 全局自由函数（无命名空间/类）用非嵌套名，其余用 `N...E` 嵌套名。
/// 任一参数类型无法编码时返回 `None`（调用方告警并跳过该声明）。
pub fn mangle_function(
    ns: &[String],
    class: Option<&str>,
    method: MethodName,
    params: &[String],
    is_const_method: bool,
) -> Option<String> {
    let mut s = String::from("_Z");
    if ns.is_empty() && class.is_none() {
        // 全局自由函数：<len>name，无 N...E 包裹
        match &method {
            MethodName::Named(m) => push_len_name(&mut s, m),
            MethodName::Operator(code) => s.push_str(code),
            // 全局构造/析构不存在；按普通名处理以免产生非法符号
            MethodName::Ctor | MethodName::Dtor => return None,
        }
    } else {
        s.push_str(&build_nested(ns, class, &method, is_const_method));
    }
    if params.is_empty() {
        s.push('v');
    } else {
        for p in params {
            s.push_str(&mangle_type(p)?);
        }
    }
    Some(s)
}

/// 把 C++ 类型字符串简单词法化：标识符/数字成词，`::`/`&&`/`...` 合并，
/// 其余有效符号单字符成词，无效字符忽略。
fn lex_type(s: &str) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c.is_alphanumeric() || c == '_' {
            let start = i;
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            out.push(chars[start..i].iter().collect());
            continue;
        }
        if c == ':' && i + 1 < chars.len() && chars[i + 1] == ':' {
            out.push("::".to_string());
            i += 2;
            continue;
        }
        if c == '&' && i + 1 < chars.len() && chars[i + 1] == '&' {
            out.push("&&".to_string());
            i += 2;
            continue;
        }
        if c == '.' && i + 2 < chars.len() && chars[i + 1] == '.' && chars[i + 2] == '.' {
            out.push("...".to_string());
            i += 3;
            continue;
        }
        match c {
            '*' | '&' | '<' | '>' | ',' | '[' | ']' => {
                out.push(c.to_string());
            }
            _ => {}
        }
        i += 1;
    }
    out
}

/// 基类型关键字组合 → Itanium 类型编码
fn mangle_builtin(joined: &str) -> Option<&'static str> {
    Some(match joined {
        "void" => "v",
        "bool" => "b",
        "char" => "c",
        "signed char" => "a",
        "unsigned char" => "h",
        "short" | "short int" | "signed short" | "signed short int" => "s",
        "unsigned short" | "unsigned short int" => "t",
        "int" | "signed" | "signed int" => "i",
        "unsigned" | "unsigned int" => "j",
        "long" | "long int" | "signed long" | "signed long int" => "l",
        "unsigned long" | "unsigned long int" => "m",
        "long long" | "long long int" | "signed long long" | "signed long long int" => "x",
        "unsigned long long" | "unsigned long long int" => "y",
        "float" => "f",
        "double" => "d",
        "wchar_t" => "w",
        "char8_t" => "Du",
        "char16_t" => "Ds",
        "char32_t" => "Di",
        // size_t 按 unsigned long 编码（LP64 正确；LLP64 上与 unsigned long long
        // 不同，但调用方生成的声明在两个平台上链接的库通常也一致地使用该 typedef）
        "size_t" => "m",
        "..." => "z",
        _ => return None,
    })
}

/// 把 C++ 类型字符串编码为 Itanium 类型编码；无法编码时返回 `None`。
///
/// 支持：基类型（含 cv 前缀）、指针 `P`、左值引用 `R`、右值引用 `O`、
/// 定长数组 `A<n>_`、限定名 `ns::Foo`（`N...E`）、未知单词名按 class 类型
/// （`<len>Name`）编码。模板实参（`<...>`）、`long double`、多词未知类型 → `None`。
pub fn mangle_type(cpp_ty: &str) -> Option<String> {
    let toks = lex_type(cpp_ty);
    if toks.is_empty() {
        return None;
    }
    // 模板实参 / 逗号 → 无法表示（需要 C++ 编译器实例化）
    if toks.iter().any(|t| t == "<" || t == ">" || t == ",") {
        return None;
    }
    // 基类型前导 cv 限定
    let mut idx = 0;
    let mut prefix = String::new();
    while idx < toks.len() {
        match toks[idx].as_str() {
            "const" => {
                prefix.push('K');
                idx += 1;
            }
            "volatile" => {
                prefix.push('V');
                idx += 1;
            }
            _ => break,
        }
    }
    // 剥离 class/struct/enum 等阐述关键字
    while idx < toks.len()
        && matches!(
            toks[idx].as_str(),
            "class" | "struct" | "union" | "enum" | "typename"
        )
    {
        idx += 1;
    }
    // 基类型终点：首个 * & [ 出现处
    let mut base_end = idx;
    while base_end < toks.len()
        && !matches!(toks[base_end].as_str(), "*" | "&" | "&&" | "[")
    {
        base_end += 1;
    }
    if base_end == idx {
        return None;
    }
    let base_toks = &toks[idx..base_end];
    // 基类型编码
    let mut acc: String = if base_toks.iter().any(|t| t == "::") {
        // 限定名 ns::Foo（允许前导 :: 表示全局）
        let mut parts: Vec<&str> = Vec::new();
        for t in base_toks {
            if t == "::" {
                continue;
            }
            parts.push(t);
        }
        if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
            return None;
        }
        if parts.len() == 1 {
            // `::Global` 与 `Global` 编码相同（单组件不用 N...E）
            let mut q = String::new();
            push_len_name(&mut q, parts[0]);
            q
        } else {
            let mut q = String::from("N");
            for p in parts {
                push_len_name(&mut q, p);
            }
            q.push('E');
            q
        }
    } else {
        let joined = base_toks.join(" ");
        if joined == "long double" {
            return None; // 不支持（与 Cay 类型映射策略一致）
        }
        match mangle_builtin(&joined) {
            Some(code) => code.to_string(),
            None => {
                // 未知单词名 → class 类型 <len>Name；多词未知 → 放弃
                if base_toks.len() == 1 {
                    let mut q = String::new();
                    push_len_name(&mut q, &base_toks[0]);
                    q
                } else {
                    return None;
                }
            }
        }
    };
    // cv 前缀作用于基类型（const Foo → K3Foo）
    if !prefix.is_empty() {
        acc = format!("{}{}", prefix, acc);
    }
    // 后缀：数组维度与指针/引用（源序从左到右依次包裹）
    let mut j = base_end;
    while j < toks.len() {
        match toks[j].as_str() {
            "*" => {
                acc = format!("P{}", acc);
                j += 1;
            }
            "&" => {
                acc = format!("R{}", acc);
                j += 1;
            }
            "&&" => {
                acc = format!("O{}", acc);
                j += 1;
            }
            "[" => {
                // 收集连续数组维度后逆序包裹：int[2][3] = array[2] of array[3] of int
                // → A2_A3_i；无定长（[]）按退化指针处理
                let mut dims: Vec<String> = Vec::new();
                while j < toks.len() && toks[j] == "[" {
                    if j + 2 < toks.len()
                        && toks[j + 1].chars().all(|c| c.is_ascii_digit())
                        && toks[j + 2] == "]"
                    {
                        dims.push(toks[j + 1].clone());
                        j += 3;
                    } else if j + 1 < toks.len() && toks[j + 1] == "]" {
                        dims.push(String::new()); // 空维度 → 退化指针
                        j += 2;
                    } else {
                        return None;
                    }
                }
                for d in dims.iter().rev() {
                    if d.is_empty() {
                        acc = format!("P{}", acc);
                    } else {
                        acc = format!("A{}_{}", d, acc);
                    }
                }
            }
            // 指针/引用自身的顶层 cv 限定在函数签名 mangling 中被忽略
            "const" | "volatile" => {
                j += 1;
            }
            _ => return None,
        }
    }
    Some(acc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ns(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn test_free_function() {
        let m = mangle_function(&[], None, MethodName::Named("foo".to_string()), &[], false);
        assert_eq!(m.as_deref(), Some("_Z3foov"));
    }

    #[test]
    fn test_method_int_param() {
        // Foo::bar(int) → _ZN3Foo3barEi
        let m = mangle_function(
            &[],
            Some("Foo"),
            MethodName::Named("bar".to_string()),
            &["int".to_string()],
            false,
        );
        assert_eq!(m.as_deref(), Some("_ZN3Foo3barEi"));
    }

    #[test]
    fn test_ctor_dtor() {
        let c = mangle_function(&[], Some("Foo"), MethodName::Ctor, &[], false);
        assert_eq!(c.as_deref(), Some("_ZN3FooC1Ev"));
        let d = mangle_function(&[], Some("Foo"), MethodName::Dtor, &[], false);
        assert_eq!(d.as_deref(), Some("_ZN3FooD1Ev"));
    }

    #[test]
    fn test_const_method() {
        // const bar() → _ZNK3Foo3barEv
        let m = mangle_function(&[], Some("Foo"), MethodName::Named("bar".to_string()), &[], true);
        assert_eq!(m.as_deref(), Some("_ZNK3Foo3barEv"));
    }

    #[test]
    fn test_namespaced_class() {
        // ns::Foo::bar(int) → _ZN2ns3Foo3barEi
        let m = mangle_function(
            &ns(&["ns"]),
            Some("Foo"),
            MethodName::Named("bar".to_string()),
            &["int".to_string()],
            false,
        );
        assert_eq!(m.as_deref(), Some("_ZN2ns3Foo3barEi"));
    }

    #[test]
    fn test_class_pointer_param() {
        // bar(Foo*) → _ZN3Foo3barEP3Foo（不做 S_ 替换压缩，第二个 3Foo 展开写）
        let m = mangle_function(
            &[],
            Some("Foo"),
            MethodName::Named("bar".to_string()),
            &["Foo*".to_string()],
            false,
        );
        assert_eq!(m.as_deref(), Some("_ZN3Foo3barEP3Foo"));
    }

    #[test]
    fn test_const_ref_param() {
        // operator+(const Foo&) → _ZNK3FooplERK3Foo
        let m = mangle_function(
            &[],
            Some("Foo"),
            MethodName::Operator("pl"),
            &["const Foo&".to_string()],
            true,
        );
        assert_eq!(m.as_deref(), Some("_ZNK3FooplERK3Foo"));
    }

    #[test]
    fn test_rvalue_ref() {
        assert_eq!(mangle_type("Foo&&").as_deref(), Some("O3Foo"));
    }

    #[test]
    fn test_double_pointer() {
        assert_eq!(mangle_type("int**").as_deref(), Some("PPi"));
    }

    #[test]
    fn test_builtin_types() {
        assert_eq!(mangle_type("void").as_deref(), Some("v"));
        assert_eq!(mangle_type("bool").as_deref(), Some("b"));
        assert_eq!(mangle_type("unsigned long long").as_deref(), Some("y"));
        assert_eq!(mangle_type("size_t").as_deref(), Some("m"));
        assert_eq!(mangle_type("const char*").as_deref(), Some("PKc"));
        assert_eq!(mangle_type("...").as_deref(), Some("z"));
    }

    #[test]
    fn test_array_type() {
        assert_eq!(mangle_type("int[2][3]").as_deref(), Some("A2_A3_i"));
        assert_eq!(mangle_type("int[]").as_deref(), Some("Pi"));
    }

    #[test]
    fn test_qualified_param_type() {
        assert_eq!(mangle_type("ns::Foo*").as_deref(), Some("PN2ns3FooE"));
        assert_eq!(mangle_type("::Global&").as_deref(), Some("R6Global"));
    }

    #[test]
    fn test_unmangleable() {
        assert_eq!(mangle_type("std::vector<int>"), None);
        assert_eq!(mangle_type("long double"), None);
        assert_eq!(mangle_type(""), None);
    }

    #[test]
    fn test_mangle_function_param_failure() {
        let m = mangle_function(
            &[],
            Some("Foo"),
            MethodName::Named("bar".to_string()),
            &["std::vector<int>".to_string()],
            false,
        );
        assert_eq!(m, None);
    }

    #[test]
    fn test_qualified_name_only() {
        let q = mangle_qualified_name(&ns(&["ns"]), Some("Foo"), MethodName::Named("bar".to_string()));
        assert_eq!(q, "N2ns3Foo3barE");
    }
}
