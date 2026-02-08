//! Switch语句代码生成
//!
//! 处理switch-case语句的代码生成。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::EolResult;

impl IRGenerator {
    /// 生成 switch 语句代码
    pub fn generate_switch_statement(&mut self, switch_stmt: &SwitchStmt) -> EolResult<()> {
        let end_label = self.new_label("switch.end");
        let default_label = if switch_stmt.default.is_some() {
            self.new_label("switch.default")
        } else {
            end_label.clone()
        };

        // 生成条件表达式
        let expr = self.generate_expression(&switch_stmt.expr)?;
        let (expr_type, expr_val) = self.parse_typed_value(&expr);

        // 创建 case 标签
        let mut case_labels: Vec<(i64, String, usize)> = Vec::new();
        for (idx, case) in switch_stmt.cases.iter().enumerate() {
            let label = self.new_label(&format!("switch.case.{}", case.value));
            case_labels.push((case.value, label, idx));
        }

        // 将表达式值转换为 i64（如果还不是的话）
        let switch_val = if expr_type == "i64" {
            expr_val.to_string()
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to i64", temp, expr_type, expr_val));
            temp
        };

        // 生成 switch 指令
        self.emit_line(&format!("  switch i64 {}, label %{} [", switch_val, default_label));
        for (value, label, _) in &case_labels {
            self.emit_line(&format!("    i64 {}, label %{}", value, label));
        }
        self.emit_line("  ]");

        // 生成 case 块
        let mut fallthrough = false;
        for i in 0..case_labels.len() {
            let (value, label, case_idx) = &case_labels[i];
            let case = &switch_stmt.cases[*case_idx];
            self.emit_line(&format!("{}:", label));

            // 执行 case 体
            if case.body.is_empty() {
                // 空的 case 体，直接穿透到下一个 case
                fallthrough = true;
            } else {
                for (j, stmt) in case.body.iter().enumerate() {
                    match stmt {
                        Stmt::Break => {
                            // 遇到 break，跳转到 switch 结束
                            self.emit_line(&format!("  br label %{}", end_label));
                            fallthrough = false;
                            break;
                        }
                        _ => {
                            self.generate_statement(stmt)?;
                            // 如果不是最后一条，继续执行
                            if j == case.body.len() - 1 {
                                // 最后一条语句，检查是否需要穿透
                                fallthrough = true;
                            }
                        }
                    }
                }
            }

            // 如果不是 break，穿透到下一个 case
            if fallthrough && i < case_labels.len() - 1 {
                let (_, next_label, _) = &case_labels[i + 1];
                self.emit_line(&format!("  br label %{}", next_label));
                fallthrough = false;
            } else if fallthrough {
                // 最后一个 case 没有 break，穿透到 default 或结束
                if switch_stmt.default.is_some() {
                    self.emit_line(&format!("  br label %{}", default_label));
                } else {
                    self.emit_line(&format!("  br label %{}", end_label));
                }
                fallthrough = false;
            }
        }

        // 生成 default 块
        if let Some(default_body) = switch_stmt.default.as_ref() {
            self.emit_line(&format!("{}:", default_label));
            for stmt in default_body {
                match stmt {
                    Stmt::Break => {
                        self.emit_line(&format!("  br label %{}", end_label));
                        break;
                    }
                    _ => {
                        self.generate_statement(stmt)?;
                    }
                }
            }
            // 确保 default 最后跳转到结束
            self.emit_line(&format!("  br label %{}", end_label));
        }

        // 结束块
        self.emit_line(&format!("{}:", end_label));

        Ok(())
    }
}
