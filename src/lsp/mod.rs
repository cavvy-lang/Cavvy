//! Cavvy 语言服务器实现。
//!
//! 模块划分：
//! - [`util`]：UTF-16/UTF-8 位置转换、括号匹配、文档注释等文本工具；
//! - [`builtins`]：读取 `caylibs/__builtins__` 的内建类型/函数签名存根；
//! - [`model`]：从 AST 构建符号、引用、作用域与类型索引；
//! - [`analysis`]：预处理 → 词法 → 语法 → 语义 → 模型 的文档分析流水线；
//! - [`workspace`]：工作区文件索引（跨文件跳转/引用/工作区符号）；
//! - [`server`]：tower-lsp 服务实现。

pub mod analysis;
pub mod builtins;
pub mod model;
pub mod server;
pub mod util;
pub mod workspace;
