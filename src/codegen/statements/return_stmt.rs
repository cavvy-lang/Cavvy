//! Return语句代码生成
//!
//! 处理return语句的代码生成。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;

/// 收集返回表达式中应视为“所有权已转移”的局部标识符。
///
/// 策略：递归查找所有函数/方法调用参数、构造函数参数以及三元表达式分支中
/// 出现的标识符。方法调用的接收者（`obj.method(...)` 中的 `obj`）不视为转移，
/// 因此不会被收集。
fn collect_moved_identifiers(expr: &Expr) -> Vec<String> {
    let mut ids = Vec::new();
    match expr {
        Expr::Identifier(id) => ids.push(id.name.clone()),
        Expr::Call(call) => {
            for arg in &call.args {
                ids.extend(collect_moved_identifiers(arg));
            }
            ids.extend(collect_moved_identifiers(&call.callee));
        }
        Expr::New(new_expr) => {
            for arg in &new_expr.args {
                ids.extend(collect_moved_identifiers(arg));
            }
        }
        Expr::Ternary(ternary) => {
            ids.extend(collect_moved_identifiers(&ternary.true_branch));
            ids.extend(collect_moved_identifiers(&ternary.false_branch));
        }
        // 方法调用的接收者、二元/一元表达式等不视为所有权转移。
        _ => {}
    }
    ids
}

impl IRGenerator {
    /// 生成return语句代码
    pub fn generate_return_statement(&mut self, expr: &Option<Expr>) -> CayResult<()> {
        // 先计算返回值（如果有），并确定最终 ret 指令。
        // 注意：不能先调用析构函数，否则 `return local_obj;` 会在 ret 之前
        // 把要返回的对象析构掉。
        let ret_instr = if let Some(e) = expr.as_ref() {
            let value = self.generate_expression(e)?;
            let (value_type, val) = self.parse_typed_value(&value);
            let ret_type = self.current_return_type.clone();

            // 如果返回类型是 void，但表达式非空，这是错误（但由语义分析处理）
            if ret_type == "void" {
                "  ret void".to_string()
            } else if value_type != ret_type {
                // 需要类型转换
                let temp = self.new_temp();

                // 特殊处理：null 值（i64 0）转换为指针类型
                if value == "i64 0" && ret_type.ends_with("*") {
                    format!("  ret {} null", ret_type)
                }
                // 浮点类型转换
                else if value_type == "double" && ret_type == "float" {
                    // double -> float 转换
                    self.emit_line(&format!(
                        "  {} = fptrunc double {} to float",
                        temp, val
                    ));
                    let _align = self.get_type_align("float");
                    format!("  ret float {}", temp)
                } else if value_type == "float" && ret_type == "double" {
                    // float -> double 转换
                    self.emit_line(&format!(
                        "  {} = fpext float {} to double",
                        temp, val
                    ));
                    let _align = self.get_type_align("double");
                    format!("  ret double {}", temp)
                }
                // 指针到整数转换 (ptrtoint)
                else if value_type.ends_with("*")
                    && ret_type.starts_with("i")
                    && !ret_type.ends_with("*")
                {
                    self.emit_line(&format!(
                        "  {} = ptrtoint {} {} to {}",
                        temp, value_type, val, ret_type
                    ));
                    format!("  ret {} {}", ret_type, temp)
                }
                // 整数到指针转换 (inttoptr)
                else if value_type.starts_with("i")
                    && !value_type.ends_with("*")
                    && ret_type.ends_with("*")
                {
                    self.emit_line(&format!(
                        "  {} = inttoptr {} {} to {}",
                        temp, value_type, val, ret_type
                    ));
                    format!("  ret {} {}", ret_type, temp)
                }
                // 整数类型转换
                else if value_type.starts_with("i")
                    && ret_type.starts_with("i")
                    && !value_type.ends_with("*")
                    && !ret_type.ends_with("*")
                {
                    let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                    let to_bits: u32 = ret_type.trim_start_matches('i').parse().unwrap_or(64);

                    if to_bits > from_bits {
                        // 符号扩展
                        self.emit_line(&format!(
                            "  {} = sext {} {} to {}",
                            temp, value_type, val, ret_type
                        ));
                    } else {
                        // 截断
                        self.emit_line(&format!(
                            "  {} = trunc {} {} to {}",
                            temp, value_type, val, ret_type
                        ));
                    }
                    format!("  ret {} {}", ret_type, temp)
                }
                // 整数到浮点数转换
                else if value_type.starts_with("i")
                    && !value_type.ends_with("*")
                    && (ret_type == "float" || ret_type == "double")
                {
                    self.emit_line(&format!(
                        "  {} = sitofp {} {} to {}",
                        temp, value_type, val, ret_type
                    ));
                    format!("  ret {} {}", ret_type, temp)
                }
                // 浮点数到整数转换
                else if (value_type == "float" || value_type == "double")
                    && ret_type.starts_with("i")
                {
                    self.emit_line(&format!(
                        "  {} = fptosi {} {} to {}",
                        temp, value_type, val, ret_type
                    ));
                    format!("  ret {} {}", ret_type, temp)
                } else {
                    // 类型不兼容，直接返回（可能会出错）
                    format!("  ret {}", value)
                }
            } else {
                // 类型匹配，直接返回
                format!("  ret {}", value)
            }
        } else {
            "  ret void".to_string()
        };

        // ROADMAP 5.3.x 自动 RAII：返回表达式中作为调用参数、构造参数或三元
        // 分支出现的局部类对象变量，视为所有权已转移给调用者，从析构候选中
        // 移除，避免在 ret 前析构它。
        if let Some(e) = expr.as_ref() {
            for name in collect_moved_identifiers(e) {
                self.scope_manager
                    .remove_dtor_candidate_by_var_name(&name);
            }
        }

        // ROADMAP 5.3.x 自动 RAII：return 前调用所有尚未退出的作用域的析构函数。
        self.emit_all_scope_dtors();

        self.emit_line(&ret_instr);
        Ok(())
    }
}
