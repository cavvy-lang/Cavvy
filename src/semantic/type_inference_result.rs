//! 类型推断结果 - 支持错误收集的类型推断
//!
//! 这个模块提供 TypeInferenceResult 类型，用于在类型推断过程中收集多个错误
//! 而不是遇到第一个错误就停止

use crate::types::Type;

/// 类型推断错误信息
#[derive(Debug, Clone)]
pub struct TypeInferenceError {
    pub line: usize,
    pub column: usize,
    pub message: String,
    pub file: Option<String>,
}

/// 类型推断结果
/// 包含推断出的类型和收集到的错误
#[derive(Debug, Clone)]
pub struct TypeInferenceResult {
    pub ty: Type,
    pub errors: Vec<TypeInferenceError>,
}

impl TypeInferenceResult {
    /// 创建成功的结果
    pub fn success(ty: Type) -> Self {
        Self {
            ty,
            errors: Vec::new(),
        }
    }

    /// 创建带有错误的结果
    pub fn with_error(ty: Type, line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            ty,
            errors: vec![TypeInferenceError {
                line,
                column,
                message: message.into(),
                file: None,
            }],
        }
    }

    /// 创建带有错误的结果（带文件路径）
    pub fn with_error_and_file(ty: Type, file: Option<String>, line: usize, column: usize, message: impl Into<String>) -> Self {
        Self {
            ty,
            errors: vec![TypeInferenceError {
                line,
                column,
                message: message.into(),
                file,
            }],
        }
    }

    /// 添加错误
    pub fn add_error(&mut self, line: usize, column: usize, message: impl Into<String>) {
        self.errors.push(TypeInferenceError {
            line,
            column,
            message: message.into(),
            file: None,
        });
    }

    /// 添加错误（带文件路径）
    pub fn add_error_with_file(&mut self, file: Option<String>, line: usize, column: usize, message: impl Into<String>) {
        self.errors.push(TypeInferenceError {
            line,
            column,
            message: message.into(),
            file,
        });
    }

    /// 合并另一个结果的错误
    pub fn merge_errors(&mut self, other: &TypeInferenceResult) {
        self.errors.extend(other.errors.clone());
    }

    /// 检查是否有错误
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// 获取错误数量
    pub fn error_count(&self) -> usize {
        self.errors.len()
    }
}

/// 类型推断错误收集器
/// 用于在类型推断过程中收集多个错误
#[derive(Debug, Clone, Default)]
pub struct TypeInferenceErrorCollector {
    pub errors: Vec<TypeInferenceError>,
}

impl TypeInferenceErrorCollector {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
        }
    }

    pub fn add_error(&mut self, line: usize, column: usize, message: impl Into<String>) {
        self.errors.push(TypeInferenceError {
            line,
            column,
            message: message.into(),
            file: None,
        });
    }

    pub fn add_error_with_file(&mut self, file: Option<String>, line: usize, column: usize, message: impl Into<String>) {
        self.errors.push(TypeInferenceError {
            line,
            column,
            message: message.into(),
            file,
        });
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.errors.len()
    }

    /// 将所有错误转移到语义分析器的错误列表中
    pub fn transfer_to(&self, analyzer: &mut super::analyzer::SemanticAnalyzer) {
        for err in &self.errors {
            analyzer.errors.push(super::analyzer::SemanticErrorInfo {
                line: err.line,
                column: err.column,
                message: err.message.clone(),
                file: err.file.clone(),
            });
        }
    }
}
