//! Cavvy 诊断系统测试
//!
//! 测试错误诊断系统，包括错误代码、错误收集和友好的错误信息

use cavvy::miette_diagnostic::*;
use cavvy::lexer::lex_with_diagnostics;

// ==================== 辅助 trait — 为 Vec<CayError> 添加便利方法 ====================

trait DiagnosticVec {
    fn has_errors(&self) -> bool;
    fn has_warnings(&self) -> bool;
    fn error_count(&self) -> usize;
    fn warning_count(&self) -> usize;
    fn has_fatal_errors(&self) -> bool;
    fn all_errors(&self) -> &[CayError];
    fn get_error_code(&self, code: &str) -> Option<&CayError>;
}

impl DiagnosticVec for Vec<CayError> {
    fn has_errors(&self) -> bool {
        self.iter().any(|e| matches!(e.severity(), Severity::Error | Severity::Fatal))
    }
    fn has_warnings(&self) -> bool {
        self.iter().any(|e| e.severity() == Severity::Warning)
    }
    fn error_count(&self) -> usize {
        self.iter().filter(|e| matches!(e.severity(), Severity::Error | Severity::Fatal)).count()
    }
    fn warning_count(&self) -> usize {
        self.iter().filter(|e| e.severity() == Severity::Warning).count()
    }
    fn has_fatal_errors(&self) -> bool {
        self.iter().any(|e| e.severity() == Severity::Fatal)
    }
    fn all_errors(&self) -> &[CayError] {
        self.as_slice()
    }
    fn get_error_code(&self, code: &str) -> Option<&CayError> {
        self.iter().find(|e| e.error_code() == code)
    }
}

/// 创建一个语义警告（CayError 中只有 CodeGen 变体支持 is_warning）
fn semantic_warning(code: &'static str, line: usize, column: usize, message: impl Into<String>) -> CayError {
    CayError::CodeGen {
        error_code: code,
        kind: "语义警告".to_string(),
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
        is_warning: true,
    }
}

fn fatal_error(code: &'static str, line: usize, column: usize, message: impl Into<String>) -> CayError {
    CayError::CodeGen {
        error_code: code,
        kind: "致命错误".to_string(),
        file: None,
        line,
        column,
        message: message.into(),
        suggestion: ErrorCodes::get_suggestion(code).to_string(),
        is_warning: false,
    }
}

// ==================== 构造测试（曾经的 DiagnosticCollector 功能） ====================

#[test]
fn test_collector_basic() {
    let mut errors: Vec<CayError> = Vec::new();

    errors.push(semantic_error(
        ErrorCodes::SEMANTIC_TYPE_MISMATCH,
        10, 5,
        "类型不匹配",
    ));

    assert!(errors.has_errors());
    assert_eq!(errors.error_count(), 1);
    assert_eq!(errors.warning_count(), 0);
}

#[test]
fn test_collector_multiple_errors() {
    let mut errors: Vec<CayError> = Vec::new();

    errors.push(semantic_error(
        ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER,
        5, 10,
        "未定义变量 x",
    ));

    errors.push(semantic_error(
        ErrorCodes::SEMANTIC_TYPE_MISMATCH,
        8, 15,
        "类型不匹配",
    ));

    errors.push(semantic_warning(
        ErrorCodes::SEMANTIC_WARN_UNUSED_VARIABLE,
        12, 5,
        "未使用的变量",
    ));

    assert!(errors.has_errors());
    assert_eq!(errors.error_count(), 2);
    assert_eq!(errors.warning_count(), 1);
    assert_eq!(errors.len(), 3);
}

// ==================== 错误代码测试 ====================

#[test]
fn test_error_codes_descriptions() {
    assert_eq!(
        ErrorCodes::get_description(ErrorCodes::LEXER_INVALID_CHARACTER),
        "非法字符"
    );
    assert_eq!(
        ErrorCodes::get_description(ErrorCodes::LEXER_UNTERMINATED_STRING),
        "未闭合的字符串"
    );
    assert_eq!(
        ErrorCodes::get_description(ErrorCodes::PARSER_EXPECTED_SEMICOLON),
        "缺少分号"
    );
    assert_eq!(
        ErrorCodes::get_description(ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER),
        "未定义的标识符"
    );
    assert_eq!(
        ErrorCodes::get_description(ErrorCodes::SEMANTIC_TYPE_MISMATCH),
        "类型不匹配"
    );
    assert_eq!(
        ErrorCodes::get_description(ErrorCodes::CODEGEN_UNSUPPORTED_FEATURE),
        "不支持的功能"
    );
    assert_eq!(ErrorCodes::get_description("UNKNOWN"), "未知错误");
}

