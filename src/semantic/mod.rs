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

        // 检查主类冲突（在收集类之后，类型检查之前）
        self.check_main_class_conflicts(program)?;

        // 第二遍：分析方法定义
        self.analyze_methods(program)?;

        // 第三遍：类型检查
        self.type_check_program(program)?;

        if !self.errors.is_empty() {
            return Err(semantic_error(0, 0, self.errors.join("\n")));
        }

        Ok(())
    }

    /// 检查主类冲突
    /// 规则：
    /// 1. 如果只有一个类有 main 方法，自动选为主类
    /// 2. 如果有多个类有 main 方法：
    ///    - 如果只有一个类标记了 @main，选该类为主类
    ///    - 如果有多个类标记了 @main，报错
    ///    - 如果没有类标记 @main，报错并提示使用 @main
    fn check_main_class_conflicts(&mut self, program: &Program) -> EolResult<()> {
        // 收集所有有 main 方法的类
        let mut main_classes: Vec<(String, bool)> = Vec::new(); // (类名, 是否有@main标记)

        for class in &program.classes {
            let has_main = class.members.iter().any(|m| {
                if let crate::ast::ClassMember::Method(method) = m {
                    method.name == "main"
                        && method.modifiers.contains(&crate::ast::Modifier::Public)
                        && method.modifiers.contains(&crate::ast::Modifier::Static)
                } else {
                    false
                }
            });

            if has_main {
                let has_main_marker = class.modifiers.contains(&crate::ast::Modifier::Main);
                main_classes.push((class.name.clone(), has_main_marker));
            }
        }

        // 分析冲突
        match main_classes.len() {
            0 => {
                // 没有主类，这是允许的（可能是库文件）
                Ok(())
            }
            1 => {
                // 只有一个主类，没有冲突
                Ok(())
            }
            _ => {
                // 多个类有 main 方法，需要检查 @main 标记
                let marked_classes: Vec<&(String, bool)> = main_classes.iter()
                    .filter(|(_, marked)| *marked)
                    .collect();

                match marked_classes.len() {
                    0 => {
                        // 多个类有 main，但没有标记 @main
                        let class_names: Vec<String> = main_classes.iter()
                            .map(|(name, _)| name.clone())
                            .collect();
                        Err(crate::error::semantic_error(
                            0, 0,
                            format!(
                                "多个类包含 main 方法: {}。请使用 @main 标记指定主类，例如：\n@main public class {} {{ ... }}",
                                class_names.join(", "),
                                class_names[0]
                            )
                        ))
                    }
                    1 => {
                        // 只有一个类标记了 @main，这是正确的
                        Ok(())
                    }
                    _ => {
                        // 多个类标记了 @main
                        let marked_names: Vec<String> = marked_classes.iter()
                            .map(|(name, _)| name.clone())
                            .collect();
                        Err(crate::error::semantic_error(
                            0, 0,
                            format!(
                                "多个类标记了 @main: {}。只能有一个主类。",
                                marked_names.join(", ")
                            )
                        ))
                    }
                }
            }
        }
    }

    /// 获取类型注册表（用于代码生成）
    pub fn get_type_registry(&self) -> &TypeRegistry {
        &self.type_registry
    }

    fn collect_classes(&mut self, program: &Program) -> EolResult<()> {
        for class in &program.classes {
            let mut class_info = ClassInfo {
                name: class.name.clone(),
                methods: HashMap::new(),
                fields: HashMap::new(),
                parent: class.parent.clone(),
            };
            
            // 收集字段信息
            for member in &class.members {
                if let ClassMember::Field(field) = member {
                    let field_info = FieldInfo {
                        name: field.name.clone(),
                        field_type: field.field_type.clone(),
                        is_public: field.modifiers.contains(&Modifier::Public),
                        is_static: field.modifiers.contains(&Modifier::Static),
                    };
                    class_info.fields.insert(field.name.clone(), field_info);
                }
            }
            
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
                        class_info.add_method(method_info);
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

    fn infer_expr_type(&mut self, expr: &Expr) -> EolResult<Type> {
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
            Expr::MemberAccess(member) => {
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
            Expr::ArrayInit(init) => {
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
            Expr::MethodRef(method_ref) => {
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
            Expr::Lambda(lambda) => {
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
    }

    fn types_compatible(&self, from: &Type, to: &Type) -> bool {
        if from == to {
            return true;
        }
        
        // null 可以赋值给任何引用类型（包括 string）
        if let Type::Object(obj_name) = from {
            if obj_name == "Object" {
                // null 是 Object 类型，可以赋值给 String 或其他引用类型
                return true;
            }
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
            // char 可以赋值给 int (ASCII 码值)
            (Type::Char, Type::Int32) => true,
            (Type::Char, Type::Int64) => true,
            _ => false,
        }
    }

    fn promote_types(&self, left: &Type, right: &Type) -> Type {
        // 类型提升规则
        match (left, right) {
            (Type::Float64, _) | (_, Type::Float64) => Type::Float64,
            (Type::Float32, _) | (_, Type::Float32) => Type::Float32,
            (Type::Int64, _) | (_, Type::Int64) => Type::Int64,
            // char 类型在算术运算中提升为 int32
            (Type::Char, Type::Char) => Type::Int32,
            (Type::Char, Type::Int32) | (Type::Int32, Type::Char) => Type::Int32,
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

    /// 检查参数是否与参数定义兼容（支持可变参数）
    fn check_arguments_compatible(&mut self, args: &[Expr], params: &[ParameterInfo], _line: usize, _column: usize) -> Result<(), String> {
        if params.is_empty() {
            if args.is_empty() {
                return Ok(());
            } else {
                return Err(format!("Expected 0 arguments, got {}", args.len()));
            }
        }

        // 检查最后一个参数是否是可变参数
        let last_idx = params.len() - 1;
        if params[last_idx].is_varargs {
            // 可变参数：至少需要 params.len() - 1 个参数
            if args.len() < last_idx {
                return Err(format!("Expected at least {} arguments, got {}", last_idx, args.len()));
            }

            // 检查固定参数
            for i in 0..last_idx {
                let arg_type = self.infer_expr_type(&args[i]).map_err(|e| e.to_string())?;
                if !self.types_compatible(&arg_type, &params[i].param_type) {
                    return Err(format!("Argument {} type mismatch: expected {}, got {}",
                        i + 1, params[i].param_type, arg_type));
                }
            }

            // 检查可变参数
            // 可变参数类型是 Array(ElementType)，需要匹配 ElementType
            let vararg_element_type = match &params[last_idx].param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => &params[last_idx].param_type,
            };
            for i in last_idx..args.len() {
                let arg_type = self.infer_expr_type(&args[i]).map_err(|e| e.to_string())?;
                if !self.types_compatible(&arg_type, vararg_element_type) {
                    return Err(format!("Varargs argument {} type mismatch: expected {}, got {}",
                        i + 1, vararg_element_type, arg_type));
                }
            }
        } else {
            // 非可变参数：参数数量必须完全匹配
            if params.len() != args.len() {
                return Err(format!("Expected {} arguments, got {}", params.len(), args.len()));
            }

            for (i, (arg, param)) in args.iter().zip(params.iter()).enumerate() {
                let arg_type = self.infer_expr_type(arg).map_err(|e| e.to_string())?;
                if !self.types_compatible(&arg_type, &param.param_type) {
                    return Err(format!("Argument {} type mismatch: expected {}, got {}",
                        i + 1, param.param_type, arg_type));
                }
            }
        }

        Ok(())
    }

    /// 推断 String 方法调用的返回类型
    fn infer_string_method_call(&mut self, method_name: &str, args: &[Expr], line: usize, column: usize) -> EolResult<Type> {
        match method_name {
            "length" => {
                if !args.is_empty() {
                    return Err(semantic_error(line, column, "String.length() takes no arguments".to_string()));
                }
                Ok(Type::Int32)
            }
            "substring" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(semantic_error(line, column, "String.substring() takes 1 or 2 arguments".to_string()));
                }
                // 检查参数类型
                for (i, arg) in args.iter().enumerate() {
                    let arg_type = self.infer_expr_type(arg)?;
                    if !arg_type.is_integer() {
                        return Err(semantic_error(line, column, format!("Argument {} of substring() must be integer, got {}", i + 1, arg_type)));
                    }
                }
                Ok(Type::String)
            }
            "indexOf" => {
                if args.len() != 1 {
                    return Err(semantic_error(line, column, "String.indexOf() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type(&args[0])?;
                if arg_type != Type::String {
                    return Err(semantic_error(line, column, format!("Argument of indexOf() must be string, got {}", arg_type)));
                }
                Ok(Type::Int32)
            }
            "charAt" => {
                if args.len() != 1 {
                    return Err(semantic_error(line, column, "String.charAt() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type(&args[0])?;
                if !arg_type.is_integer() {
                    return Err(semantic_error(line, column, format!("Argument of charAt() must be integer, got {}", arg_type)));
                }
                Ok(Type::Char)
            }
            "replace" => {
                if args.len() != 2 {
                    return Err(semantic_error(line, column, "String.replace() takes 2 arguments".to_string()));
                }
                for (i, arg) in args.iter().enumerate() {
                    let arg_type = self.infer_expr_type(arg)?;
                    if arg_type != Type::String {
                        return Err(semantic_error(line, column, format!("Argument {} of replace() must be string, got {}", i + 1, arg_type)));
                    }
                }
                Ok(Type::String)
            }
            _ => Err(semantic_error(line, column, format!("Unknown String method '{}'", method_name))),
        }
    }
}
