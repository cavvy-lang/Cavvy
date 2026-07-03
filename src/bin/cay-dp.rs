// cay-dp: Cavvy Debugger - Parse PreViewer
// 语法解析预览工具 - 显示源代码的语法分析结果（AST）

use cavvy::ast::{
    ClassDecl, ClassMember, FieldDecl, InterfaceDecl, MethodDecl, Program, TopLevelFunction,
};
use cavvy::lexer::lex_with_diagnostics;
use cavvy::parser::parse_with_source;
use cavvy::preprocessor::preprocess;
use std::env;
use std::fs;
use std::path::Path;
use std::process;

const VERSION: &str = env!("CAY_DP_VERSION");

fn print_usage(program: &str) {
    eprintln!("Cavvy Debugger - Parse PreViewer v{}", VERSION);
    eprintln!("用法: {} <源文件.cay> [选项]", program);
    eprintln!();
    eprintln!("选项:");
    eprintln!("  --json          以 JSON 格式输出 AST");
    eprintln!("  --no-color      禁用彩色输出");
    eprintln!("  --compact       紧凑输出模式");
    eprintln!("  --no-preprocess 禁用预处理器");
    eprintln!("  -h, --help      显示帮助信息");
    eprintln!("  -v, --version   显示版本信息");
}

#[derive(Debug)]
struct Options {
    json_output: bool,
    no_color: bool,
    compact: bool,
    no_preprocess: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            json_output: false,
            no_color: false,
            compact: false,
            no_preprocess: false,
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    if args.len() < 2 {
        print_usage(&program);
        process::exit(1);
    }

    let mut options = Options::default();
    let mut file_path: Option<String> = None;

    for arg in &args[1..] {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage(&program);
                process::exit(0);
            }
            "-v" | "--version" => {
                println!("cay-dp v{}", VERSION);
                process::exit(0);
            }
            "--json" => options.json_output = true,
            "--no-color" => options.no_color = true,
            "--compact" => options.compact = true,
            "--no-preprocess" => options.no_preprocess = true,
            _ if arg.starts_with('-') => {
                eprintln!("错误: 未知选项 {}", arg);
                print_usage(&program);
                process::exit(1);
            }
            _ => {
                if file_path.is_none() {
                    file_path = Some(arg.clone());
                } else {
                    eprintln!("错误: 只能指定一个源文件");
                    process::exit(1);
                }
            }
        }
    }

    let file_path = match file_path {
        Some(path) => path,
        None => {
            eprintln!("错误: 未指定源文件");
            print_usage(&program);
            process::exit(1);
        }
    };

    let source = match fs::read_to_string(&file_path) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("错误: 无法读取文件 '{}': {}", file_path, e);
            process::exit(1);
        }
    };

    // 预处理阶段
    let source_to_parse = if options.no_preprocess {
        source
    } else {
        let base_dir = Path::new(&file_path)
            .parent()
            .map(|p| p.to_str().unwrap_or("."))
            .unwrap_or(".");

        match preprocess(&source, &file_path, base_dir) {
            Ok(processed) => processed,
            Err(e) => {
                eprintln!("预处理错误: {}", e);
                process::exit(1);
            }
        }
    };

    let (tokens, lexer_diagnostics) = lex_with_diagnostics(&source_to_parse);

    if !lexer_diagnostics.is_empty() {
        eprintln!("词法分析错误:");
        for diag in lexer_diagnostics.iter() {
            let (line, col) = diag.location().unwrap_or((0, 0));
            eprintln!(
                "  [{}] {} (行 {}, 列 {})",
                diag.error_code(), diag.message(), line, col
            );
        }
        process::exit(1);
    }

    match parse_with_source(tokens, source_to_parse.clone()) {
        Ok(ast) => {
            if options.json_output {
                print_ast_json(&ast);
            } else {
                print_ast_pretty(&ast, &options, &file_path);
            }
        }
        Err(e) => {
            eprintln!("语法分析错误: {}", e);
            process::exit(1);
        }
    }
}

fn print_ast_pretty(ast: &Program, options: &Options, file_path: &str) {
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
        "{}║         Cavvy Debugger - Parse PreViewer v{}              ║{}",
        header_color, VERSION, reset
    );
    println!(
        "{}╚══════════════════════════════════════════════════════════════╝{}",
        header_color, reset
    );
    println!();
    println!("源文件: {}", file_path);
    println!();

    // 统计信息
    println!("{}统计信息:{}", section_color, reset);
    println!("  类数量: {}", ast.classes.len());
    println!("  接口数量: {}", ast.interfaces.len());
    println!("  顶层函数数量: {}", ast.top_level_functions.len());
    println!("  Extern 声明数量: {}", ast.extern_declarations.len());
    println!("  类型别名数量: {}", ast.type_aliases.len());
    println!();

    // 类型别名
    if !ast.type_aliases.is_empty() {
        println!("{}类型别名:{}", section_color, reset);
        for alias in &ast.type_aliases {
            println!(
                "  {}{} {}= {}{}",
                item_color,
                alias.name,
                detail_color,
                format!("{:?}", alias.target_type),
                reset
            );
        }
        println!();
    }

    // Extern 声明
    if !ast.extern_declarations.is_empty() {
        println!("{}Extern 声明:{}", section_color, reset);
        for ext in &ast.extern_declarations {
            println!(
                "  {}{:?} 调用约定{}",
                item_color, ext.calling_convention, reset
            );
            for func in &ext.functions {
                let params: Vec<String> = func
                    .params
                    .iter()
                    .map(|p| format!("{}: {:?}", p.name, p.param_type))
                    .collect();
                println!(
                    "    {}fn {}({}) -> {:?}{}",
                    detail_color,
                    func.name,
                    params.join(", "),
                    func.return_type,
                    reset
                );
            }
        }
        println!();
    }

    // 接口
    if !ast.interfaces.is_empty() {
        println!("{}接口:{}", section_color, reset);
        for iface in &ast.interfaces {
            print_interface(iface, options, 2);
        }
        println!();
    }

    // 类
    if !ast.classes.is_empty() {
        println!("{}类:{}", section_color, reset);
        for class in &ast.classes {
            print_class(class, options, 2);
        }
        println!();
    }

    // 顶层函数
    if !ast.top_level_functions.is_empty() {
        println!("{}顶层函数:{}", section_color, reset);
        for func in &ast.top_level_functions {
            print_top_level_function(func, options, 2);
        }
    }
}

