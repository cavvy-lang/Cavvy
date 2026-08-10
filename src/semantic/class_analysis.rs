//! 类定义、继承关系分析和主类冲突分析

use super::analyzer::SemanticAnalyzer;
use crate::ast::{ClassMember, Modifier, Program};
use crate::miette_diagnostic::{CayResult, ErrorCodes, SourceLocation, semantic_error_with_file};
use crate::types::{ClassInfo, FieldInfo, MethodInfo, ParameterInfo, Type};

/// impl 声明可扩展的目标类型
enum ImplTarget {
    Class(usize),
    Struct(usize),
    Enum(usize),
}

/// 从类型使用中抽取基础类型名（去除泛型实参）。
/// 例如 Object("Foo<int>") -> "Foo<int>", Generic("Foo", _) -> "Foo"。
fn type_base_name(ty: &Type) -> String {
    match ty {
        Type::Generic(name, _) => name.clone(),
        Type::Object(name) => name.clone(),
        Type::Pointer(inner) => type_base_name(inner),
        _ => ty.to_string(),
    }
}

impl SemanticAnalyzer {
    /// 检查主类冲突
    /// 规则：
    /// 1. 如果只有一个类有 main 方法，自动选为主类
    /// 2. 如果有多个类有 main 方法：
    ///    - 如果只有一个类标记了 @main，选该类为主类
    ///    - 如果有多个类标记了 @main，报错
    ///    - 如果没有类标记 @main，报错并提示使用 @main
    pub fn check_main_class_conflicts(&mut self, program: &Program) -> CayResult<()> {
        // 收集所有有 main 方法的类
        let mut main_classes: Vec<(String, bool, SourceLocation)> = Vec::new(); // (类名, 是否有@main标记, 源位置)

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
                main_classes.push((class.name.clone(), has_main_marker, class.loc.clone()));
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
                let marked_classes: Vec<&(String, bool, SourceLocation)> =
                    main_classes.iter().filter(|(_, marked, _)| *marked).collect();

                match marked_classes.len() {
                    0 => {
                        // 多个类有 main，但没有标记 @main
                        let class_names: Vec<String> =
                            main_classes.iter().map(|(name, _, _)| name.clone()).collect();
                        let first_loc = main_classes
                            .first()
                            .map(|(_, _, loc)| loc.clone())
                            .unwrap_or_else(|| SourceLocation::new(self.current_file.clone(), 1, 1));
                        Err(semantic_error_with_file(
                            ErrorCodes::SEMANTIC_INVALID_OPERATION,
                            first_loc.file.clone().or_else(|| self.current_file.clone()),
                            first_loc.line,
                            first_loc.column,
                            format!(
                                "多个类包含 main 方法: {}。请使用 @main 标记指定主类，例如：\n@main public class {} {{ ... }}",
                                class_names.join(", "),
                                class_names[0]
                            ),
                        ))
                    }
                    1 => {
                        // 只有一个类标记了 @main，这是正确的
                        Ok(())
                    }
                    _ => {
                        // 多个类标记了 @main
                        let marked_names: Vec<String> = marked_classes
                            .iter()
                            .map(|(name, _, _)| name.clone())
                            .collect();
                        let first_loc = marked_classes
                            .first()
                            .map(|(_, _, loc)| (*loc).clone())
                            .unwrap_or_else(|| SourceLocation::new(self.current_file.clone(), 1, 1));
                        Err(semantic_error_with_file(
                            ErrorCodes::SEMANTIC_INVALID_OPERATION,
                            first_loc.file.clone().or_else(|| self.current_file.clone()),
                            first_loc.line,
                            first_loc.column,
                            format!(
                                "多个类标记了 @main: {}。只能有一个主类。",
                                marked_names.join(", ")
                            ),
                        ))
                    }
                }
            }
        }
    }

    /// 处理 impl 声明：将 impl 块中的方法附加到目标类型，并记录接口实现关系。
    ///
    /// 必须在 collect_classes/collect_structs/collect_enums 之前调用，这样后续
    /// 类型注册表收集阶段会把 impl 方法当作目标类型的普通方法一并注册。
    pub fn process_impl_decls(&mut self, program: &mut Program) -> CayResult<()> {
        for impl_decl in &program.impl_decls {
            let target_name = type_base_name(&impl_decl.target_type);
            let target_ns = &impl_decl.namespace_path;

            // 尝试解析目标类型：先按限定名查找，再按 impl 所在命名空间查找
            let target = if target_name.contains("::") {
                self.find_decl_by_qualified_name(program, &target_name)
            } else {
                self.find_decl_by_simple_name(program, &target_name, target_ns)
            };

            let target = match target {
                Some(t) => t,
                None => {
                    let (file, line) = self.resolve_file_and_line(impl_decl.loc.line);
                    return Err(semantic_error_with_file(
                        ErrorCodes::SEMANTIC_INVALID_OPERATION,
                        file,
                        line,
                        impl_decl.loc.column,
                        format!(
                            "impl 声明的目标类型 '{}' 未定义",
                            target_name
                        ),
                    ));
                }
            };

            match target {
                ImplTarget::Class(class_idx) => {
                    let class = &mut program.classes[class_idx];
                    for method in &impl_decl.methods {
                        class.members.push(ClassMember::Method(method.clone()));
                    }
                    if let Some(ref interface_type) = impl_decl.interface_type {
                        class.interfaces.push(interface_type.clone());
                    }
                }
                ImplTarget::Struct(struct_idx) => {
                    let struct_decl = &mut program.structs[struct_idx];
                    for method in &impl_decl.methods {
                        struct_decl.methods.push(method.clone());
                    }
                    if impl_decl.interface_type.is_some() {
                        let (file, line) = self.resolve_file_and_line(impl_decl.loc.line);
                        return Err(semantic_error_with_file(
                            ErrorCodes::SEMANTIC_INVALID_OPERATION,
                            file,
                            line,
                            impl_decl.loc.column,
                            "struct 不支持接口实现".to_string(),
                        ));
                    }
                }
                ImplTarget::Enum(enum_idx) => {
                    let enum_decl = &mut program.enums[enum_idx];
                    for method in &impl_decl.methods {
                        enum_decl.methods.push(method.clone());
                    }
                    if impl_decl.interface_type.is_some() {
                        let (file, line) = self.resolve_file_and_line(impl_decl.loc.line);
                        return Err(semantic_error_with_file(
                            ErrorCodes::SEMANTIC_INVALID_OPERATION,
                            file,
                            line,
                            impl_decl.loc.column,
                            "enum 不支持接口实现".to_string(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    /// 按限定名查找 class/struct/enum 索引
    fn find_decl_by_qualified_name(
        &self,
        program: &Program,
        qualified_name: &str,
    ) -> Option<ImplTarget> {
        for (idx, class) in program.classes.iter().enumerate() {
            let class_qname = if class.namespace_path.is_empty() {
                class.name.clone()
            } else {
                format!("{}::{}", class.namespace_path.join("::"), class.name)
            };
            if class_qname == qualified_name {
                return Some(ImplTarget::Class(idx));
            }
        }
        for (idx, s) in program.structs.iter().enumerate() {
            let qname = if s.namespace_path.is_empty() {
                s.name.clone()
            } else {
                format!("{}::{}", s.namespace_path.join("::"), s.name)
            };
            if qname == qualified_name {
                return Some(ImplTarget::Struct(idx));
            }
        }
        for (idx, e) in program.enums.iter().enumerate() {
            let qname = if e.namespace_path.is_empty() {
                e.name.clone()
            } else {
                format!("{}::{}", e.namespace_path.join("::"), e.name)
            };
            if qname == qualified_name {
                return Some(ImplTarget::Enum(idx));
            }
        }
        None
    }

    /// 按简单名查找 class/struct/enum 索引，优先匹配 impl 所在命名空间，其次全局。
    fn find_decl_by_simple_name(
        &self,
        program: &Program,
        simple_name: &str,
        impl_ns: &[String],
    ) -> Option<ImplTarget> {
        // 优先：impl 所在命名空间内的同名类型
        if !impl_ns.is_empty() {
            let preferred = format!("{}::{}", impl_ns.join("::"), simple_name);
            if let Some(t) = self.find_decl_by_qualified_name(program, &preferred) {
                return Some(t);
            }
        }
        // 其次：全局（无命名空间）同名类型
        for (idx, class) in program.classes.iter().enumerate() {
            if class.namespace_path.is_empty() && class.name == simple_name {
                return Some(ImplTarget::Class(idx));
            }
        }
        for (idx, s) in program.structs.iter().enumerate() {
            if s.namespace_path.is_empty() && s.name == simple_name {
                return Some(ImplTarget::Struct(idx));
            }
        }
        for (idx, e) in program.enums.iter().enumerate() {
            if e.namespace_path.is_empty() && e.name == simple_name {
                return Some(ImplTarget::Enum(idx));
            }
        }
        None
    }

    /// 收集类定义
    pub fn collect_classes(&mut self, program: &Program) -> CayResult<()> {
        // 首先收集接口定义
        for interface in &program.interfaces {
            let mut interface_info = crate::types::InterfaceInfo::new(interface.name.clone());
            interface_info.type_params = interface
                .type_params
                .iter()
                .map(|p| crate::types::TypeParamInfo {
                    name: p.name.clone(),
                    bound: p.bound.clone(),
                    default_type: p.default_type.clone(),
                })
                .collect();

            // 收集接口方法，并将接口类型参数名替换为 GenericParam 类型，
            // 以便与实现类的方法签名进行统一比较。
            for method in &interface.methods {
                let mut all_type_params = interface.type_params.clone();
                all_type_params.extend(method.type_params.clone());
                let params = self.replace_params_type_params(&method.params, &all_type_params);
                let return_type = self.replace_type_params(&method.return_type, &all_type_params);
                let method_info = MethodInfo {
                    name: method.name.clone(),
                    class_name: interface.name.clone(),
                    type_params: method
                        .type_params
                        .iter()
                        .map(|p| crate::types::TypeParamInfo {
                            name: p.name.clone(),
                            bound: p.bound.clone(),
                            default_type: p.default_type.clone(),
                        })
                        .collect(),
                    params,
                    return_type,
                    is_public: true, // 接口方法默认是public
                    is_private: false,
                    is_protected: false,
                    is_static: false,
                    is_native: false,
                    is_const: false,
                    is_abstract: false, // 接口方法在Cavvy中视为非抽象（有默认实现机制）
                    is_override: false,
                    is_final: false, // 接口方法不是final
                    is_test: false,  // 接口方法不能是 @Test
                    vtable_slot: None,
                    loc: method.loc.clone(),
                };
                interface_info.add_method(method_info);
            }

            // 将预处理行号映射为原始文件行号（支持 #include 后的正确错误定位）
            let (file, line) = self.resolve_file_and_line(interface.loc.line);
            self.type_registry.register_interface(
                interface_info,
                file,
                line,
                interface.loc.column,
            )?;
        }

        // 然后收集类定义
        for class in &program.classes {
            let is_abstract = class.modifiers.contains(&Modifier::Abstract);
            let is_final = class.modifiers.contains(&Modifier::Final);
            let is_interop = class.modifiers.contains(&Modifier::Interop);
            let is_stack_only = class.modifiers.contains(&Modifier::StackOnly);
            let mut class_info = ClassInfo {
                name: class.name.clone(),
                type_params: class
                    .type_params
                    .iter()
                    .map(|p| crate::types::TypeParamInfo {
                        name: p.name.clone(),
                        bound: p.bound.clone(),
                        default_type: p.default_type.clone(),
                    })
                    .collect(),
                methods: std::collections::HashMap::new(),
                fields: std::collections::HashMap::new(),
                constructors: Vec::new(),
                has_destructor: false,
                // 未显式指定父类时，默认继承 Object 根类。
                // C++ 互操作类（interop）不继承 Cavvy 根类，避免与外部 C++ 布局冲突。
                parent: if is_interop {
                    class.parent.clone()
                } else {
                    class.parent.clone().or(Some("Object".to_string()))
                },
                interfaces: class.interfaces.clone(),
                is_abstract,
                is_final,
                is_interop,
                is_stack_only,
                vtable_layout: None,
            };

            // 收集字段信息
            for member in &class.members {
                match member {
                    ClassMember::Field(field) => {
                        let is_final = field.modifiers.contains(&Modifier::Final);
                        let is_static = field.modifiers.contains(&Modifier::Static);
                        // static final 字段且初始化值为字面量时，标记为编译期常量
                        let is_const_expr = is_static
                            && is_final
                            && field
                                .initializer
                                .as_ref()
                                .map_or(false, |e| matches!(e, crate::ast::Expr::Literal(_)));
                        // 将泛型参数替换为 GenericParam 类型
                        let field_type =
                            self.replace_type_params(&field.field_type, &class.type_params);
                        let field_info = FieldInfo {
                            name: field.name.clone(),
                            field_type,
                            is_public: field.modifiers.contains(&Modifier::Public),
                            is_private: field.modifiers.contains(&Modifier::Private),
                            is_protected: field.modifiers.contains(&Modifier::Protected),
                            is_static,
                            is_final,
                            is_const_expr,
                        };
                        class_info.fields.insert(field.name.clone(), field_info);
                    }
                    ClassMember::Constructor(ctor) => {
                        // 将泛型参数替换为 GenericParam 类型
                        let params =
                            self.replace_params_type_params(&ctor.params, &class.type_params);
                        let ctor_info = crate::types::ConstructorInfo {
                            params: params.clone(),
                            is_public: ctor.modifiers.contains(&Modifier::Public),
                            is_private: ctor.modifiers.contains(&Modifier::Private),
                            is_protected: ctor.modifiers.contains(&Modifier::Protected),
                            loc: ctor.loc.clone(),
                        };
                        // 检查是否存在签名完全相同的构造函数（重复定义）
                        for existing in &class_info.constructors {
                            if existing.params.len() == params.len() {
                                let same_params =
                                    existing.params.iter().zip(params.iter()).all(|(a, b)| {
                                        a.param_type == b.param_type && a.is_varargs == b.is_varargs
                                    });
                                if same_params {
                                    let first_loc = &existing.loc;
                                    let first_loc_str = if first_loc
                                        .file
                                        .as_deref()
                                        .unwrap_or("")
                                        .is_empty()
                                    {
                                        format!("{}:{}", first_loc.line, first_loc.column)
                                    } else {
                                        format!(
                                            "{}:{}:{}",
                                            first_loc.file_str(),
                                            first_loc.line,
                                            first_loc.column
                                        )
                                    };
                                    let current_file = ctor
                                        .loc
                                        .file
                                        .clone()
                                        .or_else(|| self.current_file.clone());
                                    return Err(semantic_error_with_file(
                                        ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
                                        current_file,
                                        ctor.loc.line,
                                        ctor.loc.column,
                                        format!(
                                            "构造函数已被定义，不能重复定义相同签名的构造函数\n首次定义于: {}\n当前定义于: {}:{}:{}",
                                            first_loc_str,
                                            ctor.loc.file.as_deref().unwrap_or("<unknown>"),
                                            ctor.loc.line,
                                            ctor.loc.column
                                        ),
                                    ));
                                }
                            }
                        }
                        class_info.constructors.push(ctor_info);
                    }
                    ClassMember::Destructor(_) => {
                        class_info.has_destructor = true;
                    }
                    _ => {}
                }
            }

            // 使用类定义的实际文件位置（支持 #include 后的正确错误定位）
            let file = class.loc.file.clone().or_else(|| self.current_file.clone());
            let line = class.loc.line;

            // 如果有命名空间路径，使用限定名注册并记录
            if !class.namespace_path.is_empty() {
                let qualified_name = format!("{}::{}", class.namespace_path.join("::"), class.name);
                let mut qualified_class_info = class_info.clone();
                qualified_class_info.name = qualified_name.clone();
                self.type_registry.register_class(
                    qualified_class_info,
                    file,
                    line,
                    class.loc.column,
                )?;
                self.type_registry
                    .set_class_namespace(&qualified_name, class.namespace_path.clone());
            } else {
                self.type_registry
                    .register_class(class_info, file, line, class.loc.column)?;
            }

            // 验证 @stack_only 只能用于类声明，不能用于成员。
            for member in &class.members {
                let has_stack_only = match member {
                    ClassMember::Method(m) => m.modifiers.contains(&Modifier::StackOnly),
                    ClassMember::Field(f) => f.modifiers.contains(&Modifier::StackOnly),
                    ClassMember::Constructor(c) => c.modifiers.contains(&Modifier::StackOnly),
                    ClassMember::Destructor(d) => d.modifiers.contains(&Modifier::StackOnly),
                    _ => false,
                };
                if has_stack_only {
                    let (member_kind, loc) = match member {
                        ClassMember::Method(m) => ("方法", m.loc.clone()),
                        ClassMember::Field(f) => ("字段", f.loc.clone()),
                        ClassMember::Constructor(c) => ("构造函数", c.loc.clone()),
                        ClassMember::Destructor(d) => ("析构函数", d.loc.clone()),
                        _ => ("成员", SourceLocation::default()),
                    };
                    return Err(semantic_error_with_file(
                        ErrorCodes::SEMANTIC_INVALID_OPERATION,
                        self.current_file.clone(),
                        loc.line,
                        loc.column,
                        format!("@stack_only 只能用于类声明，不能用于{}", member_kind),
                    ));
                }
            }
        }
        Ok(())
    }

    /// 将类型中的泛型参数名替换为 GenericParam 类型
    /// 例如：Object("T") -> GenericParam("T")
    pub fn replace_type_params(&self, ty: &Type, type_params: &[crate::ast::TypeParam]) -> Type {
        match ty {
            Type::Object(name) => {
                if type_params.iter().any(|p| &p.name == name) {
                    Type::GenericParam(name.clone())
                } else {
                    ty.clone()
                }
            }
            Type::Array(elem) => Type::Array(Box::new(self.replace_type_params(elem, type_params))),
            Type::Generic(name, args) => {
                let new_args = args
                    .iter()
                    .map(|arg| self.replace_type_params(arg, type_params))
                    .collect();
                Type::Generic(name.clone(), new_args)
            }
            Type::Function(func) => Type::Function(Box::new(crate::types::FunctionType {
                return_type: Box::new(self.replace_type_params(
                    &func.return_type,
                    type_params,
                )),
                params: func
                    .params
                    .iter()
                    .map(|p| self.replace_type_params(p, type_params))
                    .collect(),
                is_static: func.is_static,
                is_closure: func.is_closure,
            })),
            Type::Pointer(inner) => {
                Type::Pointer(Box::new(self.replace_type_params(inner, type_params)))
            }
            _ => ty.clone(),
        }
    }

    /// 将参数列表中的泛型参数名替换为 GenericParam 类型
    fn replace_params_type_params(
        &self,
        params: &[ParameterInfo],
        type_params: &[crate::ast::TypeParam],
    ) -> Vec<ParameterInfo> {
        params
            .iter()
            .map(|p| ParameterInfo {
                name: p.name.clone(),
                param_type: self.replace_type_params(&p.param_type, type_params),
                is_varargs: p.is_varargs,
            })
            .collect()
    }

    /// 获取类的限定查找名（包含命名空间路径）
    ///
    /// 使用限定名可避免 `using` 别名或全局非限定名注册导致类查找错位，
    /// 确保不同命名空间下同名类的成员被收集到正确的 ClassInfo 中。
    fn qualified_class_name(&self, class: &crate::ast::ClassDecl) -> String {
        if class.namespace_path.is_empty() {
            class.name.clone()
        } else {
            format!("{}::{}", class.namespace_path.join("::"), class.name)
        }
    }

    /// 分析方法定义
    pub fn analyze_methods(&mut self, program: &Program) -> CayResult<()> {
        for class in &program.classes {
            let class_lookup_name = self.qualified_class_name(class);
            self.current_class = Some(class_lookup_name.clone());
            self.type_registry.current_namespace = class.namespace_path.clone();

            for member in &class.members {
                if let ClassMember::Method(method) = member {
                    let is_test = method.modifiers.contains(&Modifier::Test);

                    // 将泛型参数替换为 GenericParam 类型
                    let mut all_type_params = class.type_params.clone();
                    all_type_params.extend(method.type_params.clone());
                    let params =
                        self.replace_params_type_params(&method.params, &all_type_params);
                    let return_type =
                        self.replace_type_params(&method.return_type, &all_type_params);

                    let method_info = MethodInfo {
                        name: method.name.clone(),
                        class_name: class.name.clone(),
                        type_params: method
                            .type_params
                            .iter()
                            .map(|p| crate::types::TypeParamInfo {
                                name: p.name.clone(),
                                bound: p.bound.clone(),
                                default_type: p.default_type.clone(),
                            })
                            .collect(),
                        params,
                        return_type,
                        is_public: method.modifiers.contains(&Modifier::Public),
                        is_private: method.modifiers.contains(&Modifier::Private),
                        is_protected: method.modifiers.contains(&Modifier::Protected),
                        is_static: method.modifiers.contains(&Modifier::Static),
                        is_native: method.modifiers.contains(&Modifier::Native),
                        is_const: method.modifiers.contains(&Modifier::Const),
                        is_abstract: method.modifiers.contains(&Modifier::Abstract),
                        is_override: method.modifiers.contains(&Modifier::Override),
                        is_final: method.modifiers.contains(&Modifier::Final),
                        is_test,
                        vtable_slot: None,
                        loc: method.loc.clone(),
                    };

                    // 验证 @Test 方法签名
                    if is_test {
                        // @Test 方法必须是 void 返回类型
                        if method.return_type != Type::Void {
                            return Err(semantic_error_with_file(
                                ErrorCodes::SEMANTIC_INVALID_OPERATION,
                                self.current_file.clone(),
                                method.loc.line,
                                method.loc.column,
                                format!(
                                    "@Test 方法 '{}' 的返回类型必须是 void，当前为 {}\n提示: 将返回类型改为 void，例如: public void {}()",
                                    method.name, method.return_type, method.name
                                ),
                            ));
                        }
                        // @Test 方法不能有参数
                        if !method.params.is_empty() {
                            return Err(semantic_error_with_file(
                                ErrorCodes::SEMANTIC_INVALID_OPERATION,
                                self.current_file.clone(),
                                method.loc.line,
                                method.loc.column,
                                format!(
                                    "@Test 方法 '{}' 不能有参数（发现 {} 个参数）\n提示: 移除参数，例如: public void {}()",
                                    method.name,
                                    method.params.len(),
                                    method.name
                                ),
                            ));
                        }
                        // @Test 方法不能是 private
                        if method.modifiers.contains(&Modifier::Private) {
                            return Err(semantic_error_with_file(
                                ErrorCodes::SEMANTIC_INVALID_OPERATION,
                                self.current_file.clone(),
                                method.loc.line,
                                method.loc.column,
                                format!(
                                    "@Test 方法 '{}' 不能是 private\n提示: 将 private 改为 public，例如: public void {}()",
                                    method.name, method.name
                                ),
                            ));
                        }
                    }

                    // 预计算「仅返回类型不同的重载是否分别匹配类实现的泛型接口
                    // 实例化」（get_class_mut 可变借用期间无法再调用 &self 辅助方法）
                    let new_method_matches_interface =
                        self.return_only_overload_implements_interfaces(class, &method_info);
                    let existing_matches_interface: Vec<bool> = self
                        .type_registry
                        .get_class(&class_lookup_name)
                        .and_then(|ci| ci.methods.get(&method_info.name))
                        .map(|ms| {
                            ms.iter()
                                .map(|m| self.return_only_overload_implements_interfaces(class, m))
                                .collect()
                        })
                        .unwrap_or_default();

                    if let Some(class_info) = self.type_registry.get_class_mut(&class_lookup_name) {
                        // 检查是否存在签名完全相同的方法（重复定义）
                        if let Some(existing_methods) = class_info.methods.get(&method_info.name) {
                            for (existing_idx, existing) in existing_methods.iter().enumerate() {
                                if existing.params.len() == method_info.params.len() {
                                    let same_params =
                                        existing.params.iter().zip(method_info.params.iter()).all(
                                            |(a, b)| {
                                                a.param_type == b.param_type
                                                    && a.is_varargs == b.is_varargs
                                            },
                                        );
                                    if same_params {
                                        // 仅返回类型不同的重载：当两个方法分别匹配
                                        // 类所实现的泛型接口的不同实例化时允许
                                        // （如 Into<IOError> 与 Into<ParseError>
                                        // 各要求一个返回类型不同的 into()）。
                                        let return_differs =
                                            existing.return_type != method_info.return_type;
                                        if return_differs
                                            && new_method_matches_interface
                                            && existing_matches_interface
                                                .get(existing_idx)
                                                .copied()
                                                .unwrap_or(false)
                                        {
                                            continue;
                                        }
                                        let first_loc = &existing.loc;
                                        let first_loc_str = if first_loc
                                            .file
                                            .as_deref()
                                            .unwrap_or("")
                                            .is_empty()
                                        {
                                            format!(
                                                "{}:{}",
                                                first_loc.line, first_loc.column
                                            )
                                        } else {
                                            format!(
                                                "{}:{}:{}",
                                                first_loc.file_str(),
                                                first_loc.line,
                                                first_loc.column
                                            )
                                        };
                                        let current_file = method
                                            .loc
                                            .file
                                            .clone()
                                            .or_else(|| self.current_file.clone());
                                        return Err(semantic_error_with_file(
                                            ErrorCodes::SEMANTIC_DUPLICATE_DEFINITION,
                                            current_file,
                                            method.loc.line,
                                            method.loc.column,
                                            format!(
                                                "方法 '{}' 已被定义，不能重复定义相同签名的方法\n首次定义于: {}\n当前定义于: {}:{}:{}",
                                                method.name,
                                                first_loc_str,
                                                method.loc.file.as_deref().unwrap_or("<unknown>"),
                                                method.loc.line,
                                                method.loc.column
                                            ),
                                        ));
                                    }
                                }
                            }
                        }
                        class_info.add_method(method_info);
                    }
                }
            }
        }
        Ok(())
    }

    /// 检查继承关系
    /// 1. 验证父类是否存在
    /// 2. 检查 final 类不能被继承
    /// 3. 检测循环继承
    /// 4. 验证 @Override 注解
    /// 5. 检查 final 方法不能被重写
    pub fn check_inheritance(&mut self, program: &Program) -> CayResult<()> {
        // 第一遍：验证所有父类存在
        for class in &program.classes {
            self.type_registry.current_namespace = class.namespace_path.clone();
            if let Some(ref parent_name) = class.parent {
                if !self.type_registry.class_exists(parent_name) {
                    return Err(semantic_error_with_file(
                        ErrorCodes::SEMANTIC_INVALID_OPERATION,
                        class.loc.file.clone(),
                        class.loc.line,
                        class.loc.column,
                        format!(
                            "Class '{}' extends undefined class '{}'",
                            class.name, parent_name
                        ),
                    ));
                }
            }
        }

        // 第二遍：检查 final 类不能被继承
        for class in &program.classes {
            self.type_registry.current_namespace = class.namespace_path.clone();
            if let Some(ref parent_name) = class.parent {
                if let Some(parent_class) = self.type_registry.get_class(parent_name) {
                    if parent_class.is_final {
                        return Err(semantic_error_with_file(
                            ErrorCodes::SEMANTIC_INVALID_OPERATION,
                            class.loc.file.clone(),
                            class.loc.line,
                            class.loc.column,
                            format!(
                                "Class '{}' cannot inherit from final class '{}'",
                                class.name, parent_name
                            ),
                        ));
                    }
                }
            }
        }

        // 第三遍：检测循环继承
        for class in &program.classes {
            self.check_circular_inheritance(&class.name, &class.name, &mut Vec::new(), &class.loc)?;
        }

        // 第四遍：验证 @Override 注解 和 final 方法检查
        for class in &program.classes {
            self.check_override_methods(class)?;
            self.check_final_method_override(class)?;
        }

        // 第五遍：检查接口方法实现
        for class in &program.classes {
            self.check_interface_implementations(class)?;
        }

        Ok(())
    }

    /// 检查类是否实现了其声明的所有接口方法
    ///
    /// 支持泛型接口实参的替换：例如 ArrayListIterator<T> implements Iterator<T> 时，
    /// 将 Iterator 方法签名中的 T 替换为 ArrayListIterator 的 T 后再进行比较。
    fn check_interface_implementations(&self, class: &crate::ast::ClassDecl) -> CayResult<()> {
        for interface_type in &class.interfaces {
            let interface_name = interface_type_name(interface_type);
            let interface_info = match self.type_registry.get_interface(&interface_name) {
                Some(info) => info,
                None => {
                    return Err(semantic_error_with_file(
                        ErrorCodes::SEMANTIC_INVALID_OPERATION,
                        class.loc.file.clone(),
                        class.loc.line,
                        class.loc.column,
                        format!(
                            "Class '{}' implements undefined interface '{}'",
                            class.name, interface_name
                        ),
                    ));
                }
            };

            // 建立接口类型参数到类类型实参的映射。
            // 类实参中可能包含类自身的泛型参数名（如 ArrayListIterator<T> implements Iterator<T>），
            // 需要将这些标识符也统一替换为 GenericParam，以便与方法签名中的类型一致。
            let type_args: Vec<crate::types::Type> = match interface_type {
                crate::types::Type::Generic(_, args) => args
                    .iter()
                    .map(|a| self.replace_type_params(a, &class.type_params))
                    .collect(),
                crate::types::Type::Object(_) => interface_info
                    .type_params
                    .iter()
                    .map(|p| crate::types::Type::GenericParam(p.name.clone()))
                    .collect(),
                _ => continue,
            };

            for (method_name, interface_method) in &interface_info.methods {
                // 替换接口方法签名中的类型参数为类提供的实参
                let substituted_params: Vec<crate::types::ParameterInfo> = interface_method
                    .params
                    .iter()
                    .map(|p| crate::types::ParameterInfo {
                        name: p.name.clone(),
                        param_type: substitute_interface_type(
                            &p.param_type,
                            &interface_info.type_params,
                            &type_args,
                        ),
                        is_varargs: p.is_varargs,
                    })
                    .collect();
                let substituted_return = substitute_interface_type(
                    &interface_method.return_type,
                    &interface_info.type_params,
                    &type_args,
                );

                // 检查当前类或其祖先中是否存在匹配的方法实现
                if !self.method_exists_in_class_or_ancestors(
                    &class.name,
                    method_name,
                    &substituted_params,
                    &substituted_return,
                ) {
                    return Err(semantic_error_with_file(
                        ErrorCodes::SEMANTIC_INVALID_OPERATION,
                        class.loc.file.clone(),
                        class.loc.line,
                        class.loc.column,
                        format!(
                            "Class '{}' must implement interface method '{}.{}'",
                            class.name, interface_name, method_name
                        ),
                    ));
                }
            }
        }
        Ok(())
    }

    /// 检查类或其祖先中是否存在匹配的方法（用于接口实现检查）
    fn method_exists_in_class_or_ancestors(
        &self,
        class_name: &str,
        method_name: &str,
        params: &[ParameterInfo],
        return_type: &Type,
    ) -> bool {
        if let Some(class_info) = self.type_registry.get_class(class_name) {
            // 在当前类中查找方法
            if let Some(methods) = class_info.methods.get(method_name) {
                for method in methods {
                    if method.params.len() == params.len() {
                        let class_param_types: Vec<Type> =
                            method.params.iter().map(|p| p.param_type.clone()).collect();
                        let expected_param_types: Vec<Type> =
                            params.iter().map(|p| p.param_type.clone()).collect();
                        if self.types_match(&class_param_types, &expected_param_types)
                            && self.method_return_matches(&method.return_type, return_type)
                        {
                            return true;
                        }
                    }
                }
            }

            // 递归检查父类
            if let Some(ref parent_name) = class_info.parent {
                return self.method_exists_in_class_or_ancestors(
                    parent_name,
                    method_name,
                    params,
                    return_type,
                );
            }
        }

        false
    }

    /// 判断「仅返回类型不同」的重载方法是否实现了类的某个泛型接口实例化方法。
    /// 仅当方法名+参数与接口方法（类型实参替换后）一致、且返回类型等于替换后的
    /// 接口方法返回类型时成立——如 `IOError into()` 匹配 `Into<IOError>`，
    /// `ParseError into()` 匹配 `Into<ParseError>`，二者可共存。
    fn return_only_overload_implements_interfaces(
        &self,
        class: &crate::ast::ClassDecl,
        method_info: &crate::types::MethodInfo,
    ) -> bool {
        for interface_type in &class.interfaces {
            let interface_name = interface_type_name(interface_type);
            let Some(interface_info) = self.type_registry.get_interface(&interface_name) else {
                continue;
            };
            // 仅泛型接口的不同实例化能产生「同签名不同返回」的方法要求
            if interface_info.type_params.is_empty() {
                continue;
            }
            let type_args: Vec<crate::types::Type> = match interface_type {
                crate::types::Type::Generic(_, args) => args
                    .iter()
                    .map(|a| self.replace_type_params(a, &class.type_params))
                    .collect(),
                _ => continue,
            };
            let Some(interface_method) = interface_info.methods.get(&method_info.name) else {
                continue;
            };
            let substituted_return = substitute_interface_type(
                &interface_method.return_type,
                &interface_info.type_params,
                &type_args,
            );
            let params_match = interface_method.params.len() == method_info.params.len()
                && interface_method
                    .params
                    .iter()
                    .zip(method_info.params.iter())
                    .all(|(ip, mp)| {
                        substitute_interface_type(
                            &ip.param_type,
                            &interface_info.type_params,
                            &type_args,
                        ) == mp.param_type
                    });
            if params_match && substituted_return == method_info.return_type {
                return true;
            }
        }
        false
    }

    /// 递归检查循环继承
    fn check_circular_inheritance(
        &self,
        original: &str,
        current: &str,
        visited: &mut Vec<String>,
        loc: &SourceLocation,
    ) -> CayResult<()> {
        if visited.contains(&current.to_string()) {
            let file = loc.file.clone().or_else(|| self.current_file.clone());
            return Err(semantic_error_with_file(
                ErrorCodes::SEMANTIC_INVALID_OPERATION,
                file,
                loc.line,
                loc.column,
                format!(
                    "Circular inheritance detected involving class '{}'",
                    original
                ),
            ));
        }

        if let Some(class_info) = self.type_registry.get_class(current) {
            if let Some(ref parent_name) = class_info.parent {
                visited.push(current.to_string());
                self.check_circular_inheritance(original, parent_name, visited, loc)?;
            }
        }

        Ok(())
    }

    /// 检查 @Override 注解的方法
    fn check_override_methods(&self, class: &crate::ast::ClassDecl) -> CayResult<()> {
        for member in &class.members {
            if let ClassMember::Method(method) = member {
                if method.modifiers.contains(&Modifier::Override) {
                    // 检查父类是否存在
                    let parent_name = match &class.parent {
                        Some(p) => p,
                        None => {
                            return Err(semantic_error_with_file(
                                ErrorCodes::SEMANTIC_INVALID_OPERATION,
                                method.loc.file.clone(),
                                method.loc.line,
                                method.loc.column,
                                format!(
                                    "Method '{}' has @Override annotation but class '{}' does not extend any class",
                                    method.name, class.name
                                ),
                            ));
                        }
                    };

                    // 检查父类中是否存在同名方法
                    if !self.method_exists_in_parent(
                        parent_name,
                        &method.name,
                        &method.params,
                        &method.return_type,
                    ) {
                        return Err(semantic_error_with_file(
                            ErrorCodes::SEMANTIC_INVALID_OPERATION,
                            method.loc.file.clone(),
                            method.loc.line,
                            method.loc.column,
                            format!(
                                "Method '{}' has @Override annotation but does not override any method from parent class '{}'",
                                method.name, parent_name
                            ),
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// 检查父类中是否存在匹配的方法
    fn method_exists_in_parent(
        &self,
        parent_name: &str,
        method_name: &str,
        params: &[ParameterInfo],
        return_type: &Type,
    ) -> bool {
        if let Some(parent_class) = self.type_registry.get_class(parent_name) {
            // 获取参数类型列表
            let param_types: Vec<Type> = params.iter().map(|p| p.param_type.clone()).collect();

            // 在父类中查找方法
            if let Some(methods) = parent_class.methods.get(method_name) {
                for method in methods {
                    // 检查参数数量和类型是否匹配
                    if method.params.len() == params.len() {
                        let parent_param_types: Vec<Type> =
                            method.params.iter().map(|p| p.param_type.clone()).collect();
                        if self.types_match(&parent_param_types, &param_types)
                            && method.return_type == *return_type
                        {
                            return true;
                        }
                    }
                }
            }

            // 递归检查父类的父类
            if let Some(ref grandparent) = parent_class.parent {
                return self.method_exists_in_parent(grandparent, method_name, params, return_type);
            }
        }

        false
    }

    /// 检查 final 方法是否被重写
    fn check_final_method_override(&self, class: &crate::ast::ClassDecl) -> CayResult<()> {
        // 获取父类名
        let parent_name = match &class.parent {
            Some(p) => p,
            None => return Ok(()), // 没有父类，不需要检查
        };

        // 检查类中是否有方法重写了父类的 final 方法
        for member in &class.members {
            if let ClassMember::Method(method) = member {
                let param_types: Vec<Type> =
                    method.params.iter().map(|p| p.param_type.clone()).collect();

                // 在父类及其祖先中查找同名的 final 方法
                if let Err(e) = self.check_final_method_in_ancestors(
                    parent_name,
                    &method.name,
                    &param_types,
                    method.loc.line,
                    method.loc.column,
                ) {
                    return Err(e);
                }
            }
        }

        Ok(())
    }

    /// 递归检查父类中的 final 方法
    fn check_final_method_in_ancestors(
        &self,
        parent_name: &str,
        method_name: &str,
        param_types: &[Type],
        line: usize,
        column: usize,
    ) -> CayResult<()> {
        if let Some(parent_class) = self.type_registry.get_class(parent_name) {
            // 在父类中查找方法
            if let Some(methods) = parent_class.methods.get(method_name) {
                for method in methods {
                    // 检查参数类型是否匹配
                    if method.params.len() == param_types.len() {
                        let parent_param_types: Vec<Type> =
                            method.params.iter().map(|p| p.param_type.clone()).collect();

                        if self.types_match(&parent_param_types, param_types) {
                            // 找到匹配的方法，检查是否是 final
                            if method.is_final {
                                return Err(semantic_error_with_file(
                                    ErrorCodes::SEMANTIC_INVALID_OPERATION,
                                    None,
                                    line,
                                    column,
                                    format!(
                                        "Method '{}' cannot override final method from class '{}'",
                                        method_name, parent_name
                                    ),
                                ));
                            }
                        }
                    }
                }
            }

            // 递归检查父类的父类
            if let Some(ref grandparent) = parent_class.parent {
                return self.check_final_method_in_ancestors(
                    grandparent,
                    method_name,
                    param_types,
                    line,
                    column,
                );
            }
        }

        Ok(())
    }

    /// 检查方法返回类型是否兼容。
    ///
    /// 支持返回类型协变：当期望类型是接口时，实际返回类型可以是实现该接口的类。
    /// 例如 Iterable<T>.iterator() 期望返回 Iterator<T>，而 ArrayListIterator<T>
    /// 实现了 Iterator<T>，因此兼容。
    fn method_return_matches(&self, actual: &Type, expected: &Type) -> bool {
        if actual == expected {
            return true;
        }

        let expected_name = match expected {
            Type::Object(name) | Type::Generic(name, _) => name,
            _ => return false,
        };
        let actual_name = match actual {
            Type::Object(name) | Type::Generic(name, _) => name,
            _ => return false,
        };

        // 期望类型必须是接口
        if !self.type_registry.interface_exists(expected_name) {
            return false;
        }

        // 实际类型必须是类，且实现了期望接口
        if let Some(class_info) = self.type_registry.get_class(actual_name) {
            return class_info.interfaces.iter().any(|i| {
                let bare_name = match i {
                    Type::Object(name) | Type::Generic(name, _) => {
                        name.split('<').next().unwrap_or(name)
                    }
                    _ => &format!("{}", i),
                };
                bare_name == expected_name
            });
        }

        false
    }

    /// 检查类型列表是否匹配
    fn types_match(&self, types1: &[Type], types2: &[Type]) -> bool {
        if types1.len() != types2.len() {
            return false;
        }

        types1.iter().zip(types2.iter()).all(|(t1, t2)| t1 == t2)
    }

    /// 收集 struct 定义并注册到 TypeRegistry
    pub fn collect_structs(&mut self, program: &Program) -> CayResult<()> {
        for struct_decl in &program.structs {
            let mut struct_info = crate::types::StructInfo {
                name: struct_decl.name.clone(),
                type_params: struct_decl
                    .type_params
                    .iter()
                    .map(|tp| crate::types::TypeParamInfo {
                        name: tp.name.clone(),
                        bound: tp.bound.clone(),
                        default_type: tp.default_type.clone(),
                    })
                    .collect(),
                fields: std::collections::HashMap::new(),
                field_order: Vec::new(),
                methods: std::collections::HashMap::new(),
                constructors: struct_decl
                    .constructors
                    .iter()
                    .map(|ctor| crate::types::ConstructorInfo {
                        params: self
                            .replace_params_type_params(&ctor.params, &struct_decl.type_params),
                        is_public: ctor
                            .modifiers
                            .iter()
                            .any(|m| matches!(m, Modifier::Public)),
                        is_private: ctor
                            .modifiers
                            .iter()
                            .any(|m| matches!(m, Modifier::Private)),
                        is_protected: ctor
                            .modifiers
                            .iter()
                            .any(|m| matches!(m, Modifier::Protected)),
                        loc: ctor.loc.clone(),
                    })
                    .collect(),
                is_public: struct_decl
                    .modifiers
                    .iter()
                    .any(|m| matches!(m, Modifier::Public)),
            };

            // 收集字段（保持定义顺序）
            for field in &struct_decl.fields {
                let field_info = crate::types::FieldInfo {
                    name: field.name.clone(),
                    // 将泛型参数替换为 GenericParam 类型
                    field_type: self.replace_type_params(&field.field_type, &struct_decl.type_params),
                    is_public: true, // struct 字段默认公开
                    is_private: false,
                    is_protected: false,
                    is_static: false,
                    is_final: false,
                    is_const_expr: false,
                };
                struct_info.fields.insert(field.name.clone(), field_info);
                struct_info.field_order.push(field.name.clone());
            }

            // 收集方法
            for method in &struct_decl.methods {
                let method_info = MethodInfo {
                    name: method.name.clone(),
                    class_name: struct_decl.name.clone(),
                    type_params: Vec::new(),
                    params: self
                        .replace_params_type_params(&method.params, &struct_decl.type_params),
                    return_type: self
                        .replace_type_params(&method.return_type, &struct_decl.type_params),
                    is_public: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Public)),
                    is_private: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Private)),
                    is_protected: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Protected)),
                    is_static: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Static)),
                    is_native: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Native)),
                    is_const: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Const)),
                    is_abstract: false,
                    is_override: false,
                    is_final: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Final)),
                    is_test: false,
                    vtable_slot: None,
                    loc: method.loc.clone(),
                };
                struct_info
                    .methods
                    .entry(method.name.clone())
                    .or_insert_with(Vec::new)
                    .push(method_info);
            }

            let (file, line) = self.resolve_file_and_line(struct_decl.loc.line);
            self.type_registry
                .register_struct(struct_info, file, line, struct_decl.loc.column)?;
        }
        Ok(())
    }

    /// 收集 enum 定义并注册到 TypeRegistry
    pub fn collect_enums(&mut self, program: &Program) -> CayResult<()> {
        for enum_decl in &program.enums {
            let variants = enum_decl
                .variants
                .iter()
                .map(|v| crate::types::EnumVariantInfo {
                    name: v.name.clone(),
                    // 将泛型参数替换为 GenericParam 类型（如 Object("T") -> GenericParam("T")），
                    // 以便在 variant 构造时按类型实参替换 payload 类型。
                    payload_type: v
                        .payload_type
                        .as_ref()
                        .map(|pt| self.replace_type_params(pt, &enum_decl.type_params)),
                })
                .collect();

            let mut enum_info = crate::types::EnumInfo {
                name: enum_decl.name.clone(),
                type_params: enum_decl
                    .type_params
                    .iter()
                    .map(|p| crate::types::TypeParamInfo {
                        name: p.name.clone(),
                        bound: p.bound.clone(),
                        default_type: p.default_type.clone(),
                    })
                    .collect(),
                variants,
                methods: std::collections::HashMap::new(),
                is_public: enum_decl
                    .modifiers
                    .iter()
                    .any(|m| matches!(m, Modifier::Public)),
            };

            // 收集 enum 方法（可来自 impl 块）
            for method in &enum_decl.methods {
                let method_info = MethodInfo {
                    name: method.name.clone(),
                    class_name: enum_decl.name.clone(),
                    type_params: Vec::new(),
                    params: self
                        .replace_params_type_params(&method.params, &enum_decl.type_params),
                    return_type: self
                        .replace_type_params(&method.return_type, &enum_decl.type_params),
                    is_public: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Public)),
                    is_private: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Private)),
                    is_protected: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Protected)),
                    is_static: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Static)),
                    is_native: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Native)),
                    is_const: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Const)),
                    is_abstract: false,
                    is_override: false,
                    is_final: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Final)),
                    is_test: false,
                    vtable_slot: None,
                    loc: method.loc.clone(),
                };
                enum_info
                    .methods
                    .entry(method.name.clone())
                    .or_insert_with(Vec::new)
                    .push(method_info);
            }

            let (file, line) = self.resolve_file_and_line(enum_decl.loc.line);
            self.type_registry
                .register_enum(enum_info, file, line, enum_decl.loc.column)?;
        }
        Ok(())
    }

    /// 检查 @FreeFunction 冲突
    /// 当两个不同类中的方法都标记了 @FreeFunction 且同名时，报错
    pub fn check_free_function_conflicts(&mut self, program: &Program) -> CayResult<()> {
        use crate::ast::Modifier;

        for class_decl in &program.classes {
            for member in &class_decl.members {
                if let ClassMember::Method(method) = member {
                    if method.modifiers.contains(&Modifier::FreeFunction) {
                        // 构建 MethodInfo 用于注册
                        let method_info = MethodInfo {
                            name: method.name.clone(),
                            class_name: class_decl.name.clone(),
                            type_params: Vec::new(),
                            params: method.params.clone(),
                            return_type: method.return_type.clone(),
                            is_public: method.modifiers.contains(&Modifier::Public),
                            is_private: method.modifiers.contains(&Modifier::Private),
                            is_protected: method.modifiers.contains(&Modifier::Protected),
                            is_static: method.modifiers.contains(&Modifier::Static),
                            is_native: false,
                            is_const: false,
                            is_abstract: false,
                            is_override: false,
                            is_final: false,
                            is_test: false,
                            vtable_slot: None,
                            loc: method.loc.clone(),
                        };

                        // 获取源映射后的位置
                        let (file, line) = self.resolve_file_and_line(method.loc.line);
                        let loc = crate::miette_diagnostic::SourceLocation {
                            file,
                            line,
                            column: method.loc.column,
                        };

                        // 计算限定类名（包含命名空间路径）
                        let qualified_class_name = if class_decl.namespace_path.is_empty() {
                            class_decl.name.clone()
                        } else {
                            format!(
                                "{}::{}",
                                class_decl.namespace_path.join("::"),
                                class_decl.name
                            )
                        };

                        // 注册到 TypeRegistry（内部会检查冲突）
                        self.type_registry.register_free_function(
                            &method.name,
                            &qualified_class_name,
                            method_info.clone(),
                            loc.clone(),
                        )?;

                        // 如果在命名空间内，同时注册命名空间限定名
                        // 例如: namespace math 中的 square → 也注册 math::square
                        if !class_decl.namespace_path.is_empty() {
                            let qualified_name = format!(
                                "{}::{}",
                                class_decl.namespace_path.join("::"),
                                method.name
                            );
                            let ns_loc = crate::miette_diagnostic::SourceLocation {
                                line: method.loc.line,
                                column: method.loc.column,
                                ..loc
                            };
                            self.type_registry.register_free_function(
                                &qualified_name,
                                &qualified_class_name,
                                method_info,
                                ns_loc,
                            )?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// 计算所有类的 vtable 槽位分配
    ///
    /// 算法：
    /// 1. 从根类开始，父类方法先分配槽位
    /// 2. 子类继承父类的槽位布局
    /// 3. 子类新增方法追加到末尾
    /// 4. 重写方法复用父类的槽位
    pub fn compute_vtable_layouts(&mut self) {
        // 收集所有类名，按继承深度排序（父类在前）
        let class_names: Vec<String> = self.type_registry.classes.keys().cloned().collect();

        // 计算每个类的继承深度
        let mut depth_map = std::collections::HashMap::new();
        for name in &class_names {
            let depth = self.compute_class_depth(name);
            depth_map.insert(name.clone(), depth);
        }

        // 按深度排序（深度小的先处理，即父类先处理）
        let mut sorted_classes = class_names;
        sorted_classes.sort_by_key(|name| depth_map.get(name).unwrap_or(&0));

        // 为每个类计算 vtable 布局
        for class_name in &sorted_classes {
            // 先用不可变借用判断是否互操作类，避免与下方 get_mut 冲突
            let is_interop = self
                .type_registry
                .classes
                .get(class_name)
                .map(|c| c.is_interop)
                .unwrap_or(false);
            if is_interop {
                if let Some(class_info) = self.type_registry.classes.get_mut(class_name) {
                    // C++ 互操作类无对象头，不生成 vtable（虚函数派发/instanceof 对其不可用）
                    class_info.vtable_layout = None;
                }
                continue;
            }
            let layout = self.compute_single_class_vtable(class_name);
            if let Some(class_info) = self.type_registry.classes.get_mut(class_name) {
                class_info.vtable_layout = Some(layout);
            }
        }

        let max_class_vtable_size = self
            .type_registry
            .classes
            .values()
            .filter_map(|c| c.vtable_layout.as_ref().map(|v| v.size))
            .max()
            .unwrap_or(0);

        self.compute_interface_vtable_slots(max_class_vtable_size);
        self.attach_interface_slots_to_class_vtables(&sorted_classes);
    }

    /// 计算类的继承深度
    fn compute_class_depth(&self, class_name: &str) -> usize {
        let mut depth = 0;
        let mut current = class_name.to_string();
        // 防止循环继承导致死循环
        let mut visited = std::collections::HashSet::new();

        while let Some(class_info) = self.type_registry.get_class(&current) {
            if !visited.insert(current.clone()) {
                break; // 检测到循环继承
            }
            if let Some(ref parent) = class_info.parent {
                depth += 1;
                current = parent.clone();
            } else {
                break;
            }
        }

        depth
    }

    /// 计算单个类的 vtable 布局
    fn compute_single_class_vtable(&self, class_name: &str) -> crate::types::VTableLayout {
        let mut slots = std::collections::HashMap::new();
        let mut next_slot = 0;

        // 如果有父类，先复制父类的 vtable 布局
        if let Some(class_info) = self.type_registry.get_class(class_name) {
            if let Some(ref parent_name) = class_info.parent {
                if let Some(parent_info) = self.type_registry.get_class(parent_name) {
                    if let Some(ref parent_vtable) = parent_info.vtable_layout {
                        // 复制父类的所有槽位
                        for (method_sig, slot) in &parent_vtable.slots {
                            slots.insert(method_sig.clone(), *slot);
                        }
                        next_slot = parent_vtable.size;
                    }
                }
            }
        }

        // 收集当前类的虚方法（非 static、非 native、非 private）
        // 虚方法：实例方法 + 非 final + 非 native
        // 为每个重载方法分配独立的槽位，使用方法签名（名字+参数类型）作为键
        if let Some(class_info) = self.type_registry.get_class(class_name) {
            // 收集所有虚方法签名
            let mut instance_method_sigs: Vec<String> = Vec::new();
            for (method_name, methods) in &class_info.methods {
                for method in methods {
                    // 只收集非 static、非 native、非 private 的实例方法
                    if !method.is_static && !method.is_native && !method.is_private {
                        let sig = crate::types::TypeRegistry::build_method_signature(
                            method_name,
                            &method.params,
                        );
                        if !instance_method_sigs.contains(&sig) {
                            instance_method_sigs.push(sig);
                        }
                    }
                }
            }
            // 排序确保 vtable 槽位分配一致
            instance_method_sigs.sort();

            // 为每个虚方法签名分配槽位
            for method_sig in &instance_method_sigs {
                if !slots.contains_key(method_sig) {
                    // 新方法，分配新槽位
                    slots.insert(method_sig.clone(), next_slot);
                    next_slot += 1;
                }
                // 如果已经存在（从父类继承），保持原槽位（重写）
            }
        }

        crate::types::VTableLayout {
            class_name: class_name.to_string(),
            slots,
            size: next_slot,
        }
    }

    fn compute_interface_vtable_slots(&mut self, start_slot: usize) {
        let mut entries = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Phase 1: 非泛型接口——保持原有行为：每个 (接口, 方法) 分配一个槽位。
        // 泛型接口的「裸名」槽位也在此阶段登记，用作未特化查找路径的回退键，
        // 但实际特化类会同时持有按类型实参区分的独立槽位（Phase 2）。
        let mut interface_names: Vec<String> =
            self.type_registry.interfaces.keys().cloned().collect();
        interface_names.sort();

        for interface_name in &interface_names {
            if let Some(interface_info) = self.type_registry.get_interface(interface_name) {
                let mut method_entries: Vec<(String, String)> = interface_info
                    .methods
                    .iter()
                    .map(|(method_name, method)| {
                        (
                            method_name.clone(),
                            crate::types::TypeRegistry::build_method_signature(
                                method_name,
                                &method.params,
                            ),
                        )
                    })
                    .collect();
                method_entries.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));

                for (_, method_sig) in method_entries {
                    let key = crate::types::TypeRegistry::build_interface_vtable_key(
                        interface_name,
                        &method_sig,
                    );
                    if seen.insert(key.clone()) {
                        entries.push(key);
                    }
                }
            }
        }

        // Phase 2: 泛型接口实例化——为每个被类实际实现的 (接口, 类型实参, 方法)
        // 元组分配独立槽位。这一步让 `Into<IOError>::into` 与 `Into<ParseError>::into`
        // 各占独立槽位，避免动态分派只命中其一的 bug。
        // 对非泛型接口（type_args 为空），其槽位已在 Phase 1 登记，跳过。
        //
        // 仅当类型实参全部为具体类型时才分配独立槽位。
        // 含类型参数的实例化（如 `Iterator<T>`，其中 `T` 是类的类型参数）的槽位键
        // 与调用点（如 `Iterator<int>`）不匹配，会破坏 vtable 布局；这类接口使用
        // Phase 1 的裸名槽位即可（attach 阶段与调用点都会回退到裸键）。
        //
        // 注意：解析器将接口类型实参 `T` 存储为 `Type::Object("T")` 而非
        // `Type::GenericParam("T")`，故需同时检查两种形式，并回查类的
        // `type_params` 以识别「名字与类类型参数同名」的 Object 实例。
        let mut class_names: Vec<String> = self.type_registry.classes.keys().cloned().collect();
        class_names.sort();

        for class_name in &class_names {
            if let Some(class_info) = self.type_registry.get_class(class_name) {
                // 收集该类的类型参数名，用于识别 Object("T") 形式的类型参数
                let class_type_param_names: std::collections::HashSet<&str> = class_info
                    .type_params
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect();
                for interface in &class_info.interfaces {
                    let (interface_name, type_args) = match interface {
                        crate::types::Type::Generic(name, args) if !args.is_empty() => {
                            (name.clone(), args.clone())
                        }
                        _ => continue,
                    };
                    // 跳过含类型参数的实例化（如 Iterator<T>），仅处理具体类型实例化。
                    // 同时识别 GenericParam 与 Object("T")（T 是类类型参数）两种形式。
                    let is_type_param_instantiation = type_args.iter().any(|t| match t {
                        crate::types::Type::GenericParam(_) => true,
                        crate::types::Type::Object(name) => {
                            class_type_param_names.contains(name.as_str())
                        }
                        _ => false,
                    });
                    if is_type_param_instantiation {
                        continue;
                    }
                    if let Some(interface_info) =
                        self.type_registry.get_interface(&interface_name)
                    {
                        for (method_name, method) in &interface_info.methods {
                            let method_sig =
                                crate::types::TypeRegistry::build_method_signature(
                                    method_name,
                                    &method.params,
                                );
                            let key = crate::types::TypeRegistry::build_interface_vtable_key_with_type_args(
                                &interface_name,
                                &type_args,
                                &method_sig,
                            );
                            if seen.insert(key.clone()) {
                                entries.push(key);
                            }
                        }
                    }
                }
            }
        }

        // 排序确保 vtable 槽位分配在多次编译间一致
        entries.sort();

        self.type_registry.interface_vtable_slots.clear();
        for (offset, key) in entries.into_iter().enumerate() {
            self.type_registry
                .interface_vtable_slots
                .insert(key, start_slot + offset);
        }
    }

    fn attach_interface_slots_to_class_vtables(&mut self, class_names: &[String]) {
        for class_name in class_names {
            let interfaces = self.collect_all_interface_types_for_class(class_name);
            if interfaces.is_empty() {
                continue;
            }

            let mut additions = Vec::new();
            for interface in interfaces {
                let (interface_name, type_args) = match &interface {
                    crate::types::Type::Generic(name, args) => {
                        (name.clone(), args.clone())
                    }
                    crate::types::Type::Object(name) => (name.clone(), Vec::new()),
                    _ => continue,
                };
                if let Some(interface_info) =
                    self.type_registry.get_interface(&interface_name)
                {
                    for (method_name, method) in &interface_info.methods {
                        let method_sig = crate::types::TypeRegistry::build_method_signature(
                            method_name,
                            &method.params,
                        );
                        let key = if type_args.is_empty() {
                            crate::types::TypeRegistry::build_interface_vtable_key(
                                &interface_name,
                                &method_sig,
                            )
                        } else {
                            crate::types::TypeRegistry::build_interface_vtable_key_with_type_args(
                                &interface_name,
                                &type_args,
                                &method_sig,
                            )
                        };
                        // 先按特化键查找；若失败，回退到裸键查找
                        // （兼容含类型参数的实例化，如 Iterator<T>，
                        // 其特化键未在 Phase 2 分配，使用 Phase 1 裸名槽位）
                        let slot = self
                            .type_registry
                            .interface_vtable_slots
                            .get(&key)
                            .copied()
                            .or_else(|| {
                                if !type_args.is_empty() {
                                    let bare_key =
                                        crate::types::TypeRegistry::build_interface_vtable_key(
                                            &interface_name,
                                            &method_sig,
                                        );
                                    self.type_registry
                                        .interface_vtable_slots
                                        .get(&bare_key)
                                        .copied()
                                } else {
                                    None
                                }
                            });
                        if let Some(slot) = slot {
                            additions.push((key, slot));
                        }
                    }
                }
            }

            if let Some(class_info) = self.type_registry.classes.get_mut(class_name) {
                if let Some(layout) = class_info.vtable_layout.as_mut() {
                    for (key, slot) in additions {
                        layout.slots.insert(key, slot);
                        layout.size = layout.size.max(slot + 1);
                    }
                }
            }
        }
    }

    /// 收集类（沿父类链）实现的所有接口，保留泛型类型实参。
    /// 与旧的 `collect_all_interfaces_for_class` 不同，这里返回 `Vec<Type>`
    /// 而非 `Vec<String>`，使下游可以按 `Into<IOError>` 与 `Into<ParseError>`
    /// 分别分配/查找 vtable 槽位。
    fn collect_all_interface_types_for_class(&self, class_name: &str) -> Vec<crate::types::Type> {
        let mut interfaces: Vec<crate::types::Type> = Vec::new();
        let mut current = class_name.to_string();

        while let Some(class_info) = self.type_registry.get_class(&current) {
            for interface in &class_info.interfaces {
                if !interfaces.iter().any(|existing| existing == interface) {
                    interfaces.push(interface.clone());
                }
            }
            if let Some(parent) = &class_info.parent {
                current = parent.clone();
            } else {
                break;
            }
        }

        // 注：Type 未实现 Ord，故不排序；下游按接口名+类型实参查询 vtable 槽位，
        // 顺序对最终槽位分配无影响（槽位编号由全局 interface_vtable_slots 表决定）。
        interfaces
    }
}

/// 从接口类型中提取基础接口名（如 Iterator<T> -> Iterator）。
fn interface_type_name(interface_type: &crate::types::Type) -> String {
    match interface_type {
        crate::types::Type::Object(name) | crate::types::Type::Generic(name, _) => {
            name.split('<').next().unwrap_or(name).to_string()
        }
        _ => format!("{}", interface_type),
    }
}

/// 将接口方法签名中的类型参数替换为类提供的具体实参。
///
/// 时间复杂度 O(k)，k 为类型 AST 节点数；空间复杂度 O(k)（递归创建新类型节点）。
fn substitute_interface_type(
    ty: &crate::types::Type,
    interface_type_params: &[crate::types::TypeParamInfo],
    type_args: &[crate::types::Type],
) -> crate::types::Type {
    match ty {
        crate::types::Type::GenericParam(name) => {
            if let Some(idx) = interface_type_params.iter().position(|p| &p.name == name) {
                type_args.get(idx).cloned().unwrap_or(ty.clone())
            } else {
                ty.clone()
            }
        }
        crate::types::Type::Generic(name, args) => crate::types::Type::Generic(
            name.clone(),
            args.iter()
                .map(|a| substitute_interface_type(a, interface_type_params, type_args))
                .collect(),
        ),
        crate::types::Type::Array(elem) => crate::types::Type::Array(Box::new(
            substitute_interface_type(elem, interface_type_params, type_args),
        )),
        crate::types::Type::Pointer(inner) => crate::types::Type::Pointer(Box::new(
            substitute_interface_type(inner, interface_type_params, type_args),
        )),
        _ => ty.clone(),
    }
}
