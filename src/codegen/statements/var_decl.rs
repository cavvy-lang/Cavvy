//! 变量声明代码生成
//!
//! 处理变量声明和初始化的代码生成。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::types::Type;
use crate::error::cayResult;

impl IRGenerator {
    /// 从表达式推断类型
    fn infer_type_from_expr(&self, expr: &Expr) -> Option<Type> {
        match expr {
            Expr::Literal(lit) => match lit {
                LiteralValue::Int32(_) => Some(Type::Int32),
                LiteralValue::Int64(_) => Some(Type::Int64),
                LiteralValue::Float32(_) => Some(Type::Float32),
                LiteralValue::Float64(_) => Some(Type::Float64),
                LiteralValue::String(_) => Some(Type::String),
                LiteralValue::Bool(_) => Some(Type::Bool),
                LiteralValue::Char(_) => Some(Type::Char),
                LiteralValue::Null => Some(Type::Object("Object".to_string())),
            },
            Expr::Identifier(name) => {
                // 从变量类型映射中查找
                self.var_types.get(name.as_ref()).and_then(|llvm_type| {
                    self.llvm_type_to_cay_type(llvm_type)
                })
            },
            Expr::Binary(bin) => {
                // 对于二元表达式，尝试推断结果类型
                self.infer_type_from_expr(&bin.left)
            },
            Expr::Unary(unary) => {
                self.infer_type_from_expr(&unary.operand)
            },
            Expr::Call(call) => {
                // 对于函数调用，尝试从类型注册表获取返回类型
                self.infer_call_return_type(call)
            },
            Expr::MemberAccess(member) => {
                // 对于方法调用如 obj.method()，尝试推断返回类型
                if let Expr::Identifier(obj_name) = &*member.object {
                    // 获取对象类型
                    if let Some(class_name) = self.var_class_map.get(obj_name.as_ref()) {
                        return self.infer_method_return_type(class_name, &member.member);
                    }
                }
                None
            }
            Expr::New(new_expr) => {
                // new 表达式返回对象类型
                Some(Type::Object(new_expr.class_name.clone()))
            }
            _ => None, // 无法推断，返回 None
        }
    }

    /// 推断函数调用的返回类型
    pub fn infer_call_return_type(&self, call: &CallExpr) -> Option<Type> {
        // 处理内置函数
        if let Expr::Identifier(name) = call.callee.as_ref() {
            match name.as_str() {
                "print" | "println" => return Some(Type::Void),
                "readInt" => return Some(Type::Int32),
                "readLong" => return Some(Type::Int64),
                "readFloat" => return Some(Type::Float32),
                "readDouble" => return Some(Type::Float64),
                "readLine" => return Some(Type::String),
                "readChar" => return Some(Type::Char),
                "readBool" => return Some(Type::Bool),
                _ => {}
            }
        }

        // 尝试从类型注册表获取
        if let Some(ref registry) = self.type_registry {
            if let Expr::Identifier(name) = call.callee.as_ref() {
                // 尝试在当前类中查找
                if !self.current_class.is_empty() {
                    if let Some(method_info) = registry.get_method(&self.current_class, name.as_ref()) {
                        return Some(method_info.return_type.clone());
                    }
                }
            } else if let Expr::MemberAccess(member) = call.callee.as_ref() {
                // obj.method() 形式
                if let Expr::Identifier(obj_name) = &*member.object {
                    let obj_name_str = obj_name.as_ref();
                    // 首先检查是否是已知的类名（静态方法调用）
                    if registry.class_exists(obj_name_str) {
                        // 类名.方法名() 形式，如 Vector2.right()
                        if let Some(method_info) = registry.get_method(obj_name_str, &member.member) {
                            return Some(method_info.return_type.clone());
                        }
                    }
                    // 否则尝试从变量映射获取
                    if let Some(class_name) = self.var_class_map.get(obj_name_str) {
                        if let Some(method_info) = registry.get_method(class_name, &member.member) {
                            return Some(method_info.return_type.clone());
                        }
                    }
                    // 检查是否是 String 类型变量
                    if let Some(var_cay_type) = self.var_cay_types.get(obj_name_str) {
                        if let crate::types::Type::String = var_cay_type {
                            // String 类型特殊处理
                            if member.member == "length" || member.member == "indexOf" || 
                               member.member == "lastIndexOf" {
                                return Some(crate::types::Type::Int32);
                            } else if member.member == "substring" || 
                                      member.member == "toString" || member.member == "replace" {
                                return Some(crate::types::Type::String);
                            } else if member.member == "equals" || member.member == "isEmpty" ||
                                      member.member == "startsWith" || member.member == "endsWith" ||
                                      member.member == "contains" {
                                return Some(crate::types::Type::Bool);
                            } else if member.member == "charAt" {
                                return Some(crate::types::Type::Char);
                            }
                        }
                    }
                } else {
                    // 处理链式调用：obj 不是 Identifier，递归推断其类型
                    if let Some(obj_type) = self.get_expression_type(&member.object) {
                        if let crate::types::Type::Object(class_name) = obj_type {
                            if let Some(method_info) = registry.get_method(&class_name, &member.member) {
                                return Some(method_info.return_type.clone());
                            }
                        } else if let crate::types::Type::String = obj_type {
                            // String 类型特殊处理
                            if member.member == "length" || member.member == "indexOf" || 
                               member.member == "lastIndexOf" {
                                return Some(crate::types::Type::Int32);
                            } else if member.member == "substring" || 
                                      member.member == "toString" || member.member == "replace" {
                                return Some(crate::types::Type::String);
                            } else if member.member == "equals" || member.member == "isEmpty" ||
                                      member.member == "startsWith" || member.member == "endsWith" ||
                                      member.member == "contains" {
                                return Some(crate::types::Type::Bool);
                            } else if member.member == "charAt" {
                                return Some(crate::types::Type::Char);
                            }
                        }
                    }
                }
            }
        }

        // 无法推断
        None
    }

