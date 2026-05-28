//! 表达式代码生成工具函数
//!
//! 提供类型提升、左值信息获取等通用工具函数。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error_at};

impl IRGenerator {
    /// 提升整数操作数到相同类型
    ///
    /// # Arguments
    /// * `left_type` - 左操作数类型
    /// * `left_val` - 左操作数值
    /// * `right_type` - 右操作数类型
    /// * `right_val` - 右操作数值
    ///
    /// # Returns
    /// (目标类型, 提升后的左值, 提升后的右值)
    pub fn promote_integer_operands(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str) -> (String, String, String) {
        // 检查是否为指针类型（如 i8*），指针类型不参与整数提升
        let left_is_ptr = left_type.ends_with('*');
        let right_is_ptr = right_type.ends_with('*');
        
        if left_is_ptr || right_is_ptr {
            // 指针类型不应该调用此函数，返回原值以避免错误
            return (left_type.to_string(), left_val.to_string(), right_val.to_string());
        }
        
        // 如果类型相同，直接返回
        if left_type == right_type {
            return (left_type.to_string(), left_val.to_string(), right_val.to_string());
        }
        
        // 确定提升后的类型（选择位数更大的类型）
        let left_bits: u32 = left_type.trim_start_matches('i').parse().unwrap_or(64);
        let right_bits: u32 = right_type.trim_start_matches('i').parse().unwrap_or(64);
        
        // char (i8) 类型在算术运算中需要提升，但如果另一个操作数是 i64，则提升到 i64
        let target_type = if left_bits >= right_bits { left_type } else { right_type };
        
        // 提升左操作数
        let promoted_left = if left_type != target_type {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to {}", temp, left_type, left_val, target_type));
            temp
        } else {
            left_val.to_string()
        };
        
        // 提升右操作数
        let promoted_right = if right_type != target_type {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to {}", temp, right_type, right_val, target_type));
            temp
        } else {
            right_val.to_string()
        };
        
        (target_type.to_string(), promoted_left, promoted_right)
    }
    
    /// 提升浮点操作数到相同类型
    ///
    /// # Arguments
    /// * `left_type` - 左操作数类型
    /// * `left_val` - 左操作数值
    /// * `right_type` - 右操作数类型
    /// * `right_val` - 右操作数值
    ///
    /// # Returns
    /// (目标类型, 提升后的左值, 提升后的右值)
    pub fn promote_float_operands(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str) -> (String, String, String) {
        if left_type == right_type {
            return (left_type.to_string(), left_val.to_string(), right_val.to_string());
        }

        // 确定提升后的类型（选择精度更高的类型：double > float）
        if left_type == "double" || right_type == "double" {
            let promoted_type = "double".to_string();
            let mut promoted_left = left_val.to_string();
            let mut promoted_right = right_val.to_string();

            if left_type == "float" {
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = fpext float {} to double", temp, left_val));
                promoted_left = temp;
            }

            if right_type == "float" {
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = fpext float {} to double", temp, right_val));
                promoted_right = temp;
            }

            (promoted_type, promoted_left, promoted_right)
        } else {
            // 两者都是float，无需提升
            (left_type.to_string(), left_val.to_string(), right_val.to_string())
        }
    }

    /// 处理整数和浮点数的混合运算，将整数转换为浮点数
    ///
    /// # Arguments
    /// * `left_type` - 左操作数类型
    /// * `left_val` - 左操作数值
    /// * `right_type` - 右操作数类型
    /// * `right_val` - 右操作数值
    ///
    /// # Returns
    /// Some((目标类型, 提升后的左值, 提升后的右值)) 如果是混合类型
    /// None 如果不是混合类型
    pub fn promote_mixed_operands(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str) -> Option<(String, String, String)> {
        // 检查是否是混合类型（整数 + 浮点数）
        let left_is_int = left_type.starts_with("i") && !left_type.ends_with("*");
        let right_is_int = right_type.starts_with("i") && !right_type.ends_with("*");
        let left_is_float = left_type == "float" || left_type == "double";
        let right_is_float = right_type == "float" || right_type == "double";

        if left_is_int && right_is_float {
            // 整数 + 浮点数：将整数转换为浮点数
            let promoted_type = if right_type == "double" { "double" } else { "float" };
            let converted_left = self.new_temp();
            if promoted_type == "double" {
                self.emit_line(&format!("  {} = sitofp {} {} to double", converted_left, left_type, left_val));
            } else {
                self.emit_line(&format!("  {} = sitofp {} {} to float", converted_left, left_type, left_val));
            }
            Some((promoted_type.to_string(), converted_left, right_val.to_string()))
        } else if left_is_float && right_is_int {
            // 浮点数 + 整数：将整数转换为浮点数
            let promoted_type = if left_type == "double" { "double" } else { "float" };
            let converted_right = self.new_temp();
            if promoted_type == "double" {
                self.emit_line(&format!("  {} = sitofp {} {} to double", converted_right, right_type, right_val));
            } else {
                self.emit_line(&format!("  {} = sitofp {} {} to float", converted_right, right_type, right_val));
            }
            Some((promoted_type.to_string(), left_val.to_string(), converted_right))
        } else {
            None
        }
    }

