//! 表达式类型推断

use crate::ast::*;
use crate::types::Type;
use crate::error::{cayResult, semantic_error, semantic_error_with_file};
use super::analyzer::SemanticAnalyzer;
use super::symbol_table::SemanticSymbolInfo;

/// 辅助函数：根据SourceLocation创建语义错误
fn semantic_error_at_loc(loc: &crate::error::SourceLocation, message: impl Into<String>) -> crate::error::cayError {
    semantic_error_with_file(loc.file.clone(), loc.line, loc.column, message)
}

impl SemanticAnalyzer {
    /// 推断表达式类型（带错误收集）
    /// 这个版本会收集错误到 self.errors 而不是直接返回 Err
    pub fn infer_expr_type_collect_errors(&mut self, expr: &Expr) -> Type {
        match self.infer_expr_type_internal(expr) {
            Ok(ty) => ty,
            Err(e) => {
                // 将错误转换为 SemanticErrorInfo 并收集
                if let Some((line, column)) = crate::error::get_error_location(&e) {
                    let message = crate::error::get_error_message(&e);
                    let file = crate::error::get_error_file(&e);
                    self.errors.push(self.create_error_info_with_file(file, line, column, message));
                }
                Type::Int32 // 返回默认类型继续分析
            }
        }
    }

