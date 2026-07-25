//! if 表达式代码生成 (6.2.x)
//!
//! 处理 `if (cond) { stmts; tail } else { stmts; tail }` 表达式：
//! 两个分支在新作用域中生成，分支值为其 tail_expr，merge 处用 phi 合并。
//!
//! 块结构（每个可到达 merge 的分支）：
//!   if_expr.then.N:   分支体（tail 求值可能引入嵌套控制流，跨多个块）
//!     br label %if_expr.then.out.N
//!   if_expr.then.out.N:
//!     br label %if_expr.then.conv.N
//!   if_expr.then.conv.N:  数值类型转换（若有），在 phi 前完成
//!     br label %if_expr.end.N
//!   if_expr.end.N:
//!     %r = phi ty [ tval, %then.conv ], [ eval, %else.conv ]
//! out/conv 两级跳转保证：phi 的 incoming 块是已知的单一前驱（嵌套 if/三元
//! 的 tail 值定义在内部 merge 块，不能直接引用分支入口标签），且转换指令
//! 不在 end 块内（phi 必须是块内第一条指令）。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;

/// 一个分支的生成结果：tail 的 (类型, 值) 与 phi incoming 用的 conv 块标签
struct IfExprBranchResult {
    ty: String,
    val: String,
    out_label: String,
    conv_label: String,
}

