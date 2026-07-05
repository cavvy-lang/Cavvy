//! 语句块代码生成
//!
//! 处理语句块（带作用域管理）的代码生成。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;

impl IRGenerator {
    /// 生成语句块代码（带作用域管理）
    pub fn generate_block(&mut self, block: &Block) -> CayResult<()> {
        // 进入新作用域
        self.scope_manager.enter_scope();

        for stmt in &block.statements {
            self.generate_statement(stmt)?;
        }

        // ROADMAP 5.3.x 自动 RAII：作用域正常退出前，逆序调用本层带析构函数
        // 的局部变量的 `@ClassName.__dtor`。
        //
        // 若块已被 return 等终止指令结束，emit_scope_exit_dtors 会保留候选，
        // 由 return 语句统一调用 emit_all_scope_dtors，避免在 ret 后追加指令。
        self.emit_scope_exit_dtors();

        // 退出作用域
        self.scope_manager.exit_scope();
        Ok(())
    }

    /// 生成语句块代码（不带新作用域，用于函数体等已有作用域的场景）
    pub fn generate_block_without_scope(&mut self, block: &Block) -> CayResult<()> {
        for stmt in &block.statements {
            self.generate_statement(stmt)?;
        }
        Ok(())
    }
}
