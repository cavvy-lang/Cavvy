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
            // class.loc.file 包含了类定义所在的实际文件路径
            let file = class.loc.file.clone().or_else(|| self.current_file.clone());
            let line = class.loc.line;
            self.type_registry.register_class(
                class_info,
                file,
                line,
                class.loc.column,
            )?;
        }
        Ok(())
    }

    /// 分析方法定义
    pub fn analyze_methods(&mut self, program: &Program) -> cayResult<()> {
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
                        is_private: method.modifiers.contains(&Modifier::Private),
                        is_protected: method.modifiers.contains(&Modifier::Protected),
                        is_static: method.modifiers.contains(&Modifier::Static),
                        is_native: method.modifiers.contains(&Modifier::Native),
                        is_override: method.modifiers.contains(&Modifier::Override),
                        is_final: method.modifiers.contains(&Modifier::Final),
                    };

                    if let Some(class_info) = self.type_registry.classes.get_mut(&class.name) {
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
}
