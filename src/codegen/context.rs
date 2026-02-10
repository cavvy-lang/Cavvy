//! IR生成上下文和状态管理
use std::collections::HashMap;
use crate::types::TypeRegistry;

/// 循环上下文，用于支持 break/continue
#[derive(Debug, Clone)]
pub struct LoopContext {
    pub cond_label: String,  // continue 跳转的目标（条件检查）
    pub end_label: String,   // break 跳转的目标（循环结束）
}

/// 静态字段信息
#[derive(Debug, Clone)]
pub struct StaticFieldInfo {
    pub name: String,           // 完整名称: @ClassName.fieldName
    pub llvm_type: String,      // LLVM 类型
    pub size: usize,            // 大小（字节）
    pub field_type: crate::types::Type,  // 原始类型
    pub initializer: Option<crate::ast::Expr>,  // 初始化器
    pub class_name: String,     // 类名
    pub field_name: String,     // 字段名
}

/// 变量作用域信息
#[derive(Debug, Clone)]
pub struct VarScope {
    pub name: String,           // 原始变量名
    pub llvm_name: String,      // LLVM 中的唯一名称（带作用域后缀）
    pub var_type: String,       // 变量类型
}

/// 作用域栈管理
pub struct ScopeManager {
    scopes: Vec<HashMap<String, VarScope>>,  // 作用域栈
    scope_counter: usize,                     // 作用域计数器（用于生成唯一名称）
}

impl ScopeManager {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],  // 全局作用域
            scope_counter: 0,
        }
    }

    /// 进入新作用域
    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.scope_counter += 1;
    }

    /// 退出当前作用域
    pub fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    /// 声明变量（在当前作用域）
    pub fn declare_var(&mut self, name: &str, var_type: &str) -> String {
        let llvm_name = if self.scopes.len() == 1 {
            // 全局作用域，使用原始名称
            name.to_string()
        } else {
            // 局部作用域，添加作用域后缀
            format!("{}_s{}", name, self.scope_counter)
        };

        let var_scope = VarScope {
            name: name.to_string(),
            llvm_name: llvm_name.clone(),
            var_type: var_type.to_string(),
        };

        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), var_scope);
        }

        llvm_name
    }

    /// 查找变量（从内层作用域到外层）
    pub fn lookup_var(&self, name: &str) -> Option<&VarScope> {
        for scope in self.scopes.iter().rev() {
            if let Some(var) = scope.get(name) {
                return Some(var);
            }
        }
        None
    }

    /// 获取变量类型
    pub fn get_var_type(&self, name: &str) -> Option<String> {
        self.lookup_var(name).map(|v| v.var_type.clone())
    }

    /// 获取变量的 LLVM 名称
    pub fn get_llvm_name(&self, name: &str) -> Option<String> {
        self.lookup_var(name).map(|v| v.llvm_name.clone())
    }

    /// 检查变量是否在当前作用域中声明
    pub fn is_declared_in_current_scope(&self, name: &str) -> bool {
        self.scopes.last().map_or(false, |s| s.contains_key(name))
    }

    /// 重置（用于新函数）
    pub fn reset(&mut self) {
        self.scopes.clear();
        self.scopes.push(HashMap::new());
        self.scope_counter = 0;
    }
}

/// IR生成器核心上下文
pub struct IRGenerator {
    pub output: String,
    pub indent: usize,
    pub label_counter: usize,
    pub temp_counter: usize,
    pub global_strings: HashMap<String, String>,
    pub global_counter: usize,
    pub current_function: String,
    pub current_class: String,
    pub current_return_type: String,   // 当前函数的返回类型
    pub var_types: HashMap<String, String>,  // 保留用于兼容性
    pub var_class_map: HashMap<String, String>,
    pub loop_stack: Vec<LoopContext>,  // 循环上下文栈
    pub target_triple: String,         // 目标平台三元组
    pub static_fields: Vec<StaticFieldInfo>, // 静态字段列表
    pub static_field_map: HashMap<String, StaticFieldInfo>, // 静态字段映射（按类名.字段名）
    pub type_registry: Option<TypeRegistry>, // 类型注册表（可选，用于方法查找）
    pub scope_manager: ScopeManager,   // 作用域管理器
    pub lambda_functions: Vec<String>, // Lambda 函数字符串列表
    pub code: String,                  // 当前代码缓冲区
    pub method_declarations: Vec<String>, // 方法声明列表
}

impl IRGenerator {
    pub fn new() -> Self {
        Self::with_target("x86_64-w64-mingw32".to_string())
    }

    pub fn with_target(target_triple: String) -> Self {
        Self {
            output: String::new(),
            indent: 0,
            label_counter: 0,
            temp_counter: 0,
            global_strings: HashMap::new(),
            global_counter: 0,
            current_function: String::new(),
            current_class: String::new(),
            current_return_type: String::new(),
            var_types: HashMap::new(),
            var_class_map: HashMap::new(),
            loop_stack: Vec::new(),
            target_triple,
            static_fields: Vec::new(),
            static_field_map: HashMap::new(),
            type_registry: None,
            scope_manager: ScopeManager::new(),
            lambda_functions: Vec::new(),
            code: String::new(),
            method_declarations: Vec::new(),
        }
    }

    /// 设置类型注册表
    pub fn set_type_registry(&mut self, registry: TypeRegistry) {
        self.type_registry = Some(registry);
    }

