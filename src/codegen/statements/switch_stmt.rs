//! Switch语句代码生成
//!
//! 处理switch-case语句的代码生成。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::error::{cayResult, codegen_error_at};

impl IRGenerator {
    /// 将 CaseValue 转换为 i64 常量值
    /// 对于 enum variant，查找其在 enum 定义中的索引
    fn resolve_case_value(&self, case: &Case) -> cayResult<i64> {
        match &case.value {
            CaseValue::Integer(v) => Ok(*v),
            CaseValue::EnumVariant {
                enum_name,
                variant_name,
            } => {
                // 查找 enum 定义
                if let Some(ref registry) = self.type_registry {
                    if let Some(enum_info) = registry.get_enum(enum_name) {
                        // 在 variants 中查找 variant 的索引
                        for (idx, variant) in enum_info.variants.iter().enumerate() {
                            if variant.name == *variant_name {
                                return Ok(idx as i64);
                            }
                        }
                        return Err(codegen_error_at(
                            case.loc.clone(),
                            format!("enum '{}' 中不存在 variant '{}'", enum_name, variant_name),
                        ));
                    }
                }
                Err(codegen_error_at(
                    case.loc.clone(),
                    format!("未知的 enum '{}' 在 case 标签中", enum_name),
                ))
            }
        }
    }

