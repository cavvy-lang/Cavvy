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

/// Struct 布局信息（值类型，无对象头）
#[derive(Debug, Clone)]
pub struct StructLayoutInfo {
    pub struct_name: String,
    pub total_size: usize,
    pub fields: HashMap<String, InstanceFieldInfo>,
    pub field_order: Vec<String>, // 字段定义顺序，用于 LLVM GEP 索引
    pub llvm_type_def: String,    // LLVM 类型定义: %struct.Point = type { i32, i32 }
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

/// 析构候选：登记一个在作用域退出时需要自动调用 `@ClassName.__dtor` 的局部变量。
///
/// ROADMAP 5.3.x 自动 RAII：当局部变量的类型是带析构函数的类（或泛型特化类）
/// 时，作用域正常退出前需逆序调用其 `__dtor`。提前 return/break/continue 时
/// 也需先于跳转触发本层及外层（至函数/循环边界）的析构。
#[derive(Debug, Clone)]
pub struct DtorCandidate {
    /// 变量名（Cavvy 源码名，用于查 llvm_name）
    pub var_name: String,
    /// 类名（普通类用基础名；泛型特化类用特化名如 "std::UniquePtr<int>"）
    pub class_name: String,
    /// 该变量的 LLVM alloca 名（直接持有对象指针 i8*）
    pub llvm_name: String,
}

/// 作用域栈管理
pub struct ScopeManager {
    scopes: Vec<HashMap<String, VarScope>>, // 作用域栈
    scope_counter: usize,                   // 作用域计数器（用于生成唯一名称）
    /// 每层作用域的析构候选（与 scopes 索引对齐）。
    /// 仅局部 VarDecl 登记；参数与 this 不登记。
    dtor_candidates: Vec<Vec<DtorCandidate>>,
}

impl ScopeManager {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()], // 全局作用域
            scope_counter: 0,
            dtor_candidates: vec![Vec::new()],
        }
    }

    /// 进入新作用域
    pub fn enter_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.dtor_candidates.push(Vec::new());
        self.scope_counter += 1;
    }

    /// 退出当前作用域
    pub fn exit_scope(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
            self.dtor_candidates.pop();
        }
    }

    /// 登记一个析构候选到当前作用域（仅对带析构函数的类类型局部变量调用）
    pub fn register_dtor_candidate(&mut self, candidate: DtorCandidate) {
        if let Some(scope) = self.dtor_candidates.last_mut() {
            scope.push(candidate);
        }
    }

    /// 取出当前作用域的析构候选（逆序返回，符合 C++ 后构造先析构语义）。
    /// 取出后清空当前作用域候选，避免重复析构。
    pub fn drain_current_scope_dtors(&mut self) -> Vec<DtorCandidate> {
        if let Some(scope) = self.dtor_candidates.last_mut() {
            let mut v = std::mem::take(scope);
            v.reverse();
            v
        } else {
            Vec::new()
        }
    }

    /// 取出所有作用域的析构候选，从内层到外层，每层内部逆序。
    /// 用于 return 等提前退出路径：一次性析构所有尚未退出的作用域。
    pub fn drain_all_scope_dtors(&mut self) -> Vec<DtorCandidate> {
        let mut result = Vec::new();
        for scope in self.dtor_candidates.iter_mut().rev() {
            let mut v = std::mem::take(scope);
            v.reverse();
            result.extend(v);
        }
        result
    }

    /// 当前作用域是否有析构候选
    pub fn current_scope_has_dtors(&self) -> bool {
        self.dtor_candidates
            .last()
            .map_or(false, |s| !s.is_empty())
    }

    /// 按变量名移除析构候选（从内层作用域到外层搜索），返回被移除的候选。
    ///
    /// 用于 `return local_var;` 场景：返回的局部对象所有权转移给调用者，
    /// 不应在当前函数末尾析构它。
    pub fn remove_dtor_candidate_by_var_name(
        &mut self,
        var_name: &str,
    ) -> Option<DtorCandidate> {
        for scope in self.dtor_candidates.iter_mut().rev() {
            if let Some(pos) = scope.iter().position(|c| c.var_name == var_name) {
                return Some(scope.remove(pos));
            }
        }
        None
    }

    /// 当前作用域栈深度（用于记录 return/break/continue 时的边界）
    pub fn scope_depth(&self) -> usize {
        self.scopes.len()
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
        self.dtor_candidates.clear();
        self.dtor_candidates.push(Vec::new());
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
    /// 当前正在生成的方法所属类的特化布局键（含命名空间与类型实参），
    /// 如 "std::Optional<double>"。仅当处于泛型特化方法体内时为 `Some`。
    /// `current_class` 被裁剪为裸类名（用于参数命名），无法定位特化布局；
    /// 此字段保留完整特化名，使 `this`/隐式字段访问能解析到已单态化的字段类型。
    pub current_class_specialized: Option<String>,
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
    pub struct_layouts: HashMap<String, StructLayoutInfo>, // struct 值类型布局信息
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
    // DWARF 调试信息增强（per-instruction 级别）
    pub debug_scope_stack: Vec<usize>,               // 当前调试作用域栈（DISubprogram/DILexicalBlock）
    debug_locations: Vec<DebugLocation>,              // DILocation 节点列表
    debug_lexical_blocks: Vec<DebugLexicalBlock>,     // DILexicalBlock 节点列表
    debug_variables: Vec<DebugVariable>,              // DILocalVariable 节点列表
    debug_location_cache: HashMap<(usize, usize, usize), usize>, // (line, col, scope) -> node_id
    // 测试模式
    pub test_mode: bool,                     // 是否生成测试入口
    pub test_methods: Vec<(String, String)>, // (类名, 方法名) 列表
    pub field_initializers: HashMap<String, Vec<crate::ast::FieldDecl>>, // 类名 -> 有初始化器的字段列表
    pub lambda_captures: HashMap<String, Vec<(String, crate::types::Type)>>, // lambda函数名 -> 捕获变量列表 [(变量名, 类型)]
    pub lambda_envs: HashMap<String, String>, // lambda变量名 -> 环境指针临时变量名
    pub lambda_counter: usize,                // Lambda函数名计数器，确保唯一性
    // 泛型特化：当前类型参数映射（如 {"T" -> Type::Int32}）
    pub generic_type_args: HashMap<String, crate::types::Type>,
    // 泛型特化：new 表达式的期望目标类型（如变量声明 `Box<int> b = new Box(42)`
    // 中的 `Box<int>`），用于将无显式类型参数的 new 单态化到具体特化版本。
    pub pending_new_expected_type: Option<crate::types::Type>,
    // 泛型特化：已收集的特化实例（基础类名 -> 实例集合）
    pub specializations:
        HashMap<String, HashSet<crate::codegen::specialization::SpecializationInstance>>,
    // 泛型特化：已生成的特化方法名（避免重复生成）
    pub generated_specializations: HashSet<String>,
    // 已生成的 vtable 全局常量（避免重复生成）
    pub generated_vtables: HashSet<String>,
    // 已生成的方法定义（避免重复生成）
    pub generated_methods: HashSet<String>,
    // 代码生成阶段收集的警告（使用 RefCell 允许在 &self 方法中修改）
    pub warnings: std::cell::RefCell<Vec<crate::miette_diagnostic::CayError>>,
    // 类定义缓存（用于显式特化查找原始类）
    pub classes_cache: std::collections::HashMap<String, crate::ast::ClassDecl>,
    // struct 定义缓存（用于泛型 struct 单态化查找原始定义）
    pub structs_cache: std::collections::HashMap<String, crate::ast::StructDecl>,
    // 显式特化类型组合记录（基础类名 -> 特化类型参数列表集合）
    pub explicit_specializations:
        std::collections::HashMap<String, std::collections::HashSet<String>>,
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

/// DILocation 元数据节点（指令级别的源位置）
#[derive(Debug, Clone)]
struct DebugLocation {
    line: usize,
    column: usize,
    scope_node_id: usize, // DISubprogram 或 DILexicalBlock
    node_id: usize,
}

/// DILexicalBlock 元数据节点（作用域嵌套信息）
#[derive(Debug, Clone)]
struct DebugLexicalBlock {
    parent_scope_id: usize,
    file_node_id: usize,
    line: usize,
    column: usize,
    node_id: usize,
}

/// DILocalVariable 元数据节点（变量调试信息）
#[derive(Debug, Clone)]
struct DebugVariable {
    name: String,
    scope_node_id: usize,
    file_node_id: usize,
    line: usize,
    node_id: usize,
    arg: Option<usize>, // 参数编号（函数参数时使用）
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
            current_class_specialized: None,
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
            struct_layouts: HashMap::new(),
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
            debug_scope_stack: Vec::new(),
            debug_locations: Vec::new(),
            debug_lexical_blocks: Vec::new(),
            debug_variables: Vec::new(),
            debug_location_cache: HashMap::new(),
            test_mode: false,
            test_methods: Vec::new(),
            field_initializers: HashMap::new(),
            lambda_captures: HashMap::new(),
            lambda_envs: HashMap::new(),
            lambda_counter: 0,
            generic_type_args: HashMap::new(),
            pending_new_expected_type: None,
            specializations: HashMap::new(),
            generated_specializations: HashSet::new(),
            generated_vtables: HashSet::new(),
            generated_methods: HashSet::new(),
            warnings: std::cell::RefCell::new(Vec::new()),
            classes_cache: std::collections::HashMap::new(),
            structs_cache: std::collections::HashMap::new(),
            explicit_specializations: std::collections::HashMap::new(),
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

    /// 获取extern函数的LLVM返回类型（用户声明优先，否则返回默认值）
    /// 通用方法，不特化任何函数
    pub fn get_extern_ret_type(&self, func_name: &str, default: &str) -> String {
        self.get_extern_function(func_name)
            .map(|f| self.type_to_llvm(&f.return_type))
            .unwrap_or_else(|| default.to_string())
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

    /// 确保 `declare void @free(i8*)` 在 IR 中出现一次。
    /// ROADMAP 5.3.x 智能指针注入需要调用 free，但用户代码未必引用它。
    pub fn ensure_free_declared(&mut self) {
        if !self.code.contains("declare void @free(i8*)") {
            // 在第一个 define 之前插入声明；如果还没有 define，直接追加。
            let decl = "declare void @free(i8*)\n";
            if let Some(pos) = self.code.find("define ") {
                self.code.insert_str(pos, decl);
            } else {
                self.code.push_str(decl);
            }
        }
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

        // DWARF 调试信息: 处理指令级别的 !dbg 注解
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
                    // 将 DISubprogram 压入作用域栈，后续指令的 DILocation 引用它
                    self.debug_scope_stack.push(node_id);

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
        } else if self.debug_info && !self.debug_scope_stack.is_empty() {
            // 为指令行附加 DILocation 节点引用
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(';') || trimmed == "}" || trimmed.ends_with(':')
            {
                line.to_string()
            } else {
                let line_num = self.source_line;
                let col = self.source_column;
                let scope_id = *self.debug_scope_stack.last().unwrap();
                let cache_key = (line_num, col, scope_id);
                let loc_node_id = if let Some(&id) = self.debug_location_cache.get(&cache_key) {
                    id
                } else {
                    let id = self.debug_node_counter;
                    self.debug_node_counter += 1;
                    self.debug_locations.push(DebugLocation {
                        line: line_num,
                        column: col,
                        scope_node_id: scope_id,
                        node_id: id,
                    });
                    self.debug_location_cache.insert(cache_key, id);
                    id
                };
                format!("{}, !dbg !{}", line.trim_end(), loc_node_id)
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

        // DWARF: 闭合函数时弹出作用域
        if self.debug_info && line.trim() == "}" && !self.debug_scope_stack.is_empty() {
            self.debug_scope_stack.pop();
        }
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

        // DWARF 调试信息: 处理指令级别的 !dbg 注解
        let annotated_line = if self.debug_info && line.trim_start().starts_with("define ") {
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
                    self.debug_scope_stack.push(node_id);

                    if let Some(brace_pos) = line.rfind('{') {
                        let before_brace = &line[..brace_pos];
                        let after_brace = &line[brace_pos..];
                        self.output.push_str(&format!(
                            "{}!dbg !{} {}\n",
                            before_brace, node_id, after_brace
                        ));
                        self.current_ir_line += 1;
                        return;
                    } else {
                        line.to_string()
                    }
                } else {
                    line.to_string()
                }
            } else {
                line.to_string()
            }
        } else if self.debug_info && !self.debug_scope_stack.is_empty() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with(';') || trimmed == "}" || trimmed.ends_with(':')
            {
                line.to_string()
            } else {
                let line_num = self.source_line;
                let col = self.source_column;
                let scope_id = *self.debug_scope_stack.last().unwrap();
                let cache_key = (line_num, col, scope_id);
                let loc_node_id = if let Some(&id) = self.debug_location_cache.get(&cache_key) {
                    id
                } else {
                    let id = self.debug_node_counter;
                    self.debug_node_counter += 1;
                    self.debug_locations.push(DebugLocation {
                        line: line_num,
                        column: col,
                        scope_node_id: scope_id,
                        node_id: id,
                    });
                    self.debug_location_cache.insert(cache_key, id);
                    id
                };
                format!("{}, !dbg !{}", line.trim_end(), loc_node_id)
            }
        } else {
            line.to_string()
        };

        self.output.push_str(&annotated_line);
        self.output.push('\n');
        self.current_ir_line += 1;

        // DWARF: 闭合函数时弹出作用域
        if self.debug_info && line.trim() == "}" && !self.debug_scope_stack.is_empty() {
            self.debug_scope_stack.pop();
        }
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
    pub fn set_source_from_loc(
        &mut self,
        loc: &crate::miette_diagnostic::SourceLocation,
        file: &str,
    ) {
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

    /// ROADMAP 5.3.x 自动 RAII：作用域正常退出时为带析构函数的局部变量调用 `__dtor`。
    ///
    /// 在当前基本块尚未被终止指令结束时直接追加析构调用。若块已终止（如末尾
    /// return），不追加也不 drain——析构候选留给 `emit_all_scope_dtors` 在 return
    /// 路径统一处理，避免在 `ret` 之后生成指令。
    pub fn emit_scope_exit_dtors(&mut self) {
        if !self.scope_manager.current_scope_has_dtors() {
            return;
        }
        // 若当前基本块已终止，不能再追加指令；保留候选给 return 路径处理。
        if self.current_block_terminated() {
            return;
        }
        let candidates = self.scope_manager.drain_current_scope_dtors();
        self.emit_dtor_candidates(&candidates);
    }

    /// ROADMAP 5.3.x 自动 RAII：为所有尚未退出的作用域（内层优先）调用 `__dtor`。
    ///
    /// 用于 return 语句或函数默认返回前：把从当前内层到函数边界的所有析构候选
    /// 按 C++ 后构造先析构顺序全部调用，并清空候选。
    pub fn emit_all_scope_dtors(&mut self) {
        let candidates = self.scope_manager.drain_all_scope_dtors();
        self.emit_dtor_candidates(&candidates);
    }

    /// 为一组析构候选发射 `load` + `call @_ZN...D1Ev` 指令。
    fn emit_dtor_candidates(&mut self, candidates: &[DtorCandidate]) {
        for cand in candidates {
            // 局部类实例变量的 alloca 存储的是对象指针 i8*。
            // 加载该指针并调用 Itanium ABI 析构函数名。
            let dtor_fn = self.mangle_itanium_method(&cand.class_name, "D1", &[], false, true);
            let obj_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = load i8*, i8** %{}",
                obj_temp, cand.llvm_name
            ));
            self.emit_line(&format!("  call void @{}(i8* {})", dtor_fn, obj_temp));
        }
    }

    /// 判断当前已发射代码的最后一个非空、非注释行是否是 LLVM 终止指令。
    /// 复用 if_stmt.rs 的终止检测模式。
    pub fn current_block_terminated(&self) -> bool {
        let lines: Vec<&str> = self.code.trim().lines().collect();
        if let Some(last) = lines.last() {
            let t = last.trim();
            // 跳过标签行与空行
            if t.is_empty() || t.ends_with(':') {
                return false;
            }
            return t.starts_with("ret")
                || t.starts_with("br ")
                || t.starts_with("switch")
                || t.starts_with("unreachable");
        }
        false
    }

    /// ROADMAP 5.3.x 自动 RAII：判断给定 Cavvy 类型是否是带析构函数的类，
    /// 若是则把局部变量登记为析构候选。
    ///
    /// - `Type::Object(name)`：查 type_registry 该类 has_destructor。
    /// - `Type::Generic(name, args)`：若全部具体，查基础类 has_destructor，
    ///   并用特化名（如 "std::UniquePtr<int>"）登记，以便阶段 3 生成的特化
    ///   `__dtor` 被正确调用。
    ///
    /// 仅由局部 VarDecl 调用；参数与 this 不调用此方法。
    pub fn register_dtor_candidate_if_applicable(
        &mut self,
        var_name: &str,
        llvm_name: &str,
        cay_type: &crate::types::Type,
    ) {
        use crate::types::Type;
        let (base_class, specialized_name): (Option<String>, Option<String>) = match cay_type {
            Type::Object(name) => (Some(name.clone()), Some(name.clone())),
            Type::Generic(name, args) => {
                let resolved: Vec<Type> =
                    args.iter().map(|t| self.resolve_type_arg_concrete(t)).collect();
                let all_concrete =
                    !resolved.is_empty() && resolved.iter().all(|t| self.type_arg_is_concrete(t));
                if all_concrete {
                    let args_str: Vec<String> = resolved.iter().map(|t| t.display_name()).collect();
                    (
                        Some(name.clone()),
                        Some(format!("{}<{ }>", name, args_str.join(", "))),
                    )
                } else {
                    (None, None) // 类型参数未全部具体，无法生成特化 __dtor，跳过
                }
            }
            _ => (None, None),
        };

        let (base_class, spec_name) = match (base_class, specialized_name) {
            (Some(b), Some(s)) => (b, s),
            _ => return,
        };

        // 查基础类是否声明了析构函数（has_destructor）。注意：基础类名可能含
        // 命名空间（如 "std::UniquePtr"）；registry.get_class 会处理。
        let has_dtor = self
            .type_registry
            .as_ref()
            .and_then(|r| r.get_class(&base_class))
            .map_or(false, |c| c.has_destructor);
        if !has_dtor {
            return;
        }

        self.scope_manager.register_dtor_candidate(DtorCandidate {
            var_name: var_name.to_string(),
            class_name: spec_name.clone(),
            llvm_name: llvm_name.to_string(),
        });
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
            Type::Array(inner) => Type::Array(Box::new(self.substitute_generic_params(*inner))),
            Type::Pointer(inner) => Type::Pointer(Box::new(self.substitute_generic_params(*inner))),
            Type::Function(func_type) => {
                let new_return = self.substitute_generic_params(*func_type.return_type);
                let new_params = func_type
                    .params
                    .into_iter()
                    .map(|p| self.substitute_generic_params(p))
                    .collect();
                Type::Function(Box::new(crate::types::FunctionType {
                    return_type: Box::new(new_return),
                    params: new_params,
                    is_static: func_type.is_static,
                    is_closure: func_type.is_closure,
                }))
            }
            Type::Generic(base, args) => {
                let new_args = args
                    .into_iter()
                    .map(|a| self.substitute_generic_params(a))
                    .collect();
                Type::Generic(base, new_args)
            }
            _ => ty,
        }
    }

    /// 将泛型类型实参经 `generic_type_args` 递归替换为具体类型。
    /// 兼容解析器为裸类型参数发出的 `Type::Object("T")` 与语义分析产生的
    /// `Type::GenericParam("T")` 两种表示。
    pub fn resolve_type_arg_concrete(&self, ty: &crate::types::Type) -> crate::types::Type {
        self.resolve_type_arg_concrete_depth(ty, 0)
    }

    fn resolve_type_arg_concrete_depth(
        &self,
        ty: &crate::types::Type,
        depth: usize,
    ) -> crate::types::Type {
        use crate::types::Type;
        // 深度上限防止自映射（如 T -> Object("T")）导致的无限递归
        if depth > 16 {
            return ty.clone();
        }
        match ty {
            Type::GenericParam(name) | Type::Object(name) => {
                if let Some(actual) = self.generic_type_args.get(name) {
                    // 若映射到自身则停止，避免死循环
                    let maps_to_self = matches!(actual,
                        Type::GenericParam(n) | Type::Object(n) if n == name);
                    if maps_to_self {
                        ty.clone()
                    } else {
                        self.resolve_type_arg_concrete_depth(actual, depth + 1)
                    }
                } else {
                    ty.clone()
                }
            }
            Type::Array(inner) => Type::Array(Box::new(
                self.resolve_type_arg_concrete_depth(inner, depth + 1),
            )),
            Type::Pointer(inner) => Type::Pointer(Box::new(
                self.resolve_type_arg_concrete_depth(inner, depth + 1),
            )),
            Type::Generic(base, args) => Type::Generic(
                base.clone(),
                args.iter()
                    .map(|a| self.resolve_type_arg_concrete_depth(a, depth + 1))
                    .collect(),
            ),
            _ => ty.clone(),
        }
    }

    /// 判断类型实参是否为具体类型（非未解析的泛型参数）。
    /// `GenericParam` 恒为非具体；`Object(name)` 仅当 `name` 是已注册的类时才算
    /// 具体（未注册的短名如 "T" 视为未解析参数）。
    pub fn type_arg_is_concrete(&self, ty: &crate::types::Type) -> bool {
        use crate::types::Type;
        match ty {
            Type::GenericParam(_) => false,
            Type::Object(name) => self
                .type_registry
                .as_ref()
                .map(|r| {
                    r.get_class(name).is_some()
                        || r.get_interface(name).is_some()
                        || r.get_struct(name).is_some()
                })
                .unwrap_or(false),
            Type::Array(inner) | Type::Pointer(inner) => self.type_arg_is_concrete(inner),
            Type::Generic(_, args) => args.iter().all(|a| self.type_arg_is_concrete(a)),
            _ => true,
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
                    // 单态化：变量声明类型可能是泛型参数（如方法形参 `K key`），
                    // 经当前特化上下文的 generic_type_args 解析为具体类型，
                    // 否则方法调用等下游逻辑会误将参数名当作类名。
                    return Some(self.resolve_type_arg_concrete(cay_type));
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
                                            let field_type = self.resolve_type_arg_concrete(
                                                &field_info.field_type,
                                            );
                                            return Some(field_type);
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
                                            field_type = self.resolve_type_arg_concrete(
                                                &field_type,
                                            );
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
                                            let field_type = self.resolve_type_arg_concrete(
                                                &field_info.field_type,
                                            );
                                            return Some(field_type);
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

    /// 5.3.0: 检查标识符是否是省略 new 的类实例化目标
    ///
    /// 返回 true 当且仅当 name 是已注册的类/struct/限定类名，
    /// 且不被局部变量、顶层函数或 extern 函数遮蔽。
    pub(crate) fn is_class_instantiation_target(&self, name: &str) -> bool {
        // 被局部变量遮蔽
        if self.get_variable_type(name).is_some()
            || self.scope_manager.get_var_type(name).is_some()
            || self.var_types.contains_key(name)
            || self.var_class_map.contains_key(name)
        {
            return false;
        }
        // 与顶层函数/extern函数同名时，保留函数调用语义
        if self.is_top_level_function(name) || self.is_extern_function(name) {
            return false;
        }
        self.type_registry.as_ref().map_or(false, |registry| {
            registry.class_exists(name)
                || registry.get_struct(name).is_some()
                || registry.find_qualified_class(name).is_some()
        })
    }

    /// 5.3.0: 尝试将标识符解析为省略 new 的类实例化类名
    pub(crate) fn try_resolve_class_instantiation(&self, name: &str) -> Option<String> {
        if self.is_class_instantiation_target(name) {
            Some(name.to_string())
        } else {
            None
        }
    }

    /// 5.3.0: 检查标识符是否是命名空间式静态类方法调用目标
    pub(crate) fn is_static_method_call_target(&self, name: &str) -> bool {
        if !name.contains("::") {
            return false;
        }
        let Some((class_prefix, method_name)) = name.rsplit_once("::") else {
            return false;
        };
        if class_prefix.is_empty() || method_name.is_empty() {
            return false;
        }
        // 被局部变量/函数遮蔽
        if self.get_variable_type(name).is_some()
            || self.scope_manager.get_var_type(name).is_some()
            || self.var_types.contains_key(name)
            || self.var_class_map.contains_key(name)
            || self.is_top_level_function(name)
            || self.is_extern_function(name)
        {
            return false;
        }
        let Some(registry) = self.type_registry.as_ref() else {
            return false;
        };
        let Some(class_info) = registry.get_class(class_prefix) else {
            return false;
        };
        class_info
            .methods
            .get(method_name)
            .map_or(false, |methods| methods.iter().any(|m| m.is_static))
    }

    /// 5.3.0: 尝试将形如 ClassName::methodName 的标识符解析为静态方法调用
    ///
    /// 返回 (类前缀, 方法名)。调用方需自行从类型注册表查找具体方法重载并推断返回类型。
    pub(crate) fn try_resolve_static_method_call(&self, name: &str) -> Option<(String, String)> {
        if !self.is_static_method_call_target(name) {
            return None;
        }
        let (class_prefix, method_name) = name.rsplit_once("::")?;
        Some((class_prefix.to_string(), method_name.to_string()))
    }

    /// 5.3.0: 推断命名空间式静态方法调用的返回类型
    ///
    /// 在可能的情况下根据实参类型选择最匹配的重载。
    pub(crate) fn infer_static_method_call_return_type(
        &self,
        name: &str,
        call_args: &[crate::ast::Expr],
    ) -> Option<crate::types::Type> {
        let (class_prefix, method_name) = self.try_resolve_static_method_call(name)?;
        let registry = self.type_registry.as_ref()?;
        let class_info = registry.get_class(&class_prefix)?;
        let methods = class_info.methods.get(&method_name)?;
        let static_methods: Vec<_> = methods.iter().filter(|m| m.is_static).collect();
        if static_methods.is_empty() {
            return None;
        }

        // 推断实参类型以选择重载
        let mut arg_types = Vec::new();
        let mut resolved = true;
        for arg in call_args {
            if let Some(t) = self.get_expression_type(arg) {
                arg_types.push(t);
            } else {
                resolved = false;
                break;
            }
        }

        let method_info = if resolved {
            registry
                .find_method(&class_prefix, &method_name, &arg_types)
                .filter(|m| m.is_static)
                .or_else(|| static_methods.first().copied())
        } else {
            static_methods.first().copied()
        }?;

        Some(method_info.return_type.clone())
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
                    if class_name.contains('<') {
                        // 这是特化版本（如 Box<int>），使用实际类型参数生成独立名称
                        class_name
                            .replace("<", "_")
                            .replace(">", "_")
                            .replace(",", "_")
                            .replace(" ", "_")
                    } else {
                        // 这是原始泛型类（如 Box），使用类型参数名（如 T）
                        let type_param_suffix = class_info
                            .type_params
                            .iter()
                            .map(|p| p.name.as_str())
                            .collect::<Vec<_>>()
                            .join("_");
                        format!("{}_{}_", base_name, type_param_suffix)
                    }
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

    /// 从特化类名（如 `std::vector<int>`）解析类型实参，并结合类定义中的类型形参与
    /// 默认值，构建类型参数映射 `{ "T" -> Type::Int32 }`。
    pub(crate) fn build_specialization_mapping(
        &self,
        class_name: &str,
        class_info: &crate::types::ClassInfo,
    ) -> std::collections::HashMap<String, crate::types::Type> {
        let (_, type_arg_strs) = Self::parse_generic_args_from_name(class_name);
        // 使用 specialization 模块的 parse_type_str 以正确解析嵌套泛型类型
        // （如 "Rc<Tracked_>" 应解析为 Generic("Rc", [Object("Tracked_")])，
        // 而非整个字符串作为 Object）。
        let mut parsed_type_args: Vec<crate::types::Type> = type_arg_strs
            .iter()
            .map(|s| crate::codegen::specialization::parse_type_str(s))
            .collect();
        // 用默认值填充缺失的类型参数
        for (idx, param) in class_info.type_params.iter().enumerate() {
            if parsed_type_args.get(idx).is_none() {
                if let Some(default) = &param.default_type {
                    parsed_type_args.push(default.clone());
                } else {
                    parsed_type_args.push(crate::types::Type::GenericParam(param.name.clone()));
                }
            }
        }
        class_info
            .type_params
            .iter()
            .zip(parsed_type_args.iter())
            .map(|(p, t)| (p.name.clone(), t.clone()))
            .collect()
    }

    /// 使用标准 Itanium ABI 格式生成方法名，以便与 C++ 互操作。
    /// 格式: _ZN<ns><cls><method-len><method>E<itanium-params>
    pub fn generate_method_name(
        &self,
        class_name: &str,
        method: &crate::ast::MethodDecl,
    ) -> String {
        // 优先使用 AST 中已解析的参数类型。对于泛型特化类，调用方已经在生成特化代码前
        // 将方法参数中的泛型参数替换为具体类型，因此 method.params 中保存的是正确类型。
        let param_types: Vec<crate::types::Type> =
            method.params.iter().map(|p| p.param_type.clone()).collect();

        // 若参数中仍包含未替换的泛型参数，且当前类名为泛型特化形式，则回退到注册表
        // 查找基础类的方法签名，以保证旧代码路径的兼容性。
        // 回退时必须将注册表签名中的泛型参数按类名中的类型实参替换，否则会产生
        // `PPc` 等降级签名，与已生成的单态化方法不匹配。
        let needs_registry_fallback = class_name.contains('<')
            && param_types
                .iter()
                .any(|t| matches!(t, crate::types::Type::GenericParam(_)));
        if needs_registry_fallback {
            if let Some(ref registry) = self.type_registry {
                let base_class_name = if let Some(pos) = class_name.find('<') {
                    &class_name[..pos]
                } else {
                    class_name
                };
                if let Some(class_info) = registry.get_class(base_class_name) {
                    if !class_info.type_params.is_empty() {
                        if let Some(methods) = class_info.methods.get(&method.name) {
                            for method_info in methods {
                                if method_info.params.len() == method.params.len() {
                                    let mapping =
                                        self.build_specialization_mapping(class_name, class_info);
                                    let substituted: Vec<crate::types::Type> = method_info
                                        .params
                                        .iter()
                                        .map(|p| {
                                            crate::types::substitute_type_params(
                                                &p.param_type,
                                                &mapping,
                                            )
                                        })
                                        .collect();
                                    return self.mangle_itanium_method(
                                        class_name,
                                        &method.name,
                                        &substituted,
                                        false,
                                        false,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        self.mangle_itanium_method(class_name, &method.name, &param_types, false, false)
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

    /// 生成 Itanium ABI 类前缀（不含尾部 `E`），用于在类名之后继续编码方法名。
    /// 格式: _ZN<len>ns1<len>ns2<len>className
    /// 与标准 C++ 编译器输出一致。
    pub(crate) fn get_itanium_class_prefix(&self, class_name: &str) -> String {
        let mut result = "_ZN".to_string();
        // 如果 class_name 已经包含 :: 则直接使用，否则查 class_namespaces
        let parts = if class_name.contains("::") {
            let v: Vec<String> = class_name
                .split("::")
                .map(|s| self.mangle_itanium_entity_name(s))
                .collect();
            v
        } else {
            let base_name = if let Some(lt_pos) = class_name.find('<') {
                &class_name[..lt_pos]
            } else {
                class_name
            };
            let ns = self.get_class_namespace(base_name);
            let simple = if class_name.contains('<') {
                // 特化名：将 <, >, ,, 空格 替换为 _ 生成唯一标识
                self.mangle_itanium_entity_name(class_name)
            } else {
                class_name.to_string()
            };
            if ns.is_empty() {
                vec![simple]
            } else {
                let mut v: Vec<String> = ns.into_iter().collect();
                v.push(simple);
                v
            }
        };
        for part in &parts {
            result.push_str(&format!("{}{}", part.len(), part));
        }
        result
    }

    /// Itanium ABI 类型编码：将 Cavvy type 转换为 Itanium mangled 类型字符串。
    /// https://itanium-cxx-abi.github.io/cxx-abi/abi.html#mangling-type
    pub fn type_to_itanium_sig(&self, ty: &crate::types::Type) -> String {
        use crate::types::Type;
        match ty {
            Type::Void => "v".to_string(),
            Type::Int32 => "i".to_string(),
            Type::Int64 => "x".to_string(), // long long
            Type::Float32 => "f".to_string(),
            Type::Float64 => "d".to_string(),
            Type::Bool => "b".to_string(),
            Type::String => "Pc".to_string(), // char*
            Type::Char => "c".to_string(),
            Type::Object(name) => {
                // 当作 class/struct 指针: P<qualified name>
                let prefix = self.get_itanium_class_prefix(name);
                format!("P{}E", &prefix[3..]) // strip _ZN prefix, use P + name + E
            }
            Type::Array(inner) => {
                // 当作指针 (数组退化为指针)
                format!("P{}", self.type_to_itanium_sig(inner))
            }
            Type::Function(_) => "PFvvE".to_string(), // 简化函数指针
            Type::Auto => "v".to_string(),
            Type::CInt => "i".to_string(),
            Type::CUInt => "j".to_string(), // unsigned int
            Type::CLong => {
                if self.is_windows_target() { "i".to_string() } else { "l".to_string() }
            }
            Type::CULong => {
                if self.is_windows_target() { "j".to_string() } else { "m".to_string() }
            }
            Type::CShort => "s".to_string(),
            Type::CUShort => "t".to_string(),
            Type::CChar => "c".to_string(),
            Type::CUChar => "h".to_string(),
            Type::CFloat => "f".to_string(),
            Type::CDouble => "d".to_string(),
            Type::SizeT => { if self.is_windows_target() { "y".to_string() } else { "m".to_string() } }
            Type::SSizeT => "x".to_string(),
            Type::UIntPtr => "y".to_string(),
            Type::IntPtr => "l".to_string(),
            Type::CVoid => "v".to_string(),
            Type::CBool => "b".to_string(),
            Type::Pointer(inner) => {
                format!("P{}", self.type_to_itanium_sig(inner))
            }
            Type::Struct(name) => {
                // struct 指针
                let prefix = self.get_itanium_class_prefix(name);
                format!("P{}E", &prefix[3..])
            }
            Type::GenericParam(name) => {
                // 泛型参数的降级表示
                format!("Pc") // 回退到 char*
            }
            Type::Generic(name, _args) => {
                // 特化泛型类指针
                let mangled_name = self.mangle_itanium_entity_name(name);
                format!("P{}{}", mangled_name.len(), mangled_name)
            }
        }
    }

    /// 将类名中的 Itanium-非法字符（<, >, ,, 空格, ::）替换为 _。
    /// 用于构建 LLVM 标识符时确保合法性，同时生成可识别的特化名称。
    fn mangle_itanium_entity_name(&self, name: &str) -> String {
        name.replace("::", "_")
            .replace("<", "_")
            .replace(">", "_")
            .replace(",", "_")
            .replace(" ", "_")
            .replace("*", "P")
            .replace("&", "R")
    }

    /// 生成 struct 在 LLVM IR 中的类型名。
    /// 将源程序中的 struct 名（可能包含泛型参数与命名空间，如 "Point<int>"、
    /// "std::Vec<int>"）转换为合法的 LLVM 标识符（如 "Point_int_"、
    /// "std__Vec_int_"），用于 %struct.Name 类型定义与引用。
    pub fn struct_llvm_type_name(&self, name: &str) -> String {
        name.replace("::", "__")
            .replace("<", "_")
            .replace(">", "_")
            .replace(",", "_")
            .replace(" ", "_")
    }

    /// 生成完整的 Itanium ABI mangled 方法名。
    ///
    /// 格式（符合 g++/clang 标准）:
    ///   _ZN<ns-lens><ns><cls-len><cls><method-len><method>E<param-types>
    ///
    /// 示例：`HHH::Helper::add(int, int)` →
    ///   _ZN3HHH6Helper3addEii
    pub fn mangle_itanium_method(
        &self,
        class_name: &str,
        method_name: &str,
        param_types: &[crate::types::Type],
        is_constructor: bool,
        is_destructor: bool,
    ) -> String {
        let prefix = self.get_itanium_class_prefix(class_name);

        let method_enc = if is_constructor {
            "C1".to_string() // C1 = complete object constructor
        } else if is_destructor {
            "D1".to_string() // D1 = complete object destructor
        } else {
            format!("{}{}", method_name.len(), method_name)
        };

        let params_enc: String = if is_destructor || param_types.is_empty() {
            "v".to_string() // 无参函数按 Itanium ABI 编码为 v（destructor 恒为 D1Ev)
        } else {
            param_types
                .iter()
                .map(|t| self.type_to_itanium_sig(t))
                .collect::<Vec<_>>()
                .join("")
        };

        format!("{}{}E{}", prefix, method_enc, params_enc)
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
    pub fn varargs_type_to_signature(&self, ty: &crate::types::Type) -> String {
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
                // 评分规则：完全匹配 +10，整数族 +3，对象族 +5，泛型参数通配 +2，String 不匹配 -100
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
                            } else if c_sig.starts_with('g') {
                                // 泛型参数 T 作为通配匹配，分值低于精确匹配但高于不匹配，
                                // 避免与完全不相关的重载出现平分时被错误地选到最后一个。
                                score += 2;
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
            } else if let Some(struct_info) = registry.get_struct(class_name) {
                // struct 构造函数重载解析
                let candidates: Vec<_> = struct_info
                    .constructors
                    .iter()
                    .filter(|c| c.params.len() == arg_count)
                    .collect();

                if candidates.is_empty() {
                    return fallback_types.to_vec();
                }

                if candidates.len() == 1 {
                    return candidates[0]
                        .params
                        .iter()
                        .map(|p| self.type_to_signature(&p.param_type))
                        .collect();
                }

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
                                score += 10;
                            } else if Self::is_int_signature(c_sig) && Self::is_int_signature(f_sig)
                            {
                                score += 3;
                            } else if Self::is_float_signature(c_sig)
                                && Self::is_float_signature(f_sig)
                            {
                                score += 3;
                            } else if c_sig.starts_with('o') && f_sig.starts_with('o') {
                                score += if c_sig == f_sig { 5 } else { 1 };
                            } else if c_sig.starts_with('g') {
                                score += 2;
                            } else if (c_sig == "s") != (f_sig == "s") {
                                score -= 100;
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

    /// 与 `get_constructor_param_signatures` 相同的重载解析逻辑，但返回真实的
    /// `Type` 列表而非签名字符串，供 Itanium ABI mangling（`mangle_itanium_method`）使用。
    ///
    /// 对泛型 class/struct 的构造函数，自动用类名中的类型实参（如 `Point<int>`）
    /// 替换形参中的类型参数，避免生成 `Pc` 等降级签名。
    pub fn get_constructor_param_types(
        &self,
        class_name: &str,
        arg_count: usize,
        fallback_types: &[String],
    ) -> Vec<crate::types::Type> {
        // 解析可能存在的泛型实参，用于替换构造函数形参中的类型参数。
        let (base_name, type_args) = Self::parse_generic_args_from_name(class_name);

        // 将类型实参字符串解析为内部 Type（与 new.rs 中 parse_type_arg_from_str 语义一致）。
        // 使用 specialization::parse_type_str 以正确解析嵌套泛型实参。
        let parsed_type_args: Vec<crate::types::Type> = type_args
            .iter()
            .map(|s| crate::codegen::specialization::parse_type_str(s))
            .collect();

        let substitute_params = |params: &[crate::types::ParameterInfo],
                                 type_params: &[crate::types::TypeParamInfo]|
         -> Vec<crate::types::Type> {
            let needs_substitution = params
                .iter()
                .any(|p| Self::type_contains_generic_param(&p.param_type));
            if !needs_substitution {
                return params.iter().map(|p| p.param_type.clone()).collect();
            }
            // 优先使用类名中的显式类型实参；若类名不含泛型参数，则回退到当前
            // generic_type_args 上下文（泛型方法体内 new 的情况）。
            let resolved_args: Vec<crate::types::Type> = if !parsed_type_args.is_empty()
                && parsed_type_args.len() == type_params.len()
            {
                parsed_type_args.clone()
            } else {
                type_params
                    .iter()
                    .map(|p| {
                        self.generic_type_args
                            .get(&p.name)
                            .cloned()
                            .unwrap_or(crate::types::Type::GenericParam(p.name.clone()))
                    })
                    .collect()
            };
            let mapping: std::collections::HashMap<String, crate::types::Type> = type_params
                .iter()
                .zip(resolved_args.iter())
                .map(|(p, t)| (p.name.clone(), t.clone()))
                .collect();
            params
                .iter()
                .map(|p| crate::types::substitute_type_params(&p.param_type, &mapping))
                .collect()
        };

        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(&base_name) {
                let candidates: Vec<_> = class_info
                    .constructors
                    .iter()
                    .filter(|c| c.params.len() == arg_count)
                    .collect();

                if candidates.is_empty() {
                    return Vec::new();
                }

                let type_params: Vec<crate::types::TypeParamInfo> =
                    class_info.type_params.clone();

                if candidates.len() == 1 {
                    return substitute_params(&candidates[0].params, &type_params);
                }

                let best = candidates
                    .iter()
                    .max_by_key(|ctor| {
                        let ctor_sigs: Vec<String> = substitute_params(&ctor.params, &type_params)
                            .iter()
                            .map(|t| self.type_to_signature(t))
                            .collect();
                        let mut score: i32 = 0;
                        for (c_sig, f_sig) in ctor_sigs.iter().zip(fallback_types.iter()) {
                            if c_sig == f_sig {
                                score += 10;
                            } else if Self::is_int_signature(c_sig) && Self::is_int_signature(f_sig)
                            {
                                score += 3;
                            } else if Self::is_float_signature(c_sig)
                                && Self::is_float_signature(f_sig)
                            {
                                score += 3;
                            } else if c_sig.starts_with('o') && f_sig.starts_with('o') {
                                score += if c_sig == f_sig { 5 } else { 1 };
                            } else if c_sig.starts_with('g') {
                                // 泛型参数 T 作为通配匹配，分值低于精确匹配但高于不匹配
                                score += 2;
                            } else if (c_sig == "s") != (f_sig == "s") {
                                score -= 100;
                            }
                        }
                        score
                    })
                    .unwrap_or(&candidates[0]);

                return substitute_params(&best.params, &type_params);
            } else if let Some(struct_info) = registry.get_struct(&base_name) {
                // struct 构造函数参数类型解析
                let candidates: Vec<_> = struct_info
                    .constructors
                    .iter()
                    .filter(|c| c.params.len() == arg_count)
                    .collect();

                if candidates.is_empty() {
                    return Vec::new();
                }

                let type_params: Vec<crate::types::TypeParamInfo> =
                    struct_info.type_params.clone();

                if candidates.len() == 1 {
                    return substitute_params(&candidates[0].params, &type_params);
                }

                let best = candidates
                    .iter()
                    .max_by_key(|ctor| {
                        let ctor_sigs: Vec<String> = substitute_params(&ctor.params, &type_params)
                            .iter()
                            .map(|t| self.type_to_signature(t))
                            .collect();
                        let mut score: i32 = 0;
                        for (c_sig, f_sig) in ctor_sigs.iter().zip(fallback_types.iter()) {
                            if c_sig == f_sig {
                                score += 10;
                            } else if Self::is_int_signature(c_sig) && Self::is_int_signature(f_sig)
                            {
                                score += 3;
                            } else if Self::is_float_signature(c_sig)
                                && Self::is_float_signature(f_sig)
                            {
                                score += 3;
                            } else if c_sig.starts_with('o') && f_sig.starts_with('o') {
                                score += if c_sig == f_sig { 5 } else { 1 };
                            } else if c_sig.starts_with('g') {
                                score += 2;
                            } else if (c_sig == "s") != (f_sig == "s") {
                                score -= 100;
                            }
                        }
                        score
                    })
                    .unwrap_or(&candidates[0]);

                return substitute_params(&best.params, &type_params);
            }
        }
        Vec::new()
    }

    /// 检查类型（或其内部元素）是否仍包含未替换的泛型参数。
    fn type_contains_generic_param(ty: &crate::types::Type) -> bool {
        use crate::types::Type;
        match ty {
            Type::GenericParam(_) => true,
            Type::Array(inner) | Type::Pointer(inner) => Self::type_contains_generic_param(inner),
            Type::Generic(_, args) => args.iter().any(|a| Self::type_contains_generic_param(a)),
            Type::Function(ft) => {
                ft.params.iter().any(|p| Self::type_contains_generic_param(p))
                    || Self::type_contains_generic_param(&ft.return_type)
            }
            _ => false,
        }
    }

    /// 从可能带泛型参数的类名中解析基础名与类型实参字符串。
    /// 例如 `Point<int>` -> (`Point`, vec!["int"])
    pub(crate) fn parse_generic_args_from_name(name: &str) -> (String, Vec<String>) {
        if let Some(lt_pos) = name.find('<') {
            let gt_pos = name.rfind('>').unwrap_or(name.len());
            if lt_pos < gt_pos {
                let base = name[..lt_pos].to_string();
                let args_str = &name[lt_pos + 1..gt_pos];
                let args = crate::codegen::specialization::split_top_level_type_args(args_str);
                return (base, args);
            }
        }
        (name.to_string(), Vec::new())
    }

    /// 简单类型字符串解析（仅用于构造函数形参替换）。
    /// 支持基本类型别名、数组后缀与裸对象名。
    pub(crate) fn parse_simple_type_str(s: &str) -> crate::types::Type {
        use crate::types::Type;
        let t = s.trim();
        if t.ends_with("[]") {
            return Type::Array(Box::new(Self::parse_simple_type_str(&t[..t.len() - 2])));
        }
        match t {
            "int" => Type::Int32,
            "long" => Type::Int64,
            "float" => Type::Float32,
            "double" => Type::Float64,
            "bool" | "boolean" => Type::Bool,
            "string" | "String" => Type::String,
            "char" => Type::Char,
            _ => Type::Object(t.to_string()),
        }
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
    ///
    /// 接口类型支持泛型实参，但类型 ID 只记录接口基础名用于运行时 instanceof 判断。
    pub fn register_type_id(
        &mut self,
        class_name: &str,
        parent_name: Option<&str>,
        interfaces: Vec<crate::types::Type>,
    ) -> String {
        let llvm_name = self.get_qualified_class_name(class_name);
        let type_id = format!("@__type_id_{}", llvm_name);
        let parent_type_id =
            parent_name.map(|p| format!("@__type_id_{}", self.get_qualified_class_name(p)));
        let type_id_value = self.type_id_counter as i32;
        self.type_id_counter += 1;

        // 从接口类型中提取基础名（如 Iterator<T> -> Iterator）
        let interface_names: Vec<String> = interfaces
            .iter()
            .map(|t| match t {
                crate::types::Type::Object(name) | crate::types::Type::Generic(name, _) => {
                    name.split('<').next().unwrap_or(name).to_string()
                }
                _ => format!("{}", t),
            })
            .collect();

        self.type_id_map.insert(
            class_name.to_string(),
            TypeIdInfo {
                class_name: class_name.to_string(),
                parent_type_id,
                interfaces: interface_names,
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
    ///
    /// C++ 互操作类（ClassInfo.is_interop）无 16 字节对象头，字段从 offset 0 起，
    /// 与普通 C++ 类布局一致。此类不继承 Cavvy 父类的对象头布局（互操作类必须是叶子类）。
    pub fn compute_class_layout(
        &mut self,
        class_name: &str,
        fields: &[crate::ast::FieldDecl],
        parent_name: Option<&str>,
    ) -> usize {
        // 对象头大小：type_id (4 bytes) + padding (4 bytes) + vtable_ptr (8 bytes) = 16 bytes
        const HEADER_SIZE: usize = 16;
        let is_interop = self
            .type_registry
            .as_ref()
            .and_then(|r| r.get_class(class_name))
            .map(|c| c.is_interop)
            .unwrap_or(false);
        let mut current_offset = if is_interop { 0 } else { HEADER_SIZE };
        let mut field_map = HashMap::new();

        // 如果有父类，先复制父类的字段布局
        // 互操作类不继承 Cavvy 父类布局（否则会把父类的 16 字节头带进来，破坏 C++ 兼容）
        if !is_interop {
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

    /// 计算 struct 的布局（值类型，无对象头，无继承）
    ///
    /// 内存布局: [字段1...][字段2...]（从 offset 0 开始）
    /// 同时生成 LLVM 类型定义: %struct.Name = type { field1_type, field2_type, ... }
    /// 返回 struct 总大小（字节）
    pub fn compute_struct_layout(
        &mut self,
        struct_name: &str,
        fields: &[crate::ast::FieldDecl],
    ) -> usize {
        let mut current_offset = 0usize;
        let mut field_map = HashMap::new();
        let mut llvm_field_types = Vec::new();
        let mut field_order = Vec::new();

        for field in fields {
            // 跳过静态字段（struct 中不应该有静态字段，但以防万一）
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
            llvm_field_types.push(llvm_type.clone());
            field_order.push(field.name.clone());
            current_offset += size;
        }

        // 最终对齐到 8 字节边界
        let total_size = (current_offset + 7) & !7;

        // 生成 LLVM 类型定义（struct 名可能含泛型参数，需转为合法 LLVM 标识符）
        let llvm_type_name = self.struct_llvm_type_name(struct_name);
        let llvm_type_def = if llvm_field_types.is_empty() {
            format!("%struct.{} = type {{ }}\n", llvm_type_name)
        } else {
            format!(
                "%struct.{} = type {{ {} }}\n",
                llvm_type_name,
                llvm_field_types.join(", ")
            )
        };

        let layout = StructLayoutInfo {
            struct_name: struct_name.to_string(),
            total_size,
            fields: field_map,
            field_order,
            llvm_type_def,
        };

        self.struct_layouts.insert(struct_name.to_string(), layout);
        // 同时以 LLVM 类型名（如 "Pair_int__int_"）建立别名，
        // 使从 %struct.X* 反解出的名字也能查到布局（值类型拷贝需要）。
        if llvm_type_name != struct_name {
            if let Some(layout) = self.struct_layouts.get(struct_name).cloned() {
                self.struct_layouts.insert(llvm_type_name, layout);
            }
        }
        total_size
    }

    /// 获取 struct 布局信息
    /// 支持泛型特化名（如 "Point<int>"）回退到基础名查找。
    pub fn get_struct_layout(&self, struct_name: &str) -> Option<&StructLayoutInfo> {
        // 直接用传入的 struct 名查找
        if let Some(layout) = self.struct_layouts.get(struct_name) {
            return Some(layout);
        }
        // 泛型特化：回退到基础 struct 名
        let base_name = struct_name.split('<').next().unwrap_or(struct_name);
        if base_name != struct_name {
            if let Some(layout) = self.struct_layouts.get(base_name) {
                return Some(layout);
            }
        }
        // 简单名找不到，尝试用限定名
        if let Some(ref registry) = self.type_registry {
            if let Some(s) = registry.get_struct(struct_name) {
                if let Some(layout) = self.struct_layouts.get(&s.name) {
                    return Some(layout);
                }
            }
            if base_name != struct_name {
                if let Some(s) = registry.get_struct(base_name) {
                    if let Some(layout) = self.struct_layouts.get(&s.name) {
                        return Some(layout);
                    }
                }
            }
        }
        None
    }

    fn qualify_generic_layout_key(&self, class_name: &str) -> Option<String> {
        let (base_name, generic_suffix) = if let Some(pos) = class_name.find('<') {
            (&class_name[..pos], &class_name[pos..])
        } else {
            (class_name, "")
        };

        if base_name.contains("::") {
            return None;
        }

        if let Some(ref registry) = self.type_registry {
            if let Some(qualified_base) = registry.find_qualified_class(base_name) {
                return Some(format!("{}{}", qualified_base, generic_suffix));
            }
        }

        for owner in [
            self.current_class_specialized.as_deref(),
            Some(self.current_class.as_str()),
        ]
        .into_iter()
        .flatten()
        {
            let owner_base = owner.find('<').map(|pos| &owner[..pos]).unwrap_or(owner);
            if let Some(ns_end) = owner_base.rfind("::") {
                let qualified_base = format!("{}::{}", &owner_base[..ns_end], base_name);
                let qualified_key = format!("{}{}", qualified_base, generic_suffix);
                if self.class_layouts.contains_key(&qualified_key)
                    || self.class_layouts.contains_key(&qualified_base)
                    || self
                        .type_registry
                        .as_ref()
                        .map_or(false, |registry| registry.class_exists(&qualified_base))
                {
                    return Some(qualified_key);
                }
            }
        }

        None
    }

    /// 生成所有 struct 的 LLVM 类型定义
    pub fn emit_struct_type_definitions(&self) -> String {
        let mut result = String::new();
        // 布局可能同时以源语言名和 LLVM 类型名注册（别名），按类型定义去重，
        // 避免同一 %struct.X 被重复定义。
        let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for layout in self.struct_layouts.values() {
            if emitted.insert(layout.llvm_type_def.as_str()) {
                result.push_str(&layout.llvm_type_def);
            }
        }
        if !result.is_empty() {
            result.push('\n');
        }
        result
    }

    /// 获取类布局信息
    pub fn get_class_layout(&self, class_name: &str) -> Option<&ClassLayoutInfo> {
        // 直接用传入的类名查找
        if let Some(layout) = self.class_layouts.get(class_name) {
            return Some(layout);
        }
        if let Some(qualified_key) = self.qualify_generic_layout_key(class_name) {
            if let Some(layout) = self.class_layouts.get(&qualified_key) {
                return Some(layout);
            }
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

    /// 获取实例字段信息（支持 class 和 struct）
    /// 返回用于 `this`/隐式字段访问的类布局键。
    ///
    /// 在泛型特化方法体内，`current_class` 被裁剪为裸类名（如 "Optional"），
    /// 其字段布局是类型擦除的（`value: i8*`）。此时应改用完整特化名
    /// （如 "std::Optional<double>"），从而解析到已单态化的字段类型
    /// （`value: double`），使 `return value;` 加载/返回正确的具体类型。
    pub fn this_field_class_name(&self) -> String {
        self.current_class_specialized
            .clone()
            .unwrap_or_else(|| self.current_class.clone())
    }

    pub fn get_instance_field(
        &self,
        class_name: &str,
        field_name: &str,
    ) -> Option<&InstanceFieldInfo> {
        // 先用传入的类名直接查找 class 布局
        if let Some(layout) = self.class_layouts.get(class_name) {
            if let Some(result) = layout.fields.get(field_name) {
                return Some(result);
            }
        }
        // 查找 struct 布局
        if let Some(layout) = self.struct_layouts.get(class_name) {
            if let Some(result) = layout.fields.get(field_name) {
                return Some(result);
            }
        }
        if let Some(qualified_key) = self.qualify_generic_layout_key(class_name) {
            if let Some(layout) = self.class_layouts.get(&qualified_key) {
                if let Some(result) = layout.fields.get(field_name) {
                    return Some(result);
                }
            }
        }
        // 简单名找不到，尝试用限定名（class_layouts 键为 "ns::ClassName"）
        if let Some(ref registry) = self.type_registry {
            if let Some(qname) = registry.find_qualified_class(class_name) {
                return self.class_layouts.get(&qname)?.fields.get(field_name);
            }
            // struct 也按相同方式查找
            if let Some(s) = registry.get_struct(class_name) {
                // 先查 struct_layouts
                if let Some(layout) = self.struct_layouts.get(&s.name) {
                    return layout.fields.get(field_name);
                }
                // 回退到 class_layouts（兼容旧代码）
                if let Some(layout) = self.class_layouts.get(&s.name) {
                    return layout.fields.get(field_name);
                }
            }
        }
        None
    }

    /// 检查给定名称是否是 struct 类型
    /// 支持泛型特化名（如 "Point<int>"）：先用完整名查找，再用基础名查找。
    pub fn is_struct_type(&self, name: &str) -> bool {
        if let Some(ref registry) = self.type_registry {
            if registry.get_struct(name).is_some() {
                return true;
            }
            // 泛型特化：基础 struct 名在 < 之前
            let base_name = name.split('<').next().unwrap_or(name);
            if registry.get_struct(base_name).is_some() {
                return true;
            }
        }
        false
    }

    /// 获取 struct 字段的 GEP 索引（字段在 struct 定义中的顺序）
    /// 时间复杂度: O(n)，n 为字段数量
    pub fn get_struct_field_index(&self, struct_name: &str, field_name: &str) -> usize {
        for key in [struct_name, struct_name.split('<').next().unwrap_or(struct_name)] {
            if let Some(layout) = self.struct_layouts.get(key) {
                for (idx, name) in layout.field_order.iter().enumerate() {
                    if name == field_name {
                        return idx;
                    }
                }
            }
            // 回退：从类型注册表获取字段顺序
            if let Some(ref registry) = self.type_registry {
                if let Some(struct_info) = registry.get_struct(key) {
                    for (idx, name) in struct_info.field_order.iter().enumerate() {
                        if name == field_name {
                            return idx;
                        }
                    }
                }
            }
        }
        0 // 默认返回 0
    }

    /// 生成 struct 深拷贝代码（通过 llvm.memcpy）
    /// 时间复杂度: O(1) IR 生成，运行时 O(size)
    /// 空间复杂度: O(1) 额外临时变量
    pub fn emit_struct_memcpy(&mut self, dest_ptr: &str, src_ptr: &str, struct_name: &str) {        if let Some(layout) = self.get_struct_layout(struct_name) {
            let size = layout.total_size;
            let llvm_type_name = self.struct_llvm_type_name(struct_name);
            let dest_i8 = self.new_temp();
            let src_i8 = self.new_temp();
            self.emit_line(&format!(
                "  {} = bitcast {}* {} to i8*",
                dest_i8,
                format!("%struct.{}", llvm_type_name),
                dest_ptr
            ));
            self.emit_line(&format!(
                "  {} = bitcast {}* {} to i8*",
                src_i8,
                format!("%struct.{}", llvm_type_name),
                src_ptr
            ));
            self.emit_line(&format!(
                "  call void @llvm.memcpy.p0i8.p0i8.i64(i8* {}, i8* {}, i64 {}, i1 false)",
                dest_i8, src_i8, size
            ));
        }
    }

    /// 生成 struct 值的堆拷贝：malloc 一块新内存并把 src_ptr 指向的值复制进去，
    /// 返回类型为 `%struct.X*` 的新指针（仅返回值名，不含类型前缀）。
    ///
    /// 用于值类型语义中 struct 值「逃逸」到持久存储的场景：
    /// 字段赋值、数组元素赋值、enum payload、return。栈上 alloca 的副本
    /// 会随函数返回失效，这些场景必须使用堆存储。
    pub fn emit_struct_heap_copy(&mut self, src_ptr: &str, struct_name: &str) -> Option<String> {
        let layout = self.get_struct_layout(struct_name)?;
        let size = layout.total_size;
        let llvm_type_name = self.struct_llvm_type_name(struct_name);

        // 确保 malloc 已声明
        if !self.is_extern_emitted("malloc@i8*@i64") {
            self.emit_raw("declare i8* @malloc(i64)");
            self.mark_extern_emitted("malloc@i8*@i64".to_string());
        }

        let raw = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @malloc(i64 {})", raw, size));
        let typed = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to %struct.{}*",
            typed, raw, llvm_type_name
        ));
        self.emit_struct_memcpy(&typed, src_ptr, struct_name);
        Some(typed)
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
            detect_cycles: config.detect_cycles,
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
        // 泛型特化方法体内，`this` 字段写入必须使用特化布局（字段类型已单态化），
        // 与读取侧（this_field_class_name）保持一致，避免读写偏移不一致。
        if let Some(ref specialized) = self.current_class_specialized {
            return specialized.clone();
        }
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

    /// 推送调试作用域节点（DISubprogram 或 DILexicalBlock）
    /// 后续指令的 DILocation 将以此节点为 scope
    pub fn push_debug_scope(&mut self, node_id: usize) {
        self.debug_scope_stack.push(node_id);
    }

    /// 弹出当前调试作用域
    pub fn pop_debug_scope(&mut self) -> Option<usize> {
        self.debug_scope_stack.pop()
    }

    /// 进入词法块作用域：创建 DILexicalBlock 并入栈
    pub fn enter_debug_lexical_block(&mut self, line: usize, column: usize) {
        if !self.debug_info {
            return;
        }
        let parent_scope_id = *self.debug_scope_stack.last().unwrap_or(&0);
        let node_id = self.debug_node_counter;
        self.debug_node_counter += 1;
        self.debug_lexical_blocks.push(DebugLexicalBlock {
            parent_scope_id,
            file_node_id: self.debug_file_node,
            line,
            column,
            node_id,
        });
        self.debug_scope_stack.push(node_id);
    }

    /// 退出词法块作用域
    pub fn exit_debug_lexical_block(&mut self) {
        if !self.debug_info {
            return;
        }
        self.debug_scope_stack.pop();
    }

    /// 为变量发射 dbg.declare 内建调用
    /// 使调试器能定位变量的栈上地址
    /// `arg_num`: 函数参数编号（1-indexed），None 表示非参数变量
    pub fn emit_dbg_declare(
        &mut self,
        var_name: &str,
        llvm_name: &str,
        llvm_type: &str,
        line: usize,
        arg_num: Option<usize>,
    ) {
        if !self.debug_info || self.debug_scope_stack.is_empty() {
            return;
        }
        let scope_id = *self.debug_scope_stack.last().unwrap();
        let var_node_id = self.debug_node_counter;
        self.debug_node_counter += 1;
        self.debug_variables.push(DebugVariable {
            name: var_name.to_string(),
            scope_node_id: scope_id,
            file_node_id: self.debug_file_node,
            line,
            node_id: var_node_id,
            arg: arg_num,
        });
        self.emit_line(&format!(
            "call void @llvm.dbg.declare(metadata {}* %{}, metadata !{}, metadata !DIExpression())",
            llvm_type, llvm_name, var_node_id
        ));
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

        // 发射 DILexicalBlock 节点（必须在 DILocation 之前）
        for lb in &self.debug_lexical_blocks {
            self.output.push_str(&format!(
                "!{} = !DILexicalBlock(scope: !{}, file: !{}, line: {}, column: {})\n",
                lb.node_id, lb.parent_scope_id, lb.file_node_id, lb.line, lb.column
            ));
        }
        if !self.debug_lexical_blocks.is_empty() {
            self.output.push('\n');
        }

        // 发射 DILocation 节点（指令级别的源位置映射）
        for loc in &self.debug_locations {
            self.output.push_str(&format!(
                "!{} = !DILocation(line: {}, column: {}, scope: !{})\n",
                loc.node_id, loc.line, loc.column, loc.scope_node_id
            ));
        }
        if !self.debug_locations.is_empty() {
            self.output.push('\n');
        }

        // 发射 DILocalVariable 节点（变量调试信息）
        if !self.debug_variables.is_empty() {
            // 分配一个通用 DIBasicType 占位节点
            let void_type_node = self.debug_node_counter;
            self.debug_node_counter += 1;
            self.output.push_str(&format!(
                "!{} = !DIBasicType(name: \"int\", size: 32, encoding: DW_ATE_signed)\n",
                void_type_node
            ));

            for dv in &self.debug_variables {
                let arg_str = if let Some(arg_num) = dv.arg {
                    format!(", arg: {}", arg_num)
                } else {
                    String::new()
                };
                self.output.push_str(&format!(
                    "!{} = !DILocalVariable(name: \"{}\", scope: !{}, file: !{}, line: {}, type: !{}{})\n",
                    dv.node_id, dv.name, dv.scope_node_id, dv.file_node_id, dv.line, void_type_node, arg_str
                ));
            }
            self.output.push('\n');
        }
    }
}

#[cfg(test)]
mod context_tests;
