//! 成员访问表达式代码生成
//!
//! 处理静态字段访问、对象成员访问和数组 length 属性。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成数组长度访问代码（用于 .length 属性或 .length() 方法）
    ///
    /// # Arguments
    /// * `array_expr` - 数组表达式
    pub fn generate_array_length_access(&mut self, array_expr: &Expr) -> cayResult<String> {
        let obj = self.generate_expression(array_expr)?;
        let (obj_type, obj_val) = self.parse_typed_value(&obj);

        // 首先将数组指针转换为 i8*
        let obj_i8 = self.new_temp();
        self.emit_line(&format!("  {} = bitcast {} {} to i8*", obj_i8, obj_type, obj_val));

        // 数组长度存储在数组指针前面的 8 字节中
        // 计算长度地址：array_ptr - 8
        let len_ptr_i8 = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 -8", len_ptr_i8, obj_i8));

        // 将长度指针转换为 i32*
        let len_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", len_ptr, len_ptr_i8));

        // 加载长度（作为 i32）
        let len_val = self.new_temp();
        self.emit_line(&format!("  {} = load i32, i32* {}, align 4", len_val, len_ptr));

        Ok(format!("i32 {}", len_val))
    }

    /// 生成成员访问表达式代码
    ///
    /// # Arguments
    /// * `member` - 成员访问表达式
    pub fn generate_member_access(&mut self, member: &MemberAccessExpr) -> cayResult<String> {
        // 检查是否是类名.静态方法访问: ClassName.methodName
        if let Expr::Identifier(class_name) = &*member.object {
            // 首先检查是否是静态方法访问（返回函数指针）
            if let Some(ref registry) = self.type_registry {
                if let Some(class_info) = registry.get_class(class_name.as_ref()) {
                    if let Some(methods) = class_info.methods.get(&member.member) {
                        // 查找静态方法
                        if let Some(method_info) = methods.iter().find(|m| m.is_static) {
                            // 生成函数指针（函数地址）
                            // 使用 build_function_name_from_method 生成正确的函数名
                            let func_name = self.build_function_name_from_method(
                                class_name.as_ref(), 
                                &member.member, 
                                &method_info.params, 
                                false
                            );
                            // 返回函数指针类型
                            let func_type = crate::types::Type::Function(Box::new(crate::types::FunctionType {
                                params: method_info.params.iter()
                                    .filter(|p| !p.is_varargs)
                                    .map(|p| p.param_type.clone())
                                    .collect(),
                                return_type: Box::new(method_info.return_type.clone()),
                                is_static: true,
                            }));
                            let llvm_func_type = self.type_to_llvm(&func_type);
                            return Ok(format!("{} @{}", llvm_func_type, func_name));
                        }
                    }
                }
            }
            
            // 检查是否是静态字段访问: ClassName.fieldName
            let static_key = format!("{}.{}", class_name, member.member);
            if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                // 检查是否是数组类型
                let is_array = matches!(field_info.field_type, crate::types::Type::Array(_));
                
                if is_array {
                    // 静态数组字段 - 直接从全局变量加载数组指针
                    // field_info.llvm_type 是元素类型指针（如 i32*）
                    // 静态字段存储这个指针值
                    let arr_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = load {}, {}* {}, align {}",
                        arr_ptr, field_info.llvm_type, field_info.llvm_type, field_info.name,
                        self.get_type_align(&field_info.llvm_type)));
                    return Ok(format!("{} {}", field_info.llvm_type, arr_ptr));
                } else {
                    // 普通静态字段访问 - 返回全局变量的值
                    let temp = self.new_temp();
                    self.emit_line(&format!("  {} = load {}, {}* {}, align {}",
                        temp, field_info.llvm_type, field_info.llvm_type, field_info.name,
                        self.get_type_align(&field_info.llvm_type)));
                    return Ok(format!("{} {}", field_info.llvm_type, temp));
                }
            }
        }

        // 处理实例字段访问: this.fieldName 或 obj.fieldName 或 super.fieldName
        // 也支持嵌套成员访问: obj.field1.field2
        
        // 确定对象所属的类
        let class_name_opt: Option<String> = if let Expr::Identifier(name) = &*member.object {
            let name_str = name.as_ref();
            if name_str == "this" {
                Some(self.current_class.clone())
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
                _ => None
            }
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
            if let Some(field_info) = self.get_instance_field(&class_name, &member.member).cloned() {
                // 实例字段访问
                
                // 获取对象指针
                // 对于 this 和 super，从作用域管理器获取 this 的 LLVM 名称；对于其他变量，加载其值
                let obj_ptr = if let Expr::Identifier(name) = &*member.object {
                    if name == "this" || name == "super" {
                        // 从作用域管理器获取 this 的 LLVM 名称，然后加载其值
                        // super 也使用 this 指针，只是访问的类不同
                        let this_llvm_name = self.scope_manager.get_llvm_name("this")
                            .unwrap_or_else(|| "this_s1".to_string());
                        let temp = self.new_temp();
                        self.emit_line(&format!("  {} = load i8*, i8** %{}, align 8", 
                            temp, this_llvm_name));
                        temp
                    } else {
                        // 其他变量：生成表达式并提取值
                        let obj = self.generate_expression(&member.object)?;
                        let (_, obj_val) = self.parse_typed_value(&obj);
                        obj_val
                    }
                } else {
                    let obj = self.generate_expression(&member.object)?;
                    let (_, obj_val) = self.parse_typed_value(&obj);
                    obj_val
                };
                
                // 计算字段地址: obj_ptr + offset
                let field_ptr_i8 = self.new_temp();
                self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 {}", 
                    field_ptr_i8, obj_ptr, field_info.offset));
                
                // 将字段指针转换为正确类型的指针
                // 注意：如果llvm_type已经是指针类型（如i8**），则不需要再加*
                let field_ptr = self.new_temp();
                let ptr_type = if field_info.llvm_type.ends_with('*') {
                    field_info.llvm_type.clone()
                } else {
                    format!("{}*", field_info.llvm_type)
                };
                self.emit_line(&format!("  {} = bitcast i8* {} to {}", 
                    field_ptr, field_ptr_i8, ptr_type));
                
                // 加载字段值
                let field_val = self.new_temp();
                self.emit_line(&format!("  {} = load {}, {} {}, align {}", 
                    field_val, field_info.llvm_type, ptr_type, field_ptr,
                    self.get_type_align(&field_info.llvm_type)));
                
                return Ok(format!("{} {}", field_info.llvm_type, field_val));
            }
        }
        
        // 特殊处理 super 标识符 - 返回 this 指针
        if let Expr::Identifier(name) = &*member.object {
            if name == "super" {
                // super 访问使用 this 指针
                if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                    let temp = self.new_temp();
                    self.emit_line(&format!("  {} = load i8*, i8** %{}, align 8", 
                        temp, this_llvm_name));
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
    fn generate_member_access_with_class_info(&mut self, member: &MemberAccessExpr) -> cayResult<(String, Option<String>)> {
        // 确定对象所属的类
        let class_name_opt: Option<String> = if let Expr::Identifier(name) = &*member.object {
            let name_str = name.as_ref();
            if name_str == "this" {
                Some(self.current_class.clone())
            } else if name_str == "super" {
                self.get_parent_class(&self.current_class)
            } else {
                self.var_class_map.get(name_str).cloned()
            }
        } else if let Expr::MemberAccess(nested_member) = &*member.object {
            // 递归处理嵌套成员访问
            match self.generate_member_access_with_class_info(nested_member) {
                Ok((_, Some(class_name))) => Some(class_name),
                _ => None
            }
        } else {
            None
        };

        if let Some(ref class_name) = class_name_opt {
            if let Some(field_info) = self.get_instance_field(class_name, &member.member).cloned() {
                // 获取对象指针
                let obj_ptr = if let Expr::Identifier(name) = &*member.object {
                    if name == "this" || name == "super" {
                        let this_llvm_name = self.scope_manager.get_llvm_name("this")
                            .unwrap_or_else(|| "this_s1".to_string());
                        let temp = self.new_temp();
                        self.emit_line(&format!("  {} = load i8*, i8** %{}, align 8", 
                            temp, this_llvm_name));
                        temp
                    } else {
                        let obj = self.generate_expression(&member.object)?;
                        let (_, obj_val) = self.parse_typed_value(&obj);
                        obj_val
                    }
                } else {
                    // 对于嵌套成员访问，递归生成对象表达式
                    let obj = self.generate_expression(&member.object)?;
                    let (_, obj_val) = self.parse_typed_value(&obj);
                    obj_val
                };
                
                // 计算字段地址
                let field_ptr_i8 = self.new_temp();
                self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 {}", 
                    field_ptr_i8, obj_ptr, field_info.offset));
                
                // 将字段指针转换为正确类型的指针
                // 注意：如果llvm_type已经是指针类型（如i8**），则不需要再加*
                let field_ptr = self.new_temp();
                let ptr_type = if field_info.llvm_type.ends_with('*') {
                    field_info.llvm_type.clone()
                } else {
                    format!("{}*", field_info.llvm_type)
                };
                self.emit_line(&format!("  {} = bitcast i8* {} to {}", 
                    field_ptr, field_ptr_i8, ptr_type));
                
                // 加载字段值
                let field_val = self.new_temp();
                self.emit_line(&format!("  {} = load {}, {} {}, align {}", 
                    field_val, field_info.llvm_type, ptr_type, field_ptr,
                    self.get_type_align(&field_info.llvm_type)));
                
                // 从字段类型推断类名
                let result_class_name = if let crate::types::Type::Object(ref inner_class) = field_info.field_type {
                    Some(inner_class.clone())
                } else {
                    None
                };
                
                return Ok((format!("{} {}", field_info.llvm_type, field_val), result_class_name));
            }
        }

        // 无法处理，回退到普通表达式生成
        let result = self.generate_expression(&Expr::MemberAccess(member.clone()))?;
        Ok((result, None))
    }
}
