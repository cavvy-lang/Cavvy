//! 函数调用表达式代码生成 - 函数名生成
//!
//! 处理函数名生成、方法签名构建等。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{cayResult, codegen_error_at};

impl IRGenerator {
    /// 生成函数名 - 优先使用类型注册表中方法定义的参数类型，支持继承
    pub fn generate_function_name(
        &self,
        class_name: &str,
        method_name: &str,
        processed_args: &[String],
        has_varargs_array: bool,
    ) -> String {
        let llvm_class = self.get_qualified_class_name(class_name);
        // 特殊处理运行时 native 方法：直接返回运行时函数名
        if method_name == "__cay_buffer_to_string" {
            return "__cay_buffer_to_string".to_string();
        }

        // 特化泛型类的方法调用：调用名必须与特化定义（generate_method_name）
        // 完全一致——即「特化类前缀 + 注册表方法签名」，其中未解析的泛型参数保留
        // 为 gT。否则会解析到类型擦除的基础模板 Optional_T_ 而链接失败。
        if class_name.contains('<') {
            if let Some(name) = self.specialized_generic_method_name(
                class_name,
                method_name,
                processed_args.len(),
            ) {
                return name;
            }
        }

        // 获取实际参数的类型签名
        // 找到可变参数在形参列表中的位置（用于正确标记数组参数）
        let varargs_param_index = self.get_varargs_index(class_name, method_name);
        let arg_types: Vec<String> = processed_args
            .iter()
            .enumerate()
            .map(|(idx, r)| {
                let (ty, _) = self.parse_typed_value(r);
                let is_varargs_array = has_varargs_array && Some(idx) == varargs_param_index;
                let llvm_type = self.llvm_type_to_signature(&ty);
                if is_varargs_array {
                    "ai".to_string()
                } else {
                    llvm_type
                }
            })
            .collect();

        // 尝试从类型注册表获取方法信息（支持继承查找）
        if let Some(ref registry) = self.type_registry {
            // 首先在当前类中查找方法
            // 对于泛型类如 FileResult<File>，需要使用基础类名 FileResult 查找
            let base_class_name = if let Some(lt_pos) = class_name.find('<') {
                &class_name[..lt_pos]
            } else {
                class_name
            };
            let mut lookup_class_name = base_class_name.to_string();
            let llvm_current = self.get_qualified_class_name(class_name);
            loop {
                if let Some(class_info) = registry.get_class(&lookup_class_name) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        let arg_count = processed_args.len();

                        // 首先尝试找到参数类型完全匹配的方法
                        for method in methods {
                            let param_count = method.params.len();
                            let is_varargs = method.params.iter().any(|p| p.is_varargs);
                            let fixed_count = method
                                .params
                                .iter()
                                .position(|p| p.is_varargs)
                                .unwrap_or(param_count);

                            if is_varargs {
                                // 可变参数方法
                                if arg_count >= fixed_count {
                                    // 使用实际定义方法的类名生成函数名（支持继承和接口）
                                    let method_sig = self.build_function_name_from_method(
                                        &lookup_class_name,
                                        method_name,
                                        &method.params,
                                        has_varargs_array,
                                    );
                                    let expected_sig = format!(
                                        "{}.__{}_{}",
                                        llvm_current,
                                        method_name,
                                        arg_types.join("_")
                                    );
                                    if method_sig == expected_sig {
                                        return method_sig;
                                    }
                                }
                            } else if param_count == arg_count {
                                // 非可变参数方法：检查参数类型是否匹配
                                // 使用实际定义方法的类名生成函数名（支持继承和接口）
                                let method_sig = self.build_function_name_from_method(
                                    &lookup_class_name,
                                    method_name,
                                    &method.params,
                                    has_varargs_array,
                                );
                                let expected_sig = format!(
                                    "{}.__{}_{}",
                                    llvm_current,
                                    method_name,
                                    arg_types.join("_")
                                );
                                if method_sig == expected_sig {
                                    return method_sig;
                                }
                            }
                        }

                        // 如果没有找到类型完全匹配的方法，回退到参数数量匹配
                        for method in methods {
                            let param_count = method.params.len();
                            let is_varargs = method.params.iter().any(|p| p.is_varargs);
                            let fixed_count = method
                                .params
                                .iter()
                                .position(|p| p.is_varargs)
                                .unwrap_or(param_count);

                            if is_varargs {
                                if arg_count >= fixed_count {
                                    // 使用实际定义方法的类名生成函数名（支持继承）
                                    return self.build_function_name_from_method(
                                        &lookup_class_name,
                                        method_name,
                                        &method.params,
                                        has_varargs_array,
                                    );
                                }
                            } else if param_count == arg_count {
                                // 使用实际定义方法的类名生成函数名（支持继承）
                                return self.build_function_name_from_method(
                                    &lookup_class_name,
                                    method_name,
                                    &method.params,
                                    has_varargs_array,
                                );
                            }
                        }
                    }

                    // 如果在当前类中没找到，尝试在父类中查找
                    if let Some(ref parent_name) = class_info.parent {
                        lookup_class_name = parent_name.clone();
                        continue;
                    }
                }
                // 类查找失败 —— 检查是否是接口类型，若是则查找实现类
                if let Some(implementor) =
                    registry.find_implementing_class_for_method(&lookup_class_name, method_name)
                {
                    lookup_class_name = implementor.name.clone();
                    continue;
                }
                break;
            }
        }

        // 回退到使用实际参数类型生成函数名
        // 顶层函数（class_name 为空）使用 __toplevel_ 前缀
        if class_name.is_empty() {
            // 顶层函数命名：__toplevel_func_name
            format!("__toplevel_{}", method_name)
        } else if arg_types.is_empty() {
            format!("{}.{}", llvm_class, method_name)
        } else {
            format!("{}.__{}_{}", llvm_class, method_name, arg_types.join("_"))
        }
    }

    /// 为特化泛型类的方法调用生成与定义完全一致的函数名。
    ///
    /// 特化方法的定义（见 `generate_method_name`）命名为「特化类前缀 +
    /// 注册表方法签名」，签名中的泛型参数按 `type_to_signature` 保留为 `gT`
    /// （不经 `generic_type_args` 解析）。调用点必须产生完全相同的名字，
    /// 才能链接到已生成的单态化版本，而非类型擦除的基础模板。
    ///
    /// 仅当 `class_name` 是泛型类的特化名（其基础类含类型参数）时返回 `Some`。
    fn specialized_generic_method_name(
        &self,
        class_name: &str,
        method_name: &str,
        arg_count: usize,
    ) -> Option<String> {
        let registry = self.type_registry.as_ref()?;
        let base = class_name.split('<').next().unwrap_or(class_name);
        let class_info = registry.get_class(base)?;
        if class_info.type_params.is_empty() {
            return None;
        }
        let methods = class_info.methods.get(method_name)?;
        // 按参数数量选择重载（可变参数方法或数量匹配者）
        let method_info = methods
            .iter()
            .find(|m| m.params.len() == arg_count || m.params.iter().any(|p| p.is_varargs))
            .or_else(|| methods.first())?;
        let cls = self.get_qualified_class_name(class_name);
        if method_info.params.is_empty() {
            Some(format!("{}.{}", cls, method_name))
        } else {
            let sigs: Vec<String> = method_info
                .params
                .iter()
                .map(|p| {
                    if p.is_varargs {
                        self.varargs_type_to_signature(&p.param_type)
                    } else {
                        self.type_to_signature(&p.param_type)
                    }
                })
                .collect();
            Some(format!("{}.__{}_{}", cls, method_name, sigs.join("_")))
        }
    }

    /// 根据方法定义的参数类型构建函数名
    ///
    /// # Arguments
    /// * `class_name` - 类名
    /// * `method_name` - 方法名
    /// * `params` - 参数信息列表
    /// * `has_varargs_array` - 是否有可变参数数组
    pub fn build_function_name_from_method(
        &self,
        class_name: &str,
        method_name: &str,
        params: &[crate::types::ParameterInfo],
        has_varargs_array: bool,
    ) -> String {
        let llvm_cls = self.get_qualified_class_name(class_name);
        if params.is_empty() {
            return format!("{}.{}", llvm_cls, method_name);
        }

        // 获取基础类名（去除泛型参数）以查找类信息
        let base_class_name = if let Some(pos) = class_name.find('<') {
            &class_name[..pos]
        } else {
            class_name
        };

        // 获取类的泛型参数列表
        let class_type_params = if let Some(ref registry) = self.type_registry {
            registry
                .get_class(base_class_name)
                .map(|c| c.type_params.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let param_types: Vec<String> = params
            .iter()
            .map(|p| {
                let is_param_varargs = has_varargs_array && p.is_varargs;
                let resolved_type = self.resolve_type(&p.param_type);

                // 检查参数类型是否是泛型参数
                // 如果是，使用泛型参数名（如 T）而不是具体类型
                match &resolved_type {
                    crate::types::Type::Object(name) => {
                        if class_type_params.iter().any(|p| &p.name == name) {
                            // 这是泛型参数 - 检查是否有特化映射
                            if let Some(actual_type) = self.generic_type_args.get(name) {
                                self.param_type_to_signature(actual_type, is_param_varargs)
                            } else {
                                format!("g{}", name)
                            }
                        } else {
                            self.param_type_to_signature(&resolved_type, is_param_varargs)
                        }
                    }
                    crate::types::Type::GenericParam(name) => {
                        // 泛型参数类型 - 检查是否有特化映射
                        if let Some(actual_type) = self.generic_type_args.get(name) {
                            // 有特化映射，使用实际类型
                            self.param_type_to_signature(actual_type, is_param_varargs)
                        } else {
                            // 无特化映射，使用泛型参数名
                            format!("g{}", name)
                        }
                    }
                    _ => self.param_type_to_signature(&resolved_type, is_param_varargs),
                }
            })
            .collect();

        format!("{}.__{}_{}", llvm_cls, method_name, param_types.join("_"))
    }

    /// 将参数类型转换为签名
    pub fn param_type_to_signature(&self, ty: &crate::types::Type, is_varargs_array: bool) -> String {
        if is_varargs_array {
            // 可变参数数组：提取元素类型并生成签名
            return self.varargs_element_type_to_signature(ty);
        }

        match ty {
            crate::types::Type::Void => "v".to_string(),
            crate::types::Type::Int32 => "i".to_string(),
            crate::types::Type::Int64 => "l".to_string(),
            crate::types::Type::Float32 => "f".to_string(),
            crate::types::Type::Float64 => "d".to_string(),
            crate::types::Type::Bool => "b".to_string(),
            crate::types::Type::String => "s".to_string(),
            crate::types::Type::Char => "c".to_string(),
            crate::types::Type::Object(name) => format!("o{}", name),
            crate::types::Type::GenericParam(name) => format!("g{}", name),
            crate::types::Type::Array(inner) => {
                format!("a{}", self.param_type_to_signature(inner, false))
            }
            // FFI 类型
            crate::types::Type::CInt => "ci".to_string(),
            crate::types::Type::CUInt => "cu".to_string(),
            crate::types::Type::CLong => "cl".to_string(),
            crate::types::Type::CULong => "cul".to_string(),
            crate::types::Type::CShort => "cs".to_string(),
            crate::types::Type::CUShort => "cus".to_string(),
            crate::types::Type::CChar => "cc".to_string(),
            crate::types::Type::CUChar => "cuc".to_string(),
            crate::types::Type::CFloat => "cf".to_string(),
            crate::types::Type::CDouble => "cd".to_string(),
            crate::types::Type::SizeT => "sz".to_string(),
            crate::types::Type::SSizeT => "ssz".to_string(),
            crate::types::Type::UIntPtr => "uptr".to_string(),
            crate::types::Type::IntPtr => "iptr".to_string(),
            crate::types::Type::CVoid => "cv".to_string(),
            crate::types::Type::CBool => "cb".to_string(),
            crate::types::Type::Pointer(inner) => {
                format!("p{}", self.param_type_to_signature(inner, false))
            }
            // 函数指针类型
            crate::types::Type::Function(func_type) => {
                // 生成函数指针签名: fn_<return>_<param1>_<param2>_...
                let mut sig = "fn".to_string();
                sig.push_str(&self.param_type_to_signature(&func_type.return_type, false));
                for param in &func_type.params {
                    sig.push_str("_");
                    sig.push_str(&self.param_type_to_signature(param, false));
                }
                sig
            }
            _ => "x".to_string(),
        }
    }

    /// 将可变参数数组的元素类型转换为签名
    /// 可变参数类型是 Array(ElementType)，需要提取元素类型
    pub fn varargs_element_type_to_signature(&self, ty: &crate::types::Type) -> String {
        use crate::types::Type;
        match ty {
            Type::Array(elem) => match elem.as_ref() {
                Type::Int32 => "ai".to_string(),
                Type::Int64 => "al".to_string(),
                Type::Float32 => "af".to_string(),
                Type::Float64 => "ad".to_string(),
                Type::Bool => "ab".to_string(),
                Type::String => "as".to_string(),
                Type::Char => "ac".to_string(),
                Type::Object(name) => format!("ao{}", name),
                _ => "ax".to_string(),
            },
            _ => self.param_type_to_signature(ty, false), // 如果不是数组类型，回退到普通签名
        }
    }
}
