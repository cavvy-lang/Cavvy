//! 6.3.0: ? 运算符代码生成（基于 std::Try<T, E> 接口分派）
//!
//! `expr?` 通过 `std::Try<T, E>` 接口分派，不再硬编码到 Result/Optional。
//! 任何实现了 `std::Try<T, E>` 的类型均可作为操作数——与实现 `Iterator`
//! 即可用于 `for` 一致。
//!
//! 展开规则（操作数 `e : Self implements Try<T, E>`，
//! 函数返回类型 `R implements Try<T2, E2>`，T == T2 且
//! （E == E2 或 E 实现 `Into<E2>`））：
//!
//!   e?  =>
//!     {
//!       Self __t = e;
//!       if (__t.isOk()) {
//!         __t.getValue()              // 继续求值，类型 T
//!       } else {
//!         if (Self == R) {
//!           return __t;               // 零开销快路径（同型直返）
//!         }
//!         E  __err = __t.getError();
//!         E2 __e2 = __err.into();     // E == E2 时省略
//!         return R.fromError(__e2);   // 经返回类型 vtable 分派，this = null
//!       }
//!     }
//!
//! vtable 分派细节：
//! - isOk/getValue/getError：经操作数对象头 vtable 指针（offset 8）分派，
//!   槽位按 `Try<T, E>` 实例化查找。
//! - fromError：操作数在 err 分支不再持有同型实例；编译器直接访问返回类型 R
//!   的 vtable 全局 `@R.vtable`，按 `Try<T2, E2>::fromError` 槽位分派，
//!   this 传入 null（实现约定不访问 this）。
//!
//! 时间复杂度: O(1) IR 生成，运行时 O(1)（vtable 间接调用）
//! 空间复杂度: O(1) 额外临时变量

use crate::ast::TryExpr;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};
use crate::types::Type;

/// Try 接口的候选名称（带/不带命名空间前缀）。
const TRY_INTERFACE_NAMES: [&str; 2] = ["std::Try", "Try"];

impl IRGenerator {
    /// 生成 ? 运算符表达式代码。
    ///
    /// 返回成功分支中提取的 value 的 "type value" 字符串。
    pub fn generate_try_expression(&mut self, try_expr: &TryExpr) -> CayResult<String> {
        // 1. 推断操作数类型并解析 Try<T, E>
        let expr_type = self
            .get_expression_type(&try_expr.expr)
            .ok_or_else(|| codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                "Cannot determine type for '?' operator".to_string(),
            ))?;

        let (value_type, error_type) =
            self.resolve_try_type_args_codegen(&expr_type, &try_expr.loc)?;

