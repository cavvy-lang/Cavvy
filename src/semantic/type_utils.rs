//! 类型工具函数

use crate::ast::Expr;
use crate::types::{Type, ParameterInfo};
use crate::error::cayResult;
use super::analyzer::SemanticAnalyzer;

impl SemanticAnalyzer {
    /// 检查类型兼容性
    ///
    /// 验证源类型是否可以赋值给目标类型。
    /// 对于引用类型（Object），检查继承关系：子类可以赋值给父类。
    pub fn types_compatible(&self, from: &Type, to: &Type) -> bool {
        if from == to {
            return true;
        }

        // null 可以赋值给任何引用类型（包括 string 和指针）
        if let Type::Object(obj_name) = from {
            if obj_name == "Object" {
                // null 是 Object 类型，可以赋值给 String 或其他引用类型
                return true;
            }
        }

        // null (Object 类型) 可以赋值给任何指针类型
        if let Type::Object(obj_name) = from {
            if obj_name == "Object" && matches!(to, Type::Pointer(_)) {
                return true;
            }
        }

        // 基本类型之间的兼容
        match (from, to) {
            (Type::Int32, Type::Int64) => true,
            (Type::Int32, Type::Float32) => true,
            (Type::Int32, Type::Float64) => true,
            (Type::Int64, Type::Float64) => true,
            (Type::Float32, Type::Float64) => true,
            (Type::Float64, Type::Float32) => true, // 允许double到float转换（可能有精度损失）
            (Type::Object(from_name), Type::Object(to_name)) => {
                // 检查继承关系：from_name 是否是 to_name 的子类
                self.is_subtype_of(from_name, to_name)
            }
            // char 可以赋值给 int (ASCII 码值)
            (Type::Char, Type::Int32) => true,
            (Type::Char, Type::Int64) => true,
            // 数组类型：检查元素类型兼容性
            (Type::Array(from_elem), Type::Array(to_elem)) => {
                self.types_compatible(from_elem, to_elem)
            }
            // FFI 类型与基本类型之间的兼容
            // c_int <-> int
            (Type::CInt, Type::Int32) | (Type::Int32, Type::CInt) => true,
            // c_uint <-> int
            (Type::CUInt, Type::Int32) | (Type::Int32, Type::CUInt) => true,
            // c_long <-> long 和 int
            (Type::CLong, Type::Int64) | (Type::Int64, Type::CLong) => true,
            (Type::CLong, Type::Int32) | (Type::Int32, Type::CLong) => true,
            // c_ulong <-> long/int/c_long
            (Type::CULong, Type::Int64) | (Type::Int64, Type::CULong) => true,
            (Type::CULong, Type::Int32) | (Type::Int32, Type::CULong) => true,
            (Type::CULong, Type::CLong) | (Type::CLong, Type::CULong) => true,
            // c_short <-> int
            (Type::CShort, Type::Int32) | (Type::Int32, Type::CShort) => true,
            // c_char <-> int 或 char
            (Type::CChar, Type::Int32) | (Type::Int32, Type::CChar) => true,
            (Type::CChar, Type::Char) | (Type::Char, Type::CChar) => true,
            // c_float <-> float
            (Type::CFloat, Type::Float32) | (Type::Float32, Type::CFloat) => true,
            // c_double <-> double
            (Type::CDouble, Type::Float64) | (Type::Float64, Type::CDouble) => true,
            // size_t/ssize_t <-> long 和 int
            (Type::SizeT, Type::Int64) | (Type::Int64, Type::SizeT) => true,
            (Type::SizeT, Type::Int32) | (Type::Int32, Type::SizeT) => true,
            (Type::SSizeT, Type::Int64) | (Type::Int64, Type::SSizeT) => true,
            (Type::SSizeT, Type::Int32) | (Type::Int32, Type::SSizeT) => true,
            // uintptr_t/intptr_t <-> long 和 int
            (Type::UIntPtr, Type::Int64) | (Type::Int64, Type::UIntPtr) => true,
            (Type::UIntPtr, Type::Int32) | (Type::Int32, Type::UIntPtr) => true,
            (Type::IntPtr, Type::Int64) | (Type::Int64, Type::IntPtr) => true,
            (Type::IntPtr, Type::Int32) | (Type::Int32, Type::IntPtr) => true,
            // ptr (void*) <-> uintptr_t/intptr_t
            (Type::Pointer(_), Type::UIntPtr) | (Type::UIntPtr, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::IntPtr) | (Type::IntPtr, Type::Pointer(_)) => true,
            // c_bool <-> bool 和 int
            (Type::CBool, Type::Bool) | (Type::Bool, Type::CBool) => true,
            (Type::CBool, Type::Int32) | (Type::Int32, Type::CBool) => true,
            // String -> c_string (c_char*) 自动转换
            (Type::String, Type::Pointer(to_inner)) => {
                if matches!(to_inner.as_ref(), Type::CChar) {
                    return true;
                }
                false
            }
            // 指针类型与 long/int 之间的兼容（用于 FFI）
            (Type::Pointer(_), Type::Int64) | (Type::Int64, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::Int32) | (Type::Int32, Type::Pointer(_)) => true,
            // FFI 整数类型 <-> 指针 (用于 c_long/c_ulong 等作为指针值的场景)
            (Type::Pointer(_), Type::CLong) | (Type::CLong, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CULong) | (Type::CULong, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CInt) | (Type::CInt, Type::Pointer(_)) => true,
            (Type::Pointer(_), Type::CUInt) | (Type::CUInt, Type::Pointer(_)) => true,
            // ptr (void*) 可以转换为任何其他指针类型（C 语言规则）
            (Type::Pointer(from_inner), Type::Pointer(_)) => {
                if matches!(from_inner.as_ref(), Type::CVoid) {
                    return true;
                }
                false
            }
            // 数组类型可以退化为指针类型（数组作为参数传递时退化为指针）
            (Type::Array(_), Type::Pointer(_)) => true,
            // FFI 类型之间的兼容
            (Type::CInt, Type::CLong) | (Type::CLong, Type::CInt) => true,
            (Type::CInt, Type::CShort) | (Type::CShort, Type::CInt) => true,
            (Type::CInt, Type::CChar) | (Type::CChar, Type::CInt) => true,
            (Type::CFloat, Type::CDouble) | (Type::CDouble, Type::CFloat) => true,
            (Type::SizeT, Type::UIntPtr) | (Type::UIntPtr, Type::SizeT) => true,
            (Type::SSizeT, Type::IntPtr) | (Type::IntPtr, Type::SSizeT) => true,
            (Type::UIntPtr, Type::IntPtr) | (Type::IntPtr, Type::UIntPtr) => true,
            // 函数类型兼容性：函数指针可以赋值给兼容的函数指针类型
            // 顶层函数可以赋值给函数指针类型（当参数和返回类型匹配时）
            (Type::Function(from_fn), Type::Function(to_fn)) => {
                // 检查返回类型是否兼容
                let ret_compatible = Self::types_compatible(self, &from_fn.return_type, &to_fn.return_type);
                // 检查参数数量是否相同
                let params_count_match = from_fn.params.len() == to_fn.params.len();
                // 检查每个参数类型是否兼容（允许协变/逆变）
                let params_compatible = if params_count_match {
                    from_fn.params.iter().zip(to_fn.params.iter())
                        .all(|(from_param, to_param)| {
                            Self::types_compatible(self, from_param, to_param) || 
                            Self::types_compatible(self, to_param, from_param)
                        })
                } else {
                    false
                };
                ret_compatible && params_count_match && params_compatible
            }
            _ => false,
        }
    }

