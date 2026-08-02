pub mod ast;
pub mod bytecode;
pub mod codegen;
pub mod ir;
pub mod lexer;
pub mod miette_diagnostic;
pub mod parser;
pub mod preprocessor;
pub mod rcpl;
pub mod semantic;
pub mod types;
// Cavly 包管理器模块
pub mod cavly;

// IR 到 EXE 编译模块（被 cayc 和 ir2exe 共享）
pub mod ir2exe_lib;

// 嵌入式 LLVM LLC 编译器模块（实验性）
pub mod embedded_llc;

// 统一诊断系统 v2 — CayError 直接实现 miette::Diagnostic
pub use miette_diagnostic::CayError;
pub use miette_diagnostic::CayResult;
pub use miette_diagnostic::print_diagnostics;
pub use miette_diagnostic::print_error_with_context;

use std::path::{Path, PathBuf};

/// 编译器配置选项
#[derive(Debug, Clone)]
pub struct CompilerOptions {
    pub target_os: String,
    pub features: Vec<String>,
    pub no_features: Vec<String>,
    pub defines: Vec<String>,
    pub undefines: Vec<String>,
    pub obfuscate: bool,
    pub debug: bool, // 生成 DWARF 调试信息
    /// 额外的包含路径（供 #include 搜索）
    pub include_paths: Vec<String>,
    /// 测试模式：生成 __cavvy_test_main 入口，自动调用所有 @Test 方法
    pub test_mode: bool,
    /// 启用 Rc<T> 循环引用运行时检测（--detect-cycles）。
    pub detect_cycles: bool,
    /// --no-panic：将 panic()/abort() 调用转为编译错误（适用于嵌入式等不允许 panic 的环境）。
    pub no_panic: bool,
}

impl Default for CompilerOptions {
    fn default() -> Self {
        Self {
            target_os: std::env::consts::OS.to_string(),
            features: Vec::new(),
            no_features: Vec::new(),
            defines: Vec::new(),
            undefines: Vec::new(),
            obfuscate: false,
            debug: false,
            include_paths: Vec::new(),
            test_mode: false,
            detect_cycles: false,
            no_panic: false,
        }
    }
}

pub struct Compiler {
    options: CompilerOptions,
}

