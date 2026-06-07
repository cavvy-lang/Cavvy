//! cay LLVM IR 代码生成器
//!
//! 本模块将 cay AST 转换为 LLVM IR 代码。
//! 已重构为多个子模块以提高可维护性。
//!
//! # 0.5.0.0 架构更新
//!
//! 引入 CodeGen-IR Builder 协作桥，支持两个IR生成系统的协同工作：
//! - CodeGen (IRGenerator): 主代码生成管线，直接生成LLVM IR文本
//! - IR Builder (IrBuilder): 结构化IR构建管线，支持高级特性如内联IR
//!
//! 通过 `bridge` 模块实现两者之间的安全协作。

pub mod allocator;
pub mod context;
mod expressions;
mod generator;
pub mod obfuscator;
mod platform;
pub mod runtime;
pub mod source_map;
mod statements;
mod types;

// 0.5.0.0: CodeGen-IR Builder 协作桥
pub mod bridge;

// 公开 IRGenerator 作为代码生成器的入口
pub use context::IRGenerator;

// 公开源映射相关类型
pub use source_map::{
    IRSourceMap, SourceMapEmitter, SourcePosition, parse_clang_error_line, remap_clang_error,
};

// 公开桥接相关类型
pub use bridge::{InlineIrBridge, InlineIrBridgeSupport, InlineIrResult};
