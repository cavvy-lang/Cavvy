//! Itanium C++ ABI 名称改编实现
//!
//! 将 Cavvy 的泛型类/struct/enum 名称与方法签名编码为符合 Itanium ABI 的符号名，
//! 以便与 g++/clang 生成的 C++ 目标文件进行链接。
//!
//! 支持：
//! - 命名空间嵌套 (`N...E`)
//! - 模板/泛型实参列表 (`I<types>E`)
//! - 前缀与类型替换 (`S_`, `S0_`, `S1_`, ...)
//! - 基本类型编码 (`i`, `x`, `f`, `d`, `P...` 等)

use crate::codegen::specialization::{parse_type_str, split_top_level_type_args};
use crate::types::{Type, TypeRegistry};
use std::collections::HashMap;

/// Itanium ABI mangler。
///
/// 每个 mangled 符号拥有独立的替换表；同一符号内类前缀与参数类型共享替换候选，
/// 确保嵌套泛型（如 `MyVec<MyVec<int>>`）能使用 `S0_` 等缩写。
pub struct ItaniumMangler<'a> {
    type_registry: Option<&'a TypeRegistry>,
    class_namespaces: &'a HashMap<String, Vec<String>>,
    substitutions: Vec<String>,
    is_windows_target: bool,
}

impl<'a> ItaniumMangler<'a> {
    pub fn new(
        type_registry: Option<&'a TypeRegistry>,
        class_namespaces: &'a HashMap<String, Vec<String>>,
        is_windows_target: bool,
    ) -> Self {
        Self {
            type_registry,
            class_namespaces,
            substitutions: Vec::new(),
            is_windows_target,
        }
    }

    /// 生成完整的方法/构造函数/析构函数 mangled 名。
    ///
    /// 示例：
    /// - `MyNS::MyVec<int>::get(int)` -> `_ZN4MyNS5MyVecIiE3getEi`
    /// - `MyVec<int>::push_back(int)` -> `_ZN5MyVecIiE9push_backEi`
    /// - const 方法 `Foo::bar(int) const` -> `_ZNK3Foo3barEi`（`N` 后输出 `K`）
    pub fn mangle_method(
        &mut self,
        class_name: &str,
        method_name: &str,
        param_types: &[Type],
        is_constructor: bool,
        is_destructor: bool,
        is_const: bool,
    ) -> String {
        let mut result = String::from("_Z");
        let (ns, base, targs) = self.parse_class_name(class_name);
        // 记录 'N' 的写入位置：const 成员函数的 `K` 紧跟 `N` 之后
        let n_pos = result.len();
        let class_need_n =
            self.encode_nested_class(&mut result, &ns, &base, &targs, true);
        if is_const && class_need_n && result.as_bytes().get(n_pos) == Some(&b'N') {
            result.insert(n_pos + 1, 'K');
        }

        if is_constructor {
            result.push_str("C1");
        } else if is_destructor {
            result.push_str("D1");
        } else {
            result.push_str(&format!("{}{}", method_name.len(), method_name));
        }

        // 关闭方法名的 N...E 嵌套包装。
        if class_need_n {
            result.push('E');
        }

        // 参数类型
        if is_destructor || param_types.is_empty() {
            result.push('v');
        } else {
            for t in param_types {
                result.push_str(&self.mangle_type(t));
            }
        }

        result
    }

