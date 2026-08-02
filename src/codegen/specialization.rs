//! 泛型特化收集器
//!
//! 扫描整个 AST，收集所有泛型类型的实例化信息（如 vector<int>、vector<string>），
//! 为后续的 Monomorphization（单态化）提供特化需求列表。

use crate::ast::*;
use crate::types::Type;
use std::collections::{HashMap, HashSet};

/// 同类自嵌套全局深度上限。泛型类方法体中出现 `ArrayList<ArrayList<T>>`
/// 时，允许生成比当前实例更深一层的依赖，但不无限展开更深的自嵌套。
/// 该常量同时用于特化收集器与代码生成器，确保二者对“需要生成哪些特化”
/// 的认知一致。
pub(crate) const MAX_SELF_NESTING_DEPTH: usize = 4;

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
            let args: Vec<String> = self
                .type_args
                .iter()
                .map(|t| t.display_name())
                .collect();
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
            let args: Vec<String> = self.type_args.iter().map(llvm_type_suffix).collect();
            format!("{}__{}", base, args.join("__"))
        }
    }

    /// 创建类型参数映射 { "T" -> Type::Int32 }
    pub fn type_param_mapping(
        &self,
        class_type_params: &[crate::types::TypeParamInfo],
    ) -> HashMap<String, Type> {
        let resolved = self.resolve_type_args(class_type_params);
        let mut mapping = HashMap::new();
        for (param, type_arg) in class_type_params.iter().zip(resolved.iter()) {
            mapping.insert(param.name.clone(), type_arg.clone());
        }
        mapping
    }

    /// 解析最终类型参数列表，缺失时使用默认值填充
    pub fn resolve_type_args(
        &self,
        class_type_params: &[crate::types::TypeParamInfo],
    ) -> Vec<Type> {
        let mut result = Vec::with_capacity(class_type_params.len());
        for (idx, param) in class_type_params.iter().enumerate() {
            if let Some(type_arg) = self.type_args.get(idx) {
                result.push(type_arg.clone());
            } else if let Some(default) = &param.default_type {
                result.push(default.clone());
            } else {
                // 没有显式参数也没有默认值，保留占位符 GenericParam
                result.push(Type::GenericParam(param.name.clone()));
            }
        }
        result
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
        Type::String => "String".to_string(),
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
    /// 泛型类声明及其命名空间（用于收集依赖特化）
    generic_classes: Vec<(ClassDecl, Vec<String>)>,
    /// 泛型 struct 声明及其命名空间（用于收集依赖特化）
    generic_structs: Vec<(StructDecl, Vec<String>)>,
    /// 泛型 enum 声明及其命名空间（用于收集依赖特化）
    generic_enums: Vec<(EnumDecl, Vec<String>)>,
    /// 当前正在扫描的泛型类/struct/enum 的类型参数名（在作用域内）。
    /// 用于跳过尚未替换的泛型实例（如泛型类体内的 `new Other<T>()`），
    /// 这些实例会在 `collect_dependency_specializations` 中被替换为具体类型后再收集。
    type_param_scope: std::collections::HashSet<String>,
    /// 依赖特化收集阶段：当前正在处理的泛型类基础名。
    /// 用于抑制同类自嵌套导致的无限展开（如 ArrayList<T> 的方法体中出现
    /// ArrayList<ArrayList<T>> 会不断生成更深的 ArrayList<...> 特化）。
    dep_class_base: Option<String>,
    /// 依赖特化收集阶段：当前类实例允许的最大自嵌套深度。
    dep_max_depth: usize,
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

        // 收集顶层 struct / enum 中的泛型实例化
        for struct_decl in &program.structs {
            self.collect_from_struct_decl(struct_decl);
        }
        for enum_decl in &program.enums {
            self.collect_from_enum_decl(enum_decl);
        }

        // 收集顶层函数中的泛型实例化
        for func in &program.top_level_functions {
            self.collect_from_block(&func.body);
        }

        // 收集命名空间中的泛型实例化
        for ns in &program.namespace_decls {
            self.collect_from_namespace(ns);
        }

        // 收集依赖特化：泛型类/struct/enum 方法体中的 `new Other<T>()` 需要为每个特化实例
        // 生成 `Other<Concrete>`。
        self.collect_dependency_specializations();
    }

    fn collect_from_namespace(&mut self, ns: &NamespaceDecl) {
        let old_ns = self.current_namespace.clone();
        self.current_namespace.extend(ns.path.clone());

        for class in &ns.classes {
            self.collect_from_class_decl(class);
        }
        for struct_decl in &ns.structs {
            self.collect_from_struct_decl(struct_decl);
        }
        for enum_decl in &ns.enums {
            self.collect_from_enum_decl(enum_decl);
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
        // 预处理后的类既可能保留在 NamespaceDecl 中，也可能已经扁平化到
        // Program.classes。后者仍通过 ClassDecl::namespace_path 保存限定名；若只
        // 使用遍历器当前的命名空间，泛型类体内的依赖特化会被登记成裸类名。
        let old_namespace = self.current_namespace.clone();
        if !class.namespace_path.is_empty() {
            self.current_namespace = class.namespace_path.clone();
        }

        // 记录泛型类，用于后续依赖特化收集。
        if !class.type_params.is_empty() {
            self.generic_classes
                .push((class.clone(), self.current_namespace.clone()));
        }

        // 将本类的类型参数加入作用域，扫描类体时用于跳过未替换的泛型实例。
        let old_scope = self.type_param_scope.clone();
        for tp in &class.type_params {
            self.type_param_scope.insert(tp.name.clone());
        }

        // 收集类成员中的泛型实例化
        for member in &class.members {
            match member {
                ClassMember::Method(method) => {
                    if let Some(body) = &method.body {
                        // 方法级类型参数（如 map<U> 的 U）加入类型参数作用域，
                        // 避免收集 Result<U, E> 这类只有调用点才能具体化的实例。
                        // 这些实例由代码生成阶段的懒单态化按需生成。
                        let old_method_scope = self.type_param_scope.clone();
                        for tp in &method.type_params {
                            self.type_param_scope.insert(tp.name.clone());
                        }
                        self.collect_from_block(body);
                        self.type_param_scope = old_method_scope;
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

        self.type_param_scope = old_scope;
        self.current_namespace = old_namespace;
    }

    fn collect_from_struct_decl(&mut self, struct_decl: &StructDecl) {
        let old_namespace = self.current_namespace.clone();
        if !struct_decl.namespace_path.is_empty() {
            self.current_namespace = struct_decl.namespace_path.clone();
        }

        // 记录泛型 struct，用于后续依赖特化收集。
        if !struct_decl.type_params.is_empty() {
            self.generic_structs
                .push((struct_decl.clone(), self.current_namespace.clone()));
        }

        // 将本 struct 的类型参数加入作用域。
        let old_scope = self.type_param_scope.clone();
        for tp in &struct_decl.type_params {
            self.type_param_scope.insert(tp.name.clone());
        }

        // 收集 struct 字段类型、构造函数和方法体中的泛型实例化
        for field in &struct_decl.fields {
            self.collect_type(&field.field_type);
        }
        for ctor in &struct_decl.constructors {
            for param in &ctor.params {
                self.collect_type(&param.param_type);
            }
            self.collect_from_block(&ctor.body);
        }
        for method in &struct_decl.methods {
            for param in &method.params {
                self.collect_type(&param.param_type);
            }
            self.collect_type(&method.return_type);
            if let Some(body) = &method.body {
                // 同类的泛型方法：方法级类型参数加入作用域，跳过未替换实例。
                let old_method_scope = self.type_param_scope.clone();
                for tp in &method.type_params {
                    self.type_param_scope.insert(tp.name.clone());
                }
                self.collect_from_block(body);
                self.type_param_scope = old_method_scope;
            }
        }

        self.type_param_scope = old_scope;
        self.current_namespace = old_namespace;
    }

    fn collect_from_enum_decl(&mut self, enum_decl: &EnumDecl) {
        let old_namespace = self.current_namespace.clone();
        if !enum_decl.namespace_path.is_empty() {
            self.current_namespace = enum_decl.namespace_path.clone();
        }

        // 记录泛型 enum，用于后续依赖特化收集。
        if !enum_decl.type_params.is_empty() {
            self.generic_enums
                .push((enum_decl.clone(), self.current_namespace.clone()));
        }

        // 将本 enum 的类型参数加入作用域。
        let old_scope = self.type_param_scope.clone();
        for tp in &enum_decl.type_params {
            self.type_param_scope.insert(tp.name.clone());
        }

        // 收集 enum variant payload 类型中的泛型实例化
        for variant in &enum_decl.variants {
            if let Some(payload_type) = &variant.payload_type {
                self.collect_type(payload_type);
            }
        }

        self.type_param_scope = old_scope;
        self.current_namespace = old_namespace;
    }

    /// 判断类型是否引用了当前作用域内尚未替换的类型参数。
    fn references_scoped_type_param(&self, ty: &Type) -> bool {
        match ty {
            Type::GenericParam(name) => self.type_param_scope.contains(name),
            // 解析器可能将裸类型参数表示为 Object("T")
            Type::Object(name) => self.type_param_scope.contains(name),
            Type::Array(inner) | Type::Pointer(inner) => self.references_scoped_type_param(inner),
            Type::Generic(_, args) => args.iter().any(|a| self.references_scoped_type_param(a)),
            Type::Function(func_type) => {
                self.references_scoped_type_param(&func_type.return_type)
                    || func_type
                        .params
                        .iter()
                        .any(|p| self.references_scoped_type_param(p))
            }
            _ => false,
        }
    }

    fn collect_from_block(&mut self, block: &Block) {
        for stmt in &block.statements {
            self.collect_from_statement(stmt);
        }
        if let Some(tail) = &block.tail_expr {
            self.collect_from_expr(tail);
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
            Stmt::ForEach(for_each) => {
                self.collect_type(&for_each.var_type);
                self.collect_from_expr(&for_each.iterable);
                self.collect_from_statement(&for_each.body);
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
                // 检查 object 是否是泛型类型标识，如 FileResult<File>
                if let Expr::Identifier(id) = &*member.object {
                    self.collect_generic_class_name(&id.name);
                }
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
            Expr::If(if_expr) => {
                self.collect_from_expr(&if_expr.condition);
                self.collect_from_block(&if_expr.then_branch);
                self.collect_from_block(&if_expr.else_branch);
            }
            Expr::Lambda(lambda) => match &lambda.body {
                LambdaBody::Expr(expr) => self.collect_from_expr(expr),
                LambdaBody::Block(block) => self.collect_from_block(block),
            },
            _ => {}
        }
    }

    fn collect_type(&mut self, ty: &Type) {
        // 检查类型是否是泛型实例化
        if let Type::Generic(class_name, type_args) = ty {
            if !type_args.is_empty() {
                // 跳过引用了作用域内类型参数的实例（如泛型类体内的 `Other<T>`）。
                // 这些会在依赖特化阶段替换为具体类型后再收集。
                if type_args
                    .iter()
                    .any(|a| self.references_scoped_type_param(a))
                {
                    // 仍需递归检查嵌套类型（下方），但不记录本实例
                } else {
                    // 解析基础类名和命名空间
                    let parts: Vec<&str> = class_name.split("::").collect();
                    let base_name = parts
                        .last()
                        .copied()
                        .unwrap_or(class_name.as_str())
                        .to_string();
                    let ns_path = if parts.len() > 1 {
                        parts[..parts.len() - 1]
                            .iter()
                            .map(|s| s.to_string())
                            .collect()
                    } else {
                        self.current_namespace.clone()
                    };

                    let instance = SpecializationInstance {
                        base_class_name: base_name,
                        namespace_path: ns_path,
                        type_args: type_args.clone(),
                    };

                    let key = class_name.clone();
                    self.instances.entry(key).or_default().insert(instance);
                }
            }
        }

        // 递归检查嵌套类型（包括泛型类型实参中的嵌套泛型实例，
        // 如 Boxed<Pair<int, String>> 需要同时收集 Pair<int, String>）。
        match ty {
            Type::Array(inner) => self.collect_type(inner),
            Type::Pointer(inner) => self.collect_type(inner),
            Type::Function(func_type) => {
                self.collect_type(&func_type.return_type);
                for param in &func_type.params {
                    self.collect_type(param);
                }
            }
            Type::Generic(_, type_args) => {
                for arg in type_args {
                    self.collect_type(arg);
                }
            }
            _ => {}
        }
    }

    /// 从类名字符串中提取泛型实例化
    fn collect_generic_class_name(&mut self, class_name: &str) {
        // 解析类名，如 "std::vector<int>" 或 "map<string, int>"
        if let Some(lt_pos) = class_name.find('<') {
            let gt_pos = class_name.rfind('>').unwrap_or(class_name.len());
            let base_name = &class_name[..lt_pos];
            let type_args_str = &class_name[lt_pos + 1..gt_pos];

            // 按顶层逗号拆分多个类型参数（如 "string, int"）
            let type_args: Vec<Type> = split_top_level_type_args(type_args_str)
                .iter()
                .map(|s| parse_type_str(s.trim()))
                .collect();

            if type_args.is_empty() {
                return;
            }

            // 跳过引用了作用域内类型参数的实例；依赖特化阶段会替换为具体类型后再收集。
            if type_args
                .iter()
                .any(|a| self.references_scoped_type_param(a))
            {
                return;
            }

            let parts: Vec<&str> = base_name.split("::").collect();
            let base = parts.last().copied().unwrap_or(base_name).to_string();
            let ns_path = if parts.len() > 1 {
                parts[..parts.len() - 1]
                    .iter()
                    .map(|s| s.to_string())
                    .collect()
            } else {
                self.current_namespace.clone()
            };

            // 递归收集嵌套泛型实参（如 Boxed<Pair<int, String>> 中的 Pair<int, String>）。
            for arg in &type_args {
                self.collect_type(arg);
            }

            let instance = SpecializationInstance {
                base_class_name: base,
                namespace_path: ns_path,
                type_args,
            };

            self.instances
                .entry(base_name.to_string())
                .or_default()
                .insert(instance);
        }
    }

    /// 收集依赖特化。
    ///
    /// 当泛型类 `Foo<T>` 的方法体中出现 `new Bar<T>()` 时，需要为 `Foo` 的每个特化
    /// 实例（如 `Foo<int>`）生成对应的 `Bar<int>` 特化。此步骤在第一遍收集完成后
    /// 执行，将方法体中的类型参数替换为实例的具体类型并收集。
    fn collect_dependency_specializations(&mut self) {
        let generic_classes = self.generic_classes.clone();
        let generic_structs = self.generic_structs.clone();

        // 迭代至定点：某个特化实例（如 ArrayList<int>）可能在被收集后，
        // 其自身方法体又依赖其他泛型实例（如 ArrayListIterator<int>）。
        // 单遍处理只能展开一层依赖，故反复处理直到不再产生新实例。
        // 上限用于防御性地避免病态输入导致的死循环。
        const MAX_ITERATIONS: usize = 32;
        for _ in 0..MAX_ITERATIONS {
            let before = self.total_instance_count();

            for (class, ns) in &generic_classes {
                // 合并以 "std::ArrayList" 与裸名 "ArrayList" 两种 key 收集到的实例。
                // 源码中可能混用命名空间全名与裸名（如类体内部使用 ArrayList<T>，
                // 用户代码使用 std::ArrayList<T>），合并后确保所有实例的方法体都被
                // 扫描到，避免深层嵌套泛型的依赖特化遗漏。
                let instances = self.instances_for_generic_class(&class.name, ns);

                if instances.is_empty() {
                    continue;
                }

                let type_param_infos: Vec<crate::types::TypeParamInfo> = class
                    .type_params
                    .iter()
                    .map(|p| crate::types::TypeParamInfo {
                        name: p.name.clone(),
                        bound: p.bound.clone(),
                        default_type: p.default_type.clone(),
                    })
                    .collect();

                let class_base_name = class.name.split("::").last().unwrap_or(&class.name).to_string();

                self.dep_class_base = Some(class_base_name.clone());
                self.dep_max_depth = MAX_SELF_NESTING_DEPTH;

                for instance in instances {
                    let mapping = instance.type_param_mapping(&type_param_infos);

                    for member in &class.members {
                        let body_opt = match member {
                            ClassMember::Method(method) => method.body.as_ref(),
                            ClassMember::Constructor(ctor) => Some(&ctor.body),
                            ClassMember::Destructor(dtor) => Some(&dtor.body),
                            ClassMember::InstanceInitializer(block) => Some(block),
                            ClassMember::StaticInitializer(block) => Some(block),
                            _ => None,
                        };
                        if let Some(body) = body_opt {
                            let method_name = match member {
                                ClassMember::Method(m) => m.name.clone(),
                                ClassMember::Constructor(_) => "<ctor>".to_string(),
                                ClassMember::Destructor(_) => "<dtor>".to_string(),
                                _ => "<init>".to_string(),
                            };
                            let _ = &method_name;
                            // 方法级类型参数（如 map<U> 的 U）不在类实例映射中，
                            // 加入作用域以跳过 Result<U, E> 等未替换实例——
                            // 这些由代码生成阶段的懒单态化按需生成。
                            let old_method_scope = self.type_param_scope.clone();
                            if let ClassMember::Method(m) = member {
                                for tp in &m.type_params {
                                    self.type_param_scope.insert(tp.name.clone());
                                }
                            }
                            self.collect_dependency_from_block(body, &mapping, ns);
                            self.type_param_scope = old_method_scope;
                        }
                    }

                }

                self.dep_class_base = None;
                self.dep_max_depth = 0;
            }

            // 泛型 struct 的依赖特化：方法体中引用的泛型类型需按实例替换。
            for (struct_decl, ns) in &generic_structs {
                let instances = self.instances_for_generic_class(&struct_decl.name, ns);

                if instances.is_empty() {
                    continue;
                }

                let type_param_infos: Vec<crate::types::TypeParamInfo> = struct_decl
                    .type_params
                    .iter()
                    .map(|p| crate::types::TypeParamInfo {
                        name: p.name.clone(),
                        bound: p.bound.clone(),
                        default_type: p.default_type.clone(),
                    })
                    .collect();

                let struct_base_name = struct_decl.name.split("::").last().unwrap_or(&struct_decl.name).to_string();
                self.dep_class_base = Some(struct_base_name.clone());
                self.dep_max_depth = MAX_SELF_NESTING_DEPTH;

                for instance in instances {
                    let mapping = instance.type_param_mapping(&type_param_infos);
                    for method in &struct_decl.methods {
                        for param in &method.params {
                            let substituted = crate::types::substitute_type_params(&param.param_type, &mapping);
                            self.collect_type_with_ns(&substituted, ns);
                        }
                        let substituted = crate::types::substitute_type_params(&method.return_type, &mapping);
                        self.collect_type_with_ns(&substituted, ns);
                        if let Some(body) = &method.body {
                            // 方法级类型参数加入作用域，跳过未替换实例（同上）。
                            let old_method_scope = self.type_param_scope.clone();
                            for tp in &method.type_params {
                                self.type_param_scope.insert(tp.name.clone());
                            }
                            self.collect_dependency_from_block(body, &mapping, ns);
                            self.type_param_scope = old_method_scope;
                        }
                    }
                }

                self.dep_class_base = None;
                self.dep_max_depth = 0;
            }

            // 本轮未产生新实例则已到达定点，停止迭代。
            if self.total_instance_count() == before {
                break;
            }
        }

    }

    /// 返回某个泛型基类（含命名空间前缀与裸名两种 key）对应的所有特化实例。
    fn instances_for_generic_class(&self, class_name: &str, ns: &[String]) -> Vec<SpecializationInstance> {
        let class_key = if ns.is_empty() {
            class_name.to_string()
        } else {
            format!("{}::{}", ns.join("::"), class_name)
        };

        let mut result = Vec::new();
        if let Some(set) = self.instances.get(&class_key) {
            result.extend(set.iter().cloned());
        }
        if let Some(set) = self.instances.get(class_name) {
            result.extend(set.iter().cloned());
        }
        result
    }

    fn collect_type_with_ns(&mut self, ty: &Type, ns: &[String]) {
        let old_ns = self.current_namespace.clone();
        self.current_namespace = ns.to_vec();
        self.collect_type(ty);
        self.current_namespace = old_ns;
    }

    /// 统计当前已收集的特化实例总数（用于依赖收集的定点判断）。
    fn total_instance_count(&self) -> usize {
        self.instances.values().map(|set| set.len()).sum()
    }

    fn collect_dependency_from_block(
        &mut self,
        block: &Block,
        mapping: &std::collections::HashMap<String, Type>,
        ns: &[String],
    ) {
        for stmt in &block.statements {
            self.collect_dependency_from_statement(stmt, mapping, ns);
        }
        if let Some(tail) = &block.tail_expr {
            self.collect_dependency_from_expr(tail, mapping, ns);
        }
    }

    fn collect_dependency_from_statement(
        &mut self,
        stmt: &Stmt,
        mapping: &std::collections::HashMap<String, Type>,
        ns: &[String],
    ) {
        match stmt {
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => {
                self.collect_dependency_from_expr(expr, mapping, ns);
            }
            Stmt::VarDecl(var) => {
                if let Some(init) = &var.initializer {
                    self.collect_dependency_from_expr(init, mapping, ns);
                }
            }
            Stmt::If(if_stmt) => {
                self.collect_dependency_from_expr(&if_stmt.condition, mapping, ns);
                self.collect_dependency_from_statement(&if_stmt.then_branch, mapping, ns);
                if let Some(else_branch) = &if_stmt.else_branch {
                    self.collect_dependency_from_statement(else_branch, mapping, ns);
                }
            }
            Stmt::While(while_stmt) => {
                self.collect_dependency_from_expr(&while_stmt.condition, mapping, ns);
                self.collect_dependency_from_statement(&while_stmt.body, mapping, ns);
            }
            Stmt::DoWhile(do_while) => {
                self.collect_dependency_from_statement(&do_while.body, mapping, ns);
                self.collect_dependency_from_expr(&do_while.condition, mapping, ns);
            }
            Stmt::For(for_stmt) => {
                if let Some(init) = &for_stmt.init {
                    self.collect_dependency_from_statement(init, mapping, ns);
                }
                if let Some(cond) = &for_stmt.condition {
                    self.collect_dependency_from_expr(cond, mapping, ns);
                }
                if let Some(update) = &for_stmt.update {
                    self.collect_dependency_from_expr(update, mapping, ns);
                }
                self.collect_dependency_from_statement(&for_stmt.body, mapping, ns);
            }
            Stmt::ForEach(for_each) => {
                self.collect_dependency_from_expr(&for_each.iterable, mapping, ns);
                self.collect_dependency_from_statement(&for_each.body, mapping, ns);
            }
            Stmt::Switch(switch) => {
                self.collect_dependency_from_expr(&switch.expr, mapping, ns);
                for case in &switch.cases {
                    for stmt in &case.body {
                        self.collect_dependency_from_statement(stmt, mapping, ns);
                    }
                }
                if let Some(default) = &switch.default {
                    for stmt in default {
                        self.collect_dependency_from_statement(stmt, mapping, ns);
                    }
                }
            }
            Stmt::Block(block) => {
                self.collect_dependency_from_block(block, mapping, ns);
            }
            _ => {}
        }
    }

    fn collect_dependency_from_expr(
        &mut self,
        expr: &Expr,
        mapping: &std::collections::HashMap<String, Type>,
        ns: &[String],
    ) {
        match expr {
            Expr::New(new_expr) => {
                let substituted = substitute_type_args_in_class_name(&new_expr.class_name, mapping);
                self.collect_generic_class_name_with_ns(&substituted, ns);
                for arg in &new_expr.args {
                    self.collect_dependency_from_expr(arg, mapping, ns);
                }
            }
            Expr::Call(call) => {
                self.collect_dependency_from_expr(&call.callee, mapping, ns);
                for arg in &call.args {
                    self.collect_dependency_from_expr(arg, mapping, ns);
                }
            }
            Expr::MemberAccess(member) => {
                self.collect_dependency_from_expr(&member.object, mapping, ns);
                // 处理静态泛型方法调用，如 `Optional<Rc<T>>.of(result)`：
                // object 是带泛型实参的类标识符，需替换类型参数后收集依赖特化。
                if let Expr::Identifier(id) = &*member.object {
                    let substituted = substitute_type_args_in_class_name(&id.name, mapping);
                    self.collect_generic_class_name_with_ns(&substituted, ns);
                }
            }
            Expr::ArrayAccess(arr) => {
                self.collect_dependency_from_expr(&arr.array, mapping, ns);
                self.collect_dependency_from_expr(&arr.index, mapping, ns);
            }
            Expr::Assignment(assign) => {
                self.collect_dependency_from_expr(&assign.target, mapping, ns);
                self.collect_dependency_from_expr(&assign.value, mapping, ns);
            }
            Expr::Binary(binary) => {
                self.collect_dependency_from_expr(&binary.left, mapping, ns);
                self.collect_dependency_from_expr(&binary.right, mapping, ns);
            }
            Expr::Unary(unary) => {
                self.collect_dependency_from_expr(&unary.operand, mapping, ns);
            }
            Expr::Cast(cast) => {
                self.collect_dependency_from_expr(&cast.expr, mapping, ns);
            }
            Expr::Ternary(ternary) => {
                self.collect_dependency_from_expr(&ternary.condition, mapping, ns);
                self.collect_dependency_from_expr(&ternary.true_branch, mapping, ns);
                self.collect_dependency_from_expr(&ternary.false_branch, mapping, ns);
            }
            Expr::If(if_expr) => {
                self.collect_dependency_from_expr(&if_expr.condition, mapping, ns);
                self.collect_dependency_from_block(&if_expr.then_branch, mapping, ns);
                self.collect_dependency_from_block(&if_expr.else_branch, mapping, ns);
            }
            Expr::Lambda(lambda) => match &lambda.body {
                LambdaBody::Expr(expr) => self.collect_dependency_from_expr(expr, mapping, ns),
                LambdaBody::Block(block) => self.collect_dependency_from_block(block, mapping, ns),
            },
            _ => {}
        }
    }

    fn collect_generic_class_name_with_ns(&mut self, class_name: &str, ns: &[String]) {
        // 依赖特化阶段：抑制同类自嵌套超过当前实例深度，避免泛型类方法体中
        // 出现 `ArrayList<ArrayList<T>>` 时无限生成更深的特化实例。
        if let Some(ref base) = self.dep_class_base {
            if let Some(lt_pos) = class_name.find('<') {
                let gt_pos = class_name.rfind('>').unwrap_or(class_name.len());
                let candidate_base = class_name[..lt_pos]
                    .split("::")
                    .last()
                    .unwrap_or(&class_name[..lt_pos]);
                if candidate_base == base {
                    let args_str = &class_name[lt_pos + 1..gt_pos];
                    let args: Vec<Type> = split_top_level_type_args(args_str)
                        .iter()
                        .map(|s| parse_type_str(s.trim()))
                        .collect();
                    let depth = 1 + args
                        .iter()
                        .map(|a| self.nesting_depth(base, a))
                        .max()
                        .unwrap_or(0);
                    if depth > self.dep_max_depth {
                        return;
                    }
                }
            }
        }

        let old_ns = self.current_namespace.clone();
        self.current_namespace = ns.to_vec();
        self.collect_generic_class_name(class_name);
        self.current_namespace = old_ns;
    }

    /// 计算类型 `ty` 中类 `base` 的最大自嵌套深度。
    ///
    /// 例如 `ArrayList<int>` 深度为 1，`ArrayList<ArrayList<int>>` 深度为 2。
    fn nesting_depth(&self, base: &str, ty: &Type) -> usize {
        match ty {
            Type::Generic(name, args) => {
                let candidate = name.split("::").last().unwrap_or(name);
                let inner_max = args
                    .iter()
                    .map(|a| self.nesting_depth(base, a))
                    .max()
                    .unwrap_or(0);
                if candidate == base {
                    1 + inner_max
                } else {
                    inner_max
                }
            }
            Type::Array(inner) | Type::Pointer(inner) => self.nesting_depth(base, inner),
            Type::Function(ft) => std::cmp::max(
                self.nesting_depth(base, &ft.return_type),
                ft.params
                    .iter()
                    .map(|p| self.nesting_depth(base, p))
                    .max()
                    .unwrap_or(0),
            ),
            _ => 0,
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

/// 将类型参数字符串按顶层逗号拆分，正确处理嵌套泛型。
///
/// 例如 `"string, map<int, int>"` 拆分为 `["string", "map<int, int>"]`。
pub(crate) fn split_top_level_type_args(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut depth = 0i32;
    let mut current = String::new();
    for ch in s.chars() {
        match ch {
            '<' => {
                depth += 1;
                current.push(ch);
            }
            '>' => {
                depth -= 1;
                current.push(ch);
            }
            ',' if depth == 0 => {
                if !current.trim().is_empty() {
                    result.push(current.trim().to_string());
                }
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}

/// 简单解析类型字符串（用于从类名字符串中提取泛型实参）。
///
/// 支持基本类型别名、数组类型以及泛型类型。无法识别的标识符视为 Object。
pub(crate) fn parse_type_str(s: &str) -> Type {
    let s = s.trim();
    if s.ends_with("[]") {
        return Type::Array(Box::new(parse_type_str(&s[..s.len() - 2])));
    }
    if let Some(lt_pos) = s.find('<') {
        let gt_pos = s.rfind('>').unwrap_or(s.len());
        let base = s[..lt_pos].trim().to_string();
        let args_str = &s[lt_pos + 1..gt_pos];
        let args: Vec<Type> = split_top_level_type_args(args_str)
            .iter()
            .map(|part| parse_type_str(part.trim()))
            .collect();
        return Type::Generic(base, args);
    }
    match s {
        "int" => Type::Int32,
        "long" => Type::Int64,
        "float" => Type::Float32,
        "double" => Type::Float64,
        "bool" | "boolean" => Type::Bool,
        "string" | "String" => Type::String,
        "char" => Type::Char,
        _ => Type::Object(s.to_string()),
    }
}

/// 将类名字符串中的泛型类型参数按 mapping 替换为具体类型。
///
/// 例如 `ArrayListIterator<T>` 在 mapping `{T -> int}` 下替换为
/// `ArrayListIterator<int>`；嵌套泛型如 `Optional<Rc<T>>` 会递归替换为
/// `Optional<Rc<Tracked>>`。
pub(crate) fn substitute_type_args_in_class_name(
    class_name: &str,
    mapping: &std::collections::HashMap<String, Type>,
) -> String {
    let Some(lt_pos) = class_name.find('<') else {
        return class_name.to_string();
    };
    let gt_pos = class_name.rfind('>').unwrap_or(class_name.len());
    let base = &class_name[..lt_pos];
    let args_str = &class_name[lt_pos + 1..gt_pos];
    let args: Vec<String> = split_top_level_type_args(args_str)
        .iter()
        .map(|s| s.trim().to_string())
        .collect();
    let substituted: Vec<String> = args
        .iter()
        .map(|arg| substitute_type_arg_str(arg, mapping))
        .collect();
    format!("{}<{}>", base, substituted.join(", "))
}

/// 递归替换单个类型实参字符串中的类型参数。
fn substitute_type_arg_str(
    arg: &str,
    mapping: &std::collections::HashMap<String, Type>,
) -> String {
    let arg = arg.trim();

    // 数组类型：递归替换元素类型
    if arg.ends_with("[]") {
        let inner = substitute_type_arg_str(&arg[..arg.len() - 2], mapping);
        return format!("{}[]", inner);
    }

    // 泛型类型：递归替换每个类型实参
    if let Some(lt_pos) = arg.find('<') {
        let gt_pos = arg.rfind('>').unwrap_or(arg.len());
        if lt_pos < gt_pos {
            let base = &arg[..lt_pos];
            let inner = &arg[lt_pos + 1..gt_pos];
            let parts: Vec<String> = split_top_level_type_args(inner)
                .iter()
                .map(|s| substitute_type_arg_str(s.trim(), mapping))
                .collect();
            return format!("{}<{}>", base, parts.join(", "));
        }
    }

    // 简单类型参数：直接查 mapping
    if let Some(ty) = mapping.get(arg) {
        ty.display_name()
    } else {
        arg.to_string()
    }
}
