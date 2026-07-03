//! 函数调用表达式代码生成 - 可变参数处理
//!
//! 处理可变参数的打包、数组创建和元素存储。

use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;

impl IRGenerator {
    /// 将可变参数打包成数组（支持非末尾可变参数）
    pub fn pack_varargs_args(
        &mut self,
        class_name: &str,
        method_name: &str,
        arg_results: &[String],
    ) -> CayResult<Vec<String>> {
        // 从类型注册表获取可变参数位置和元素类型
        let (varargs_index, varargs_elem_type) = self.get_varargs_info(class_name, method_name);
        let fixed_param_count = varargs_index;

        // 获取总形参个数以确定可变参数之后还有多少参数
        let total_param_count = self.get_method_param_count(class_name, method_name);
        let after_varargs_count = total_param_count.saturating_sub(varargs_index + 1);

        // 可变参数的实参个数 = 总实参 - 固定参数 - 可变参数之后的参数
        let varargs_min_count = if after_varargs_count > 0 {
            after_varargs_count
        } else {
            0
        };
        if arg_results.len() <= fixed_param_count + varargs_min_count {
            // 参数数量不足，不需要打包
            return Ok(arg_results.to_vec());
        }

        // 分割：固定参数 | 可变参数 | 之后参数
        let fixed_args = &arg_results[..fixed_param_count];
        let varargs_end = arg_results.len() - varargs_min_count;
        let varargs = &arg_results[fixed_param_count..varargs_end];
        let after_args = &arg_results[varargs_end..];

        // 检查是否只有一个参数且是数组类型（直接传递数组给可变参数）
        if varargs.len() == 1 {
            let (arg_type, arg_val) = self.parse_typed_value(&varargs[0]);
            // 检查参数类型是否是数组指针（以*结尾但不是i8*）
            if arg_type.ends_with("*") && arg_type != "i8*" {
                // 直接将数组指针作为可变参数传递
                let mut result = fixed_args.to_vec();
                result.push(format!("i8* {}", arg_val));
                return Ok(result);
            }
        }

        // 创建数组来存储可变参数
        let array_size = varargs.len();
        let raw_ptr = self.new_temp();
        let array_ptr = self.new_temp();

        // 根据元素类型确定 LLVM 类型和大小
        let (llvm_elem_type, elem_size) = match varargs_elem_type {
            crate::types::Type::Int32 => ("i32", 4),
            crate::types::Type::Int64 => ("i64", 8),
            crate::types::Type::Float32 => ("float", 4),
            crate::types::Type::Float64 => ("double", 8),
            crate::types::Type::String => ("i8", 8), // String 是指针类型
            crate::types::Type::Char => ("i8", 1),
            crate::types::Type::Bool => ("i8", 1),
            _ => ("i32", 4), // 默认使用 i32
        };

        // 分配数组内存：8字节（长度+padding）+ 元素数据
        let header_size = 8;
        let data_size = array_size * elem_size;
        let total_size = header_size + data_size;
        self.emit_line(&format!(
            "  {} = call i8* @calloc(i64 1, i64 {})",
            raw_ptr, total_size
        ));

        // 存储长度信息到前4字节
        let len_ptr_i8 = self.new_temp();
        let len_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 0",
            len_ptr_i8, raw_ptr
        ));
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i32*",
            len_ptr, len_ptr_i8
        ));
        self.emit_line(&format!(
            "  store i32 {}, i32* {}, align 4",
            array_size, len_ptr
        ));

        // 计算数组元素起始地址（跳过8字节头部）
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            array_ptr, raw_ptr, header_size
        ));

        // 将可变参数存入数组
        for (i, arg_str) in varargs.iter().enumerate() {
            let (arg_type, arg_val) = self.parse_typed_value(arg_str);
            let elem_ptr_i8 = self.new_temp();
            let offset = i * elem_size;

            // 计算元素地址 (i8*)
            self.emit_line(&format!(
                "  {} = getelementptr i8, i8* {}, i64 {}",
                elem_ptr_i8, array_ptr, offset
            ));

            // 根据元素类型进行存储
            self.store_vararg_element(&elem_ptr_i8, &arg_type, &arg_val, llvm_elem_type);
        }

        // 构建结果：固定参数 + 数组指针 + 之后参数
        let mut result = fixed_args.to_vec();
        result.push(format!("i8* {}", array_ptr));
        result.extend(after_args.iter().cloned());

        Ok(result)
    }

    /// 获取可变参数方法的固定参数数量和元素类型
    /// 返回 (varargs_param_index, element_type)，如果未找到可变参数则返回 (0, Int32)
    pub fn get_varargs_info(&self, class_name: &str, method_name: &str) -> (usize, crate::types::Type) {
        if let Some(ref registry) = self.type_registry {
            if let Some(interface_info) = registry.get_interface(class_name) {
                if let Some(method) = interface_info.methods.get(method_name) {
                    for (i, param) in method.params.iter().enumerate() {
                        if param.is_varargs {
                            let elem_type = match &param.param_type {
                                crate::types::Type::Array(elem) => elem.as_ref().clone(),
                                _ => param.param_type.clone(),
                            };
                            return (i, elem_type);
                        }
                    }
                }
            }

            // 先尝试直接查找类
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    for method in methods {
                        for (i, param) in method.params.iter().enumerate() {
                            if param.is_varargs {
                                let fixed_count = i;
                                let elem_type = match &param.param_type {
                                    crate::types::Type::Array(elem) => elem.as_ref().clone(),
                                    _ => param.param_type.clone(),
                                };
                                return (fixed_count, elem_type);
                            }
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
                        for (i, param) in method.params.iter().enumerate() {
                            if param.is_varargs {
                                let fixed_count = i;
                                let elem_type = match &param.param_type {
                                    crate::types::Type::Array(elem) => elem.as_ref().clone(),
                                    _ => param.param_type.clone(),
                                };
                                return (fixed_count, elem_type);
                            }
                        }
                    }
                }
            }
        }
        (0, crate::types::Type::Int32)
    }

    /// 存储可变参数元素到数组
    pub fn store_vararg_element(
        &mut self,
        elem_ptr_i8: &str,
        arg_type: &str,
        arg_val: &str,
        llvm_elem_type: &str,
    ) {
        match llvm_elem_type {
            "i32" => {
                let elem_ptr = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i8* {} to i32*",
                    elem_ptr, elem_ptr_i8
                ));
                if arg_type == "i64" {
                    let truncated = self.new_temp();
                    self.emit_line(&format!("  {} = trunc i64 {} to i32", truncated, arg_val));
                    self.emit_line(&format!(
                        "  store i32 {}, i32* {}, align 4",
                        truncated, elem_ptr
                    ));
                } else if arg_type == "i32" {
                    self.emit_line(&format!(
                        "  store i32 {}, i32* {}, align 4",
                        arg_val, elem_ptr
                    ));
                }
            }
            "i64" => {
                let elem_ptr = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i8* {} to i64*",
                    elem_ptr, elem_ptr_i8
                ));
                if arg_type == "i32" {
                    let extended = self.new_temp();
                    self.emit_line(&format!("  {} = sext i32 {} to i64", extended, arg_val));
                    self.emit_line(&format!(
                        "  store i64 {}, i64* {}, align 8",
                        extended, elem_ptr
                    ));
                } else {
                    self.emit_line(&format!(
                        "  store i64 {}, i64* {}, align 8",
                        arg_val, elem_ptr
                    ));
                }
            }
            "float" => {
                let elem_ptr = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i8* {} to float*",
                    elem_ptr, elem_ptr_i8
                ));
                // 如果参数是 double 类型，需要转换为 float
                if arg_type == "double" {
                    let converted = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = fptrunc double {} to float",
                        converted, arg_val
                    ));
                    self.emit_line(&format!(
                        "  store float {}, float* {}, align 4",
                        converted, elem_ptr
                    ));
                } else {
                    self.emit_line(&format!(
                        "  store float {}, float* {}, align 4",
                        arg_val, elem_ptr
                    ));
                }
            }
            "double" => {
                let elem_ptr = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i8* {} to double*",
                    elem_ptr, elem_ptr_i8
                ));
                self.emit_line(&format!(
                    "  store double {}, double* {}, align 8",
                    arg_val, elem_ptr
                ));
            }
            "i8" => {
                // 用于 String (i8*), char, bool
                let elem_ptr = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i8* {} to i8**",
                    elem_ptr, elem_ptr_i8
                ));
                self.emit_line(&format!(
                    "  store i8* {}, i8** {}, align 8",
                    arg_val, elem_ptr
                ));
            }
            _ => {
                // 默认处理为 i32
                let elem_ptr = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i8* {} to i32*",
                    elem_ptr, elem_ptr_i8
                ));
                self.emit_line(&format!(
                    "  store i32 {}, i32* {}, align 4",
                    arg_val, elem_ptr
                ));
            }
        }
    }
}
