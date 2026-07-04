//! 函数调用表达式代码生成 - 方法解析
//!
//! 处理方法重载解析、参数类型匹配和参数类型转换。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, codegen_error_at};

impl IRGenerator {
    /// 获取方法的参数类型列表
    /// 时间复杂度 O(m * k)，其中 m 为重载方法数量，k 为参数数量
    pub fn get_method_param_types(
        &self,
        class_name: &str,
        method_name: &str,
        processed_args: &[String],
        has_varargs_array: bool,
    ) -> Vec<crate::types::Type> {
        if let Some(best) =
            self.resolve_best_method(class_name, method_name, processed_args, has_varargs_array)
        {
            if best.params.iter().any(|p| p.is_varargs) {
                return best
                    .params
                    .iter()
                    .filter(|p| !p.is_varargs)
                    .map(|p| p.param_type.clone())
                    .collect();
            }
            return best.params.iter().map(|p| p.param_type.clone()).collect();
        }
        Vec::new()
    }

    /// 根据调用实参找到最佳匹配的方法重载（支持继承查找）
    /// 时间复杂度 O(m * k)，其中 m 为重载方法数量，k 为参数数量
    pub fn resolve_best_method(
        &self,
        class_name: &str,
        method_name: &str,
        processed_args: &[String],
        has_varargs_array: bool,
    ) -> Option<crate::types::MethodInfo> {
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

        let registry = self.type_registry.as_ref()?;
        // 对于泛型类如 FileResult<File>，需要使用基础类名查找
        let base_class_name = if let Some(lt_pos) = class_name.find('<') {
            &class_name[..lt_pos]
        } else {
            class_name
        };

        if let Some(interface_info) = registry.get_interface(base_class_name) {
            if let Some(method) = interface_info.methods.get(method_name) {
                if method.params.len() == processed_args.len()
                    || method.params.iter().any(|p| p.is_varargs)
                {
                    return Some(method.clone());
                }
            }
        }

        let mut current_class_name = base_class_name.to_string();

        loop {
            let class_info = match registry.get_class(&current_class_name) {
                Some(info) => info,
                None => {
                    // 类查找失败 —— 检查是否是接口类型
                    if let Some(implementor) = registry
                        .find_implementing_class_for_method(&current_class_name, method_name)
                    {
                        current_class_name = implementor.name.clone();
                        continue;
                    }
                    return None;
                }
            };
            let methods = match class_info.methods.get(method_name) {
                Some(m) => m,
                None => {
                    // 当前类没有该方法 —— 检查接口
                    if let Some(implementor) = registry
                        .find_implementing_class_for_method(&current_class_name, method_name)
                    {
                        current_class_name = implementor.name.clone();
                        continue;
                    }
                    return None;
                }
            };
            let arg_count = processed_args.len();
            let llvm_current = self.get_qualified_class_name(&current_class_name);

            // 第一遍：精确类型签名匹配
            for method in methods.iter() {
                let param_count = method.params.len();
                let is_varargs = method.params.iter().any(|p| p.is_varargs);
                let fixed_count = method
                    .params
                    .iter()
                    .position(|p| p.is_varargs)
                    .unwrap_or(param_count);

                if is_varargs && arg_count >= fixed_count {
                    let method_sig = self.build_function_name_from_method(
                        &current_class_name,
                        method_name,
                        &method.params,
                        has_varargs_array,
                    );
                    let expected_sig =
                        format!("{}.__{}_{}", llvm_current, method_name, arg_types.join("_"));
                    if method_sig == expected_sig {
                        return Some(method.clone());
                    }
                } else if param_count == arg_count {
                    let method_sig = self.build_function_name_from_method(
                        &current_class_name,
                        method_name,
                        &method.params,
                        has_varargs_array,
                    );
                    let expected_sig =
                        format!("{}.__{}_{}", llvm_current, method_name, arg_types.join("_"));
                    if method_sig == expected_sig {
                        return Some(method.clone());
                    }
                }
            }

            // 第二遍：回退到参数数量匹配
            for method in methods.iter() {
                let param_count = method.params.len();
                let is_varargs = method.params.iter().any(|p| p.is_varargs);
                let fixed_count = method
                    .params
                    .iter()
                    .position(|p| p.is_varargs)
                    .unwrap_or(param_count);

                if is_varargs && arg_count >= fixed_count {
                    return Some(method.clone());
                } else if param_count == arg_count {
                    return Some(method.clone());
                }
            }

            // 在父类中查找
            if let Some(ref parent_name) = class_info.parent {
                current_class_name = parent_name.clone();
                continue;
            }
            break;
        }
        None
    }