    /// 生成 switch 语句代码
    pub fn generate_switch_statement(&mut self, switch_stmt: &SwitchStmt) -> cayResult<()> {
        let end_label = self.new_label("switch.end");
        let default_label = if switch_stmt.default.is_some() {
            self.new_label("switch.default")
        } else {
            end_label.clone()
        };

        // 将 switch 上下文压入 loop_stack，使 case 体中的 break 可以跳转到 switch.end
        self.enter_loop(end_label.clone(), end_label.clone(), None);

        // 生成条件表达式
        let expr = self.generate_expression(&switch_stmt.expr)?;
        let (mut expr_type, mut expr_val) = self.parse_typed_value(&expr);

        // 保存 enum struct 指针/值引用，供后续 payload 提取使用
        let enum_struct_ref = if expr_type.starts_with("{ i32, i64 }") {
            Some((expr_type.clone(), expr_val.clone()))
        } else {
            None
        };

        // 对于 enum struct 类型，提取 discriminant 用于 switch 比较
        if expr_type.starts_with("{ i32, i64 }") {
            let disc = self.new_temp();
            self.emit_line(&format!(
                "  {} = extractvalue {} {}, 0",
                disc, expr_type, expr_val
            ));
            expr_type = "i32".to_string();
            expr_val = disc;
        }

        // 创建 case 标签，将 CaseValue 解析为 i64
        let mut case_labels: Vec<(i64, String, usize)> = Vec::new();
        for (idx, case) in switch_stmt.cases.iter().enumerate() {
            let value = self.resolve_case_value(case)?;
            let label = self.new_label(&format!("switch.case.{}", value));
            case_labels.push((value, label, idx));
        }

        // 将表达式值转换为 i64（如果还不是的话）
        let switch_val = if expr_type == "i64" {
            expr_val.to_string()
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = sext {} {} to i64",
                temp, expr_type, expr_val
            ));
            temp
        };

        // 生成 switch 指令
        self.emit_line(&format!(
            "  switch i64 {}, label %{} [",
            switch_val, default_label
        ));
        for (value, label, _) in &case_labels {
            self.emit_line(&format!("    i64 {}, label %{}", value, label));
        }
        self.emit_line("  ]");

        // 跟踪是否所有分支都终止（return）- break 不算终止，因为它只是跳转到 switch.end
        let mut all_cases_terminate = true;

        // 生成 case 块
        let mut fallthrough = false;
        for i in 0..case_labels.len() {
            let (value, label, case_idx) = &case_labels[i];
            let case = &switch_stmt.cases[*case_idx];
            self.emit_line(&format!("{}:", label));

            // 处理 enum variant payload 绑定: case EnumName.Variant(Type var_name):
            if let Some(ref binding) = case.payload_binding {
                let var_type = self.type_to_llvm(&binding.var_type);
                let align = self.get_type_align(&var_type);
                let llvm_name = self.scope_manager.declare_var(&binding.var_name, &var_type);
                self.emit_line(&format!(
                    "  %{} = alloca {}, align {}",
                    llvm_name, var_type, align
                ));
                self.var_types
                    .insert(binding.var_name.clone(), var_type.clone());
                self.var_cay_types
                    .insert(binding.var_name.clone(), binding.var_type.clone());
                match &binding.var_type {
                    crate::types::Type::Object(class_name) => {
                        self.var_class_map
                            .insert(binding.var_name.clone(), class_name.clone());
                    }
                    crate::types::Type::Generic(class_name, _) => {
                        self.var_class_map
                            .insert(binding.var_name.clone(), class_name.clone());
                    }
                    _ => {}
                }
                // 从 enum struct 中提取 payload
                let store_val = if let Some((ref st_type, ref st_val)) = enum_struct_ref {
                    // extractvalue 获取 field 1 (payload as i64)
                    let pl_i64 = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = extractvalue {} {}, 1",
                        pl_i64, st_type, st_val
                    ));
                    // 转换 i64 payload 到目标类型
                    match var_type.as_str() {
                        "i32" => {
                            let trunc = self.new_temp();
                            self.emit_line(&format!("  {} = trunc i64 {} to i32", trunc, pl_i64));
                            format!("i32 {}", trunc)
                        }
                        "i64" => format!("i64 {}", pl_i64),
                        "i8*" | _ if var_type.ends_with('*') => {
                            let ptr = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = inttoptr i64 {} to {}",
                                ptr, pl_i64, var_type
                            ));
                            format!("{} {}", var_type, ptr)
                        }
                        _ => {
                            let trunc = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = trunc i64 {} to {}",
                                trunc, pl_i64, var_type
                            ));
                            format!("{} {}", var_type, trunc)
                        }
                    }
                } else {
                    match var_type.as_str() {
                        "i32" => "i32 0".to_string(),
                        "i64" => "i64 0".to_string(),
                        "i1" => "i1 0".to_string(),
                        "float" => "float 0.0".to_string(),
                        "double" => "double 0.0".to_string(),
                        "i8" => "i8 0".to_string(),
                        _ if var_type.ends_with('*') => format!("{} null", var_type),
                        _ => format!("{} 0", var_type),
                    }
                };
                self.emit_line(&format!(
                    "  store {}, {}* %{}",
                    store_val, var_type, llvm_name
                ));
            }

            // 执行 case 体
            if case.body.is_empty() {
                // 空的 case 体，直接穿透到下一个 case
                fallthrough = true;
                all_cases_terminate = false;
            } else {
                let mut has_return = false;
                for (j, stmt) in case.body.iter().enumerate() {
                    match stmt {
                        Stmt::Break(label, loc) => {
                            // 带标签的 break 跳出对应的循环，不带标签的 break 跳出 switch
                            if label.is_some() {
                                // 带标签的 break，使用通用处理
                                self.generate_break_statement(label, loc.clone())?;
                            } else {
                                // 不带标签的 break，跳出 switch
                                self.emit_line(&format!("  br label %{}", end_label));
                            }
                            fallthrough = false;
                            // break 不算函数终止，只是跳转到 switch.end
                            all_cases_terminate = false;
                            break;
                        }
                        Stmt::Return(_) => {
                            // return 语句终止执行，不需要生成 br
                            self.generate_statement(stmt)?;
                            fallthrough = false;
                            has_return = true;
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
                // 如果 case 体有 return，跳过 br 生成
                if has_return {
                    fallthrough = false;
                } else {
                    all_cases_terminate = false;
                }
            }

            // 如果不是 return，穿透到下一个 case
            if fallthrough && i < case_labels.len() - 1 {
                let (_, next_label, _) = &case_labels[i + 1];
                self.emit_line(&format!("  br label %{}", next_label));
                fallthrough = false;
                all_cases_terminate = false;
            } else if fallthrough {
                // 最后一个 case 没有 break，穿透到 default 或结束
                if switch_stmt.default.is_some() {
                    self.emit_line(&format!("  br label %{}", default_label));
                } else {
                    self.emit_line(&format!("  br label %{}", end_label));
                }
                fallthrough = false;
                all_cases_terminate = false;
            }
        }

        // 生成 default 块
        if let Some(default_body) = switch_stmt.default.as_ref() {
            self.emit_line(&format!("{}:", default_label));
            let mut has_return = false;
            for stmt in default_body {
                match stmt {
                    Stmt::Break(label, loc) => {
                        // 带标签的 break 跳出对应的循环，不带标签的 break 跳出 switch
                        if label.is_some() {
                            self.generate_break_statement(label, loc.clone())?;
                        } else {
                            self.emit_line(&format!("  br label %{}", end_label));
                        }
                        // break 不算函数终止
                        all_cases_terminate = false;
                        break;
                    }
                    Stmt::Return(_) => {
                        // return 语句终止执行，不需要生成 br
                        self.generate_statement(stmt)?;
                        has_return = true;
                        break;
                    }
                    _ => {
                        self.generate_statement(stmt)?;
                    }
                }
            }
            // 如果 default 体没有 return，跳转到结束
            if !has_return {
                self.emit_line(&format!("  br label %{}", end_label));
                all_cases_terminate = false;
            }
        } else {
            // 没有 default，不是所有分支都终止
            all_cases_terminate = false;
        }

        // 结束块 - 只有当并非所有分支都 return 时才生成
        if !all_cases_terminate {
            self.emit_line(&format!("{}:", end_label));
        }

        // 弹出 switch 上下文
        self.exit_loop();

        Ok(())
    }
}
