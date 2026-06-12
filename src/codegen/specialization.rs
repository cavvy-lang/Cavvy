//! 泛型特化收集器
//!
//! 扫描整个 AST，收集所有泛型类型的实例化信息（如 vector<int>、vector<string>），
//! 为后续的 Monomorphization（单态化）提供特化需求列表。

use crate::ast::*;
use crate::types::Type;
use std::collections::{HashMap, HashSet};

/// 泛型特化实例：类名 + 类型参数列表
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SpecializationInstance {
    /// 基础类名（不含泛型参数），如 "vector"
    pub base_class_name: String,
    /// 命名空间路径，如 ["std"]
    pub namespace_path: Vec<String>,
    /// 类型参数列表，如 [Type::Int32]
    pub type_args: Vec<Type>,
}

impl SpecializationInstance {
    /// 生成特化类名，如 "vector<int>"
    pub fn specialized_name(&self) -> String {
        let base = if self.namespace_path.is_empty() {
            self.base_class_name.clone()
        } else {
            format!(
                "{}::{}",
                self.namespace_path.join("::"),
                self.base_class_name
            )
        };
        if self.type_args.is_empty() {
            base
        } else {
            let args: Vec<String> = self.type_args.iter().map(|t| format!("{}", t)).collect();
            format!("{}<{}>", base, args.join(", "))
        }
    }

    /// 生成 LLVM 友好的特化类名，如 "std__vector_int"
    pub fn llvm_specialized_name(&self) -> String {
        let base = if self.namespace_path.is_empty() {
            self.base_class_name.clone()
        } else {
            format!(
                "{}__{}",
                self.namespace_path.join("__"),
                self.base_class_name
            )
        };
        if self.type_args.is_empty() {
            base
        } else {
            let args: Vec<String> = self
                .type_args
                .iter()
                .map(llvm_type_suffix)
                .collect();
            format!("{}__{}", base, args.join("__"))
        }
    }

    /// 创建类型参数映射 { "T" -> Type::Int32 }
    pub fn type_param_mapping(&self, class_type_params: &[String]) -> HashMap<String, Type> {
        let mut mapping = HashMap::new();
        for (idx, param_name) in class_type_params.iter().enumerate() {
            if let Some(type_arg) = self.type_args.get(idx) {
                mapping.insert(param_name.clone(), type_arg.clone());
            }
        }
        mapping
    }
}

/// 将类型转换为 LLVM 友好的后缀
fn llvm_type_suffix(ty: &Type) -> String {
    match ty {
        Type::Int32 => "i32".to_string(),
        Type::Int64 => "i64".to_string(),
        Type::Float32 => "f32".to_string(),
        Type::Float64 => "f64".to_string(),
        Type::Bool => "bool".to_string(),
        Type::String => "string".to_string(),
        Type::Char => "char".to_string(),
        Type::Object(name) => name.replace("::", "__"),
        Type::Array(inner) => format!("arr_{}", llvm_type_suffix(inner)),
        Type::Generic(base, args) => {
            let args_str: Vec<String> = args.iter().map(llvm_type_suffix).collect();
            format!("{}__{}", base.replace("::", "__"), args_str.join("__"))
        }
        Type::GenericParam(name) => format!("g_{}", name),
        _ => "unknown".to_string(),
    }
}

/// 泛型特化收集器
///
/// 扫描 AST 收集所有泛型实例化点，生成特化需求列表。
#[derive(Debug, Default)]
pub struct SpecializationCollector {
    /// 已收集的特化实例：基础类名 -> 实例集合
    pub instances: HashMap<String, HashSet<SpecializationInstance>>,
    /// 当前命名空间路径
    current_namespace: Vec<String>,
}

