//! Lambda 表达式类型推断

use super::super::analyzer::SemanticAnalyzer;
use super::super::symbol_table::SemanticSymbolInfo;
use super::helpers::semantic_error_at_loc;
use crate::ast::*;
use crate::types::Type;

impl SemanticAnalyzer {
    /// 推断 Lambda 表达式类型
    pub(crate) fn infer_lambda_type(
        &mut self,
        lambda: &LambdaExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // Lambda 表达式: (params) -> { body }
        // 注意：无类型标注的参数按语言约定默认为 Int32（与 codegen 的默认一致，
        // parser 的帮助文本也明确支持无类型参数写法 `(x, y) -> ...`）。
        // 在此硬报错会破坏 6.2 既有语法（如 `(a: i32, b, int c) -> ...`），
        // 真正的上下文类型推断需要贯通期望类型，超出本次修复范围。

        // 创建新的作用域
        self.symbol_table.enter_scope();
        // 使用闭包包裹主体逻辑，确保任何错误路径都能退出作用域
        let result = self.infer_lambda_type_in_scope(lambda);
        self.symbol_table.exit_scope();
        result
    }

    /// 在已进入新作用域的前提下推断 Lambda 类型
    fn infer_lambda_type_in_scope(
        &mut self,
        lambda: &LambdaExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 添加 Lambda 参数到符号表（无标注参数默认为 Int32，与 codegen 一致）
        let mut param_types = Vec::new();
        for param in &lambda.params {
            let param_type = param.param_type.clone().unwrap_or(Type::Int32);
            param_types.push(param_type.clone());
            self.symbol_table.declare(
                param.name.clone(),
                SemanticSymbolInfo {
                    name: param.name.clone(),
                    symbol_type: param_type,
                    is_final: false,
                    is_initialized: true,
                },
            );
        }

        // 推断 Lambda 体类型
        let return_type = match &lambda.body {
            LambdaBody::Expr(expr) => {
                let expr_type = self.infer_expr_type_internal(expr)?;
                Box::new(expr_type)
            }
            LambdaBody::Block(block) => {
                // 对块中的语句进行完整类型检查（变量声明、return等）
                for stmt in &block.statements {
                    self.type_check_statement(stmt, None)?;
                }
                // 分析块中的所有 return 语句，检查返回类型一致性
                let mut inferred_return: Option<Type> = None;
                for stmt in &block.statements {
                    if let Stmt::Return(ret_expr_opt) = stmt {
                        let ret_type = match ret_expr_opt {
                            Some(ret_expr) => self.infer_expr_type_internal(ret_expr)?,
                            None => Type::Void,
                        };
                        match &inferred_return {
                            None => inferred_return = Some(ret_type),
                            Some(first) => {
                                if *first == ret_type {
                                    // 类型一致，继续
                                } else if Self::is_numeric_type_helper(first)
                                    && Self::is_numeric_type_helper(&ret_type)
                                {
                                    // 数值类型进行类型提升
                                    inferred_return =
                                        Some(self.promote_types(first, &ret_type));
                                } else {
                                    return Err(semantic_error_at_loc(
                                        &lambda.loc,
                                        format!(
                                            "Lambda has inconsistent return types: {} and {}",
                                            first, ret_type
                                        ),
                                    ));
                                }
                            }
                        }
                    }
                }
                Box::new(inferred_return.unwrap_or(Type::Void))
            }
        };

        // 返回完整的函数类型
        // 注意：Cavvy 的 lambda 总是使用闭包格式（打包结构体），所以 is_closure 总是 true
        Ok(Type::Function(Box::new(crate::types::FunctionType {
            params: param_types,
            return_type,
            is_static: true,
            is_closure: true, // Lambda 总是使用闭包格式
        })))
    }
}
