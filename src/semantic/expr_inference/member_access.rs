//! 成员访问类型推断

use super::super::analyzer::SemanticAnalyzer;
use super::helpers::{check_member_access, semantic_error_at_loc};
use crate::ast::*;
use crate::types::Type;

impl SemanticAnalyzer {
    /// 推断成员访问类型
    pub(crate) fn infer_member_access_type(
        &mut self,
        member: &MemberAccessExpr,
    ) -> crate::miette_diagnostic::CayResult<Type> {
        // 检查是否是静态字段或方法访问: ClassName.fieldName 或 ClassName.methodName
        if let Expr::Identifier(class_name) = &*member.object {
            let raw_class_name = class_name.as_ref();
            if !self.identifier_has_value_binding(raw_class_name) {
                let (class_name_str, type_args) = self.split_generic_type_name(raw_class_name);
                let resolved_class_name =
                    if let Some(class_info) = self.type_registry.get_class(&class_name_str) {
                        Some(class_info.name.clone())
                    } else if let Some(qualified_name) =
                        self.type_registry.find_qualified_class(&class_name_str)
                    {
                        Some(qualified_name)
                    } else if self.type_registry.get_struct(&class_name_str).is_some() {
                        Some(class_name_str.clone())
                    } else {
                        None
                    };

                if let Some(resolved_class_name) = resolved_class_name {
                    if let Some(class_info) = self.type_registry.get_class(&resolved_class_name) {
                        // 首先检查字段
                        if let Some(field_info) = class_info.fields.get(&member.member) {
                            if field_info.is_static {
                                // 检查字段访问权限
                                check_member_access(
                                    &member.member,
                                    field_info.is_public,
                                    field_info.is_protected,
                                    field_info.is_private,
                                    &self.current_class,
                                    &resolved_class_name,
                                    &self.type_registry,
                                    &member.loc,
                                )?;
                                return Ok(field_info.field_type.clone());
                            }
                            return Err(semantic_error_at_loc(
                                &member.loc,
                                format!(
                                    "Non-static field '{}' in class '{}' cannot be referenced from a static context",
                                    member.member, resolved_class_name
                                ),
                            ));
                        }

                        let (has_instance_method, candidate_methods) = self
                            .collect_static_method_candidates(&resolved_class_name, &member.member);
                        let candidate_methods: Vec<_> = candidate_methods
                            .into_iter()
                            .map(|(owner_class, method_info)| {
                                let owner_type_params = self
                                    .type_registry
                                    .get_class(&owner_class)
                                    .map(|ci| ci.type_params.clone())
                                    .unwrap_or_default();
                                (
                                    owner_class,
                                    self.specialize_method_info(
                                        &method_info,
                                        &owner_type_params,
                                        type_args.as_deref(),
                                    ),
                                )
                            })
                            .collect();

                        // 检查静态方法 - 返回函数指针类型
                        if let Some((owner_class, method_info)) = candidate_methods.first() {
                            // 检查方法访问权限
                            check_member_access(
                                &member.member,
                                method_info.is_public,
                                method_info.is_protected,
                                method_info.is_private,
                                &self.current_class,
                                owner_class,
                                &self.type_registry,
                                &member.loc,
                            )?;
                            // 返回函数指针类型
                            let param_types = method_info
                                .params
                                .iter()
                                .filter(|p| !p.is_varargs)
                                .map(|p| p.param_type.clone())
                                .collect();
                            let return_type = Box::new(method_info.return_type.clone());
                            return Ok(Type::Function(Box::new(crate::types::FunctionType {
                                params: param_types,
                                return_type,
                                is_static: true,
                                is_closure: false,
                            })));
                        }

                        if has_instance_method {
                            return Err(semantic_error_at_loc(
                                &member.loc,
                                format!(
                                    "Non-static method '{}' in class '{}' cannot be referenced from a static context",
                                    member.member, resolved_class_name
                                ),
                            ));
                        }

                        return Err(semantic_error_at_loc(
                            &member.loc,
                            self.unknown_static_member_message(
                                &member.member,
                                &resolved_class_name,
                            ),
                        ));
                    }

                    if let Some(struct_info) = self.type_registry.get_struct(&resolved_class_name) {
                        if struct_info.fields.contains_key(&member.member) {
                            return Err(semantic_error_at_loc(
                                &member.loc,
                                format!(
                                    "Non-static field '{}' in struct '{}' cannot be referenced from a static context",
                                    member.member, resolved_class_name
                                ),
                            ));
                        }

                        let (has_instance_method, candidate_methods) = self
                            .collect_static_method_candidates(&resolved_class_name, &member.member);
                        let candidate_methods: Vec<_> = candidate_methods
                            .into_iter()
                            .map(|(owner_class, method_info)| {
                                (
                                    owner_class,
                                    self.specialize_method_info(
                                        &method_info,
                                        &[],
                                        type_args.as_deref(),
                                    ),
                                )
                            })
                            .collect();

                        if let Some((owner_class, method_info)) = candidate_methods.first() {
                            check_member_access(
                                &member.member,
                                method_info.is_public,
                                method_info.is_protected,
                                method_info.is_private,
                                &self.current_class,
                                owner_class,
                                &self.type_registry,
                                &member.loc,
                            )?;
                            let param_types = method_info
                                .params
                                .iter()
                                .filter(|p| !p.is_varargs)
                                .map(|p| p.param_type.clone())
                                .collect();
                            let return_type = Box::new(method_info.return_type.clone());
                            return Ok(Type::Function(Box::new(crate::types::FunctionType {
                                params: param_types,
                                return_type,
                                is_static: true,
                                is_closure: false,
                            })));
                        }

                        if has_instance_method {
                            return Err(semantic_error_at_loc(
                                &member.loc,
                                format!(
                                    "Non-static method '{}' in struct '{}' cannot be referenced from a static context",
                                    member.member, resolved_class_name
                                ),
                            ));
                        }

                        return Err(semantic_error_at_loc(
                            &member.loc,
                            self.unknown_static_member_message(
                                &member.member,
                                &resolved_class_name,
                            ),
                        ));
                    }
                }

                // 检查是否是 enum variant 访问: EnumName.VariantName
                if let Some(enum_info) = self.type_registry.get_enum_by_name(&class_name_str) {
                    let variant_exists = enum_info.variants.iter().any(|v| v.name == member.member);
                    if variant_exists {
                        return Ok(Type::Object(enum_info.name.clone()));
                    }
                    return Err(semantic_error_at_loc(
                        &member.loc,
                        format!(
                            "Unknown variant '{}' for enum {}",
                            member.member, enum_info.name
                        ),
                    ));
                }
            }
        }

        // 成员访问类型检查
        let obj_type = self.infer_expr_type_internal(&member.object)?;

        // 特殊处理数组的 .length 属性
        if member.member == "length" {
            if let Type::Array(_) = obj_type {
                return Ok(Type::Int32); // length 返回 int
            }
        }

        // 特殊处理 String 类型方法
        if obj_type == Type::String {
            match member.member.as_str() {
                "length" => return Ok(Type::Int32),
                _ => {}
            }
        }

        // 检查静态方法中是否访问非静态成员
        if self.current_method_is_static {
            // 检查是否是 this 访问
            if let Expr::Identifier(name) = &*member.object {
                if name == "this" {
                    return Err(semantic_error_at_loc(
                        &member.loc,
                        format!(
                            "non-static variable {} cannot be referenced from a static context",
                            member.member
                        ),
                    ));
                }
            }
        }

        // 类/struct 成员访问
        // 处理 Type::Object、Type::Generic 和 Type::GenericParam
        // 提取基础类名和类型参数
        let (base_class_name_opt, type_args_opt): (Option<String>, Option<Vec<Type>>) =
            match &obj_type {
                Type::Object(class_name) => {
                    // 解析泛型类名: "Optional<T>" -> ("Optional", Some([T]))
                    // 支持多类型参数: "Pair<int, String>" -> ("Pair", Some([int, String]))
                    if let Some(pos) = class_name.find('<') {
                        let base = class_name[..pos].to_string();
                        let args_str = &class_name[pos + 1..class_name.len() - 1];
                        // 解析多个类型参数，用逗号分隔
                        let type_args: Vec<Type> = args_str
                            .split(',')
                            .map(|s| s.trim())
                            .filter(|s| !s.is_empty())
                            .map(|s| self.parse_type_string(s))
                            .collect();
                        if type_args.is_empty() {
                            (Some(base), None)
                        } else {
                            (Some(base), Some(type_args))
                        }
                    } else {
                        (Some(class_name.clone()), None)
                    }
                }
                Type::Generic(class_name, args) => {
                    // Type::Generic 直接返回类名和类型参数
                    (Some(class_name.clone()), Some(args.clone()))
                }
                Type::GenericParam(param_name) => {
                    // 泛型类型参数：使用 bound（默认为 Object）查找方法
                    let bound_name = self
                        .current_class_type_params
                        .iter()
                        .find(|p| &p.name == param_name)
                        .and_then(|p| p.bound.clone())
                        .unwrap_or_else(|| "Object".to_string());
                    (Some(bound_name), None)
                }
                _ => (None, None),
            };

        if let Some(base_class_name) = base_class_name_opt {
            // 先查 struct
            if let Some(struct_info) = self.type_registry.get_struct(&base_class_name) {
                if let Some(field_info) = struct_info.fields.get(&member.member) {
                    return Ok(field_info.field_type.clone());
                }
            }
            // 沿继承链查找字段：先查当前类，再逐级查父类
            {
                let mut current_opt = Some(base_class_name.to_string());
                while let Some(cls_name) = current_opt {
                    if let Some(ci) = self.type_registry.get_class(&cls_name) {
                        if let Some(field_info) = ci.fields.get(&member.member) {
                            // 在定义该字段的类上检查访问权限
                            check_member_access(
                                &member.member,
                                field_info.is_public,
                                field_info.is_protected,
                                field_info.is_private,
                                &self.current_class,
                                &cls_name,
                                &self.type_registry,
                                &member.loc,
                            )?;
                            // 如果类有泛型参数，需要进行类型替换
                            if !ci.type_params.is_empty() {
                                if let Some(ref type_args) = type_args_opt {
                                    let substituted_type = self.substitute_type_params(
                                        &field_info.field_type,
                                        &ci.type_params,
                                        type_args,
                                    );
                                    return Ok(substituted_type);
                                }
                            }
                            return Ok(field_info.field_type.clone());
                        }
                        current_opt = ci.parent.clone();
                    } else {
                        break;
                    }
                }
            }
            // 检查是否是 enum variant 访问
            if let Some(enum_info) = self.type_registry.get_enum(&base_class_name) {
                if enum_info.variants.iter().any(|v| v.name == member.member) {
                    return Ok(obj_type.clone());
                }
            }
            return Err(semantic_error_at_loc(
                &member.loc,
                format!(
                    "Unknown member '{}' for class {}",
                    member.member, base_class_name
                ),
            ));
        }

        Err(semantic_error_at_loc(
            &member.loc,
            format!(
                "Cannot access member '{}' on type {}",
                member.member, obj_type
            ),
        ))
    }
}