impl SpecializationCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从程序中收集所有泛型特化实例
    pub fn collect_from_program(&mut self, program: &Program) {
        // 收集顶层类中的泛型实例化
        for class in &program.classes {
            self.collect_from_class_decl(class);
        }

        // 收集顶层函数中的泛型实例化
        for func in &program.top_level_functions {
            self.collect_from_block(&func.body);
        }

        // 收集命名空间中的泛型实例化
        for ns in &program.namespace_decls {
            self.collect_from_namespace(ns);
        }
    }

    fn collect_from_namespace(&mut self, ns: &NamespaceDecl) {
        let old_ns = self.current_namespace.clone();
        self.current_namespace.extend(ns.path.clone());

        for class in &ns.classes {
            self.collect_from_class_decl(class);
        }
        for func in &ns.top_level_functions {
            self.collect_from_block(&func.body);
        }
        for nested in &ns.nested_namespaces {
            self.collect_from_namespace(nested);
        }

        self.current_namespace = old_ns;
    }

    fn collect_from_class_decl(&mut self, class: &ClassDecl) {
        // 收集类成员中的泛型实例化
        for member in &class.members {
            match member {
                ClassMember::Method(method) => {
                    if let Some(body) = &method.body {
                        self.collect_from_block(body);
                    }
                }
                ClassMember::Constructor(ctor) => {
                    self.collect_from_block(&ctor.body);
                }
                ClassMember::Destructor(dtor) => {
                    self.collect_from_block(&dtor.body);
                }
                ClassMember::InstanceInitializer(block) => {
                    self.collect_from_block(block);
                }
                ClassMember::StaticInitializer(block) => {
                    self.collect_from_block(block);
                }
                _ => {}
            }
        }
    }

    fn collect_from_block(&mut self, block: &Block) {
        for stmt in &block.statements {
            self.collect_from_statement(stmt);
        }
    }

    fn collect_from_statement(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl(var) => {
                // 检查变量类型是否是泛型实例化
                self.collect_type(&var.var_type);
                if let Some(init) = &var.initializer {
                    self.collect_from_expr(init);
                }
            }
            Stmt::Expr(expr) => {
                self.collect_from_expr(expr);
            }
            Stmt::If(if_stmt) => {
                self.collect_from_expr(&if_stmt.condition);
                self.collect_from_statement(&if_stmt.then_branch);
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.collect_from_statement(else_branch);
                }
            }
            Stmt::While(while_stmt) => {
                self.collect_from_expr(&while_stmt.condition);
                self.collect_from_statement(&while_stmt.body);
            }
            Stmt::For(for_stmt) => {
                if let Some(init) = &for_stmt.init {
                    self.collect_from_statement(init);
                }
                if let Some(condition) = &for_stmt.condition {
                    self.collect_from_expr(condition);
                }
                if let Some(update) = &for_stmt.update {
                    self.collect_from_expr(update);
                }
                self.collect_from_statement(&for_stmt.body);
            }
            Stmt::DoWhile(do_while) => {
                self.collect_from_statement(&do_while.body);
                self.collect_from_expr(&do_while.condition);
            }
            Stmt::Return(ret) => {
                if let Some(val) = ret {
                    self.collect_from_expr(val);
                }
            }
            Stmt::Block(block) => {
                self.collect_from_block(block);
            }
            Stmt::Scope(scope) => {
                self.collect_from_block(&scope.body);
            }
            Stmt::Switch(switch) => {
                self.collect_from_expr(&switch.expr);
                for case in &switch.cases {
                    match &case.value {
                        CaseValue::Integer(_) => {}
                        CaseValue::EnumVariant { .. } => {}
                    }
                    for stmt in &case.body {
                        self.collect_from_statement(stmt);
                    }
                }
                if let Some(default) = &switch.default {
                    for stmt in default {
                        self.collect_from_statement(stmt);
                    }
                }
            }
            Stmt::Break(_, _) | Stmt::Continue(_, _) | Stmt::InlineIr(_) => {}
        }
    }

    fn collect_from_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::New(new_expr) => {
                // new vector<int>() -> 收集 vector<int>
                self.collect_generic_class_name(&new_expr.class_name);
                for arg in &new_expr.args {
                    self.collect_from_expr(arg);
                }
            }
            Expr::Call(call) => {
                self.collect_from_expr(&call.callee);
                for arg in &call.args {
                    self.collect_from_expr(arg);
                }
            }
            Expr::MemberAccess(member) => {
                self.collect_from_expr(&member.object);
            }
            Expr::ArrayAccess(arr) => {
                self.collect_from_expr(&arr.array);
                self.collect_from_expr(&arr.index);
            }
            Expr::Assignment(assign) => {
                self.collect_from_expr(&assign.target);
                self.collect_from_expr(&assign.value);
            }
            Expr::Binary(binary) => {
                self.collect_from_expr(&binary.left);
                self.collect_from_expr(&binary.right);
            }
            Expr::Unary(unary) => {
                self.collect_from_expr(&unary.operand);
            }
            Expr::Cast(cast) => {
                self.collect_type(&cast.target_type);
                self.collect_from_expr(&cast.expr);
            }
            Expr::Ternary(ternary) => {
                self.collect_from_expr(&ternary.condition);
                self.collect_from_expr(&ternary.true_branch);
                self.collect_from_expr(&ternary.false_branch);
            }
            Expr::Lambda(lambda) => {
                match &lambda.body {
                    LambdaBody::Expr(expr) => self.collect_from_expr(expr),
                    LambdaBody::Block(block) => self.collect_from_block(block),
                }
            }
            _ => {}
        }
    }

    fn collect_type(&mut self, ty: &Type) {
        // 检查类型是否是泛型实例化
        if let Type::Generic(class_name, type_args) = ty {
            if !type_args.is_empty() {
                // 解析基础类名和命名空间
                let parts: Vec<&str> = class_name.split("::").collect();
                let base_name = parts.last().copied().unwrap_or(class_name.as_str()).to_string();
                let ns_path = if parts.len() > 1 {
                    parts[..parts.len() - 1].iter().map(|s| s.to_string()).collect()
                } else {
                    self.current_namespace.clone()
                };

                let instance = SpecializationInstance {
                    base_class_name: base_name,
                    namespace_path: ns_path,
                    type_args: type_args.clone(),
                };

                let key = class_name.clone();
                self.instances
                    .entry(key)
                    .or_default()
                    .insert(instance);
            }
        }

        // 递归检查嵌套类型
        match ty {
            Type::Array(inner) => self.collect_type(inner),
            Type::Pointer(inner) => self.collect_type(inner),
            Type::Function(func_type) => {
                self.collect_type(&func_type.return_type);
                for param in &func_type.params {
                    self.collect_type(param);
                }
            }
            _ => {}
        }
    }

    /// 从类名字符串中提取泛型实例化
    fn collect_generic_class_name(&mut self, class_name: &str) {
        // 解析类名，如 "std::vector<int>" 或 "vector<int>"
        if let Some(lt_pos) = class_name.find('<') {
            let gt_pos = class_name.rfind('>').unwrap_or(class_name.len());
            let base_name = &class_name[..lt_pos];
            let type_args_str = &class_name[lt_pos + 1..gt_pos];

            // 解析类型参数（简单版本，处理单参数）
            let type_arg = self.parse_type_str(type_args_str.trim());

            let parts: Vec<&str> = base_name.split("::").collect();
            let base = parts.last().copied().unwrap_or(base_name).to_string();
            let ns_path = if parts.len() > 1 {
                parts[..parts.len() - 1].iter().map(|s| s.to_string()).collect()
            } else {
                self.current_namespace.clone()
            };

            let instance = SpecializationInstance {
                base_class_name: base,
                namespace_path: ns_path,
                type_args: vec![type_arg],
            };

            self.instances
                .entry(base_name.to_string())
                .or_default()
                .insert(instance);
        }
    }

    /// 简单类型字符串解析
    fn parse_type_str(&self, s: &str) -> Type {
        match s.trim() {
            "int" => Type::Int32,
            "long" => Type::Int64,
            "float" => Type::Float32,
            "double" => Type::Float64,
            "bool" => Type::Bool,
            "string" | "String" => Type::String,
            "char" => Type::Char,
            t if t.ends_with("[]") => {
                let inner = self.parse_type_str(&t[..t.len() - 2]);
                Type::Array(Box::new(inner))
            }
            t => Type::Object(t.to_string()),
        }
    }

    /// 获取所有特化实例的列表
    pub fn get_all_instances(&self) -> Vec<SpecializationInstance> {
        self.instances
            .values()
            .flat_map(|set| set.iter().cloned())
            .collect()
    }

    /// 检查是否有某个类的特化实例
    pub fn has_specializations(&self, class_name: &str) -> bool {
        self.instances.contains_key(class_name) && !self.instances[class_name].is_empty()
    }

    /// 获取某个类的所有特化实例
    pub fn get_class_instances(&self, class_name: &str) -> Vec<SpecializationInstance> {
        self.instances
            .get(class_name)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }
}
