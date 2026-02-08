//! 语句块代码生成
//!
//! 处理语句块（带作用域管理）的代码生成。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::EolResult;

impl IRGenerator {
    /// 生成语句块代码（带作用域管理）
    pub fn generate_block(&mut self, block: &Block) -> EolResult<()> {
        // 进入新作用域
        self.scope_manager.enter_scope();

        for stmt in &block.statements {
            self.generate_statement(stmt)?;
        }

        // 退出作用域
        self.scope_manager.exit_scope();
        Ok(())
    }

    /// 生成语句块代码（不带新作用域，用于函数体等已有作用域的场景）
    pub fn generate_block_without_scope(&mut self, block: &Block) -> EolResult<()> {
        for stmt in &block.statements {
            self.generate_statement(stmt)?;
        }
        Ok(())
    }
}
