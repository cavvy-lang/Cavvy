use std::collections::HashMap;
use crate::ast::*;
use crate::types::{Type, ParameterInfo, ClassInfo, MethodInfo, FieldInfo, FunctionType, TypeRegistry};
use crate::error::{EolResult, semantic_error};

pub struct SemanticAnalyzer {
    type_registry: TypeRegistry,
    symbol_table: SemanticSymbolTable,
    current_class: Option<String>,
    current_method: Option<String>,
    errors: Vec<String>,
}

pub struct SemanticSymbolTable {
    scopes: Vec<HashMap<String, SemanticSymbolInfo>>,
}

#[derive(Debug, Clone)]
pub struct SemanticSymbolInfo {
    pub name: String,
    pub symbol_type: Type,
    pub is_final: bool,
    pub is_initialized: bool,
}

impl SemanticSymbolTable {
    pub fn new() -> Self {
        Self { scopes: vec![HashMap::new()] }
    }

    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    pub fn declare(&mut self, name: String, info: SemanticSymbolInfo) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name, info);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<&SemanticSymbolInfo> {
        for scope in self.scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    pub fn lookup_current(&self, name: &str) -> Option<&SemanticSymbolInfo> {
        self.scopes.last().and_then(|s| s.get(name))
    }
}

impl Default for SemanticSymbolTable {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        let mut analyzer = Self {
            type_registry: TypeRegistry::new(),
            symbol_table: SemanticSymbolTable::new(),
            current_class: None,
            current_method: None,
            errors: Vec::new(),
        };
        
        // 注册内置函数
        analyzer.register_builtin_functions();
        