    /// 转换参数类型以匹配形参类型
    pub fn convert_arg_type(
        &mut self,
        arg_type: &str,
        arg_val: &str,
        param_llvm_type: &str,
    ) -> String {
        // 如果类型已经匹配，直接返回
        if arg_type == param_llvm_type {
            return format!("{} {}", arg_type, arg_val);
        }

        // double -> float 转换
        if arg_type == "double" && param_llvm_type == "float" {
            let converted = self.new_temp();
            self.emit_line(&format!(
                "  {} = fptrunc double {} to float",
                converted, arg_val
            ));
            return format!("float {}", converted);
        }

        // float -> double 转换
        if arg_type == "float" && param_llvm_type == "double" {
            let converted = self.new_temp();
            self.emit_line(&format!(
                "  {} = fpext float {} to double",
                converted, arg_val
            ));
            return format!("double {}", converted);
        }

        // i32 -> i64 转换
        if arg_type == "i32" && param_llvm_type == "i64" {
            let converted = self.new_temp();
            self.emit_line(&format!("  {} = sext i32 {} to i64", converted, arg_val));
            return format!("i64 {}", converted);
        }

        // i64 -> i32 截断
        if arg_type == "i64" && param_llvm_type == "i32" {
            let converted = self.new_temp();
            self.emit_line(&format!("  {} = trunc i64 {} to i32", converted, arg_val));
            return format!("i32 {}", converted);
        }

        // 指针 -> i64 转换 (ptrtoint)
        if arg_type.ends_with("*") && param_llvm_type == "i64" {
            let converted = self.new_temp();
            self.emit_line(&format!(
                "  {} = ptrtoint {} {} to i64",
                converted, arg_type, arg_val
            ));
            return format!("i64 {}", converted);
        }

        // i64 -> 指针 转换 (inttoptr)
        if arg_type == "i64" && param_llvm_type.ends_with("*") {
            let converted = self.new_temp();
            self.emit_line(&format!(
                "  {} = inttoptr i64 {} to {}",
                converted, arg_val, param_llvm_type
            ));
            return format!("{} {}", param_llvm_type, converted);
        }

        // 值类型 -> i8* 装箱转换（用于泛型类型存储）
        // 对于泛型类如 Box<T>，需要将具体值类型装箱为 i8* 存储
        if param_llvm_type == "i8*" {
            // i1 (bool) -> i8*：先扩展到 i8，再扩展到 i64，最后转指针
            if arg_type == "i1" {
                let ext_to_i8 = self.new_temp();
                self.emit_line(&format!("  {} = zext i1 {} to i8", ext_to_i8, arg_val));
                let ext_to_i64 = self.new_temp();
                self.emit_line(&format!("  {} = zext i8 {} to i64", ext_to_i64, ext_to_i8));
                let ptr = self.new_temp();
                self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", ptr, ext_to_i64));
                return format!("i8* {}", ptr);
            }

            // i8 (char) -> i8*：扩展到 i64，再转指针
            if arg_type == "i8" {
                let ext_to_i64 = self.new_temp();
                self.emit_line(&format!("  {} = sext i8 {} to i64", ext_to_i64, arg_val));
                let ptr = self.new_temp();
                self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", ptr, ext_to_i64));
                return format!("i8* {}", ptr);
            }

            // i16 -> i8*：扩展到 i64，再转指针
            if arg_type == "i16" {
                let ext_to_i64 = self.new_temp();
                self.emit_line(&format!("  {} = sext i16 {} to i64", ext_to_i64, arg_val));
                let ptr = self.new_temp();
                self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", ptr, ext_to_i64));
                return format!("i8* {}", ptr);
            }

            // i32 -> i8*：扩展到 i64，再转指针
            if arg_type == "i32" {
                let ext_to_i64 = self.new_temp();
                self.emit_line(&format!("  {} = sext i32 {} to i64", ext_to_i64, arg_val));
                let ptr = self.new_temp();
                self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", ptr, ext_to_i64));
                return format!("i8* {}", ptr);
            }

            // float -> i8*：先扩展到 double，再 bitcast 到 i64，最后转指针
            if arg_type == "float" {
                let ext_to_double = self.new_temp();
                self.emit_line(&format!(
                    "  {} = fpext float {} to double",
                    ext_to_double, arg_val
                ));
                let bits = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast double {} to i64",
                    bits, ext_to_double
                ));
                let ptr = self.new_temp();
                self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", ptr, bits));
                return format!("i8* {}", ptr);
            }

            // double -> i8*：bitcast 到 i64，再转指针
            if arg_type == "double" {
                let bits = self.new_temp();
                self.emit_line(&format!("  {} = bitcast double {} to i64", bits, arg_val));
                let ptr = self.new_temp();
                self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", ptr, bits));
                return format!("i8* {}", ptr);
            }
        }

        // 默认：不进行转换
        format!("{} {}", arg_type, arg_val)
    }

    /// 对 C ABI 可变参数应用默认实参提升。
    ///
    /// C 的 `...` 参数没有声明类型，调用方必须先把 float 提升为 double，
    /// 并把比 int 窄的整数类型提升为 int，否则 printf/scanf 等函数会按错宽度取参。
    pub fn promote_c_vararg_arg(
        &mut self,
        arg_type: &str,
        arg_val: &str,
        cay_type: Option<&crate::types::Type>,
    ) -> String {
        if arg_type == "float" {
            let promoted = self.new_temp();
            self.emit_line(&format!(
                "  {} = fpext float {} to double",
                promoted, arg_val
            ));
            return format!("double {}", promoted);
        }

        if arg_type == "i1" {
            let promoted = self.new_temp();
            self.emit_line(&format!("  {} = zext i1 {} to i32", promoted, arg_val));
            return format!("i32 {}", promoted);
        }

        if matches!(arg_type, "i8" | "i16") {
            let promoted = self.new_temp();
            let extension = if matches!(
                cay_type,
                Some(
                    crate::types::Type::CUChar
                        | crate::types::Type::CUShort
                        | crate::types::Type::CBool
                )
            ) {
                "zext"
            } else {
                "sext"
            };
            self.emit_line(&format!(
                "  {} = {} {} {} to i32",
                promoted, extension, arg_type, arg_val
            ));
            return format!("i32 {}", promoted);
        }

        format!("{} {}", arg_type, arg_val)
    }
}