        // 2. 取当前函数返回类型并解析 Try<T2, E2>
        let return_type = self.current_function_cay_return_type.clone().ok_or_else(|| {
            codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                try_expr.loc.clone(),
                "The '?' operator can only be used inside a function with a return type".to_string(),
            )
        })?;

        let (ret_value_type, ret_error_type) =
            self.resolve_try_type_args_codegen(&return_type, &try_expr.loc)?;

        // 3. 计算布局键（用于 vtable 全局名与类型等价比较）
        let operand_layout_key = self.compute_try_layout_key(&expr_type);
        let ret_layout_key = self.compute_try_layout_key(&return_type);

        // 4. 生成操作数，得到对象指针 (i8*)
        let operand_value = self.generate_expression(&try_expr.expr)?;
        let (operand_llvm_type, operand_ptr) = self.parse_typed_value(&operand_value);
        let operand_i8 = if operand_llvm_type == "i8*" {
            operand_ptr
        } else {
            let cast = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast {} {} to i8*",
                cast, operand_llvm_type, operand_ptr
            ));
            cast
        };

        // 5. 经操作数 vtable 分派 isOk()
        let value_llvm = self.type_to_llvm(&value_type);
        let error_llvm = self.type_to_llvm(&error_type);
        let is_ok_result = self.emit_try_object_vtable_call(
            &operand_i8,
            "isOk",
            &value_type,
            &error_type,
            "i1",
            &try_expr.loc,
        )?;
        // is_ok_result 形如 "i1 %t1"；提取值部分用于 br 指令
        let is_ok_val = self.parse_typed_value(&is_ok_result).1;

        // 6. 分支：ok 继续，err 处理后返回
        let ok_label = self.new_label("try.ok");
        let err_label = self.new_label("try.err");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            is_ok_val, ok_label, err_label
        ));

        // 7. 错误分支：调用作用域析构后构造失败值返回
        self.emit_line(&format!("{}:", err_label));
        self.emit_all_scope_dtors();

        let self_eq_ret = operand_layout_key == ret_layout_key;
        let needs_conversion = error_type != ret_error_type;

        if self_eq_ret {
            // 零开销快路径：Self == R（同型），直接返回操作数对象。
            // 此时 T == T2 且 E == E2，操作数本身就是合法的返回值。
            self.emit_line(&format!("  ret i8* {}", operand_i8));
        } else if !needs_conversion {
            // E == E2 但 Self != R：getError() + fromError(E2)
            let err_result = self.emit_try_object_vtable_call(
                &operand_i8,
                "getError",
                &value_type,
                &error_type,
                &error_llvm,
                &try_expr.loc,
            )?;
            let new_obj = self.emit_try_from_error_call(
                &ret_layout_key,
                &ret_value_type,
                &ret_error_type,
                &err_result,
                &try_expr.loc,
            )?;
            self.emit_line(&format!("  ret i8* {}", new_obj));
        } else {
            // E != E2：getError() + into() + fromError(E2)
            let err_result = self.emit_try_object_vtable_call(
                &operand_i8,
                "getError",
                &value_type,
                &error_type,
                &error_llvm,
                &try_expr.loc,
            )?;
            let converted = self.emit_into_conversion_on_error(
                &err_result,
                &error_type,
                &ret_error_type,
                &try_expr.loc,
            )?;
            let new_obj = self.emit_try_from_error_call(
                &ret_layout_key,
                &ret_value_type,
                &ret_error_type,
                &converted,
                &try_expr.loc,
            )?;
            self.emit_line(&format!("  ret i8* {}", new_obj));
        }

        // 8. 成功分支：经操作数 vtable 分派 getValue()
        self.emit_line(&format!("{}:", ok_label));
        let value_result = self.emit_try_object_vtable_call(
            &operand_i8,
            "getValue",
            &value_type,
            &error_type,
            &value_llvm,
            &try_expr.loc,
        )?;
        Ok(value_result)
    }

    // ------------------------------------------------------------------
    // Try<T, E> 类型解析（codegen 侧，与 semantic 侧 resolve_try_type_args 对齐）
    // ------------------------------------------------------------------

    /// 解析类型是否实现了 `std::Try<T, E>` 接口，返回解析后的 (T, E)。
    ///
    /// 沿父类链查找接口实现。接口声明中的类型参数（如 `Try<T, E>` 中的 T、E）
    /// 被替换为类实例化时的实际类型实参。例如 `Result<int, String>` 实现了
    /// `Try<int, String>`，返回 `Some((int, String))`；`Optional<int>` 实现了
    /// `Try<int, Object>`，返回 `Some((int, Object))`。
    fn resolve_try_type_args_codegen(
        &self,
        ty: &Type,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<(Type, Type)> {
        let (class_name, class_type_args) = match ty {
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
                    format!(
                        "The '?' operator requires the operand to implement std::Try<T, E>, got {}",
                        ty
                    ),
                ));
            }
        };

        let mut current = Some(class_name);
        let mut visited = std::collections::HashSet::new();
        while let Some(name) = current {
            if !visited.insert(name.clone()) {
                break;
            }
            let class_info = match self
                .type_registry
                .as_ref()
                .and_then(|r| r.get_class(&name))
            {
                Some(info) => info,
                None => {
                    let bare = name.rsplit("::").next().unwrap_or(&name);
                    match self
                        .type_registry
                        .as_ref()
                        .and_then(|r| r.get_class(bare))
                    {
                        Some(info) => info,
                        None => return Err(codegen_error_at(
                            ErrorCodes::CODEGEN_INVALID_OPERATION,
                            loc.clone(),
                            format!(
                                "The '?' operator requires the operand to implement std::Try<T, E>, got {}",
                                ty
                            ),
                        )),
                    }
                }
            };

            // 查找 Try 接口实现
            for iface in &class_info.interfaces {
                let bare = match iface {
                    Type::Object(n) | Type::Generic(n, _) => {
                        n.split('<').next().unwrap_or(n)
                    }
                    _ => continue,
                };
                if bare != "Try" && bare != "std::Try" {
                    continue;
                }
                if let Type::Generic(_, iface_args) = iface {
                    if iface_args.len() != 2 {
                        continue;
                    }
                    let resolved_t = substitute_type_params_codegen(
                        &iface_args[0],
                        &class_info.type_params,
                        &class_type_args,
                    );
                    let resolved_e = substitute_type_params_codegen(
                        &iface_args[1],
                        &class_info.type_params,
                        &class_type_args,
                    );
                    return Ok((resolved_t, resolved_e));
                }
            }
            current = class_info.parent.clone();
        }

        Err(codegen_error_at(
            ErrorCodes::CODEGEN_INVALID_OPERATION,
            loc.clone(),
            format!(
                "The '?' operator requires the operand to implement std::Try<T, E>, got {}",
                ty
            ),
        ))
    }

    /// 计算类型的布局键（用于 vtable 全局名查找与类型等价比较）。
    ///
    /// 键格式与 `class_layouts` 一致：`{qualified_base}<{args}>`，
    /// 例如 `std::Result<int, String>`。
    fn compute_try_layout_key(&self, ty: &Type) -> String {
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
            _ => return ty.display_name(),
        };

        let qualified_base = if base_name.contains("::") {
            base_name
        } else if let Some(ref registry) = self.type_registry {
            registry
                .find_qualified_class(&base_name)
                .unwrap_or(base_name.clone())
        } else {
            base_name
        };

        let args_str: Vec<String> = type_args.iter().map(|t| t.display_name()).collect();
        format!("{}<{ }>", qualified_base, args_str.join(", "))
    }

    // ------------------------------------------------------------------
    // vtable 分派：对象头 vtable（isOk/getValue/getError）
    // ------------------------------------------------------------------

    /// 经对象头 vtable 分派 Try 接口的实例方法（isOk/getValue/getError）。
    ///
    /// 对象头布局：`[i32 type_id][i8* vtable_ptr][...fields]`，
    /// vtable_ptr 位于 offset 8。
    ///
    /// 返回 "type value" 字符串（如 `"i1 %t1"` 或 `"i32 %t2"`）。
    fn emit_try_object_vtable_call(
        &mut self,
        obj_i8: &str,
        method_name: &str,
        value_type: &Type,
        error_type: &Type,
        ret_llvm_type: &str,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // 1. 加载对象头 vtable 指针 (offset 8)
        let vtable_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            vtable_ptr_temp, obj_i8
        ));
        let vtable_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8* {}",
            vtable_temp, vtable_ptr_temp
        ));
        let vtable_array_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8**",
            vtable_array_temp, vtable_temp
        ));

        // 2. 查找 Try 接口方法槽位
        let interface_type_args = vec![value_type.clone(), error_type.clone()];
        let slot = self.find_try_vtable_slot(method_name, &interface_type_args, loc)?;

        // 3. 加载函数指针
        let slot_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8*, i8** {}, i64 {}",
            slot_ptr_temp, vtable_array_temp, slot
        ));
        let fn_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8** {}",
            fn_ptr_temp, slot_ptr_temp
        ));

        // 4. 转换为正确函数指针类型并间接调用
        // isOk/getValue/getError 均无额外参数，仅 this (i8*)
        let fn_type = format!("{} (i8*)*", ret_llvm_type);
        let fn_cast_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}",
            fn_cast_temp, fn_ptr_temp, fn_type
        ));

        let result_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call {} {}(i8* {})",
            result_temp, ret_llvm_type, fn_cast_temp, obj_i8
        ));

        Ok(format!("{} {}", ret_llvm_type, result_temp))
    }

    // ------------------------------------------------------------------
    // vtable 分派：返回类型 vtable 全局（fromError）
    // ------------------------------------------------------------------

    /// 经返回类型 R 的 vtable 全局分派 `Try<T2, E2>::fromError(E2)`。
    ///
    /// 不持有 R 的实例；直接访问 vtable 全局 `@R.vtable` 取槽位函数指针。
    /// this 传入 null（接口约定：fromError 不访问 this，视作静态工厂）。
    ///
    /// `err_result` 为错误值的 "type value" 字符串（如 `"i8* %err"`）。
    /// 返回新构造对象的 i8* 值名（如 `"%obj"`）。
    fn emit_try_from_error_call(
        &mut self,
        ret_layout_key: &str,
        ret_value_type: &Type,
        ret_error_type: &Type,
        err_result: &str,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // 1. 解析错误值并按需转换为 E2_llvm
        let e2_llvm = self.type_to_llvm(ret_error_type);
        let (err_llvm_type, err_val) = self.parse_typed_value(err_result);
        let err_arg = if err_llvm_type == e2_llvm {
            // 类型一致，直接使用
            err_val
        } else if err_llvm_type.ends_with('*') && e2_llvm.ends_with('*') {
            // 均为指针类型，bitcast 转换
            let cast = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast {} {} to {}",
                cast, err_llvm_type, err_val, e2_llvm
            ));
            cast
        } else {
            // 类型不兼容（理论不应发生：语义阶段已校验 E == E2 或 Into<E2>）
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!(
                    "fromError argument type {} does not match expected {}",
                    err_llvm_type, e2_llvm
                ),
            ));
        };

        // 2. 取返回类型 vtable 全局名
        let registry_name = ret_layout_key
            .split('<')
            .next()
            .unwrap_or(ret_layout_key)
            .to_string();
        let llvm_class = self.get_qualified_class_name(ret_layout_key);
        let vtable_name = format!("{}.vtable", llvm_class);
        let vtable_size = self
            .type_registry
            .as_ref()
            .and_then(|r| r.get_class(&registry_name))
            .and_then(|c| c.vtable_layout.as_ref())
            .map(|v| v.size)
            .unwrap_or(0);

        if vtable_size == 0 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!(
                    "Return type '{}' has no vtable layout; cannot dispatch Try::fromError",
                    ret_layout_key
                ),
            ));
        }

        // 3. 查找 Try::fromError 槽位
        let interface_type_args = vec![ret_value_type.clone(), ret_error_type.clone()];
        let slot =
            self.find_try_vtable_slot("fromError", &interface_type_args, loc)?;

        // 4. 从 vtable 全局加载函数指针
        let vtable_array_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast [{} x i8*]* @{} to i8**",
            vtable_array_temp, vtable_size, vtable_name
        ));
        let slot_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8*, i8** {}, i64 {}",
            slot_ptr_temp, vtable_array_temp, slot
        ));
        let fn_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8** {}",
            fn_ptr_temp, slot_ptr_temp
        ));

        // 5. 转换为正确函数指针类型并间接调用
        // fromError 签名: Try<T, E> fromError(E error)
        // LLVM 层面: i8* (i8* this, E2_llvm error)*
        let fn_type = format!("i8* (i8*, {})*", e2_llvm);
        let fn_cast_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}",
            fn_cast_temp, fn_ptr_temp, fn_type
        ));

        let result_temp = self.new_temp();
        // this = null（接口约定：fromError 不访问 this）
        self.emit_line(&format!(
            "  {} = call i8* {}(i8* null, {} {})",
            result_temp, fn_cast_temp, e2_llvm, err_arg
        ));

        Ok(result_temp)
    }

    // ------------------------------------------------------------------
    // Into 转换：错误值 e.into() → E2
    // ------------------------------------------------------------------

    /// `?` 的 Into 错误转换：从 `getError()` 返回的错误值，通过 vtable 分派
    /// 调用 `e.into()`，返回 E2 的 "type value" 字符串。
    ///
    /// 语义阶段已保证 E 实现 Into<E2>，vtable 槽位必然存在。
    fn emit_into_conversion_on_error(
        &mut self,
        err_result: &str,
        error_type: &Type,
        e2_type: &Type,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // 1. 解析错误值
        let (err_llvm_type, err_val) = self.parse_typed_value(err_result);

        // 2. 统一为 i8*（实现 Into 的错误类型惯例上是类；值类型无法在此出现，
        //    因为基础类型无法实现接口——语义阶段已拦截）
        let err_i8 = if err_llvm_type == "i8*" {
            err_val
        } else if err_llvm_type.ends_with('*') {
            let cast = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast {} {} to i8*",
                cast, err_llvm_type, err_val
            ));
            cast
        } else {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!(
                    "'?' Into conversion requires a class error type, got {} ({})",
                    error_type, err_llvm_type
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
        self.emit_line(&format!(
            "  {} = load i8*, i8* {}",
            vtable_temp, vtable_ptr_temp
        ));
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
        self.emit_line(&format!(
            "  {} = load i8*, i8** {}",
            fn_ptr_temp, slot_ptr_temp
        ));

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

    // ------------------------------------------------------------------
    // Try vtable 槽位查找
    // ------------------------------------------------------------------

    /// 查找 Try 接口方法在 vtable 中的槽位编号。
    ///
    /// `arg_types` 由接口声明中的方法形参类型决定（如 `fromError(E)` 的
    /// `arg_types = [Type::GenericParam("E")]`），与槽位注册时的签名一致。
    /// `interface_type_args` 为接口实例化的类型实参（如 `[T, E]`）。
    fn find_try_vtable_slot(
        &self,
        method_name: &str,
        interface_type_args: &[Type],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<usize> {
        // 从 Try 接口声明中取方法形参类型，确保与槽位注册时签名一致。
        let arg_types: Vec<Type> = self
            .type_registry
            .as_ref()
            .and_then(|r| {
                for name in &TRY_INTERFACE_NAMES {
                    if let Some(iface) = r.get_interface(name) {
                        if let Some(method) = iface.methods.get(method_name) {
                            return Some(
                                method
                                    .params
                                    .iter()
                                    .map(|p| p.param_type.clone())
                                    .collect(),
                            );
                        }
                    }
                }
                None
            })
            .unwrap_or_default();

        for interface_name in &TRY_INTERFACE_NAMES {
            if self.interface_has_vtable_slot_with_type_args(
                interface_name,
                method_name,
                &arg_types,
                interface_type_args,
            ) {
                return Ok(self.get_interface_vtable_slot_with_type_args(
                    interface_name,
                    method_name,
                    &arg_types,
                    interface_type_args,
                ));
            }
        }

        Err(codegen_error_at(
            ErrorCodes::CODEGEN_INVALID_OPERATION,
            loc.clone(),
            format!(
                "Try::{} vtable slot not found (interface_type_args: {})",
                method_name,
                interface_type_args
                    .iter()
                    .map(|t| t.display_name())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        ))
    }
}

/// 将类型中的泛型参数替换为类实例化时的实际类型实参。
/// 例如：类 `Result<T, E>` 实例化为 `Result<int, String>` 时，
/// GenericParam("T") / Object("T") -> int，GenericParam("E") / Object("E") -> String。
///
/// 注意：解析器将接口类型实参 `T` 存储为 `Type::Object("T")` 而非
/// `Type::GenericParam("T")`，故需同时检查两种形式，并回查类的
/// `type_params` 以识别「名字与类类型参数同名」的 Object 实例。
fn substitute_type_params_codegen(
    ty: &Type,
    type_params: &[crate::types::TypeParamInfo],
    type_args: &[Type],
) -> Type {
    match ty {
        Type::GenericParam(name) => {
            if let Some(idx) = type_params.iter().position(|p| &p.name == name) {
                if idx < type_args.len() {
                    return type_args[idx].clone();
                }
            }
            ty.clone()
        }
        Type::Object(name) => {
            // 解析器将接口类型实参 `T` 存储为 `Type::Object("T")`；
            // 若该名字与类的类型参数同名，替换为对应的实际类型实参。
            if let Some(idx) = type_params.iter().position(|p| &p.name == name) {
                if idx < type_args.len() {
                    return type_args[idx].clone();
                }
            }
            ty.clone()
        }
        Type::Generic(name, args) => Type::Generic(
            name.clone(),
            args.iter()
                .map(|a| substitute_type_params_codegen(a, type_params, type_args))
                .collect(),
        ),
        Type::Int32 | Type::Int64 | Type::Float32 | Type::Float64
        | Type::Bool | Type::Char | Type::String | Type::Void => ty.clone(),
        _ => ty.clone(),
    }
}
