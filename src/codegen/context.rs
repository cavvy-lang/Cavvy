//! IR生成上下文和状态管理
use crate::codegen::platform::PlatformConfig;
use crate::types::TypeRegistry;
use std::collections::{HashMap, HashSet};

fn normalize_source_file_path(path: &str) -> String {
    path.strip_prefix("\\\\?\\").unwrap_or(path).to_string()
}

/// 循环上下文，用于支持 break/continue
#[derive(Debug, Clone)]
pub struct LoopContext {
    pub cond_label: String,    // continue 跳转的目标（条件检查）
    pub end_label: String,     // break 跳转的目标（循环结束）
    pub label: Option<String>, // 循环标签（用于带标签的 break/continue）
}

/// 静态字段信息
#[derive(Debug, Clone)]
pub struct StaticFieldInfo {
    pub name: String,                          // 完整名称: @ClassName.fieldName
    pub llvm_type: String,                     // LLVM 类型
    pub size: usize,                           // 大小（字节）
    pub field_type: crate::types::Type,        // 原始类型
    pub initializer: Option<crate::ast::Expr>, // 初始化器
    pub class_name: String,                    // 类名
    pub field_name: String,                    // 字段名
}

/// 实例字段信息
#[derive(Debug, Clone)]
pub struct InstanceFieldInfo {
    pub name: String,                   // 字段名
    pub llvm_type: String,              // LLVM 类型
    pub field_type: crate::types::Type, // 原始类型
    pub offset: usize,                  // 在对象中的偏移量（字节）
    pub size: usize,                    // 大小（字节）
}

/// 类实例布局信息
#[derive(Debug, Clone)]
pub struct ClassLayoutInfo {
    pub class_name: String,
    pub total_size: usize,                          // 对象总大小（字节）
    pub fields: HashMap<String, InstanceFieldInfo>, // 字段名 -> 字段信息
}

impl ClassLayoutInfo {
    /// 计算字段的 GEP 索引（以 i8 为单位的偏移）
    pub fn get_field_gep_offset(&self, field_name: &str) -> Option<usize> {
        self.fields.get(field_name).map(|f| f.offset)
    }
}

/// 变量作用域信息
#[derive(Debug, Clone)]
pub struct VarScope {
    pub name: String,       // 原始变量名
    pub llvm_name: String,  // LLVM 中的唯一名称（带作用域后缀）
    pub var_type: String,   // 变量类型
    pub is_parameter: bool, // 是否是参数（参数存储的是值，局部变量存储的是对象指针）
}

/// 作用域栈管理
pub struct ScopeManager {
    scopes: Vec<HashMap<String, VarScope>>, // 作用域栈
    scope_counter: usize,                   // 作用域计数器（用于生成唯一名称）
}

