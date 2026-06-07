//! cay 语义分析器
//!
//! 本模块负责 cay 语言的语义分析和类型检查。
//! 已重构为多个子模块以提高可维护性。

// 子模块声明
mod analyzer;
mod class_analysis;
mod expr_inference;
mod symbol_table;
mod type_check;
mod type_inference_result;
mod type_utils;

// 公开导出
pub use analyzer::{SemanticAnalyzer, SemanticErrorInfo};
pub use symbol_table::{SemanticSymbolInfo, SemanticSymbolTable};
pub use type_inference_result::{
    TypeInferenceError, TypeInferenceErrorCollector, TypeInferenceResult,
};
pub use type_utils::resolve_call_args;