    /// 生成带方法级泛型类型实参的 mangled 名（Cavvy 扩展，无 C++ 对应物）。
    ///
    /// 方法级类型实参编码在方法名之后、嵌套包装闭合之前：
    /// `Result<int, String>::map<long>(fn)` -> `_ZN6ResultIiPcE3mapIxEEPFvvE`
    /// 调用点与定义点必须使用同一函数，保证名字一致。
    pub fn mangle_method_with_type_args(
        &mut self,
        class_name: &str,
        method_name: &str,
        method_type_args: &[Type],
        param_types: &[Type],
    ) -> String {
        let mut result = String::from("_Z");
        let (ns, base, targs) = self.parse_class_name(class_name);
        let class_need_n =
            self.encode_nested_class(&mut result, &ns, &base, &targs, true);

        result.push_str(&format!("{}{}", method_name.len(), method_name));
        if !method_type_args.is_empty() {
            result.push('I');
            for arg in method_type_args {
                result.push_str(&self.mangle_template_arg(arg));
            }
            result.push('E');
        }

        // 关闭方法名的 N...E 嵌套包装。
        if class_need_n {
            result.push('E');
        }

        // 参数类型
        if param_types.is_empty() {
            result.push('v');
        } else {
            for t in param_types {
                result.push_str(&self.mangle_type(t));
            }
        }

        result
    }

    /// 将类类型编码为 Itanium 嵌套名主体（不含 `_Z` 前缀，不含指针 `P`）。
    ///
    /// 示例：
    /// - `MyVec<int>` -> `5MyVecIiE`
    /// - `MyNS::MyVec<int>` -> `N4MyNS5MyVecIiE`
    pub fn mangle_class_type(&mut self, class_name: &str) -> String {
        let (ns, base, targs) = self.parse_class_name(class_name);
        let mut result = String::new();
        self.encode_nested_class(&mut result, &ns, &base, &targs, false);
        result
    }

    /// 将 Cavvy 类型编码为 Itanium ABI 参数类型字符串（对象类型按指针处理）。
    pub fn mangle_type(&mut self, ty: &Type) -> String {
        self.mangle_type_impl(ty, false)
    }

    /// 将 Cavvy 类型编码为 Itanium ABI 模板实参类型字符串（对象类型按类类型本身处理）。
    pub fn mangle_template_arg(&mut self, ty: &Type) -> String {
        self.mangle_type_impl(ty, true)
    }

    fn mangle_type_impl(&mut self, ty: &Type, as_template_arg: bool) -> String {
        match ty {
            Type::Void => "v".to_string(),
            Type::Int32 => "i".to_string(),
            Type::Int64 => "x".to_string(),
            Type::Float32 => "f".to_string(),
            Type::Float64 => "d".to_string(),
            Type::Bool => "b".to_string(),
            Type::String => "Pc".to_string(),
            Type::Char => "c".to_string(),
            Type::Object(name) => {
                let class = self.mangle_class_type(name);
                if as_template_arg {
                    class
                } else {
                    format!("P{}", class)
                }
            }
            Type::Struct(name) => {
                let class = self.mangle_class_type(name);
                if as_template_arg {
                    class
                } else {
                    format!("P{}", class)
                }
            }
            Type::Array(inner) => format!("P{}", self.mangle_type_impl(inner, false)),
            Type::Pointer(inner) => format!("P{}", self.mangle_type_impl(inner, false)),
            Type::Function(_) => "PFvvE".to_string(),
            Type::Auto => "v".to_string(),
            Type::CInt => "i".to_string(),
            Type::CUInt => "j".to_string(),
            Type::CLong => {
                if self.is_windows_target {
                    "i".to_string()
                } else {
                    "l".to_string()
                }
            }
            Type::CULong => {
                if self.is_windows_target {
                    "j".to_string()
                } else {
                    "m".to_string()
                }
            }
            Type::CShort => "s".to_string(),
            Type::CUShort => "t".to_string(),
            Type::CChar => "c".to_string(),
            Type::CUChar => "h".to_string(),
            Type::CFloat => "f".to_string(),
            Type::CDouble => "d".to_string(),
            Type::SizeT => {
                if self.is_windows_target {
                    "y".to_string()
                } else {
                    "m".to_string()
                }
            }
            Type::SSizeT => "x".to_string(),
            Type::UIntPtr => "y".to_string(),
            Type::IntPtr => "l".to_string(),
            Type::CVoid => "v".to_string(),
            Type::CBool => "b".to_string(),
            Type::GenericParam(_) => {
                if as_template_arg {
                    // 未替换的泛型参数在模板实参位置无法编码为具体类型；
                    // 回退为 void* 占位，避免生成非法 IR 符号。
                    "v".to_string()
                } else {
                    "Pc".to_string()
                }
            }
            Type::Generic(name, args) => {
                let class_name = format_generic_name(name, args);
                let class = self.mangle_class_type(&class_name);
                if as_template_arg {
                    class
                } else {
                    format!("P{}", class)
                }
            }
        }
    }

