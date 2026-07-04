//! 成员访问表达式代码生成
//!
//! 处理静态字段访问、对象成员访问和数组 length 属性。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};

impl IRGenerator {
    /// 生成数组长度访问代码（用于 .length 属性或 .length() 方法）
    ///
    /// # Arguments
    /// * `array_expr` - 数组表达式
    pub fn generate_array_length_access(&mut self, array_expr: &Expr) -> CayResult<String> {
        let obj = self.generate_expression(array_expr)?;
        let (obj_type, obj_val) = self.parse_typed_value(&obj);

        // 首先将数组指针转换为 i8*
        let obj_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast {} {} to i8*",
            obj_i8, obj_type, obj_val
        ));

        // 数组长度存储在数组指针前面的 8 字节中
        // 计算长度地址：array_ptr - 8
        let len_ptr_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 -8",
            len_ptr_i8, obj_i8
        ));

        // 将长度指针转换为 i32*
        let len_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i32*",
            len_ptr, len_ptr_i8
        ));

        // 加载长度（作为 i32）
        let len_val = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i32, i32* {}, align 4",
            len_val, len_ptr
        ));

        Ok(format!("i32 {}", len_val))
    }

    /// 从数组访问表达式获取元素类型的类名（如果是对象类型）
    /// 用于 ArrayAccess 上的成员访问（如 tokens[0].intValue）
    fn get_array_element_class_name(&self, arr_access: &ArrayAccessExpr) -> Option<String> {
        let arr_type = self.get_expression_type(&arr_access.array)?;
        match arr_type {
            crate::types::Type::Array(elem) => match *elem {
                crate::types::Type::Object(class_name) => Some(class_name),
                _ => None,
            },
            _ => None,
        }
    }

    /// 生成成员访问表达式代码
    ///
    /// # Arguments
    /// * `member` - 成员访问表达式
    /// 从泛型类名中提取基础类名
    /// 例如: "FileResult<File>" -> "FileResult", "Optional<T>" -> "Optional"
    /// 时间复杂度: O(n)，n为名称长度
    fn extract_base_class_name(&self, name: &str) -> String {
        if let Some(pos) = name.find('<') {
            name[..pos].to_string()
        } else {
            name.to_string()
        }
    }

    pub fn generate_member_access(&mut self, member: &MemberAccessExpr) -> CayResult<String> {
        // 检查是否是类名.静态方法访问: ClassName.methodName
        if let Expr::Identifier(class_name) = &*member.object {
            // 提取基础类名（处理泛型类型如 FileResult<File>）
            let base_class_name = self.extract_base_class_name(class_name.as_ref());

            // 首先检查是否是静态方法访问（返回函数指针）
            if let Some(ref registry) = self.type_registry {
                // 使用基础类名查找类信息
                if let Some(class_info) = registry.get_class(&base_class_name) {
                    if let Some(methods) = class_info.methods.get(&member.member) {
                        // 查找静态方法
                        if let Some(method_info) = methods.iter().find(|m| m.is_static) {
                            // 生成函数指针（函数地址）
                            // 使用 build_function_name_from_method 生成正确的函数名
                            let func_name = self.build_function_name_from_method(
                                &base_class_name,
                                &member.member,
                                &method_info.params,
                                false,
                            );

                            // 获取参数类型和返回类型
                            let param_types: Vec<crate::types::Type> = method_info
                                .params
                                .iter()
                                .filter(|p| !p.is_varargs)
                                .map(|p| p.param_type.clone())
                                .collect();
                            let return_type = method_info.return_type.clone();

                            // 将静态方法打包成闭包格式（环境指针为 null）
                            // 确保 malloc 已声明
                            if !self.is_extern_emitted("malloc@i8*@i64") {
                                self.emit_raw("declare i8* @malloc(i64)");
                                self.mark_extern_emitted("malloc@i8*@i64".to_string());
                            }

                            // 分配结构体内存 { i8* func_ptr, i8* env_ptr }
                            let struct_ptr = self.new_temp();
                            self.emit_line(&format!("  {} = call i8* @malloc(i64 16)", struct_ptr));

                            // 获取函数指针（bitcast 为 i8*）
                            let func_ptr_type = format!(
                                "{} ({})",
                                self.type_to_llvm(&return_type),
                                param_types
                                    .iter()
                                    .map(|p| self.type_to_llvm(p))
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            );
                            let func_ptr_temp = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = bitcast {}* @{} to i8*",
                                func_ptr_temp, func_ptr_type, func_name
                            ));

                            // 存储函数指针到结构体偏移0
                            let func_ptr_slot = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = bitcast i8* {} to i8**",
                                func_ptr_slot, struct_ptr
                            ));
                            self.emit_line(&format!(
                                "  store i8* {}, i8** {}, align 8",
                                func_ptr_temp, func_ptr_slot
                            ));

                            // 存储环境指针（null）到结构体偏移8
                            let env_ptr_slot_temp = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = getelementptr i8, i8* {}, i64 8",
                                env_ptr_slot_temp, struct_ptr
                            ));
                            let env_ptr_slot_cast = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = bitcast i8* {} to i8**",
                                env_ptr_slot_cast, env_ptr_slot_temp
                            ));
                            self.emit_line(&format!(
                                "  store i8* null, i8** {}, align 8",
                                env_ptr_slot_cast
                            ));

                            // 返回函数指针类型（is_closure: true）
                            let func_type = crate::types::Type::Function(Box::new(
                                crate::types::FunctionType {
                                    params: param_types,
                                    return_type: Box::new(return_type),
                                    is_static: true,
                                    is_closure: true,
                                },
                            ));
                            let llvm_func_type = self.type_to_llvm(&func_type);
                            return Ok(format!("{} {}", llvm_func_type, struct_ptr));
                        }
                    }
                }
            }

            // 检查是否是静态字段访问: ClassName.fieldName
            // 使用基础类名构建静态字段键
            let static_key = format!("{}.{}", base_class_name, member.member);
            if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                // 检查是否是数组类型
                let is_array = matches!(field_info.field_type, crate::types::Type::Array(_));

                if is_array {
                    // 静态数组字段 - 直接从全局变量加载数组指针
                    // field_info.llvm_type 是元素类型指针（如 i32*）
                    // 静态字段存储这个指针值
                    let arr_ptr = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = load {}, {}* {}, align {}",
                        arr_ptr,
                        field_info.llvm_type,
                        field_info.llvm_type,
                        field_info.name,
                        self.get_type_align(&field_info.llvm_type)
                    ));
                    return Ok(format!("{} {}", field_info.llvm_type, arr_ptr));
                } else {
                    // 普通静态字段访问 - 返回全局变量的值
                    let temp = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = load {}, {}* {}, align {}",
                        temp,
                        field_info.llvm_type,
                        field_info.llvm_type,
                        field_info.name,
                        self.get_type_align(&field_info.llvm_type)
                    ));
                    return Ok(format!("{} {}", field_info.llvm_type, temp));
                }
            }

            // 检查是否是 enum variant 访问: EnumName.VariantName
            // 使用基础类名查找 enum（支持命名空间前缀）
            if let Some(ref registry) = self.type_registry {
                if let Some(enum_info) = registry.get_enum_by_name(&base_class_name) {
                    if let Some(idx) = enum_info
                        .variants
                        .iter()
                        .position(|v| v.name == member.member)
                    {
                        // 构造 struct { i32 discriminant, i64 payload } 值
                        let struct_val = self.new_temp();
                        self.emit_line(&format!(
                            "  {} = insertvalue {{ i32, i64 }} undef, i32 {}, 0",
                            struct_val, idx
                        ));
                        let struct_val2 = self.new_temp();
                        self.emit_line(&format!(
                            "  {} = insertvalue {{ i32, i64 }} {}, i64 0, 1",
                            struct_val2, struct_val
                        ));
                        return Ok(format!("{{ i32, i64 }} {}", struct_val2));
                    } else {
                        // 枚举存在但没有这个 variant，返回错误
                        let available: Vec<_> =
                            enum_info.variants.iter().map(|v| v.name.clone()).collect();
                        return Err(crate::miette_diagnostic::codegen_error_at(
                            ErrorCodes::CODEGEN_INVALID_OPERATION,
                            member.loc.clone(),
                            format!(
                                "枚举 '{}' 中没有 variant '{}'。可选: {:?}",
                                base_class_name, member.member, available
                            ),
                        ));
                    }
                }
            }

            if let Some(ref registry) = self.type_registry {
                let has_value_binding = self.get_variable_type(class_name.as_ref()).is_some()
                    || self
                        .scope_manager
                        .get_var_type(class_name.as_ref())
                        .is_some()
                    || self.var_types.contains_key(class_name.as_ref())
                    || self.var_class_map.contains_key(class_name.as_ref());
                let known_static_target = registry.class_exists(&base_class_name)
                    || registry.find_qualified_class(&base_class_name).is_some()
                    || registry.get_struct(&base_class_name).is_some()
                    || registry.get_enum_by_name(&base_class_name).is_some();

                if known_static_target && !has_value_binding {
                    return Err(codegen_error_at(
                        ErrorCodes::CODEGEN_INVALID_OPERATION,
                        member.loc.clone(),
                        format!(
                            "Unknown static member '{}' for type '{}'",
                            member.member, base_class_name
                        ),
                    ));
                }
            }
        }

        // 处理实例字段访问: this.fieldName 或 obj.fieldName 或 super.fieldName
        // 也支持嵌套成员访问: obj.field1.field2

        // 确定对象所属的类
        let class_name_opt: Option<String> = if let Expr::Identifier(name) = &*member.object {
            let name_str = name.as_ref();
            if name_str == "this" {
                Some(self.this_field_class_name())
            } else if name_str == "super" {
                // super 访问父类的成员
                if let Some(parent_class) = self.get_parent_class(&self.current_class) {
                    Some(parent_class)
                } else {
                    None
                }
            } else {
                // 尝试从变量类型推断类名
                self.var_class_map.get(name_str).cloned()
            }
        } else if let Expr::MemberAccess(nested_member) = &*member.object {
            // 嵌套成员访问: 需要递归处理并获取字段类型
            // 先生成嵌套成员访问代码，然后从结果类型推断类名
            match self.generate_member_access_with_class_info(nested_member) {
                Ok((_, Some(class_name))) => Some(class_name),
                _ => None,
            }
        } else if let Expr::ArrayAccess(arr_access) = &*member.object {
            // 数组元素访问: 获取元素类型，如果是对象类型则返回类名
            self.get_array_element_class_name(arr_access)
        } else if let Expr::Call(call_expr) = &*member.object {
            // 方法调用返回对象: 从调用返回类型推断类名
            self.get_expression_type(&member.object)
                .and_then(|ty| match ty {
                    crate::types::Type::Object(class_name) => Some(class_name),
                    _ => None,
                })
        } else {
            None
        };

        // 特殊处理数组的 .length 属性（但优先检查是否是对象的字段）
        if member.member == "length" {
            // 首先检查是否是当前对象的字段
            let is_field = if let Some(ref class_name) = class_name_opt {
                self.get_instance_field(class_name, "length").is_some()
            } else {
                false
            };

            // 如果不是字段，则检查是否是数组类型
            if !is_field {
                let obj = self.generate_expression(&member.object)?;
                let (obj_type, obj_val) = self.parse_typed_value(&obj);

                // 检查是否是数组类型（以 * 结尾）
                if obj_type.ends_with("*") {
                    return self.generate_array_length_access(&member.object);
                }
            }
        }

        if let Some(class_name) = class_name_opt {
            if let Some(field_info) = self
                .get_instance_field(&class_name, &member.member)
                .cloned()
            {
                // 实例字段访问
                let is_struct = self.is_struct_type(&class_name);

                if is_struct {
                    // struct 字段访问：使用 getelementptr %struct.Name
                    let llvm_struct_type = format!("%struct.{}", class_name);
                    let obj_ptr = if let Expr::Identifier(name) = &*member.object {
                        if name == "this" {
                            // this 指针：从作用域管理器获取
                            let this_llvm_name = self
                                .scope_manager
                                .get_llvm_name("this")
                                .unwrap_or_else(|| "this_s1".to_string());
                            let temp = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = load {}*, {}** %{}, align 8",
                                temp, llvm_struct_type, llvm_struct_type, this_llvm_name
                            ));
                            temp
                        } else {
                            // 其他变量：生成表达式
                            let obj = self.generate_expression(&member.object)?;
                            let (_, obj_val) = self.parse_typed_value(&obj);
                            obj_val
                        }
                    } else {
                        let obj = self.generate_expression(&member.object)?;
                        let (_, obj_val) = self.parse_typed_value(&obj);
                        obj_val
                    };

                    // 计算字段索引（从 struct 布局中获取字段顺序）
                    let field_idx = self.get_struct_field_index(&class_name, &member.member);
                    let field_ptr = self.new_temp();
                    let ptr_type = if field_info.llvm_type.ends_with('*') {
                        field_info.llvm_type.clone()
                    } else {
                        format!("{}*", field_info.llvm_type)
                    };
                    self.emit_line(&format!(
                        "  {} = getelementptr inbounds {}, {}* {}, i32 0, i32 {}",
                        field_ptr, llvm_struct_type, llvm_struct_type, obj_ptr, field_idx
                    ));

                    // 加载字段值
                    let field_val = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = load {}, {} {}, align {}",
                        field_val,
                        field_info.llvm_type,
                        ptr_type,
                        field_ptr,
                        self.get_type_align(&field_info.llvm_type)
                    ));

                    return Ok(format!("{} {}", field_info.llvm_type, field_val));
                } else {
                    // class 字段访问（原有逻辑）：i8* + offset
                    let obj_ptr = if let Expr::Identifier(name) = &*member.object {
                        if name == "this" || name == "super" {
                            let this_llvm_name = self
                                .scope_manager
                                .get_llvm_name("this")
                                .unwrap_or_else(|| "this_s1".to_string());
                            let temp = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = load i8*, i8** %{}, align 8",
                                temp, this_llvm_name
                            ));
                            temp
                        } else {
                            let obj = self.generate_expression(&member.object)?;
                            let (_, obj_val) = self.parse_typed_value(&obj);
                            obj_val
                        }
                    } else {
                        let obj = self.generate_expression(&member.object)?;
                        let (obj_type, obj_val) = self.parse_typed_value(&obj);
                        if obj_type == "i8*" {
                            obj_val
                        } else {
                            let cast_temp = self.new_temp();
                            self.emit_line(&format!(
                                "  {} = bitcast {} {} to i8*",
                                cast_temp, obj_type, obj_val
                            ));
                            cast_temp
                        }
                    };

                    // 计算字段地址: obj_ptr + offset
                    let field_ptr_i8 = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = getelementptr i8, i8* {}, i64 {}",
                        field_ptr_i8, obj_ptr, field_info.offset
                    ));

                    let field_ptr = self.new_temp();
                    let ptr_type = if field_info.llvm_type.ends_with('*') {
                        field_info.llvm_type.clone()
                    } else {
                        format!("{}*", field_info.llvm_type)
                    };
                    self.emit_line(&format!(
                        "  {} = bitcast i8* {} to {}",
                        field_ptr, field_ptr_i8, ptr_type
                    ));

                    // 加载字段值
                    let field_val = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = load {}, {} {}, align {}",
                        field_val,
                        field_info.llvm_type,
                        ptr_type,
                        field_ptr,
                        self.get_type_align(&field_info.llvm_type)
                    ));

                    return Ok(format!("{} {}", field_info.llvm_type, field_val));
                }
            }
        }

        // 特殊处理 super 标识符 - 返回 this 指针
        if let Expr::Identifier(name) = &*member.object {
            if name == "super" {
                // super 访问使用 this 指针
                if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                    let temp = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = load i8*, i8** %{}, align 8",
                        temp, this_llvm_name
                    ));
                    return Ok(format!("i8* {}", temp));
                }
            }
        }

        // 无法识别的成员访问，返回对象指针作为 fallback
        // 注意：这可能是因为：
        // 1. 访问了外部类型（如C结构体）的字段，这些字段在类型系统中未注册
        // 2. 对象类型无法确定，但运行时可以通过指针偏移访问
        // 3. 其他特殊情况（如 FFI 类型）
        // 生成对象表达式并返回其指针值
        let obj = self.generate_expression(&member.object)?;
        let (_, obj_val) = self.parse_typed_value(&obj);
        Ok(format!("i8* {}", obj_val))
    }

    /// 生成成员访问表达式代码，同时返回类名信息
    ///
    /// # Arguments
    /// * `member` - 成员访问表达式
    ///
    /// # Returns
    /// (LLVM值字符串, Option<类名>)
    fn generate_member_access_with_class_info(
        &mut self,
        member: &MemberAccessExpr,
    ) -> CayResult<(String, Option<String>)> {
        // 确定对象所属的类
        let class_name_opt: Option<String> = if let Expr::Identifier(name) = &*member.object {
            let name_str = name.as_ref();
            if name_str == "this" {
                Some(self.this_field_class_name())
            } else if name_str == "super" {
                self.get_parent_class(&self.current_class)
            } else {
                self.var_class_map.get(name_str).cloned()
            }
        } else if let Expr::MemberAccess(nested_member) = &*member.object {
            // 递归处理嵌套成员访问
            match self.generate_member_access_with_class_info(nested_member) {
                Ok((_, Some(class_name))) => Some(class_name),
                _ => None,
            }
        } else if let Expr::ArrayAccess(arr_access) = &*member.object {
            // 数组元素访问: 获取元素类型，如果是对象类型则返回类名
            self.get_array_element_class_name(arr_access)
        } else if let Expr::Call(_) = &*member.object {
            // 方法调用返回对象: 从调用返回类型推断类名
            self.get_expression_type(&member.object)
                .and_then(|ty| match ty {
                    crate::types::Type::Object(class_name) => Some(class_name),
                    _ => None,
                })
        } else {
            None
        };

        if let Some(ref class_name) = class_name_opt {
            if let Some(field_info) = self.get_instance_field(class_name, &member.member).cloned() {
                // 获取对象指针
                let obj_ptr = if let Expr::Identifier(name) = &*member.object {
                    if name == "this" || name == "super" {
                        let this_llvm_name = self
                            .scope_manager
                            .get_llvm_name("this")
                            .unwrap_or_else(|| "this_s1".to_string());
                        let temp = self.new_temp();
                        self.emit_line(&format!(
                            "  {} = load i8*, i8** %{}, align 8",
                            temp, this_llvm_name
                        ));
                        temp
                    } else {
                        let obj = self.generate_expression(&member.object)?;
                        let (_, obj_val) = self.parse_typed_value(&obj);
                        obj_val
                    }
                } else {
                    // 对于嵌套成员访问，递归生成对象表达式
                    let obj = self.generate_expression(&member.object)?;
                    let (obj_type, obj_val) = self.parse_typed_value(&obj);
                    // 确保对象指针是 i8*，供后续 GEP 使用
                    if obj_type == "i8*" {
                        obj_val
                    } else {
                        let cast_temp = self.new_temp();
                        self.emit_line(&format!(
                            "  {} = bitcast {} {} to i8*",
                            cast_temp, obj_type, obj_val
                        ));
                        cast_temp
                    }
                };

                // 计算字段地址
                let field_ptr_i8 = self.new_temp();
                self.emit_line(&format!(
                    "  {} = getelementptr i8, i8* {}, i64 {}",
                    field_ptr_i8, obj_ptr, field_info.offset
                ));

                // 将字段指针转换为正确类型的指针
                // 注意：如果llvm_type已经是指针类型（如i8**），则不需要再加*
                let field_ptr = self.new_temp();
                let ptr_type = if field_info.llvm_type.ends_with('*') {
                    field_info.llvm_type.clone()
                } else {
                    format!("{}*", field_info.llvm_type)
                };
                self.emit_line(&format!(
                    "  {} = bitcast i8* {} to {}",
                    field_ptr, field_ptr_i8, ptr_type
                ));

                // 加载字段值
                let field_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = load {}, {} {}, align {}",
                    field_val,
                    field_info.llvm_type,
                    ptr_type,
                    field_ptr,
                    self.get_type_align(&field_info.llvm_type)
                ));

                // 从字段类型推断类名
                let result_class_name =
                    if let crate::types::Type::Object(ref inner_class) = field_info.field_type {
                        Some(inner_class.clone())
                    } else {
                        None
                    };

                return Ok((
                    format!("{} {}", field_info.llvm_type, field_val),
                    result_class_name,
                ));
            }
        }

        // 无法处理，回退到普通表达式生成
        let result = self.generate_expression(&Expr::MemberAccess(member.clone()))?;
        Ok((result, None))
    }
}
