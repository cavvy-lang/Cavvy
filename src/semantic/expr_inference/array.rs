//! 数组相关类型推断

use super::super::analyzer::SemanticAnalyzer;
use super::helpers::semantic_error_at_loc;
use crate::ast::*;
use crate::types::Type;

impl SemanticAnalyzer {
    /// 推断数组创建表达式类型
    pub(crate) fn infer_array_creation_type(
        &mut self,
        arr: &ArrayCreationExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 数组创建: new Type[size] 或 new Type[size1][size2]... 或 new Type[size][] (不规则数组)
        // 检查所有维度的大小
        for (i, size) in arr.sizes.iter().enumerate() {
            // 跳过空维度（不规则数组，如 new int[5][]）
            if let Expr::Literal(lit_expr) = size {
                if let LiteralValue::Null = lit_expr.value {
                    continue;
                }
            }

            let size_type = self.infer_expr_type_internal(size)?;
            if !size_type.is_integer() {
                return Err(semantic_error_at_loc(
                    &arr.loc,
                    format!(
                        "Array size at dimension {} must be integer, got {}",
                        i + 1,
                        size_type
                    ),
                ));
            }
            // 检查负数数组大小（仅当大小是字面量或一元负号表达式时）
            // 支持直接负数字面量如 -5（被解析为 Unary(Neg, Literal(5))）
            if let Expr::Literal(lit_expr) = size {
                if let LiteralValue::Int32(n) = lit_expr.value {
                    if n < 0 {
                        return Err(semantic_error_at_loc(
                            &arr.loc,
                            format!("Array size cannot be negative: {}", n),
                        ));
                    }
                }
                if let LiteralValue::Int64(n) = lit_expr.value {
                    if n < 0 {
                        return Err(semantic_error_at_loc(
                            &arr.loc,
                            format!("Array size cannot be negative: {}", n),
                        ));
                    }
                }
            }
            // 检查一元负号表达式如 -5
            if let Expr::Unary(unary) = size {
                if let UnaryOp::Neg = unary.op {
                    if let Expr::Literal(lit_expr) = unary.operand.as_ref() {
                        if let LiteralValue::Int32(n) = lit_expr.value {
                            return Err(semantic_error_at_loc(
                                &arr.loc,
                                format!("Array size cannot be negative: -{}", n),
                            ));
                        }
                        if let LiteralValue::Int64(n) = lit_expr.value {
                            return Err(semantic_error_at_loc(
                                &arr.loc,
                                format!("Array size cannot be negative: -{}", n),
                            ));
                        }
                    }
                }
            }
        }
        Ok(Type::Array(Box::new(arr.element_type.clone())))
    }

    /// 推断分配器-backed 数组分配表达式类型
    /// __cay_alloc_array<T>(allocator, count) 返回 T[]
    pub(crate) fn infer_alloc_array_type(
        &mut self,
        alloc_array: &AllocArrayExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 检查 allocator 表达式是否为对象/接口类型（Allocator 接口）
        // 在泛型类体内，allocator 可能是类型参数 A，此时也允许，
        // 具体类型在单态化后的 codegen 阶段解析。
        let allocator_type = self.infer_expr_type_internal(&alloc_array.allocator)?;
        match allocator_type {
            Type::Object(_) | Type::Generic(_, _) | Type::GenericParam(_) => {
                // 接受任何类/接口实例或类型参数作为分配器。
            }
            _ => {
                return Err(semantic_error_at_loc(
                    &alloc_array.loc,
                    format!(
                        "__cay_alloc_array allocator must be an object, got {}",
                        allocator_type
                    ),
                ));
            }
        }

        // 检查 count 是否为整数
        let size_type = self.infer_expr_type_internal(&alloc_array.size)?;
        if !size_type.is_integer() {
            return Err(semantic_error_at_loc(
                &alloc_array.loc,
                format!(
                    "__cay_alloc_array count must be integer, got {}",
                    size_type
                ),
            ));
        }

        Ok(Type::Array(Box::new(alloc_array.element_type.clone())))
    }

    /// 推断数组初始化表达式类型
    pub(crate) fn infer_array_init_type(
        &mut self,
        init: &ArrayInitExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 数组初始化: {1, 2, 3}
        // 需要上下文来推断类型，这里返回一个占位符类型
        // 实际类型会在变量声明时根据声明类型确定
        if init.elements.is_empty() {
            return Err(semantic_error_at_loc(
                &init.loc,
                "Cannot infer type of empty array initializer".to_string(),
            ));
        }
        // 检查所有元素的类型一致性（不能只检查第一个元素）。
        // null 元素不参与元素类型的确定，但会被检查为可赋值。
        let mut elem_type: Option<Type> = None;
        for element in &init.elements {
            let ty = self.infer_expr_type_internal(element)?;
            if ty.is_null_literal() {
                continue;
            }
            match &elem_type {
                None => elem_type = Some(ty),
                Some(t) => {
                    if self.types_compatible(&ty, t) {
                        // 元素可赋值给当前元素类型，继续
                    } else if Self::is_numeric_type_helper(t) && Self::is_numeric_type_helper(&ty)
                    {
                        // 混合数值类型进行类型提升（如 {1, 2L} -> long[]）
                        elem_type = Some(self.promote_types(t, &ty));
                    } else {
                        return Err(semantic_error_at_loc(
                            &init.loc,
                            format!(
                                "Array initializer elements must have consistent types: expected {}, got {}",
                                t, ty
                            ),
                        ));
                    }
                }
            }
        }
        // 全是 null 元素时，元素类型为 null 标记类型（可赋值给任何引用类型数组）
        let elem_type =
            elem_type.unwrap_or_else(|| Type::Object(crate::types::NULL_TYPE_NAME.to_string()));
        Ok(Type::Array(Box::new(elem_type)))
    }

    /// 推断数组访问表达式类型
    pub(crate) fn infer_array_access_type(
        &mut self,
        arr: &ArrayAccessExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 数组访问: arr[index]
        let array_type = self.infer_expr_type_internal(&arr.array)?;
        let index_type = self.infer_expr_type_internal(&arr.index)?;

        if !index_type.is_integer() {
            return Err(semantic_error_at_loc(
                &arr.loc,
                format!("Array index must be integer, got {}", index_type),
            ));
        }

        match array_type {
            Type::Array(element_type) => Ok(*element_type),
            _ => Err(semantic_error_at_loc(
                &arr.loc,
                format!("Cannot index non-array type {}", array_type),
            )),
        }
    }
}
