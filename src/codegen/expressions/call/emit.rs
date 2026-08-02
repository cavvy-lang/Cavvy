//! 函数调用表达式代码生成 - 参数构建与调用发射
//!
//! 处理方法调用的命名参数重排、可变参数打包、this 指针构建、
//! 参数类型转换以及 vtable 动态分派/直接调用的 IR 发射。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};
use crate::semantic::resolve_call_args;
use crate::types::Type;

impl IRGenerator {
    /// 检查是否有命名参数需要重排；有则按形参顺序重排并返回 Some，
    /// 否则返回 None（调用方直接使用原始参数列表）。
    pub(crate) fn reorder_named_args(
        &mut self,
        call: &CallExpr,
        class_name: &str,
        method_name: &str,
    ) -> CayResult<Option<Vec<Expr>>> {
        let has_named_args = call.args.iter().any(|a| matches!(a, Expr::NamedArg(_)));
        if !has_named_args {
            return Ok(None);
        }
        // 获取方法形参以进行重排
        let params = self
            .get_method_params(class_name, method_name)
            .ok_or_else(|| {
                codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    call.loc.clone(),
                    format!(
                        "Cannot resolve parameters for '{}' to process named arguments",
                        method_name
                    ),
                )
            })?;
        let resolved = resolve_call_args(&call.args, &params).map_err(|msg| {
            codegen_error_at(ErrorCodes::CODEGEN_INVALID_OPERATION, call.loc.clone(), msg)
        })?;
        Ok(Some(resolved.args))
    }

    /// 生成实参表达式代码，处理 ArrayList.add 的 RAII 所有权转移，
    /// 并将可变参数打包成数组。返回 (处理后的参数, 是否创建了可变参数数组)。
    /// `expected_param_types` 为单态化计划的具体形参类型（实例泛型方法），
    /// 用于以期望签名发射 lambda 实参。
    pub(crate) fn generate_and_pack_args(
        &mut self,
        class_name: &str,
        method_name: &str,
        actual_args: &[Expr],
        is_varargs_method: bool,
        expected_param_types: Option<&[Type]>,
    ) -> CayResult<(Vec<String>, bool)> {
        // 生成参数表达式
        let mut arg_results = Vec::new();
        for (idx, arg) in actual_args.iter().enumerate() {
            // lambda 实参：若该位置有形参 fn 类型，以其期望签名发射 lambda
            if let Expr::Lambda(_) = arg {
                if let Some(Type::Function(ft)) =
                    expected_param_types.and_then(|types| types.get(idx))
                {
                    self.pending_lambda_expected_fn = Some((**ft).clone());
                }
            }
            arg_results.push(self.generate_expression(arg)?);
            self.pending_lambda_expected_fn = None;
        }

        // ROADMAP 5.3.x 自动 RAII：ArrayList.add 视为所有权转移。
        // 若实参是带析构函数的局部对象变量，将其从当前作用域析构候选中移除，
        // 避免容器析构元素时再次析构该局部变量（double-free）。
        if method_name == "add" {
            let base_class = class_name
                .find('<')
                .map_or(class_name, |pos| &class_name[..pos]);
            if base_class == "ArrayList" || base_class == "std::ArrayList" {
                for arg in actual_args {
                    if let Expr::Identifier(ident) = arg {
                        let var_name = &ident.name;
                        if let Some(var_type) = self.get_variable_type(var_name) {
                            if self.type_has_destructor(&var_type).is_some() {
                                self.scope_manager
                                    .remove_dtor_candidate_by_var_name(var_name);
                            }
                        }
                    }
                }
            }
        }

        // 处理可变参数：将多余参数打包成数组
        if is_varargs_method {
            let packed = self.pack_varargs_args(class_name, method_name, &arg_results)?;
            // 如果原始参数多于固定参数数量，说明创建了数组
            let (fixed_count, _) = self.get_varargs_info(class_name, method_name);
            let has_array = arg_results.len() > fixed_count;
            Ok((packed, has_array))
        } else {
            Ok((arg_results, false))
        }
    }

    /// 为实例方法构建 this 参数并加入 final_args。
    /// 返回 obj_expr 求值结果的缓存：下方 final_args 构建和 resolved_this_val
    /// 计算都需要 obj 的值，若 obj_expr 是带副作用的链式调用
    /// （如 sb.append("a").append("b")），重复求值会导致重复执行，缓存一次即可。
    pub(crate) fn push_this_arg(
        &mut self,
        obj_expr: &Option<Box<Expr>>,
        is_instance_method: bool,
        is_struct_target: bool,
        this_llvm_type: &str,
        final_args: &mut Vec<String>,
    ) -> CayResult<Option<String>> {
        let mut cached_obj_val: Option<String> = None;

        if is_instance_method {
            // 获取 this 指针
            if let Some(obj) = obj_expr {
                // 检查是否是 super 标识符
                if let Expr::Identifier(name) = obj.as_ref() {
                    if name.as_ref() == "super" {
                        // super.methodName() 使用 this 指针
                        if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                            let this_temp = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = load {}, {}* %{}, align 8",
                                this_temp, this_llvm_type, this_llvm_type, this_llvm_name
                            ));
                            final_args.push(format!("{} {}", this_llvm_type, this_temp));
                        } else {
                            final_args.push(format!("{} null", this_llvm_type));
                        }
                    } else {
                        // 通过对象表达式获取 this 指针（如 obj1.getId()）
                        let obj_result = self.generate_expression(obj)?;
                        let (obj_type, obj_val) = self.parse_typed_value(&obj_result);
                        cached_obj_val = Some(obj_val.clone());
                        if is_struct_target && obj_type.starts_with("%struct.") {
                            final_args.push(format!("{} {}", obj_type, obj_val));
                        } else {
                            final_args.push(format!("{} {}", this_llvm_type, obj_val));
                        }
                    }
                } else {
                    // 通过对象表达式获取 this 指针（链式调用等）
                    let obj_result = self.generate_expression(obj)?;
                    let (obj_type, obj_val) = self.parse_typed_value(&obj_result);
                    cached_obj_val = Some(obj_val.clone());
                    if is_struct_target && obj_type.starts_with("%struct.") {
                        final_args.push(format!("{} {}", obj_type, obj_val));
                    } else {
                        final_args.push(format!("{} {}", this_llvm_type, obj_val));
                    }
                }
            } else if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                // 通过当前方法的 this 获取（如在实例方法中调用其他实例方法）
                let this_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = load {}, {}* %{}, align 8",
                    this_temp, this_llvm_type, this_llvm_type, this_llvm_name
                ));
                final_args.push(format!("{} {}", this_llvm_type, this_temp));
            } else {
                // 在静态方法中调用实例方法且没有对象表达式，使用 null 作为 this
                final_args.push(format!("{} null", this_llvm_type));
            }
        }

        Ok(cached_obj_val)
    }

    /// 添加其他参数（根据需要进行类型转换）
    pub(crate) fn append_converted_args(
        &mut self,
        processed_args: &[String],
        param_types: &[crate::types::Type],
        final_args: &mut Vec<String>,
    ) {
        for (idx, arg_str) in processed_args.iter().enumerate() {
            let (arg_type, arg_val) = self.parse_typed_value(arg_str);

            // 检查是否需要类型转换
            if idx < param_types.len() {
                let param_llvm_type = self.type_to_llvm(&param_types[idx]);
                let converted_arg = self.convert_arg_type(&arg_type, &arg_val, &param_llvm_type);
                final_args.push(converted_arg);
            } else {
                final_args.push(arg_str.clone());
            }
        }
    }

    /// 预先计算 this 指针值（用于 vtable 分派和直接调用都可能需要）。
    /// 对于非 super、非标识符的 obj_expr（链式调用），使用 final_args 构建
    /// 阶段已缓存的求值结果，避免重复生成带副作用的链式表达式代码。
    pub(crate) fn resolve_this_value(
        &mut self,
        is_static_call: bool,
        obj_expr: &Option<Box<Expr>>,
        cached_obj_val: &Option<String>,
    ) -> Option<String> {
        if is_static_call {
            None
        } else if let Some(obj) = obj_expr {
            if let Expr::Identifier(name) = obj.as_ref() {
                if name.as_ref() == "super" {
                    if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                        let this_temp = self.new_temp();
                        self.emit_line(&format!(
                            "  {} = load i8*, i8** %{}, align 8",
                            this_temp, this_llvm_name
                        ));
                        Some(this_temp)
                    } else {
                        None
                    }
                } else if let Some(cached_val) = cached_obj_val {
                    Some(cached_val.clone())
                } else {
                    None
                }
            } else if let Some(cached_val) = cached_obj_val {
                Some(cached_val.clone())
            } else {
                None
            }
        } else if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
            let this_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = load i8*, i8** %{}, align 8",
                this_temp, this_llvm_name
            ));
            Some(this_temp)
        } else {
            None
        }
    }

    /// 发射方法调用 IR。
    /// 检查是否需要 vtable 间接调用：
    /// 条件：是实例方法，有可用的 this 指针，类有 vtable 布局，且方法不是 private
    /// private 方法不需要动态分派，直接调用即可。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_method_call(
        &mut self,
        class_name: &str,
        method_name: &str,
        is_static_call: bool,
        is_instance_method: bool,
        resolved_this_val: Option<String>,
        param_types: &[crate::types::Type],
        processed_args: &[String],
        fn_name: &str,
        ret_type: &crate::types::Type,
        llvm_ret_type: &str,
        final_args: &[String],
    ) -> String {
        let is_private = self.is_private_method(class_name, method_name);
        let is_interface_dispatch = !is_static_call && self.is_interface_type(class_name);
        // 接口动态分派需要传入接口的类型实参，以便区分同一泛型接口的不同实例化
        // （如 Into<IOError>::into 与 Into<ParseError>::into 应命中不同 vtable 槽位）。
        let interface_type_args: Vec<crate::types::Type> = if is_interface_dispatch {
            parse_interface_type_args(class_name)
        } else {
            Vec::new()
        };
        let has_dispatch_slot = if is_interface_dispatch {
            self.interface_has_vtable_slot_with_type_args(
                class_name,
                method_name,
                param_types,
                &interface_type_args,
            )
        } else {
            self.class_has_vtable(class_name)
        };
        // 带方法级类型参数的泛型方法不入 vtable：其单态化副本按调用点命名，
        // 必须直接调用，不能走 vtable 槽位间接分派。
        let is_method_level_generic =
            self.method_has_method_level_type_params(class_name, method_name);
        let needs_vtable_dispatch = is_instance_method
            && resolved_this_val.is_some()
            && has_dispatch_slot
            && !is_private
            && !is_method_level_generic;

        if needs_vtable_dispatch {
            self.emit_vtable_dispatch_call(
                class_name,
                method_name,
                is_interface_dispatch,
                resolved_this_val.unwrap(),
                param_types,
                processed_args,
                ret_type,
                llvm_ret_type,
                final_args,
                &interface_type_args,
            )
        } else {
            // 直接调用
            if llvm_ret_type == "void" {
                self.emit_line(&format!(
                    "  call void @{}({})",
                    fn_name,
                    final_args.join(", ")
                ));
                "void %dummy".to_string()
            } else {
                let temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = call {} @{}({})",
                    temp,
                    llvm_ret_type,
                    fn_name,
                    final_args.join(", ")
                ));
                format!("{} {}", llvm_ret_type, temp)
            }
        }
    }

    /// 发射 vtable 动态分派调用：从对象头加载 vtable，按槽位取函数指针后间接调用。
    #[allow(clippy::too_many_arguments)]
    fn emit_vtable_dispatch_call(
        &mut self,
        class_name: &str,
        method_name: &str,
        is_interface_dispatch: bool,
        this_val: String,
        param_types: &[crate::types::Type],
        processed_args: &[String],
        ret_type: &crate::types::Type,
        llvm_ret_type: &str,
        final_args: &[String],
        interface_type_args: &[crate::types::Type],
    ) -> String {
        // 计算 vtable 指针位置（this + 8）
        let vtable_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            vtable_ptr_temp, this_val
        ));

        // 加载 vtable 指针
        let vtable_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8* {}",
            vtable_temp, vtable_ptr_temp
        ));

        let slot = if is_interface_dispatch {
            // 接口分派：传入类型实参以区分泛型接口的不同实例化
            // （如 Into<IOError>::into 与 Into<ParseError>::into）
            self.get_interface_vtable_slot_with_type_args(
                class_name,
                method_name,
                param_types,
                interface_type_args,
            )
        } else {
            self.get_vtable_slot(class_name, method_name, param_types)
        };

        // 将 vtable 指针转换为 i8** 数组（每个元素是 i8*）
        let vtable_array_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8**",
            vtable_array_temp, vtable_temp
        ));

        // 从 vtable 加载函数指针（slot * 8 偏移）
        let slot_offset_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8*, i8** {}, i64 {}",
            slot_offset_temp, vtable_array_temp, slot
        ));
        let fn_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8** {}",
            fn_ptr_temp, slot_offset_temp
        ));

        // 将 i8* 转换为正确的函数指针类型
        let fn_type = self.build_function_type_string(
            ret_type,
            processed_args,
            class_name,
            method_name,
        );
        let fn_ptr_cast_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to {}",
            fn_ptr_cast_temp, fn_ptr_temp, fn_type
        ));

        // 间接调用
        if llvm_ret_type == "void" {
            self.emit_line(&format!(
                "  call void {}({})",
                fn_ptr_cast_temp,
                final_args.join(", ")
            ));
            "void %dummy".to_string()
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = call {} {}({})",
                temp,
                llvm_ret_type,
                fn_ptr_cast_temp,
                final_args.join(", ")
            ));
            format!("{} {}", llvm_ret_type, temp)
        }
    }
}

/// 从接口名中解析出泛型类型实参。
///
/// 接口分派调用点的 `class_name` 形如 `Into<IOError>` 或 `std::Into<IOError>`，
/// 这里提取尖括号内的类型实参列表，作为 vtable 槽位查找的输入，
/// 让同一泛型接口的不同实例化命中各自的独立槽位。
///
/// 不含尖括号的接口名（如 `Iterator`）返回空向量，保持非泛型接口既有行为。
fn parse_interface_type_args(class_name: &str) -> Vec<Type> {
    let Some(pos) = class_name.find('<') else {
        return Vec::new();
    };
    let end = class_name.rfind('>').unwrap_or(class_name.len());
    if end <= pos + 1 {
        return Vec::new();
    }
    let args_str = &class_name[pos + 1..end];
    if args_str.trim().is_empty() {
        return Vec::new();
    }
    args_str
        .split(',')
        .map(|s| Type::Object(s.trim().to_string()))
        .collect()
}
