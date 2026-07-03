//! 0.5.0.0: 内存分配表达式代码生成
//!
//! 处理 __cay_alloc 和 __cay_free 表达式的 LLVM IR 生成

use crate::ast::{AllocExpr, DeallocExpr};
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::cayResult;

impl IRGenerator {
    /// 生成内存分配表达式的 LLVM IR
    ///
    /// Cavvy 代码:
    /// ```cay
    /// long ptr = __cay_alloc(64);
    /// ```
    ///
    /// 生成的 LLVM IR:
    /// ```llvm
    ///   %size = add i64 0, 64
    ///   %ptr = call i8* @malloc(i64 %size)
    ///   %ptr.i64 = ptrtoint i8* %ptr to i64
    /// ```
    ///
    /// # Arguments
    /// * `alloc` - 分配表达式 AST 节点
    ///
    /// # Returns
    /// 格式为 "i64 value" 的 LLVM IR 值字符串（指针作为 long 返回）
    pub fn generate_alloc_expression(&mut self, alloc: &AllocExpr) -> cayResult<String> {
        // 生成大小表达式
        let size_val = self.generate_expression(&alloc.size)?;

        // 提取大小值（去掉类型前缀）
        let size = if size_val.contains(' ') {
            size_val.split_whitespace().nth(1).unwrap_or("0")
        } else {
            &size_val
        };

        // 调用 malloc 分配内存
        let malloc_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i8* @malloc(i64 {})",
            malloc_temp, size
        ));

        // 将指针转换为 i64（Cavvy 的 long 类型）
        let ptr_int = self.new_temp();
        self.emit_line(&format!(
            "  {} = ptrtoint i8* {} to i64",
            ptr_int, malloc_temp
        ));

        Ok(format!("i64 {}", ptr_int.replace('%', "")))
    }

    /// 生成内存释放表达式的 LLVM IR
    ///
    /// Cavvy 代码:
    /// ```cay
    /// __cay_free(ptr);
    /// ```
    ///
    /// 生成的 LLVM IR:
    /// ```llvm
    ///   %ptr.i8 = inttoptr i64 %ptr to i8*
    ///   call void @free(i8* %ptr.i8)
    /// ```
    ///
    /// # Arguments
    /// * `dealloc` - 释放表达式 AST 节点
    ///
    /// # Returns
    /// "void" 字符串（释放操作无返回值）
    pub fn generate_dealloc_expression(&mut self, dealloc: &DeallocExpr) -> cayResult<String> {
        // 生成指针表达式
        let ptr_val = self.generate_expression(&dealloc.ptr)?;

        // 提取指针值（去掉类型前缀）
        let ptr = if ptr_val.contains(' ') {
            ptr_val.split_whitespace().nth(1).unwrap_or("0")
        } else {
            &ptr_val
        };

        // 将 i64 转换为 i8* 指针
        let ptr_i8 = self.new_temp();
        self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", ptr_i8, ptr));

        // 调用 free 释放内存
        self.emit_line(&format!("  call void @free(i8* {})", ptr_i8));

        Ok("void".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, LiteralExpr, LiteralValue};
    use crate::miette_diagnostic::SourceLocation;

    /// 测试分配表达式结构
    #[test]
    fn test_alloc_expr_structure() {
        let alloc_expr = AllocExpr {
            size: Box::new(Expr::Literal(LiteralExpr {
                value: LiteralValue::Int64(64),
                loc: SourceLocation {
                    file: None,
                    line: 1,
                    column: 1,
                },
            })),
            align: None,
            loc: SourceLocation {
                file: None,
                line: 1,
                column: 1,
            },
        };

        // 验证结构
        assert!(matches!(alloc_expr.size.as_ref(), Expr::Literal(_)));
        assert!(alloc_expr.align.is_none());
    }

    /// 测试释放表达式结构
    #[test]
    fn test_dealloc_expr_structure() {
        let dealloc_expr = DeallocExpr {
            ptr: Box::new(Expr::Literal(LiteralExpr {
                value: LiteralValue::Int64(0x1234),
                loc: SourceLocation {
                    file: None,
                    line: 1,
                    column: 1,
                },
            })),
            loc: SourceLocation {
                file: None,
                line: 1,
                column: 1,
            },
        };

        // 验证结构
        assert!(matches!(dealloc_expr.ptr.as_ref(), Expr::Literal(_)));
    }
}