#[test]
fn test_error_codes_suggestions() {
    assert!(!ErrorCodes::get_suggestion(ErrorCodes::LEXER_INVALID_CHARACTER).is_empty());
    assert!(!ErrorCodes::get_suggestion(ErrorCodes::PARSER_EXPECTED_SEMICOLON).is_empty());
    assert!(!ErrorCodes::get_suggestion(ErrorCodes::SEMANTIC_TYPE_MISMATCH).is_empty());
}

// ==================== 严重程度测试 ====================

#[test]
fn test_severity_ordering() {
    assert!(Severity::Note < Severity::Warning);
    assert!(Severity::Warning < Severity::Error);
    assert!(Severity::Error < Severity::Fatal);
}

#[test]
fn test_severity_display() {
    assert_eq!(format!("{}", Severity::Note), "提示");
    assert_eq!(format!("{}", Severity::Warning), "警告");
    assert_eq!(format!("{}", Severity::Error), "错误");
    assert_eq!(format!("{}", Severity::Fatal), "致命错误");
}

// ==================== 编译阶段测试 ====================

#[test]
fn test_compilation_phase_display() {
    assert_eq!(format!("{}", CompilationPhase::Preprocessor), "预处理器");
    assert_eq!(format!("{}", CompilationPhase::Lexer), "词法分析");
    assert_eq!(format!("{}", CompilationPhase::Parser), "语法分析");
    assert_eq!(format!("{}", CompilationPhase::Semantic), "语义分析");
    assert_eq!(format!("{}", CompilationPhase::CodeGen), "代码生成");
    assert_eq!(format!("{}", CompilationPhase::Linker), "链接器");
}

// ==================== 源代码位置测试 ====================

#[test]
fn test_source_location() {
    let loc = SourceLocation::new(None::<String>, 10, 5);
    assert_eq!(loc.line, 10);
    assert_eq!(loc.column, 5);
    assert_eq!(format!("{}", loc), "10:5");
}

#[test]
fn test_source_span() {
    let span = SourceSpan::new(1, 5, 3, 10);
    assert_eq!(span.start.line, 1);
    assert_eq!(span.start.column, 5);
    assert_eq!(span.end.line, 3);
    assert_eq!(span.end.column, 10);

    let single = SourceSpan::single(5, 10);
    assert_eq!(single.start.line, 5);
    assert_eq!(single.end.line, 5);
}

// ==================== 词法分析诊断测试 ====================

#[test]
fn test_lexer_diagnostics_collection() {
    let source = "int x = 42 #;"; // # 是非法字符
    let (_tokens, errors) = lex_with_diagnostics(source);

    // 应该产生错误
    assert!(!errors.is_empty());
    assert!(errors.has_errors());
}

#[test]
fn test_lexer_unterminated_string_detection() {
    let source = r#"String s = "hello;"#; // 未闭合的字符串
    let (_tokens, errors) = lex_with_diagnostics(source);

    // 应该检测到未闭合的字符串
    let has_unterminated = errors
        .iter()
        .any(|d| d.error_code() == ErrorCodes::LEXER_UNTERMINATED_STRING);
    assert!(has_unterminated, "应该检测到未闭合的字符串错误");
}

// ==================== 诊断打印测试 ====================

#[test]
fn test_print_diagnostics() {
    let source = "int x = 42;\nint y = x + 1;";
    let mut errors: Vec<CayError> = Vec::new();

    errors.push(semantic_error(
        ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER,
        2, 9,
        "未定义变量 x",
    ));

    // print_diagnostics 应无 panic
    print_diagnostics(&errors, source, "test.cay");
}

// ==================== 诊断收集器 clear 测试 ====================

#[test]
fn test_collector_clear() {
    let mut errors: Vec<CayError> = Vec::new();

    errors.push(semantic_error(
        ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER,
        1, 1,
        "错误",
    ));

    assert!(errors.has_errors());

    errors.clear();

    assert!(!errors.has_errors());
    assert_eq!(errors.error_count(), 0);
    assert_eq!(errors.warning_count(), 0);
    assert!(errors.is_empty());
}

// ==================== 修复建议测试 ====================

#[test]
fn test_fix_suggestion_basic() {
    let suggestion = FixSuggestion::new("添加分号");
    assert_eq!(suggestion.description, "添加分号");
    assert!(suggestion.replacement.is_none());
    assert!(suggestion.span.is_none());
}

#[test]
fn test_fix_suggestion_with_replacement() {
    let span = SourceSpan::single(5, 10);
    let suggestion = FixSuggestion::new("添加分号").with_replacement(";", span);

    assert_eq!(suggestion.description, "添加分号");
    assert_eq!(suggestion.replacement, Some(";".to_string()));
    assert!(suggestion.span.is_some());
}

