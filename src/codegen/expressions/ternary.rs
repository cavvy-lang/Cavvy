//! 三元运算符表达式代码生成
//!
//! 处理条件表达式 ? :
//!
//! 两个分支各自求值后经 out → conv 两级跳转汇入 merge 块：
//! 分支值的 LLVM 类型可能不同（如 `cond ? 2147483647 : -2147483648` 的
//! i32 与 i64 字面量），必须先在 conv 块内做数值类型统一，再参与 phi，
//! 否则 phi 的操作数类型与声明类型不一致，llc 会报
//! "'%t' defined with type 'i32' but expected 'i64'" 或产生错误值。
//! 该模式与 if 表达式（if_expr.rs）保持一致。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;

impl IRGenerator {
    /// 生成三元运算符表达式代码
    ///
    /// # Arguments
    /// * `ternary` - 三元表达式
    pub fn generate_ternary_expression(&mut self, ternary: &TernaryExpr) -> CayResult<String> {
        // 创建标签
        let then_label = self.new_label("ternary.then");
        let else_label = self.new_label("ternary.else");
        let end_label = self.new_label("ternary.end");

        // 生成条件表达式
        let cond_result = self.generate_expression(&ternary.condition)?;
        let (cond_type, cond_val) = self.parse_typed_value(&cond_result);
        let cond_reg = self.new_temp();

        // 将条件转换为 i1 类型
        if cond_type == "i1" {
            self.emit_line(&format!("  {} = icmp ne i1 {}, 0", cond_reg, cond_val));
        } else {
            // 对于整数类型，先与 0 比较
            self.emit_line(&format!(
                "  {} = icmp ne {} {}, 0",
                cond_reg, cond_type, cond_val
            ));
        }

        // 条件分支
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            cond_reg, then_label, else_label
        ));

        // then 分支
        self.emit_line(&format!("\n{}:", then_label));
        let then_result = self.generate_expression(&ternary.true_branch)?;
        let (then_type, then_val) = self.parse_typed_value(&then_result);
        let then_out = self.new_label("ternary.then.out");
        let then_conv = self.new_label("ternary.then.conv");
        self.emit_line(&format!("  br label %{}", then_out));
        self.emit_line(&format!("\n{}:", then_out));
        self.emit_line(&format!("  br label %{}", then_conv));

        // else 分支
        self.emit_line(&format!("\n{}:", else_label));
        let else_result = self.generate_expression(&ternary.false_branch)?;
        let (else_type, else_val) = self.parse_typed_value(&else_result);
        let else_out = self.new_label("ternary.else.out");
        let else_conv = self.new_label("ternary.else.conv");
        self.emit_line(&format!("  br label %{}", else_out));
        self.emit_line(&format!("\n{}:", else_out));
        self.emit_line(&format!("  br label %{}", else_conv));

        // 统一 phi 类型：相同直接使用；不同但皆为数值时向较宽类型转换
        let phi_ty = Self::unify_numeric_type(&then_type, &else_type);

        // conv 块：执行必要的类型转换后跳入 end
        self.emit_line(&format!("\n{}:", then_conv));
        let then_incoming = self.emit_numeric_conversion(&then_type, &then_val, &phi_ty);
        self.emit_line(&format!("  br label %{}", end_label));

        self.emit_line(&format!("\n{}:", else_conv));
        let else_incoming = self.emit_numeric_conversion(&else_type, &else_val, &phi_ty);
        self.emit_line(&format!("  br label %{}", end_label));

        // 合并点
        self.emit_line(&format!("\n{}:", end_label));
        let result_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = phi {} [ {}, %{} ], [ {}, %{} ]",
            result_temp, phi_ty, then_incoming, then_conv, else_incoming, else_conv
        ));

        Ok(format!("{} {}", phi_ty, result_temp))
    }
}
