//! 语句代码生成（包含所有控制流结构）
use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::types::Type;
use crate::error::{EolResult, codegen_error};

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

    /// 生成单个语句代码
    pub fn generate_statement(&mut self, stmt: &Stmt) -> EolResult<()> {
        match stmt {
            Stmt::Expr(expr) => {
                self.generate_expression(expr)?;
            }
            Stmt::VarDecl(var) => {
                let var_type = self.type_to_llvm(&var.var_type);
                let align = self.get_type_align(&var_type);  // 获取对齐

                // 使用作用域管理器生成唯一的 LLVM 变量名
                let llvm_name = self.scope_manager.declare_var(&var.name, &var_type);

                self.emit_line(&format!("  %{} = alloca {}, align {}", llvm_name, var_type, align));
                // 同时存储到旧系统以保持兼容性
                self.var_types.insert(var.name.clone(), var_type.clone());
                // 如果变量类型是对象，记录其类名以便后续方法调用解析
                if let Type::Object(class_name) = &var.var_type {
                    self.var_class_map.insert(var.name.clone(), class_name.clone());
                }

                if let Some(init) = var.initializer.as_ref() {
                    let value = self.generate_expression(init)?;
                    let (value_type, val) = self.parse_typed_value(&value);

                    // 如果值类型与变量类型不匹配，需要转换
                    if value_type != var_type {
                        let temp = self.new_temp();

                        // 浮点类型转换
                        if value_type == "double" && var_type == "float" {
                            // double -> float 转换
                            self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                            let align = self.get_type_align("float");
                            self.emit_line(&format!("  store float {}, float* %{}, align {}", temp, llvm_name, align));
                        } else if value_type == "float" && var_type == "double" {
                            // float -> double 转换
                            self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                            let align = self.get_type_align("double");
                            self.emit_line(&format!("  store double {}, double* %{}, align {}", temp, llvm_name, align));
                        }
                        // 指针类型转换 (bitcast)
                        else if value_type.ends_with("*") && var_type.ends_with("*") {
                            self.emit_line(&format!("  {} = bitcast {} {} to {}",
                                temp, value_type, val, var_type));
                            self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                        }
                        // 整数类型转换
                        else if value_type.starts_with("i") && var_type.starts_with("i") && !value_type.ends_with("*") && !var_type.ends_with("*") {
                            let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                            let to_bits: u32 = var_type.trim_start_matches('i').parse().unwrap_or(64);

                            if to_bits > from_bits {
                                // 符号扩展
                                self.emit_line(&format!("  {} = sext {} {} to {}",
                                    temp, value_type, val, var_type));
                            } else {
                                // 截断
                                self.emit_line(&format!("  {} = trunc {} {} to {}",
                                    temp, value_type, val, var_type));
                            }
                            self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                        } else {
                            // 类型不兼容，直接存储（可能会出错）
                            self.emit_line(&format!("  store {}, {}* %{}",
                                value, var_type, llvm_name));
                        }
                    } else {
                        // 类型匹配，直接存储
                        self.emit_line(&format!("  store {}, {}* %{}",
                            value, var_type, llvm_name));
                    }
                }
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr.as_ref() {
                    let value = self.generate_expression(e)?;
                    let (value_type, val) = self.parse_typed_value(&value);
                    let ret_type = self.current_return_type.clone();
                    
                    // 如果返回类型是 void，但表达式非空，这是错误（但由语义分析处理）
                    if ret_type == "void" {
                        self.emit_line("  ret void");
                    } else if value_type != ret_type {
                        // 需要类型转换
                        let temp = self.new_temp();
                        
                        // 浮点类型转换
                        if value_type == "double" && ret_type == "float" {
                            // double -> float 转换
                            self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                            let align = self.get_type_align("float");
                            self.emit_line(&format!("  ret float {}", temp));
                        } else if value_type == "float" && ret_type == "double" {
                            // float -> double 转换
                            self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                            let align = self.get_type_align("double");
                            self.emit_line(&format!("  ret double {}", temp));
                        }
                        // 整数类型转换
                        else if value_type.starts_with("i") && ret_type.starts_with("i") {
                            let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                            let to_bits: u32 = ret_type.trim_start_matches('i').parse().unwrap_or(64);
                            
                            if to_bits > from_bits {
                                // 符号扩展
                                self.emit_line(&format!("  {} = sext {} {} to {}",
                                    temp, value_type, val, ret_type));
                            } else {
                                // 截断
                                self.emit_line(&format!("  {} = trunc {} {} to {}",
                                    temp, value_type, val, ret_type));
                            }
                            self.emit_line(&format!("  ret {} {}", ret_type, temp));
                        } else {
                            // 类型不兼容，直接返回（可能会出错）
                            self.emit_line(&format!("  ret {}", value));
                        }
                    } else {
                        // 类型匹配，直接返回
                        self.emit_line(&format!("  ret {}", value));
                    }
                } else {
                    self.emit_line("  ret void");
                }
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

    /// 生成 if 语句代码
    pub fn generate_if_statement(&mut self, if_stmt: &IfStmt) -> EolResult<()> {
        let then_label = self.new_label("then");
        let else_label = self.new_label("else");
        let merge_label = self.new_label("ifmerge");

        let cond = self.generate_expression(&if_stmt.condition)?;
        let (_, cond_val) = self.parse_typed_value(&cond);
        let cond_reg = self.new_temp();
        self.emit_line(&format!("  {} = icmp ne i1 {}, 0", cond_reg, cond_val));

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

    /// 生成 while 语句代码
    pub fn generate_while_statement(&mut self, while_stmt: &WhileStmt) -> EolResult<()> {
        let cond_label = self.new_label("while.cond");
        let body_label = self.new_label("while.body");
        let end_label = self.new_label("while.end");

        // 进入循环上下文
        self.enter_loop(cond_label.clone(), end_label.clone());

        self.emit_line(&format!("  br label %{}", cond_label));

        // 条件块
        self.emit_line(&format!("{}:", cond_label));
        let cond = self.generate_expression(&while_stmt.condition)?;
        let (_, cond_val) = self.parse_typed_value(&cond);
        let cond_reg = self.new_temp();
        self.emit_line(&format!("  {} = icmp ne i1 {}, 0", cond_reg, cond_val));
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
    pub fn generate_for_statement(&mut self, for_stmt: &ForStmt) -> EolResult<()> {
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
            let (_, cond_val) = self.parse_typed_value(&cond);
            let cond_reg = self.new_temp();
            self.emit_line(&format!("  {} = icmp ne i1 {}, 0", cond_reg, cond_val));
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
    pub fn generate_do_while_statement(&mut self, do_while_stmt: &DoWhileStmt) -> EolResult<()> {
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
        let (_, cond_val) = self.parse_typed_value(&cond);
        let cond_reg = self.new_temp();
        self.emit_line(&format!("  {} = icmp ne i1 {}, 0", cond_reg, cond_val));
        self.emit_line(&format!("  br i1 {}, label %{}, label %{}",
            cond_reg, body_label, end_label));

        // 结束块
        self.emit_line(&format!("{}:", end_label));

        // 退出循环上下文
        self.exit_loop();

        Ok(())
    }

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

    /// 生成 break 语句代码
    fn generate_break_statement(&mut self) -> EolResult<()> {
        if let Some(loop_ctx) = self.current_loop() {
            self.emit_line(&format!("  br label %{}", loop_ctx.end_label));
        } else {
            return Err(codegen_error("break statement outside of loop".to_string()));
        }
        Ok(())
    }

    /// 生成 continue 语句代码
    fn generate_continue_statement(&mut self) -> EolResult<()> {
        if let Some(loop_ctx) = self.current_loop() {
            self.emit_line(&format!("  br label %{}", loop_ctx.cond_label));
        } else {
            return Err(codegen_error("continue statement outside of loop".to_string()));
        }
        Ok(())
    }
}