impl ScopeManager {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()], // 全局作用域
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
        self.declare_var_with_flag(name, var_type, false)
    }

    /// 声明变量（带参数标志）
    pub fn declare_var_with_flag(
        &mut self,
        name: &str,
        var_type: &str,
        is_parameter: bool,
    ) -> String {
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
            is_parameter,
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

    /// 检查变量是否是参数
    pub fn is_parameter(&self, name: &str) -> bool {
        self.lookup_var(name)
            .map(|v| v.is_parameter)
            .unwrap_or(false)
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

    /// 获取所有可见变量（从内层到外层）
    ///
    /// 返回 (变量名, VarScope) 的列表
    pub fn get_all_visible_vars(&self) -> Vec<(String, &VarScope)> {
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for scope in self.scopes.iter().rev() {
            for (name, var_scope) in scope.iter() {
                if seen.insert(name.clone()) {
                    result.push((name.clone(), var_scope));
                }
            }
        }

        result
    }
}

/// 类型标识符信息
#[derive(Debug, Clone)]
pub struct TypeIdInfo {
    pub class_name: String,
    pub parent_type_id: Option<String>,
    pub interfaces: Vec<String>,
    pub type_id_value: i32, // 唯一的整数标识符
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
    pub var_cay_types: HashMap<String, crate::types::Type>, // 变量名到Cavvy类型的映射
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
    pub class_layouts: HashMap<String, ClassLayoutInfo>, // 类实例布局信息
    pub platform_config: Option<PlatformConfig>,
    pub extern_declarations: Vec<crate::ast::ExternDecl>, // FFI extern 声明
    pub extern_function_map: HashMap<String, usize>,      // 函数名 -> extern_declarations索引
    pub emitted_externs: HashSet<String>,                 // 已生成的extern声明（函数名 -> 签名）
    pub top_level_functions: Vec<crate::ast::TopLevelFunction>, // 顶层函数列表
    pub current_param_order: Vec<String>,                 // 当前函数参数顺序（用于内联IR）
    pub type_aliases: HashMap<String, crate::types::Type>, // 类型别名映射
    pub class_namespaces: HashMap<String, Vec<String>>,   // 类名 -> 命名空间路径 映射
    // 源映射相关
    pub current_ir_line: usize,  // 当前IR行号
    pub source_file: String,     // 当前源文件
    pub source_line: usize,      // 当前源行号
    pub source_column: usize,    // 当前源列号
    pub enable_source_map: bool, // 是否启用源映射
    pub preprocessor_source_map: Option<std::collections::HashMap<usize, (String, usize)>>, // 预处理器源映射 (输出行 -> (文件, 源行))
    pub reverse_source_map: Option<std::collections::HashMap<(String, usize), usize>>, // 反向映射 ((文件, 源行) -> 输出行)
    // DWARF 调试信息
    pub debug_info: bool,                    // 是否生成 DWARF 调试信息
    debug_node_counter: usize,               // 调试元数据节点计数器
    debug_file_node: usize,                  // DIFile 节点编号
    debug_empty_node: usize,                 // 空元组节点编号
    debug_subprograms: Vec<DebugSubprogram>, // 记录所有子程序元数据
    // 测试模式
    pub test_mode: bool,                     // 是否生成测试入口
    pub test_methods: Vec<(String, String)>, // (类名, 方法名) 列表
    pub field_initializers: HashMap<String, Vec<crate::ast::FieldDecl>>, // 类名 -> 有初始化器的字段列表
    pub lambda_captures: HashMap<String, Vec<(String, crate::types::Type)>>, // lambda函数名 -> 捕获变量列表 [(变量名, 类型)]
    pub lambda_envs: HashMap<String, String>, // lambda变量名 -> 环境指针临时变量名
    pub lambda_counter: usize,                // Lambda函数名计数器，确保唯一性
    // 泛型特化：当前类型参数映射（如 {"T" -> Type::Int32}）
    pub generic_type_args: HashMap<String, crate::types::Type>,
    // 泛型特化：已收集的特化实例（基础类名 -> 实例集合）
    pub specializations: HashMap<String, HashSet<crate::codegen::specialization::SpecializationInstance>>,
    // 泛型特化：已生成的特化方法名（避免重复生成）
    pub generated_specializations: HashSet<String>,
    // 已生成的 vtable 全局常量（避免重复生成）
    pub generated_vtables: HashSet<String>,
    // 已生成的方法定义（避免重复生成）
    pub generated_methods: HashSet<String>,
}

/// DWARF 子程序元数据
#[derive(Debug, Clone)]
struct DebugSubprogram {
    func_name: String,   // 函数名（LLVM 中的名称）
    source_file: String, // 源文件路径
    source_line: usize,  // 源行号
    node_id: usize,      // DISubprogram 节点编号
    type_node_id: usize, // DISubroutineType 节点编号
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
            var_cay_types: HashMap::new(),
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
            class_layouts: HashMap::new(),
            platform_config: None,
            extern_declarations: Vec::new(),
            extern_function_map: HashMap::new(),
            emitted_externs: HashSet::new(),
            top_level_functions: Vec::new(),
            current_param_order: Vec::new(),
            type_aliases: HashMap::new(),
            class_namespaces: HashMap::new(),
            // 源映射初始化
            current_ir_line: 1,
            source_file: String::new(),
            source_line: 1,
            source_column: 1,
            enable_source_map: true, // 默认启用
            preprocessor_source_map: None,
            reverse_source_map: None,
            // DWARF 调试信息初始化
            debug_info: false,
            debug_node_counter: 5, // 0=DICompileUnit, 1=DebugInfoVersion, 2=DwarfVersion, 3=DIFile, 4=empty tuple
            debug_file_node: 3,
            debug_empty_node: 4,
            debug_subprograms: Vec::new(),
            test_mode: false,
            test_methods: Vec::new(),
            field_initializers: HashMap::new(),
            lambda_captures: HashMap::new(),
            lambda_envs: HashMap::new(),
            lambda_counter: 0,
            generic_type_args: HashMap::new(),
            specializations: HashMap::new(),
            generated_specializations: HashSet::new(),
            generated_vtables: HashSet::new(),
            generated_methods: HashSet::new(),
        }
    }

    /// 设置类型注册表
    pub fn set_type_registry(&mut self, registry: TypeRegistry) {
        self.type_registry = Some(registry);
    }

    /// 设置预处理器源映射（用于多文件include场景）
    pub fn set_preprocessor_source_map(
        &mut self,
        source_map: std::collections::HashMap<usize, (String, usize)>,
    ) {
        // 创建反向映射：(文件, 源行) -> 输出行
        let mut reverse_map = std::collections::HashMap::new();
        for (output_line, (file, source_line)) in &source_map {
            reverse_map.insert((file.clone(), *source_line), *output_line);
        }
        self.reverse_source_map = Some(reverse_map);
        self.preprocessor_source_map = Some(source_map);
    }

    /// 设置 extern 声明并构建索引
    /// 支持别名：如果extern函数有alias，则只能通过alias调用，原名不再识别
    pub fn set_extern_declarations(&mut self, extern_declarations: Vec<crate::ast::ExternDecl>) {
        self.extern_function_map.clear();
        for (decl_idx, extern_decl) in extern_declarations.iter().enumerate() {
            for func in &extern_decl.functions {
                // 如果函数有别名，使用别名作为映射键；否则使用原名
                let key = func.alias.as_ref().unwrap_or(&func.name).clone();
                self.extern_function_map.insert(key, decl_idx);
            }
        }
        self.extern_declarations = extern_declarations;
    }

    /// 检查函数是否是 extern 声明的（使用HashMap O(1)查找）
    /// 注意：如果extern函数有别名，只能通过别名找到
    pub fn is_extern_function(&self, func_name: &str) -> bool {
        self.extern_function_map.contains_key(func_name)
    }

    /// 获取 extern 函数的信息（使用HashMap O(1)查找）
    /// 注意：如果extern函数有别名，只能通过别名获取
    pub fn get_extern_function(&self, func_name: &str) -> Option<&crate::ast::ExternFunction> {
        self.extern_function_map
            .get(func_name)
            .and_then(|&decl_idx| {
                self.extern_declarations.get(decl_idx).and_then(|decl| {
                    // 查找匹配的函数：有别名的按别名匹配，没别名的按原名匹配
                    decl.functions.iter().find(|f| match &f.alias {
                        Some(alias) => alias == func_name,
                        None => f.name == func_name,
                    })
                })
            })
    }

    /// 获取extern函数的LLVM调用名（实际C函数名）
    /// 用于代码生成时生成正确的call指令
    pub fn get_extern_llvm_name(&self, func_name: &str) -> Option<String> {
        self.get_extern_function(func_name).map(|f| f.name.clone())
    }

    /// 检查是否是顶层函数
    pub fn is_top_level_function(&self, func_name: &str) -> bool {
        self.top_level_functions.iter().any(|f| f.name == func_name)
    }

    /// 获取顶层函数的类型
    /// 时间复杂度: O(n)，n为顶层函数数量
    pub fn get_top_level_function_type(&self, func_name: &str) -> crate::types::Type {
        if let Some(func) = self
            .top_level_functions
            .iter()
            .find(|f| f.name == func_name)
        {
            crate::types::Type::Function(Box::new(crate::types::FunctionType {
                params: func.params.iter().map(|p| p.param_type.clone()).collect(),
                return_type: Box::new(func.return_type.clone()),
                is_static: true,
                is_closure: false,
            }))
        } else {
            // 默认返回 int () 类型
            crate::types::Type::Function(Box::new(crate::types::FunctionType {
                params: vec![],
                return_type: Box::new(crate::types::Type::Int32),
                is_static: true,
                is_closure: false,
            }))
        }
    }

    /// 获取变量的Cavvy类型
    /// 时间复杂度: O(1)
    pub fn get_variable_type(&self, var_name: &str) -> Option<crate::types::Type> {
        // 首先检查局部变量
        if let Some(cay_type) = self.var_cay_types.get(var_name) {
            return Some(cay_type.clone());
        }
        // 然后检查作用域管理器
        if let Some(llvm_name) = self.scope_manager.get_llvm_name(var_name) {
            if let Some(cay_type) = self.var_cay_types.get(&llvm_name) {
                return Some(cay_type.clone());
            }
        }
        None
    }

    /// 检查extern声明是否已生成（基于函数签名）
    pub fn is_extern_emitted(&self, func_signature: &str) -> bool {
        self.emitted_externs.contains(func_signature)
    }

    /// 标记extern声明已生成
    pub fn mark_extern_emitted(&mut self, func_signature: String) {
        self.emitted_externs.insert(func_signature);
    }

    /// 检查是否是 Windows 目标平台
    pub fn is_windows_target(&self) -> bool {
        if let Some(config) = &self.platform_config {
            config.target_os == "windows"
        } else {
            self.target_triple.contains("windows") || self.target_triple.contains("mingw32")
        }
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

    /// 获取当前源位置
    /// 复杂度: O(1) 直接返回当前设置的源位置
    /// 注意：现在source_file已经通过set_source_from_loc从loc.file获取了正确的文件路径
    fn get_current_source_position(&self) -> (String, usize, usize) {
        (
            self.source_file.clone(),
            self.source_line,
            self.source_column,
        )
    }

    /// 发射一行代码到当前代码缓冲区
    pub fn emit_line(&mut self, line: &str) {
        // 添加源映射注释（如果启用且不是LLVM注释行）
        if self.enable_source_map && !line.trim().starts_with(';') && !line.trim().is_empty() {
            let (source_file, source_line, source_column) = self.get_current_source_position();
            if !source_file.is_empty() {
                self.code.push_str(&format!(
                    "; !source {}:{}:{}\n",
                    source_file, source_line, source_column
                ));
                self.current_ir_line += 1;
            }
        }

        // DWARF 调试信息: 为 define 行注入 !dbg 注解
        let actual_line = if self.debug_info && line.trim_start().starts_with("define ") {
            let trimmed = line.trim_start();
            if let Some(at_pos) = trimmed.find('@') {
                let after_at = &trimmed[at_pos + 1..];
                let func_name = if let Some(paren_pos) = after_at.find('(') {
                    after_at[..paren_pos].to_string()
                } else {
                    after_at.to_string()
                };

                let has_source = !self.source_file.is_empty();
                if has_source {
                    let sf = self.source_file.clone();
                    let sl = self.source_line;
                    let node_id = self.allocate_debug_subprogram(&func_name, &sf, sl);

                    if let Some(brace_pos) = line.rfind('{') {
                        let before_brace = &line[..brace_pos];
                        let after_brace = &line[brace_pos..];
                        format!("{}!dbg !{} {}", before_brace, node_id, after_brace)
                    } else {
                        line.to_string()
                    }
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        } else {
            line.to_string()
        };

        if !actual_line.is_empty() {
            self.code.push_str(&"  ".repeat(self.indent));
        }
        self.code.push_str(&actual_line);
        self.code.push('\n');
        self.current_ir_line += 1;
    }

    /// 发射代码但不添加缩进（用于全局声明）
    pub fn emit_raw(&mut self, line: &str) {
        // 添加源映射注释（如果启用且不是LLVM注释行）
        if self.enable_source_map && !line.trim().starts_with(';') && !line.trim().is_empty() {
            let (source_file, source_line, source_column) = self.get_current_source_position();
            if !source_file.is_empty() {
                self.output.push_str(&format!(
                    "; !source {}:{}:{}\n",
                    source_file, source_line, source_column
                ));
                self.current_ir_line += 1;
            }
        }

        // DWARF 调试信息: 为 define 行注入 !dbg 注解
        if self.debug_info && line.trim_start().starts_with("define ") {
            // 提取函数名: "define <type> @<name>(...)" -> "<name>"
            let trimmed = line.trim_start();
            if let Some(at_pos) = trimmed.find('@') {
                let after_at = &trimmed[at_pos + 1..];
                let func_name = if let Some(paren_pos) = after_at.find('(') {
                    after_at[..paren_pos].to_string()
                } else {
                    after_at.to_string()
                };

                // 查找该函数是否已分配子程序节点
                // 检查是否来自运行时（没有源文件位置），跳过
                let has_source = !self.source_file.is_empty();

                if has_source {
                    let sf = self.source_file.clone();
                    let sl = self.source_line;
                    let node_id = self.allocate_debug_subprogram(&func_name, &sf, sl);

                    // 在 { 之前注入 !dbg !N
                    if let Some(brace_pos) = line.rfind('{') {
                        let before_brace = &line[..brace_pos];
                        let after_brace = &line[brace_pos..];
                        self.output.push_str(&format!(
                            "{}!dbg !{} {}\n",
                            before_brace, node_id, after_brace
                        ));
                        self.current_ir_line += 1;
                        return;
                    }
                }
            }
        }

        self.output.push_str(line);
        self.output.push('\n');
        self.current_ir_line += 1;
    }

    /// 设置当前源位置
    pub fn set_source_position(&mut self, file: impl Into<String>, line: usize, column: usize) {
        self.source_file = file.into();
        self.source_line = line;
        self.source_column = column;
    }

    /// 从SourceLocation设置源位置
    /// 优先使用loc中的file字段，如果为None则使用传入的file参数
    /// 只有loc未携带原始文件时才回退到预处理器源映射
    pub fn set_source_from_loc(&mut self, loc: &crate::error::SourceLocation, file: &str) {
        if let Some(loc_file) = &loc.file {
            self.source_file = normalize_source_file_path(loc_file);
            self.source_line = loc.line;
            self.source_column = loc.column;
            return;
        }

        if let Some(ref source_map) = self.preprocessor_source_map {
            if let Some((original_file, original_line)) = source_map.get(&loc.line) {
                self.source_file = normalize_source_file_path(original_file);
                self.source_line = *original_line;
                self.source_column = loc.column;
                return;
            }
        }

        self.source_file = normalize_source_file_path(file);
        self.source_line = loc.line;
        self.source_column = loc.column;
    }

    /// 获取类型的 LLVM 对齐字节数
    pub fn get_type_align(&self, llvm_type: &str) -> u32 {
        match llvm_type {
            "i1" | "i8" => 1,
            "i16" => 2,
            "i32" | "float" => 4, // float 是 4 字节对齐！
            "i64" | "double" => 8,
            t if t.ends_with("*") => 8, // 所有指针都是 8 字节（64位系统）
            _ => 8,                     // 默认 8 字节
        }
    }

    /// 获取 LLVM 类型的大小（字节）
    pub fn get_type_size(&self, llvm_type: &str) -> i64 {
        match llvm_type {
            "i1" => 1,
            "i8" => 1,
            "i32" => 4,
            "i64" => 8,
            "float" => 4,
            "double" => 8,
            t if t.ends_with("*") => 8, // 所有指针都是 8 字节（64位系统）
            _ => 8,
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
    pub fn enter_loop(&mut self, cond_label: String, end_label: String, label: Option<String>) {
        self.loop_stack.push(LoopContext {
            cond_label,
            end_label,
            label,
        });
    }

    /// 退出循环上下文
    pub fn exit_loop(&mut self) {
        self.loop_stack.pop();
    }

    /// 获取当前循环上下文（用于 break/continue）
    pub fn current_loop(&self) -> Option<&LoopContext> {
        self.loop_stack.last()
    }

    /// 根据标签获取循环上下文（用于带标签的 break/continue）
    pub fn get_loop_by_label(&self, label: &str) -> Option<&LoopContext> {
        self.loop_stack
            .iter()
            .rev()
            .find(|ctx| ctx.label.as_deref() == Some(label))
    }

    /// 替换类型中的泛型参数为实际类型（使用 generic_type_args 映射）
    pub fn substitute_generic_params(&self, ty: crate::types::Type) -> crate::types::Type {
        use crate::types::Type;
        match ty {
            Type::GenericParam(name) => {
                if let Some(actual_type) = self.generic_type_args.get(&name) {
                    actual_type.clone()
                } else {
                    Type::GenericParam(name)
                }
            }
            Type::Array(inner) => {
                Type::Array(Box::new(self.substitute_generic_params(*inner)))
            }
            Type::Pointer(inner) => {
                Type::Pointer(Box::new(self.substitute_generic_params(*inner)))
            }
            Type::Function(func_type) => {
                let new_return = self.substitute_generic_params(*func_type.return_type);
                let new_params = func_type.params.into_iter().map(|p| {
                    self.substitute_generic_params(p)
                }).collect();
                Type::Function(Box::new(crate::types::FunctionType {
                    return_type: Box::new(new_return),
                    params: new_params,
                    is_static: func_type.is_static,
                    is_closure: func_type.is_closure,
                }))
            }
            Type::Generic(base, args) => {
                let new_args = args.into_iter().map(|a| self.substitute_generic_params(a)).collect();
                Type::Generic(base, new_args)
            }
            _ => ty,
        }
    }

    /// 获取表达式的类型
    /// 用于在代码生成期间推断表达式类型
    pub fn get_expression_type(&self, expr: &crate::ast::Expr) -> Option<crate::types::Type> {
        use crate::ast::*;
        use crate::types::Type;

        match expr {
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::Int32(_) => Some(Type::Int32),
                LiteralValue::Int64(_) => Some(Type::Int64),
                LiteralValue::Float32(_) => Some(Type::Float32),
                LiteralValue::Float64(_) => Some(Type::Float64),
                LiteralValue::String(_) => Some(Type::String),
                LiteralValue::Bool(_) => Some(Type::Bool),
                LiteralValue::Char(_) => Some(Type::Char),
                LiteralValue::Null => Some(Type::Object("Object".to_string())),
            },
            Expr::Identifier(name) => {
                // 首先从Cavvy类型映射中查找（更准确）
                if let Some(cay_type) = self.var_cay_types.get(name.as_ref()) {
                    return Some(cay_type.clone());
                }
                // 回退到LLVM类型映射
                if let Some(llvm_type) = self.var_types.get(name.as_ref()) {
                    Self::map_llvm_type_to_cay(llvm_type)
                } else {
                    None
                }
            }
            Expr::MemberAccess(member) => {
                // 对于成员访问，尝试获取对象的类型
                self.get_expression_type(&member.object)
                    .and_then(|obj_type| {
                        match obj_type {
                            Type::Array(_) if member.member == "length" => Some(Type::Int32),
                            Type::String if member.member == "length" => Some(Type::Int32),
                            Type::Object(class_name) => {
                                // 对于对象类型，从类型注册表查找字段类型
                                if let Some(ref registry) = self.type_registry {
                                    if let Some(class_info) = registry.get_class(&class_name) {
                                        // 查找字段
                                        if let Some(field_info) =
                                            class_info.fields.get(&member.member)
                                        {
                                            return Some(field_info.field_type.clone());
                                        }
                                    }
                                }
                                None
                            }
                            Type::Generic(class_name, type_args) => {
                                // 对于泛型类型（如 vector<Student>），从类型注册表查找字段类型
                                if let Some(ref registry) = self.type_registry {
                                    if let Some(class_info) = registry.get_class(&class_name) {
                                        // 查找字段
                                        if let Some(field_info) =
                                            class_info.fields.get(&member.member)
                                        {
                                            let mut field_type = field_info.field_type.clone();
                                            // 如果字段类型包含泛型参数，使用类型参数映射替换
                                            field_type = self.substitute_generic_params(field_type);
                                            return Some(field_type);
                                        }
                                    }
                                }
                                None
                            }
                            _ => None,
                        }
                    })
                    .or_else(|| {
                        // 如果无法从对象类型推断，尝试从当前类的字段信息获取
                        // 这用于处理 this.field 的情况
                        if let Expr::Identifier(obj_name) = member.object.as_ref() {
                            let obj_name_str = obj_name.as_ref();
                            // 检查是否是 this 或当前类实例
                            if obj_name_str == "this"
                                || (obj_name_str == self.current_class.as_str())
                            {
                                // 从类型注册表获取当前类的字段信息
                                if let Some(ref registry) = self.type_registry {
                                    if let Some(class_info) =
                                        registry.get_class(&self.current_class)
                                    {
                                        // 查找字段 (HashMap<String, FieldInfo>)
                                        if let Some(field_info) =
                                            class_info.fields.get(&member.member)
                                        {
                                            return Some(field_info.field_type.clone());
                                        }
                                    }
                                }
                            }
                        }
                        None
                    })
            }
            Expr::ArrayAccess(arr) => {
                // 数组访问返回元素类型
                self.get_expression_type(&arr.array)
                    .map(|arr_type| match arr_type {
                        Type::Array(elem) => (*elem).clone(),
                        _ => Type::Int32,
                    })
            }
            Expr::New(new_expr) => {
                // New 表达式返回对象类型
                Some(Type::Object(new_expr.class_name.clone()))
            }
            Expr::Call(call) => {
                // 对于函数调用，尝试推断返回类型
                self.infer_call_return_type(call)
            }
            _ => None,
        }
    }

    /// 将 LLVM 类型字符串映射到 Cavvy 类型
    fn map_llvm_type_to_cay(llvm_type: &str) -> Option<crate::types::Type> {
        use crate::types::Type;
        match llvm_type {
            "i32" => Some(Type::Int32),
            "i64" => Some(Type::Int64),
            "float" => Some(Type::Float32),
            "double" => Some(Type::Float64),
            "i1" => Some(Type::Bool),
            "i8" => Some(Type::Char),
            t if t == "i8*" || t == "%String*" => Some(Type::String),
            t if t.ends_with("*") => {
                // 可能是数组或对象指针
                if t.contains("[") {
                    Some(Type::Array(Box::new(Type::Int32)))
                } else {
                    Some(Type::Object(t.trim_end_matches('*').to_string()))
                }
            }
            _ => None,
        }
    }

    /// 获取或创建字符串常量
    pub fn get_or_create_string_constant(&mut self, s: &str) -> String {
        if let Some(name) = self.global_strings.get(s) {
            return name.clone();
        }

        let name = format!("@.str.{}", self.global_counter);
        self.global_counter += 1;

        // 转义字符串
        let escaped = s
            .replace("\\", "\\\\")
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
            let escaped = s
                .replace("\\", "\\5C")
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

    /// 获取带命名空间的类名（用于 LLVM IR 标识符）
    /// 使用 Itanium ABI 名称改编: _ZN<len>ns1<len>ns2<len>classNameE
    /// 泛型类型名中的 < > 会被替换为 _ 以生成合法的 LLVM 标识符
    /// 对于泛型类（如 FileResult<T>），使用原始泛型类名（FileResult_T_）
    pub(crate) fn get_qualified_class_name(&self, class_name: &str) -> String {
        // 提取基础类名（去除泛型参数）
        let base_name = if let Some(lt_pos) = class_name.find('<') {
            &class_name[..lt_pos]
        } else {
            class_name
        };

        // 检查是否是泛型类
        let processed_name = if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(base_name) {
                if !class_info.type_params.is_empty() {
                    // 这是泛型类，使用原始类型参数名（如 T）
                    let type_param_suffix = class_info.type_params.join("_");
                    format!("{}_{}_", base_name, type_param_suffix)
                } else {
                    // 不是泛型类，正常处理
                    if class_name.contains('<') {
                        class_name
                            .replace("<", "_")
                            .replace(">", "_")
                            .replace(",", "_")
                            .replace(" ", "_")
                    } else {
                        class_name.to_string()
                    }
                }
            } else {
                // 类不存在，正常处理
                if class_name.contains('<') {
                    class_name
                        .replace("<", "_")
                        .replace(">", "_")
                        .replace(",", "_")
                        .replace(" ", "_")
                } else {
                    class_name.to_string()
                }
            }
        } else {
            // 类型注册表不可用，正常处理
            if class_name.contains('<') {
                class_name
                    .replace("<", "_")
                    .replace(">", "_")
                    .replace(",", "_")
                    .replace(" ", "_")
            } else {
                class_name.to_string()
            }
        };

        // 如果 processed_name 已经包含 ::，直接从中提取命名空间和简单名
        if processed_name.contains("::") {
            let parts: Vec<&str> = processed_name.split("::").collect();
            let simple_name = parts.last().unwrap_or(&"").to_string();
            let namespace: Vec<String> = parts[..parts.len() - 1]
                .iter()
                .map(|s| s.to_string())
                .collect();
            if namespace.is_empty() {
                simple_name
            } else {
                self.mangle_namespace(&namespace, &simple_name)
            }
        } else {
            let namespace = self.get_class_namespace(&processed_name);
            // 使用 Itanium ABI 格式
            self.mangle_namespace(&namespace, &processed_name)
        }
    }

    /// 生成带参数签名的方法名以支持方法重载
    /// 格式: _ZN<len>ns1<len>ns2<len>classNameE.methodName 或 _ZN<len>ns1<len>ns2<len>classNameE.__methodName_paramTypes
    pub fn generate_method_name(
        &self,
        class_name: &str,
        method: &crate::ast::MethodDecl,
    ) -> String {
        let cls = self.get_qualified_class_name(class_name);

        // 对泛型特化版本，使用基础类名作为前缀（与调用端一致）
        let base_cls = if class_name.contains('<') {
            let base_name = class_name.split('<').next().unwrap_or(class_name);
            self.get_qualified_class_name(base_name)
        } else {
            cls.clone()
        };

        // 只对泛型类尝试从类型注册表获取方法信息
        // 因为类型注册表中的 MethodInfo 已经将泛型参数替换为 GenericParam 类型
        if class_name.contains('<') {
            if let Some(ref registry) = self.type_registry {
                // 提取基础类名（去除泛型参数）
                let base_class_name = if let Some(pos) = class_name.find('<') {
                    &class_name[..pos]
                } else {
                    class_name
                };

                if let Some(class_info) = registry.get_class(base_class_name) {
                    if !class_info.type_params.is_empty() {
                        if let Some(methods) = class_info.methods.get(&method.name) {
                            // 找到参数数量匹配的方法
                            for method_info in methods {
                                if method_info.params.len() == method.params.len() {
                                    // 使用 MethodInfo 中的参数类型（已替换泛型参数）
                                    if method_info.params.is_empty() {
                                        return format!("{}.{}", base_cls, method.name);
                                    } else {
                                        let param_types: Vec<String> = method_info
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
                                        return format!(
                                            "{}.__{}_{}",
                                            base_cls,
                                            method.name,
                                            param_types.join("_")
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // 回退：使用 AST 中的参数类型
        if method.params.is_empty() {
            format!("{}.{}", base_cls, method.name)
        } else {
            let param_types: Vec<String> = method
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
            format!("{}.__{}_{}", base_cls, method.name, param_types.join("_"))
        }
    }

    /// 获取类的命名空间路径
    pub(crate) fn get_class_namespace(&self, class_name: &str) -> Vec<String> {
        // 提取基础类名（移除泛型参数）
        // 例如: "Optional_T_" -> "Optional", "std::Optional_T_" -> "std::Optional"
        let base_class_name = if let Some(pos) = class_name.find('_') {
            // 检查是否是泛型参数后缀（如 _T_）
            if class_name[pos..].starts_with("_T") || class_name[pos..].starts_with("_") {
                &class_name[..pos]
            } else {
                class_name
            }
        } else {
            class_name
        };

        // 直接查找（限定名 key）
        if let Some(ns) = self.class_namespaces.get(base_class_name) {
            return ns.clone();
        }
        // 检查 using 别名
        if let Some(ref registry) = self.type_registry {
            if let Some(qualified) = registry.namespace_aliases.get(base_class_name) {
                if let Some(ns) = self.class_namespaces.get(qualified) {
                    return ns.clone();
                }
            }
        }
        // 回退：在当前命名空间上下文中查找
        if let Some(ref registry) = self.type_registry {
            if !registry.current_namespace.is_empty() {
                let qualified = format!(
                    "{}::{}",
                    registry.current_namespace.join("::"),
                    base_class_name
                );
                if let Some(ns) = self.class_namespaces.get(&qualified) {
                    return ns.clone();
                }
            }
        }
        // 注意：不在全局回退查找其他命名空间中的类
        // 必须通过 using 声明或限定名显式引用
        Vec::new()
    }

    /// Itanium ABI 名称改编: _ZN<len>ns1<len>ns2<len>classNameE
    pub(crate) fn mangle_namespace(&self, namespace_path: &[String], name: &str) -> String {
        let mut result = "_ZN".to_string();
        for part in namespace_path {
            result.push_str(&format!("{}{}", part.len(), part));
        }
        result.push_str(&format!("{}{}E", name.len(), name));
        result
    }

    /// Mangle 变量名以确保是合法的 LLVM 标识符
    /// 将 < > , 空格等非法字符替换为 _
    /// 时间复杂度: O(n)，n为名称长度
    /// 空间复杂度: O(n)，返回新字符串
    pub fn mangle_var_name(&self, name: &str) -> String {
        name.replace("<", "_")
            .replace(">", "_")
            .replace(",", "_")
            .replace(" ", "_")
            .replace("::", "_")
    }

    /// 将可变参数类型转换为签名
    /// 可变参数在内部表示为 Array(ElementType)，需要提取元素类型
    fn varargs_type_to_signature(&self, ty: &crate::types::Type) -> String {
        use crate::types::Type;
        // 可变参数类型是 Array(ElementType)，提取元素类型
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
            _ => self.type_to_signature(ty), // 如果不是数组类型，回退到普通签名
        }
    }

    /// 从类型注册表查找构造函数的真实参数类型签名
    /// 当有多个构造函数参数数量相同时，通过类型匹配选择最合适的重载
    /// 如果类型注册表不可用或找不到匹配的构造函数，回退到 fallback_types
    pub fn get_constructor_param_signatures(
        &self,
        class_name: &str,
        arg_count: usize,
        fallback_types: &[String],
    ) -> Vec<String> {
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                // 收集所有参数数量匹配的构造函数
                let candidates: Vec<_> = class_info
                    .constructors
                    .iter()
                    .filter(|c| c.params.len() == arg_count)
                    .collect();

                if candidates.is_empty() {
                    return fallback_types.to_vec();
                }

                // 如果只有一个候选，直接使用
                if candidates.len() == 1 {
                    return candidates[0]
                        .params
                        .iter()
                        .map(|p| self.type_to_signature(&p.param_type))
                        .collect();
                }

                // 多个候选：通过类型匹配评分选择最佳重载
                // 评分规则：完全匹配 +10，整数族 +3，对象族 +5，String 不匹配 -100
                let best = candidates
                    .iter()
                    .max_by_key(|ctor| {
                        let ctor_sigs: Vec<String> = ctor
                            .params
                            .iter()
                            .map(|p| self.type_to_signature(&p.param_type))
                            .collect();
                        let mut score: i32 = 0;
                        for (c_sig, f_sig) in ctor_sigs.iter().zip(fallback_types.iter()) {
                            if c_sig == f_sig {
                                score += 10; // exact match
                            } else if Self::is_int_signature(c_sig) && Self::is_int_signature(f_sig)
                            {
                                score += 3; // int family (e.g., i32→i64 widening)
                            } else if Self::is_float_signature(c_sig)
                                && Self::is_float_signature(f_sig)
                            {
                                score += 3; // float family
                            } else if c_sig.starts_with('o') && f_sig.starts_with('o') {
                                score += if c_sig == f_sig { 5 } else { 1 };
                            } else if (c_sig == "s") != (f_sig == "s") {
                                score -= 100; // String vs non-String is a bad match
                            }
                        }
                        score
                    })
                    .unwrap_or(&candidates[0]);

                return best
                    .params
                    .iter()
                    .map(|p| self.type_to_signature(&p.param_type))
                    .collect();
            }
        }
        fallback_types.to_vec()
    }

    /// 检查签名是否是整数类型（i8, i16, i32, i64 等，但非指针）
    fn is_int_signature(sig: &str) -> bool {
        sig.starts_with('i') && !sig.contains('*') && sig != "i8"
    }

    /// 检查签名是否是浮点类型（f, d 或 float, double）
    fn is_float_signature(sig: &str) -> bool {
        sig == "f" || sig == "d"
    }

    /// 将类型转换为方法签名的一部分
    pub fn type_to_signature(&self, ty: &crate::types::Type) -> String {
        use crate::types::Type;
        // 首先解析类型别名
        let resolved_ty = self.resolve_type(ty);
        match &resolved_ty {
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
            Type::Function(func_type) => {
                // 生成完整的函数指针签名: fn_<return>_<param1>_<param2>_...
                let mut sig = "fn".to_string();
                sig.push_str(&self.type_to_signature(&func_type.return_type));
                for param in &func_type.params {
                    sig.push_str("_");
                    sig.push_str(&self.type_to_signature(param));
                }
                sig
            }
            Type::Auto => "x".to_string(), // 不应到达此处，语义分析应已解析
            // FFI 类型签名
            Type::CInt => "ci".to_string(),
            Type::CUInt => "cui".to_string(),
            Type::CLong => "cl".to_string(),
            Type::CULong => "cul".to_string(),
            Type::CShort => "cs".to_string(),
            Type::CUShort => "cus".to_string(),
            Type::CChar => "cc".to_string(),
            Type::CUChar => "cuc".to_string(),
            Type::CFloat => "cf".to_string(),
            Type::CDouble => "cd".to_string(),
            Type::SizeT => "sz".to_string(),
            Type::SSizeT => "ssz".to_string(),
            Type::UIntPtr => "up".to_string(),
            Type::IntPtr => "ip".to_string(),
            Type::CVoid => "cv".to_string(),
            Type::CBool => "cb".to_string(),
            // FFI 指针和结构体
            Type::Pointer(inner) => format!("p{}", self.type_to_signature(inner)),
            Type::Struct(name) => format!("st{}", name),
            // 泛型类型 - 使用类型参数的字符串表示
            Type::GenericParam(name) => format!("g{}", name),
            Type::Generic(name, args) => {
                let mut sig = format!("G{}", name);
                for arg in args {
                    sig.push_str(&self.type_to_signature(arg));
                }
                sig
            }
        }
    }

    /// 生成顶层函数名称
    /// 格式: __toplevel_function_name
    pub fn generate_top_level_function_name(&self, name: &str) -> String {
        format!("__toplevel_{}", name)
    }

    /// 注册类型标识符
    pub fn register_type_id(
        &mut self,
        class_name: &str,
        parent_name: Option<&str>,
        interfaces: Vec<String>,
    ) -> String {
        let llvm_name = self.get_qualified_class_name(class_name);
        let type_id = format!("@__type_id_{}", llvm_name);
        let parent_type_id =
            parent_name.map(|p| format!("@__type_id_{}", self.get_qualified_class_name(p)));
        let type_id_value = self.type_id_counter as i32;
        self.type_id_counter += 1;

        self.type_id_map.insert(
            class_name.to_string(),
            TypeIdInfo {
                class_name: class_name.to_string(),
                parent_type_id,
                interfaces,
                type_id_value,
            },
        );

        type_id
    }

    /// 获取类型的整数标识符值
    pub fn get_type_id_value(&self, class_name: &str) -> Option<i32> {
        self.type_id_map
            .get(class_name)
            .map(|info| info.type_id_value)
    }

    /// 获取类型标识符
    pub fn get_type_id(&self, class_name: &str) -> Option<String> {
        let llvm_name = self.get_qualified_class_name(class_name);
        self.type_id_map
            .get(class_name)
            .map(|_| format!("@__type_id_{}", llvm_name))
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
            let llvm_name = self.get_qualified_class_name(class_name);
            let type_id_name = format!("@__type_id_{}", llvm_name);
            result.push_str(&format!(
                "{} = private constant i32 {}, align 4\n",
                type_id_name, info.type_id_value
            ));
        }
        result
    }

    /// 计算类的实例布局（支持继承）
    ///
    /// 对象内存布局: [type_id: i32][padding: i32][父类字段...][子类字段...]
    /// 返回对象总大小（字节）
    pub fn compute_class_layout(
        &mut self,
        class_name: &str,
        fields: &[crate::ast::FieldDecl],
        parent_name: Option<&str>,
    ) -> usize {
        // 对象头大小：type_id (4 bytes) + padding (4 bytes) + vtable_ptr (8 bytes) = 16 bytes
        // vtable 指针偏移量
        const VTABLE_OFFSET: usize = 8;
        const HEADER_SIZE: usize = 16;
        let mut current_offset = HEADER_SIZE;
        let mut field_map = HashMap::new();

        // 如果有父类，先复制父类的字段布局
        if let Some(parent) = parent_name {
            // 先用简单名找，找不到就用限定名（class_layouts 现在用限定名存储）
            let parent_layout = self
                .class_layouts
                .get(parent)
                .or_else(|| {
                    self.type_registry
                        .as_ref()
                        .and_then(|r| r.find_qualified_class(parent))
                        .and_then(|q| self.class_layouts.get(&q))
                })
                .cloned();
            if let Some(parent_layout) = parent_layout {
                // 复制父类的所有字段信息
                for (field_name, field_info) in &parent_layout.fields {
                    field_map.insert(field_name.clone(), field_info.clone());
                }
                // 从父类布局的结束位置开始
                current_offset = parent_layout.total_size;
            }
        }

        for field in fields {
            // 跳过静态字段
            if field.modifiers.contains(&crate::ast::Modifier::Static) {
                continue;
            }

            let llvm_type = self.type_to_llvm(&field.field_type);
            let size = field.field_type.size_in_bytes();

            // 对齐处理
            let align = self.get_type_align(&llvm_type) as usize;
            current_offset = (current_offset + align - 1) & !(align - 1);

            let field_info = InstanceFieldInfo {
                name: field.name.clone(),
                llvm_type: llvm_type.clone(),
                field_type: field.field_type.clone(),
                offset: current_offset,
                size,
            };

            field_map.insert(field.name.clone(), field_info);
            current_offset += size;
        }

        // 最终对齐到 8 字节边界
        let total_size = (current_offset + 7) & !7;

        let layout = ClassLayoutInfo {
            class_name: class_name.to_string(),
            total_size,
            fields: field_map,
        };

        self.class_layouts.insert(class_name.to_string(), layout);
        total_size
    }

    /// 获取类布局信息
    pub fn get_class_layout(&self, class_name: &str) -> Option<&ClassLayoutInfo> {
        // 直接用传入的类名查找
        if let Some(layout) = self.class_layouts.get(class_name) {
            return Some(layout);
        }
        // 简单名找不到，尝试用限定名（class_layouts 键为 "ns::ClassName"）
        if let Some(ref registry) = self.type_registry {
            if let Some(qname) = registry.find_qualified_class(class_name) {
                return self.class_layouts.get(&qname);
            }
            // struct 也按相同方式查找
            let struct_qname = if let Some(s) = registry.get_struct(class_name) {
                Some(s.name.clone())
            } else {
                None
            };
            if let Some(ref qname) = struct_qname {
                if let Some(layout) = self.class_layouts.get(qname) {
                    return Some(layout);
                }
            }
        }
        None
    }

    /// 获取实例字段信息
    pub fn get_instance_field(
        &self,
        class_name: &str,
        field_name: &str,
    ) -> Option<&InstanceFieldInfo> {
        // 先用传入的类名直接查找
        if let Some(layout) = self.class_layouts.get(class_name) {
            if let Some(result) = layout.fields.get(field_name) {
                return Some(result);
            }
        }
        // 简单名找不到，尝试用限定名（class_layouts 键为 "ns::ClassName"）
        if let Some(ref registry) = self.type_registry {
            if let Some(qname) = registry.find_qualified_class(class_name) {
                return self.class_layouts.get(&qname)?.fields.get(field_name);
            }
            // struct 也按相同方式查找
            if let Some(s) = registry.get_struct(class_name) {
                if let Some(layout) = self.class_layouts.get(&s.name) {
                    return layout.fields.get(field_name);
                }
            }
        }
        None
    }

    /// 获取类的父类名
    pub fn get_parent_class(&self, class_name: &str) -> Option<String> {
        if let Some(registry) = &self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                return class_info.parent.clone();
            }
        }
        None
    }

    /// 设置平台配置
    pub fn set_platform_config(&mut self, config: &crate::CompilerOptions) {
        let platform_config = PlatformConfig {
            target_os: config.target_os.clone(),
            features: config.features.clone(),
            no_features: config.no_features.clone(),
            defines: config.defines.clone(),
            undefines: config.undefines.clone(),
            obfuscate: config.obfuscate,
        };
        self.platform_config = Some(platform_config);
    }

    /// 获取平台配置
    pub fn get_platform_config(&self) -> Option<&PlatformConfig> {
        self.platform_config.as_ref()
    }

    /// 生成平台特定的运行时声明
    pub fn generate_platform_declarations(&self) -> String {
        if let Some(config) = &self.platform_config {
            config.generate_platform_declarations()
        } else {
            String::new()
        }
    }

    /// 生成平台特定的初始化代码
    pub fn generate_platform_init(&self) -> String {
        if let Some(config) = &self.platform_config {
            config.generate_platform_init()
        } else {
            String::new()
        }
    }

    /// 生成调用约定属性组定义
    pub fn generate_calling_convention_attributes(&self) -> String {
        let mut attrs = String::new();
        attrs.push_str("; Calling convention attributes\n");
        attrs.push_str("attributes #0 = { \"cdecl\" }\n");
        attrs.push_str("attributes #1 = { \"stdcall\" }\n");
        attrs.push_str("attributes #2 = { \"fastcall\" }\n");
        attrs.push_str("attributes #3 = { \"sysv64\" }\n");
        attrs.push_str("attributes #4 = { \"win64\" }\n");
        attrs.push('\n');
        attrs
    }

    // ============================================================
    // IR Builder Bridge 支持方法
    // ============================================================

    /// 获取当前作用域的所有变量
    ///
    /// 用于IR Builder Bridge收集变量信息
    pub fn get_all_scope_vars(&self) -> Vec<(String, &VarScope)> {
        self.scope_manager.get_all_visible_vars()
    }

    /// 获取变量的Cavvy类型
    pub fn get_var_cay_type(&self, name: &str) -> Option<crate::types::Type> {
        self.var_cay_types.get(name).cloned()
    }

    /// 获取CodeGen的当前函数名
    pub fn get_current_function(&self) -> &str {
        &self.current_function
    }

    /// 获取CodeGen的当前类名
    pub fn get_current_class(&self) -> &str {
        &self.current_class
    }

    /// 将当前简单类名解析为限定名（如 "HttpHeaders" → "http::HttpHeaders"）
    pub fn resolve_current_qualified_class(&self) -> String {
        if let Some(ref registry) = self.type_registry {
            if let Some(qname) = registry.find_qualified_class(&self.current_class) {
                return qname;
            }
        }
        self.current_class.clone()
    }

    /// 获取当前函数的参数顺序（用于内联IR）
    pub fn get_current_param_order(&self) -> &[String] {
        &self.current_param_order
    }

    // ============================================================
    // DWARF 调试信息支持
    // ============================================================

    /// 启用 DWARF 调试信息生成
    pub fn enable_debug_info(&mut self) {
        self.debug_info = true;
    }

    /// 启用测试模式
    ///
    /// 在测试模式下，代码生成器会额外生成 `__cavvy_test_main` 入口函数，
    /// 自动调用所有带 `@Test` 注解的方法，并打印测试结果。
    pub fn enable_test_mode(&mut self) {
        self.test_mode = true;
    }

    /// 为函数定义分配 DWARF 子程序元数据节点
    /// 返回用于 `!dbg !N` 注解的节点编号
    pub fn allocate_debug_subprogram(
        &mut self,
        func_name: &str,
        source_file: &str,
        source_line: usize,
    ) -> usize {
        let subprogram_node = self.debug_node_counter;
        let type_node = self.debug_node_counter + 1;
        self.debug_node_counter += 2;

        self.debug_subprograms.push(DebugSubprogram {
            func_name: func_name.to_string(),
            source_file: source_file.to_string(),
            source_line,
            node_id: subprogram_node,
            type_node_id: type_node,
        });

        subprogram_node
    }

    /// 发射 DWARF 模块级引用（在 emit_header 中 target triple 之后调用）
    pub fn emit_debug_header(&mut self) {
        if !self.debug_info {
            return;
        }
        // 模块级 DWARF 引用 — 节点定义在末尾的 emit_debug_metadata 中
        self.output.push_str(&format!(
            "!llvm.dbg.cu = !{{!{}}}\n",
            self.debug_file_node - 3 // !0 = DICompileUnit
        ));
        self.output.push_str("!llvm.module.flags = !{!1, !2}\n");
        self.output.push('\n');
    }

    /// 发射所有 DWARF 调试元数据节点（在 IR 生成最后调用）
    pub fn emit_debug_metadata(&mut self) {
        if !self.debug_info {
            return;
        }

        let source_file = if self.source_file.is_empty() {
            "unknown.cay"
        } else {
            &self.source_file
        };

        // 转义路径中的反斜杠（Windows 路径等）
        let escaped_file = source_file.replace('\\', "\\\\");

        // !0 = distinct !DICompileUnit
        self.output.push_str(&format!(
            "!0 = distinct !DICompileUnit(language: DW_LANG_C_plus_plus, file: !{}, producer: \"Cavvy Compiler\", isOptimized: false, runtimeVersion: 0, emissionKind: FullDebug, enums: !{}, splitDebugInlining: false)\n",
            self.debug_file_node, self.debug_empty_node
        ));

        // !1 = Debug Info Version
        self.output
            .push_str("!1 = !{i32 2, !\"Debug Info Version\", i32 3}\n");

        // !2 = Dwarf Version
        self.output
            .push_str("!2 = !{i32 2, !\"Dwarf Version\", i32 4}\n");

        // !3 = !DIFile
        self.output.push_str(&format!(
            "!{} = !DIFile(filename: \"{}\", directory: \".\")\n",
            self.debug_file_node, escaped_file
        ));

        // !4 = empty tuple
        self.output
            .push_str(&format!("!{} = !{{}}\n", self.debug_empty_node));
        self.output.push('\n');

        // 发射所有子程序元数据
        let cu_node = self.debug_file_node - 3; // !0

        for sp in &self.debug_subprograms {
            let escaped_sp_file = sp.source_file.replace('\\', "\\\\");
            let sp_file_node = if sp.source_file == source_file {
                self.debug_file_node
            } else {
                // 对于来自不同文件的函数，使用相同的 DIFile（简化处理）
                self.debug_file_node
            };

            // DISubroutineType
            self.output.push_str(&format!(
                "!{} = !DISubroutineType(types: !{})\n",
                sp.type_node_id, self.debug_empty_node
            ));

            // DISubprogram
            self.output.push_str(&format!(
                "!{} = distinct !DISubprogram(name: \"{}\", linkageName: \"{}\", scope: !{}, file: !{}, line: {}, type: !{}, scopeLine: {}, spFlags: DISPFlagDefinition, unit: !{})\n",
                sp.node_id, sp.func_name, sp.func_name,
                cu_node, sp_file_node, sp.source_line,
                sp.type_node_id, sp.source_line, cu_node
            ));
        }
    }
}
