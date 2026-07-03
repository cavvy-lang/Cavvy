//! Lambda 表达式类型推断

use super::super::analyzer::SemanticAnalyzer;
use crate::ast::*;
use crate::types::Type;
use super::super::symbol_table::SemanticSymbolInfo;

impl SemanticAnalyzer {
    /// 推断 Lambda 表达式类型
    pub(crate) fn infer_lambda_type(&mut self, lambda: &LambdaExpr) -> crate::miette_diagnostic::cayResult<Type> {
        // Lambda 表达式: (params) -> { body }
        // 创建新的作用域
        self.symbol_table.enter_scope();

        // 添加 Lambda 参数到符号表
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
                // 分析块中的语句，查找 return 语句推断返回类型
                let mut inferred_return: Option<Type> = None;
                for stmt in &block.statements {
                    if let Stmt::Return(ret_expr_opt) = stmt {
                        if let Some(ret_expr) = ret_expr_opt {
                            let ret_type = self.infer_expr_type_internal(ret_expr)?;
                            inferred_return = Some(ret_type);
                        } else {
                            inferred_return = Some(Type::Void);
                        }
                        break; // 使用第一个 return 语句的类型
                    }
                }
                Box::new(inferred_return.unwrap_or(Type::Void))
            }
        };

        self.symbol_table.exit_scope();

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
