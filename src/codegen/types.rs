//! 类型转换和类型系统支持
use crate::codegen::context::IRGenerator;
use crate::types::Type;
use crate::miette_diagnostic::ErrorCodes;

impl IRGenerator {
    /// 解析类型（包括类型别名）
    pub fn resolve_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Object(name) => {
                // 检查是否是类型别名
                if let Some(aliased_type) = self.type_aliases.get(name) {
                    aliased_type.clone()
                } else {
                    ty.clone()
                }
            }
            _ => ty.clone(),
        }
    }

    /// 将 cay 类型转换为 LLVM IR 类型
    pub fn type_to_llvm(&self, ty: &Type) -> String {
        // 首先解析类型别名
        let resolved_ty = self.resolve_type(ty);

        match &resolved_ty {
            Type::Void => "void".to_string(),
            Type::Int32 => "i32".to_string(),
            Type::Int64 => "i64".to_string(),
            Type::Float32 => "float".to_string(),
            Type::Float64 => "double".to_string(),
            Type::Bool => "i1".to_string(),
            Type::String => "i8*".to_string(),
            Type::Char => "i8".to_string(),
            Type::Object(name) => {
                // 检查是否是 enum 类型 - enum 存储为 struct { i32 discriminant, i64 payload }
                if let Some(ref registry) = self.type_registry {
                    if registry.get_enum_by_name(name).is_some() {
                        "{ i32, i64 }".to_string()
                    } else if registry.get_struct(name).is_some() {
                        // struct 类型使用 %struct.Name* 指针
                        format!("%struct.{}*", name)
                    } else {
                        "i8*".to_string()
                    }
                } else {
                    "i8*".to_string()
                }
            }
            Type::Array(inner) => {
                // LLVM 不允许 void*，使用 i8* 代替
                if matches!(inner.as_ref(), Type::CVoid) {
                    "i8*".to_string()
                } else {
                    format!("{}*", self.type_to_llvm(inner))
                }
            }
            Type::Function(_) => "i8*".to_string(),
            Type::Auto => "i8*".to_string(), // 不应到达此处，语义分析应已解析
            // FFI 类型映射
            Type::CInt => "i32".to_string(),    // C int 通常为 32 位
            Type::CUInt => "i32".to_string(),   // C unsigned int 通常为 32 位
            Type::CLong => self.c_long_llvm(),  // 平台相关
            Type::CULong => self.c_long_llvm(), // 同 CLong
            Type::CShort => "i16".to_string(),  // C short 为 16 位
            Type::CUShort => "i16".to_string(), // C unsigned short 为 16 位
            Type::CChar => "i8".to_string(),    // C char 为 8 位
            Type::CUChar => "i8".to_string(),   // C unsigned char 为 8 位
            Type::CFloat => "float".to_string(), // C float 为 32 位
            Type::CDouble => "double".to_string(), // C double 为 64 位
            Type::SizeT => "i64".to_string(),   // size_t 在 64 位系统为 64 位
            Type::SSizeT => "i64".to_string(),  // ssize_t 在 64 位系统为 64 位
            Type::UIntPtr => "i64".to_string(), // uintptr_t 在 64 位系统为 64 位
            Type::IntPtr => "i64".to_string(),  // intptr_t 在 64 位系统为 64 位
            Type::CVoid => "void".to_string(),  // C void
            Type::CBool => "i8".to_string(),    // C bool 通常为 8 位
            // FFI 指针和结构体
            Type::Pointer(inner) => {
                // LLVM 不允许 void*，使用 i8* 代替
                if matches!(inner.as_ref(), Type::CVoid) {
                    "i8*".to_string()
                } else {
                    format!("{}*", self.type_to_llvm(inner))
                }
            }
            Type::Struct(name) => format!("%struct.{}*", name), // 命名结构体指针（变量存储指针）
            // 泛型类型参数 - 编译期单态化
            // 注：在正确的单态化流程中，所有 GenericParam 应在代码生成前被替换为具体类型
            // 若到达此处，说明单态化阶段有遗漏，返回 i8* 作为安全回退并记录警告
            Type::GenericParam(param_name) => {
                if let Some(actual_type) = self.generic_type_args.get(param_name) {
                    // 防御：若类型参数解析到自身，说明单态化上下文不完整
                    // （如收集到未替换的泛型实例）。直接回退到 i8*，避免
                    // type_to_llvm 无限递归导致栈溢出。
                    match actual_type {
                        Type::GenericParam(inner) if inner == param_name => "i8*".to_string(),
                        _ => self.type_to_llvm(actual_type),
                    }
                } else {
                    let warning = crate::miette_diagnostic::codegen_warning_at(ErrorCodes::CODEGEN_INVALID_OPERATION, 
                        crate::miette_diagnostic::SourceLocation::new(Some(self.source_file.clone()), self.source_line, self.source_column),
                        format!("泛型类型参数 '{}' 未在单态化上下文中解析，将使用 i8*。", param_name)
                    );
                    self.warnings.borrow_mut().push(warning);
                    "i8*".to_string()
                }
            }
            Type::Generic(class_name, type_args) => {
                // 泛型类型实例（如 Box<int>）。类实例在本编译器中统一用不透明指针 i8*
                // 表示，enum/struct 有各自的表示。此处按基础类名分类解析，避免误报警告。
                let base_name = class_name
                    .split('<')
                    .next()
                    .unwrap_or(class_name)
                    .trim_end();
                if let Some(ref registry) = self.type_registry {
                    if registry.get_enum_by_name(base_name).is_some() {
                        return "{ i32, i64 }".to_string();
                    }
                    if registry.get_struct(base_name).is_some() {
                        // struct 特化仍使用基础名的命名结构体指针
                        return format!("%struct.{}*", base_name);
                    }
                }
                // 未使用的 type_args 变量占位（类实例为不透明指针，无需展开）
                let _ = type_args;
                "i8*".to_string()
            }
        }
    }

    /// 获取 C long 类型的 LLVM 表示（平台相关）
    fn c_long_llvm(&self) -> String {
        // Windows: long 是 32 位
        // Linux/macOS: long 是 64 位
        if self.is_windows_target() {
            "i32".to_string()
        } else {
            "i64".to_string()
        }
    }

    /// 解析类型化的值，返回 (类型, 值)
    pub fn parse_typed_value(&self, typed_val: &str) -> (String, String) {
        // Handle struct types like { i32, i64 } or { i32, i64 }* which contain spaces
        if typed_val.starts_with('{') {
            // Find matching closing brace
            let mut depth = 0;
            let mut brace_end = 0;
            for (i, ch) in typed_val.char_indices() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                }
                if depth == 0 {
                    brace_end = i + 1;
                    break;
                }
            }
            if brace_end > 0 {
                // Check for pointer suffix (*) right after closing brace
                let after_brace = typed_val[brace_end..].trim_start();
                let type_end = if after_brace.starts_with('*') {
                    brace_end + (typed_val[brace_end..].find('*').unwrap_or(0) + 1)
                } else {
                    brace_end
                };
                let type_part = &typed_val[..type_end];
                let value_part = typed_val[type_end..].trim();
                if !value_part.is_empty() {
                    return (type_part.to_string(), value_part.to_string());
                }
            }
        }
        let parts: Vec<&str> = typed_val.splitn(2, ' ').collect();
        if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string())
        } else {
            ("i64".to_string(), typed_val.to_string())
        }
    }

    /// 判断是否为整数类型
    pub fn is_integer_type(&self, ty: &str) -> bool {
        ty.starts_with("i") && !ty.ends_with("*")
    }

    /// 判断是否为浮点类型
    pub fn is_float_type(&self, ty: &str) -> bool {
        ty == "float" || ty == "double"
    }

    /// 判断是否为布尔类型
    pub fn is_bool_type(&self, ty: &str) -> bool {
        ty == "i1"
    }

    /// 判断是否为字符串类型
    pub fn is_string_type(&self, ty: &str) -> bool {
        ty == "i8*"
    }
}
