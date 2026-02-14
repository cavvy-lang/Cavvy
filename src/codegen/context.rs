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

/// 类型标识符信息
#[derive(Debug, Clone)]
pub struct TypeIdInfo {
    pub class_name: String,
    pub parent_type_id: Option<String>,
    pub interfaces: Vec<String>,
    pub type_id_value: i32,  // 唯一的整数标识符
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
    pub current_return_type: String,
    pub var_types: HashMap<String, String>,
    pub var_class_map: HashMap<String, String>,
    pub loop_stack: Vec<LoopContext>,
    pub target_triple: String,
    pub static_fields: Vec<StaticFieldInfo>,
    pub static_field_map: HashMap<String, StaticFieldInfo>,
    pub type_registry: Option<TypeRegistry>,
    pub scope_manager: ScopeManager,
    pub lambda_functions: Vec<String>,
    pub code: String,
    pub method_declarations: Vec<String>,
    pub type_id_map: HashMap<String, TypeIdInfo>,
    pub type_id_counter: usize,
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
            type_id_map: HashMap::new(),
            type_id_counter: 0,
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
    /// 格式: ClassName.__methodName_param1Type_param2Type
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
    pub fn type_to_signature(&self, ty: &crate::types::Type) -> String {
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
            Type::Auto => panic!("Type::Auto should have been resolved before code generation"),
        }
    }

    /// 注册类型标识符
    pub fn register_type_id(&mut self, class_name: &str, parent_name: Option<&str>, interfaces: Vec<String>) -> String {
        let type_id = format!("@__type_id_{}", class_name);
        let parent_type_id = parent_name.map(|p| format!("@__type_id_{}", p));
        let type_id_value = self.type_id_counter as i32;
        self.type_id_counter += 1;
        
        self.type_id_map.insert(
            class_name.to_string(),
            TypeIdInfo {
                class_name: class_name.to_string(),
                parent_type_id,
                interfaces,
                type_id_value,
            }
        );
        
        type_id
    }
    
    /// 获取类型的整数标识符值
    pub fn get_type_id_value(&self, class_name: &str) -> Option<i32> {
        self.type_id_map.get(class_name).map(|info| info.type_id_value)
    }

    /// 获取类型标识符
    pub fn get_type_id(&self, class_name: &str) -> Option<String> {
        self.type_id_map.get(class_name).map(|_| format!("@__type_id_{}", class_name))
    }

    /// 检查类型是否是另一个类型的子类或实现了该接口
    pub fn is_subtype(&self, class_name: &str, target_name: &str) -> bool {
        if class_name == target_name {
            return true;
        }
        
        let mut current = class_name.to_string();
        while let Some(info) = self.type_id_map.get(&current) {
            // 检查是否实现了目标接口
            if info.interfaces.contains(&target_name.to_string()) {
                return true;
            }
            // 检查父类
            if let Some(ref parent) = info.parent_type_id {
                let parent_class = parent.replace("@__type_id_", "");
                if parent_class == target_name {
                    return true;
                }
                current = parent_class;
            } else {
                break;
            }
        }
        
        false
    }

    /// 生成类型标识符全局变量声明
    pub fn emit_type_id_declarations(&self) -> String {
        let mut result = String::new();
        for (class_name, info) in &self.type_id_map {
            let type_id_name = format!("@__type_id_{}", class_name);
            // 使用整数标识符作为类型标识符的值
            result.push_str(&format!(
                "{} = private constant i32 {}, align 4\n",
                type_id_name, info.type_id_value
            ));
        }
        result
    }
}
