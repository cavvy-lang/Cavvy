pub mod error;
pub mod diagnostic;
pub mod miette_diagnostic;
pub mod types;
pub mod ast;
pub mod preprocessor;
pub mod lexer;
pub mod parser;
pub mod semantic;
pub mod codegen;
pub mod ir;
pub mod rcpl;
pub mod bytecode;

// GUI模块（cay-idle使用）
pub mod idle;

// Cavly 包管理器模块
pub mod cavly;

/// 新的统一错误类型（推荐使用）
pub use error::CompilerError;
pub use error::CompilerResult;
pub use diagnostic::print_diagnostics;
pub use diagnostic::DiagnosticCollector;

use std::path::{Path, PathBuf};
use error::cayResult;

/// 编译器配置选项
#[derive(Debug, Clone)]
pub struct CompilerOptions {
    pub target_os: String,
    pub features: Vec<String>,
    pub no_features: Vec<String>,
    pub defines: Vec<String>,
    pub undefines: Vec<String>,
    pub obfuscate: bool,
    pub debug: bool,               // 生成 DWARF 调试信息
    /// 额外的包含路径（供 #include 搜索）
    pub include_paths: Vec<String>,
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
        }
    }
}

pub struct Compiler {
    options: CompilerOptions,
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

    /// 编译源代码为 LLVM IR
    ///
    /// # Arguments
    /// * `source` - 原始源代码（已预处理）
    /// * `output_path` - 输出文件路径
    ///
    /// # Returns
    /// 编译成功返回 Ok(())
    pub fn compile(&self, source: &str, output_path: &str) -> cayResult<()> {
        // 1. 词法分析
        let tokens = lexer::lex(source)?;

        // 调试：打印所有token
        #[cfg(debug_assertions)]
        {
            println!("Tokens:");
            for (i, t) in tokens.iter().enumerate() {
                println!("  {}: {:?} at {}", i, t.token, t.loc);
            }
            println!();
        }

        // 2. 语法分析（传入源代码以支持内联IR解析）
        let ast = parser::parse_with_source(tokens, source.to_string())?;

        // 3. 语义分析
        let mut analyzer = semantic::SemanticAnalyzer::with_features(self.options.features.clone());
        analyzer.analyze(&ast)?;

        // 4. 代码生成 - 生成LLVM IR（字符串常量已在生成器内处理）
        let mut ir_gen = codegen::IRGenerator::new();
        // 传递多平台配置
        ir_gen.set_platform_config(&self.options);
        // 传递类型注册表以支持正确的方法名生成
        ir_gen.set_type_registry(analyzer.get_type_registry().clone());
        // 启用 DWARF 调试信息
        if self.options.debug {
            ir_gen.enable_debug_info();
        }
        // 注意：compile方法没有源文件路径，使用空字符串
        let mut ir = ir_gen.generate(&ast, "")?;

        // 5. 如果启用了混淆，应用IR混淆
        if self.options.obfuscate {
            use codegen::obfuscator::IRObfuscator;
            let mut obfuscator = IRObfuscator::new();
            ir = obfuscator.obfuscate_ir(&ir);
        }

        // 输出到文件
        std::fs::write(output_path, ir)
            .map_err(|e| error::cayError::Io(e.to_string()))?;

        Ok(())
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
    pub fn compile_with_source_map(&self, source: &str, source_map: std::collections::HashMap<usize, (String, usize)>, output_path: &str) -> cayResult<()> {
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
    pub fn compile_with_source_map_and_main_file(&self, source: &str, source_map: std::collections::HashMap<usize, (String, usize)>, output_path: &str, main_file: Option<String>) -> cayResult<()> {
        // 保留一份源映射用于语义分析错误定位
        let source_map_for_analyzer = source_map.clone();

        // 1. 词法分析（带源映射和当前文件路径）
        let tokens = lexer::lex_with_source_map_and_file(source, source_map, main_file.clone())?;

        // 调试：打印所有token
        #[cfg(debug_assertions)]
        {
            println!("Tokens:");
            for (i, t) in tokens.iter().enumerate() {
                if let Some(ref file) = t.source_file {
                    println!("  {}: {:?} at {}:{} (original: {})", i, t.token, file, t.source_line.unwrap_or(t.loc.line), t.loc);
                } else {
                    println!("  {}: {:?} at {}", i, t.token, t.loc);
                }
            }
            println!();
        }

        // 2. 语法分析（传入源代码以支持内联IR解析）
        let ast = parser::parse_with_source(tokens, source.to_string())?;

        // 3. 语义分析
        let mut analyzer = semantic::SemanticAnalyzer::with_features(self.options.features.clone());
        analyzer.set_current_file(main_file.clone());
        // 传递源映射表以支持多文件include场景下的正确错误定位
        analyzer.set_source_map(source_map_for_analyzer.clone());
        analyzer.analyze(&ast)?;

        // 4. 代码生成 - 生成LLVM IR（字符串常量已在生成器内处理）
        let mut ir_gen = codegen::IRGenerator::new();
        // 传递多平台配置
        ir_gen.set_platform_config(&self.options);
        // 传递类型注册表以支持正确的方法名生成
        ir_gen.set_type_registry(analyzer.get_type_registry().clone());
        // 设置预处理器源映射（用于多文件include场景）
        ir_gen.set_preprocessor_source_map(source_map_for_analyzer.clone());
        // 启��� DWARF 调试信息
        if self.options.debug {
            ir_gen.enable_debug_info();
        }
        // 设置源文件路径以启用源映射
        let source_file = main_file.as_deref().unwrap_or("");
        let mut ir = ir_gen.generate(&ast, source_file)?;

        // 5. 如果启用了混淆，应用IR混淆
        if self.options.obfuscate {
            use codegen::obfuscator::IRObfuscator;
            let mut obfuscator = IRObfuscator::new();
            ir = obfuscator.obfuscate_ir(&ir);
        }

        // 输出到文件
        std::fs::write(output_path, ir)
            .map_err(|e| error::cayError::Io(e.to_string()))?;

        Ok(())
    }

    /// 从文件编译，自动执行预处理
    ///
    /// # Arguments
    /// * `input_path` - 输入源文件路径
    /// * `output_path` - 输出 LLVM IR 文件路径
    ///
    /// # Returns
    /// 编译成功返回 Ok(())
    pub fn compile_file(&self, input_path: &str, output_path: &str) -> cayResult<()> {
        // 读取源文件
        let source = std::fs::read_to_string(input_path)
            .map_err(|e| error::cayError::Io(
                format!("无法读取源文件 '{}': {}", input_path, e)
            ))?;

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
        let result = pp.process_with_source_map(&source, input_path)?;

        let source_map = Self::convert_source_map(&result.source_map);

        // 编译预处理后的代码（带源映射和主文件路径）
        let main_file = Some(input_path.to_string());
        self.compile_with_source_map_and_main_file(&result.code, source_map, output_path, main_file)
    }

    /// 将预处理器源映射转换为HashMap格式
    fn convert_source_map(source_map: &preprocessor::SourceMap) -> std::collections::HashMap<usize, (String, usize)> {
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