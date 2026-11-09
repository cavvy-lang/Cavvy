//! 内置类型/方法声明库（`caylibs/__builtins__`）的加载与查询。
//!
//! 编译器本身在语义分析器中硬编码了 `String` / `array` / 基本类型等内建能力。
//! 为了让外部编辑器分析器（以及本项目 LSP）不必跟着编译器版本硬编码，仓库在
//! `caylibs/__builtins__/` 下维护一份 **Cavvy 源码形式的签名存根**，本模块负责
//! 读取 `index.json`、解析存根，并暴露补全 / 悬停 / 跳转所需的成员信息。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::ast::Modifier;
use crate::lexer;
use crate::parser;
use crate::types::{Type, substitute_type_params};

use super::util::{BytePos, LocatedSpan, Span, TextFile};

/// 存根目录名（约定中的特殊文件夹）。
pub const BUILTINS_DIR_NAME: &str = "__builtins__";

#[derive(Debug, Clone)]
pub struct ParamSig {
    pub name: String,
    pub ty: Type,
    pub is_varargs: bool,
}

impl ParamSig {
    pub fn display(&self) -> String {
        if self.is_varargs {
            format!(
                "{}... {}",
                super::builtins::display_type(&self.ty),
                self.name
            )
        } else {
            format!("{} {}", super::builtins::display_type(&self.ty), self.name)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinMemberKind {
    Field,
    Method,
}

/// 一个内建成员的只读视图（已按接收者类型做泛型替换）。
#[derive(Debug, Clone)]
pub struct BuiltinMember {
    pub owner: String,
    pub name: String,
    pub kind: BuiltinMemberKind,
    pub ty: Type,
    pub params: Vec<ParamSig>,
    pub is_static: bool,
    pub documentation: String,
    pub location: LocatedSpan,
    pub label: String,
    pub is_varargs: bool,
}

impl BuiltinMember {
    pub fn is_method(&self) -> bool {
        self.kind == BuiltinMemberKind::Method
    }

    pub fn signature(&self) -> String {
        self.label.clone()
    }
}

#[derive(Debug, Clone)]
pub struct BuiltinField {
    pub name: String,
    pub ty: Type,
    pub documentation: String,
    pub location: LocatedSpan,
    pub is_static: bool,
}

#[derive(Debug, Clone)]
pub struct BuiltinMethod {
    pub name: String,
    pub params: Vec<ParamSig>,
    pub return_type: Type,
    pub is_static: bool,
    pub documentation: String,
    pub location: LocatedSpan,
    pub label: String,
    pub owner: String,
}

impl BuiltinMethod {
    pub fn arity(&self) -> usize {
        self.params.len()
    }

    pub fn accepts_arity(&self, n: usize) -> bool {
        if self.params.last().map(|p| p.is_varargs).unwrap_or(false) {
            n + 1 >= self.params.len()
        } else {
            n == self.params.len()
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuiltinType {
    pub canonical: String,
    pub stub_name: String,
    pub file: String,
    pub type_params: Vec<String>,
    pub documentation: String,
    pub location: LocatedSpan,
    pub fields: Vec<BuiltinField>,
    pub methods: Vec<BuiltinMethod>,
}

#[derive(Debug, Clone, Default)]
pub struct BuiltinIndex {
    pub types: HashMap<String, BuiltinType>,
    pub functions: Vec<BuiltinMethod>,
    pub function_stub: Option<String>,
    pub root: Option<PathBuf>,
    pub loaded: bool,
    /// 加载过程中的非致命错误（例如某个存根解析失败），用于日志排查。
    pub errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    types: Vec<ManifestType>,
    #[serde(default)]
    functions: Option<ManifestFunctions>,
}

#[derive(Debug, Deserialize)]
struct ManifestType {
    builtin: String,
    stub: String,
    file: String,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ManifestFunctions {
    stub: String,
    file: String,
}

impl BuiltinIndex {
    /// 在常见位置查找 `caylibs/__builtins__` 并加载。找不到时返回空索引，
    /// 调用方仍可依靠语言核心的硬编码回退工作。
    pub fn load(workspace_roots: &[PathBuf]) -> Self {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(dir) = std::env::var("CAY_BUILTINS_DIR") {
            let p = PathBuf::from(dir);
            candidates.push(
                if p.file_name()
                    .map(|n| n == BUILTINS_DIR_NAME)
                    .unwrap_or(false)
                {
                    p
                } else {
                    p.join(BUILTINS_DIR_NAME)
                },
            );
        }
        for root in workspace_roots {
            candidates.push(root.join("caylibs").join(BUILTINS_DIR_NAME));
            if let Some(parent) = root.parent() {
                candidates.push(parent.join("caylibs").join(BUILTINS_DIR_NAME));
            }
        }
        if let Ok(cwd) = std::env::current_dir() {
            candidates.push(cwd.join("caylibs").join(BUILTINS_DIR_NAME));
        }
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                candidates.push(dir.join("caylibs").join(BUILTINS_DIR_NAME));
            }
        }
        candidates.push(PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/caylibs/__builtins__"
        )));

        for candidate in candidates {
            if candidate.is_dir() {
                return Self::load_from_root(&candidate);
            }
        }
        Self::fallback()
    }

    /// 从指定目录加载。目录不需要位于仓库内。
    pub fn load_from_root(root: &Path) -> Self {
        let mut index = Self {
            root: Some(root.to_path_buf()),
            loaded: true,
            ..Default::default()
        };

        let manifest_path = root.join("index.json");
        let manifest = std::fs::read_to_string(&manifest_path)
            .ok()
            .and_then(|s| serde_json::from_str::<Manifest>(&s).ok());

        if let Some(manifest) = manifest {
            for entry in manifest.types {
                let path = root.join(&entry.file);
                match Self::parse_stub_file(&path, &entry.builtin, &entry.stub) {
                    Ok(bt) => {
                        index.types.insert(entry.builtin.clone(), bt);
                    }
                    Err(e) => index.errors.push(format!(
                        "加载内置存根失败 {} -> {}: {}",
                        entry.file, entry.stub, e
                    )),
                }
            }
            if let Some(functions) = manifest.functions {
                let path = root.join(&functions.file);
                match Self::parse_functions_file(&path, &functions.stub) {
                    Ok(mut funcs) => {
                        index.function_stub = Some(functions.stub);
                        index.functions.append(&mut funcs);
                    }
                    Err(e) => index
                        .errors
                        .push(format!("加载内置函数存根失败 {}: {}", functions.file, e)),
                }
            }
            return index;
        }

        // 没有 index.json：扫描目录，按文件名词干推断规范名。
        index.errors.push(format!(
            "{} 缺少 index.json，回退到按文件名扫描",
            root.display()
        ));
        let mut entries: Vec<PathBuf> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e == "cay").unwrap_or(false))
            .collect();
        entries.sort();
        for path in entries {
            let builtin = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let stub = if builtin == "functions" {
                "Builtins".to_string()
            } else if builtin.starts_with("Cay") {
                builtin.clone()
            } else {
                builtin.clone()
            };
            if builtin == "functions" {
                if let Ok(mut funcs) = Self::parse_functions_file(&path, &stub) {
                    index.function_stub = Some(stub);
                    index.functions.append(&mut funcs);
                }
                continue;
            }
            let stub_name = if index
                .types
                .get(&builtin)
                .map(|t| t.stub_name == stub)
                .unwrap_or(false)
            {
                stub
            } else {
                stub
            };
            if let Ok(bt) = Self::parse_stub_file(&path, &builtin, &stub_name) {
                index.types.insert(builtin, bt);
            }
        }
        index
    }

    /// 构建不依赖仓库文件的兜底索引（只覆盖最常用、编译器明确支持的内建）。
    pub fn fallback() -> Self {
        let mut index = Self {
            loaded: false,
            ..Default::default()
        };
        let dummy = LocatedSpan::new("<builtin>", Span::point(BytePos::new(0, 0)));
        let mut array = BuiltinType {
            canonical: "array".to_string(),
            stub_name: "array".to_string(),
            file: "<builtin>".to_string(),
            type_params: vec!["T".to_string()],
            documentation: "Cavvy 数组类型；`T[]` 等价于 `array<T>`。".to_string(),
            location: dummy.clone(),
            fields: vec![BuiltinField {
                name: "length".to_string(),
                ty: Type::Int32,
                documentation: "数组元素个数。".to_string(),
                location: dummy.clone(),
                is_static: false,
            }],
            methods: Vec::new(),
        };
        array.methods.push(BuiltinMethod {
            name: "length".to_string(),
            params: Vec::new(),
            return_type: Type::Int32,
            is_static: false,
            documentation: "数组元素个数（与 `.length` 字段等价）。".to_string(),
            location: dummy.clone(),
            label: "int length()".to_string(),
            owner: "array".to_string(),
        });
        index.types.insert("array".to_string(), array);

        let mut string = BuiltinType {
            canonical: "String".to_string(),
            stub_name: "CayString".to_string(),
            file: "<builtin>".to_string(),
            type_params: Vec::new(),
            documentation: "Cavvy 内建字符串类型。".to_string(),
            location: dummy.clone(),
            fields: Vec::new(),
            methods: Vec::new(),
        };
        let simple_string_methods: &[(&str, Vec<ParamSig>, Type, &str)] = &[
            ("length", vec![], Type::Int32, "返回字符串长度。"),
            (
                "substring",
                vec![ParamSig {
                    name: "beginIndex".to_string(),
                    ty: Type::Int32,
                    is_varargs: false,
                }],
                Type::String,
                "返回从 beginIndex 到末尾的子串。",
            ),
            (
                "charAt",
                vec![ParamSig {
                    name: "index".to_string(),
                    ty: Type::Int32,
                    is_varargs: false,
                }],
                Type::Char,
                "返回指定索引处的字符。",
            ),
            ("isEmpty", vec![], Type::Bool, "字符串是否为空。"),
            ("trim", vec![], Type::String, "去除首尾空白。"),
            ("toUpperCase", vec![], Type::String, "转为大写。"),
            ("toLowerCase", vec![], Type::String, "转为小写。"),
            ("hashCode", vec![], Type::Int32, "字符串哈希值。"),
        ];
        for (name, params, ret, doc) in simple_string_methods {
            string.methods.push(BuiltinMethod {
                name: name.to_string(),
                params: params.clone(),
                return_type: ret.clone(),
                is_static: false,
                documentation: doc.to_string(),
                location: dummy.clone(),
                label: format!(
                    "{} {}({})",
                    display_type(ret),
                    name,
                    params
                        .iter()
                        .map(|p| p.display())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                owner: "String".to_string(),
            });
        }
        index.types.insert("String".to_string(), string);

        let mut object = BuiltinType {
            canonical: "Object".to_string(),
            stub_name: "Object".to_string(),
            file: "<builtin>".to_string(),
            type_params: Vec::new(),
            documentation: "所有引用类型的根类。".to_string(),
            location: dummy.clone(),
            fields: Vec::new(),
            methods: Vec::new(),
        };
        for (name, ret, doc) in [
            ("hashCode", Type::Int32, "对象身份哈希值。"),
            ("equals", Type::Bool, "对象身份相等性比较。"),
        ] {
            let params = if name == "equals" {
                vec![ParamSig {
                    name: "other".to_string(),
                    ty: Type::Object("Object".to_string()),
                    is_varargs: false,
                }]
            } else {
                vec![]
            };
            object.methods.push(BuiltinMethod {
                name: name.to_string(),
                params: params.clone(),
                return_type: ret.clone(),
                is_static: false,
                documentation: doc.to_string(),
                location: dummy.clone(),
                label: format!(
                    "{} {}({})",
                    display_type(&ret),
                    name,
                    params
                        .iter()
                        .map(|p| p.display())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                owner: "Object".to_string(),
            });
        }
        index.types.insert("Object".to_string(), object);
        index
    }

    fn parse_stub_file(
        path: &Path,
        canonical: &str,
        stub_name: &str,
    ) -> Result<BuiltinType, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("无法读取 {}: {}", path.display(), e))?;
        let text = TextFile::new(path.to_string_lossy().into_owned(), content.clone());
        let tokens = lexer::lex(&content).map_err(|e| format!("词法错误: {:?}", e))?;
        let ast = parser::parse(tokens).map_err(|e| format!("语法错误: {:?}", e))?;

        let class = ast
            .classes
            .iter()
            .find(|c| c.name == stub_name)
            .or_else(|| ast.classes.first())
            .ok_or_else(|| format!("{} 中没有类声明", path.display()))?;

        let file = path.to_string_lossy().into_owned();
        let class_name_span = text
            .name_after_keyword(BytePos::from_source_location(&class.loc), "class")
            .map(|(s, _)| s)
            .unwrap_or_else(|| {
                Span::new(
                    BytePos::from_source_location(&class.loc),
                    BytePos::from_source_location(&class.loc),
                )
            });
        let documentation = text.doc_comment_before(class.loc.line.saturating_sub(1) as u32);

        let mut fields = Vec::new();
        for (name, field) in class_fields(class) {
            let span = text
                .find_last_word_before(
                    BytePos::from_source_location(&field.loc),
                    &name,
                    &[';', '=', ','],
                )
                .or_else(|| {
                    text.find_word_after(
                        BytePos::from_source_location(&field.loc),
                        &name,
                        &[';', '=', ','],
                    )
                })
                .unwrap_or_else(|| {
                    Span::new(
                        BytePos::from_source_location(&field.loc),
                        BytePos::from_source_location(&field.loc),
                    )
                });
            fields.push(BuiltinField {
                name,
                ty: normalize_builtin_type(&field.field_type),
                documentation: text.doc_comment_before(field.loc.line.saturating_sub(1) as u32),
                location: LocatedSpan::new(&file, span),
                is_static: field.modifiers.contains(&Modifier::Static),
            });
        }

        let mut methods = Vec::new();
        for method in class_methods(class) {
            let name_span = method_name_span(&text, &method.loc, &method.name);
            let mut params = Vec::new();
            for p in &method.params {
                params.push(ParamSig {
                    name: p.name.clone(),
                    ty: normalize_builtin_type(&p.param_type),
                    is_varargs: p.is_varargs,
                });
            }
            let return_type = normalize_builtin_type(&method.return_type);
            let label = format!(
                "{}{} {}({})",
                if method.modifiers.contains(&Modifier::Static) {
                    "static "
                } else {
                    ""
                },
                display_type(&return_type),
                method.name,
                params
                    .iter()
                    .map(|p| p.display())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            methods.push(BuiltinMethod {
                name: method.name.clone(),
                params,
                return_type,
                is_static: method.modifiers.contains(&Modifier::Static),
                documentation: text.doc_comment_before(method.loc.line.saturating_sub(1) as u32),
                location: LocatedSpan::new(&file, name_span),
                label,
                owner: canonical.to_string(),
            });
        }

        Ok(BuiltinType {
            canonical: canonical.to_string(),
            stub_name: stub_name.to_string(),
            file,
            type_params: class.type_params.iter().map(|p| p.name.clone()).collect(),
            documentation,
            location: LocatedSpan::new(text.path.clone(), class_name_span),
            fields,
            methods,
        })
    }

    fn parse_functions_file(path: &Path, stub_name: &str) -> Result<Vec<BuiltinMethod>, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("无法读取 {}: {}", path.display(), e))?;
        let text = TextFile::new(path.to_string_lossy().into_owned(), content.clone());
        let tokens = lexer::lex(&content).map_err(|e| format!("词法错误: {:?}", e))?;
        let ast = parser::parse(tokens).map_err(|e| format!("语法错误: {:?}", e))?;
        let class = ast
            .classes
            .iter()
            .find(|c| c.name == stub_name)
            .or_else(|| ast.classes.first())
            .ok_or_else(|| format!("{} 中没有类声明", path.display()))?;
        let file = path.to_string_lossy().into_owned();
        let mut methods = Vec::new();
        for method in class_methods(class) {
            if !method.modifiers.contains(&Modifier::Static)
                && method.modifiers.contains(&Modifier::Private)
            {
                continue;
            }
            let name_span = method_name_span(&text, &method.loc, &method.name);
            let mut params = Vec::new();
            for p in &method.params {
                params.push(ParamSig {
                    name: p.name.clone(),
                    ty: normalize_builtin_type(&p.param_type),
                    is_varargs: p.is_varargs,
                });
            }
            let return_type = normalize_builtin_type(&method.return_type);
            let label = format!(
                "{} {}({})",
                display_type(&return_type),
                method.name,
                params
                    .iter()
                    .map(|p| p.display())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            methods.push(BuiltinMethod {
                name: method.name.clone(),
                params,
                return_type,
                is_static: true,
                documentation: text.doc_comment_before(method.loc.line.saturating_sub(1) as u32),
                location: LocatedSpan::new(&file, name_span),
                label,
                owner: "Builtins".to_string(),
            });
        }
        Ok(methods)
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded && (self.root.is_some() || !self.types.is_empty())
    }

    /// 按类型查找对应的内建存根。`T[]` 归一化为 `array<T>`。
    pub fn builtin_type_for(&self, ty: &Type) -> Option<&BuiltinType> {
        match ty {
            Type::String => self.types.get("String"),
            Type::Int32 => self.types.get("int"),
            Type::Int64 => self.types.get("long"),
            Type::Float32 => self.types.get("float"),
            Type::Float64 => self.types.get("double"),
            Type::Bool => self.types.get("bool"),
            Type::Char => self.types.get("char"),
            Type::Array(_) => self.types.get("array"),
            Type::Object(name) => self.get_by_name(name),
            Type::Generic(name, _) => self.get_by_name(name),
            _ => None,
        }
    }

    pub fn get_by_name(&self, name: &str) -> Option<&BuiltinType> {
        if let Some(bt) = self.types.get(name) {
            return Some(bt);
        }
        let base = name.split('<').next().unwrap_or(name).trim();
        if let Some(bt) = self.types.get(base) {
            return Some(bt);
        }
        // 忽略命名空间前缀
        if let Some(last) = base.rsplit("::").next() {
            if let Some(bt) = self.types.get(last) {
                return Some(bt);
            }
        }
        // Cay 前缀映射
        let mapped = match base {
            "CayString" => "String",
            "CayInt" => "int",
            "CayLong" => "long",
            "CayFloat" => "float",
            "CayDouble" => "double",
            "CayBool" => "bool",
            "CayChar" => "char",
            "CayInteger" => "Integer",
            other => other,
        };
        self.types.get(mapped)
    }

    /// 返回某个接收者类型可见的内建成员（字段 + 方法），已完成泛型替换。
    pub fn members_for(&self, ty: &Type) -> Vec<BuiltinMember> {
        let Some(bt) = self.builtin_type_for(ty) else {
            return Vec::new();
        };
        let mut mapping: HashMap<String, Type> = HashMap::new();
        match ty {
            Type::Array(inner) => {
                if let Some(tp) = bt.type_params.first() {
                    mapping.insert(tp.clone(), (**inner).clone());
                }
            }
            Type::Generic(_, args) => {
                for (tp, arg) in bt.type_params.iter().zip(args.iter()) {
                    mapping.insert(tp.clone(), arg.clone());
                }
            }
            _ => {}
        }
        let owner = bt.canonical.clone();
        let mut out = Vec::new();
        for field in &bt.fields {
            out.push(BuiltinMember {
                owner: owner.clone(),
                name: field.name.clone(),
                kind: BuiltinMemberKind::Field,
                ty: substitute_builtin(&field.ty, &mapping),
                params: Vec::new(),
                is_static: field.is_static,
                documentation: field.documentation.clone(),
                location: field.location.clone(),
                label: format!(
                    "{} {}",
                    display_type(&substitute_builtin(&field.ty, &mapping)),
                    field.name
                ),
                is_varargs: false,
            });
        }
        for method in &bt.methods {
            out.push(BuiltinMember {
                owner: owner.clone(),
                name: method.name.clone(),
                kind: BuiltinMemberKind::Method,
                ty: substitute_builtin(&method.return_type, &mapping),
                params: method
                    .params
                    .iter()
                    .map(|p| ParamSig {
                        name: p.name.clone(),
                        ty: substitute_builtin(&p.ty, &mapping),
                        is_varargs: p.is_varargs,
                    })
                    .collect(),
                is_static: method.is_static,
                documentation: method.documentation.clone(),
                location: method.location.clone(),
                label: signature_label(
                    method.is_static,
                    &substitute_builtin(&method.return_type, &mapping),
                    &method.name,
                    &method
                        .params
                        .iter()
                        .map(|p| ParamSig {
                            name: p.name.clone(),
                            ty: substitute_builtin(&p.ty, &mapping),
                            is_varargs: p.is_varargs,
                        })
                        .collect::<Vec<_>>(),
                ),
                is_varargs: method.params.last().map(|p| p.is_varargs).unwrap_or(false),
            });
        }

        // 数组的 `.length()` 是 `.length` 字段的语法糖，存根里只有字段；
        // 为 LSP 补全/签名帮助补一个合成方法。
        if bt.canonical == "array" {
            if let Some(field) = out.iter().find(|m| m.name == "length").cloned() {
                out.push(BuiltinMember {
                    owner: owner.clone(),
                    name: "length".to_string(),
                    kind: BuiltinMemberKind::Method,
                    ty: Type::Int32,
                    params: Vec::new(),
                    is_static: false,
                    documentation: field.documentation.clone(),
                    location: field.location.clone(),
                    label: "int length()".to_string(),
                    is_varargs: false,
                });
            }
        }
        out
    }

    /// 返回某个内建/已注册类型的静态成员。
    pub fn static_members_for(&self, type_name: &str) -> Vec<BuiltinMember> {
        let Some(bt) = self.get_by_name(type_name) else {
            return Vec::new();
        };
        let owner = bt.canonical.clone();
        bt.methods
            .iter()
            .filter(|m| m.is_static)
            .map(|m| BuiltinMember {
                owner: owner.clone(),
                name: m.name.clone(),
                kind: BuiltinMemberKind::Method,
                ty: m.return_type.clone(),
                params: m.params.clone(),
                is_static: true,
                documentation: m.documentation.clone(),
                location: m.location.clone(),
                label: signature_label(true, &m.return_type, &m.name, &m.params),
                is_varargs: m.params.last().map(|p| p.is_varargs).unwrap_or(false),
            })
            .collect()
    }

    /// 所有全局内建函数（`print` / `println` / `readInt` ...）。
    pub fn global_functions(&self) -> &[BuiltinMethod] {
        &self.functions
    }

    pub fn find_global_function(&self, name: &str, arity: usize) -> Option<&BuiltinMethod> {
        self.functions
            .iter()
            .find(|m| m.name == name && m.accepts_arity(arity))
    }

    pub fn global_function_symbol(&self, name: &str) -> Option<&BuiltinMethod> {
        self.functions.iter().find(|m| m.name == name)
    }

    pub fn all_type_names(&self) -> impl Iterator<Item = (&String, &BuiltinType)> {
        self.types.iter()
    }

    /// 把 `String` / `array` 等语言内建名归一化为存根规范名。
    pub fn canonical_name(&self, name: &str) -> Option<String> {
        self.get_by_name(name).map(|t| t.canonical.clone())
    }
}

fn class_fields(class: &crate::ast::ClassDecl) -> Vec<(String, crate::ast::FieldDecl)> {
    let mut out = Vec::new();
    for member in &class.members {
        if let crate::ast::ClassMember::Field(f) = member {
            out.push((f.name.clone(), f.clone()));
        }
    }
    out
}

fn class_methods(class: &crate::ast::ClassDecl) -> Vec<crate::ast::MethodDecl> {
    let mut out = Vec::new();
    for member in &class.members {
        if let crate::ast::ClassMember::Method(m) = member {
            out.push(m.clone());
        }
    }
    out
}

fn method_name_span(
    text: &TextFile,
    loc: &crate::miette_diagnostic::SourceLocation,
    name: &str,
) -> Span {
    let start = BytePos::from_source_location(loc);
    text.find_word_after(start, name, &['(', ';'])
        .or_else(|| text.find_last_word_before(start, name, &['(', ';']))
        .unwrap_or_else(|| Span::point(start))
}

/// 把存根中的 `CayString` / `CayInt` 等类名映射回语言内建类型。
pub fn normalize_builtin_type(ty: &Type) -> Type {
    match ty {
        Type::Object(name) => match name.as_str() {
            "CayString" => Type::String,
            "CayInt" => Type::Int32,
            "CayLong" => Type::Int64,
            "CayFloat" => Type::Float32,
            "CayDouble" => Type::Float64,
            "CayBool" => Type::Bool,
            "CayChar" => Type::Char,
            "CayInteger" => Type::Object("Integer".to_string()),
            _ => ty.clone(),
        },
        Type::Array(inner) => Type::Array(Box::new(normalize_builtin_type(inner))),
        Type::Pointer(inner) => Type::Pointer(Box::new(normalize_builtin_type(inner))),
        Type::Generic(name, args) => Type::Generic(
            name.clone(),
            args.iter().map(normalize_builtin_type).collect(),
        ),
        _ => ty.clone(),
    }
}

fn substitute_builtin(ty: &Type, mapping: &HashMap<String, Type>) -> Type {
    let normalized = normalize_builtin_type(ty);
    substitute_type_params(&normalized, mapping)
}

pub fn signature_label(
    is_static: bool,
    return_type: &Type,
    name: &str,
    params: &[ParamSig],
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

/// 以 Cavvy 用户可见的拼写显示类型（而不是内部 `Object("CayString")`）。
pub fn display_type(ty: &Type) -> String {
    match ty {
        Type::Object(name) => match name.as_str() {
            "CayString" => "String".to_string(),
            "CayInt" => "int".to_string(),
            "CayLong" => "long".to_string(),
            "CayFloat" => "float".to_string(),
            "CayDouble" => "double".to_string(),
            "CayBool" => "bool".to_string(),
            "CayChar" => "char".to_string(),
            "CayInteger" => "Integer".to_string(),
            _ => name.clone(),
        },
        Type::Generic(name, args) if name == "array" && !args.is_empty() => {
            format!("{}[]", display_type(&args[0]))
        }
        Type::Generic(name, args) => format!(
            "{}<{}>",
            name,
            args.iter().map(display_type).collect::<Vec<_>>().join(", ")
        ),
        Type::Array(inner) => format!("{}[]", display_type(inner)),
        Type::Pointer(inner) => format!("{}*", display_type(inner)),
        Type::Function(f) => format!(
            "fn({}) -> {}",
            f.params
                .iter()
                .map(display_type)
                .collect::<Vec<_>>()
                .join(", "),
            display_type(&f.return_type)
        ),
        other => other.display_name(),
    }
}

/// 把存根文件整体当作一个虚拟库解析时，返回 `(类型规范名, 存根类型)`。
pub fn load_all(root: &Path) -> Vec<BuiltinType> {
    let index = BuiltinIndex::load_from_root(root);
    let mut out: Vec<BuiltinType> = index.types.values().cloned().collect();
    out.sort_by(|a, b| a.canonical.cmp(&b.canonical));
    out
}

/// 判断一个路径是否位于内置存根目录中（用于避免把存根当作普通用户代码索引）。
pub fn is_builtin_stub_path(path: &Path) -> bool {
    path.components()
        .any(|c| c.as_os_str() == BUILTINS_DIR_NAME)
}

/// 读取存根目录的 `index.json` 原始内容（供文档/调试使用）。
pub fn manifest_text(root: &Path) -> Option<String> {
    std::fs::read_to_string(root.join("index.json")).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_builtins() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/caylibs/__builtins__"))
    }

    #[test]
    fn loads_manifest_and_stubs() {
        let index = BuiltinIndex::load_from_root(&repo_builtins());
        assert!(index.errors.is_empty(), "存根加载错误: {:?}", index.errors);
        assert!(index.is_loaded());
        assert!(index.types.contains_key("String"));
        assert!(index.types.contains_key("array"));

        let array = index.types.get("array").expect("array 存根");
        assert!(array.fields.iter().any(|f| f.name == "length"));

        let string = index.types.get("String").expect("String 存根");
        assert!(string.methods.iter().any(|m| m.name == "substring"));
        assert!(string.methods.iter().any(|m| m.name == "length"));
    }

    #[test]
    fn array_members_substitute_element_type() {
        let index = BuiltinIndex::load_from_root(&repo_builtins());
        let members = index.members_for(&Type::Array(Box::new(Type::Int32)));
        let length = members
            .iter()
            .find(|m| m.name == "length" && m.kind == BuiltinMemberKind::Field)
            .expect("array<int>.length");
        assert_eq!(length.ty, Type::Int32);
        assert!(members.iter().any(|m| m.name == "length" && m.is_method()));
    }

    #[test]
    fn string_methods_normalize_cay_string() {
        let index = BuiltinIndex::load_from_root(&repo_builtins());
        let members = index.members_for(&Type::String);
        let substring = members
            .iter()
            .find(|m| m.name == "substring" && m.params.len() == 1)
            .expect("String.substring(int)");
        assert_eq!(substring.ty, Type::String);
        assert_eq!(substring.params[0].ty, Type::Int32);
    }

    #[test]
    fn global_functions_loaded() {
        let index = BuiltinIndex::load_from_root(&repo_builtins());
        assert!(index.find_global_function("println", 1).is_some());
        assert!(index.find_global_function("readInt", 0).is_some());
        assert!(index.find_global_function("printf", 2).is_some());
    }

    #[test]
    fn fallback_has_core_builtins() {
        let index = BuiltinIndex::fallback();
        assert!(index.types.contains_key("array"));
        let members = index.members_for(&Type::Array(Box::new(Type::String)));
        assert!(members.iter().any(|m| m.name == "length"));
    }

    /// 编译器语义层硬编码的内建能力必须都能在存根中找到；否则说明更新编译器
    /// 时忘记同步 `caylibs/__builtins__`，外部分析器会与编译器脱节。
    #[test]
    fn stubs_cover_compiler_hardcoded_builtins() {
        let index = BuiltinIndex::load_from_root(&repo_builtins());
        assert!(index.errors.is_empty(), "{:?}", index.errors);

        let string = index.types.get("String").expect("String 存根");
        for name in [
            "length",
            "substring",
            "indexOf",
            "lastIndexOf",
            "charAt",
            "replace",
            "isEmpty",
            "equals",
            "equalsIgnoreCase",
            "c_str",
            "startsWith",
            "endsWith",
            "trim",
            "toLowerCase",
            "toUpperCase",
            "contains",
            "compareTo",
            "hashCode",
            "valueOf",
        ] {
            assert!(
                string.methods.iter().any(|m| m.name == name),
                "String 存根缺少编译器支持的 {}",
                name
            );
        }

        let array = index.types.get("array").expect("array 存根");
        assert!(array.fields.iter().any(|f| f.name == "length"));

        let integers = index.types.get("Integer").expect("Integer 存根");
        assert!(integers.methods.iter().any(|m| m.name == "parseInt"));

        for primitive in ["int", "long", "float", "double", "bool", "char"] {
            let bt = index
                .types
                .get(primitive)
                .unwrap_or_else(|| panic!("缺少基本类型存根 {}", primitive));
            assert!(
                bt.methods.iter().any(|m| m.name == "toString"),
                "{} 缺少 toString",
                primitive
            );
        }

        for name in [
            "print",
            "println",
            "eprint",
            "eprintln",
            "printf",
            "readInt",
            "readLong",
            "readFloat",
            "readDouble",
            "readLine",
            "readChar",
            "exit",
            "panic",
            "abort",
        ] {
            assert!(
                index.functions.iter().any(|m| m.name == name),
                "functions 存根缺少 {}",
                name
            );
        }
    }
}