    /// 检查是否是 Windows 目标平台
    pub fn is_windows_target(&self) -> bool {
        self.target_triple.contains("windows") || self.target_triple.contains("mingw32")
    }

    /// 获取 i64 类型的 printf/scanf 格式符
    /// Windows 平台使用 %lld，其他平台使用 %ld
    pub fn get_i64_format_specifier(&self) -> &'static str {
        if self.is_windows_target() {
            "%lld"
        } else {
            "%ld"
        }
    }

    /// 发射一行代码到当前代码缓冲区
    pub fn emit_line(&mut self, line: &str) {
        if !line.is_empty() {
            self.code.push_str(&"  ".repeat(self.indent));
        }
        self.code.push_str(line);
        self.code.push('\n');
    }

    /// 发射代码但不添加缩进（用于全局声明）
    pub fn emit_raw(&mut self, line: &str) {
        self.output.push_str(line);
        self.output.push('\n');
    }


    /// 获取类型的 LLVM 对齐字节数
    pub fn get_type_align(&self, llvm_type: &str) -> u32 {
        match llvm_type {
            "i1" | "i8" => 1,
            "i16" => 2,
            "i32" | "float" => 4,  // float 是 4 字节对齐！
            "i64" | "double" => 8,
            t if t.ends_with("*") => 8,  // 所有指针都是 8 字节（64位系统）
            _ => 8, // 默认 8 字节
        }
    }

    /// 创建新标签
    pub fn new_label(&mut self, prefix: &str) -> String {
        let label = format!("{}.{}", prefix, self.label_counter);
        self.label_counter += 1;
        label
    }

    /// 创建新的临时变量
    pub fn new_temp(&mut self) -> String {
        let temp = format!("%t{}", self.temp_counter);
        self.temp_counter += 1;
        temp
    }

    /// 进入循环上下文
    pub fn enter_loop(&mut self, cond_label: String, end_label: String) {
        self.loop_stack.push(LoopContext { cond_label, end_label });
    }

    /// 退出循环上下文
    pub fn exit_loop(&mut self) {
        self.loop_stack.pop();
    }

    /// 获取当前循环上下文（用于 break/continue）
    pub fn current_loop(&self) -> Option<&LoopContext> {
        self.loop_stack.last()
    }

    /// 获取或创建字符串常量
    pub fn get_or_create_string_constant(&mut self, s: &str) -> String {
        if let Some(name) = self.global_strings.get(s) {
            return name.clone();
        }

        let name = format!("@.str.{}", self.global_counter);
        self.global_counter += 1;

        // 转义字符串
        let escaped = s.replace("\\", "\\\\")
            .replace("\"", "\\\"")
            .replace("\n", "\\0A")
            .replace("\r", "\\0D")
            .replace("\t", "\\09");

        // 存储以便稍后输出到全局区
        self.global_strings.insert(s.to_string(), name.clone());

        name
    }

    /// 获取字符串常量的声明
    pub fn get_string_declarations(&self) -> String {
        let mut result = String::new();
        for (s, name) in &self.global_strings {
            // 计算实际字节数：使用UTF-8字节长度
            let actual_len = s.as_bytes().len();
            
            // 转义特殊字符用于LLVM IR输出
            // 在LLVM IR中，特殊字符使用十六进制转义序列
            let escaped = s.replace("\\", "\\5C")
                .replace("\"", "\\22")
                .replace("\n", "\\0A")
                .replace("\r", "\\0D")
                .replace("\t", "\\09")
                .replace("\0", "\\00");
            let len = actual_len + 1; // +1 for null terminator
            result.push_str(&format!(
                "{} = private unnamed_addr constant [{} x i8] c\"{}\\00\", align 1\n",
                name, len, escaped
            ));
        }
        result
    }

    /// 获取全局字符串映射（用于后处理）
    pub fn get_global_strings(&self) -> &std::collections::HashMap<String, String> {
        &self.global_strings
    }

    /// 生成带参数签名的方法名以支持方法重载
    /// 格式: ClassName.methodName__param1Type_param2Type
    /// 注意：LLVM IR 中函数名不能包含 @ 符号，使用 __ 作为分隔符
    pub fn generate_method_name(&self, class_name: &str, method: &crate::ast::MethodDecl) -> String {
        if method.params.is_empty() {
            // 无参数方法，使用简单名称
            format!("{}.{}", class_name, method.name)
        } else {
            // 有参数方法，添加参数类型签名
            let param_types: Vec<String> = method.params.iter()
                .map(|p| self.type_to_signature(&p.param_type))
                .collect();
            format!("{}.__{}_{}", class_name, method.name, param_types.join("_"))
        }
    }

    /// 将类型转换为方法签名的一部分
    fn type_to_signature(&self, ty: &crate::types::Type) -> String {
        use crate::types::Type;
        match ty {
            Type::Void => "v".to_string(),
            Type::Int32 => "i".to_string(),
            Type::Int64 => "l".to_string(),
            Type::Float32 => "f".to_string(),
            Type::Float64 => "d".to_string(),
            Type::Bool => "b".to_string(),
            Type::String => "s".to_string(),
            Type::Char => "c".to_string(),
            Type::Object(name) => format!("o{}", name),
            Type::Array(inner) => format!("a{}", self.type_to_signature(inner)),
            Type::Function(_) => "fn".to_string(),
        }
    }
}