    // ============================================================
    // 内部辅助
    // ============================================================

    /// 把 `class_name` 解析为 (命名空间路径, 基础类名, 模板实参列表)。
    ///
    /// 支持显式命名空间（如 `std::vector<int>`）与隐式命名空间查找。
    fn parse_class_name(&self, class_name: &str) -> (Vec<String>, String, Vec<Type>) {
        let (ns_str, base_with_targs) = split_namespace(class_name);

        let mut ns: Vec<String> = if ns_str.is_empty() {
            Vec::new()
        } else {
            ns_str.split("::").map(|s| s.to_string()).collect()
        };

        let (base, targs) = if let Some(lt_pos) = base_with_targs.find('<') {
            let gt_pos = base_with_targs.rfind('>').unwrap_or(base_with_targs.len());
            let base = base_with_targs[..lt_pos].to_string();
            let args_str = &base_with_targs[lt_pos + 1..gt_pos];
            let args: Vec<Type> = split_top_level_type_args(args_str)
                .iter()
                .map(|s| parse_type_str(s.trim()))
                .collect();
            (base, args)
        } else {
            (base_with_targs.to_string(), Vec::new())
        };

        if ns.is_empty() {
            ns = self.lookup_namespace(&base);
        }

        (ns, base, targs)
    }

    /// 查找基础类名所在的命名空间路径。
    fn lookup_namespace(&self, base_name: &str) -> Vec<String> {
        if let Some(ns) = self.class_namespaces.get(base_name) {
            return ns.clone();
        }
        if let Some(registry) = self.type_registry {
            if let Some(qualified) = registry.namespace_aliases.get(base_name) {
                if let Some(ns) = self.class_namespaces.get(qualified) {
                    return ns.clone();
                }
            }
            if !registry.current_namespace.is_empty() {
                let qualified = format!("{}::{}", registry.current_namespace.join("::"), base_name);
                if let Some(ns) = self.class_namespaces.get(&qualified) {
                    return ns.clone();
                }
            }
        }
        Vec::new()
    }

    /// 编码嵌套类名到 `out`，返回是否使用了 `N...E` 包装。
    ///
    /// `for_method` 为 true 时表示正在构造方法名前缀，此时不追加末尾 `E`
    ///（由调用方在方法名后追加）。
    fn encode_nested_class(
        &mut self,
        out: &mut String,
        ns: &[String],
        base: &str,
        targs: &[Type],
        for_method: bool,
    ) -> bool {
        let full_key = class_key(ns, base, targs);
        if let Some(sub) = self.find_substitution(&full_key) {
            out.push_str(&sub);
            // 方法名中的类前缀永远是嵌套名的一部分；类类型仅在存在命名空间或模板实参时为嵌套名。
            return for_method || ns.len() + 1 > 1 || !targs.is_empty();
        }

        let name_parts: Vec<&str> = ns
            .iter()
            .map(|s| s.as_str())
            .chain(std::iter::once(base))
            .collect();
        // 方法名中的类前缀永远是嵌套名的一部分；类类型仅在存在命名空间或模板实参时用 N...E。
        let need_n = for_method || name_parts.len() > 1;
        if need_n {
            out.push('N');
        }

        let mut i = 0;
        while i < name_parts.len() {
            // 优先使用能覆盖最长前缀的替换
            let mut matched: Option<(usize, String)> = None;
            for j in (i..name_parts.len()).rev() {
                let key = parts_key(&name_parts[..=j]);
                if let Some(sub) = self.find_substitution(&key) {
                    matched = Some((j, sub));
                    break;
                }
            }

            if let Some((j, sub)) = matched {
                out.push_str(&sub);
                i = j + 1;
            } else {
                let name = name_parts[i];
                out.push_str(&format!("{}{}", name.len(), name));
                let key = parts_key(&name_parts[..=i]);
                self.add_substitution_candidate(&key);
                i += 1;
            }
        }

        if !targs.is_empty() {
            out.push('I');
            for arg in targs {
                out.push_str(&self.mangle_template_arg(arg));
            }
            out.push('E');
            self.add_substitution_candidate(&full_key);
        }

        if need_n && !for_method {
            out.push('E');
        }

        need_n
    }

