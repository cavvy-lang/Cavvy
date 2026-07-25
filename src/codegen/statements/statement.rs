//! 语句分发代码生成
//!
//! 处理语句类型的分发。

use crate::ast::*;
use crate::codegen::bridge::{InlineIrBridge, InlineIrResult};
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;

impl IRGenerator {
    /// 生成单个语句代码
    /// 复杂度: O(1) 每语句，设置源位置后分发到具体生成器
    pub fn generate_statement(&mut self, stmt: &Stmt) -> CayResult<()> {
        // 设置源位置（如果语句有位置信息）
        let source_file = self.source_file.clone();
        match stmt {
            Stmt::Expr(expr) => {
                self.set_source_from_loc(&expr.location(), &source_file);
                self.generate_expression(expr)?;
            }
            Stmt::VarDecl(var) => {
                self.set_source_from_loc(&var.loc, &source_file);
                self.generate_var_decl(var)?;
            }
            Stmt::Return(expr) => {
                // Return语句没有独立loc，使用表达式位置或默认位置
                if let Some(ref e) = *expr {
                    self.set_source_from_loc(&e.location(), &source_file);
                }
                self.generate_return_statement(expr)?;
            }
            Stmt::Block(block) => {
                self.set_source_from_loc(&block.loc, &source_file);
                // 检查是否是多变量声明生成的块（只包含 VarDecl 且无尾表达式）
                let is_multi_var_decl = block.tail_expr.is_none()
                    && !block.statements.is_empty()
                    && block
                        .statements
                        .iter()
                        .all(|s| matches!(s, Stmt::VarDecl(_)));
                if is_multi_var_decl {
                    // 多变量声明不创建新作用域，在当前作用域内声明所有变量
                    for stmt in &block.statements {
                        if let Stmt::VarDecl(var) = stmt {
                            self.generate_var_decl(var)?;
                        }
                    }
                } else {
                    self.generate_block(block)?;
                }
            }
            Stmt::If(if_stmt) => {
                self.set_source_from_loc(&if_stmt.loc, &source_file);
                self.generate_if_statement(if_stmt)?;
            }
            Stmt::While(while_stmt) => {
                self.set_source_from_loc(&while_stmt.loc, &source_file);
                self.generate_while_statement(while_stmt)?;
            }
            Stmt::For(for_stmt) => {
                self.set_source_from_loc(&for_stmt.loc, &source_file);
                self.generate_for_statement(for_stmt)?;
            }
            Stmt::ForEach(for_each_stmt) => {
                self.set_source_from_loc(&for_each_stmt.loc, &source_file);
                self.generate_for_each_statement(for_each_stmt)?;
            }
            Stmt::DoWhile(do_while_stmt) => {
                self.set_source_from_loc(&do_while_stmt.loc, &source_file);
                self.generate_do_while_statement(do_while_stmt)?;
            }
            Stmt::Switch(switch_stmt) => {
                self.set_source_from_loc(&switch_stmt.loc, &source_file);
                self.generate_switch_statement(switch_stmt)?;
            }
            Stmt::Scope(scope_stmt) => {
                self.set_source_from_loc(&scope_stmt.loc, &source_file);
                self.generate_scope(scope_stmt)?;
            }
            Stmt::Break(label, loc) => {
                self.generate_break_statement(label, loc.clone())?;
            }
            Stmt::Continue(label, loc) => {
                self.generate_continue_statement(label, loc.clone())?;
            }
            Stmt::InlineIr(inline_ir) => {
                self.set_source_from_loc(&inline_ir.loc, &source_file);
                // 0.5.0.0: 使用CodeGen-IR Builder协作桥处理内联IR
                self.generate_inline_ir(inline_ir)?;
            }
        }
        Ok(())
    }

    /// 生成内联IR语句
    ///
    /// 通过协作桥调用IR Builder处理内联IR，然后将结果合并到CodeGen输出。
    ///
    /// # 流程
    /// 1. 创建协作桥
    /// 2. 收集当前作用域变量
    /// 3. 调用IR Builder解析和验证内联IR
    /// 4. 将生成的LLVM IR文本输出到CodeGen
    ///
    /// # 复杂度
    /// - 时间: O(n*m)，n为内联IR行数，m为作用域变量数
    /// - 空间: O(n+m)
    fn generate_inline_ir(&mut self, inline_ir: &InlineIrStmt) -> CayResult<()> {
        // 创建协作桥
        let bridge = InlineIrBridge::new();

        // 通过桥处理内联IR
        let result = bridge.process_inline_ir(self, inline_ir)?;

        // 将生成的LLVM IR文本输出到CodeGen
        // 添加注释标记内联IR块开始
        self.emit_line("  ; Inline IR block start");

        for line in &result.llvm_ir_lines {
            self.emit_line(&format!("  {}", line));
        }

        // 添加注释标记内联IR块结束
        self.emit_line("  ; Inline IR block end");

        Ok(())
    }
}