    /// 类型提升规则
    pub fn promote_types(&self, left: &Type, right: &Type) -> Type {
        match (left, right) {
            (Type::Float64, _) | (_, Type::Float64) => Type::Float64,
            (Type::Float32, _) | (_, Type::Float32) => Type::Float32,
            (Type::Int64, _) | (_, Type::Int64) => Type::Int64,
            // char 类型在算术运算中提升为 int32
            (Type::Char, Type::Char) => Type::Int32,
            (Type::Char, Type::Int32) | (Type::Int32, Type::Char) => Type::Int32,
            (Type::Int32, Type::Int32) => Type::Int32,
            _ => left.clone(),
        }
    }

    /// 检查类型是否为数值类型
    /// 检查类型是否为数值类型
    /// 时间复杂度: O(1)
    pub fn is_numeric_type(ty: &Type) -> bool {
        matches!(ty, 
            // 内置数值类型
            Type::Int32 | Type::Int64 | Type::Float32 | Type::Float64 | Type::Char |
            // FFI 数值类型
            Type::CInt | Type::CUInt | Type::CLong |
            Type::CShort | Type::CUShort | Type::CChar | Type::CUChar |
            Type::CFloat | Type::CDouble | Type::SizeT | Type::SSizeT |
            Type::UIntPtr | Type::IntPtr
        )
    }

