//! 函数调用表达式代码生成 - 函数名生成
//!
//! 处理函数名生成、方法签名构建等。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, codegen_error_at};

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
            if let Some(name) =
                self.specialized_generic_method_name(class_name, method_name, processed_args.len())
            {
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
            loop {
                if let Some(class_info) = registry.get_class(&lookup_class_name) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        let arg_count = processed_args.len();

                        // 首先尝试找到参数类型完全匹配的方法（用旧格式签名字符串比较，
                        // 与实参 arg_types 语义一致；匹配后再用 Itanium ABI 生成最终函数名）
                        for method in methods {
                            let param_count = method.params.len();
                            let is_varargs = method.params.iter().any(|p| p.is_varargs);
                            let fixed_count = method
                                .params
                                .iter()
                                .position(|p| p.is_varargs)
                                .unwrap_or(param_count);

                            let sig_matches = if is_varargs {
                                arg_count >= fixed_count
                                    && self.method_param_signatures_match(
                                        &method.params,
                                        &arg_types,
                                        has_varargs_array,
                                    )
                            } else {
                                param_count == arg_count
                                    && self.method_param_signatures_match(
                                        &method.params,
                                        &arg_types,
                                        has_varargs_array,
                                    )
                            };

                            if sig_matches {
                                // 使用实际定义方法的类名生成函数名（支持继承和接口）
                                return self.build_function_name_from_method(
                                    &lookup_class_name,
                                    method_name,
                                    &method.params,
                                    has_varargs_array,
                                );
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

        // 回退：类型注册表中找不到方法定义时，直接用实参类型按 Itanium ABI 生成函数名
        // 顶层函数（class_name 为空）使用 __toplevel_ 前缀（非 C++ 互操作场景）
        if class_name.is_empty() {
            // 顶层函数命名：__toplevel_func_name
            format!("__toplevel_{}", method_name)
        } else {
            let param_types: Vec<crate::types::Type> = processed_args
                .iter()
                .map(|r| {
                    let (ty, _) = self.parse_typed_value(r);
                    self.llvm_type_to_cay_type(&ty).unwrap_or(crate::types::Type::Int32)
                })
                .collect();
            self.mangle_itanium_method(class_name, method_name, &param_types, false, false)
        }
    }

    /// 检查方法形参列表的旧格式签名是否与调用实参签名匹配（用于重载解析）。
    /// 仅做匹配判断，不涉及最终函数名格式。
    pub(crate) fn method_param_signatures_match(
        &self,
        params: &[crate::types::ParameterInfo],
        arg_types: &[String],
        has_varargs_array: bool,
    ) -> bool {
        if params.len() != arg_types.len() {
            return false;
        }
        params
            .iter()
            .zip(arg_types.iter())
            .all(|(p, arg_sig)| {
                let is_param_varargs = has_varargs_array && p.is_varargs;
                let resolved = self.resolve_type(&p.param_type);
                let param_sig = self.param_type_to_signature(&resolved, is_param_varargs);
                if param_sig == *arg_sig {
                    return true;
                }

                // 接口对象、普通对象和函数值在当前 LLVM ABI 中均擦除为 i8*，
                // parse_typed_value 因而只能得到字符串指针签名 "s"。允许它与这些
                // 指针语义形参匹配，避免重载解析退化为仅按参数数量选择。
                arg_sig == "s"
                    && matches!(
                        resolved,
                        crate::types::Type::Object(_)
                            | crate::types::Type::Generic(_, _)
                            | crate::types::Type::Function(_)
                    )
            })
    }

    /// 为特化泛型类的方法调用生成与定义完全一致的函数名。
    ///
    /// 特化方法的定义（见 `generate_method_name`）命名为「特化类前缀 +
    /// 已替换类型实参的方法签名」。调用点必须产生完全相同的名字，才能链接到
    /// 已生成的单态化版本，而非类型擦除的基础模板。
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

        if let Some(class_info) = registry.get_class(base) {
            if class_info.type_params.is_empty() {
                return None;
            }
            let methods = class_info.methods.get(method_name)?;
            // 按参数数量选择重载（可变参数方法或数量匹配者）
            let method_info = methods
                .iter()
                .find(|m| m.params.len() == arg_count || m.params.iter().any(|p| p.is_varargs))
                .or_else(|| methods.first())?;
            // 将注册表方法签名中的泛型参数按类名中的类型实参替换，避免生成
            // `PPc` 等降级签名，确保调用名与单态化定义完全一致。
            let mapping = self.build_specialization_mapping(class_name, class_info);
            let param_types: Vec<crate::types::Type> = method_info
                .params
                .iter()
                .map(|p| crate::types::substitute_type_params(&p.param_type, &mapping))
                .collect();
            return Some(self.mangle_itanium_method(class_name, method_name, &param_types, false, false));
        }

        // 泛型 struct 的特化方法调用：与泛型类同理，但 struct 无继承、无 vtable，
        // 直接在 StructInfo 上按类型实参替换方法签名。
        if let Some(struct_info) = registry.get_struct(base) {
            if struct_info.type_params.is_empty() {
                return None;
            }
            let methods = struct_info.methods.get(method_name)?;
            let method_info = methods
                .iter()
                .find(|m| m.params.len() == arg_count || m.params.iter().any(|p| p.is_varargs))
                .or_else(|| methods.first())?;
            let (_, type_arg_strs) = Self::parse_generic_args_from_name(class_name);
            let mut parsed_type_args: Vec<crate::types::Type> = type_arg_strs
                .iter()
                .map(|s| crate::codegen::specialization::parse_type_str(s))
                .collect();
            // 用默认值/占位符填充缺失的类型参数
            for (idx, param) in struct_info.type_params.iter().enumerate() {
                if parsed_type_args.get(idx).is_none() {
                    parsed_type_args.push(
                        param
                            .default_type
                            .clone()
                            .unwrap_or(crate::types::Type::GenericParam(param.name.clone())),
                    );
                }
            }
            let mapping: std::collections::HashMap<String, crate::types::Type> = struct_info
                .type_params
                .iter()
                .zip(parsed_type_args.iter())
                .map(|(p, t)| (p.name.clone(), t.clone()))
                .collect();
            let param_types: Vec<crate::types::Type> = method_info
                .params
                .iter()
                .map(|p| crate::types::substitute_type_params(&p.param_type, &mapping))
                .collect();
            return Some(self.mangle_itanium_method(class_name, method_name, &param_types, false, false));
        }

        None
    }

    /// 根据方法定义的参数类型构建函数名（Itanium ABI 格式，与 C++ 互操作）
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
        if params.is_empty() {
            return self.mangle_itanium_method(class_name, method_name, &[], false, false);
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

        // 使用方法定义中的完整参数类型生成 Itanium 名称。
        // 可变参数在方法定义中仍以 Array(Element) 形式存在，调用点传递数组指针
        // 后函数签名与定义一致，因此不过滤 varargs 参数。
        let param_types: Vec<crate::types::Type> = params
            .iter()
            .map(|p| {
                let resolved_type = self.resolve_type(&p.param_type);

                // 检查参数类型是否是泛型参数
                // 如果是，检查是否有特化映射；否则保留泛型参数（降级为 char* 编码）
                match &resolved_type {
                    crate::types::Type::Object(name)
                        if class_type_params.iter().any(|p| &p.name == name) =>
                    {
                        self.generic_type_args
                            .get(name)
                            .cloned()
                            .unwrap_or(resolved_type)
                    }
                    crate::types::Type::GenericParam(name) => self
                        .generic_type_args
                        .get(name)
                        .cloned()
                        .unwrap_or(resolved_type),
                    _ => resolved_type,
                }
            })
            .collect();

        self.mangle_itanium_method(class_name, method_name, &param_types, false, false)
    }

    /// 将参数类型转换为签名
    pub fn param_type_to_signature(
        &self,
        ty: &crate::types::Type,
        is_varargs_array: bool,
    ) -> String {
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