    /// 推断表达式类型（内部实现）
    fn infer_expr_type_internal(&mut self, expr: &Expr) -> cayResult<Type> {
        match expr {
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::Int32(_) => Ok(Type::Int32),
                LiteralValue::Int64(_) => Ok(Type::Int64),
                LiteralValue::Float32(_) => Ok(Type::Float32),
                LiteralValue::Float64(_) => Ok(Type::Float64),
                LiteralValue::String(_) => Ok(Type::String),
                LiteralValue::Bool(_) => Ok(Type::Bool),
                LiteralValue::Char(_) => Ok(Type::Char),
                LiteralValue::Null => Ok(Type::Object("Object".to_string())),
            }
            Expr::Identifier(ident) => {
                let name = &ident.name;
                let loc = &ident.loc;

                // 处理 this 标识符
                if name == "this" {
                    // 检查是否在静态上下文中访问 this
                    if self.current_method_is_static {
                        return Err(semantic_error_at_loc(
                            loc,
                            "non-static variable this cannot be referenced from a static context".to_string()
                        ));
                    }
                    // 返回当前类类型
                    if let Some(current_class_name) = &self.current_class {
                        return Ok(Type::Object(current_class_name.clone()));
                    }
                    return Err(semantic_error_at_loc(
                        loc,
                        "this can only be used inside a class".to_string()
                    ));
                }

                // 处理 super 标识符
                if name == "super" {
                    // 检查是否在静态上下文中访问 super
                    if self.current_method_is_static {
                        return Err(semantic_error_at_loc(
                            loc,
                            "non-static variable super cannot be referenced from a static context".to_string()
                        ));
                    }
                    // 返回父类类型
                    if let Some(current_class_name) = &self.current_class {
                        if let Some(class_info) = self.type_registry.get_class(current_class_name) {
                            if let Some(parent_name) = &class_info.parent {
                                return Ok(Type::Object(parent_name.clone()));
                            }
                        }
                    }
                    return Err(semantic_error_at_loc(
                        loc,
                        "super can only be used in a class that extends another class".to_string()
                    ));
                }
                
                // 首先检查本地符号表（参数、局部变量优先于类字段）
                if let Some(info) = self.symbol_table.lookup(name) {
                    return Ok(info.symbol_type.clone());
                }
                
                // 检查是否是当前类的字段（包括静态和非静态）
                if let Some(current_class_name) = &self.current_class {
                    if let Some(class_info) = self.type_registry.get_class(current_class_name) {
                        if let Some(field_info) = class_info.fields.get(name) {
                            if field_info.is_static {
                                return Ok(field_info.field_type.clone());
                            } else if self.current_method_is_static {
                                // 静态方法中不能访问非静态字段
                                return Err(semantic_error_at_loc(
                                    loc,
                                    format!("non-static variable {} cannot be referenced from a static context", name)
                                ));
                            }
                            // 非静态方法中返回字段类型
                            return Ok(field_info.field_type.clone());
                        }
                        // 检查父类的字段（继承）
                        if let Some(parent_name) = &class_info.parent {
                            if let Some(parent_info) = self.type_registry.get_class(parent_name) {
                                if let Some(field_info) = parent_info.fields.get(name) {
                                    if field_info.is_static {
                                        return Ok(field_info.field_type.clone());
                                    } else if self.current_method_is_static {
                                        return Err(semantic_error_at_loc(
                                            loc,
                                            format!("non-static variable {} cannot be referenced from a static context", name)
                                        ));
                                    }
                                    return Ok(field_info.field_type.clone());
                                }
                            }
                        }
                    }
                }
                
                if self.type_registry.class_exists(name) 
                    || self.type_registry.get_struct(name).is_some()
                    || self.type_registry.get_enum(name).is_some() {
                    // 标识符是类名，返回类类型（用于静态成员访问）
                    Ok(Type::Object(name.clone()))
                } else {
                    Err(crate::error::undefined_identifier_error_with_file(
                        loc.file.clone(), loc.line, loc.column, name
                    ))
                }
            }
            Expr::Binary(bin) => self.infer_binary_type(bin),
            Expr::Unary(unary) => self.infer_unary_type(unary),
            Expr::Call(call) => self.infer_call_type(call),
            Expr::MemberAccess(member) => self.infer_member_access_type(member),
            Expr::New(new_expr) => self.infer_new_type(new_expr),
            Expr::Assignment(assign) => self.infer_assignment_type(assign),
            Expr::Cast(cast) => self.infer_cast_type(cast),
            Expr::ArrayCreation(arr) => self.infer_array_creation_type(arr),
            Expr::ArrayInit(init) => self.infer_array_init_type(init),
            Expr::ArrayAccess(arr) => self.infer_array_access_type(arr),
            Expr::MethodRef(method_ref) => self.infer_method_ref_type(method_ref),
            Expr::Lambda(lambda) => self.infer_lambda_type(lambda),
            Expr::Ternary(ternary) => self.infer_ternary_type(ternary),
            Expr::InstanceOf(instanceof) => self.infer_instanceof_type(instanceof),
            Expr::Alloc(_) => Ok(Type::Int64),  // 0.5.0.0: alloc 返回 long (指针)
            Expr::Dealloc(_) => Ok(Type::Void), // 0.5.0.0: dealloc 返回 void
            Expr::NamedArg(named) => self.infer_expr_type_internal(&named.value), // 命名参数返回其值的类型
        }
    }

    /// 推断二元表达式类型
    fn infer_binary_type(&mut self, bin: &BinaryExpr) -> cayResult<Type> {
        let left_type = self.infer_expr_type_internal(&bin.left)?;
        let right_type = self.infer_expr_type_internal(&bin.right)?;
        
        match bin.op {
            BinaryOp::Add => {
                // 字符串连接：两个操作数都必须是字符串
                if left_type == Type::String && right_type == Type::String {
                    Ok(Type::String)
                }
                // 字符串 + char：允许，结果为字符串
                else if left_type == Type::String && right_type == Type::Char {
                    Ok(Type::String)
                }
                // char + 字符串：允许，结果为字符串
                else if left_type == Type::Char && right_type == Type::String {
                    Ok(Type::String)
                }
                // 数值加法：两个操作数都必须是基本数值类型
                else if left_type.is_primitive() && right_type.is_primitive() {
                    // 类型提升
                    Ok(self.promote_types(&left_type, &right_type))
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        format!("Cannot add {} and {}: addition requires both operands to be numeric or both to be strings", left_type, right_type)
                    ))
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                if left_type.is_primitive() && right_type.is_primitive() {
                    // 检查除零和模零（仅当右操作数是字面量0时）
                    if matches!(bin.op, BinaryOp::Div | BinaryOp::Mod) {
                        if let Expr::Literal(lit_expr) = bin.right.as_ref() {
                            if let LiteralValue::Int32(0) = lit_expr.value {
                                return Err(semantic_error_at_loc(
                                    &bin.loc,
                                    "/ by zero".to_string()
                                ));
                            }
                            if let LiteralValue::Int64(0) = lit_expr.value {
                                return Err(semantic_error_at_loc(
                                    &bin.loc,
                                    "/ by zero".to_string()
                                ));
                            }
                        }
                    }
                    // 类型提升
                    Ok(self.promote_types(&left_type, &right_type))
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        format!("Cannot apply {:?} to {} and {}: operator requires numeric operands", bin.op, left_type, right_type)
                    ))
                }
            }
            BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                Ok(Type::Bool)
            }
            BinaryOp::And | BinaryOp::Or => {
                if left_type == Type::Bool && right_type == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        "Logical operators require boolean operands"
                    ))
                }
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                if left_type.is_integer() && right_type.is_integer() {
                    Ok(self.promote_integer_types(&left_type, &right_type))
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        format!("Bitwise operator {:?} requires integer operands, got {} and {}",
                               bin.op, left_type, right_type)
                    ))
                }
            }
            BinaryOp::Shl | BinaryOp::Shr | BinaryOp::UnsignedShr => {
                if left_type.is_integer() && right_type.is_integer() {
                    // 移位运算符的结果类型与左操作数相同（经过整数提升）
                    Ok(self.promote_integer_types(&left_type, &right_type))
                } else {
                    Err(semantic_error_at_loc(
                        &bin.loc,
                        format!("Shift operator {:?} requires integer operands, got {} and {}",
                               bin.op, left_type, right_type)
                    ))
                }
            }
            _ => Ok(left_type),
        }
    }

    /// 推断一元表达式类型
    fn infer_unary_type(&mut self, unary: &UnaryExpr) -> cayResult<Type> {
        let operand_type = self.infer_expr_type_internal(&unary.operand)?;
        match unary.op {
            UnaryOp::Neg => {
                // 特殊处理：-2147483648 (i32::MIN)
                // 正数 2147483648 超出 i32::MAX，被解析为 Int64，
                // 但取反后 -2147483648 = i32::MIN，应该被视为 Int32
                if let Expr::Literal(lit) = unary.operand.as_ref() {
                    if let LiteralValue::Int64(val) = lit.value {
                        if val == -(i32::MIN as i64) {
                            return Ok(Type::Int32);
                        }
                    }
                }
                Ok(operand_type)
            }
            UnaryOp::Not => {
                if operand_type == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(semantic_error_at_loc(
                        &unary.loc,
                        "Cannot apply '!' to non-boolean"
                    ))
                }
            }
            UnaryOp::BitNot => Ok(operand_type),
            UnaryOp::AddressOf => {
                // &操作符返回指向操作数的指针，类型为 Int64 (long)
                Ok(Type::Int64)
            }
            UnaryOp::Deref => {
                // *操作符解引用指针，返回指针指向的类型
                // 根据操作数类型推断解引用返回类型
                match &operand_type {
                    Type::Int64 => {
                        // long 类型被视为指针，解引用返回类型需要根据上下文确定
                        // 由于 Cavvy 使用 Int64 表示指针，我们无法仅从类型知道指向的内容
                        // 这里返回一个特殊的指针类型标记，让赋值检查来处理
                        Ok(Type::Int64)
                    }
                    Type::Array(elem_type) => {
                        // 数组类型解引用返回元素类型
                        Ok((**elem_type).clone())
                    }
                    _ => {
                        // 对于其他类型，报错
                        Err(semantic_error_at_loc(
                            &unary.loc,
                            format!("Cannot dereference non-pointer type '{}'", operand_type)
                        ))
                    }
                }
            }
            _ => Ok(operand_type),
        }
    }

    /// 推断函数调用类型
    fn infer_call_type(&mut self, call: &CallExpr) -> cayResult<Type> {
        // 首先处理标识符调用（内置函数、extern函数、方法调用等）
        // 这需要在函数指针检查之前，因为函数指针变量也是标识符
        // 但我们需要先检查是否是已知的函数名
        if let Expr::Identifier(name) = call.callee.as_ref() {
            // 内置输入函数的类型推断
            match name.as_str() {
                "print" | "println" => return Ok(Type::Void),
                "readInt" => return Ok(Type::Int32),
                "readLong" => return Ok(Type::Int64),
                "readFloat" => return Ok(Type::Float32),
                "readDouble" => return Ok(Type::Float64),
                "readLine" => return Ok(Type::String),
                "readChar" => return Ok(Type::Char),
                "readBool" => return Ok(Type::Bool),
                // 运行时辅助函数
                "__cay_read_ptr" => {
                    // 检查参数数量
                    if call.args.len() != 1 {
                        return Err(semantic_error_at_loc(&call.loc, format!("Function '__cay_read_ptr' requires 1 argument, but got {}", call.args.len())));
                    }
                    return Ok(Type::Int64);
                }
                "__cay_ptr_to_string" => {
                    // 检查参数数量
                    if call.args.len() != 1 {
                        return Err(semantic_error_at_loc(&call.loc, format!("Function '__cay_ptr_to_string' requires 1 argument, but got {}", call.args.len())));
                    }
                    return Ok(Type::String);
                }
                "__cay_write_ptr" => {
                    // 检查参数数量
                    if call.args.len() != 2 {
                        return Err(semantic_error_at_loc(&call.loc, format!("Function '__cay_write_ptr' requires 2 arguments, but got {}", call.args.len())));
                    }
                    return Ok(Type::Void);
                }
                "__cay_write_int" => {
                    // 检查参数数量
                    if call.args.len() != 2 {
                        return Err(semantic_error_at_loc(&call.loc, format!("Function '__cay_write_int' requires 2 arguments, but got {}", call.args.len())));
                    }
                    return Ok(Type::Void);
                }
                _ => {}
            }

            // 检查是否是 extern 函数（全局函数）
            // 注意：如果extern函数有别名，只能通过别名调用
            let extern_func_info = if let Some(ref prog) = self.program {
                let mut found_func = None;
                for extern_decl in &prog.extern_declarations {
                    for extern_func in &extern_decl.functions {
                        // 检查是否匹配：有别名的按别名匹配，没别名的按原名匹配
                        let is_match = match &extern_func.alias {
                            Some(alias) => alias == name.as_ref(),
                            None => extern_func.name == name.as_ref(),
                        };
                        
                        if is_match {
                            // 检查参数数量（不包括可变参数）
                            let fixed_param_count = extern_func.params.iter()
                                .filter(|p| !p.is_varargs)
                                .count();
                            let has_varargs = extern_func.params.iter().any(|p| p.is_varargs);
                            
                            if has_varargs {
                                // 可变参数函数：参数数量 >= 固定参数数量
                                if call.args.len() < fixed_param_count {
                                    return Err(semantic_error_at_loc(&call.loc,
                                        format!("Function '{}' requires at least {} arguments, but got {}",
                                            name, fixed_param_count, call.args.len())));
                                }
                            } else {
                                // 非可变参数函数：参数数量必须匹配
                                if call.args.len() != extern_func.params.len() {
                                    return Err(semantic_error_at_loc(&call.loc,
                                        format!("Function '{}' requires {} arguments, but got {}",
                                            name, extern_func.params.len(), call.args.len())));
                                }
                            }
                            
                            found_func = Some((extern_func.return_type.clone(), extern_func.params.clone()));
                            break;
                        }
                    }
                    if found_func.is_some() {
                        break;
                    }
                }
                found_func
            } else {
                None
            };
            
            // 在可变借用self之前检查extern函数参数类型
            if let Some((return_type, params)) = extern_func_info {
                // 检查参数类型兼容性
                for (i, (arg, param)) in call.args.iter().zip(params.iter()).enumerate() {
                    if param.is_varargs {
                        break; // 可变参数后面不再检查
                    }
                    let arg_type = self.infer_expr_type_internal(arg)?;
                    if !self.types_compatible(&arg_type, &param.param_type) {
                        return Err(semantic_error_at_loc(&call.loc, format!("Argument {} type mismatch: expected {}, got {}",
                                i + 1, param.param_type, arg_type)
                        ));
                    }
                }
                return Ok(return_type);
            }

            // 尝试查找当前类的方法（无对象调用）- 支持方法重载
            if let Some(ref current_class) = self.current_class.clone() {
                // 先推断所有参数类型
                let mut arg_types = Vec::new();
                for arg in &call.args {
                    arg_types.push(self.infer_expr_type_internal(arg)?);
                }

                // 使用参数类型查找匹配的方法
                if let Some(method_info) = self.type_registry.find_method(current_class, name.as_ref(), &arg_types) {
                    let return_type = method_info.return_type.clone();
                    let params = method_info.params.clone();
                    // 检查参数类型兼容性（支持可变参数）
                    if let Err(msg) = self.check_arguments_compatible(&call.args, &params, call.loc.line, call.loc.column) {
                        return Err(semantic_error_at_loc(&call.loc, msg));
                    }

                    return Ok(return_type);
                }
            }

            // 如果找不到任何合适的方法，尝试查找顶层函数
            // 先收集顶层函数信息，避免借用冲突
            let top_level_func_info = if let Some(program) = &self.program {
                program.top_level_functions.iter()
                    .find(|func| func.name == name.as_ref())
                    .map(|func| (func.params.clone(), func.return_type.clone()))
            } else {
                None
            };

            if let Some((params, return_type)) = top_level_func_info {
                // 找到顶层函数，检查参数类型兼容性
                if let Err(msg) = self.check_arguments_compatible(&call.args, &params, call.loc.line, call.loc.column) {
                    return Err(semantic_error_at_loc(&call.loc, msg));
                }
                return Ok(return_type);
            }
        }

        // 支持成员调用: obj.method(...) 或 ClassName.method()（静态方法）
        if let Expr::MemberAccess(member) = call.callee.as_ref() {
            // 推断对象类型
            let obj_type = self.infer_expr_type_internal(&member.object)?;

            // 处理 String 类型方法调用
            if obj_type == Type::String {
                return self.infer_string_method_call(&member.member, &call.args, call.loc.line, call.loc.column);
            }

            // 检查是否是类名（静态方法调用）- 支持方法重载
            if let Expr::Identifier(class_name) = &*member.object {
                let class_name_str = class_name.as_ref().to_string();
                // 先推断所有参数类型
                let mut arg_types = Vec::new();
                for arg in &call.args {
                    arg_types.push(self.infer_expr_type_internal(arg)?);
                }

                if let Some(class_info) = self.type_registry.get_class(&class_name_str) {
                    // 使用参数类型查找匹配的静态方法
                    if let Some(method_info) = class_info.find_method(&member.member, &arg_types) {
                        if method_info.is_static {
                            let return_type = method_info.return_type.clone();
                            let params = method_info.params.clone();
                            // 检查参数类型兼容性（支持可变参数）
                            if let Err(msg) = self.check_arguments_compatible(&call.args, &params, call.loc.line, call.loc.column) {
                                return Err(semantic_error_at_loc(&call.loc, msg));
                            }

                            return Ok(return_type);
                        }
                    }
                }
            }
            
            // 检查是否是 enum 构造函数调用: EnumName.VariantName(args)
            if let Expr::Identifier(class_name) = &*member.object {
                let class_name_str = class_name.as_ref().to_string();
                let member_name = member.member.clone();
                if let Some(enum_info) = self.type_registry.get_enum(&class_name_str) {
                    if let Some(variant) = enum_info.variants.iter().find(|v| v.name == member_name) {
                        let payload_type_opt = variant.payload_type.clone();
                        drop(member); // 释放对 call 的借用
                        drop(enum_info);
                        // 验证参数数量
                        match &payload_type_opt {
                            Some(expected_payload_type) => {
                                if call.args.len() != 1 {
                                    return Err(semantic_error_at_loc(
                                        &call.loc,
                                        format!("Enum variant '{}.{}' with payload expects 1 argument, but got {}", 
                                            class_name_str, member_name, call.args.len())
                                    ));
                                }
                                // 验证参数类型
                                let arg_type = self.infer_expr_type_internal(&call.args[0])?;
                                if !self.types_compatible(&arg_type, expected_payload_type) {
                                    return Err(semantic_error_at_loc(
                                        &call.loc,
                                        format!("Enum variant '{}.{}' payload type mismatch: expected {}, got {}",
                                            class_name_str, member_name, expected_payload_type, arg_type)
                                    ));
                                }
                            }
                            None => {
                                if !call.args.is_empty() {
                                    return Err(semantic_error_at_loc(
                                        &call.loc,
                                        format!("Enum variant '{}.{}' has no payload, but got {} argument(s)",
                                            class_name_str, member_name, call.args.len())
                                    ));
                                }
                            }
                        }
                        return Ok(Type::Object(class_name_str));
                    }
                    return Err(semantic_error_at_loc(
                        &call.loc,
                        format!("Unknown variant '{}' for enum {}", member_name, class_name_str)
                    ));
                }
            }

            // 处理数组类型的 length() 方法调用（作为 .length 属性的语法糖）
            if let Type::Array(_) = &obj_type {
                if member.member == "length" && call.args.is_empty() {
                    return Ok(Type::Int32);
                }
            }

            // 处理类实例方法调用 - 支持方法重载
            // 获取类名（支持 Type::Object 和 Type::Generic）
            let class_name_opt = match &obj_type {
                Type::Object(class_name) => Some(class_name.clone()),
                Type::Generic(class_name, _) => Some(class_name.clone()),
                _ => None,
            };
            
            if let Some(class_name) = class_name_opt {
                // 先推断所有参数类型
                let mut arg_types = Vec::new();
                for arg in &call.args {
                    arg_types.push(self.infer_expr_type_internal(arg)?);
                }

                // 首先检查是否是函数指针字段调用
                // 查找类的字段，看是否是函数指针类型
                if let Some(class_info) = self.type_registry.get_class(&class_name) {
                    if let Some(field_info) = class_info.fields.get(&member.member) {
                        if let Type::Function(func_type) = &field_info.field_type {
                            // 是函数指针字段调用
                            let return_type = *func_type.return_type.clone();
                            let params = func_type.params.clone();
                            // 检查参数数量
                            if call.args.len() != params.len() {
                                return Err(semantic_error_at_loc(&call.loc, format!("Function pointer field '{}' requires {} arguments, but got {}",
                                        member.member, params.len(), call.args.len())
                                ));
                            }
                            // 检查参数类型兼容性（手动检查，因为params是Vec<Type>而不是Vec<ParameterInfo>）
                            for (i, (arg, expected_type)) in call.args.iter().zip(params.iter()).enumerate() {
                                let arg_type = self.infer_expr_type_internal(arg)?;
                                if !self.types_compatible(&arg_type, expected_type) {
                                    return Err(semantic_error_at_loc(&call.loc, format!("Argument {} type mismatch: expected {}, got {}", i + 1, expected_type, arg_type)
                                    ));
                                }
                            }
                            return Ok(return_type);
                        }
                    }
                }

                // 使用参数类型查找匹配的方法
                // 首先尝试直接使用类名查找
                let method_result = if let Some(method_info) = self.type_registry.find_method(&class_name, &member.member, &arg_types) {
                    Some((method_info.return_type.clone(), method_info.params.clone()))
                } else {
                    // 如果直接查找失败，尝试查找限定类名
                    if let Some(qualified_name) = self.type_registry.find_qualified_class(&class_name) {
                        // eprintln!("[DEBUG] Found qualified class: {} -> {}", class_name, qualified_name);
                        self.type_registry.find_method(&qualified_name, &member.member, &arg_types)
                            .map(|m| (m.return_type.clone(), m.params.clone()))
                    } else {
                        // eprintln!("[DEBUG] Could not find qualified class for: {}", class_name);
                        None
                    }
                };
                
                if let Some((return_type, params)) = method_result {
                    // eprintln!("[DEBUG] Found method: {}.{}, params={:?}, return_type={:?}", class_name, member.member, params, return_type);
                    // 检查参数类型兼容性（支持可变参数）
                    if let Err(msg) = self.check_arguments_compatible(&call.args, &params, call.loc.line, call.loc.column) {
                        return Err(semantic_error_at_loc(&call.loc, msg));
                    }

                    // 如果对象是泛型类型，替换返回类型中的泛型参数
                    let final_return_type = if let Type::Generic(_, type_args) = &obj_type {
                        if let Some(class_info) = self.type_registry.get_class(&class_name) {
                            self.substitute_type_params(&return_type, &class_info.type_params, type_args)
                        } else {
                            return_type
                        }
                    } else {
                        return_type
                    };

                    return Ok(final_return_type);
                } else {
                    return Err(semantic_error_at_loc(&call.loc, format!("Unknown method '{}' for class {}", member.member, class_name)
                    ));
                }
            }
        }

        // 检查标识符是否是函数指针变量
        if let Expr::Identifier(name) = call.callee.as_ref() {
            // 首先检查是否是函数指针变量 - 先收集类型信息避免借用冲突
            let func_ptr_info = self.symbol_table.lookup(name.as_ref()).and_then(|info| {
                if let Type::Function(func_type) = &info.symbol_type {
                    Some((func_type.params.clone(), *func_type.return_type.clone()))
                } else {
                    None
                }
            });
            
            if let Some((params, return_type)) = func_ptr_info {
                // 检查参数数量
                let expected_args = params.len();
                let actual_args = call.args.len();
                if actual_args != expected_args {
                    return Err(semantic_error_at_loc(&call.loc, format!("Function pointer call requires {} arguments, but got {}", expected_args, actual_args)
                    ));
                }
                // 检查参数类型兼容性
                for (i, (arg, expected_type)) in call.args.iter().zip(params.iter()).enumerate() {
                    let arg_type = self.infer_expr_type_internal(arg)?;
                    if !self.types_compatible(&arg_type, expected_type) {
                        return Err(semantic_error_at_loc(&call.loc, format!("Argument {} type mismatch: expected {}, got {}", i + 1, expected_type, arg_type)
                        ));
                    }
                }
                return Ok(return_type);
            }
            
            // 检查 @FreeFunction 注册的自由函数
            if let Some((_class_name, method_info, _loc)) = self.type_registry.free_functions.get(name.as_ref()).cloned() {
                // 验证参数（使用 check_arguments_compatible 以支持可变参数）
                if let Err(msg) = self.check_arguments_compatible(&call.args, &method_info.params, call.loc.line, call.loc.column) {
                    return Err(semantic_error_at_loc(&call.loc,
                        format!("@FreeFunction '{}' {}", name, msg)));
                }
                return Ok(method_info.return_type.clone());
            }
            
            // 检查是否存在同名方法（参数不匹配）
            if let Some(ref current_class) = self.current_class {
                if let Some(class_info) = self.type_registry.get_class(current_class) {
                    if class_info.methods.contains_key(name.as_ref()) {
                        return Err(semantic_error_at_loc(&call.loc, format!("Method '{}' in class '{}' cannot be applied to given types: argument mismatch", name, current_class)
                        ));
                    }
                }
            }
            return Err(semantic_error_at_loc(
                &call.loc,
                format!("Cannot find method '{}'", name)
            ));
        }

        if let Expr::MemberAccess(member) = call.callee.as_ref() {
            if let Expr::Identifier(class_name) = &*member.object {
                return Err(semantic_error_at_loc(&call.loc, format!("Method '{}' in class '{}' cannot be applied to given types: argument mismatch", member.member, class_name)
                ));
            }
            if let Type::Object(class_name) = self.infer_expr_type_internal(&member.object)? {
                return Err(semantic_error_at_loc(&call.loc, format!("Method '{}' in class '{}' cannot be applied to given types: argument mismatch", member.member, class_name)
                ));
            }
        }

        // 最后检查是否是函数指针类型调用: fn_ptr(args...)
        // 如果callee不是标识符或标识符不是已知函数名，则尝试作为函数指针处理
        let callee_type = self.infer_expr_type_internal(&call.callee)?;
        if let Type::Function(func_type) = &callee_type {
            // 检查参数数量
            let expected_args = func_type.params.len();
            let actual_args = call.args.len();
            if actual_args != expected_args {
                return Err(semantic_error_at_loc(&call.loc, format!("Function pointer call requires {} arguments, but got {}", expected_args, actual_args)
                ));
            }
            // 检查参数类型兼容性
            for (i, (arg, expected_type)) in call.args.iter().zip(func_type.params.iter()).enumerate() {
                let arg_type = self.infer_expr_type_internal(arg)?;
                if !self.types_compatible(&arg_type, expected_type) {
                    return Err(semantic_error_at_loc(&call.loc, format!("Argument {} type mismatch: expected {}, got {}", i + 1, expected_type, arg_type)
                    ));
                }
            }
            return Ok(*func_type.return_type.clone());
        }

        Err(semantic_error_at_loc(&call.loc,
            "Cannot resolve method call".to_string()
        ))
    }

    /// 推断成员访问类型
    fn infer_member_access_type(&mut self, member: &MemberAccessExpr) -> cayResult<Type> {
        // 检查是否是静态字段或方法访问: ClassName.fieldName 或 ClassName.methodName
        if let Expr::Identifier(class_name) = &*member.object {
            if let Some(class_info) = self.type_registry.get_class(class_name.as_ref()) {
                // 首先检查字段
                if let Some(field_info) = class_info.fields.get(&member.member) {
                    if field_info.is_static {
                        // 检查私有字段访问权限
                        if !field_info.is_public {
                            if let Some(current_class) = &self.current_class {
                                if current_class != class_name.as_ref() {
                                    return Err(semantic_error_at_loc(
                                        &member.loc,
                                        format!("{} has private access in {}", member.member, class_name)
                                    ));
                                }
                            } else {
                                return Err(semantic_error_at_loc(
                                    &member.loc,
                                    format!("{} has private access in {}", member.member, class_name)
                                ));
                            }
                        }
                        return Ok(field_info.field_type.clone());
                    }
                }
                
                // 检查静态方法 - 返回函数指针类型
                // 由于支持方法重载，需要遍历所有同名方法
                if let Some(methods) = class_info.methods.get(&member.member) {
                    // 查找第一个静态方法（假设没有重载的静态方法）
                    if let Some(method_info) = methods.iter().find(|m| m.is_static) {
                        // 检查私有方法访问权限
                        if !method_info.is_public {
                            if let Some(current_class) = &self.current_class {
                                if current_class != class_name.as_ref() {
                                    return Err(semantic_error_at_loc(
                                        &member.loc,
                                        format!("{} has private access in {}", member.member, class_name)
                                    ));
                                }
                            } else {
                                return Err(semantic_error_at_loc(
                                    &member.loc,
                                    format!("{} has private access in {}", member.member, class_name)
                                ));
                            }
                        }
                        // 返回函数指针类型
                        let param_types = method_info.params.iter()
                            .filter(|p| !p.is_varargs)
                            .map(|p| p.param_type.clone())
                            .collect();
                        let return_type = Box::new(method_info.return_type.clone());
                        return Ok(Type::Function(Box::new(crate::types::FunctionType {
                            params: param_types,
                            return_type,
                            is_static: true,
                        })));
                    }
                }
            }
            
            // 检查是否是 enum variant 访问: EnumName.VariantName
            if let Some(enum_info) = self.type_registry.get_enum(class_name.as_ref()) {
                let variant_exists = enum_info.variants.iter().any(|v| v.name == member.member);
                if variant_exists {
                    return Ok(Type::Object(class_name.to_string()));
                }
                return Err(semantic_error_at_loc(
                    &member.loc,
                    format!("Unknown variant '{}' for enum {}", member.member, class_name)
                ));
            }
        }

        // 成员访问类型检查
        let obj_type = self.infer_expr_type_internal(&member.object)?;

        // 特殊处理数组的 .length 属性
        if member.member == "length" {
            if let Type::Array(_) = obj_type {
                return Ok(Type::Int32);  // length 返回 int
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
                        format!("non-static variable {} cannot be referenced from a static context", member.member)
                    ));
                }
            }
        }

        // 类/struct 成员访问
        // 处理 Type::Object 和 Type::Generic
        let base_class_name_opt = match &obj_type {
            Type::Object(class_name) => {
                // 解析泛型类名: "Optional<T>" -> "Optional"
                if let Some(pos) = class_name.find('<') {
                    Some(&class_name[..pos])
                } else {
                    Some(class_name.as_str())
                }
            }
            Type::Generic(class_name, _) => {
                // Type::Generic 直接返回类名
                Some(class_name.as_str())
            }
            _ => None
        };
        
        if let Some(base_class_name) = base_class_name_opt {
            // 先查 struct
            if let Some(struct_info) = self.type_registry.get_struct(base_class_name) {
                if let Some(field_info) = struct_info.fields.get(&member.member) {
                    return Ok(field_info.field_type.clone());
                }
            }
            if let Some(class_info) = self.type_registry.get_class(base_class_name) {
                if let Some(field_info) = class_info.fields.get(&member.member) {
                    // 检查私有字段访问权限
                    if !field_info.is_public {
                        if let Some(current_class) = &self.current_class {
                            if current_class != base_class_name {
                                return Err(semantic_error_at_loc(
                                    &member.loc,
                                    format!("{} has private access in {}", member.member, base_class_name)
                                ));
                            }
                        } else {
                            return Err(semantic_error_at_loc(
                                &member.loc,
                                format!("{} has private access in {}", member.member, base_class_name)
                            ));
                        }
                    }
                    return Ok(field_info.field_type.clone());
                }
            }
            // 检查是否是 enum variant 访问
            if let Some(enum_info) = self.type_registry.get_enum(base_class_name) {
                if enum_info.variants.iter().any(|v| v.name == member.member) {
                    return Ok(obj_type.clone());
                }
            }
            return Err(semantic_error_at_loc(
                &member.loc,
                format!("Unknown member '{}' for class {}", member.member, base_class_name)
            ));
        }

        Err(semantic_error_at_loc(
            &member.loc,
            format!("Cannot access member '{}' on type {}", member.member, obj_type)
        ))
    }

    /// 推断 new 表达式类型
    fn infer_new_type(&mut self, new_expr: &NewExpr) -> cayResult<Type> {
        // 解析泛型类名: "Optional<T>" -> ("Optional", Some("T"))
        let (base_class_name, type_param) = if let Some(pos) = new_expr.class_name.find('<') {
            let base = &new_expr.class_name[..pos];
            let param_start = pos + 1;
            let param_end = new_expr.class_name.len().saturating_sub(1);
            let param = if param_end > param_start {
                Some(&new_expr.class_name[param_start..param_end])
            } else {
                None
            };
            (base.to_string(), param)
        } else {
            (new_expr.class_name.clone(), None)
        };
        
        // 检查基础类是否存在
        if let Some(class_info) = self.type_registry.get_class(&base_class_name) {
            // 检查是否是抽象类
            if class_info.is_abstract {
                return Err(semantic_error_at_loc(
                    &new_expr.loc,
                    format!("Cannot instantiate abstract class '{}'", base_class_name)
                ));
            }
            
            // 如果类有泛型参数，验证类型参数是否合法
            if !class_info.type_params.is_empty() {
                if let Some(param) = type_param {
                    // 检查类型参数是否是当前类的泛型参数或者是已知类型
                    let is_valid_param = class_info.type_params.contains(&param.to_string())
                        || self.type_registry.class_exists(param)
                        || self.type_registry.get_struct(param).is_some()
                        || matches!(param, "int" | "long" | "float" | "double" | "boolean" | "char" | "String");
                    
                    if !is_valid_param {
                        return Err(semantic_error_at_loc(
                            &new_expr.loc,
                            format!("Unknown type parameter '{}' for class '{}'", param, base_class_name)
                        ));
                    }
                }
                // 返回泛型类型
                Ok(Type::Object(new_expr.class_name.clone()))
            } else {
                // 非泛型类
                Ok(Type::Object(base_class_name))
            }
        } else if self.type_registry.get_struct(&base_class_name).is_some() {
            // struct 是值类型，用 Object 包装
            Ok(Type::Object(new_expr.class_name.clone()))
        } else {
            Err(semantic_error_at_loc(
                &new_expr.loc,
                format!("Unknown class or struct: {}", base_class_name)
            ))
        }
    }

    /// 推断赋值表达式类型
    fn infer_assignment_type(&mut self, assign: &AssignmentExpr) -> cayResult<Type> {
        // 检查是否是 final 变量重新赋值
        if let Expr::Identifier(name) = &assign.target.as_ref() {
            if let Some(info) = self.symbol_table.lookup(name.as_ref()) {
                if info.is_final {
                    return Err(semantic_error_at_loc(
                        &assign.loc,
                        format!("Cannot assign a value to final variable '{}'", name)
                    ));
                }
            }
        }

        let target_type = self.infer_expr_type_internal(&assign.target)?;
        let value_type = self.infer_expr_type_internal(&assign.value)?;

        if self.types_compatible(&value_type, &target_type) {
            Ok(target_type)
        } else {
            Err(semantic_error_with_file(
                assign.loc.file.clone(),
                assign.loc.line,
                assign.loc.column,
                format!("Cannot assign {} to {}", value_type, target_type)
            ))
        }
    }

    /// 推断类型转换表达式类型
    ///
    /// 验证类型转换的合法性并返回目标类型。
    /// 支持的转换类型：
    /// - 数值类型之间的转换（int <-> float，精度可能损失）
    /// - 引用类型之间的转换（继承层次结构内）
    /// - char 与 int 之间的转换
    ///
    /// # Arguments
    /// * `cast` - 类型转换表达式
    ///
    /// # Returns
    /// 成功时返回目标类型，失败时返回语义错误
    ///
    /// # Type Conversion Rules
    /// 1. 相同类型：允许（无实际效果）
    /// 2. 数值类型之间：允许（可能精度损失）
    /// 3. 引用类型之间：仅当存在继承关系时允许
    /// 4. char <-> int：允许
    /// 5. 数组类型之间：仅当元素类型兼容时允许
    /// 6. 其他组合：非法转换
    fn infer_cast_type(&mut self, cast: &CastExpr) -> cayResult<Type> {
        let source_type = self.infer_expr_type_internal(&cast.expr)?;
        let target_type = &cast.target_type;
        
        // 相同类型，无需转换
        if source_type == *target_type {
            return Ok(target_type.clone());
        }
        
        // 检查转换是否合法
        if self.is_valid_cast(&source_type, target_type) {
            Ok(target_type.clone())
        } else {
            Err(semantic_error_at_loc(
                &cast.loc,
                format!("Invalid cast from {} to {}", source_type, target_type)
            ))
        }
    }
    
    /// 检查类型转换是否合法
    ///
    /// # Arguments
    /// * `from` - 源类型
    /// * `to` - 目标类型
    ///
    /// # Returns
    /// 如果转换合法返回 true
    fn is_valid_cast(&self, from: &Type, to: &Type) -> bool {
        use crate::types::Type;

        match (from, to) {
            // 相同类型
            (a, b) if a == b => true,

            // 数值类型之间的转换（所有组合都允许，可能精度损失）
            (Type::Int32, Type::Int64) |
            (Type::Int32, Type::Float32) |
            (Type::Int32, Type::Float64) |
            (Type::Int64, Type::Int32) |
            (Type::Int64, Type::Float32) |
            (Type::Int64, Type::Float64) |
            (Type::Float32, Type::Int32) |
            (Type::Float32, Type::Int64) |
            (Type::Float32, Type::Float64) |
            (Type::Float64, Type::Int32) |
            (Type::Float64, Type::Int64) |
            (Type::Float64, Type::Float32) => true,

            // char 与数值类型之间的转换
            (Type::Char, Type::Int32) |
            (Type::Char, Type::Int64) |
            (Type::Char, Type::CInt) |
            (Type::Int32, Type::Char) |
            (Type::Int64, Type::Char) |
            (Type::CInt, Type::Char) => true,

            // 任何基本类型都可以转换为 string
            (Type::Int32, Type::String) |
            (Type::Int64, Type::String) |
            (Type::Float32, Type::String) |
            (Type::Float64, Type::String) |
            (Type::Char, Type::String) |
            (Type::Bool, Type::String) => true,

            // String 与 c_string (c_char*) 之间的转换（两者在底层都是 i8*）
            (Type::String, Type::CChar) |
            (Type::CChar, Type::String) => true,
            // String 与 c_char* (Pointer(CChar)) 之间的转换
            (Type::String, Type::Pointer(inner)) if matches!(inner.as_ref(), Type::CChar) => true,
            (Type::Pointer(inner), Type::String) if matches!(inner.as_ref(), Type::CChar) => true,

            // c_void* 与任意指针类型之间的转换（C风格）
            (Type::Pointer(from_inner), Type::Pointer(to_inner)) 
                if matches!(from_inner.as_ref(), Type::CVoid) || matches!(to_inner.as_ref(), Type::CVoid) => true,

            // FFI 类型与基本类型之间的转换
            // c_int <-> int
            (Type::CInt, Type::Int32) | (Type::Int32, Type::CInt) => true,
            // c_long <-> long
            (Type::CLong, Type::Int64) | (Type::Int64, Type::CLong) => true,
            // c_short <-> int (16位到32位)
            (Type::CShort, Type::Int32) | (Type::Int32, Type::CShort) => true,
            // c_char/c_uchar <-> int
            (Type::CChar, Type::Int32) | (Type::Int32, Type::CChar) => true,
            (Type::CUChar, Type::Int32) | (Type::Int32, Type::CUChar) => true,
            (Type::CChar, Type::Char) | (Type::Char, Type::CChar) => true,
            // c_short/c_ushort <-> int
            (Type::CShort, Type::Int32) | (Type::Int32, Type::CShort) => true,
            (Type::CUShort, Type::Int32) | (Type::Int32, Type::CUShort) => true,
            // c_int/c_uint <-> int/long
            (Type::CInt, Type::Int32) | (Type::Int32, Type::CInt) => true,
            (Type::CUInt, Type::Int32) | (Type::Int32, Type::CUInt) => true,
            (Type::CInt, Type::Int64) | (Type::Int64, Type::CInt) => true,
            (Type::CUInt, Type::Int64) | (Type::Int64, Type::CUInt) => true,
            // c_long <-> int/long
            (Type::CLong, Type::Int32) | (Type::Int32, Type::CLong) => true,
            (Type::CLong, Type::Int64) | (Type::Int64, Type::CLong) => true,
            // c_ulong <-> int/long/c_long
            (Type::CULong, Type::Int32) | (Type::Int32, Type::CULong) => true,
            (Type::CULong, Type::Int64) | (Type::Int64, Type::CULong) => true,
            (Type::CULong, Type::CLong) | (Type::CLong, Type::CULong) => true,
            (Type::CULong, Type::CUInt) | (Type::CUInt, Type::CULong) => true,
            // c_float <-> float/double
            (Type::CFloat, Type::Float32) | (Type::Float32, Type::CFloat) => true,
            (Type::CFloat, Type::Float64) | (Type::Float64, Type::CFloat) => true,
            // c_double <-> float/double
            (Type::CDouble, Type::Float64) | (Type::Float64, Type::CDouble) => true,
            (Type::CDouble, Type::Float32) | (Type::Float32, Type::CDouble) => true,
            // size_t/ssize_t <-> long 和 int
            (Type::SizeT, Type::Int64) | (Type::Int64, Type::SizeT) => true,
            (Type::SizeT, Type::Int32) | (Type::Int32, Type::SizeT) => true,
            (Type::SSizeT, Type::Int64) | (Type::Int64, Type::SSizeT) => true,
            (Type::SSizeT, Type::Int32) | (Type::Int32, Type::SSizeT) => true,
            // uintptr_t/intptr_t <-> long 和 int
            (Type::UIntPtr, Type::Int64) | (Type::Int64, Type::UIntPtr) => true,
            (Type::UIntPtr, Type::Int32) | (Type::Int32, Type::UIntPtr) => true,
            (Type::IntPtr, Type::Int64) | (Type::Int64, Type::IntPtr) => true,
            (Type::IntPtr, Type::Int32) | (Type::Int32, Type::IntPtr) => true,
            // ptr <-> uintptr_t/intptr_t (指针与整数类型转换)
            (Type::Pointer(_), Type::UIntPtr) | (Type::UIntPtr, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::IntPtr) | (Type::IntPtr, Type::Pointer(_)) => true,
            // ptr <-> long/int (指针与基本整数类型转换，用于 FFI 中 & 和 c_str 等返回 IntPtr 的场景)
            (Type::Pointer(_), Type::Int64) | (Type::Int64, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::Int32) | (Type::Int32, Type::Pointer(_)) => true,
            // FFI 整数类型 <-> 指针 (用于 c_long/c_ulong 等作为指针值的场景)
            (Type::Pointer(_), Type::CLong) | (Type::CLong, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CULong) | (Type::CULong, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CInt) | (Type::CInt, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CUInt) | (Type::CUInt, Type::Pointer(_)) => true,
            // c_bool <-> bool 和 int
            (Type::CBool, Type::Bool) | (Type::Bool, Type::CBool) => true,
            (Type::CBool, Type::Int32) | (Type::Int32, Type::CBool) => true,

            // FFI 类型之间的转换
            (Type::CInt, Type::CLong) | (Type::CLong, Type::CInt) => true,
            (Type::CInt, Type::CShort) | (Type::CShort, Type::CInt) => true,
            (Type::CInt, Type::CChar) | (Type::CChar, Type::CInt) => true,
            (Type::CFloat, Type::CDouble) | (Type::CDouble, Type::CFloat) => true,
            (Type::SizeT, Type::UIntPtr) | (Type::UIntPtr, Type::SizeT) => true,
            (Type::SSizeT, Type::IntPtr) | (Type::IntPtr, Type::SSizeT) => true,
            (Type::UIntPtr, Type::IntPtr) | (Type::IntPtr, Type::UIntPtr) => true,

            // 引用类型之间的转换：需要继承关系
            (Type::Object(from_name), Type::Object(to_name)) => {
                // 检查是否存在继承关系（双向）
                self.is_related_type(from_name, to_name)
            }

            // 数组类型之间的转换：元素类型兼容
            (Type::Array(from_elem), Type::Array(to_elem)) => {
                self.is_valid_cast(from_elem, to_elem)
            }

            // null 可以转换为任何引用类型
            (Type::Object(obj_name), Type::Object(_)) if obj_name == "Object" => true,

            // 其他组合都不合法
            _ => false,
        }
    }
    
    /// 检查两个类型是否存在继承关系（双向）
    ///
    /// 用于类型转换检查，允许向上转型（子类->父类）和向下转型（父类->子类）
    fn is_related_type(&self, type_a: &str, type_b: &str) -> bool {
        // 相同类型
        if type_a == type_b {
            return true;
        }
        
        // 检查 type_a 是否是 type_b 的子类型
        if self.is_subtype_of_by_name(type_a, type_b) {
            return true;
        }
        
        // 检查 type_b 是否是 type_a 的子类型
        if self.is_subtype_of_by_name(type_b, type_a) {
            return true;
        }
        
        false
    }
    
    /// 通过类型名称检查子类型关系
    ///
    /// 辅助函数，用于检查一个类型是否是另一个类型的子类型
    fn is_subtype_of_by_name(&self, subtype: &str, supertype: &str) -> bool {
        // 相同类型
        if subtype == supertype {
            return true;
        }
        
        // 所有类都是 Object 的子类型
        if supertype == "Object" {
            return self.type_registry.class_exists(subtype)
                || subtype == "String"
                || subtype == "Function";
        }
        
        // 迭代遍历继承链
        let mut current = subtype.to_string();
        let mut visited = std::collections::HashSet::new();
        
        loop {
            // 防止循环继承导致的无限循环
            if !visited.insert(current.clone()) {
                return false;
            }
            
            if let Some(class_info) = self.type_registry.get_class(&current) {
                match &class_info.parent {
                    Some(parent) => {
                        if parent == supertype {
                            return true;
                        }
                        current = parent.clone();
                    }
                    None => return false,
                }
            } else {
                // 内置类型检查
                return (subtype == "String" || subtype == "Function") && supertype == "Object";
            }
        }
    }

    /// 推断数组创建表达式类型
    fn infer_array_creation_type(&mut self, arr: &ArrayCreationExpr) -> cayResult<Type> {
        // 数组创建: new Type[size] 或 new Type[size1][size2]... 或 new Type[size][] (不规则数组)
        // 检查所有维度的大小
        for (i, size) in arr.sizes.iter().enumerate() {
            // 跳过空维度（不规则数组，如 new int[5][]）
            if let Expr::Literal(lit_expr) = size {
                if let LiteralValue::Null = lit_expr.value {
                    continue;
                }
            }
            
            let size_type = self.infer_expr_type_internal(size)?;
            if !size_type.is_integer() {
                return Err(semantic_error_at_loc(
                    &arr.loc,
                    format!("Array size at dimension {} must be integer, got {}", i + 1, size_type)
                ));
            }
            // 检查负数数组大小（仅当大小是字面量或一元负号表达式时）
            // 支持直接负数字面量如 -5（被解析为 Unary(Neg, Literal(5))）
            if let Expr::Literal(lit_expr) = size {
                if let LiteralValue::Int32(n) = lit_expr.value {
                    if n < 0 {
                        return Err(semantic_error_at_loc(
                            &arr.loc,
                            format!("Array size cannot be negative: {}", n)
                        ));
                    }
                }
                if let LiteralValue::Int64(n) = lit_expr.value {
                    if n < 0 {
                        return Err(semantic_error_at_loc(
                            &arr.loc,
                            format!("Array size cannot be negative: {}", n)
                        ));
                    }
                }
            }
            // 检查一元负号表达式如 -5
            if let Expr::Unary(unary) = size {
                if let UnaryOp::Neg = unary.op {
                    if let Expr::Literal(lit_expr) = unary.operand.as_ref() {
                        if let LiteralValue::Int32(n) = lit_expr.value {
                            return Err(semantic_error_at_loc(
                                &arr.loc,
                                format!("Array size cannot be negative: -{}", n)
                            ));
                        }
                        if let LiteralValue::Int64(n) = lit_expr.value {
                            return Err(semantic_error_at_loc(
                                &arr.loc,
                                format!("Array size cannot be negative: -{}", n)
                            ));
                        }
                    }
                }
            }
        }
        Ok(Type::Array(Box::new(arr.element_type.clone())))
    }

    /// 推断数组初始化表达式类型
    fn infer_array_init_type(&mut self, init: &ArrayInitExpr) -> cayResult<Type> {
        // 数组初始化: {1, 2, 3}
        // 需要上下文来推断类型，这里返回一个占位符类型
        // 实际类型会在变量声明时根据声明类型确定
        if init.elements.is_empty() {
            return Err(semantic_error_at_loc(
                &init.loc,
                "Cannot infer type of empty array initializer".to_string()
            ));
        }
        // 推断第一个元素的类型作为数组元素类型
        let elem_type = self.infer_expr_type_internal(&init.elements[0])?;
        Ok(Type::Array(Box::new(elem_type)))
    }

    /// 推断数组访问表达式类型
    fn infer_array_access_type(&mut self, arr: &ArrayAccessExpr) -> cayResult<Type> {
        // 数组访问: arr[index]
        let array_type = self.infer_expr_type_internal(&arr.array)?;
        let index_type = self.infer_expr_type_internal(&arr.index)?;

        if !index_type.is_integer() {
            return Err(semantic_error_at_loc(
                &arr.loc,
                format!("Array index must be integer, got {}", index_type)
            ));
        }

        match array_type {
            Type::Array(element_type) => Ok(*element_type),
            _ => Err(semantic_error_at_loc(
                &arr.loc,
                format!("Cannot index non-array type {}", array_type)
            )),
        }
    }

    /// 推断方法引用表达式类型
    fn infer_method_ref_type(&mut self, method_ref: &MethodRefExpr) -> cayResult<Type> {
        // 方法引用: ClassName::methodName 或 obj::methodName
        // 返回函数类型，包含参数类型和返回类型信息
        
        if let Some(ref class_name) = method_ref.class_name {
            // 检查类是否存在
            if !self.type_registry.class_exists(class_name) {
                return Err(semantic_error_at_loc(
                    &method_ref.loc,
                    format!("Unknown class: {}", class_name)
                ));
            }
            // 获取方法信息
            if let Some(class_info) = self.type_registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(&method_ref.method_name) {
                    if let Some(method_info) = methods.first() {
                        // 构建函数类型
                        let param_types: Vec<Type> = method_info.params.iter()
                            .map(|p| p.param_type.clone())
                            .collect();
                        let return_type = Box::new(method_info.return_type.clone());
                        
                        return Ok(Type::Function(Box::new(crate::types::FunctionType {
                            params: param_types,
                            return_type,
                            is_static: method_info.is_static,
                        })));
                    }
                } else {
                    return Err(semantic_error_at_loc(
                        &method_ref.loc,
                        format!("Unknown method '{}' for class {}", method_ref.method_name, class_name)
                    ));
                }
            }
        } else if let Some(object) = method_ref.object.as_ref() {
            // 实例方法引用: obj::methodName
            let obj_type = self.infer_expr_type_internal(object)?;
            if let Type::Object(class_name) = obj_type {
                if let Some(class_info) = self.type_registry.get_class(&class_name) {
                    if let Some(methods) = class_info.methods.get(&method_ref.method_name) {
                        if let Some(method_info) = methods.first() {
                            let param_types: Vec<Type> = method_info.params.iter()
                                .map(|p| p.param_type.clone())
                                .collect();
                            let return_type = Box::new(method_info.return_type.clone());
                            
                            return Ok(Type::Function(Box::new(crate::types::FunctionType {
                                params: param_types,
                                return_type,
                                is_static: false,
                            })));
                        }
                    }
                }
            }
        }
        
        // 无法确定具体函数类型，返回通用 Function 类型
        Ok(Type::Object("Function".to_string()))
    }

    /// 推断 Lambda 表达式类型
    fn infer_lambda_type(&mut self, lambda: &LambdaExpr) -> cayResult<Type> {
        // Lambda 表达式: (params) -> { body }
        // 创建新的作用域
        self.symbol_table.enter_scope();

        // 添加 Lambda 参数到符号表
        let mut param_types = Vec::new();
        for param in &lambda.params {
            let param_type = param.param_type.clone().unwrap_or(Type::Int32);
            param_types.push(param_type.clone());
            self.symbol_table.declare(
                param.name.clone(),
                SemanticSymbolInfo {
                    name: param.name.clone(),
                    symbol_type: param_type,
                    is_final: false,
                    is_initialized: true,
                }
            );
        }

        // 推断 Lambda 体类型
        let return_type = match &lambda.body {
            LambdaBody::Expr(expr) => {
                let expr_type = self.infer_expr_type_internal(expr)?;
                Box::new(expr_type)
            }
            LambdaBody::Block(block) => {
                // 分析块中的语句，查找 return 语句
                let mut inferred_return: Option<Type> = None;
                for stmt in &block.statements {
                    if let Stmt::Return(ret_expr_opt) = stmt {
                        if let Some(ret_expr) = ret_expr_opt {
                            let ret_type = self.infer_expr_type_internal(ret_expr)?;
                            inferred_return = Some(ret_type);
                        } else {
                            inferred_return = Some(Type::Void);
                        }
                        break; // 使用第一个 return 语句的类型
                    }
                }
                Box::new(inferred_return.unwrap_or(Type::Void))
            }
        };

        self.symbol_table.exit_scope();

        // 返回完整的函数类型
        Ok(Type::Function(Box::new(crate::types::FunctionType {
            params: param_types,
            return_type,
            is_static: true,
        })))
    }

    /// 推断三元运算符表达式类型
    fn infer_ternary_type(&mut self, ternary: &TernaryExpr) -> cayResult<Type> {
        // 推断条件表达式类型
        let cond_type = self.infer_expr_type_internal(&ternary.condition)?;

        // 条件必须是布尔类型
        if cond_type != Type::Bool {
            return Err(semantic_error_at_loc(
                &ternary.loc,
                format!("Ternary operator condition must be boolean, got {}", cond_type)
            ));
        }

        // 推断两个分支的类型
        let true_type = self.infer_expr_type_internal(&ternary.true_branch)?;
        let false_type = self.infer_expr_type_internal(&ternary.false_branch)?;

        // 两个分支类型必须兼容
        if true_type == false_type {
            Ok(true_type)
        } else if Self::is_numeric_type_helper(&true_type) && Self::is_numeric_type_helper(&false_type) {
            // 数值类型进行类型提升
            Ok(self.promote_types(&true_type, &false_type))
        } else {
            Err(semantic_error_at_loc(
                &ternary.loc,
                format!("Ternary operator branches must have compatible types, got {} and {}", true_type, false_type)
            ))
        }
    }

    /// 推断 instanceof 表达式类型
    fn infer_instanceof_type(&mut self, instanceof: &InstanceOfExpr) -> cayResult<Type> {
        // 检查表达式类型
        let expr_type = self.infer_expr_type_internal(&instanceof.expr)?;

        // 检查目标类型是否存在（类或接口）
        match &instanceof.target_type {
            Type::Object(class_name) => {
                if !self.type_registry.class_exists(class_name) && !self.type_registry.interface_exists(class_name) {
                    return Err(semantic_error_at_loc(
                        &instanceof.loc,
                        format!("Unknown type in instanceof: {}", class_name)
                    ));
                }
            }
            _ => {
                // instanceof 只能用于引用类型
                return Err(semantic_error_at_loc(
                    &instanceof.loc,
                    format!("instanceof can only be used with reference types, got {}", instanceof.target_type)
                ));
            }
        }

        // instanceof 返回布尔类型
        Ok(Type::Bool)
    }

    /// 辅助方法：检查类型是否为数值类型
    fn is_numeric_type_helper(ty: &Type) -> bool {
        matches!(ty, 
            // 内置数值类型
            Type::Int32 | Type::Int64 | Type::Float32 | Type::Float64 | Type::Char |
            // FFI 数值类型
            Type::CInt | Type::CUInt | Type::CLong |
            Type::CShort | Type::CUShort | Type::CChar | Type::CUChar |
            Type::CFloat | Type::CDouble | Type::SizeT | Type::SSizeT |
            Type::UIntPtr | Type::IntPtr
        )
    }

    /// 替换类型中的泛型参数为实际类型
    /// 
    /// 例如：将 GenericParam("T") 替换为 Int32
    fn substitute_type_params(&self, ty: &Type, type_params: &[String], type_args: &[Type]) -> Type {
        match ty {
            Type::GenericParam(name) => {
                // 查找泛型参数在列表中的位置
                if let Some(idx) = type_params.iter().position(|p| p == name) {
                    if idx < type_args.len() {
                        return type_args[idx].clone();
                    }
                }
                ty.clone()
            }
            Type::Array(elem) => {
                Type::Array(Box::new(self.substitute_type_params(elem, type_params, type_args)))
            }
            Type::Generic(name, args) => {
                let new_args = args.iter()
                    .map(|arg| self.substitute_type_params(arg, type_params, type_args))
                    .collect();
                Type::Generic(name.clone(), new_args)
            }
            Type::Function(func_type) => {
                let new_params = func_type.params.iter()
                    .map(|p| self.substitute_type_params(p, type_params, type_args))
                    .collect();
                let new_return = self.substitute_type_params(&func_type.return_type, type_params, type_args);
                Type::Function(Box::new(crate::types::FunctionType {
                    params: new_params,
                    return_type: Box::new(new_return),
                    is_static: func_type.is_static,
                }))
            }
            Type::Pointer(inner) => {
                Type::Pointer(Box::new(self.substitute_type_params(inner, type_params, type_args)))
            }
            _ => ty.clone(),
        }
    }
}
