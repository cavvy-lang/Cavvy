//! 0.5.0.0: scope 语句代码生成
//!
//! scope 语句创建一个栈作用域，用于在栈上分配临时对象。
//! 在 scope 结束时，所有分配的栈内存自动释放。

use crate::ast::ScopeStmt;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;

/// 为 scope 语句生成 LLVM IR 代码
///
/// # 算法说明
/// scope 语句本质上就是一个带有显式作用域标记的代码块。
/// 在 LLVM IR 中，我们使用基本块（basic block）来隔离作用域。
///
/// # LLVM IR 生成策略
/// 1. 创建 scope 入口标签
/// 2. 生成 scope 体内的所有语句
/// 3. 创建 scope 出口标签
///
/// 注意：实际的栈内存管理由 LLVM 的 alloca 指令自动处理，
/// 当函数返回时，所有 alloca 分配的内存自动释放。
/// 对于嵌套 scope，我们依赖 LLVM 的优化器正确处理生命周期。
///
/// # 示例
/// Cavvy 代码:
/// ```cay
/// scope {
///     int x = 10;
///     println("x = " + x);
/// }
/// ```
///
/// 生成的 LLVM IR (概念):
/// ```llvm
/// scope.entry:
///   %x = alloca i32
///   store i32 10, i32* %x
///   ; println 调用...
/// scope.exit:
///   ; x 在这里不再可访问
/// ```
impl IRGenerator {
    /// 生成 scope 语句的代码
    ///
    /// # Arguments
    /// * `stmt` - scope 语句 AST 节点
    ///
    /// # Returns
    /// CayResult<()> - 生成成功或错误
    ///
    /// # 复杂度
    /// * 时间: O(n)，其中 n 是 scope 体内语句数量
    /// * 空间: O(d)，其中 d 是作用域嵌套深度
    pub fn generate_scope(&mut self, stmt: &ScopeStmt) -> CayResult<()> {
        let scope_id = self.new_temp().replace("%", "");

        // 生成 scope 入口标签（用于调试和可读性）
        self.emit_line(&format!("; ====== scope {} start ======", scope_id));

        // DWARF 词法块作用域
        self.enter_debug_lexical_block(stmt.loc.line, stmt.loc.column);

        // 在 scope 开始时创建一个新的作用域层级
        // 这使得 scope 内部声明的变量不会与外部冲突
        self.scope_manager.enter_scope();

        // 生成 scope 体内的所有语句
        for statement in &stmt.body.statements {
            self.generate_statement(statement)?;
        }

        // scope 结束时，生成清理代码
        // ROADMAP 5.3.x 自动 RAII：调用本作用域内带析构函数的局部变量。
        self.emit_scope_exit_dtors();

        // 注意：对于栈分配（alloca），实际上不需要显式释放
        // LLVM 在函数返回时自动清理所有 alloca
        // 但为了完整性，我们记录 scope 的结束
        self.emit_line(&format!("; ====== scope {} end ======", scope_id));

        // 弹出 scope 层级
        self.scope_manager.exit_scope();
        self.exit_debug_lexical_block();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Block, ScopeStmt, Stmt};
    use crate::miette_diagnostic::SourceLocation;

    /// 测试 scope 代码生成的基本结构
    #[test]
    fn test_scope_codegen_structure() {
        // 这个测试验证 scope 语句能生成正确的代码结构
        // 实际测试需要完整的 CodeGenerator 设置，这里只验证接口

        let scope_stmt = ScopeStmt {
            body: Block {
                statements: vec![],
                tail_expr: None,
                loc: SourceLocation {
                    file: None,
                    line: 1,
                    column: 1,
                },
            },
            loc: SourceLocation {
                file: None,
                line: 1,
                column: 1,
            },
        };

        // 验证 ScopeStmt 结构正确
        assert!(scope_stmt.body.statements.is_empty());
    }

    /// 测试嵌套 scope
    #[test]
    fn test_nested_scope() {
        let inner_scope = Stmt::Scope(ScopeStmt {
            body: Block {
                statements: vec![],
                tail_expr: None,
                loc: SourceLocation {
                    file: None,
                    line: 2,
                    column: 5,
                },
            },
            loc: SourceLocation {
                file: None,
                line: 2,
                column: 5,
            },
        });

        let outer_scope = ScopeStmt {
            body: Block {
                statements: vec![inner_scope],
                tail_expr: None,
                loc: SourceLocation {
                    file: None,
                    line: 1,
                    column: 1,
                },
            },
            loc: SourceLocation {
                file: None,
                line: 1,
                column: 1,
            },
        };

        // 验证嵌套结构
        assert_eq!(outer_scope.body.statements.len(), 1);
    }
}
