//! cay 语义分析器
//!
//! 本模块负责 cay 语言的语义分析和类型检查。
//! 已重构为多个子模块以提高可维护性。

// 子模块声明
mod symbol_table;
mod analyzer;
mod class_analysis;
mod type_check;
mod expr_inference;
mod type_utils;
mod type_inference_result;

// 公开导出
pub use symbol_table::{SemanticSymbolTable, SemanticSymbolInfo};
pub use analyzer::{SemanticAnalyzer, SemanticErrorInfo};
pub use type_inference_result::{TypeInferenceResult, TypeInferenceError, TypeInferenceErrorCollector};
