//! 类定义、继承关系分析和主类冲突分析

use crate::ast::{Program, ClassMember, Modifier, MethodDecl};
use crate::types::{ClassInfo, FieldInfo, MethodInfo, ParameterInfo, Type};
use crate::error::{cayResult, semantic_error, semantic_error_with_file};
use super::analyzer::SemanticAnalyzer;

impl SemanticAnalyzer {
    /// 检查主类冲突
    /// 规则：
    /// 1. 如果只有一个类有 main 方法，自动选为主类
    /// 2. 如果有多个类有 main 方法：
    ///    - 如果只有一个类标记了 @main，选该类为主类
    ///    - 如果有多个类标记了 @main，报错
    ///    - 如果没有类标记 @main，报错并提示使用 @main
    pub fn check_main_class_conflicts(&mut self, program: &Program) -> cayResult<()> {
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
                        Err(semantic_error_with_file(
                            None, 0, 0,
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
                        Err(semantic_error_with_file(
                            None, 0, 0,
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

    /// 收集类定义
    pub fn collect_classes(&mut self, program: &Program) -> cayResult<()> {
        // 首先收集接口定义
        for interface in &program.interfaces {
            let mut interface_info = crate::types::InterfaceInfo::new(interface.name.clone());

            // 收集接口方法
            for method in &interface.methods {
                let method_info = MethodInfo {
                    name: method.name.clone(),
                    class_name: interface.name.clone(),
                    params: method.params.clone(),
                    return_type: method.return_type.clone(),
                    is_public: true,  // 接口方法默认是public
                    is_private: false,
                    is_protected: false,
                    is_static: false,
                    is_native: false,
                    is_override: false,
                    is_final: false,  // 接口方法不是final
                    is_test: false,   // 接口方法不能是 @Test
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
            let mut class_info = ClassInfo {
                name: class.name.clone(),
                type_params: class.type_params.clone(),
                methods: std::collections::HashMap::new(),
                fields: std::collections::HashMap::new(),
                constructors: Vec::new(),
                has_destructor: false,
                parent: class.parent.clone(),
                interfaces: class.interfaces.clone(),
                is_abstract,
                is_final,
            };

            // 收集字段信息
            for member in &class.members {
                match member {
                    ClassMember::Field(field) => {
                        let is_final = field.modifiers.contains(&Modifier::Final);
                        let is_static = field.modifiers.contains(&Modifier::Static);
                        // static final 字段且初始化值为字面量时，标记为编译期常量
                        let is_const_expr = is_static && is_final && field.initializer.as_ref().map_or(false, |e| {
                            matches!(e, crate::ast::Expr::Literal(_))
                        });
                        let field_info = FieldInfo {
                            name: field.name.clone(),
                            field_type: field.field_type.clone(),
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
                        let ctor_info = crate::types::ConstructorInfo {
                            params: ctor.params.clone(),
                            is_public: ctor.modifiers.contains(&Modifier::Public),
                            is_private: ctor.modifiers.contains(&Modifier::Private),
                            is_protected: ctor.modifiers.contains(&Modifier::Protected),
                        };
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
                self.type_registry.set_class_namespace(&qualified_name, class.namespace_path.clone());
            } else {
                self.type_registry.register_class(
                    class_info,
                    file,
                    line,
                    class.loc.column,
                )?;
            }
        }
        Ok(())
    }

    /// 分析方法定义
    pub fn analyze_methods(&mut self, program: &Program) -> cayResult<()> {
        for class in &program.classes {
            self.current_class = Some(class.name.clone());
            self.type_registry.current_namespace = class.namespace_path.clone();

            for member in &class.members {
                if let ClassMember::Method(method) = member {
                    let is_test = method.modifiers.contains(&Modifier::Test);
                    
                    let method_info = MethodInfo {
                        name: method.name.clone(),
                        class_name: class.name.clone(),
                        params: method.params.clone(),
                        return_type: method.return_type.clone(),
                        is_public: method.modifiers.contains(&Modifier::Public),
                        is_private: method.modifiers.contains(&Modifier::Private),
                        is_protected: method.modifiers.contains(&Modifier::Protected),
                        is_static: method.modifiers.contains(&Modifier::Static),
                        is_native: method.modifiers.contains(&Modifier::Native),
                        is_override: method.modifiers.contains(&Modifier::Override),
                        is_final: method.modifiers.contains(&Modifier::Final),
                        is_test,
                    };
                    
                    // 验证 @Test 方法签名
                    if is_test {
                        // @Test 方法必须是 void 返回类型
                        if method.return_type != Type::Void {
                            return Err(semantic_error_with_file(
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
                                self.current_file.clone(),
                                method.loc.line,
                                method.loc.column,
                                format!(
                                    "@Test 方法 '{}' 不能有参数（发现 {} 个参数）\n提示: 移除参数，例如: public void {}()",
                                    method.name, method.params.len(), method.name
                                ),
                            ));
                        }
                        // @Test 方法不能是 private
                        if method.modifiers.contains(&Modifier::Private) {
                            return Err(semantic_error_with_file(
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

                    if let Some(class_info) = self.type_registry.get_class_mut(&class.name) {
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
    pub fn check_inheritance(&mut self, program: &Program) -> cayResult<()> {
        // 第一遍：验证所有父类存在
        for class in &program.classes {
            self.type_registry.current_namespace = class.namespace_path.clone();
            if let Some(ref parent_name) = class.parent {
                if !self.type_registry.class_exists(parent_name) {
                    return Err(semantic_error_with_file(
                        class.loc.file.clone(),
                        class.loc.line,
                        class.loc.column,
                        format!("Class '{}' extends undefined class '{}'", class.name, parent_name)
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
                            class.loc.file.clone(),
                            class.loc.line,
                            class.loc.column,
                            format!("Class '{}' cannot inherit from final class '{}'", class.name, parent_name)
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

        Ok(())
    }

    /// 递归检查循环继承
    fn check_circular_inheritance(&self, original: &str, current: &str, visited: &mut Vec<String>) -> cayResult<()> {
        if visited.contains(&current.to_string()) {
            return Err(semantic_error_with_file(
                None, 0, 0,
                format!("Circular inheritance detected involving class '{}'", original)
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
    fn check_override_methods(&self, class: &crate::ast::ClassDecl) -> cayResult<()> {
        for member in &class.members {
            if let ClassMember::Method(method) = member {
                if method.modifiers.contains(&Modifier::Override) {
                    // 检查父类是否存在
                    let parent_name = match &class.parent {
                        Some(p) => p,
                        None => {
                            return Err(semantic_error_with_file(
                                method.loc.file.clone(),
                                method.loc.line,
                                method.loc.column,
                                format!("Method '{}' has @Override annotation but class '{}' does not extend any class", 
                                    method.name, class.name)
                            ));
                        }
                    };

                    // 检查父类中是否存在同名方法
                    if !self.method_exists_in_parent(parent_name, &method.name, &method.params, &method.return_type) {
                        return Err(semantic_error_with_file(
                            method.loc.file.clone(),
                            method.loc.line,
                            method.loc.column,
                            format!("Method '{}' has @Override annotation but does not override any method from parent class '{}'", 
                                method.name, parent_name)
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// 检查父类中是否存在匹配的方法
    fn method_exists_in_parent(&self, parent_name: &str, method_name: &str, params: &[ParameterInfo], return_type: &Type) -> bool {
        if let Some(parent_class) = self.type_registry.get_class(parent_name) {
            // 获取参数类型列表
            let param_types: Vec<Type> = params.iter().map(|p| p.param_type.clone()).collect();

            // 在父类中查找方法
            if let Some(methods) = parent_class.methods.get(method_name) {
                for method in methods {
                    // 检查参数数量和类型是否匹配
                    if method.params.len() == params.len() {
                        let parent_param_types: Vec<Type> = method.params.iter().map(|p| p.param_type.clone()).collect();
                        if self.types_match(&parent_param_types, &param_types) && method.return_type == *return_type {
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
    fn check_final_method_override(&self, class: &crate::ast::ClassDecl) -> cayResult<()> {
        // 获取父类名
        let parent_name = match &class.parent {
            Some(p) => p,
            None => return Ok(()), // 没有父类，不需要检查
        };

        // 检查类中是否有方法重写了父类的 final 方法
        for member in &class.members {
            if let ClassMember::Method(method) = member {
                let param_types: Vec<Type> = method.params.iter().map(|p| p.param_type.clone()).collect();
                
                // 在父类及其祖先中查找同名的 final 方法
                if let Err(e) = self.check_final_method_in_ancestors(
                    parent_name,
                    &method.name,
                    &param_types,
                    method.loc.line,
                    method.loc.column
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
        column: usize
    ) -> cayResult<()> {
        if let Some(parent_class) = self.type_registry.get_class(parent_name) {
            // 在父类中查找方法
            if let Some(methods) = parent_class.methods.get(method_name) {
                for method in methods {
                    // 检查参数类型是否匹配
                    if method.params.len() == param_types.len() {
                        let parent_param_types: Vec<Type> = method.params.iter()
                            .map(|p| p.param_type.clone())
                            .collect();
                        
                        if self.types_match(&parent_param_types, param_types) {
                            // 找到匹配的方法，检查是否是 final
                            if method.is_final {
                                return Err(semantic_error_with_file(
                                    None, line, column,
                                    format!(
                                        "Method '{}' cannot override final method from class '{}'",
                                        method_name, parent_name
                                    )
                                ));
                            }
                        }
                    }
                }
            }

            // 递归检查父类的父类
            if let Some(ref grandparent) = parent_class.parent {
                return self.check_final_method_in_ancestors(
                    grandparent, method_name, param_types, line, column
                );
            }
        }

        Ok(())
    }

    /// 检查类型列表是否匹配
    fn types_match(&self, types1: &[Type], types2: &[Type]) -> bool {
        if types1.len() != types2.len() {
            return false;
        }

        types1.iter().zip(types2.iter()).all(|(t1, t2)| t1 == t2)
    }

    /// 收集 struct 定义并注册到 TypeRegistry
    pub fn collect_structs(&mut self, program: &Program) -> cayResult<()> {
        for struct_decl in &program.structs {
            let mut struct_info = crate::types::StructInfo {
                name: struct_decl.name.clone(),
                fields: std::collections::HashMap::new(),
                methods: std::collections::HashMap::new(),
                is_public: struct_decl.modifiers.iter().any(|m| matches!(m, Modifier::Public)),
            };

            // 收集字段
            for field in &struct_decl.fields {
                let field_info = crate::types::FieldInfo {
                    name: field.name.clone(),
                    field_type: field.field_type.clone(),
                    is_public: true,  // struct 字段默认公开
                    is_private: false,
                    is_protected: false,
                    is_static: false,
                    is_final: false,
                    is_const_expr: false,
                };
                struct_info.fields.insert(field.name.clone(), field_info);
            }

            // 收集方法
            for method in &struct_decl.methods {
                let method_info = MethodInfo {
                    name: method.name.clone(),
                    class_name: struct_decl.name.clone(),
                    params: method.params.clone(),
                    return_type: method.return_type.clone(),
                    is_public: method.modifiers.iter().any(|m| matches!(m, Modifier::Public)),
                    is_private: method.modifiers.iter().any(|m| matches!(m, Modifier::Private)),
                    is_protected: method.modifiers.iter().any(|m| matches!(m, Modifier::Protected)),
                    is_static: method.modifiers.iter().any(|m| matches!(m, Modifier::Static)),
                    is_native: method.modifiers.iter().any(|m| matches!(m, Modifier::Native)),
                    is_override: false,
                    is_final: method.modifiers.iter().any(|m| matches!(m, Modifier::Final)),
                    is_test: false,
                };
                struct_info.methods
                    .entry(method.name.clone())
                    .or_insert_with(Vec::new)
                    .push(method_info);
            }

            let (file, line) = self.resolve_file_and_line(struct_decl.loc.line);
            self.type_registry.register_struct(
                struct_info,
                file,
                line,
                struct_decl.loc.column,
            )?;
        }
        Ok(())
    }

    /// 收集 enum 定义并注册到 TypeRegistry
    pub fn collect_enums(&mut self, program: &Program) -> cayResult<()> {
        for enum_decl in &program.enums {
            let variants = enum_decl.variants.iter().map(|v| crate::types::EnumVariantInfo {
                name: v.name.clone(),
                payload_type: v.payload_type.clone(),
            }).collect();

            let enum_info = crate::types::EnumInfo {
                name: enum_decl.name.clone(),
                type_params: enum_decl.type_params.clone(),
                variants,
                methods: std::collections::HashMap::new(),
                is_public: enum_decl.modifiers.iter().any(|m| matches!(m, Modifier::Public)),
            };

            let (file, line) = self.resolve_file_and_line(enum_decl.loc.line);
            self.type_registry.register_enum(
                enum_info,
                file,
                line,
                enum_decl.loc.column,
            )?;
        }
        Ok(())
    }

    /// 检查 @FreeFunction 冲突
    /// 当两个不同类中的方法都标记了 @FreeFunction 且同名时，报错
    pub fn check_free_function_conflicts(&mut self, program: &Program) -> cayResult<()> {
        use crate::ast::Modifier;
        
        for class_decl in &program.classes {
            for member in &class_decl.members {
                if let ClassMember::Method(method) = member {
                    if method.modifiers.contains(&Modifier::FreeFunction) {
                        // 构建 MethodInfo 用于注册
                        let method_info = MethodInfo {
                            name: method.name.clone(),
                            class_name: class_decl.name.clone(),
                            params: method.params.clone(),
                            return_type: method.return_type.clone(),
                            is_public: method.modifiers.contains(&Modifier::Public),
                            is_private: method.modifiers.contains(&Modifier::Private),
                            is_protected: method.modifiers.contains(&Modifier::Protected),
                            is_static: method.modifiers.contains(&Modifier::Static),
                            is_native: false,
                            is_override: false,
                            is_final: false,
                            is_test: false,
                        };

                        // 获取源映射后的位置
                        let (file, line) = self.resolve_file_and_line(method.loc.line);
                        let loc = crate::error::SourceLocation {
                            file,
                            line,
                            column: method.loc.column,
                        };

                        // 计算限定类名（包含命名空间路径）
                        let qualified_class_name = if class_decl.namespace_path.is_empty() {
                            class_decl.name.clone()
                        } else {
                            format!("{}::{}", class_decl.namespace_path.join("::"), class_decl.name)
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
                            let ns_loc = crate::error::SourceLocation {
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
}
