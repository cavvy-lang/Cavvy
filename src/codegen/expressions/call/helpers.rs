//! 函数调用表达式代码生成 - 辅助函数
//!
//! 提供各种辅助查询函数，如方法属性检查、字段类型获取等。

use crate::codegen::context::IRGenerator;

/// 简单解析类型实参字符串（与 SpecializationCollector 语义对齐）。
fn parse_type_arg_str(s: &str) -> crate::types::Type {
    use crate::types::Type;
    if s.ends_with("[]") {
        return Type::Array(Box::new(parse_type_arg_str(&s[..s.len() - 2])));
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

impl IRGenerator {
    /// 将类名规范化为类型注册表可查询的名称。
    ///
    /// 隐式单态化的特化类（如 `Optional<int>`）不会注册到类型注册表，
    /// 而其基础类（`Optional`）已注册。若特化名未注册，则回退到去除泛型
    /// 实参的基础类名，使 `is_instance_method`、`class_has_vtable`、
    /// `get_vtable_slot` 等按基础类查询仍能成功（this 传递、vtable 分派）。
    /// 显式特化（已注册特化名）则原样返回，保持既有行为。
    pub fn resolved_class_lookup_name(&self, class_name: &str) -> String {
        if let Some(ref registry) = self.type_registry {
            if registry.get_class(class_name).is_some()
                || registry.get_interface(class_name).is_some()
                || registry.get_struct(class_name).is_some()
                || registry.get_enum_by_name(class_name).is_some()
            {
                return class_name.to_string();
            }
        }
        if let Some(pos) = class_name.find('<') {
            class_name[..pos].to_string()
        } else {
            class_name.to_string()
        }
    }

    /// 根据方法调用接收者表达式的具体泛型类型，将类型参数映射安装到
    /// `generic_type_args`，以便在调用点解析方法签名中的泛型参数
    /// （例如 `Box<int>.get()` 的返回类型 `T` 解析为 `int`）。
    ///
    /// 主循环上下文（如 main 方法）中 `generic_type_args` 为空，若不安装映射，
    /// 泛型方法的返回/参数类型会被降级为 i8*，既产生警告又导致类型不匹配。
    /// 调用方应在方法调用代码生成结束后恢复调用前的映射快照。
    pub fn install_receiver_generic_args(&mut self, obj_expr: &Option<Box<crate::ast::Expr>>) {
        use crate::ast::Expr;
        use crate::types::Type;
        let Some(obj) = obj_expr else {
            return;
        };
        // 支持三种形式：
        // 1. 表达式类型已是 Type::Generic（如变量、new 表达式）。
        // 2. 表达式类型是 Type::Object("Class<T>")（由变量类型推断产生）。
        // 3. 表达式是裸类名标识符且自身就带泛型实参，如 `Rc<Tracked>.fromRaw(...)`。
        //    此时 get_expression_type 对该标识符返回 None，需要直接从标识符名解析。
        let (base, args): (String, Vec<Type>) = match obj.as_ref() {
            Expr::Identifier(name)
                if name.name.contains('<') && name.name.ends_with('>') =>
            {
                let name_str = &name.name;
                if let Some(pos) = name_str.find('<') {
                    let base = name_str[..pos].to_string();
                    let args_str = &name_str[pos + 1..name_str.len() - 1];
                    let args: Vec<Type> = crate::codegen::specialization::split_top_level_type_args(args_str)
                        .iter()
                        .map(|s| parse_type_arg_str(s.trim()))
                        .collect();
                    (base, args)
                } else {
                    return;
                }
            }
            _ => match self.get_expression_type(obj) {
                Some(Type::Generic(base, args)) => (base, args),
                Some(Type::Object(class_name)) => {
                    if class_name.contains('<') && class_name.ends_with('>') {
                        if let Some(pos) = class_name.find('<') {
                            let base = class_name[..pos].to_string();
                            let args_str = &class_name[pos + 1..class_name.len() - 1];
                            let args: Vec<Type> = args_str
                                .split(',')
                                .map(|s| s.trim())
                                .filter(|s| !s.is_empty())
                                .map(|s| parse_type_arg_str(s))
                                .collect();
                            (base, args)
                        } else {
                            return;
                        }
                    } else {
                        return;
                    }
                }
                _ => return,
            },
        };
        if args.is_empty() {
            return;
        }
        // 提取基础类名（去除泛型参数与命名空间前缀）
        let base_name = base.split('<').next().unwrap_or(&base);
        let base_name = base_name
            .rsplit("::")
            .next()
            .unwrap_or(base_name)
            .to_string();
        let type_params = self
            .type_registry
            .as_ref()
            .and_then(|r| r.get_class(&base_name))
            .map(|c| c.type_params.clone())
            .or_else(|| {
                self.type_registry
                    .as_ref()
                    .and_then(|r| r.get_interface(&base_name))
                    .map(|i| i.type_params.clone())
            });
        if let Some(params) = type_params {
            for (idx, param) in params.iter().enumerate() {
                let resolved_arg = args
                    .get(idx)
                    .cloned()
                    .or_else(|| param.default_type.clone())
                    .unwrap_or_else(|| Type::GenericParam(param.name.clone()));
                self.generic_type_args
                    .insert(param.name.clone(), resolved_arg);
            }
        }
    }

    pub fn install_class_generic_args(&mut self, class_name: &str) {
        use crate::types::Type;

        if !class_name.contains('<') || !class_name.ends_with('>') {
            return;
        }

        let Some(pos) = class_name.find('<') else {
            return;
        };
        let gt_pos = class_name.rfind('>').unwrap_or(class_name.len());
        let base = &class_name[..pos];
        let args_str = &class_name[pos + 1..gt_pos];
        let args: Vec<Type> = crate::codegen::specialization::split_top_level_type_args(args_str)
            .iter()
            .map(|s| parse_type_arg_str(s.trim()))
            .collect();
        if args.is_empty() {
            return;
        }

        let base_name = base.split('<').next().unwrap_or(base);
        let bare_name = base_name.rsplit("::").next().unwrap_or(base_name);
        let type_params = self.type_registry.as_ref().and_then(|registry| {
            registry
                .get_class(base_name)
                .map(|c| c.type_params.clone())
                .or_else(|| registry.get_class(bare_name).map(|c| c.type_params.clone()))
                .or_else(|| {
                    registry
                        .get_interface(base_name)
                        .map(|i| i.type_params.clone())
                })
                .or_else(|| {
                    registry
                        .get_interface(bare_name)
                        .map(|i| i.type_params.clone())
                })
        });

        if let Some(params) = type_params {
            for (idx, param) in params.iter().enumerate() {
                let resolved_arg = args
                    .get(idx)
                    .cloned()
                    .or_else(|| param.default_type.clone())
                    .unwrap_or_else(|| Type::GenericParam(param.name.clone()));
                self.generic_type_args
                    .insert(param.name.clone(), resolved_arg);
            }
        }
    }

    /// 获取方法的返回类型
    /// 支持继承查找：如果在当前类中找不到方法，会递归查找父类
    pub fn get_method_return_type(
        &self,
        class_name: &str,
        method_name: &str,
        processed_args: &[String],
        has_varargs_array: bool,
    ) -> crate::types::Type {
        if let Some(best) =
            self.resolve_best_method(class_name, method_name, processed_args, has_varargs_array)
        {
            // 提取调用处的类型实参
            let (base_name, type_args) = if let Some(pos) = class_name.find('<') {
                let gt_pos = class_name.rfind('>').unwrap_or(class_name.len());
                (
                    class_name[..pos].to_string(),
                    crate::codegen::specialization::split_top_level_type_args(
                        &class_name[pos + 1..gt_pos],
                    )
                    .iter()
                    .map(|s| parse_type_arg_str(s.trim()))
                    .collect::<Vec<_>>(),
                )
            } else {
                (class_name.to_string(), Vec::new())
            };

            if let Some(ref registry) = self.type_registry {
                let type_params = registry
                    .get_class(&base_name)
                    .map(|c| c.type_params.clone())
                    .or_else(|| {
                        registry
                            .get_interface(&base_name)
                            .map(|i| i.type_params.clone())
                    })
                    .unwrap_or_default();

                if !type_params.is_empty() && !type_args.is_empty() {
                    let mapping: std::collections::HashMap<String, crate::types::Type> =
                        type_params
                            .iter()
                            .zip(type_args.iter())
                            .map(|(param, arg)| (param.name.clone(), arg.clone()))
                            .collect();
                    return crate::types::substitute_type_params(&best.return_type, &mapping);
                }
            }

            return best.return_type.clone();
        }
        // 默认返回 i64 类型
        crate::types::Type::Int64
    }

    /// 获取方法的形参列表
    pub fn get_method_params(
        &self,
        class_name: &str,
        method_name: &str,
    ) -> Option<Vec<crate::types::ParameterInfo>> {
        let class_name = self.resolved_class_lookup_name(class_name);
        let class_name = class_name.as_str();
        if let Some(ref registry) = self.type_registry {
            if let Some(interface_info) = registry.get_interface(class_name) {
                if let Some(method) = interface_info.methods.get(method_name) {
                    return Some(method.params.clone());
                }
            }

            // enum 方法
            if let Some(enum_info) = registry.get_enum_by_name(class_name) {
                if let Some(methods) = enum_info.methods.get(method_name) {
                    return methods.first().map(|m| m.params.clone());
                }
                return None;
            }

            let mut current = class_name.to_string();
            loop {
                if let Some(class_info) = registry.get_class(&current) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        // 返回第一个匹配的方法
                        return methods.first().map(|m| m.params.clone());
                    }
                    if let Some(ref parent) = class_info.parent {
                        current = parent.clone();
                        continue;
                    }
                } else {
                    // 类查找失败 —— 检查是否是接口类型
                    if let Some(implementor) =
                        registry.find_implementing_class_for_method(&current, method_name)
                    {
                        if let Some(methods) = implementor.methods.get(method_name) {
                            return methods.first().map(|m| m.params.clone());
                        }
                    }
                }
                break;
            }
        }
        None
    }

    /// 获取方法形参个数
    pub fn get_method_param_count(&self, class_name: &str, method_name: &str) -> usize {
        self.get_method_params(class_name, method_name)
            .map(|p| p.len())
            .unwrap_or(0)
    }

    /// 获取可变参数在形参列表中的索引
    pub fn get_varargs_index(&self, class_name: &str, method_name: &str) -> Option<usize> {
        self.get_method_params(class_name, method_name)
            .and_then(|params| params.iter().position(|p| p.is_varargs))
    }

    /// 检查方法是否是可变参数方法
    /// 查询类型注册表来确定方法是否真的是可变参数方法
    pub fn is_varargs_method(&self, class_name: &str, method_name: &str) -> bool {
        let class_name = self.resolved_class_lookup_name(class_name);
        let class_name = class_name.as_str();
        // 查询类型注册表
        if let Some(ref registry) = self.type_registry {
            if let Some(interface_info) = registry.get_interface(class_name) {
                if let Some(method) = interface_info.methods.get(method_name) {
                    return method.params.iter().any(|p| p.is_varargs);
                }
            }

            // enum 方法
            if let Some(enum_info) = registry.get_enum_by_name(class_name) {
                if let Some(methods) = enum_info.methods.get(method_name) {
                    return methods.iter().any(|m| m.params.iter().any(|p| p.is_varargs));
                }
                return false;
            }

            // 先尝试直接查找类
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    for method in methods {
                        if method.params.iter().any(|p| p.is_varargs) {
                            return true;
                        }
                    }
                }
            }
            // 如果是接口类型，查找实现类
            if let Some(implementor) =
                registry.find_implementing_class_for_method(class_name, method_name)
            {
                if let Some(methods) = implementor.methods.get(method_name) {
                    for method in methods {
                        if method.params.iter().any(|p| p.is_varargs) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// 检查方法是否是实例方法（非静态方法）- 支持继承
    pub fn is_instance_method(&self, class_name: &str, method_name: &str) -> bool {
        let class_name = self.resolved_class_lookup_name(class_name);
        let class_name = class_name.as_str();
        // 查询类型注册表，支持继承查找
        if let Some(ref registry) = self.type_registry {
            if let Some(interface_info) = registry.get_interface(class_name) {
                return interface_info.methods.contains_key(method_name);
            }

            // 先查 struct
            if let Some(struct_info) = registry.get_struct(class_name) {
                if let Some(methods) = struct_info.methods.get(method_name) {
                    for method in methods {
                        if !method.is_static {
                            return true;
                        }
                    }
                }
                return false;
            }

            // 查 enum（impl 块添加的实例方法）
            if let Some(enum_info) = registry.get_enum_by_name(class_name) {
                if let Some(methods) = enum_info.methods.get(method_name) {
                    for method in methods {
                        if !method.is_static {
                            return true;
                        }
                    }
                }
                return false;
            }

            let mut current_class_name = class_name.to_string();
            loop {
                if let Some(class_info) = registry.get_class(&current_class_name) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        // 检查是否有任何方法是实例方法（非静态）
                        for method in methods {
                            if !method.is_static {
                                return true;
                            }
                        }
                    }
                    // 在当前类没找到，查找父类
                    if let Some(ref parent_name) = class_info.parent {
                        current_class_name = parent_name.clone();
                    } else {
                        break;
                    }
                } else {
                    // 类查找失败 —— 检查是否是接口类型
                    if let Some(implementor) = registry
                        .find_implementing_class_for_method(&current_class_name, method_name)
                    {
                        if let Some(methods) = implementor.methods.get(method_name) {
                            for method in methods {
                                if !method.is_static {
                                    return true;
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
        // 默认返回false
        false
    }

    /// 检查方法是否是 private 方法
    pub fn is_private_method(&self, class_name: &str, method_name: &str) -> bool {
        let class_name = self.resolved_class_lookup_name(class_name);
        let class_name = class_name.as_str();
        if let Some(ref registry) = self.type_registry {
            let mut current_class_name = class_name.to_string();
            loop {
                if let Some(class_info) = registry.get_class(&current_class_name) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        // 检查是否有任何方法是 private
                        for method in methods {
                            if method.is_private {
                                return true;
                            }
                        }
                        // 在当前类找到方法但不是 private，返回 false
                        return false;
                    }
                    // 在当前类没找到，查找父类
                    if let Some(ref parent_name) = class_info.parent {
                        current_class_name = parent_name.clone();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        // 默认返回 false（不是 private）
        false
    }

    /// 获取类的字段类型
    pub fn get_field_type(&self, class_name: &str, field_name: &str) -> Option<crate::types::Type> {
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(field_info) = class_info.fields.get(field_name) {
                    return Some(field_info.field_type.clone());
                }
            }
        }
        None
    }

    /// 检查类是否有 vtable（用于动态分派）
    pub fn class_has_vtable(&self, class_name: &str) -> bool {
        let class_name = self.resolved_class_lookup_name(class_name);
        let class_name = class_name.as_str();
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                // final 类没有 vtable（不能被继承，不需要动态分派）
                if class_info.is_final {
                    return false;
                }
                // 检查是否有 vtable 布局
                return class_info.vtable_layout.is_some();
            }
        }
        false
    }

    pub fn is_interface_type(&self, class_name: &str) -> bool {
        let base_class_name = if let Some(pos) = class_name.find('<') {
            &class_name[..pos]
        } else {
            class_name
        };

        self.type_registry
            .as_ref()
            .is_some_and(|registry| registry.get_interface(base_class_name).is_some())
    }

    pub fn interface_has_vtable_slot(
        &self,
        interface_name: &str,
        method_name: &str,
        arg_types: &[crate::types::Type],
    ) -> bool {
        self.interface_has_vtable_slot_with_type_args(
            interface_name,
            method_name,
            arg_types,
            &[],
        )
    }

    /// 与 [`interface_has_vtable_slot`] 相同，但允许调用方传入接口的类型实参
    /// 以便区分同一泛型接口的不同实例化（如 `Into<IOError>` 与 `Into<ParseError>`）。
    pub fn interface_has_vtable_slot_with_type_args(
        &self,
        interface_name: &str,
        method_name: &str,
        arg_types: &[crate::types::Type],
        interface_type_args: &[crate::types::Type],
    ) -> bool {
        self.type_registry.as_ref().is_some_and(|registry| {
            registry
                .get_interface_vtable_slot(
                    interface_name,
                    method_name,
                    arg_types,
                    interface_type_args,
                )
                .is_some()
        })
    }

    /// 获取方法在 vtable 中的槽位编号
    /// 使用方法签名（方法名+参数类型）作为键，支持重载方法
    pub fn get_vtable_slot(
        &self,
        class_name: &str,
        method_name: &str,
        arg_types: &[crate::types::Type],
    ) -> usize {
        let class_name = self.resolved_class_lookup_name(class_name);
        let class_name = class_name.as_str();
        // 构建方法签名：方法名(参数类型1,参数类型2,...)
        let param_type_strs: Vec<String> = arg_types.iter().map(|t| format!("{:?}", t)).collect();
        let method_sig = if param_type_strs.is_empty() {
            method_name.to_string()
        } else {
            format!("{}({})", method_name, param_type_strs.join(","))
        };

        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(ref vtable) = class_info.vtable_layout {
                    return vtable.slots.get(&method_sig).copied().unwrap_or(0);
                }
            }
        }
        0
    }

    pub fn get_interface_vtable_slot(
        &self,
        interface_name: &str,
        method_name: &str,
        arg_types: &[crate::types::Type],
    ) -> usize {
        self.get_interface_vtable_slot_with_type_args(
            interface_name,
            method_name,
            arg_types,
            &[],
        )
    }

    /// 与 [`get_interface_vtable_slot`] 相同，但允许调用方传入接口的类型实参
    /// 以便区分同一泛型接口的不同实例化（如 `Into<IOError>` 与 `Into<ParseError>`）。
    pub fn get_interface_vtable_slot_with_type_args(
        &self,
        interface_name: &str,
        method_name: &str,
        arg_types: &[crate::types::Type],
        interface_type_args: &[crate::types::Type],
    ) -> usize {
        self.type_registry
            .as_ref()
            .and_then(|registry| {
                registry.get_interface_vtable_slot(
                    interface_name,
                    method_name,
                    arg_types,
                    interface_type_args,
                )
            })
            .unwrap_or(0)
    }

    /// 构建函数指针类型字符串（用于 vtable 间接调用）
    pub fn build_function_type_string(
        &self,
        ret_type: &crate::types::Type,
        args: &[String],
        class_name: &str,
        method_name: &str,
    ) -> String {
        let llvm_ret = self.type_to_llvm(ret_type);

        // 构建参数类型列表（第一个参数是 i8* this）
        let mut param_types = vec!["i8*".to_string()];

        // 从 TypeRegistry 获取方法参数类型
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    if let Some(method) = methods.first() {
                        for param in &method.params {
                            param_types.push(self.type_to_llvm(&param.param_type));
                        }
                    }
                }
            } else if let Some(interface_info) = registry.get_interface(class_name) {
                if let Some(method) = interface_info.methods.get(method_name) {
                    for param in &method.params {
                        param_types.push(self.type_to_llvm(&param.param_type));
                    }
                }
            }
        }

        // 如果没有从 TypeRegistry 获取到，使用参数推断
        if param_types.len() == 1 {
            for arg in args {
                let (ty, _) = self.parse_typed_value(arg);
                param_types.push(ty);
            }
        }

        format!("{} ({})*", llvm_ret, param_types.join(", "))
    }
}
