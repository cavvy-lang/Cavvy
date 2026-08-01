//! 函数调用表达式代码生成 - 调用目标解析
//!
//! 将 callee 表达式解析为 (类名, 方法名, 接收者表达式, 是否静态调用)，
//! 并处理解析过程中即可直接生成结果的分支
//! （extern/函数指针调用、enum 变体构造、函数指针字段调用）。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};

/// 调用目标解析结果
pub(crate) enum CallTargetResolution {
    /// 已解析的调用目标：(类名, 方法名, 接收者表达式, 是否静态调用)
    Resolved(String, String, Option<Box<Expr>>, bool),
    /// 解析过程中已直接生成完整调用结果（如函数指针调用、enum 变体构造）
    Generated(String),
}

impl IRGenerator {
    /// 解析普通函数调用的目标信息（类名、方法名、接收者表达式、是否静态调用）。
    /// 对于实例方法调用，obj_expr 保存对象表达式以获取 this 指针；
    /// is_static_call 表示是否是类名.方法名() 形式的静态方法调用。
    pub(crate) fn resolve_call_target(
        &mut self,
        call: &CallExpr,
    ) -> CayResult<CallTargetResolution> {
        match call.callee.as_ref() {
            Expr::Identifier(name) => self.resolve_identifier_call_target(call, name),
            Expr::MemberAccess(member) => self.resolve_member_call_target(call, member),
            _ => Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                call.loc.clone(),
                "Invalid function call".to_string(),
            )),
        }
    }

    /// 解析标识符形式的 callee：自由函数、顶层函数、当前类隐式方法调用
    fn resolve_identifier_call_target(
        &mut self,
        call: &CallExpr,
        name: &IdentifierExpr,
    ) -> CayResult<CallTargetResolution> {
        let name_str = name.as_ref();
        // 检查是否是全局 extern 函数
        if let Some(_extern_func) = self.get_extern_function(name_str) {
            return Ok(CallTargetResolution::Generated(
                self.generate_extern_function_call(name_str, &call.args, &call.loc)?,
            ));
        }
        // 检查是否是函数指针变量
        if let Some(var_type) = self.get_variable_type(name_str) {
            if matches!(var_type, crate::types::Type::Function(_)) {
                return Ok(CallTargetResolution::Generated(
                    self.generate_function_pointer_call(name_str, &call.args, &var_type, &call.loc)?,
                ));
            }
        }
        // 检查是否是 @FreeFunction 导出的自由函数
        let free_fn_info = self
            .type_registry
            .as_ref()
            .and_then(|r| r.free_functions.get(name_str))
            .map(|(class_name, method_info, _)| {
                (class_name.clone(), method_info.name.clone())
            });
        if let Some((owner_class, method_name)) = free_fn_info {
            // 将 @FreeFunction 调用转为对应类的静态方法调用
            // 使用注册时的方法名（而非调用时的限定名）
            Ok(CallTargetResolution::Resolved(
                owner_class,
                method_name,
                None,
                true,
            ))
        } else if self.is_top_level_function(name_str) {
            // 顶层函数没有类名前缀
            Ok(CallTargetResolution::Resolved(
                String::new(),
                name_str.to_string(),
                None,
                false,
            ))
        } else if !self.current_class.is_empty() {
            // 泛型特化方法体内使用完整特化类名，确保隐式静态/实例调用链接到
            // 已单态化的特化版本，而非类型擦除的基础模板。
            let class_name = self
                .current_class_specialized
                .clone()
                .unwrap_or_else(|| self.current_class.clone());
            Ok(CallTargetResolution::Resolved(
                class_name,
                name_str.to_string(),
                None,
                false,
            ))
        } else {
            Ok(CallTargetResolution::Resolved(
                String::new(),
                name_str.to_string(),
                None,
                false,
            ))
        }
    }

    /// 解析成员访问形式的 callee（obj.method() / ClassName.method()）
    fn resolve_member_call_target(
        &mut self,
        call: &CallExpr,
        member: &MemberAccessExpr,
    ) -> CayResult<CallTargetResolution> {
        // 检查 object 是否是标识符（类名或变量名）
        match member.object.as_ref() {
            Expr::Identifier(obj_name) => {
                self.resolve_named_object_member_target(call, member, obj_name.as_ref())
            }
            _ => self.resolve_expr_object_member_target(call, member),
        }
    }

    /// 解析 object 为标识符的成员调用：super/this/变量/类名
    fn resolve_named_object_member_target(
        &mut self,
        call: &CallExpr,
        member: &MemberAccessExpr,
        obj_name_str: &str,
    ) -> CayResult<CallTargetResolution> {
        // 特殊处理 super 标识符
        if obj_name_str == "super" {
            // super.methodName() 调用父类的方法
            let parent_class = self
                .get_parent_class(&self.current_class)
                .unwrap_or_else(|| self.current_class.clone());
            Ok(CallTargetResolution::Resolved(
                parent_class,
                member.member.clone(),
                Some(member.object.clone()),
                false,
            ))
        } else if obj_name_str == "this" {
            // this.methodName() - 首先检查是否是函数指针字段
            let class_name = self
                .current_class_specialized
                .clone()
                .unwrap_or_else(|| self.current_class.clone());
            if let Some(field_type) = self.get_field_type(&class_name, &member.member) {
                if matches!(field_type, crate::types::Type::Function(_)) {
                    // 是函数指针字段调用
                    return Ok(CallTargetResolution::Generated(
                        self.generate_member_func_ptr_call(
                            member,
                            &call.args,
                            &field_type,
                            &call.loc,
                        )?,
                    ));
                }
            }
            // 不是函数指针字段，按普通方法处理
            Ok(CallTargetResolution::Resolved(
                class_name,
                member.member.clone(),
                Some(member.object.clone()),
                false,
            ))
        } else {
            // 检查是否是 enum 构造函数调用: EnumName.VariantName(args)
            if let Some(result) = self.try_generate_enum_variant_ctor(call, member, obj_name_str)? {
                return Ok(CallTargetResolution::Generated(result));
            }
            // 首先检查是否是已知的类名
            // 对于泛型类型如 FileResult<File>，需要提取基础类名 FileResult 进行检查
            let base_obj_name = if let Some(lt_pos) = obj_name_str.find('<') {
                &obj_name_str[..lt_pos]
            } else {
                obj_name_str
            };
            let (class_name, is_class) = if let Some(ref registry) = self.type_registry {
                if registry.class_exists(base_obj_name)
                    || registry.find_qualified_class(base_obj_name).is_some()
                    || registry.get_struct(base_obj_name).is_some()
                    || registry.get_enum_by_name(base_obj_name).is_some()
                {
                    // 是类名，保留原始泛型类型名（如 FileResult<File>）
                    (obj_name_str.to_string(), true)
                } else {
                    // 不是类名，尝试从变量映射获取
                    let result = self
                        .var_class_map
                        .get(obj_name_str)
                        .cloned()
                        .unwrap_or_else(|| obj_name_str.to_string());
                    (result, false)
                }
            } else {
                let result = self
                    .var_class_map
                    .get(obj_name_str)
                    .cloned()
                    .unwrap_or_else(|| obj_name_str.to_string());
                (result, false)
            };

            // 如果不是类名（是变量），检查是否是函数指针字段调用
            if !is_class {
                if let Some(field_type) = self.get_field_type(&class_name, &member.member) {
                    if matches!(field_type, crate::types::Type::Function(_)) {
                        // 是函数指针字段调用，生成函数指针调用代码
                        return Ok(CallTargetResolution::Generated(
                            self.generate_member_func_ptr_call(
                                member,
                                &call.args,
                                &field_type,
                                &call.loc,
                            )?,
                        ));
                    }
                }
            }

            // 如果是类名.方法名() 形式，标记为静态方法调用
            Ok(CallTargetResolution::Resolved(
                class_name,
                member.member.clone(),
                Some(member.object.clone()),
                is_class,
            ))
        }
    }

    /// 检查是否是 enum 构造函数调用: EnumName.VariantName(args)
    /// 支持泛型 enum（如 Option<int>.Some(42)），使用基础 enum 名查找。
    /// 命中时生成 struct { i32 discriminant, i64 payload } 并返回 Some(结果)。
    fn try_generate_enum_variant_ctor(
        &mut self,
        call: &CallExpr,
        member: &MemberAccessExpr,
        obj_name_str: &str,
    ) -> CayResult<Option<String>> {
        let enum_base_name = if let Some(lt_pos) = obj_name_str.find('<') {
            &obj_name_str[..lt_pos]
        } else {
            obj_name_str
        };
        if let Some(ref registry) = self.type_registry {
            if let Some(enum_info) = registry.get_enum(enum_base_name) {
                if let Some(idx) = enum_info
                    .variants
                    .iter()
                    .position(|v| v.name == member.member)
                {
                    let has_payload = enum_info.variants[idx].payload_type.is_some();
                    let payload_val = if has_payload {
                        let val = self.generate_expression(&call.args[0])?;
                        let (pl_type, pl_val) = self.parse_typed_value(&val);
                        if pl_type == "i32" {
                            let ext = self.new_temp();
                            self.emit_line(&format!("  {} = sext i32 {} to i64", ext, pl_val));
                            ext
                        } else if let Some(struct_name) =
                            self.extract_struct_name_from_ptr_type(&pl_type)
                        {
                            // 值类型语义：struct payload 堆拷贝后存入
                            // payload 槽，避免 enum 与源变量共享存储。
                            let fresh = self
                                .emit_struct_heap_copy(&pl_val, &struct_name)
                                .unwrap_or_else(|| pl_val.to_string());
                            let ptr_to_i64 = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = ptrtoint {} {} to i64",
                                ptr_to_i64, pl_type, fresh
                            ));
                            ptr_to_i64
                        } else if pl_type == "i8*" || pl_type.ends_with('*') {
                            let ptr_to_i64 = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = ptrtoint {} {} to i64",
                                ptr_to_i64, pl_type, pl_val
                            ));
                            ptr_to_i64
                        } else {
                            pl_val.to_string()
                        }
                    } else {
                        "0".to_string()
                    };
                    // 构造 struct { i32 discriminant, i64 payload }
                    let struct_val = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = insertvalue {{ i32, i64 }} undef, i32 {}, 0",
                        struct_val, idx
                    ));
                    let struct_val2 = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = insertvalue {{ i32, i64 }} {}, i64 {}, 1",
                        struct_val2, struct_val, payload_val
                    ));
                    return Ok(Some(format!("{{ i32, i64 }} {}", struct_val2)));
                }
            }
        }
        Ok(None)
    }

    /// 解析 object 为非标识符表达式（如 new 表达式、链式调用）的成员调用，
    /// 尝试从表达式推断类型
    fn resolve_expr_object_member_target(
        &mut self,
        call: &CallExpr,
        member: &MemberAccessExpr,
    ) -> CayResult<CallTargetResolution> {
        if let Some(obj_type) = self.get_expression_type(&member.object) {
            match obj_type {
                crate::types::Type::Object(class_name) => {
                    // 首先检查是否是函数指针字段
                    if let Some(field_type) = self.get_field_type(&class_name, &member.member) {
                        if matches!(field_type, crate::types::Type::Function(_)) {
                            // 是函数指针字段调用，生成函数指针调用代码
                            return Ok(CallTargetResolution::Generated(
                                self.generate_member_func_ptr_call(
                                    member,
                                    &call.args,
                                    &field_type,
                                    &call.loc,
                                )?,
                            ));
                        }
                    }
                    // 不是函数指针字段，按普通方法处理
                    Ok(CallTargetResolution::Resolved(
                        class_name,
                        member.member.clone(),
                        Some(member.object.clone()),
                        false,
                    ))
                }
                crate::types::Type::Generic(class_name, type_args) => {
                    // 对于泛型类型（如 vector<Student>），处理其方法调用
                    // 建立类型参数映射，支持泛型特化
                    if let Some(ref registry) = self.type_registry {
                        if let Some(class_info) = registry.get_class(&class_name) {
                            if !class_info.type_params.is_empty() && !type_args.is_empty() {
                                for (idx, param) in class_info.type_params.iter().enumerate() {
                                    let resolved_arg = type_args
                                        .get(idx)
                                        .cloned()
                                        .or_else(|| param.default_type.clone())
                                        .unwrap_or_else(|| {
                                            crate::types::Type::GenericParam(param.name.clone())
                                        });
                                    self.generic_type_args
                                        .insert(param.name.clone(), resolved_arg);
                                }
                            }
                        }
                    }
                    // 构建完整特化类名（如 Box<int>）用于方法查找与生成
                    let type_args_str: Vec<String> =
                        type_args.iter().map(|t| t.display_name()).collect();
                    let specialized_class_name =
                        format!("{}<{ }>", class_name, type_args_str.join(", "));
                    // 首先检查是否是函数指针字段
                    if let Some(field_type) = self.get_field_type(&class_name, &member.member) {
                        if matches!(field_type, crate::types::Type::Function(_)) {
                            // 是函数指针字段调用，生成函数指针调用代码
                            return Ok(CallTargetResolution::Generated(
                                self.generate_member_func_ptr_call(
                                    member,
                                    &call.args,
                                    &field_type,
                                    &call.loc,
                                )?,
                            ));
                        }
                    }
                    // 不是函数指针字段，按普通方法处理
                    Ok(CallTargetResolution::Resolved(
                        specialized_class_name,
                        member.member.clone(),
                        Some(member.object.clone()),
                        false,
                    ))
                }
                _ => Err(codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    member.loc.clone(),
                    format!(
                        "Cannot call method '{}' on non-class type",
                        member.member
                    ),
                )),
            }
        } else {
            Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                member.loc.clone(),
                format!("Cannot determine type for method call '{}'", member.member),
            ))
        }
    }

    /// 调用目标类名的特化处理。
    ///
    /// 泛型静态工厂调用的单态化：形如 `Optional<int> x = Optional.of(42)`，
    /// 调用点的 class_name 只有裸类名 "Optional"，若不特化则解析到未定义的
    /// 类型擦除基础模板 Optional_T_.of。此处依据变量声明的期望类型 Optional<int>
    /// 推断类型参数（T=int），将 class_name 特化为 "Optional<int>" 并安装类型映射，
    /// 使调用解析到单态化版本 Optional_int_.of。
    pub(crate) fn specialize_call_class_name(
        &mut self,
        mut class_name: String,
        is_static_call: bool,
        expected_static_type: &Option<crate::types::Type>,
    ) -> String {
        // 若 class_name 含未解析的泛型参数（如 WeakPtr<Tracked> 方法体内出现
        // `Optional<Rc<T>>.of(...)`），用当前 generic_type_args 替换，避免链接到
        // 未定义的类型擦除基础模板。
        if class_name.contains('<') {
            class_name = crate::codegen::specialization::substitute_type_args_in_class_name(
                &class_name,
                &self.generic_type_args,
            );
        }

        if is_static_call && !class_name.contains('<') {
            if let Some(crate::types::Type::Generic(exp_base, exp_args)) = expected_static_type {
                if !exp_args.is_empty() {
                    let exp_base_bare = exp_base.rsplit("::").next().unwrap_or(exp_base);
                    let cn_bare = class_name
                        .rsplit("::")
                        .next()
                        .unwrap_or(&class_name)
                        .to_string();
                    if exp_base_bare == cn_bare {
                        let type_params = self
                            .type_registry
                            .as_ref()
                            .and_then(|r| r.get_class(&cn_bare))
                            .map(|c| c.type_params.clone());
                        if let Some(params) = type_params {
                            if !params.is_empty() {
                                for (idx, param) in params.iter().enumerate() {
                                    let resolved_arg = exp_args
                                        .get(idx)
                                        .cloned()
                                        .or_else(|| param.default_type.clone())
                                        .unwrap_or_else(|| {
                                            crate::types::Type::GenericParam(param.name.clone())
                                        });
                                    self.generic_type_args
                                        .insert(param.name.clone(), resolved_arg);
                                }
                                let args_str: Vec<String> =
                                    exp_args.iter().map(|t| t.display_name()).collect();
                                class_name = format!("{}<{ }>", cn_bare, args_str.join(", "));
                            }
                        }
                    }
                }
            }
        }

        class_name
    }
}
