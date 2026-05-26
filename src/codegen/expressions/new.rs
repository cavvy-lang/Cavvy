//! new 表达式代码生成
//!
//! 处理对象创建和数组创建。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成 new 表达式代码
    ///
    /// # Arguments
    /// * `new_expr` - new 表达式
    pub fn generate_new_expression(&mut self, new_expr: &NewExpr) -> cayResult<String> {
        let class_name = &new_expr.class_name;
        // 如果类是命名空间限定的，解析到TypeRegistry中获取规范名称
        let canonical_name = if class_name.contains("::") {
            if let Some(ref registry) = self.type_registry {
                if let Some(class_info) = registry.get_class(class_name) {
                    class_info.name.clone()
                } else {
                    class_name.clone()
                }
            } else {
                class_name.clone()
            }
        } else {
            class_name.clone()
        };
        let type_id_value = self.get_type_id_value(&canonical_name).unwrap_or(0);

        // 获取类布局信息，确定对象大小
        let obj_size = self.get_class_layout(&canonical_name)
            .map(|layout| layout.total_size as i64)
            .unwrap_or(8i64); // 默认最小大小

        let calloc_temp = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @calloc(i64 1, i64 {})", calloc_temp, obj_size));

        // calloc 失败保护：返回 null 而非崩溃（跳过构造函数调用）
        let is_null_new = self.new_temp();
        self.emit_line(&format!("  {} = icmp eq i8* {}, null", is_null_new, calloc_temp));
        let new_ok = self.new_label("new.ok");
        let new_fail = self.new_label("new.fail");
        let new_merge = self.new_label("new.merge");

        // 结果槽（alloca 必须在 br 之前）
        let new_result_slot = self.new_temp();
        self.emit_line(&format!("  {} = alloca i8*", new_result_slot));

        self.emit_line(&format!("  br i1 {}, label %{}, label %{}", is_null_new, new_fail, new_ok));

        self.emit_line(&format!("\n{}:", new_fail));
        self.emit_line(&format!("  store i8* null, i8** {}", new_result_slot));
        self.emit_line(&format!("  br label %{}", new_merge));

        self.emit_line(&format!("\n{}:", new_ok));

        let type_id_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", type_id_ptr, calloc_temp));
        self.emit_line(&format!("  store i32 {}, i32* {}", type_id_value, type_id_ptr));

        // 调用构造函数（无论是否有参数）
        // 先推断参数类型（作为回退），优先使用类型注册表中的真实构造函数参数类型
        let fallback_types: Vec<String> = new_expr.args.iter()
            .map(|arg| self.infer_argument_type(arg))
            .collect();
        let param_types = self.get_constructor_param_signatures(
            &canonical_name,
            new_expr.args.len(),
            &fallback_types,
        );
        
        // 生成参数值
        let mut arg_values = Vec::new();
        for arg in &new_expr.args {
            let arg_val = self.generate_expression(arg)?;
            arg_values.push(arg_val);
        }
        
        // 生成构造函数名（使用类型注册表中的真实参数类型）
        let ctor_name = self.generate_constructor_call_name_with_types(&canonical_name, &param_types);
        
        // struct (值类型) 如果无参构造，跳过构造函数调用（struct 默认值初始化即可）
        let is_struct = self.type_registry.as_ref()
            .and_then(|r| r.get_struct(&canonical_name))
            .is_some();
        
        // 非 struct 或有参构造才调用构造函数
        if !is_struct || !param_types.is_empty() {
            // 生成参数列表
            let mut arg_strs = vec![format!("i8* {}", calloc_temp)];
            arg_strs.extend(arg_values);
            
            // 调用构造函数
            self.emit_line(&format!("  call void @{}({})",
                ctor_name, arg_strs.join(", ")));
        }

        let cast_temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i8*", cast_temp, calloc_temp));
        self.emit_line(&format!("  store i8* {}, i8** {}", cast_temp, new_result_slot));
        self.emit_line(&format!("  br label %{}", new_merge));

        self.emit_line(&format!("\n{}:", new_merge));
        let result_temp = self.new_temp();
        self.emit_line(&format!("  {} = load i8*, i8** {}", result_temp, new_result_slot));
        Ok(format!("i8* {}", result_temp))
    }
    
    /// 推断参数类型（返回类型签名）
    fn infer_argument_type(&self, expr: &Expr) -> String {
        match expr {
            Expr::Literal(lit_expr) => {
                match &lit_expr.value {
                    LiteralValue::Int32(_) => "i".to_string(),
                    LiteralValue::Int64(_) => "l".to_string(),
                    LiteralValue::Float32(_) => "f".to_string(),
                    LiteralValue::Float64(_) => "d".to_string(),
                    LiteralValue::Bool(_) => "b".to_string(),
                    LiteralValue::Char(_) => "c".to_string(),
                    LiteralValue::String(_) => "s".to_string(),
                    LiteralValue::Null => "o".to_string(),
                }
            }
            Expr::Identifier(ident) => {
                // 查找变量类型
                if let Some(cay_type) = self.var_cay_types.get(&ident.name) {
                    self.type_to_signature(cay_type)
                } else {
                    "i".to_string() // 默认int
                }
            }
            Expr::MemberAccess(member) => {
                // 尝试推断成员访问的类型
                if let Some(cay_type) = self.infer_member_access_type(member) {
                    self.type_to_signature(&cay_type)
                } else {
                    "i".to_string() // 默认int
                }
            }
            Expr::Binary(binary) => {
                // 二元表达式的类型通常是左操作数的类型
                self.infer_argument_type(&binary.left)
            }
            Expr::Unary(unary) => {
                self.infer_argument_type(&unary.operand)
            }
            Expr::Cast(cast) => {
                self.type_to_signature(&cast.target_type)
            }
            Expr::Call(call) => {
                // 尝试推断函数调用的返回类型
                if let Some(cay_type) = self.infer_call_return_type(call) {
                    self.type_to_signature(&cay_type)
                } else {
                    "i".to_string() // 默认int
                }
            }
            Expr::New(new_expr) => {
                // new 表达式返回对象类型
                format!("o{}", new_expr.class_name)
            }
            _ => "i".to_string(), // 默认int
        }
    }

    /// 推断成员访问表达式的类型
    fn infer_member_access_type(&self, member: &MemberAccessExpr) -> Option<crate::types::Type> {
        use crate::types::{Type, FunctionType};

        // 获取对象类型或类名
        let obj_type = self.infer_expr_type_for_member(&member.object)?;

        match obj_type {
            Type::Object(class_name) => {
                // 获取命名空间限定名（用于在 class_layouts 中查找）
                let qualified_name = {
                    let ns = self.get_class_namespace(&class_name);
                    if !ns.is_empty() {
                        format!("{}::{}", ns.join("::"), class_name)
                    } else {
                        class_name.clone()
                    }
                };
                // 首先查找类字段（先试简单名，再试命名空间限定名）
                let class_layout = self.class_layouts.get(&class_name)
                    .or_else(|| self.class_layouts.get(&qualified_name));
                if let Some(class_info) = class_layout {
                    if let Some(field) = class_info.fields.get(&member.member) {
                        return Some(field.field_type.clone());
                    }
                }
                // 然后查找静态方法（如 MathUtils.multiply）
                if let Some(ref registry) = self.type_registry {
                    let registry_class = registry.get_class(&class_name)
                        .or_else(|| registry.get_class(&qualified_name));
                    if let Some(class_info) = registry_class {
                        // 查找静态方法
                        for (method_name, methods) in &class_info.methods {
                            if method_name == &member.member {
                                for method in methods {
                                    if method.is_static {
                                        // 返回函数指针类型
                                        let param_types = method.params.iter()
                                            .map(|p| p.param_type.clone())
                                            .collect();
                                        return Some(Type::Function(Box::new(FunctionType {
                                            params: param_types,
                                            return_type: Box::new(method.return_type.clone()),
                                            is_static: true,
                                        })));
                                    }
                                }
                            }
                        }
                    }
                }
                None
            }
            Type::Array(_) if member.member == "length" => {
                // 数组的 length 属性返回 int
                Some(Type::Int32)
            }
            _ => None,
        }
    }

    /// 推断表达式类型（用于成员访问类型推断）
    fn infer_expr_type_for_member(&self, expr: &Expr) -> Option<crate::types::Type> {
        use crate::types::Type;

        match expr {
            Expr::Identifier(ident) => {
                // 首先检查是否是变量
                if let Some(var_type) = self.var_cay_types.get(&ident.name) {
                    return Some(var_type.clone());
                }
                // 特殊处理 "this"
                if ident.name == "this" {
                    return Some(Type::Object(self.current_class.clone()));
                }
                // 检查是否是类名（静态方法调用如 MathUtils.multiply）
                if let Some(ref registry) = self.type_registry {
                    if registry.class_exists(&ident.name) {
                        return Some(Type::Object(ident.name.clone()));
                    }
                }
                None
            }
            Expr::Literal(lit_expr) => {
                match &lit_expr.value {
                    LiteralValue::Int32(_) => Some(Type::Int32),
                    LiteralValue::Int64(_) => Some(Type::Int64),
                    LiteralValue::Float32(_) => Some(Type::Float32),
                    LiteralValue::Float64(_) => Some(Type::Float64),
                    LiteralValue::Bool(_) => Some(Type::Bool),
                    LiteralValue::Char(_) => Some(Type::Char),
                    LiteralValue::String(_) => Some(Type::String),
                    LiteralValue::Null => None,
                }
            }
            _ => None,
        }
    }
}
