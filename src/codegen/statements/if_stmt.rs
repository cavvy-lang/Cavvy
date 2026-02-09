//! If语句代码生成
//!
//! 处理if-else语句的代码生成。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成 if 语句代码
    pub fn generate_if_statement(&mut self, if_stmt: &IfStmt) -> cayResult<()> {
        let then_label = self.new_label("then");
        let else_label = self.new_label("else");
        let merge_label = self.new_label("ifmerge");

        let cond = self.generate_expression(&if_stmt.condition)?;
        let (cond_type, cond_val) = self.parse_typed_value(&cond);
        let cond_reg = self.new_temp();
        
        // 将条件转换为 i1 类型
        if cond_type == "i1" {
            self.emit_line(&format!("  {} = icmp ne i1 {}, 0", cond_reg, cond_val));
        } else {
            // 对于整数类型，先与 0 比较
            self.emit_line(&format!("  {} = icmp ne {} {}, 0", cond_reg, cond_type, cond_val));
        }

        let has_else = if_stmt.else_branch.is_some();

        if has_else {
            self.emit_line(&format!("  br i1 {}, label %{}, label %{}",
                cond_reg, then_label, else_label));
        } else {
            self.emit_line(&format!("  br i1 {}, label %{}, label %{}",
                cond_reg, then_label, merge_label));
        }

        // then块
        self.emit_line(&format!("{}:", then_label));
        let then_code_before = self.code.len();
        self.generate_statement(&if_stmt.then_branch)?;
        let then_code_after = self.code.len();

        // 检查 then 块是否以终止指令结束
        let mut then_terminates = false;
        if then_code_after > then_code_before {
            let then_code = &self.code[then_code_before..then_code_after];
            let then_lines: Vec<&str> = then_code.trim().lines().collect();
            if let Some(last_line) = then_lines.last() {
                let trimmed = last_line.trim();
                if trimmed.starts_with("ret") || trimmed.starts_with("br") || trimmed.starts_with("switch") || trimmed.starts_with("unreachable") {
                    then_terminates = true;
                } else {
                    self.emit_line(&format!("  br label %{}", merge_label));
                }
            } else {
                self.emit_line(&format!("  br label %{}", merge_label));
            }
        } else {
            self.emit_line(&format!("  br label %{}", merge_label));
        }

        // else块
        let mut else_terminates = false;
        if let Some(else_branch) = if_stmt.else_branch.as_ref() {
            self.emit_line(&format!("{}:", else_label));
            let else_code_before = self.code.len();
            self.generate_statement(else_branch)?;
            let else_code_after = self.code.len();

            // 检查 else 块是否以终止指令结束
            if else_code_after > else_code_before {
                let else_code = &self.code[else_code_before..else_code_after];
                let else_lines: Vec<&str> = else_code.trim().lines().collect();
                if let Some(last_line) = else_lines.last() {
                    let trimmed = last_line.trim();
                    if trimmed.starts_with("ret") || trimmed.starts_with("br") || trimmed.starts_with("switch") || trimmed.starts_with("unreachable") {
                        else_terminates = true;
                    } else {
                        self.emit_line(&format!("  br label %{}", merge_label));
                    }
                } else {
                    self.emit_line(&format!("  br label %{}", merge_label));
                }
            } else {
                self.emit_line(&format!("  br label %{}", merge_label));
            }
        }

        // merge块
        self.emit_line(&format!("{}:", merge_label));

        // 只有当两个分支都以终止指令结束时，merge 才不可达
        // 特殊情况：如果没有 else，false 分支直接 fall-through 到 merge，所以 merge 一定可达
        let merge_is_unreachable = if has_else {
            then_terminates && else_terminates
        } else {
            false  // 无 else 时，merge 总是可达的
        };

        if merge_is_unreachable {
            self.emit_line("  unreachable");
        }
        // 否则，后续代码会在这个块中继续生成（不要加 unreachable）

        Ok(())
    }
}