fn print_class(class: &ClassDecl, options: &Options, indent: usize) {
    let item_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let _detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);

    let modifiers: Vec<&str> = class
        .modifiers
        .iter()
        .map(|m| match m {
            cavvy::ast::Modifier::Public => "public",
            cavvy::ast::Modifier::Private => "private",
            cavvy::ast::Modifier::Protected => "protected",
            cavvy::ast::Modifier::Static => "static",
            cavvy::ast::Modifier::Final => "final",
            cavvy::ast::Modifier::Abstract => "abstract",
            cavvy::ast::Modifier::Native => "native",
            _ => "",
        })
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
            "".to_string()
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
                _ => {}
            }
        }
    }
}

fn print_field(field: &FieldDecl, options: &Options, indent: usize) {
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);

    let modifiers: Vec<&str> = field
        .modifiers
        .iter()
        .map(|m| match m {
            cavvy::ast::Modifier::Public => "public",
            cavvy::ast::Modifier::Private => "private",
            cavvy::ast::Modifier::Protected => "protected",
            cavvy::ast::Modifier::Static => "static",
            cavvy::ast::Modifier::Final => "final",
            _ => "",
        })
        .filter(|s| !s.is_empty())
        .collect();

    println!(
        "{}  {}{} {}: {:?}{}",
        indent_str,
        if modifiers.is_empty() {
            "".to_string()
        } else {
            format!("{} ", modifiers.join(" "))
        },
        detail_color,
        field.name,
        field.field_type,
        reset
    );
}

fn print_interface(iface: &InterfaceDecl, options: &Options, indent: usize) {
    let item_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);

    println!(
        "{}{}interface {}{}",
        indent_str, item_color, iface.name, reset
    );

    if !options.compact {
        for method in &iface.methods {
            print_method(method, options, indent + 2);
        }
    }
}

fn print_method(method: &MethodDecl, options: &Options, indent: usize) {
    let _item_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);

    let modifiers: Vec<&str> = method
        .modifiers
        .iter()
        .map(|m| match m {
            cavvy::ast::Modifier::Public => "public",
            cavvy::ast::Modifier::Private => "private",
            cavvy::ast::Modifier::Protected => "protected",
            cavvy::ast::Modifier::Static => "static",
            cavvy::ast::Modifier::Final => "final",
            cavvy::ast::Modifier::Abstract => "abstract",
            cavvy::ast::Modifier::Native => "native",
            _ => "",
        })
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
            "".to_string()
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

fn print_constructor(ctor: &cavvy::ast::ConstructorDecl, options: &Options, indent: usize) {
    let _item_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);

    let modifiers: Vec<&str> = ctor
        .modifiers
        .iter()
        .map(|m| match m {
            cavvy::ast::Modifier::Public => "public",
            cavvy::ast::Modifier::Private => "private",
            cavvy::ast::Modifier::Protected => "protected",
            _ => "",
        })
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
            "".to_string()
        } else {
            format!("{} ", modifiers.join(" "))
        },
        detail_color,
        params.join(", "),
        reset
    );
}

fn print_top_level_function(func: &TopLevelFunction, options: &Options, indent: usize) {
    let _item_color = if options.no_color { "" } else { "\x1b[1;33m" };
    let detail_color = if options.no_color { "" } else { "\x1b[32m" };
    let reset = if options.no_color { "" } else { "\x1b[0m" };
    let indent_str = " ".repeat(indent);

    let modifiers: Vec<&str> = func
        .modifiers
        .iter()
        .map(|m| match m {
            cavvy::ast::Modifier::Public => "public",
            cavvy::ast::Modifier::Static => "static",
            _ => "",
        })
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
            "".to_string()
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

fn print_ast_json(ast: &Program) {
    use std::io::Write;

    let mut output = String::new();
    output.push_str("{\n");
    output.push_str(&format!(
        "  \"type_aliases\": {},\n",
        ast.type_aliases.len()
    ));
    output.push_str(&format!(
        "  \"extern_declarations\": {},\n",
        ast.extern_declarations.len()
    ));
    output.push_str(&format!("  \"interfaces\": {},\n", ast.interfaces.len()));
    output.push_str(&format!("  \"classes\": {},\n", ast.classes.len()));
    output.push_str(&format!(
        "  \"top_level_functions\": {}\n",
        ast.top_level_functions.len()
    ));
    output.push_str("}\n");

    if let Err(e) = std::io::stdout().write_all(output.as_bytes()) {
        eprintln!("写入stdout失败: {}", e);
    }
}
