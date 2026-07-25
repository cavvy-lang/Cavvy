//! 6.1.0: ? 运算符代码生成
//!
//! `expr?` 在运行时展开为：
//!   - 如果 Result::isOk 为 true，提取 value 并继续执行
//!   - 如果 Result::isOk 为 false，直接返回当前函数（携带原 Result 对象）
//!
//! 时间复杂度: O(1) IR 生成，运行时 O(1)
//! 空间复杂度: O(1) 额外临时变量

use crate::ast::TryExpr;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};
use crate::types::Type;

impl IRGenerator {
    /// 生成 ? 运算符表达式代码
    ///
    /// # Arguments
    /// * `try_expr` - Try 表达式节点
    ///
    /// # Returns
    /// 成功分支中提取的 value 的 "type value" 字符串
    pub fn generate_try_expression(&mut self, try_expr: &TryExpr) -> CayResult<String> {
        // 1. 推断操作数的 Result<T, E> 类型
        let expr_type = self
            .get_expression_type(&try_expr.expr)
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                "Cannot determine type for '?' operator".to_string(),
            ))?;

        let (base_name, type_args, class_layout_key) =
            self.resolve_result_class_info(&expr_type, &try_expr.loc)?;

        if type_args.len() != 2 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                format!("Result<T, E> requires 2 type arguments, got {}", type_args.len()),
            ));
        }

        let value_type = type_args[0].clone();

        // 2. 生成操作数，得到 Result 对象指针 (i8*)
        let result_value = self.generate_expression(&try_expr.expr)?;
        let (result_llvm_type, result_ptr) = self.parse_typed_value(&result_value);
        let result_ptr_i8 = if result_llvm_type == "i8*" {
            result_ptr
        } else {
            let cast = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast {} {} to i8*",
                cast, result_llvm_type, result_ptr
            ));
            cast
        };

        // 3. 加载 isOk 字段
        let is_ok_field = self
            .get_instance_field(&class_layout_key, "isOk")
            .cloned()
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                format!("Result class '{}' missing 'isOk' field", class_layout_key),
            ))?;

        let is_ok_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            is_ok_ptr_i8, result_ptr_i8, is_ok_field.offset
        ));
        let is_ok_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i1*",
            is_ok_ptr, is_ok_ptr_i8
        ));
        let is_ok_val = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i1, i1* {}, align {}",
            is_ok_val, is_ok_ptr, self.get_type_align("i1")
        ));

        // 4. 分支：ok 继续，err 直接返回原 Result
        let ok_label = self.new_label("try.ok");
        let err_label = self.new_label("try.err");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok_val, ok_label, err_label
        ));

        // 错误分支：调用析构函数后返回原 Result 对象
        self.emit_line(&format!("{}:", err_label));
        self.emit_all_scope_dtors();
        self.emit_line(&format!("  ret i8* {}", result_ptr_i8));

        // 成功分支：提取 value 字段
        self.emit_line(&format!("{}:", ok_label));
        let value_field = self
            .get_instance_field(&class_layout_key, "value")
            .cloned()
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                format!("Result class '{}' missing 'value' field", class_layout_key),
            ))?;

        let value_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            value_ptr_i8, result_ptr_i8, value_field.offset
        ));
        let value_ptr = self.new_temp();
        let value_ptr_type = if value_field.llvm_type.ends_with('*') {
            value_field.llvm_type.clone()
        } else {
            format!("{}*", value_field.llvm_type)
        };
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}",
            value_ptr, value_ptr_i8, value_ptr_type
        ));
        let value_val = self.new_temp();
        self.emit_line(&format!(
            "  {} = load {}, {} {}, align {}",
            value_val,
            value_field.llvm_type,
            value_ptr_type,
            value_ptr,
            self.get_type_align(&value_field.llvm_type)
        ));

        // 返回 value，类型为 T
        Ok(format!("{} {}", value_field.llvm_type, value_val))
    }

    /// 解析 Result<T, E> 类型信息
    ///
    /// 返回 (基础类名, 类型参数列表, 用于 class_layouts 查找的特化类名)
    fn resolve_result_class_info(
        &self,
        ty: &Type,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<(String, Vec<Type>, String)> {
        let (base_name, type_args) = match ty {
            Type::Generic(name, args) => (name.clone(), args.clone()),
            Type::Object(name) => {
                if let Some(pos) = name.find('<') {
                    let base = name[..pos].to_string();
                    let end = name.len().saturating_sub(1);
                    let args_str = if end > pos + 1 {
                        &name[pos + 1..end]
                    } else {
                        ""
                    };
                    let args: Vec<Type> = if args_str.is_empty() {
                        Vec::new()
                    } else {
                        args_str
                            .split(',')
                            .map(|s| Type::Object(s.trim().to_string()))
                            .collect()
                    };
                    (base, args)
                } else {
                    (name.clone(), Vec::new())
                }
            }
            _ => {
                return Err(codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    loc.clone(),
                    format!("'?' operator requires Result<T, E>, got {}", ty),
                ));
            }
        };

        let is_result = base_name == "Result" || base_name == "std::Result";
        if !is_result {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("'?' operator requires Result<T, E>, got {}", ty),
            ));
        }

        // 解析限定名，确保 class_layouts 键正确
        let qualified_base = if base_name.contains("::") {
            base_name.clone()
        } else if let Some(ref registry) = self.type_registry {
            registry
                .find_qualified_class(&base_name)
                .unwrap_or(base_name.clone())
        } else {
            base_name.clone()
        };

        let args_str: Vec<String> = type_args.iter().map(|t| t.display_name()).collect();
        let layout_key = format!("{}<{ }>", qualified_base, args_str.join(", "));

        Ok((qualified_base, type_args, layout_key))
    }
}
