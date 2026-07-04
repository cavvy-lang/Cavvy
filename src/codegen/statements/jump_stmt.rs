//! 跳转语句代码生成
//!
//! 处理break和continue语句的代码生成。

use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, SourceLocation, codegen_error_at};

impl IRGenerator {
    /// 生成 break 语句代码
    pub fn generate_break_statement(
        &mut self,
        label: &Option<String>,
        loc: SourceLocation,
    ) -> CayResult<()> {
        let loop_ctx = if let Some(label_name) = label {
            // 带标签的 break
            self.get_loop_by_label(label_name).ok_or_else(|| {
                codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    loc.clone(),
                    format!("break label '{}' not found", label_name),
                )
            })?
        } else {
            // 不带标签的 break
            self.current_loop().ok_or_else(|| {
                codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    loc.clone(),
                    "break statement outside of loop".to_string(),
                )
            })?
        };
        self.emit_line(&format!("  br label %{}", loop_ctx.end_label));
        Ok(())
    }

    /// 生成 continue 语句代码
    pub fn generate_continue_statement(
        &mut self,
        label: &Option<String>,
        loc: SourceLocation,
    ) -> CayResult<()> {
        let loop_ctx = if let Some(label_name) = label {
            // 带标签的 continue
            self.get_loop_by_label(label_name).ok_or_else(|| {
                codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    loc.clone(),
                    format!("continue label '{}' not found", label_name),
                )
            })?
        } else {
            // 不带标签的 continue
            self.current_loop().ok_or_else(|| {
                codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    loc.clone(),
                    "continue statement outside of loop".to_string(),
                )
            })?
        };
        self.emit_line(&format!("  br label %{}", loop_ctx.cond_label));
        Ok(())
    }
}
