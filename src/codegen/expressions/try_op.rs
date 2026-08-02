//! 6.1.0: ? 运算符代码生成
//!
//! `expr?` 支持两种操作数类型：
//! - `Result<T, E>`：
//!   - isOk == true → 提取 value 继续
//!   - isOk == false → 调用作用域析构后返回（携带原 Result 或构造 Result<T2, E2>）
//! - `Optional<T>`：
//!   - hasValue == true → 提取 value 继续
//!   - hasValue == false → 调用作用域析构后返回新构造的 `Optional<U>.empty()`
//!     （U == T，由语义阶段校验）
//!
//! 时间复杂度: O(1) IR 生成，运行时 O(1)
//! 空间复杂度: O(1) 额外临时变量

use crate::ast::TryExpr;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};
use crate::types::Type;

/// `?` 运算符操作数的「结果/可选」类型分类。
#[derive(Clone, Copy, PartialEq, Eq)]
enum TryKind {
    Result,
    Optional,
}

impl IRGenerator {
    /// 生成 ? 运算符表达式代码
    ///
    /// # Arguments
    /// * `try_expr` - Try 表达式节点
    ///
    /// # Returns
    /// 成功分支中提取的 value 的 "type value" 字符串
    pub fn generate_try_expression(&mut self, try_expr: &TryExpr) -> CayResult<String> {
        // 1. 推断操作数的类型，并分类为 Result / Optional
        let expr_type = self
            .get_expression_type(&try_expr.expr)
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                "Cannot determine type for '?' operator".to_string(),
            ))?;

        let (kind, type_args, class_layout_key) =
            self.resolve_try_class_info(&expr_type, &try_expr.loc)?;

        match kind {
            TryKind::Result => self.emit_try_for_result(try_expr, type_args, class_layout_key),
            TryKind::Optional => self.emit_try_for_optional(try_expr, type_args, class_layout_key),
        }
    }

    /// `Result<T, E>` 路径：保持 6.1.0 既有的运行时展开语义。
    fn emit_try_for_result(
        &mut self,
        try_expr: &TryExpr,
        type_args: Vec<Type>,
        class_layout_key: String,
    ) -> CayResult<String> {
        if type_args.len() != 2 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                format!("Result<T, E> requires 2 type arguments, got {}", type_args.len()),
            ));
        }

        let value_type = type_args[0].clone();
        let error_type = type_args[1].clone();

        // 函数返回的错误类型 E2（语义阶段已校验：E == E2 或 E 实现 Into<E2>）。
        // 取不到返回类型时退化为 E2 == E（不转换，保持旧行为）。
        let (ret_error_type, ret_layout_key) = match self.current_function_cay_return_type.clone() {
            Some(rt) => match self.resolve_try_class_info(&rt, &try_expr.loc) {
                Ok((TryKind::Result, args, key)) if args.len() == 2 => {
                    (Some(args[1].clone()), Some(key))
                }
                _ => (None, None),
            },
            None => (None, None),
        };
        let needs_conversion = matches!(&ret_error_type, Some(e2) if *e2 != error_type);

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

        // 错误分支：调用析构函数后返回
        self.emit_line(&format!("{}:", err_label));
        self.emit_all_scope_dtors();
        if !needs_conversion {
            // E == E2：直接返回原 Result 对象
            self.emit_line(&format!("  ret i8* {}", result_ptr_i8));
        } else {
            // E ≠ E2：调用 e.into() 转换错误，重新构造 Result<T2, E2> 返回
            // （ROADMAP 6.1.x：return Result::err(e.into())）
            let e2_type = ret_error_type.clone().unwrap();
            let ret_key = ret_layout_key.clone().unwrap();
            let converted = self.emit_try_into_conversion(
                &result_ptr_i8,
                &class_layout_key,
                &error_type,
                &e2_type,
                &try_expr.loc,
            )?;
            let new_obj =
                self.emit_try_construct_err_result(&ret_key, &converted, &try_expr.loc)?;
            self.emit_line(&format!("  ret i8* {}", new_obj));
        }

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

    /// `Optional<T>` 路径：
    /// - hasValue == true → 提取 value 字段（类型 T）
    /// - hasValue == false → 构造 `Optional<U>.empty()` 返回（U == T）
    fn emit_try_for_optional(
        &mut self,
        try_expr: &TryExpr,
        type_args: Vec<Type>,
        class_layout_key: String,
    ) -> CayResult<String> {
        if type_args.len() != 1 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                format!("Optional<T> requires 1 type argument, got {}", type_args.len()),
            ));
        }

        let value_type = type_args[0].clone();

        // 函数返回类型必须为 Optional<U>（语义阶段已校验 U == T）。
        // 提前取好返回类型的 layout 键，用于空值分支构造 Optional<U>.empty()。
        let ret_layout_key = match self.current_function_cay_return_type.clone() {
            Some(rt) => match self.resolve_try_class_info(&rt, &try_expr.loc) {
                Ok((TryKind::Optional, _, key)) => Some(key),
                _ => None,
            },
            None => None,
        };

        // 2. 生成操作数，得到 Optional 对象指针 (i8*)
        let opt_value = self.generate_expression(&try_expr.expr)?;
        let (opt_llvm_type, opt_ptr) = self.parse_typed_value(&opt_value);
        let opt_ptr_i8 = if opt_llvm_type == "i8*" {
            opt_ptr
        } else {
            let cast = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast {} {} to i8*",
                cast, opt_llvm_type, opt_ptr
            ));
            cast
        };

        // 3. 加载 hasValue 字段
        let has_value_field = self
            .get_instance_field(&class_layout_key, "hasValue")
            .cloned()
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                format!("Optional class '{}' missing 'hasValue' field", class_layout_key),
            ))?;

        let has_value_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            has_value_ptr_i8, opt_ptr_i8, has_value_field.offset
        ));
        let has_value_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i1*",
            has_value_ptr, has_value_ptr_i8
        ));
        let has_value_val = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i1, i1* {}, align {}",
            has_value_val, has_value_ptr, self.get_type_align("i1")
        ));

        // 4. 分支：present 继续，empty 提前返回
        let present_label = self.new_label("try.present");
        let empty_label = self.new_label("try.empty");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            has_value_val, present_label, empty_label
        ));

        // 空值分支：调用作用域析构后构造并返回 Optional<U>.empty()
        self.emit_line(&format!("{}:", empty_label));
        self.emit_all_scope_dtors();
        let empty_obj = match ret_layout_key.clone() {
            Some(ret_key) => {
                self.emit_try_construct_empty_optional(&ret_key, &try_expr.loc)?
            }
            None => {
                // 取不到返回类型 layout 时退化为复用操作数 layout
                // （操作数已是 Optional<T>，与 Optional<U> 同构，因为 U == T）。
                self.emit_try_construct_empty_optional(&class_layout_key, &try_expr.loc)?
            }
        };
        self.emit_line(&format!("  ret i8* {}", empty_obj));

        // 有值分支：提取 value 字段
        self.emit_line(&format!("{}:", present_label));
        let value_field = self
            .get_instance_field(&class_layout_key, "value")
            .cloned()
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                format!("Optional class '{}' missing 'value' field", class_layout_key),
            ))?;

        let value_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            value_ptr_i8, opt_ptr_i8, value_field.offset
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
        let _ = &value_type; // 仅用于语义校验上下文，IR 不需要
        Ok(format!("{} {}", value_field.llvm_type, value_val))
    }

    /// 解析 ? 运算符操作数的「结果/可选」类型信息
    ///
    /// 返回 `(kind, 类型参数列表, 用于 class_layouts 查找的特化类名)`。
    /// kind 标识操作数是 Result 还是 Optional。
    fn resolve_try_class_info(
        &self,
        ty: &Type,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<(TryKind, Vec<Type>, String)> {
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
                    format!("'?' operator requires Result<T, E> or Optional<T>, got {}", ty),
                ));
            }
        };

        let is_result = base_name == "Result" || base_name == "std::Result";
        let is_optional = base_name == "Optional" || base_name == "std::Optional";

        if !is_result && !is_optional {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("'?' operator requires Result<T, E> or Optional<T>, got {}", ty),
            ));
        }

        let kind = if is_result { TryKind::Result } else { TryKind::Optional };

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

        Ok((kind, type_args, layout_key))
    }

    /// `?` 的 Into 错误转换：从操作数 Result 对象取出 error 字段，
    /// 通过 vtable 分派调用 `e.into()`，返回 E2 的 "type value" 字符串。
    ///
    /// 语义阶段已保证 E 实现 Into<E2>，vtable 槽位必然存在。
    fn emit_try_into_conversion(
        &mut self,
        result_ptr_i8: &str,
        class_layout_key: &str,
        error_type: &Type,
        e2_type: &Type,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // 1. 加载 error 字段（与 value 字段同构的布局访问）
        let error_field = self
            .get_instance_field(class_layout_key, "error")
            .cloned()
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("Result class '{}' missing 'error' field", class_layout_key),
            ))?;

        let err_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            err_ptr_i8, result_ptr_i8, error_field.offset
        ));
        let err_ptr = self.new_temp();
        let err_ptr_type = if error_field.llvm_type.ends_with('*') {
            error_field.llvm_type.clone()
        } else {
            format!("{}*", error_field.llvm_type)
        };
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}",
            err_ptr, err_ptr_i8, err_ptr_type
        ));
        let err_val = self.new_temp();
        self.emit_line(&format!(
            "  {} = load {}, {} {}, align {}",
            err_val,
            error_field.llvm_type,
            err_ptr_type,
            err_ptr,
            self.get_type_align(&error_field.llvm_type)
        ));

        // 2. 统一为 i8*（实现 Into 的错误类型惯例上是类；值类型无法在此出现，
        //    因为基础类型无法实现接口——语义阶段已拦截）
        let err_i8 = if error_field.llvm_type == "i8*" {
            err_val
        } else if error_field.llvm_type.ends_with('*') {
            let cast = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast {} {} to i8*",
                cast, error_field.llvm_type, err_val
            ));
            cast
        } else {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!(
                    "'?' Into conversion requires a class error type, got {}",
                    error_type
                ),
            ));
        };

        // 3. 若错误类有「仅返回类型不同」的 into() 重载集合（实现多个 Into
        //    实例化），按 ? 的目标错误类型 E2 静态选择重载并直接调用——
        //    vtable 槽位按裸名分配只有一个，无法区分这些重载。
        if let Some(direct) =
            self.try_emit_return_overload_into_call(error_type, e2_type, &err_i8)?
        {
            return Ok(direct);
        }

        // 4. vtable 分派 e.into()：vtable 指针位于对象头 offset 8
        let vtable_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            vtable_ptr_temp, err_i8
        ));
        let vtable_temp = self.new_temp();
        self.emit_line(&format!("  {} = load i8*, i8* {}", vtable_temp, vtable_ptr_temp));
        let vtable_array_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8**",
            vtable_array_temp, vtable_temp
        ));

        let interface_names = ["std::Into", "Into"];
        // 目标错误类型 E2 即 Into<E2> 的类型实参——vtable 槽位按此实参区分
        // 同一错误类实现 Into<A>/Into<B> 等多个实例化时的不同 into() 重载。
        let interface_type_args = vec![e2_type.clone()];
        let mut slot = None;
        for interface_name in &interface_names {
            if self.interface_has_vtable_slot_with_type_args(
                interface_name,
                "into",
                &[],
                &interface_type_args,
            ) {
                slot = Some(self.get_interface_vtable_slot_with_type_args(
                    interface_name,
                    "into",
                    &[],
                    &interface_type_args,
                ));
                break;
            }
        }
        let slot = slot.ok_or_else(|| codegen_error_at(
            ErrorCodes::CODEGEN_INVALID_OPERATION,
            loc.clone(),
            format!("error type {} has no Into vtable slot", error_type),
        ))?;

        let slot_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8*, i8** {}, i64 {}",
            slot_ptr_temp, vtable_array_temp, slot
        ));
        let fn_ptr_temp = self.new_temp();
        self.emit_line(&format!("  {} = load i8*, i8** {}", fn_ptr_temp, slot_ptr_temp));

        let e2_llvm = self.type_to_llvm(e2_type);
        let fn_cast_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {} (i8*)*",
            fn_cast_temp, fn_ptr_temp, e2_llvm
        ));
        let converted_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call {} {}(i8* {})",
            converted_temp, e2_llvm, fn_cast_temp, err_i8
        ));

        Ok(format!("{} {}", e2_llvm, converted_temp))
    }

    /// 若错误类实现了多个 `Into` 实例化（同名同参数、仅返回类型不同的
    /// `into()` 重载集合），按 `?` 的目标错误类型 E2 静态选择重载并直接调用。
    ///
    /// 返回 `Some("type value")` 表示已发射直接调用；`None` 表示回退 vtable 分派。
    /// vtable 槽位按裸名分配只有一个，无法区分这些重载，因此必须静态选择。
    fn try_emit_return_overload_into_call(
        &mut self,
        error_type: &Type,
        e2_type: &Type,
        err_i8: &str,
    ) -> CayResult<Option<String>> {
        // 1. 取错误类的注册表信息
        let raw_name = match error_type {
            Type::Object(name) => name.clone(),
            Type::Generic(name, _) => name.clone(),
            _ => return Ok(None),
        };
        let base_name = raw_name.split('<').next().unwrap_or(&raw_name).to_string();
        let Some(ref registry) = self.type_registry else {
            return Ok(None);
        };
        let Some(class_info) = registry.get_class(&base_name).or_else(|| {
            let bare = base_name.rsplit("::").next().unwrap_or(&base_name);
            registry.get_class(bare)
        }) else {
            return Ok(None);
        };
        let Some(methods) = class_info.methods.get("into") else {
            return Ok(None);
        };

        // 2. 仅处理无参实例 into() 重载集合；单一实现回退 vtable 路径
        let candidates: Vec<&crate::types::MethodInfo> = methods
            .iter()
            .filter(|m| !m.is_static && m.params.is_empty())
            .collect();
        if candidates.len() <= 1 {
            return Ok(None);
        }

        // 3. 按目标错误类型 E2 选重载（先精确比较，再按去命名空间的裸名比较）
        let e2_name = e2_type.display_name();
        let e2_bare = e2_name.rsplit("::").next().unwrap_or(&e2_name);
        let target = candidates.iter().find(|m| {
            if m.return_type == *e2_type {
                return true;
            }
            let ret_name = m.return_type.display_name();
            ret_name == e2_name || ret_name.rsplit("::").next().unwrap_or(&ret_name) == e2_bare
        });
        let Some(target) = target else {
            return Ok(None);
        };

        // 4. 直接调用消歧后的符号（与定义处 generate_method_name 同名）
        let fn_name = self.mangle_method_with_return_disambiguation(
            &class_info.name,
            "into",
            &[],
            &target.return_type,
            &target.loc,
        );
        let e2_llvm = self.type_to_llvm(e2_type);
        let converted_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call {} @{}(i8* {})",
            converted_temp, e2_llvm, fn_name, err_i8
        ));
        Ok(Some(format!("{} {}", e2_llvm, converted_temp)))
    }

    /// `?` 的 Into 错误转换：手工构造 `Result<T2, E2>` 错误对象
    /// （calloc + 对象头 + isOk=false + error 字段），返回 i8* 对象指针。
    ///
    /// Result<T2,E2> 作为当前函数返回类型必然已被特化收集，
    /// 布局/vtable/方法均已生成，这里只按布局写字段。
    fn emit_try_construct_err_result(
        &mut self,
        ret_layout_key: &str,
        converted: &str,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        let (converted_ty, converted_val) = self.parse_typed_value(converted);
        let registry_name = ret_layout_key
            .split('<')
            .next()
            .unwrap_or(ret_layout_key)
            .to_string();

        // 1. 分配对象
        let obj_size = self
            .get_class_layout(ret_layout_key)
            .map(|layout| layout.total_size as i64)
            .unwrap_or(8);
        let obj = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i8* @calloc(i64 1, i64 {})",
            obj, obj_size
        ));

        // 2. 对象头：type_id (offset 0) + vtable 指针 (offset 8)
        let type_id_value = self.get_type_id_value(&registry_name).unwrap_or(0);
        let type_id_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", type_id_ptr, obj));
        self.emit_line(&format!(
            "  store i32 {}, i32* {}",
            type_id_value, type_id_ptr
        ));

        let llvm_class = self.get_qualified_class_name(ret_layout_key);
        let vtable_name = format!("{}.vtable", llvm_class);
        let vtable_size = self
            .type_registry
            .as_ref()
            .and_then(|r| r.get_class(&registry_name))
            .and_then(|c| c.vtable_layout.as_ref())
            .map(|v| v.size)
            .unwrap_or(0);
        let vtable_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            vtable_ptr_temp, obj
        ));
        if vtable_size > 0 {
            let vtable_global_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast [{} x i8*]* @{} to i8*",
                vtable_global_temp, vtable_size, vtable_name
            ));
            self.emit_line(&format!(
                "  store i8* {}, i8* {}",
                vtable_global_temp, vtable_ptr_temp
            ));
        } else {
            self.emit_line(&format!("  store i8* null, i8* {}", vtable_ptr_temp));
        }

        // 3. isOk = false
        let is_ok_field = self
            .get_instance_field(ret_layout_key, "isOk")
            .cloned()
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("Result class '{}' missing 'isOk' field", ret_layout_key),
            ))?;
        let is_ok_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            is_ok_ptr_i8, obj, is_ok_field.offset
        ));
        let is_ok_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i1*", is_ok_ptr, is_ok_ptr_i8));
        self.emit_line(&format!("  store i1 0, i1* {}", is_ok_ptr));

        // 4. error = converted
        let error_field = self
            .get_instance_field(ret_layout_key, "error")
            .cloned()
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("Result class '{}' missing 'error' field", ret_layout_key),
            ))?;
        let err_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            err_ptr_i8, obj, error_field.offset
        ));
        let err_ptr = self.new_temp();
        let err_ptr_type = if converted_ty.ends_with('*') {
            converted_ty.clone()
        } else {
            format!("{}*", converted_ty)
        };
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}",
            err_ptr, err_ptr_i8, err_ptr_type
        ));
        self.emit_line(&format!(
            "  store {} {}, {} {}",
            converted_ty, converted_val, err_ptr_type, err_ptr
        ));

        Ok(obj)
    }

    /// `?` 的 Optional 路径：手工构造 `Optional<U>.empty()` 空对象
    /// （calloc + 对象头 + hasValue=false），返回 i8* 对象指针。
    ///
    /// Optional<U> 作为当前函数返回类型必然已被特化收集，
    /// 布局/vtable/方法均已生成；value 字段由 calloc 零初始化即可，
    /// 不需要再写入（语义上 Optional.empty() 不携带值）。
    fn emit_try_construct_empty_optional(
        &mut self,
        ret_layout_key: &str,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        let registry_name = ret_layout_key
            .split('<')
            .next()
            .unwrap_or(ret_layout_key)
            .to_string();

        // 1. 分配对象
        let obj_size = self
            .get_class_layout(ret_layout_key)
            .map(|layout| layout.total_size as i64)
            .unwrap_or(8);
        let obj = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i8* @calloc(i64 1, i64 {})",
            obj, obj_size
        ));

        // 2. 对象头：type_id (offset 0) + vtable 指针 (offset 8)
        let type_id_value = self.get_type_id_value(&registry_name).unwrap_or(0);
        let type_id_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", type_id_ptr, obj));
        self.emit_line(&format!(
            "  store i32 {}, i32* {}",
            type_id_value, type_id_ptr
        ));

        let llvm_class = self.get_qualified_class_name(ret_layout_key);
        let vtable_name = format!("{}.vtable", llvm_class);
        let vtable_size = self
            .type_registry
            .as_ref()
            .and_then(|r| r.get_class(&registry_name))
            .and_then(|c| c.vtable_layout.as_ref())
            .map(|v| v.size)
            .unwrap_or(0);
        let vtable_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            vtable_ptr_temp, obj
        ));
        if vtable_size > 0 {
            let vtable_global_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast [{} x i8*]* @{} to i8*",
                vtable_global_temp, vtable_size, vtable_name
            ));
            self.emit_line(&format!(
                "  store i8* {}, i8* {}",
                vtable_global_temp, vtable_ptr_temp
            ));
        } else {
            self.emit_line(&format!("  store i8* null, i8* {}", vtable_ptr_temp));
        }

        // 3. hasValue = false（value 字段由 calloc 零初始化，不写入）
        let has_value_field = self
            .get_instance_field(ret_layout_key, "hasValue")
            .cloned()
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("Optional class '{}' missing 'hasValue' field", ret_layout_key),
            ))?;
        let has_value_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            has_value_ptr_i8, obj, has_value_field.offset
        ));
        let has_value_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i1*",
            has_value_ptr, has_value_ptr_i8
        ));
        self.emit_line(&format!("  store i1 0, i1* {}", has_value_ptr));

        Ok(obj)
    }
}