    fn find_substitution(&self, key: &str) -> Option<String> {
        for (idx, candidate) in self.substitutions.iter().enumerate() {
            if candidate == key {
                return Some(encode_seq_id(idx));
            }
        }
        None
    }

    fn add_substitution_candidate(&mut self, key: &str) {
        if key.is_empty() {
            return;
        }
        if !self.substitutions.iter().any(|s| s == key) {
            self.substitutions.push(key.to_string());
        }
    }
}

/// 将替换序号编码为 Itanium ABI 的 `S<seq-id>_` 形式。
fn encode_seq_id(idx: usize) -> String {
    if idx == 0 {
        "S_".to_string()
    } else {
        let mut n = idx - 1;
        let mut digits = Vec::new();
        loop {
            digits.push(encode_base36_digit(n % 36));
            n /= 36;
            if n == 0 {
                break;
            }
        }
        digits.reverse();
        format!(
            "S{}_",
            digits.into_iter().collect::<String>()
        )
    }
}

fn encode_base36_digit(d: usize) -> char {
    match d {
        0..=9 => (b'0' + d as u8) as char,
        10..=35 => (b'A' + (d - 10) as u8) as char,
        _ => unreachable!(),
    }
}

/// 从字符串末尾开始查找不在 `<...>` 内的 `::`，拆分为命名空间与基础名。
fn split_namespace(s: &str) -> (&str, &str) {
    let mut depth = 0i32;
    let mut prev_char = '\0';
    let mut split_at = None;
    for (i, c) in s.char_indices().rev() {
        match c {
            '>' => depth += 1,
            '<' => depth -= 1,
            ':' => {
                if depth == 0 && prev_char == ':' {
                    split_at = Some(i);
                    break;
                }
            }
            _ => {}
        }
        prev_char = c;
    }
    if let Some(i) = split_at {
        (&s[..i], &s[i + 2..])
    } else {
        ("", s)
    }
}

fn parts_key(parts: &[&str]) -> String {
    parts.join("::")
}

fn class_key(ns: &[String], base: &str, targs: &[Type]) -> String {
    let mut parts = ns.to_vec();
    parts.push(base.to_string());
    let key = parts.join("::");
    if targs.is_empty() {
        key
    } else {
        let args: Vec<String> = targs.iter().map(|t| t.display_name()).collect();
        format!("{}<{}>", key, args.join(", "))
    }
}

