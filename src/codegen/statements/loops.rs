//! 循环语句代码生成
//!
//! 处理while、for、do-while循环的代码生成。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;
use crate::types::Type;

impl IRGenerator {
    /// 生成 while 语句代码
    pub fn generate_while_statement(&mut self, while_stmt: &WhileStmt) -> CayResult<()> {
        let cond_label = self.new_label("while.cond");
        let body_label = self.new_label("while.body");
        let end_label = self.new_label("while.end");

        // 进入循环上下文
        self.enter_loop(
            cond_label.clone(),
            end_label.clone(),
            while_stmt.label.clone(),
        );

        self.emit_line(&format!("  br label %{}", cond_label));

        // 条件块
        self.emit_line(&format!("{}:", cond_label));
        let cond = self.generate_expression(&while_stmt.condition)?;
        let (cond_type, cond_val) = self.parse_typed_value(&cond);
        let cond_reg = self.new_temp();
        if cond_type == "i1" {
            self.emit_line(&format!("  {} = icmp ne i1 {}, 0", cond_reg, cond_val));
        } else {
            self.emit_line(&format!(
                "  {} = icmp ne {} {}, 0",
                cond_reg, cond_type, cond_val
            ));
        }
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            cond_reg, body_label, end_label
        ));

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
    pub fn generate_for_statement(&mut self, for_stmt: &ForStmt) -> CayResult<()> {
        let cond_label = self.new_label("for.cond");
        let body_label = self.new_label("for.body");
        let update_label = self.new_label("for.update");
        let end_label = self.new_label("for.end");

        // 初始化部分
        if let Some(init) = for_stmt.init.as_ref() {
            self.generate_for_initializer(init)?;
        }

        // 进入循环上下文（continue 跳转到 update 标签）
        self.enter_loop(
            update_label.clone(),
            end_label.clone(),
            for_stmt.label.clone(),
        );

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
                self.emit_line(&format!(
                    "  {} = icmp ne {} {}, 0",
                    cond_reg, cond_type, cond_val
                ));
            }
            self.emit_line(&format!(
                "  br i1 {}, label %{}, label %{}",
                cond_reg, body_label, end_label
            ));
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

    fn generate_for_initializer(&mut self, init: &Stmt) -> CayResult<()> {
        match init {
            Stmt::VarDecl(var) if self.scope_manager.get_var_type(&var.name).is_some() => {
                if let Some(initializer) = &var.initializer {
                    let assign = AssignmentExpr {
                        target: Box::new(Expr::Identifier(IdentifierExpr {
                            name: var.name.clone(),
                            loc: var.loc.clone(),
                        })),
                        value: Box::new(initializer.clone()),
                        op: AssignOp::Assign,
                        loc: var.loc.clone(),
                    };
                    self.generate_assignment(&assign)?;
                }
                Ok(())
            }
            Stmt::Block(block)
                if block
                    .statements
                    .iter()
                    .all(|s| matches!(s, Stmt::VarDecl(_))) =>
            {
                for stmt in &block.statements {
                    self.generate_for_initializer(stmt)?;
                }
                Ok(())
            }
            _ => self.generate_statement(init),
        }
    }

    /// 生成 do-while 语句代码
    pub fn generate_do_while_statement(&mut self, do_while_stmt: &DoWhileStmt) -> CayResult<()> {
        let body_label = self.new_label("dowhile.body");
        let cond_label = self.new_label("dowhile.cond");
        let end_label = self.new_label("dowhile.end");

        // 进入循环上下文
        self.enter_loop(
            cond_label.clone(),
            end_label.clone(),
            do_while_stmt.label.clone(),
        );

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
            self.emit_line(&format!(
                "  {} = icmp ne {} {}, 0",
                cond_reg, cond_type, cond_val
            ));
        }
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            cond_reg, body_label, end_label
        ));

        // 结束块
        self.emit_line(&format!("{}:", end_label));

        // 退出循环上下文
        self.exit_loop();

        Ok(())
    }

    /// 生成增强 for 语句代码
    ///
    /// 将 `for (T x : iterable) body` 解糖为：
    /// ```cay
    /// {
    ///     Iterator<T> __cay_iter = iterable.iterator();
    ///     while (__cay_iter.hasNext()) {
    ///         T x = __cay_iter.next();
    ///         body
    ///     }
    /// }
    /// ```
    pub fn generate_for_each_statement(&mut self, for_each: &ForEachStmt) -> CayResult<()> {
        let iter_idx = self.temp_counter;
        self.temp_counter += 1;
        let iter_var = format!("__cay_iter_{}", iter_idx);
        let loc = for_each.loc.clone();

        // 构造 iterable.iterator() 方法调用表达式
        let iterator_call = Expr::Call(CallExpr {
            callee: Box::new(Expr::MemberAccess(MemberAccessExpr {
                object: Box::new(for_each.iterable.clone()),
                member: "iterator".to_string(),
                loc: loc.clone(),
            })),
            args: vec![],
            loc: loc.clone(),
        });

        // 使用 auto 推断迭代器具体类型，避免依赖 Iterator<T> 接口赋值
        let iterator_type = Type::Auto;

        // 构造迭代器变量声明: auto __cay_iter = iterable.iterator();
        let iter_decl = Stmt::VarDecl(VarDecl {
            name: iter_var.clone(),
            var_type: iterator_type,
            initializer: Some(iterator_call),
            is_final: true,
            loc: loc.clone(),
        });

        let iter_identifier = Expr::Identifier(IdentifierExpr {
            name: iter_var.clone(),
            loc: loc.clone(),
        });

        // 构造 __cay_iter.hasNext() 条件表达式
        let has_next_expr = Expr::Call(CallExpr {
            callee: Box::new(Expr::MemberAccess(MemberAccessExpr {
                object: Box::new(iter_identifier.clone()),
                member: "hasNext".to_string(),
                loc: loc.clone(),
            })),
            args: vec![],
            loc: loc.clone(),
        });

        // 构造 T x = __cay_iter.next();
        let next_call = Expr::Call(CallExpr {
            callee: Box::new(Expr::MemberAccess(MemberAccessExpr {
                object: Box::new(iter_identifier),
                member: "next".to_string(),
                loc: loc.clone(),
            })),
            args: vec![],
            loc: loc.clone(),
        });
        let var_decl = Stmt::VarDecl(VarDecl {
            name: for_each.var_name.clone(),
            var_type: for_each.var_type.clone(),
            initializer: Some(next_call),
            is_final: true,
            loc: loc.clone(),
        });

        // 构造循环体：先声明变量，再执行原 body
        let body_block = Stmt::Block(Block {
            statements: vec![var_decl, (*for_each.body).clone()],
            tail_expr: None,
            loc: loc.clone(),
        });

        // 构造 while 循环
        let while_stmt = Stmt::While(WhileStmt {
            condition: has_next_expr,
            body: Box::new(body_block),
            label: for_each.label.clone(),
            loc: loc.clone(),
        });

        // 包装成 block 以避免迭代器变量泄漏到外部作用域
        let desugared = Stmt::Block(Block {
            statements: vec![iter_decl, while_stmt],
            tail_expr: None,
            loc: loc.clone(),
        });

        self.generate_statement(&desugared)
    }
}
