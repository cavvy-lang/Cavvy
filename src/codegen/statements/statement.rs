//! 语句分发代码生成
//!
//! 处理语句类型的分发。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::EolResult;

impl IRGenerator {
    /// 生成单个语句代码
    pub fn generate_statement(&mut self, stmt: &Stmt) -> EolResult<()> {
        match stmt {
            Stmt::Expr(expr) => {
                self.generate_expression(expr)?;
            }
            Stmt::VarDecl(var) => {
                self.generate_var_decl(var)?;
            }
            Stmt::Return(expr) => {
                self.generate_return_statement(expr)?;
            }
            Stmt::Block(block) => {
                self.generate_block(block)?;
            }
            Stmt::If(if_stmt) => {
                self.generate_if_statement(if_stmt)?;
            }
            Stmt::While(while_stmt) => {
                self.generate_while_statement(while_stmt)?;
            }
            Stmt::For(for_stmt) => {
                self.generate_for_statement(for_stmt)?;
            }
            Stmt::DoWhile(do_while_stmt) => {
                self.generate_do_while_statement(do_while_stmt)?;
            }
            Stmt::Switch(switch_stmt) => {
                self.generate_switch_statement(switch_stmt)?;
            }
            Stmt::Break => {
                self.generate_break_statement()?;
            }
            Stmt::Continue => {
                self.generate_continue_statement()?;
            }
        }
        Ok(())
    }
}