    /// 检查 subtype 是否是 supertype 的子类型
    ///
    /// 通过递归遍历继承层次结构来确定类型兼容性。
    /// 子类可以赋值给父类（里氏替换原则）。
    ///
    /// # Arguments
    /// * `subtype` - 待检查的子类型名称
    /// * `supertype` - 目标父类型名称
    ///
    /// # Returns
    /// 如果 subtype 是 supertype 的子类型则返回 true
    ///
    /// # Algorithm
    /// 时间复杂度: O(h)，其中 h 是继承链的高度
    /// 空间复杂度: O(1)，迭代实现避免递归栈溢出
    fn is_subtype_of(&self, subtype: &str, supertype: &str) -> bool {
        // 相同类型必然是子类型
        if subtype == supertype {
            return true;
        }
        
        // 特殊处理：所有类都是 Object 的子类型
        if supertype == "Object" {
            // 检查 subtype 是否是一个有效的类名（不是内置类型别名）
            return self.type_registry.class_exists(subtype)
                || subtype == "String"
                || subtype == "Function";
        }
        
        // 迭代遍历继承链
        let mut current = subtype.to_string();
        let mut visited = std::collections::HashSet::new();
        
        loop {
            // 防止循环继承导致的无限循环
            if !visited.insert(current.clone()) {
                return false; // 检测到循环继承
            }
            
            if let Some(class_info) = self.type_registry.get_class(&current) {
                match &class_info.parent {
                    Some(parent) => {
                        if parent == supertype {
                            return true;
                        }
                        current = parent.clone();
                    }
                    None => return false, // 到达继承链顶端
                }
            } else {
                // 如果不是类，检查内置类型关系
                // String 是 Object 的子类型，但其他内置类型不是
                return (subtype == "String" || subtype == "Function") && supertype == "Object";
            }
        }
    }

    /// 整数类型提升
    pub fn promote_integer_types(&self, left: &Type, right: &Type) -> Type {
        match (left, right) {
            (Type::Int64, _) | (_, Type::Int64) => Type::Int64,
            _ => Type::Int32,
        }
    }

    /// 检查参数是否与参数定义兼容（支持可变参数）
    pub fn check_arguments_compatible(&mut self, args: &[Expr], params: &[ParameterInfo], _line: usize, _column: usize) -> Result<(), String> {
        if params.is_empty() {
            if args.is_empty() {
                return Ok(());
            } else {
                return Err(format!("Expected 0 arguments, got {}", args.len()));
            }
        }

        // 检查最后一个参数是否是可变参数
        let last_idx = params.len() - 1;
        if params[last_idx].is_varargs {
            // 可变参数：至少需要 params.len() - 1 个参数
            if args.len() < last_idx {
                return Err(format!("Expected at least {} arguments, got {}", last_idx, args.len()));
            }

            // 检查固定参数
            for i in 0..last_idx {
                let arg_type = self.infer_expr_type_collect_errors(&args[i]);
                if !self.types_compatible(&arg_type, &params[i].param_type) {
                    return Err(format!("Argument {} type mismatch: expected {}, got {}",
                        i + 1, params[i].param_type, arg_type));
                }
            }

            // 检查可变参数
            // 可变参数类型是 Array(ElementType)，需要匹配 ElementType
            let vararg_param_type = &params[last_idx].param_type;
            let vararg_element_type = match vararg_param_type {
                Type::Array(elem) => elem.as_ref(),
                _ => vararg_param_type,
            };

            // 如果只有一个参数且类型匹配数组类型，直接接受（传递数组给可变参数）
            if args.len() == last_idx + 1 {
                let arg_type = self.infer_expr_type_collect_errors(&args[last_idx]);
                if self.types_compatible(&arg_type, vararg_param_type) {
                    // 参数类型与可变参数的数组类型匹配，直接接受
                    return Ok(());
                }
            }

            // 否则，按元素类型检查每个参数
            for i in last_idx..args.len() {
                let arg_type = self.infer_expr_type_collect_errors(&args[i]);
                if !self.types_compatible(&arg_type, vararg_element_type) {
                    return Err(format!("Varargs argument {} type mismatch: expected {}, got {}",
                        i + 1, vararg_element_type, arg_type));
                }
            }
        } else {
            // 非可变参数：参数数量必须完全匹配
            if params.len() != args.len() {
                return Err(format!("Expected {} arguments, got {}", params.len(), args.len()));
            }

            for (i, (arg, param)) in args.iter().zip(params.iter()).enumerate() {
                let arg_type = self.infer_expr_type_collect_errors(arg);
                if !self.types_compatible(&arg_type, &param.param_type) {
                    return Err(format!("Argument {} type mismatch: expected {}, got {}",
                        i + 1, param.param_type, arg_type));
                }
            }
        }

        Ok(())
    }