/// 是否启用 token 流调试输出。
/// 默认关闭；仅在 CAVVY_DEBUG_TOKENS=1（或 true）时启用。
/// 输出一律走 stderr，避免污染 LSP 等依赖 stdout 的 JSON-RPC 通道。
fn debug_tokens_enabled() -> bool {
    std::env::var("CAVVY_DEBUG_TOKENS")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

impl Compiler {
    pub fn new() -> Self {
        Self {
            options: CompilerOptions::default(),
        }
    }

    pub fn with_options(options: CompilerOptions) -> Self {
        Self { options }
    }

    /// 统一打印编译过程中收集到的警告
    fn print_warnings(warnings: &[CayError], source: &str, filename: &str) {
        if warnings.is_empty() {
            return;
        }
        crate::miette_diagnostic::print_diagnostics_by_file(warnings, source, filename);
    }

    /// 编译流水线公共实现：词法 → 语法 → 语义 → 代码生成 → 写出 IR 文件。
    ///
    /// `compile()` 与 `compile_with_source_map_and_link_libs()` 共享此实现；
    /// 当 `source_map_for_analyzer` 为 None 时按无源映射模式运行
    /// （不设置分析器/生成器的源映射与当前文件）。
    fn compile_pipeline(
        &self,
        source: &str,
        mut lexer: lexer::Lexer,
        source_map_for_analyzer: Option<std::collections::HashMap<usize, (String, usize)>>,
        output_path: &str,
        main_file: Option<String>,
        link_libraries: Vec<crate::ast::LinkLibraryDecl>,
    ) -> CayResult<()> {
        let default_filename = main_file.as_deref().unwrap_or("");
        let mut warnings: Vec<CayError> = Vec::new();

        // 1. 词法分析（同时收集警告）
        let tokens = match lexer.tokenize() {
            Ok(tokens) => {
                warnings.extend(lexer.take_warnings());
                tokens
            }
            Err(e) => {
                warnings.extend(lexer.take_warnings());
                Self::print_warnings(&warnings, source, default_filename);
                return Err(e);
            }
        };

        // 调试：打印所有 token（默认关闭，CAVVY_DEBUG_TOKENS=1 时输出到 stderr）
        if debug_tokens_enabled() {
            eprintln!("Tokens:");
            for (i, t) in tokens.iter().enumerate() {
                if let Some(ref file) = t.source_file {
                    eprintln!(
                        "  {}: {:?} at {}:{} (original: {})",
                        i,
                        t.token,
                        file,
                        t.source_line.unwrap_or(t.loc.line),
                        t.loc
                    );
                } else {
                    eprintln!("  {}: {:?} at {}", i, t.token, t.loc);
                }
            }
            eprintln!();
        }

        // 2. 语法分析（传入源代码以支持内联IR解析）
        let mut ast = parser::parse_with_source(tokens, source.to_string())?;

        // 合并预处理器收集的链接库信息到 AST
        ast.link_libraries = link_libraries;

        // 3. 语义分析
        let mut analyzer = semantic::SemanticAnalyzer::with_features(self.options.features.clone());
        if let Some(ref map) = source_map_for_analyzer {
            analyzer.set_current_file(main_file.clone());
            // 传递源映射表以支持多文件include场景下的正确错误定位
            analyzer.set_source_map(map.clone());
        }
        let ast = analyzer.analyze(ast)?;

        // 4. 代码生成 - 生成LLVM IR（字符串常量已在生成器内处理）
        let mut ir_gen = codegen::IRGenerator::new();
        // 传递多平台配置
        ir_gen.set_platform_config(&self.options);
        // 传递类型注册表以支持正确的方法名生成
        ir_gen.set_type_registry(analyzer.get_type_registry().clone());
        if let Some(ref map) = source_map_for_analyzer {
            // 设置预处理器源映射（用于多文件include场景）
            ir_gen.set_preprocessor_source_map(map.clone());
        }
        // 启用 DWARF 调试信息
        if self.options.debug {
            ir_gen.enable_debug_info();
        }
        // 启用测试模式
        if self.options.test_mode {
            ir_gen.enable_test_mode();
        }
        // 设置源文件路径以启用源映射
        let source_file = main_file.as_deref().unwrap_or("");
        let gen_result = ir_gen.generate(&ast, source_file);
        warnings.extend(std::mem::take(&mut *ir_gen.warnings.borrow_mut()));
        let mut ir = gen_result?;

        // 5. 如果启用了混淆，应用IR混淆
        if self.options.obfuscate {
            use codegen::obfuscator::IRObfuscator;
            let mut obfuscator = IRObfuscator::new();
            ir = obfuscator.obfuscate_ir(&ir);
        }

        // 输出到文件
        std::fs::write(output_path, ir).map_err(|e| CayError::Io {
            error_code: "I0001",
            file: Some(output_path.to_string()),
            message: e.to_string(),
        })?;

        Self::print_warnings(&warnings, source, default_filename);

        Ok(())
    }

    /// 编译源代码为 LLVM IR
    ///
    /// # Arguments
    /// * `source` - 原始源代码（已预处理）
    /// * `output_path` - 输出文件路径
    ///
    /// # Returns
    /// 编译成功返回 Ok(())
    pub fn compile(&self, source: &str, output_path: &str) -> CayResult<()> {
        // 注意：compile 方法没有源文件路径与源映射，使用无源映射模式
        let lexer = lexer::Lexer::new(source);
        self.compile_pipeline(source, lexer, None, output_path, None, Vec::new())
    }

    /// 编译源代码为 LLVM IR（带源映射）
    ///
    /// # Arguments
    /// * `source` - 原始源代码（已预处理）
    /// * `source_map` - 源映射表
    /// * `output_path` - 输出文件路径
    ///
    /// # Returns
    /// 编译成功返回 Ok(())
    pub fn compile_with_source_map(
        &self,
        source: &str,
        source_map: std::collections::HashMap<usize, (String, usize)>,
        output_path: &str,
    ) -> CayResult<()> {
        self.compile_with_source_map_and_main_file(source, source_map, output_path, None)
    }

    /// 使用源映射编译（带主文件路径）
    ///
    /// # Arguments
    /// * `source` - 预处理后的源代码
    /// * `source_map` - 源映射表
    /// * `output_path` - 输出文件路径
    /// * `main_file` - 主文件路径（用于错误报告）
    ///
    /// # Returns
    /// 编译成功返回 Ok(())
    pub fn compile_with_source_map_and_main_file(
        &self,
        source: &str,
        source_map: std::collections::HashMap<usize, (String, usize)>,
        output_path: &str,
        main_file: Option<String>,
    ) -> CayResult<()> {
        self.compile_with_source_map_and_link_libs(
            source,
            source_map,
            output_path,
            main_file,
            Vec::new(),
        )
    }

    /// 使用源映射编译（带主文件路径和链接库信息）
    ///
    /// # Arguments
    /// * `source` - 预处理后的源代码
    /// * `source_map` - 源映射表
    /// * `output_path` - 输出文件路径
    /// * `main_file` - 主文件路径（用于错误报告）
    /// * `link_libraries` - #link 声明的链接库列表
    ///
    /// # Returns
    /// 编译成功返回 Ok(())
    pub fn compile_with_source_map_and_link_libs(
        &self,
        source: &str,
        source_map: std::collections::HashMap<usize, (String, usize)>,
        output_path: &str,
        main_file: Option<String>,
        link_libraries: Vec<crate::ast::LinkLibraryDecl>,
    ) -> CayResult<()> {
        // 保留一份源映射用于语义分析错误定位（词法分析器按值消耗一份）
        let source_map_for_analyzer = source_map.clone();
        // 词法分析器带源映射和当前文件路径（同时收集警告）
        let lexer = lexer::Lexer::with_source_map_and_file(source, source_map, main_file.clone());
        self.compile_pipeline(
            source,
            lexer,
            Some(source_map_for_analyzer),
            output_path,
            main_file,
            link_libraries,
        )
    }

    /// 从文件编译，自动执行预处理
    ///
    /// # Arguments
    /// * `input_path` - 输入源文件路径
    /// * `output_path` - 输出 LLVM IR 文件路径
    ///
    /// # Returns
    /// 编译成功返回 Ok(())
    pub fn compile_file(&self, input_path: &str, output_path: &str) -> CayResult<()> {
        // 读取源文件
        let source = std::fs::read_to_string(input_path).map_err(|e| CayError::Io {
            error_code: "I0001",
            file: Some(input_path.to_string()),
            message: format!("无法读取源文件: {}", e),
        })?;

        // 获取基础目录（用于解析相对路径的 #include）
        let base_dir = Path::new(input_path)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        // 构建系统包含路径列表（包含 caylibs 目录）
        let mut system_paths = Vec::new();

        // 尝试获取可执行文件所在目录，并添加 caylibs 子目录
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let caylibs_dir = exe_dir.join("caylibs");
                if caylibs_dir.exists() {
                    system_paths.push(caylibs_dir);
                }
            }
        }

        // 也尝试从当前工作目录添加 caylibs
        let cwd_caylibs = PathBuf::from("caylibs");
        if cwd_caylibs.exists() && !system_paths.contains(&cwd_caylibs) {
            system_paths.push(cwd_caylibs);
        }

        // 添加 CompilerOptions 中指定的额外包含路径（-I 参数）
        for path in &self.options.include_paths {
            let path_buf = PathBuf::from(path);
            if path_buf.exists() && !system_paths.contains(&path_buf) {
                system_paths.push(path_buf);
            }
        }

        // 使用带系统路径的预处理器（带源映射）

        let mut pp = if system_paths.is_empty() {
            preprocessor::Preprocessor::new(base_dir)
        } else {
            preprocessor::Preprocessor::with_include_paths(base_dir, system_paths)
        };
        pp.seed_defines(&self.options.defines);
        pp.seed_undefines(&self.options.undefines);
        let result = pp.process_with_source_map(&source, input_path)?;

        let source_map = Self::convert_source_map(&result.source_map);

        // 转换链接库信息
        let link_libraries: Vec<crate::ast::LinkLibraryDecl> = result
            .link_libraries
            .into_iter()
            .map(|lib| crate::ast::LinkLibraryDecl {
                name: lib.name,
                is_system: lib.is_system,
                loc: crate::miette_diagnostic::SourceLocation::default(),
            })
            .collect();

        // 编译预处理后的代码（带源映射、主文件路径和链接库信息）
        let main_file = Some(input_path.to_string());
        self.compile_with_source_map_and_link_libs(
            &result.code,
            source_map,
            output_path,
            main_file,
            link_libraries,
        )
    }

    /// 将预处理器源映射转换为HashMap格式
    fn convert_source_map(
        source_map: &preprocessor::SourceMap,
    ) -> std::collections::HashMap<usize, (String, usize)> {
        let mut map = std::collections::HashMap::new();
        for (idx, pos) in source_map.mappings.iter().enumerate() {
            // idx + 1 是 1-based 的输出行号
            // pos.line 已经是 1-based（预处理器使用 line_number + 1）
            map.insert(idx + 1, (pos.file.clone(), pos.line));
        }
        map
    }
}

