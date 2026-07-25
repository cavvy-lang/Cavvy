//! 类定义、继承关系分析和主类冲突分析

use super::analyzer::SemanticAnalyzer;
use crate::ast::{ClassMember, MethodDecl, Modifier, Program};
use crate::miette_diagnostic::{CayResult, ErrorCodes, SourceLocation, semantic_error, semantic_error_with_file};
use crate::types::{ClassInfo, FieldInfo, MethodInfo, ParameterInfo, Type};

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
                let marked_classes: Vec<&(String, bool)> =
                    main_classes.iter().filter(|(_, marked)| *marked).collect();

                match marked_classes.len() {
                    0 => {
                        // 多个类有 main，但没有标记 @main
                        let class_names: Vec<String> =
                            main_classes.iter().map(|(name, _)| name.clone()).collect();
                        Err(semantic_error_with_file(
                            ErrorCodes::SEMANTIC_INVALID_OPERATION,
                            None,
                            0,
                            0,
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
                            .map(|(name, _)| name.clone())
                            .collect();
                        Err(semantic_error_with_file(
                            ErrorCodes::SEMANTIC_INVALID_OPERATION,
                            None,
                            0,
                            0,
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
                    is_abstract: false, // 接口方法在Cavvy中视为非抽象（有默认实现机制）
                    is_override: false,
                    is_final: false, // 接口方法不是final
                    is_test: false,  // 接口方法不能是 @Test
                    vtable_slot: None,
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
                        };
                        // 检查是否存在签名完全相同的构造函数（重复定义）
                        for existing in &class_info.constructors {
                            if existing.params.len() == params.len() {
                                let same_params =
                                    existing.params.iter().zip(params.iter()).all(|(a, b)| {
                                        a.param_type == b.param_type && a.is_varargs == b.is_varargs
                                    });
                                if same_params {
                                    return Err(semantic_error_with_file(
                                        ErrorCodes::SEMANTIC_INVALID_OPERATION,
                                        self.current_file.clone(),
                                        ctor.loc.line,
                                        ctor.loc.column,
                                        "构造函数已被定义，不能重复定义相同签名的构造函数"
                                            .to_string(),
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

    /// 分析方法定义
    pub fn analyze_methods(&mut self, program: &Program) -> CayResult<()> {
        for class in &program.classes {
            self.current_class = Some(class.name.clone());
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
                        is_abstract: method.modifiers.contains(&Modifier::Abstract),
                        is_override: method.modifiers.contains(&Modifier::Override),
                        is_final: method.modifiers.contains(&Modifier::Final),
                        is_test,
                        vtable_slot: None,
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
                                    "@Test 方法 '{}' 的返回类型必须是 void，当前为 {}\n提示: 将返回类型改为 void，例如: public void {}(, ErrorCodes::get_suggestion(ErrorCodes::SEMANTIC_INVALID_OPERATION).to_string())",
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
                                    "@Test 方法 '{}' 不能有参数（发现 {} 个参数）\n提示: 移除参数，例如: public void {}(, ErrorCodes::get_suggestion(ErrorCodes::SEMANTIC_INVALID_OPERATION).to_string())",
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
                                    "@Test 方法 '{}' 不能是 private\n提示: 将 private 改为 public，例如: public void {}(, ErrorCodes::get_suggestion(ErrorCodes::SEMANTIC_INVALID_OPERATION).to_string())",
                                    method.name, method.name
                                ),
                            ));
                        }
                    }

                    if let Some(class_info) = self.type_registry.get_class_mut(&class.name) {
                        // 检查是否存在签名完全相同的方法（重复定义）
                        if let Some(existing_methods) = class_info.methods.get(&method_info.name) {
                            for existing in existing_methods {
                                if existing.params.len() == method_info.params.len() {
                                    let same_params =
                                        existing.params.iter().zip(method_info.params.iter()).all(
                                            |(a, b)| {
                                                a.param_type == b.param_type
                                                    && a.is_varargs == b.is_varargs
                                            },
                                        );
                                    if same_params {
                                        return Err(semantic_error_with_file(
                                            ErrorCodes::SEMANTIC_INVALID_OPERATION,
                                            self.current_file.clone(),
                                            method.loc.line,
                                            method.loc.column,
                                            format!(
                                                "方法 '{}' 已被定义，不能重复定义相同签名的方法",
                                                method.name
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
            self.check_circular_inheritance(&class.name, &class.name, &mut Vec::new())?;
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

    /// 递归检查循环继承
    fn check_circular_inheritance(
        &self,
        original: &str,
        current: &str,
        visited: &mut Vec<String>,
    ) -> CayResult<()> {
        if visited.contains(&current.to_string()) {
            return Err(semantic_error_with_file(
                ErrorCodes::SEMANTIC_INVALID_OPERATION,
                None,
                0,
                0,
                format!(
                    "Circular inheritance detected involving class '{}'",
                    original
                ),
            ));
        }

        if let Some(class_info) = self.type_registry.get_class(current) {
            if let Some(ref parent_name) = class_info.parent {
                visited.push(current.to_string());
                self.check_circular_inheritance(original, parent_name, visited)?;
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
                    is_abstract: false,
                    is_override: false,
                    is_final: method
                        .modifiers
                        .iter()
                        .any(|m| matches!(m, Modifier::Final)),
                    is_test: false,
                    vtable_slot: None,
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

            let enum_info = crate::types::EnumInfo {
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
                            is_abstract: false,
                            is_override: false,
                            is_final: false,
                            is_test: false,
                            vtable_slot: None,
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

        while let Some(class_info) = self.type_registry.get_class(&current) {
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
        let mut interface_names: Vec<String> =
            self.type_registry.interfaces.keys().cloned().collect();
        interface_names.sort();

        for interface_name in interface_names {
            if let Some(interface_info) = self.type_registry.get_interface(&interface_name) {
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
                        &interface_name,
                        &method_sig,
                    );
                    entries.push(key);
                }
            }
        }

        self.type_registry.interface_vtable_slots.clear();
        for (offset, key) in entries.into_iter().enumerate() {
            self.type_registry
                .interface_vtable_slots
                .insert(key, start_slot + offset);
        }
    }

    fn attach_interface_slots_to_class_vtables(&mut self, class_names: &[String]) {
        for class_name in class_names {
            let interfaces = self.collect_all_interfaces_for_class(class_name);
            if interfaces.is_empty() {
                continue;
            }

            let mut additions = Vec::new();
            for interface_name in interfaces {
                if let Some(interface_info) = self.type_registry.get_interface(&interface_name) {
                    for (method_name, method) in &interface_info.methods {
                        let method_sig = crate::types::TypeRegistry::build_method_signature(
                            method_name,
                            &method.params,
                        );
                        let key = crate::types::TypeRegistry::build_interface_vtable_key(
                            &interface_name,
                            &method_sig,
                        );
                        if let Some(slot) = self.type_registry.interface_vtable_slots.get(&key) {
                            additions.push((key, *slot));
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

    fn collect_all_interfaces_for_class(&self, class_name: &str) -> Vec<String> {
        let mut interfaces = std::collections::HashSet::new();
        let mut current = class_name.to_string();

        while let Some(class_info) = self.type_registry.get_class(&current) {
            for interface in &class_info.interfaces {
                interfaces.insert(interface_type_name(interface));
            }
            if let Some(parent) = &class_info.parent {
                current = parent.clone();
            } else {
                break;
            }
        }

        let mut result: Vec<String> = interfaces.into_iter().collect();
        result.sort();
        result
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