    /// 推断 String 方法调用的返回类型
    pub fn infer_string_method_call(&mut self, method_name: &str, args: &[Expr], line: usize, column: usize) -> cayResult<Type> {
        match method_name {
            "length" => {
                if !args.is_empty() {
                    return Err(self.report_error(line, column, "String.length() takes no arguments".to_string()));
                }
                Ok(Type::Int32)
            }
            "substring" => {
                if args.is_empty() || args.len() > 2 {
                    return Err(self.report_error(line, column, "String.substring() takes 1 or 2 arguments".to_string()));
                }
                // 检查参数类型
                for (i, arg) in args.iter().enumerate() {
                    let arg_type = self.infer_expr_type_collect_errors(arg);
                    if !arg_type.is_integer() {
                        return Err(self.report_error(line, column, format!("Argument {} of substring() must be integer, got {}", i + 1, arg_type)));
                    }
                }
                Ok(Type::String)
            }
            "indexOf" => {
                if args.len() < 1 || args.len() > 2 {
                    return Err(self.report_error(line, column, "String.indexOf() takes 1 or 2 arguments".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("First argument of indexOf() must be string, got {}", arg_type)));
                }
                if args.len() == 2 {
                    let start_type = self.infer_expr_type_collect_errors(&args[1]);
                    if !start_type.is_integer() {
                        return Err(self.report_error(line, column, format!("Second argument of indexOf() must be integer, got {}", start_type)));
                    }
                }
                Ok(Type::Int32)
            }
            "lastIndexOf" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.lastIndexOf() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of lastIndexOf() must be string, got {}", arg_type)));
                }
                Ok(Type::Int32)
            }
            "charAt" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.charAt() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if !arg_type.is_integer() {
                    return Err(self.report_error(line, column, format!("Argument of charAt() must be integer, got {}", arg_type)));
                }
                Ok(Type::Char)
            }
            "replace" => {
                if args.len() != 2 {
                    return Err(self.report_error(line, column, "String.replace() takes 2 arguments".to_string()));
                }
                for (i, arg) in args.iter().enumerate() {
                    let arg_type = self.infer_expr_type_collect_errors(arg);
                    if arg_type != Type::String {
                        return Err(self.report_error(line, column, format!("Argument {} of replace() must be string, got {}", i + 1, arg_type)));
                    }
                }
                Ok(Type::String)
            }
            "isEmpty" => {
                if !args.is_empty() {
                    return Err(self.report_error(line, column, "String.isEmpty() takes no arguments".to_string()));
                }
                Ok(Type::Bool)
            }
            "equals" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.equals() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of equals() must be string, got {}", arg_type)));
                }
                Ok(Type::Bool)
            }
            "equalsIgnoreCase" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.equalsIgnoreCase() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of equalsIgnoreCase() must be string, got {}", arg_type)));
                }
                Ok(Type::Bool)
            }
            "c_str" => {
                if !args.is_empty() {
                    return Err(self.report_error(line, column, "String.c_str() takes no arguments".to_string()));
                }
                Ok(Type::Pointer(Box::new(Type::CChar)))  // 返回 c_char* 指针类型，与 codegen 中的 i8* 一致
            }
            "startsWith" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.startsWith() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of startsWith() must be string, got {}", arg_type)));
                }
                Ok(Type::Bool)
            }
            "endsWith" => {
                if args.len() != 1 {
                    return Err(self.report_error(line, column, "String.endsWith() takes 1 argument".to_string()));
                }
                let arg_type = self.infer_expr_type_collect_errors(&args[0]);
                if arg_type != Type::String {
                    return Err(self.report_error(line, column, format!("Argument of endsWith() must be string, got {}", arg_type)));
                }
                Ok(Type::Bool)
            }
            "trim" => {
                if !args.is_empty() {
                    return Err(self.report_error(line, column, "String.trim() takes no arguments".to_string()));
                }
                Ok(Type::String)
            }
            _ => Err(self.report_error(line, column, format!("Unknown String method '{}'", method_name))),
        }
    }
}