    /// 获取左值的类型和 LLVM 指针表示
    ///
    /// # Arguments
    /// * `expr` - 表达式
    ///
    /// # Returns
    /// (类型字符串, 指针字符串, 是否是参数)
    pub fn get_lvalue_info_with_param_flag(&mut self, expr: &Expr) -> cayResult<(String, String, bool)> {
        match expr {
            Expr::Identifier(name) => {
                let name_str = name.as_ref();
                // 优先使用作用域管理器获取变量类型
                let (var_type, llvm_name, is_param) = if let Some(scope_type) = self.scope_manager.get_var_type(name_str) {
                    let llvm_name = self.scope_manager.get_llvm_name(name_str).unwrap_or_else(|| name_str.to_string());
                    let is_param = self.scope_manager.is_parameter(name_str);
                    (scope_type, llvm_name, is_param)
                } else {
                    // 检查是否是当前类的静态字段
                    if !self.current_class.is_empty() {
                        let static_key = format!("{}.{}", self.current_class, name_str);
                        if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                            return Ok((field_info.llvm_type, field_info.name, false));
                        }
                    }
                    // 回退到旧系统
                    let var_type = self.var_types.get(name_str)
                        .ok_or_else(|| codegen_error_at(expr.location().clone(), format!("Variable '{}' not found", name_str)))?
                        .clone();
                    (var_type, name_str.to_string(), false)
                };
                Ok((var_type, format!("%{}", llvm_name), is_param))
            }
            Expr::ArrayAccess(arr) => {
                let (elem_type, elem_ptr, _) = self.get_array_element_ptr(arr)?;
                Ok((elem_type, elem_ptr, false))
            }
            Expr::MemberAccess(member) => {
                // 处理实例字段作为左值（如 this.sp）
                let (ty, ptr) = self.get_member_field_pointer(member)?;
                Ok((ty, ptr, false))
            }
            _ => Err(codegen_error_at(expr.location().clone(), "Invalid lvalue expression".to_string()))
        }
    }
    
    /// 获取左值信息（向后兼容版本）
    ///
    /// # Returns
    /// (类型字符串, 指针字符串)
    pub fn get_lvalue_info(&mut self, expr: &Expr) -> cayResult<(String, String)> {
        let (ty, ptr, _) = self.get_lvalue_info_with_param_flag(expr)?;
        Ok((ty, ptr))
    }

    /// 生成运行时除零检查代码
    ///
    /// # Arguments
    /// * `val_type` - 除数类型
    /// * `val` - 除数值
    pub fn generate_division_by_zero_check(&mut self, val_type: &str, val: &str) -> cayResult<()> {
        // 创建标签
        let error_label = self.new_label("div.error");
        let continue_label = self.new_label("div.cont");

        // 检查除数是否为零
        let is_zero = self.new_temp();
        self.emit_line(&format!("  {} = icmp eq {} {}, 0", is_zero, val_type, val));
        self.emit_line(&format!("  br i1 {}, label %{}, label %{}", is_zero, error_label, continue_label));

        // 错误处理块
        self.emit_line(&format!("{}:", error_label));
        // 输出错误信息到 stderr
        let error_msg = self.get_or_create_string_constant("Error: Division by zero\n");
        self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {})", error_msg));
        // 调用 exit 退出程序
        self.emit_line("  call void @exit(i32 1)");
        self.emit_line("  unreachable");

        // 正常继续块
        self.emit_line(&format!("{}:", continue_label));

        Ok(())
    }

    /// 将 LLVM 类型转换为方法签名
    pub fn llvm_type_to_signature(&self, llvm_type: &str) -> String {
        match llvm_type {
            "i32" => "i".to_string(),
            "i64" => "l".to_string(),
            "float" => "f".to_string(),
            "double" => "d".to_string(),
            "i1" => "b".to_string(),
            "i8*" => "s".to_string(),
            "i8" => "c".to_string(),
            t if t.ends_with("*") => "o".to_string(), // 对象/数组指针
            _ => "x".to_string(), // 未知类型
        }
    }

    /// 将 LLVM 类型转换为方法签名（支持可变参数数组类型）
    pub fn llvm_type_to_signature_with_varargs(&self, llvm_type: &str, is_varargs_array: bool) -> String {
        if is_varargs_array {
            // 可变参数数组使用 ai 签名（array of int）
            "ai".to_string()
        } else {
            self.llvm_type_to_signature(llvm_type)
        }
    }

    /// 获取成员字段的指针（用于左值）
    ///
    /// # Arguments
    /// * `member` - 成员访问表达式（如 this.sp）
    ///
    /// # Returns
    /// (LLVM类型字符串, 指针字符串)
    pub fn get_member_field_pointer(&mut self, member: &MemberAccessExpr) -> cayResult<(String, String)> {
        // 确定对象所属的类
        let class_name_opt: Option<String> = if let Expr::Identifier(name) = member.object.as_ref() {
            let name_str = name.as_ref();
            if name_str == "this" {
                Some(self.current_class.clone())
            } else {
                // 首先检查是否是变量对应的类
                let var_class = self.var_class_map.get(name_str).cloned();
                if var_class.is_some() {
                    var_class
                } else {
                    // 检查是否是直接的类名（用于静态字段访问如 Fibonacci.memo）
                    // 通过检查 static_field_map 中是否有该类的静态字段来确定
                    let static_key_prefix = format!("{}.", name_str);
                    let has_static_fields = self.static_field_map.keys().any(|k| k.starts_with(&static_key_prefix));
                    if has_static_fields {
                        Some(name_str.to_string())
                    } else {
                        None
                    }
                }
            }
        } else {
            None
        };

        if let Some(class_name) = class_name_opt {
            // 首先尝试获取实例字段
            if let Some(field_info) = self.get_instance_field(&class_name, &member.member).cloned() {
                // 获取对象指针
                let obj_ptr = if let Expr::Identifier(name) = member.object.as_ref() {
                    let name_str = name.as_ref();
                    if name_str == "this" {
                        "%this".to_string()
                    } else {
                        let obj = self.generate_expression(member.object.as_ref())?;
                        let (_, obj_val) = self.parse_typed_value(&obj);
                        obj_val
                    }
                } else {
                    let obj = self.generate_expression(member.object.as_ref())?;
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

                // 返回字段类型和指针
                return Ok((field_info.llvm_type, field_ptr));
            }
            
            // 尝试获取静态字段
            let static_key = format!("{}.{}", class_name, member.member);
            if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                // 静态字段使用全局变量名（包含 _s 后缀）
                let llvm_type = field_info.llvm_type.clone();
                
                // 对于数组类型的静态字段，需要加载指针值
                // 因为静态数组字段存储的是指向数组数据的指针
                let is_array = matches!(field_info.field_type, crate::types::Type::Array(_));
                if is_array {
                    // 加载数组指针
                    let ptr_temp = self.new_temp();
                    self.emit_line(&format!("  {} = load {}, {}* {}, align 8",
                        ptr_temp, llvm_type, llvm_type, field_info.name));
                    return Ok((llvm_type.clone(), ptr_temp));
                } else {
                    // 非数组类型，返回全局变量地址
                    return Ok((llvm_type.clone(), field_info.name.clone()));
                }
            }
        }

        Err(codegen_error_at(member.loc.clone(), format!("Cannot get field pointer for member access: {}", member.member)))
    }

    /// 获取嵌套成员字段的指针（用于链式成员访问赋值，如 obj.field1.field2）
    ///
    /// # Arguments
    /// * `member` - 成员访问表达式（如 s.returnStmt.value）
    ///
    /// # Returns
    /// (LLVM类型字符串, 指针字符串)
    pub fn get_nested_field_pointer(&mut self, member: &MemberAccessExpr) -> cayResult<(String, String)> {
        // 递归处理链式成员访问
        self.get_nested_field_pointer_recursive(member, true)
    }

    /// 递归获取嵌套成员字段的指针
    ///
    /// # Arguments
    /// * `member` - 成员访问表达式
    /// * `is_root` - 是否为最顶层调用（用于确定是否需要加载对象指针）
    fn get_nested_field_pointer_recursive(&mut self, member: &MemberAccessExpr, is_root: bool) -> cayResult<(String, String)> {
        // 获取对象指针和类型
        let (obj_ptr, obj_class_name) = match member.object.as_ref() {
            Expr::Identifier(name) => {
                let name_str = name.as_ref();
                if name_str == "this" {
                    // this 指针直接使用
                    let ptr = if is_root {
                        // 需要加载 this 指针
                        let this_llvm_name = self.scope_manager.get_llvm_name("this")
                            .unwrap_or_else(|| "this_s1".to_string());
                        let temp = self.new_temp();
                        self.emit_line(&format!("  {} = load i8*, i8** %{}, align 8", temp, this_llvm_name));
                        temp
                    } else {
                        // 嵌套情况下直接使用 %this
                        "%this".to_string()
                    };
                    let qualified = self.resolve_current_qualified_class();
                    (ptr, Some(qualified))
                } else {
                    // 普通变量 - 总是需要加载变量值作为对象指针
                    let class_name = self.var_class_map.get(name_str).cloned();
                    let obj = self.generate_expression(member.object.as_ref())?;
                    let (_, obj_val) = self.parse_typed_value(&obj);
                    (obj_val, class_name)
                }
            }
            Expr::MemberAccess(nested_member) => {
                // 递归处理嵌套成员访问 - 这里的 is_root 应该为 true，因为我们需要加载最外层对象的值
                // 同时获取字段信息以确定对象类型
                let (nested_type, nested_ptr, field_info_opt) = self.get_nested_field_pointer_recursive_with_info(nested_member, true)?;
                
                // 从嵌套字段加载对象指针
                let obj_ptr = self.new_temp();
                self.emit_line(&format!("  {} = load {}, {}* {}, align {}",
                    obj_ptr, nested_type, nested_type, nested_ptr, self.get_type_align(&nested_type)));
                
                // 从字段信息中提取类名
                let class_name = if let Some(ref field_info) = field_info_opt {
                    if let crate::types::Type::Object(class_name) = &field_info.field_type {
                        Some(class_name.clone())
                    } else {
                        None
                    }
                } else {
                    None
                };
                
                (obj_ptr, class_name)
            }
            _ => {
                // 其他表达式类型，尝试直接生成
                let obj = self.generate_expression(member.object.as_ref())?;
                let (_, obj_val) = self.parse_typed_value(&obj);
                (obj_val, None)
            }
        };

        // 获取字段信息
        if let Some(ref class_name) = obj_class_name {
            if let Some(field_info) = self.get_instance_field(class_name, &member.member).cloned() {
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

                // 返回字段类型和指针
                return Ok((field_info.llvm_type, field_ptr));
            }
        }

        Err(codegen_error_at(member.loc.clone(), format!(
            "Cannot get nested field pointer for member access: {} (object class: {:?})",
            member.member, obj_class_name
        )))
    }

    /// 递归获取嵌套成员字段的指针，同时返回字段信息
    ///
    /// # Arguments
    /// * `member` - 成员访问表达式
    /// * `is_root` - 是否为最顶层调用
    ///
    /// # Returns
    /// (LLVM类型字符串, 指针字符串, 字段信息Option)
    fn get_nested_field_pointer_recursive_with_info(&mut self, member: &MemberAccessExpr, is_root: bool) 
        -> cayResult<(String, String, Option<crate::codegen::context::InstanceFieldInfo>)> 
    {
        // 获取对象指针和类型
        let (obj_ptr, obj_class_name) = match member.object.as_ref() {
            Expr::Identifier(name) => {
                let name_str = name.as_ref();
                if name_str == "this" {
                    let ptr = if is_root {
                        let this_llvm_name = self.scope_manager.get_llvm_name("this")
                            .unwrap_or_else(|| "this_s1".to_string());
                        let temp = self.new_temp();
                        self.emit_line(&format!("  {} = load i8*, i8** %{}, align 8", temp, this_llvm_name));
                        temp
                    } else {
                        "%this".to_string()
                    };
                    let qualified = self.resolve_current_qualified_class();
                    (ptr, Some(qualified))
                } else {
                    let class_name = self.var_class_map.get(name_str).cloned();
                    let obj = self.generate_expression(member.object.as_ref())?;
                    let (_, obj_val) = self.parse_typed_value(&obj);
                    (obj_val, class_name)
                }
            }
            Expr::MemberAccess(nested_member) => {
                let (nested_type, nested_ptr, _) = self.get_nested_field_pointer_recursive_with_info(nested_member, true)?;
                
                let obj_ptr = self.new_temp();
                self.emit_line(&format!("  {} = load {}, {}* {}, align {}",
                    obj_ptr, nested_type, nested_type, nested_ptr, self.get_type_align(&nested_type)));
                
                let class_name = None;
                (obj_ptr, class_name)
            }
            _ => {
                let obj = self.generate_expression(member.object.as_ref())?;
                let (_, obj_val) = self.parse_typed_value(&obj);
                (obj_val, None)
            }
        };

        // 获取字段信息
        if let Some(ref class_name) = obj_class_name {
            if let Some(field_info) = self.get_instance_field(class_name, &member.member).cloned() {
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

                // 返回字段类型、指针和字段信息
                let field_info_clone = field_info.clone();
                return Ok((field_info.llvm_type, field_ptr, Some(field_info_clone)));
            }
        }

        Err(codegen_error_at(member.loc.clone(), format!(
            "Cannot get nested field pointer for member access: {} (object class: {:?})",
            member.member, obj_class_name
        )))
    }

}
