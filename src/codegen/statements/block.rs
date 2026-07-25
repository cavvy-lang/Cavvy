//! 语句块代码生成
//!
//! 处理语句块（带作用域管理）的代码生成。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;

impl IRGenerator {
    /// 生成语句块代码（带作用域管理）
    pub fn generate_block(&mut self, block: &Block) -> CayResult<()> {
        // DWARF 词法块作用域
        self.enter_debug_lexical_block(block.loc.line, block.loc.column);

        // 进入新作用域
        self.scope_manager.enter_scope();

        for stmt in &block.statements {
            self.generate_statement(stmt)?;
        }

        // 6.2.x: 语句位置的块尾表达式只求值后丢弃；
        // 块已被 return/break 终止时跳过，避免在终止指令后追加代码
        if let Some(tail) = &block.tail_expr {
            if !self.current_block_terminated() {
                self.generate_expression(tail)?;
            }
        }

        // ROADMAP 5.3.x 自动 RAII：作用域正常退出前，逆序调用本层带析构函数
        // 的局部变量的 `@ClassName.__dtor`。
        //
        // 若块已被 return 等终止指令结束，emit_scope_exit_dtors 会保留候选，
        // 由 return 语句统一调用 emit_all_scope_dtors，避免在 ret 后追加指令。
        self.emit_scope_exit_dtors();

        // 退出作用域
        self.scope_manager.exit_scope();
        self.exit_debug_lexical_block();
        Ok(())
    }

    /// 生成语句块代码（不带新作用域，用于函数体等已有作用域的场景）
    pub fn generate_block_without_scope(&mut self, block: &Block) -> CayResult<()> {
        for stmt in &block.statements {
            self.generate_statement(stmt)?;
        }
        // 6.2.x: 语句位置的块尾表达式只求值后丢弃；
        // 块已被 return/break 终止时跳过
        if let Some(tail) = &block.tail_expr {
            if !self.current_block_terminated() {
                self.generate_expression(tail)?;
            }
        }
        Ok(())
    }
}