fn format_generic_name(name: &str, args: &[Type]) -> String {
    let args_str: Vec<String> = args.iter().map(|t| t.display_name()).collect();
    format!("{}<{}>", name, args_str.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_mangler<'a>(namespaces: &'a HashMap<String, Vec<String>>) -> ItaniumMangler<'a> {
        ItaniumMangler::new(None, namespaces, false)
    }

    #[test]
    fn test_simple_generic_class_type() {
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        assert_eq!(m.mangle_class_type("MyVec<int>"), "5MyVecIiE");
    }

    #[test]
    fn test_namespaced_generic_class_type() {
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        assert_eq!(m.mangle_class_type("MyNS::MyVec<int>"), "N4MyNS5MyVecIiEE");
    }

    #[test]
    fn test_nested_generic_class_type() {
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_class_type("MyNS::MyVec<MyNS::MyVec<int>>"),
            "N4MyNS5MyVecINS0_IiEEEE"
        );
    }

    #[test]
    fn test_simple_generic_method() {
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_method("MyVec<int>", "get", &[Type::Int32], false, false, false),
            "_ZN5MyVecIiE3getEi"
        );
    }

    #[test]
    fn test_namespaced_generic_method() {
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_method("MyNS::MyVec<int>", "get", &[Type::Int32], false, false, false),
            "_ZN4MyNS5MyVecIiE3getEi"
        );
    }

    #[test]
    fn test_nested_generic_method() {
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        // 方法名的外层 N...E 闭合 E 位于方法名之后，因此类前缀只有 3 个 E。
        assert_eq!(
            m.mangle_method(
                "MyNS::MyVec<MyNS::MyVec<int>>",
                "get",
                &[Type::Int32],
                false,
                false,
                false
            ),
            "_ZN4MyNS5MyVecINS0_IiEEE3getEi"
        );
    }

    #[test]
    fn test_object_pointer_parameter_no_extra_e() {
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        // 全局类对象引用（指针）：P5MyVecIiE，末尾仅一个 E（来自模板实参列表）。
        assert_eq!(m.mangle_type(&Type::Object("MyVec<int>".to_string())), "P5MyVecIiE");
        // 命名空间类对象引用（指针）：PN4MyNS5MyVecIiEE，不额外追加 E。
        assert_eq!(
            m.mangle_type(&Type::Object("MyNS::MyVec<int>".to_string())),
            "PN4MyNS5MyVecIiEE"
        );
    }

    #[test]
    fn test_generic_as_template_arg() {
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        // 模板实参中的泛型类类型不应带 P...E 指针包装。
        assert_eq!(
            m.mangle_template_arg(&Type::Generic("MyVec".to_string(), vec![Type::Int32])),
            "5MyVecIiE"
        );
        assert_eq!(
            m.mangle_template_arg(&Type::Generic("MyNS::MyVec".to_string(), vec![Type::Int32])),
            "N4MyNS5MyVecIiEE"
        );
    }

    #[test]
    fn test_global_generic_constructor_destructor() {
        let namespaces = HashMap::new();
        // 无命名空间的模板类成员仍须用 N...E 包装。
        // 每个 mangled 符号使用独立替换表，因此分别创建 mangler。
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_method("MyVec<int>", "C1", &[Type::Int32], true, false, false),
            "_ZN5MyVecIiEC1Ei"
        );
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_method("MyVec<int>", "D1", &[], false, true, false),
            "_ZN5MyVecIiED1Ev"
        );
    }

    #[test]
    fn test_push_back_const_ref() {
        // Cavvy 中对象以指针传递，因此参数类型编码为 P... 而非 R...
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_method("MyNS::MyVec<int>", "push_back", &[Type::Int32], false, false, false),
            "_ZN4MyNS5MyVecIiE9push_backEi"
        );
    }

    #[test]
    fn test_destructor() {
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_method("MyNS::MyVec<int>", "D1", &[], false, true, false),
            "_ZN4MyNS5MyVecIiED1Ev"
        );
    }

    #[test]
    fn test_const_method_k_marker() {
        // const 成员函数在 N 后输出 K：Foo::bar(int) const -> _ZNK3Foo3barEi
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_method("Foo", "bar", &[Type::Int32], false, false, true),
            "_ZNK3Foo3barEi"
        );
        // 命名空间内的 const 方法：K 同样紧跟最外层 N
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_method("ns::Foo", "bar", &[], false, false, true),
            "_ZNK2ns3Foo3barEv"
        );
        // 非 const 对照：无 K
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_method("Foo", "bar", &[Type::Int32], false, false, false),
            "_ZN3Foo3barEi"
        );
    }

    #[test]
    fn test_constructor() {
        let namespaces = HashMap::new();
        let mut m = new_mangler(&namespaces);
        assert_eq!(
            m.mangle_method("MyNS::MyVec<int>", "C1", &[Type::Int32], true, false, false),
            "_ZN4MyNS5MyVecIiEC1Ei"
        );
    }
}