impl IRGenerator {
    /// 生成 if 表达式代码
    ///
    /// # Arguments
    /// * `if_expr` - if 表达式 AST 节点
    ///
    /// # Returns
    /// 格式为 "type value" 的 LLVM IR 值字符串
    pub fn generate_if_expression(&mut self, if_expr: &IfExpr) -> CayResult<String> {
        let then_label = self.new_label("if_expr.then");
        let else_label = self.new_label("if_expr.else");
        let end_label = self.new_label("if_expr.end");

        // 生成条件表达式并转换为 i1
        let cond_result = self.generate_expression(&if_expr.condition)?;
        let (cond_type, cond_val) = self.parse_typed_value(&cond_result);
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
            cond_reg, then_label, else_label
        ));

        // then 分支
        self.emit_line(&format!("\n{}:", then_label));
        let then_branch = self.generate_if_expr_branch(&if_expr.then_branch, "if_expr.then")?;

        // else 分支
        self.emit_line(&format!("\n{}:", else_label));
        let else_branch = self.generate_if_expr_branch(&if_expr.else_branch, "if_expr.else")?;

        // 统一 phi 类型：相同直接使用；不同但皆为数值时向较宽类型转换
        let phi_ty = match (&then_branch, &else_branch) {
            (Some(t), Some(e)) => Self::unify_numeric_type(&t.ty, &e.ty),
            (Some(t), None) => t.ty.clone(),
            (None, Some(e)) => e.ty.clone(),
            (None, None) => "i32".to_string(),
        };

        // conv 块：执行必要的类型转换后跳入 end
        let mut emit_conv = |g: &mut Self, branch: &IfExprBranchResult| -> String {
            g.emit_line(&format!("\n{}:", branch.conv_label));
            let val = g.emit_numeric_conversion(&branch.ty, &branch.val, &phi_ty);
            g.emit_line(&format!("  br label %{}", end_label));
            val
        };
        let then_incoming = then_branch
            .as_ref()
            .map(|b| (emit_conv(self, b), b.conv_label.clone()));
        let else_incoming = else_branch
            .as_ref()
            .map(|b| (emit_conv(self, b), b.conv_label.clone()));

        // 合并点
        self.emit_line(&format!("\n{}:", end_label));
        match (then_incoming, else_incoming) {
            (Some((tval, tlab)), Some((eval, elab))) => {
                let result_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = phi {} [ {}, %{} ], [ {}, %{} ]",
                    result_temp, phi_ty, tval, tlab, eval, elab
                ));
                Ok(format!("{} {}", phi_ty, result_temp))
            }
            (Some((val, lab)), None) | (None, Some((val, lab))) => {
                let result_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = phi {} [ {}, %{} ]",
                    result_temp, phi_ty, val, lab
                ));
                Ok(format!("{} {}", phi_ty, result_temp))
            }
            // 两分支都以 return/break 结束：merge 不可达
            (None, None) => {
                self.emit_line("  unreachable");
                Ok("i32 0".to_string())
            }
        }
    }

    /// 生成 if 表达式的一个分支：新作用域 → 语句 → tail 求值 → RAII 析构，
    /// 最后经 out 块跳到 conv 块（conv 块由调用方统一生成）。
    ///
    /// 返回 None 表示分支语句已以 return/break 等终止（不参与 phi）。
    fn generate_if_expr_branch(
        &mut self,
        block: &Block,
        prefix: &str,
    ) -> CayResult<Option<IfExprBranchResult>> {
        self.enter_debug_lexical_block(block.loc.line, block.loc.column);
        self.scope_manager.enter_scope();

        let mut terminated = false;
        for stmt in &block.statements {
            self.generate_statement(stmt)?;
            if self.current_block_terminated() {
                terminated = true;
                break;
            }
        }

        if terminated {
            self.emit_scope_exit_dtors();
            self.scope_manager.exit_scope();
            self.exit_debug_lexical_block();
            return Ok(None);
        }

        let tail = block.tail_expr.as_ref().ok_or_else(|| {
            crate::miette_diagnostic::codegen_error_at(
                crate::miette_diagnostic::ErrorCodes::CODEGEN_INVALID_OPERATION,
                block.loc.clone(),
                "if expression branch missing tail expression".to_string(),
            )
        })?;
        let tail_result = self.generate_expression(tail)?;
        let (ty, val) = self.parse_typed_value(&tail_result);

        // RAII：作用域退出前析构本层局部变量（tail 已先求值）
        self.emit_scope_exit_dtors();
        self.scope_manager.exit_scope();
        self.exit_debug_lexical_block();

        // out → conv 两级跳转：无论 tail 求值引入了多少嵌套块，
        // phi 的 incoming 块都是唯一的 conv 块
        let out_label = self.new_label(&format!("{}.out", prefix));
        let conv_label = self.new_label(&format!("{}.conv", prefix));
        self.emit_line(&format!("  br label %{}", out_label));
        self.emit_line(&format!("\n{}:", out_label));
        self.emit_line(&format!("  br label %{}", conv_label));

        Ok(Some(IfExprBranchResult {
            ty,
            val,
            out_label,
            conv_label,
        }))
    }

    /// 数值类型统一：相同则返回原类型；不同则取较宽类型。
    /// 非数值组合返回左操作数类型（与三元运算符一致，交由语义阶段把关）。
    fn unify_numeric_type(a: &str, b: &str) -> String {
        if a == b {
            return a.to_string();
        }
        let rank = |ty: &str| -> i32 {
            match ty {
                "i1" | "i8" => 0,
                "i32" => 1,
                "i64" => 2,
                "float" => 3,
                "double" => 4,
                _ => -1,
            }
        };
        let (ra, rb) = (rank(a), rank(b));
        if ra < 0 || rb < 0 || ra == rb {
            return a.to_string();
        }
        if ra > rb { a.to_string() } else { b.to_string() }
    }

    /// 在 conv 块内把数值从 from_ty 转换到 target_ty（相同则不产生指令）。
    fn emit_numeric_conversion(&mut self, from_ty: &str, val: &str, target_ty: &str) -> String {
        if from_ty == target_ty {
            return val.to_string();
        }
        let instr = match (from_ty, target_ty) {
            ("i1", "i32") | ("i1", "i64") | ("i8", "i32") | ("i8", "i64") | ("i32", "i64") => {
                "sext"
            }
            ("float", "double") => "fpext",
            ("i1", "float") | ("i1", "double") | ("i8", "float") | ("i8", "double")
            | ("i32", "float") | ("i32", "double") | ("i64", "float") | ("i64", "double") => {
                "sitofp"
            }
            _ => return val.to_string(), // 非数值组合不转换
        };
        let tmp = self.new_temp();
        self.emit_line(&format!(
            "  {} = {} {} {} to {}",
            tmp, instr, from_ty, val, target_ty
        ));
        tmp
    }
}
