//! 循环语句代码生成
//!
//! 处理while、for、do-while循环的代码生成。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成 while 语句代码
    pub fn generate_while_statement(&mut self, while_stmt: &WhileStmt) -> cayResult<()> {
        let cond_label = self.new_label("while.cond");
        let body_label = self.new_label("while.body");
        let end_label = self.new_label("while.end");

        // 进入循环上下文
        self.enter_loop(cond_label.clone(), end_label.clone());

        self.emit_line(&format!("  br label %{}", cond_label));

        // 条件块
        self.emit_line(&format!("{}:", cond_label));
        let cond = self.generate_expression(&while_stmt.condition)?;
        let (cond_type, cond_val) = self.parse_typed_value(&cond);
        let cond_reg = self.new_temp();
        if cond_type == "i1" {
            self.emit_line(&format!("  {} = icmp ne i1 {}, 0", cond_reg, cond_val));
        } else {
            self.emit_line(&format!("  {} = icmp ne {} {}, 0", cond_reg, cond_type, cond_val));
        }
        self.emit_line(&format!("  br i1 {}, label %{}, label %{}",
            cond_reg, body_label, end_label));

        // 循环体
        self.emit_line(&format!("{}:", body_label));
        self.generate_statement(&while_stmt.body)?;
        self.emit_line(&format!("  br label %{}", cond_label));

        // 结束块
        self.emit_line(&format!("{}:", end_label));

        // 退出循环上下文
        self.exit_loop();

        Ok(())
    }

    /// 生成 for 语句代码
    pub fn generate_for_statement(&mut self, for_stmt: &ForStmt) -> cayResult<()> {
        let cond_label = self.new_label("for.cond");
        let body_label = self.new_label("for.body");
        let update_label = self.new_label("for.update");
        let end_label = self.new_label("for.end");

        // 初始化部分
        if let Some(init) = for_stmt.init.as_ref() {
            self.generate_statement(init)?;
        }

        // 进入循环上下文（continue 跳转到 update 标签）
        self.enter_loop(update_label.clone(), end_label.clone());

        self.emit_line(&format!("  br label %{}", cond_label));

        // 条件块
        self.emit_line(&format!("{}:", cond_label));
        if let Some(condition) = for_stmt.condition.as_ref() {
            let cond = self.generate_expression(condition)?;
            let (cond_type, cond_val) = self.parse_typed_value(&cond);
            let cond_reg = self.new_temp();
            if cond_type == "i1" {
                self.emit_line(&format!("  {} = icmp ne i1 {}, 0", cond_reg, cond_val));
            } else {
                self.emit_line(&format!("  {} = icmp ne {} {}, 0", cond_reg, cond_type, cond_val));
            }
            self.emit_line(&format!("  br i1 {}, label %{}, label %{}",
                cond_reg, body_label, end_label));
        } else {
            // 无条件时默认跳转到循环体（无限循环）
            self.emit_line(&format!("  br label %{}", body_label));
        }

        // 循环体
        self.emit_line(&format!("{}:", body_label));
        self.generate_statement(&for_stmt.body)?;
        self.emit_line(&format!("  br label %{}", update_label));

        // 更新块
        self.emit_line(&format!("{}:", update_label));
        if let Some(update) = for_stmt.update.as_ref() {
            self.generate_expression(update)?;
        }
        self.emit_line(&format!("  br label %{}", cond_label));

        // 结束块
        self.emit_line(&format!("{}:", end_label));

        // 退出循环上下文
        self.exit_loop();

        Ok(())
    }

    /// 生成 do-while 语句代码
    pub fn generate_do_while_statement(&mut self, do_while_stmt: &DoWhileStmt) -> cayResult<()> {
        let body_label = self.new_label("dowhile.body");
        let cond_label = self.new_label("dowhile.cond");
        let end_label = self.new_label("dowhile.end");

        // 进入循环上下文
        self.enter_loop(cond_label.clone(), end_label.clone());

        // 先执行循环体
        self.emit_line(&format!("  br label %{}", body_label));
        self.emit_line(&format!("{}:", body_label));
        self.generate_statement(&do_while_stmt.body)?;
        self.emit_line(&format!("  br label %{}", cond_label));

        // 条件检查
        self.emit_line(&format!("{}:", cond_label));
        let cond = self.generate_expression(&do_while_stmt.condition)?;
        let (cond_type, cond_val) = self.parse_typed_value(&cond);
        let cond_reg = self.new_temp();
        if cond_type == "i1" {
            self.emit_line(&format!("  {} = icmp ne i1 {}, 0", cond_reg, cond_val));
        } else {
            self.emit_line(&format!("  {} = icmp ne {} {}, 0", cond_reg, cond_type, cond_val));
        }
        self.emit_line(&format!("  br i1 {}, label %{}, label %{}",
            cond_reg, body_label, end_label));

        // 结束块
        self.emit_line(&format!("{}:", end_label));

        // 退出循环上下文
        self.exit_loop();

        Ok(())
    }
}
