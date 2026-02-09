//! 表达式类型推断

use crate::ast::*;
use crate::types::Type;
use crate::error::{cayResult, semantic_error};
use super::analyzer::SemanticAnalyzer;
use super::symbol_table::SemanticSymbolInfo;

impl SemanticAnalyzer {
    /// 推断表达式类型
    pub fn infer_expr_type(&mut self, expr: &Expr) -> cayResult<Type> {
        match expr {
            Expr::Literal(lit) => match lit {
                LiteralValue::Int32(_) => Ok(Type::Int32),
                LiteralValue::Int64(_) => Ok(Type::Int64),
                LiteralValue::Float32(_) => Ok(Type::Float32),
                LiteralValue::Float64(_) => Ok(Type::Float64),
                LiteralValue::String(_) => Ok(Type::String),
                LiteralValue::Bool(_) => Ok(Type::Bool),
                LiteralValue::Char(_) => Ok(Type::Char),
                LiteralValue::Null => Ok(Type::Object("Object".to_string())),
            }
            Expr::Identifier(name) => {
                if let Some(info) = self.symbol_table.lookup(name) {
                    Ok(info.symbol_type.clone())
                } else if self.type_registry.class_exists(name) {
                    // 标识符是类名，返回类类型（用于静态成员访问）
                    Ok(Type::Object(name.clone()))
                } else {
                    // 检查是否是当前类的静态字段
                    if let Some(current_class_name) = &self.current_class {
                        if let Some(class_info) = self.type_registry.get_class(current_class_name) {
                            if let Some(field_info) = class_info.fields.get(name) {
                                if field_info.is_static {
                                    return Ok(field_info.field_type.clone());
                                }
                            }
                        }
                    }
                    Err(semantic_error(0, 0, format!("Undefined variable: {}", name)))
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
        }
    }

    /// 推断二元表达式类型
    fn infer_binary_type(&mut self, bin: &BinaryExpr) -> cayResult<Type> {
        let left_type = self.infer_expr_type(&bin.left)?;
        let right_type = self.infer_expr_type(&bin.right)?;
        
        match bin.op {
            BinaryOp::Add => {
                // 字符串连接：两个操作数都必须是字符串
                if left_type == Type::String && right_type == Type::String {
                    Ok(Type::String)
                }
                // 数值加法：两个操作数都必须是基本数值类型
                else if left_type.is_primitive() && right_type.is_primitive() {
                    // 类型提升
                    Ok(self.promote_types(&left_type, &right_type))
                } else {
                    Err(semantic_error(
                        bin.loc.line,
                        bin.loc.column,
                        format!("Cannot add {} and {}: addition requires both operands to be numeric or both to be strings", left_type, right_type)
                    ))
                }
            }
            BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                if left_type.is_primitive() && right_type.is_primitive() {
                    // 类型提升
                    Ok(self.promote_types(&left_type, &right_type))
                } else {
                    Err(semantic_error(
                        bin.loc.line,
                        bin.loc.column,
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
                    Err(semantic_error(
                        bin.loc.line,
                        bin.loc.column,
                        "Logical operators require boolean operands"
                    ))
                }
            }
            BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor => {
                if left_type.is_integer() && right_type.is_integer() {
                    Ok(self.promote_integer_types(&left_type, &right_type))
                } else {
                    Err(semantic_error(
                        bin.loc.line,
                        bin.loc.column,
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
                    Err(semantic_error(
                        bin.loc.line,
                        bin.loc.column,
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
        let operand_type = self.infer_expr_type(&unary.operand)?;
        match unary.op {
            UnaryOp::Neg => Ok(operand_type),
            UnaryOp::Not => {
                if operand_type == Type::Bool {
                    Ok(Type::Bool)
                } else {
                    Err(semantic_error(
                        unary.loc.line,
                        unary.loc.column,
                        "Cannot apply '!' to non-boolean"
                    ))
                }
            }
            UnaryOp::BitNot => Ok(operand_type),
            _ => Ok(operand_type),
        }
    }

    /// 推断函数调用类型
    fn infer_call_type(&mut self, call: &CallExpr) -> cayResult<Type> {
        // 特殊处理内置函数
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
                _ => {}
            }

            // 尝试查找当前类的方法（无对象调用）- 支持方法重载
            if let Some(ref current_class) = self.current_class.clone() {
                // 先推断所有参数类型
                let mut arg_types = Vec::new();
                for arg in &call.args {
                    arg_types.push(self.infer_expr_type(arg)?);
                }

                // 使用参数类型查找匹配的方法
                if let Some(method_info) = self.type_registry.find_method(current_class, name, &arg_types) {
                    let return_type = method_info.return_type.clone();
                    let params = method_info.params.clone();
                    // 检查参数类型兼容性（支持可变参数）
                    if let Err(msg) = self.check_arguments_compatible(&call.args, &params, call.loc.line, call.loc.column) {
                        return Err(semantic_error(call.loc.line, call.loc.column, msg));
                    }

                    return Ok(return_type);
                }
            }
        }

        // 支持成员调用: obj.method(...) 或 ClassName.method()（静态方法）
        if let Expr::MemberAccess(member) = call.callee.as_ref() {
            // 推断对象类型
            let obj_type = self.infer_expr_type(&member.object)?;

            // 处理 String 类型方法调用
            if obj_type == Type::String {
                return self.infer_string_method_call(&member.member, &call.args, call.loc.line, call.loc.column);
            }

            // 检查是否是类名（静态方法调用）- 支持方法重载
            if let Expr::Identifier(class_name) = &*member.object {
                let class_name = class_name.clone();
                // 先推断所有参数类型
                let mut arg_types = Vec::new();
                for arg in &call.args {
                    arg_types.push(self.infer_expr_type(arg)?);
                }

                if let Some(class_info) = self.type_registry.get_class(&class_name) {
                    // 使用参数类型查找匹配的静态方法
                    if let Some(method_info) = class_info.find_method(&member.member, &arg_types) {
                        if method_info.is_static {
                            let return_type = method_info.return_type.clone();
                            let params = method_info.params.clone();
                            // 检查参数类型兼容性（支持可变参数）
                            if let Err(msg) = self.check_arguments_compatible(&call.args, &params, call.loc.line, call.loc.column) {
                                return Err(semantic_error(call.loc.line, call.loc.column, msg));
                            }

                            return Ok(return_type);
                        }
                    }
                }
            }

            // 处理类实例方法调用 - 支持方法重载
            if let Type::Object(class_name) = obj_type {
                // 先推断所有参数类型
                let mut arg_types = Vec::new();
                for arg in &call.args {
                    arg_types.push(self.infer_expr_type(arg)?);
                }

                // 使用参数类型查找匹配的方法
                if let Some(method_info) = self.type_registry.find_method(&class_name, &member.member, &arg_types) {
                    let return_type = method_info.return_type.clone();
                    let params = method_info.params.clone();
                    // 检查参数类型兼容性（支持可变参数）
                    if let Err(msg) = self.check_arguments_compatible(&call.args, &params, call.loc.line, call.loc.column) {
                        return Err(semantic_error(call.loc.line, call.loc.column, msg));
                    }

                    return Ok(return_type);
                } else {
                    return Err(semantic_error(
                        call.loc.line,
                        call.loc.column,
                        format!("Unknown method '{}' for class {}", member.member, class_name)
                    ));
                }
            }
        }

        // 如果找不到任何合适的方法，返回 Void（保持向后兼容）
        Ok(Type::Void)
    }

    /// 推断成员访问类型
    fn infer_member_access_type(&mut self, member: &MemberAccessExpr) -> cayResult<Type> {
        // 检查是否是静态字段访问: ClassName.fieldName
        if let Expr::Identifier(class_name) = &*member.object {
            if let Some(class_info) = self.type_registry.get_class(class_name) {
                if let Some(field_info) = class_info.fields.get(&member.member) {
                    if field_info.is_static {
                        return Ok(field_info.field_type.clone());
                    }
                }
            }
        }

        // 成员访问类型检查
        let obj_type = self.infer_expr_type(&member.object)?;

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

        // 类成员访问
        if let Type::Object(class_name) = obj_type {
            if let Some(class_info) = self.type_registry.get_class(&class_name) {
                if let Some(field_info) = class_info.fields.get(&member.member) {
                    return Ok(field_info.field_type.clone());
                }
            }
            return Err(semantic_error(
                member.loc.line,
                member.loc.column,
                format!("Unknown member '{}' for class {}", member.member, class_name)
            ));
        }

        Err(semantic_error(
            member.loc.line,
            member.loc.column,
            format!("Cannot access member '{}' on type {}", member.member, obj_type)
        ))
    }

    /// 推断 new 表达式类型
    fn infer_new_type(&mut self, new_expr: &NewExpr) -> cayResult<Type> {
        if self.type_registry.class_exists(&new_expr.class_name) {
            Ok(Type::Object(new_expr.class_name.clone()))
        } else {
            Err(semantic_error(
                new_expr.loc.line,
                new_expr.loc.column,
                format!("Unknown class: {}", new_expr.class_name)
            ))
        }
    }

    /// 推断赋值表达式类型
    fn infer_assignment_type(&mut self, assign: &AssignmentExpr) -> cayResult<Type> {
        let target_type = self.infer_expr_type(&assign.target)?;
        let value_type = self.infer_expr_type(&assign.value)?;
        
        if self.types_compatible(&value_type, &target_type) {
            Ok(target_type)
        } else {
            Err(semantic_error(
                assign.loc.line,
                assign.loc.column,
                format!("Cannot assign {} to {}", value_type, target_type)
            ))
        }
    }

    /// 推断类型转换表达式类型
    fn infer_cast_type(&mut self, cast: &CastExpr) -> cayResult<Type> {
        // TODO: 检查转换是否合法
        Ok(cast.target_type.clone())
    }

    /// 推断数组创建表达式类型
    fn infer_array_creation_type(&mut self, arr: &ArrayCreationExpr) -> cayResult<Type> {
        // 数组创建: new Type[size] 或 new Type[size1][size2]...
        // 检查所有维度的大小
        for (i, size) in arr.sizes.iter().enumerate() {
            let size_type = self.infer_expr_type(size)?;
            if !size_type.is_integer() {
                return Err(semantic_error(
                    arr.loc.line,
                    arr.loc.column,
                    format!("Array size at dimension {} must be integer, got {}", i + 1, size_type)
                ));
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
            return Err(semantic_error(
                init.loc.line,
                init.loc.column,
                "Cannot infer type of empty array initializer".to_string()
            ));
        }
        // 推断第一个元素的类型作为数组元素类型
        let elem_type = self.infer_expr_type(&init.elements[0])?;
        Ok(Type::Array(Box::new(elem_type)))
    }

    /// 推断数组访问表达式类型
    fn infer_array_access_type(&mut self, arr: &ArrayAccessExpr) -> cayResult<Type> {
        // 数组访问: arr[index]
        let array_type = self.infer_expr_type(&arr.array)?;
        let index_type = self.infer_expr_type(&arr.index)?;

        if !index_type.is_integer() {
            return Err(semantic_error(
                arr.loc.line,
                arr.loc.column,
                format!("Array index must be integer, got {}", index_type)
            ));
        }

        match array_type {
            Type::Array(element_type) => Ok(*element_type),
            _ => Err(semantic_error(
                arr.loc.line,
                arr.loc.column,
                format!("Cannot index non-array type {}", array_type)
            )),
        }
    }

    /// 推断方法引用表达式类型
    fn infer_method_ref_type(&mut self, method_ref: &MethodRefExpr) -> cayResult<Type> {
        // 方法引用: ClassName::methodName 或 obj::methodName
        // 返回函数类型（这里简化为 Object 类型，实际应该返回函数类型）
        // TODO: 实现完整的函数类型系统
        if let Some(ref class_name) = method_ref.class_name {
            // 检查类是否存在
            if !self.type_registry.class_exists(class_name) {
                return Err(semantic_error(
                    method_ref.loc.line,
                    method_ref.loc.column,
                    format!("Unknown class: {}", class_name)
                ));
            }
            // 检查方法是否存在
            if let Some(class_info) = self.type_registry.get_class(class_name) {
                if !class_info.methods.contains_key(&method_ref.method_name) {
                    return Err(semantic_error(
                        method_ref.loc.line,
                        method_ref.loc.column,
                        format!("Unknown method '{}' for class {}", method_ref.method_name, class_name)
                    ));
                }
            }
        }
        // 方法引用返回 Object 类型（简化处理）
        Ok(Type::Object("Function".to_string()))
    }

    /// 推断 Lambda 表达式类型
    fn infer_lambda_type(&mut self, lambda: &LambdaExpr) -> cayResult<Type> {
        // Lambda 表达式: (params) -> { body }
        // 创建新的作用域
        self.symbol_table.enter_scope();

        // 添加 Lambda 参数到符号表
        for param in &lambda.params {
            let param_type = param.param_type.clone().unwrap_or(Type::Int32);
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
        let body_type = match &lambda.body {
            LambdaBody::Expr(expr) => self.infer_expr_type(expr)?,
            LambdaBody::Block(block) => {
                // 分析块中的语句
                let mut last_type = Type::Void;
                for stmt in &block.statements {
                    // 查找 return 语句来确定返回类型
                    if let Stmt::Return(Some(ret_expr)) = stmt {
                        last_type = self.infer_expr_type(ret_expr)?;
                    }
                }
                last_type
            }
        };

        self.symbol_table.exit_scope();

        // Lambda 表达式返回 Object 类型（简化处理）
        Ok(Type::Object("Function".to_string()))
    }
}
