//! 跳转语句代码生成
//!
//! 处理break和continue语句的代码生成。

use crate::codegen::context::IRGenerator;
use crate::error::{EolResult, codegen_error};

impl IRGenerator {
    /// 生成 break 语句代码
    pub fn generate_break_statement(&mut self) -> EolResult<()> {
        if let Some(loop_ctx) = self.current_loop() {
            self.emit_line(&format!("  br label %{}", loop_ctx.end_label));
        } else {
            return Err(codegen_error("break statement outside of loop".to_string()));
        }
        Ok(())
    }

    /// 生成 continue 语句代码
    pub fn generate_continue_statement(&mut self) -> EolResult<()> {
        if let Some(loop_ctx) = self.current_loop() {
            self.emit_line(&format!("  br label %{}", loop_ctx.cond_label));
        } else {
            return Err(codegen_error("continue statement outside of loop".to_string()));
        }
        Ok(())
    }
}
