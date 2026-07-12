// cay-ast: Cavvy AST printer
// 以人类可读文本或 JSON 格式输出 .cay 文件的抽象语法树。

mod debug_common;

use cavvy::ast::{
    ClassDecl, ClassMember, EnumDecl, ExternDecl, FieldDecl, InterfaceDecl, MethodDecl, Program,
    StructDecl, TopLevelFunction,
};
use debug_common::*;
use std::process;

const VERSION: &str = env!("CAY_AST_VERSION");

fn main() {
    let args = args();
    let (options, file_path) = match parse_common_args(&args) {
        Ok((opts, path)) => {
            if opts.show_version {
                print_version("cay-ast", VERSION);
                return;
            }
            if opts.show_help {
                print_usage("cay-ast", VERSION, &[]);
                return;
            }
            (opts, path)
        }
        Err(e) => exit_with_error(&e),
    };

    let source = match read_source(&file_path) {
        Ok(s) => s,
        Err(e) => exit_with_error(&e),
    };

    let processed = match preprocess_source(&source, &file_path, options.no_preprocess) {
        Ok(s) => s,
        Err(e) => exit_with_error(&e),
    };

    let mut ast = match parse_source(&processed) {
        Ok(a) => a,
        Err(e) => {
            print_error(&e, &processed, &file_path);
            process::exit(1);
        }
    };

    if !options.no_semantics {
        let mut analyzer = cavvy::semantic::SemanticAnalyzer::new();
        analyzer.set_current_file(Some(file_path.clone()));
        match analyzer.analyze(ast) {
            Ok(a) => ast = a,
            Err(e) => {
                print_error(&e, &processed, &file_path);
                process::exit(1);
            }
        }
    }

    if options.json_output {
        match serde_json::to_string_pretty(&ast) {
            Ok(json) => println!("{}", json),
            Err(e) => exit_with_error(&format!("JSON 序列化失败: {}", e)),
        }
    } else {
        print_ast_pretty(&ast, &options, &file_path);
    }
}

fn print_ast_pretty(ast: &Program, options: &CommonOptions, file_path: &str) {
    let header_color = if options.no_color { "" } else { "\x1b[1;36m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let section_color = if options.no_color { "" } else { "\x1b[1;35m" };
    let item_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };

    println!(
        "{}╔══════════════════════════════════════════════════════════════╗{}",
        header_color, reset
    );
    println!(
        "{}║              Cavvy AST Viewer v{}                       ║{}",
        header_color, VERSION, reset
    );
    println!(
        "{}╚══════════════════════════════════════════════════════════════╝{}",
        header_color, reset
    );
    println!();
    println!("源文件: {}", file_path);
    println!();

    println!("{}统计信息:{}", section_color, reset);
    println!("  类数量: {}", ast.classes.len());
    println!("  接口数量: {}", ast.interfaces.len());
    println!("  结构体数量: {}", ast.structs.len());
    println!("  枚举数量: {}", ast.enums.len());
    println!("  顶层函数数量: {}", ast.top_level_functions.len());
    println!("  Extern 声明数量: {}", ast.extern_declarations.len());
    println!("  类型别名数量: {}", ast.type_aliases.len());
    println!();

    if !ast.type_aliases.is_empty() {
        println!("{}类型别名:{}", section_color, reset);
        for alias in &ast.type_aliases {
            println!(
                "  {}{} {}= {}{:?}{}",
                item_color, alias.name, detail_color, alias.target_type, reset, ""
            );
        }
        println!();
    }

    if !ast.extern_declarations.is_empty() {
        println!("{}Extern 声明:{}", section_color, reset);
        for ext in &ast.extern_declarations {
            print_extern_decl(ext, options, 2);
        }
        println!();
    }

    if !ast.interfaces.is_empty() {
        println!("{}接口:{}", section_color, reset);
        for iface in &ast.interfaces {
            print_interface(iface, options, 2);
        }
        println!();
    }

    if !ast.structs.is_empty() {
        println!("{}结构体:{}", section_color, reset);
        for st in &ast.structs {
            print_struct(st, options, 2);
        }
        println!();
    }

    if !ast.enums.is_empty() {
        println!("{}枚举:{}", section_color, reset);
        for en in &ast.enums {
            print_enum(en, options, 2);
        }
        println!();
    }

    if !ast.classes.is_empty() {
        println!("{}类:{}", section_color, reset);
        for class in &ast.classes {
            print_class(class, options, 2);
        }
        println!();
    }

    if !ast.top_level_functions.is_empty() {
        println!("{}顶层函数:{}", section_color, reset);
        for func in &ast.top_level_functions {
            print_top_level_function(func, options, 2);
        }
    }
}

fn print_extern_decl(ext: &ExternDecl, options: &CommonOptions, indent: usize) {
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);
    println!(
        "{}{:?} 调用约定{}",
        indent_str, ext.calling_convention, reset
    );
    for func in &ext.functions {
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| format!("{}: {:?}", p.name, p.param_type))
            .collect();
        println!(
            "{}  {}fn {}({}) -> {:?}{}",
            indent_str,
            detail_color,
            func.name,
            params.join(", "),
            func.return_type,
            reset
        );
    }
}

fn print_interface(iface: &InterfaceDecl, options: &CommonOptions, indent: usize) {
    let item_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);
    println!(
        "{}{}{}interface {}{}",
        indent_str, item_color, iface.name, reset, ""
    );
    if !options.compact {
        for method in &iface.methods {
            print_method(method, options, indent + 2);
        }
    }
}

