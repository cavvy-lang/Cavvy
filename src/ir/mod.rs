//! EOL/Cavvy 语句 IR (Statement Intermediate Representation)
//!
//! # 概述
//!
//! 本模块实现了一个灵活的、生产级的语句级中间表示（IR），
//! 位于 AST 和代码生成后端之间。
//!
//! # 核心特性
//!
//! - **后端无关**: 可生成 LLVM IR 文本、字节码或其他目标格式
//! - **SSA 风格**: 值不可变，通过寄存器引用
//! - **类型安全**: 每个值和指令都带类型信息
//! - **可优化**: 支持 IR 级别的优化 pass（内联、死代码消除等）
//! - **可序列化**: 支持 .cayir 格式（计划中）
//! - **内联 IR**: 支持嵌入原始 LLVM IR 代码片段
//!
//! # 架构
//!
//! ```text
//! AST ──► IR Builder ──► IrModule ──► LLVM Backend ──► .ll 文件
//!                  │                    │
//!                  │                    ├──► Inliner Pass
//!                  │                    ├──► Verification
//!                  │                    └──► Bytecode Backend (未来)
//!                  │
//!                  └──► 内联 IR 解析器 (__ir { ... })
//! ```
//!
//! # 使用示例
//!
//! ```rust,ignore
//! use crate::ir::{IrModule, IrBuilder, LlvmBackend};
//!
//! // 从 AST 构建 IR
//! let mut builder = IrBuilder::new();
//! let module = builder.build_from_ast(&ast)?;
//!
//! // 验证 IR
//! module.verify()?;
//!
//! // 优化
//! let mut inliner = Inliner::new();
//! let module = inliner.run(module)?;
//!
//! // 发射 LLVM IR 文本
//! let backend = LlvmBackend::new();
//! let llvm_ir = backend.emit(&module)?;
//! ```

pub mod block;
pub mod builder;
pub mod function;
pub mod inline_ir;
pub mod inliner;
pub mod llvm_backend;
pub mod module;
pub mod types;
pub mod value;
pub mod verification;

#[cfg(test)]
mod integration_tests;

// 核心类型重导出
pub use block::IrBasicBlock;
pub use builder::IrBuilder;
pub use function::{IrFunction, IrLinkage, IrParam};
pub use inline_ir::{InlineIrBlock, InlineIrParser};
pub use inliner::{Inliner, InlinerConfig};
pub use llvm_backend::LlvmBackend;
pub use module::{
    IrExternDecl, IrGlobal, IrGlobalLinkage, IrModule, IrModuleStats, IrStringConstant, IrTypeDecl,
};
pub use types::IrType;
pub use value::{IrBinaryOp, IrCastKind, IrCmpOp, IrInstruction, IrTerminator, IrValue};
pub use verification::IrVerifier;
