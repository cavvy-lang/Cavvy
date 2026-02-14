//! 三元运算符表达式代码生成
//!
//! 处理条件表达式 ? :

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成三元运算符表达式代码
    ///
    /// # Arguments
    /// * `ternary` - 三元表达式
    pub fn generate_ternary_expression(&mut self, ternary: &TernaryExpr) -> cayResult<String> {
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
            self.emit_line(&format!("  {} = icmp ne {} {}, 0", cond_reg, cond_type, cond_val));
        }

        // 条件分支
        self.emit_line(&format!("  br i1 {}, label %{}, label %{}", cond_reg, then_label, else_label));

        // then 分支
        self.emit_line(&format!("\n{}:", then_label));
        let then_result = self.generate_expression(&ternary.true_branch)?;
        let (then_type, then_val) = self.parse_typed_value(&then_result);
        let then_temp = self.new_temp();
        self.emit_line(&format!("  {} = add {} {}, 0", then_temp, then_type, then_val));
        self.emit_line(&format!("  br label %{}", end_label));

        // else 分支
        self.emit_line(&format!("\n{}:", else_label));
        let else_result = self.generate_expression(&ternary.false_branch)?;
        let (else_type, else_val) = self.parse_typed_value(&else_result);
        let else_temp = self.new_temp();
        self.emit_line(&format!("  {} = add {} {}, 0", else_temp, else_type, else_val));
        self.emit_line(&format!("  br label %{}", end_label));

        // 合并点
        self.emit_line(&format!("\n{}:", end_label));
        let result_temp = self.new_temp();
        self.emit_line(&format!("  {} = phi {} [ {}, %{} ], [ {}, %{} ]",
            result_temp, then_type, then_temp, then_label, else_temp, else_label));

        Ok(format!("{} {}", then_type, result_temp))
    }
}