    /// 推断方法的返回类型
    fn infer_method_return_type(&self, class_name: &str, method_name: &str) -> Option<Type> {
        if let Some(ref registry) = self.type_registry {
            if let Some(method_info) = registry.get_method(class_name, method_name) {
                return Some(method_info.return_type.clone());
            }
        }
        None
    }

    /// 将 LLVM 类型转换为 Cayvy 类型
    fn llvm_type_to_cay_type(&self, llvm_type: &str) -> Option<Type> {
        match llvm_type {
            "i32" => Some(Type::Int32),
            "i64" => Some(Type::Int64),
            "float" => Some(Type::Float32),
            "double" => Some(Type::Float64),
            "i1" => Some(Type::Bool),
            "i8" => Some(Type::Char),
            "i8*" => Some(Type::String),
            "void" => Some(Type::Void),
            _ => {
                // 检查是否是对象指针类型
                if llvm_type.starts_with("%") && llvm_type.ends_with("*") {
                    let class_name = llvm_type.trim_start_matches('%').trim_end_matches('*');
                    Some(Type::Object(class_name.to_string()))
                } else {
                    None
                }
            }
        }
    }

    /// 生成变量声明代码
    pub fn generate_var_decl(&mut self, var: &VarDecl) -> cayResult<()> {
        // 处理 auto 类型推断
        let actual_type = if var.var_type == Type::Auto {
            // 从初始化器推断类型
            if let Some(init) = &var.initializer {
                self.infer_type_from_expr(init).unwrap_or(Type::Int32)
            } else {
                return Err(crate::error::semantic_error(
                    var.loc.line, var.loc.column,
                    "'auto' variable declaration requires an initializer".to_string()
                ));
            }
        } else {
            var.var_type.clone()
        };

        let var_type = self.type_to_llvm(&actual_type);
        let align = self.get_type_align(&var_type);  // 获取对齐

        // 使用作用域管理器生成唯一的 LLVM 变量名
        let llvm_name = self.scope_manager.declare_var(&var.name, &var_type);

        self.emit_line(&format!("  %{} = alloca {}, align {}", llvm_name, var_type, align));
        // 同时存储到旧系统以保持兼容性
        self.var_types.insert(var.name.clone(), var_type.clone());
        // 存储Cavvy类型信息，用于准确的类型推断
        self.var_cay_types.insert(var.name.clone(), actual_type.clone());
        // 如果变量类型是对象，记录其类名以便后续方法调用解析
        if let Type::Object(class_name) = &actual_type {
            self.var_class_map.insert(var.name.clone(), class_name.clone());
        }

        if let Some(init) = var.initializer.as_ref() {
            // 特殊处理数组初始化，传递目标类型信息
            if let Expr::ArrayInit(array_init) = init {
                let value = self.generate_array_init_with_type(array_init, &actual_type)?;
                self.emit_line(&format!("  store {}, {}* %{}",
                    value, var_type, llvm_name));
            } else {
                let value = self.generate_expression(init)?;
                let (value_type, val) = self.parse_typed_value(&value);

                // 如果值类型与变量类型不匹配，需要转换
                if value_type != var_type {
                    let temp = self.new_temp();

                    // null 赋值给指针类型（int 0 转换为指针）- 必须最先检查
                    if (val == "0" || val == "null") && var_type.ends_with("*") {
                        // null 可以直接存储到指针类型
                        self.emit_line(&format!("  store {} null, {}* %{}, align {}", var_type, var_type, llvm_name, align));
                    }
                    // 浮点类型转换
                    else if value_type == "double" && var_type == "float" {
                        // double -> float 转换
                        self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                        let align = self.get_type_align("float");
                        self.emit_line(&format!("  store float {}, float* %{}, align {}", temp, llvm_name, align));
                    } else if value_type == "float" && var_type == "double" {
                        // float -> double 转换
                        self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                        let align = self.get_type_align("double");
                        self.emit_line(&format!("  store double {}, double* %{}, align {}", temp, llvm_name, align));
                    }
                    // 指针类型转换 (bitcast)
                    else if value_type.ends_with("*") && var_type.ends_with("*") {
                        self.emit_line(&format!("  {} = bitcast {} {} to {}",
                            temp, value_type, val, var_type));
                        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                    }
                    // 整数类型转换
                    else if value_type.starts_with("i") && var_type.starts_with("i") && !value_type.ends_with("*") && !var_type.ends_with("*") {
                        let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                        let to_bits: u32 = var_type.trim_start_matches('i').parse().unwrap_or(64);

                        if to_bits > from_bits {
                            // 符号扩展
                            self.emit_line(&format!("  {} = sext {} {} to {}",
                                temp, value_type, val, var_type));
                        } else {
                            // 截断
                            self.emit_line(&format!("  {} = trunc {} {} to {}",
                                temp, value_type, val, var_type));
                        }
                        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                    }
                    // 整数到浮点数转换
                    else if value_type.starts_with("i") && !value_type.ends_with("*")
                        && (var_type == "float" || var_type == "double") {
                        self.emit_line(&format!("  {} = sitofp {} {} to {}",
                            temp, value_type, val, var_type));
                        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                    }
                    // 浮点数到整数转换
                    else if (value_type == "float" || value_type == "double") && var_type.starts_with("i") {
                        self.emit_line(&format!("  {} = fptosi {} {} to {}",
                            temp, value_type, val, var_type));
                        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                    }
                    // 指针到整数转换 (ptrtoint)
                    else if value_type.ends_with("*") && var_type.starts_with("i") && !var_type.ends_with("*") {
                        self.emit_line(&format!("  {} = ptrtoint {} {} to {}",
                            temp, value_type, val, var_type));
                        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                    }
                    // 整数到指针转换 (inttoptr)
                    else if var_type.ends_with("*") && value_type.starts_with("i") && !value_type.ends_with("*") {
                        // LLVM 不支持 void*，使用 i8* 代替
                        let llvm_var_type: &str = if var_type == "void*" { "i8*" } else { &var_type };
                        self.emit_line(&format!("  {} = inttoptr {} {} to {}",
                            temp, value_type, val, llvm_var_type));
                        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                    }
                    else {
                        // 类型不兼容，报错
                        return Err(crate::error::codegen_error(
                            format!("Cannot convert {} to {} in variable initialization '{}' at line {}", 
                                value_type, var_type, var.name, var.loc.line)
                        ));
                    }
                } else {
                    // 类型匹配，直接存储
                    self.emit_line(&format!("  store {}, {}* %{}",
                        value, var_type, llvm_name));
                }
            }
        }

        Ok(())
    }
}