        analyzer
    }

    fn register_builtin_functions(&mut self) {
        // 注册 print 函数 - 作为特殊处理
        // print 可以接受任意类型参数
    }

    pub fn analyze(&mut self, program: &Program) -> EolResult<()> {
        // 第一遍：收集所有类定义
        self.collect_classes(program)?;
        
        // 第二遍：分析方法定义
        self.analyze_methods(program)?;
        
        // 第三遍：类型检查
        self.type_check_program(program)?;
        
        if !self.errors.is_empty() {
            return Err(semantic_error(0, 0, self.errors.join("\n")));
        }
        
        Ok(())
    }

    fn collect_classes(&mut self, program: &Program) -> EolResult<()> {
        for class in &program.classes {
            let class_info = ClassInfo {
                name: class.name.clone(),
                methods: HashMap::new(),
                fields: HashMap::new(),
                parent: class.parent.clone(),
            };
            
            self.type_registry.register_class(class_info)?;
        }
        Ok(())
    }

    fn analyze_methods(&mut self, program: &Program) -> EolResult<()> {
        for class in &program.classes {
            self.current_class = Some(class.name.clone());
            
            for member in &class.members {
                if let ClassMember::Method(method) = member {
                    let method_info = MethodInfo {
                        name: method.name.clone(),
                        class_name: class.name.clone(),
                        params: method.params.clone(),
                        return_type: method.return_type.clone(),
                        is_public: method.modifiers.contains(&Modifier::Public),
                        is_static: method.modifiers.contains(&Modifier::Static),
                        is_native: method.modifiers.contains(&Modifier::Native),
                    };
                    
                    if let Some(class_info) = self.type_registry.classes.get_mut(&class.name) {
                        class_info.methods.insert(method.name.clone(), method_info);
                    }
                }
            }
        }
        Ok(())
    }

    fn type_check_program(&mut self, program: &Program) -> EolResult<()> {
        for class in &program.classes {
            self.current_class = Some(class.name.clone());
            
            for member in &class.members {
                match member {
                    ClassMember::Method(method) => {
                        self.current_method = Some(method.name.clone());
                        self.symbol_table.enter_scope();
                        
                        // 添加参数到符号表
                        for param in &method.params {
                            self.symbol_table.declare(
                                param.name.clone(),
                                SemanticSymbolInfo {
                                    name: param.name.clone(),
                                    symbol_type: param.param_type.clone(),
                                    is_final: false,
                                    is_initialized: true,
                                }
                            );
                        }
                        
                        // 类型检查方法体
                        if let Some(body) = &method.body {
                            self.type_check_statement(&Stmt::Block(body.clone()), Some(&method.return_type))?;
                        }
                        
                        self.symbol_table.exit_scope();
                        self.current_method = None;
                    }
                    ClassMember::Field(_) => {
                        // 字段类型检查暂不实现
                    }
                }
            }
            
            self.current_class = None;
        }
        Ok(())
    }

    fn type_check_statement(&mut self, stmt: &Stmt, expected_return: Option<&Type>) -> EolResult<()> {
        match stmt {
            Stmt::Expr(expr) => {
                self.infer_expr_type(expr)?;
            }
            Stmt::VarDecl(var) => {
                let var_type = var.var_type.clone();
                if let Some(init) = &var.initializer {
                    let init_type = self.infer_expr_type(init)?;
                    if !self.types_compatible(&init_type, &var_type) {
                        self.errors.push(format!(
                            "Cannot assign {} to {} at line {}",
                            init_type, var_type, var.loc.line
                        ));
                    }
                }
                
                self.symbol_table.declare(
                    var.name.clone(),
                    SemanticSymbolInfo {
                        name: var.name.clone(),
                        symbol_type: var_type,
                        is_final: var.is_final,
                        is_initialized: var.initializer.is_some(),
                    }
                );
            }
            Stmt::Return(expr) => {
                let return_type = if let Some(e) = expr {
                    self.infer_expr_type(e)?
                } else {
                    Type::Void
                };
                
                if let Some(expected) = expected_return {
                    if !self.types_compatible(&return_type, expected) {
                        self.errors.push(format!(
                            "Return type mismatch: expected {}, got {}",
                            expected, return_type
                        ));
                    }
                }
            }
            Stmt::Block(block) => {
                self.symbol_table.enter_scope();
                for stmt in &block.statements {
                    self.type_check_statement(stmt, expected_return)?;
                }
                self.symbol_table.exit_scope();
            }
            _ => {}
        }
        
        Ok(())
    }

    fn infer_expr_type(&self, expr: &Expr) -> EolResult<Type> {
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
                } else {
                    Err(semantic_error(0, 0, format!("Undefined variable: {}", name)))
                }
            }
            Expr::Binary(bin) => {
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
            Expr::Unary(unary) => {
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
            Expr::Call(call) => {
                // 特殊处理内置函数
                if let Expr::Identifier(name) = call.callee.as_ref() {
                    // 在这里添加内置输入函数的类型推断
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

                    // 尝试查找当前类的方法（无对象调用）
                    if let Some(current_class) = &self.current_class {
                        if let Some(method_info) = self.type_registry.get_method(current_class, name) {
                            if call.args.len() != method_info.params.len() {
                                return Err(semantic_error(
                                    call.loc.line,
                                    call.loc.column,
                                    format!("Method '{}' expects {} arguments, got {}",
                                        name, method_info.params.len(), call.args.len())
                                ));
                            }

                            for (i, (arg, param)) in call.args.iter().zip(method_info.params.iter()).enumerate() {
                                let arg_type = self.infer_expr_type(arg)?;
                                if !self.types_compatible(&arg_type, &param.param_type) {
                                    return Err(semantic_error(
                                        call.loc.line,
                                        call.loc.column,
                                        format!("Argument {} type mismatch: expected {}, got {}",
                                            i + 1, param.param_type, arg_type)
                                    ));
                                }
                            }

                            return Ok(method_info.return_type.clone());
                        }
                    }
                }

                // 支持成员调用: obj.method(...)
                if let Expr::MemberAccess(member) = call.callee.as_ref() {
                    // 推断对象类型
                    let obj_type = self.infer_expr_type(&member.object)?;
                    if let Type::Object(class_name) = obj_type {
                        if let Some(method_info) = self.type_registry.get_method(&class_name, &member.member) {
                            if call.args.len() != method_info.params.len() {
                                return Err(semantic_error(
                                    call.loc.line,
                                    call.loc.column,
                                    format!("Method '{}' expects {} arguments, got {}",
                                        member.member, method_info.params.len(), call.args.len())
                                ));
                            }

                            for (i, (arg, param)) in call.args.iter().zip(method_info.params.iter()).enumerate() {
                                let arg_type = self.infer_expr_type(arg)?;
                                if !self.types_compatible(&arg_type, &param.param_type) {
                                    return Err(semantic_error(
                                        call.loc.line,
                                        call.loc.column,
                                        format!("Argument {} type mismatch: expected {}, got {}",
                                            i + 1, param.param_type, arg_type)
                                    ));
                                }
                            }

                            return Ok(method_info.return_type.clone());
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
            Expr::MemberAccess(_) => {
                // TODO: 成员访问类型检查
                Ok(Type::Void)
            }
            Expr::New(new_expr) => {
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
            Expr::Assignment(assign) => {
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
            Expr::Cast(cast) => {
                // TODO: 检查转换是否合法
                Ok(cast.target_type.clone())
            }
            Expr::ArrayCreation(arr) => {
                // 数组创建: new Type[size]
                let size_type = self.infer_expr_type(&arr.size)?;
                if !size_type.is_integer() {
                    return Err(semantic_error(
                        arr.loc.line,
                        arr.loc.column,
                        format!("Array size must be integer, got {}", size_type)
                    ));
                }
                Ok(Type::Array(Box::new(arr.element_type.clone())))
            }
            Expr::ArrayAccess(arr) => {
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
        }
    }

    fn types_compatible(&self, from: &Type, to: &Type) -> bool {
        if from == to {
            return true;
        }
        
        // 基本类型之间的兼容
        match (from, to) {
            (Type::Int32, Type::Int64) => true,
            (Type::Int32, Type::Float32) => true,
            (Type::Int32, Type::Float64) => true,
            (Type::Int64, Type::Float64) => true,
            (Type::Float32, Type::Float64) => true,
            (Type::Float64, Type::Float32) => true, // 允许double到float转换（可能有精度损失）
            (Type::Object(_), Type::Object(_)) => true, // TODO: 继承检查
            _ => false,
        }
    }

    fn promote_types(&self, left: &Type, right: &Type) -> Type {
        // 类型提升规则
        match (left, right) {
            (Type::Float64, _) | (_, Type::Float64) => Type::Float64,
            (Type::Float32, _) | (_, Type::Float32) => Type::Float32,
            (Type::Int64, _) | (_, Type::Int64) => Type::Int64,
            (Type::Int32, Type::Int32) => Type::Int32,
            _ => left.clone(),
        }
    }

    fn promote_integer_types(&self, left: &Type, right: &Type) -> Type {
        match (left, right) {
            (Type::Int64, _) | (_, Type::Int64) => Type::Int64,
            _ => Type::Int32,
        }
    }
}