impl Default for Compiler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hello_lexer() {
        let source = r#"public class hello {
    public static void main() {
        print("Hello, World");
    }
}"#;
        let tokens = lexer::lex(source).unwrap();
        println!("Tokens:");
        for (i, t) in tokens.iter().enumerate() {
            println!("  {}: {:?} at {}", i, t.token, t.loc);
        }
    }

    #[test]
    fn test_hello_parser() {
        let source = r#"public class hello {
    public static void main() {
        print("Hello, World");
    }
}"#;
        let tokens = lexer::lex(source).unwrap();
        let ast = parser::parse(tokens).unwrap();
        println!("AST: {:?}", ast);
    }

    #[test]
    fn test_nested_generic_type_parser_keeps_shift_expression() {
        let source = r#"public class pair<T> {
}

public class vector<T> {
}

public class Test {
    public void run() {
        vector<pair<int>> arr = vector();
        int shifted = 8 >> 1;
    }
}"#;
        let tokens = lexer::lex(source).unwrap();
        let ast = parser::parse(tokens).unwrap();
        println!("AST: {:?}", ast);
    }

    fn analyze_source(source: &str) -> miette_diagnostic::CayResult<()> {
        let tokens = lexer::lex(source)?;
        let ast = parser::parse_with_source(tokens, source.to_string())?;
        let mut analyzer = semantic::SemanticAnalyzer::new();
        analyzer.analyze(ast).map(|_| ())
    }

    #[test]
    fn test_static_method_named_like_string_method_resolves_to_class() {
        let source = r#"public class TextBuffer {
    private static bool contains(String text, String pattern) {
        return true;
    }

    public static void main() {
        bool ok = TextBuffer.contains("abc", "a");
        if (ok) {
            println("ok");
        }
    }
}"#;

        analyze_source(source)
            .expect("ClassName.contains should resolve to the static class method");
    }

    #[test]
    fn test_missing_instance_method_is_semantic_error_with_suggestion() {
        let source = r#"public class File {
    public File() {
    }

    public bool isOpened() {
        return true;
    }
}