// ==================== 综合诊断场景测试 ====================

#[test]
fn test_comprehensive_error_scenario() {
    // 创建一个包含多种错误的场景
    let mut errors: Vec<CayError> = Vec::new();

    // 词法错误
    errors.push(lexer_error(
        ErrorCodes::LEXER_INVALID_CHARACTER,
        1, 10,
        "非法字符 '@'",
    ));

    // 语法错误
    errors.push(parser_error(
        ErrorCodes::PARSER_EXPECTED_SEMICOLON,
        3, 15,
        "缺少分号",
    ));

    // 语义错误
    errors.push(semantic_error(
        ErrorCodes::SEMANTIC_UNDEFINED_IDENTIFIER,
        5, 8,
        "未定义变量 'foo'",
    ));

    // 警告
    errors.push(semantic_warning(
        ErrorCodes::SEMANTIC_WARN_UNUSED_VARIABLE,
        7, 5,
        "变量 'bar' 未使用",
    ));

    assert_eq!(errors.error_count(), 3);
    assert_eq!(errors.warning_count(), 1);

    // 验证每个诊断都有正确的阶段
    let phases: Vec<CompilationPhase> = errors.iter().map(|d| d.phase()).collect();
    assert!(phases.contains(&CompilationPhase::Lexer));
    assert!(phases.contains(&CompilationPhase::Parser));
    assert!(phases.contains(&CompilationPhase::Semantic));
}

// ==================== 边缘情况测试 ====================

#[test]
fn test_empty_source_location() {
    let loc = SourceLocation::default();
    assert_eq!(loc.line, 0);
    assert_eq!(loc.column, 0);
}

#[test]
fn test_error_without_suggestion() {
    let err = io_error(None, "文件未找到");
    assert!(err.suggestion_text().is_none());
}

#[test]
fn test_fatal_error_detection() {
    let mut errors: Vec<CayError> = Vec::new();

    errors.push(fatal_error(
        ErrorCodes::CODEGEN_LLVM_ERROR,
        1, 1,
        "LLVM致命错误",
    ));

    assert!(errors.has_fatal_errors());
}

// ==================== 行号为0调试信息测试 ====================

/// 测试当诊断信息的行号为0时，系统应输出调试信息并保存到文件
/// 这是为了检测和报告编译器内部的定位问题
#[test]
fn test_zero_line_debug_info() {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let mut errors: Vec<CayError> = Vec::new();
    let source = "public class Test {}";

    // 创建一个行号为0的诊断（模拟内部错误定位失败）
    errors.push(duplicate_definition_error_with_file(
        Some("test.cay".to_string()),
        0, 1,
        "Test",
    ));

    // 获取当前时间戳用于查找生成的调试文件
    let before_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // 调用 print_diagnostics 触发调试信息输出
    // 注意：这会输出到 stderr 并可能创建 debug_*.txt 文件
    print_diagnostics(&errors, source, "test.cay");

    // 验证：查找是否生成了调试文件
    let entries = fs::read_dir(".").expect("无法读取当前目录");
    let mut found_debug_file = false;

    for entry in entries {
        if let Ok(entry) = entry {
            let filename = entry.file_name();
            let filename_str = filename.to_string_lossy();

            // 检查是否是 debug_*.txt 文件且创建时间在测试之后
            if filename_str.starts_with("debug_") && filename_str.ends_with(".txt") {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(elapsed) = modified.duration_since(SystemTime::UNIX_EPOCH) {
                            let file_timestamp = elapsed.as_secs();
                            // 文件是在测试开始之后创建的
                            if file_timestamp >= before_timestamp {
                                found_debug_file = true;

                                // 验证文件内容
                                let content =
                                    fs::read_to_string(entry.path()).expect("无法读取调试文件");
                                assert!(content.contains("Cavvy Bug Report"));
                                assert!(content.contains("错误代码:"));
                                assert!(content.contains("E4002"));
                                assert!(content.contains("重复定义"));
                                assert!(content.contains("行号: 0 (行号为0)"));
                                assert!(content.contains("=== 源代码 ==="));
                                assert!(content.contains("public class Test {}"));

                                // 清理测试文件
                                let _ = fs::remove_file(entry.path());
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // 如果 stderr 被捕获，可能无法验证输出，但至少验证文件被创建
    // 注意：在测试环境中，stderr 可能被捕获，所以主要验证文件创建
    assert!(found_debug_file, "应该生成 debug_*.txt 调试文件当行号为0时");
}