fn print_struct(st: &StructDecl, options: &CommonOptions, indent: usize) {
    let item_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);
    println!(
        "{}{}{}struct {}{}",
        indent_str, item_color, st.name, reset, ""
    );
    if !options.compact {
        for field in &st.fields {
            print_field(field, options, indent + 2);
        }
        for method in &st.methods {
            print_method(method, options, indent + 2);
        }
    }
}

fn print_enum(en: &EnumDecl, options: &CommonOptions, indent: usize) {
    let item_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);
    println!(
        "{}{}{}enum {}{}",
        indent_str, item_color, en.name, reset, ""
    );
    if !options.compact {
        for variant in &en.variants {
            let payload = match &variant.payload_type {
                Some(t) => format!("({:?})", t),
                None => String::new(),
            };
            println!(
                "{}  {}{}{}{}",
                indent_str, detail_color, variant.name, payload, reset
            );
        }
    }
}

fn print_class(class: &ClassDecl, options: &CommonOptions, indent: usize) {
    let item_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);

    let modifiers: Vec<&str> = class
        .modifiers
        .iter()
        .map(|m| modifier_str(m))
        .filter(|s| !s.is_empty())
        .collect();

    let extends = if let Some(ref parent) = class.parent {
        format!(" extends {}", parent)
    } else {
        String::new()
    };

    let implements = if !class.interfaces.is_empty() {
        let names: Vec<String> = class.interfaces.iter().map(|t| format!("{}", t)).collect();
        format!(" implements {}", names.join(", "))
    } else {
        String::new()
    };

    println!(
        "{}{}{}class {}{}{}{}",
        indent_str,
        if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        },
        item_color,
        class.name,
        extends,
        implements,
        reset
    );

    if !options.compact {
        for member in &class.members {
            match member {
                ClassMember::Field(field) => print_field(field, options, indent + 2),
                ClassMember::Method(method) => print_method(method, options, indent + 2),
                ClassMember::Constructor(ctor) => print_constructor(ctor, options, indent + 2),
                ClassMember::Destructor(dest) => print_destructor(dest, options, indent + 2),
                _ => {}
            }
        }
    }
}

fn print_field(field: &FieldDecl, options: &CommonOptions, indent: usize) {
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);
    let modifiers: Vec<&str> = field
        .modifiers
        .iter()
        .map(|m| modifier_str(m))
        .filter(|s| !s.is_empty())
        .collect();
    println!(
        "{}{}{} {}: {:?}{}",
        indent_str,
        if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        },
        detail_color,
        field.name,
        field.field_type,
        reset
    );
}

fn print_method(method: &MethodDecl, options: &CommonOptions, indent: usize) {
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);
    let modifiers: Vec<&str> = method
        .modifiers
        .iter()
        .map(|m| modifier_str(m))
        .filter(|s| !s.is_empty())
        .collect();
    let params: Vec<String> = method
        .params
        .iter()
        .map(|p| format!("{}: {:?}", p.name, p.param_type))
        .collect();
    let body_indicator = if method.body.is_some() { "" } else { ";" };
    println!(
        "{}{}{}fn {}({}) -> {:?}{}{}",
        indent_str,
        if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        },
        detail_color,
        method.name,
        params.join(", "),
        method.return_type,
        body_indicator,
        reset
    );
}

fn print_constructor(ctor: &cavvy::ast::ConstructorDecl, options: &CommonOptions, indent: usize) {
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);
    let modifiers: Vec<&str> = ctor
        .modifiers
        .iter()
        .map(|m| modifier_str(m))
        .filter(|s| !s.is_empty())
        .collect();
    let params: Vec<String> = ctor
        .params
        .iter()
        .map(|p| format!("{}: {:?}", p.name, p.param_type))
        .collect();
    println!(
        "{}{}{}constructor({}){}",
        indent_str,
        if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        },
        detail_color,
        params.join(", "),
        reset
    );
}

fn print_destructor(dest: &cavvy::ast::DestructorDecl, options: &CommonOptions, indent: usize) {
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);
    let modifiers: Vec<&str> = dest
        .modifiers
        .iter()
        .map(|m| modifier_str(m))
        .filter(|s| !s.is_empty())
        .collect();
    println!(
        "{}{}{}~this(){}",
        indent_str,
        if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        },
        detail_color,
        reset
    );
}

fn print_top_level_function(func: &TopLevelFunction, options: &CommonOptions, indent: usize) {
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);
    let modifiers: Vec<&str> = func
        .modifiers
        .iter()
        .map(|m| modifier_str(m))
        .filter(|s| !s.is_empty())
        .collect();
    let params: Vec<String> = func
        .params
        .iter()
        .map(|p| format!("{}: {:?}", p.name, p.param_type))
        .collect();
    println!(
        "{}{}{}fn {}({}) -> {:?}{}",
        indent_str,
        if modifiers.is_empty() {
            String::new()
        } else {
            format!("{} ", modifiers.join(" "))
        },
        detail_color,
        func.name,
        params.join(", "),
        func.return_type,
        reset
    );
}

fn modifier_str(m: &cavvy::ast::Modifier) -> &'static str {
    match m {
        cavvy::ast::Modifier::Public => "public",
        cavvy::ast::Modifier::Private => "private",
        cavvy::ast::Modifier::Protected => "protected",
        cavvy::ast::Modifier::Static => "static",
        cavvy::ast::Modifier::Final => "final",
        cavvy::ast::Modifier::Abstract => "abstract",
        cavvy::ast::Modifier::Native => "native",
        cavvy::ast::Modifier::Interop => "interop",
        cavvy::ast::Modifier::Main => "main",
        cavvy::ast::Modifier::Override => "override",
        cavvy::ast::Modifier::Test => "@Test",
        cavvy::ast::Modifier::FreeFunction => "@FreeFunction",
        cavvy::ast::Modifier::StackOnly => "@stack_only",
    }
}