public class Example {
    public static void main() {
        File file = new File();
        if (!file.isOpen()) {
            println("closed");
        }
    }
}"#;

        let err =
            analyze_source(source).expect_err("missing method should fail in semantic analysis");
        let message = format!("{:?}", err);
        assert!(message.contains("Unknown method 'isOpen'"), "{}", message);
        assert!(message.contains("isOpened"), "{}", message);
    }

    #[test]
    fn test_missing_static_member_is_semantic_error_with_suggestion() {
        let source = r#"public class FileMode {
    public FileMode() {
    }

    public static FileMode read() {
        return new FileMode();
    }
}

public class File {
    public File(FileMode mode) {
    }
}

public class Example {
    public static void main() {
        File file = new File(FileMode.READ);
    }
}"#;

        let err = analyze_source(source)
            .expect_err("missing static member should fail in semantic analysis");
        let message = format!("{:?}", err);
        assert!(
            message.contains("Unknown static member 'READ'"),
            "{}",
            message
        );
        assert!(message.contains("read()"), "{}", message);
    }

    #[test]
    fn test_source_location_file_prevents_double_source_map_remap() {
        let mut ir_gen = codegen::IRGenerator::new();
        ir_gen.enable_source_map = true;

        // 使用仓库内真实路径（CARGO_MANIFEST_DIR），不再硬编码开发者机器路径。
        // 保留 Windows 扩展长度路径前缀 \\?\ 以覆盖前缀剥离逻辑。
        let manifest = env!("CARGO_MANIFEST_DIR");
        let mut source_map = std::collections::HashMap::new();
        source_map.insert(
            621,
            (
                format!(r"\\?\{}\target\release\caylibs\StringBuilder.cay", manifest),
                10,
            ),
        );
        ir_gen.set_preprocessor_source_map(source_map);

        let loc = miette_diagnostic::SourceLocation::new(
            Some(format!(r"\\?\{}\examples\text_editor.cay", manifest)),
            621,
            9,
        );
        ir_gen.set_source_from_loc(&loc, "fallback.cay");
        ir_gen.emit_line("%x = alloca i32");

        assert!(
            ir_gen.code.contains(&format!(
                "; !source {}\\examples\\text_editor.cay:621:9",
                manifest
            )),
            "{}",
            ir_gen.code
        );
        assert!(
            !ir_gen.code.contains("StringBuilder.cay"),
            "{}",
            ir_gen.code
        );
        assert!(!ir_gen.code.contains(r"\\?\"), "{}", ir_gen.code);
    }

    #[test]
    fn test_llc_error_text_can_remap_to_cay_source() {
        let mut source_map = ir2exe_lib::IRSourceMap::new();
        source_map.add_mapping(4834, "examples/text_editor.cay".to_string(), 637, 18);

        let error = "llc.exe: error: llc.exe: text_editor.ll:4834:21: error: use of undefined value '@_ZN3std4FileE.isOpen'\n    %t11 = call i64 @_ZN3std4FileE.isOpen()\n                    ^";
        let remapped = ir2exe_lib::remap_clang_error(error, &source_map, "text_editor.ll");

        assert!(
            remapped.contains("at examples/text_editor.cay:637:18"),
            "{}",
            remapped
        );
        assert!(remapped.contains("use of undefined value"), "{}", remapped);
        assert!(!remapped.contains("text_editor.ll:4834:21"), "{}", remapped);
    }

    #[test]
    fn test_preprocessor_define() {
        let source = r#"
#define DEBUG 1
public class Test {
    public static void main() {
        int x = DEBUG;
    }
}
"#;
        let preprocessed = preprocessor::preprocess(source, "test.cay", ".").unwrap();
        assert!(preprocessed.contains("int x = 1;"));
    }

    #[test]
    fn test_preprocessor_ifdef() {
        let source = r#"
#define DEBUG
#ifdef DEBUG
public class DebugClass {
}
#endif
"#;
        let preprocessed = preprocessor::preprocess(source, "test.cay", ".").unwrap();
        assert!(preprocessed.contains("DebugClass"));
    }
}
