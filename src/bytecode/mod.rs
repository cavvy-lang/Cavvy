pub mod constant_pool;
pub mod instructions;
pub mod jit;
pub mod linker;
pub mod obfuscator;
pub mod serializer;

use constant_pool::*;
use instructions::*;
use std::collections::HashMap;

/// Cavvy字节码文件魔数
pub const CAYBC_MAGIC: &[u8; 4] = b"CAY\x01";

/// 字节码文件版本
pub const CAYBC_VERSION_MAJOR: u16 = 0;
pub const CAYBC_VERSION_MINOR: u16 = 1;

/// Cavvy字节码模块
/// 这是Cavvy源代码编译后的中间表示形式
#[derive(Debug, Clone)]
pub struct BytecodeModule {
    /// 模块元数据
    pub header: ModuleHeader,
    /// 常量池
    pub constant_pool: ConstantPool,
    /// 类型定义（类、接口）
    pub type_definitions: Vec<TypeDefinition>,
    /// 函数定义
    pub functions: Vec<FunctionDefinition>,
    /// 全局变量
    pub global_variables: Vec<GlobalVariable>,
    /// 字符串表（用于混淆后的符号映射）
    pub string_table: Vec<String>,
    /// 元数据（调试信息、源码映射等）
    pub metadata: HashMap<String, Vec<u8>>,
}

/// 模块头部信息
#[derive(Debug, Clone)]
pub struct ModuleHeader {
    /// 模块名称
    pub name: String,
    /// 目标平台
    pub target_platform: String,
    /// 编译时间戳
    pub timestamp: u64,
    /// 是否已混淆
    pub obfuscated: bool,
    /// 所需的运行时版本
    pub runtime_version: (u16, u16),
    /// 依赖的外部库
    pub external_libs: Vec<String>,
}

/// 类型定义（类或接口）
#[derive(Debug, Clone)]
pub struct TypeDefinition {
    /// 类型名称索引（指向常量池）
    pub name_index: ConstantIndex,
    /// 父类名称索引（可选）
    pub parent_index: Option<ConstantIndex>,
    /// 实现的接口索引列表
    pub interface_indices: Vec<ConstantIndex>,
    /// 类型修饰符
    pub modifiers: TypeModifiers,
    /// 字段列表
    pub fields: Vec<FieldDefinition>,
    /// 方法列表
    pub methods: Vec<MethodDefinition>,
}

/// 类型修饰符
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeModifiers {
    pub is_public: bool,
    pub is_final: bool,
    pub is_abstract: bool,
    pub is_interface: bool,
}

/// 字段定义
#[derive(Debug, Clone)]
pub struct FieldDefinition {
    /// 字段名称索引
    pub name_index: ConstantIndex,
    /// 字段类型索引
    pub type_index: ConstantIndex,
    /// 字段修饰符
    pub modifiers: FieldModifiers,
    /// 初始值索引（可选，指向常量池）
    pub initial_value: Option<ConstantIndex>,
}

/// 字段修饰符
#[derive(Debug, Clone, Copy, Default)]
pub struct FieldModifiers {
    pub is_public: bool,
    pub is_private: bool,
    pub is_protected: bool,
    pub is_static: bool,
    pub is_final: bool,
}

/// 方法定义
#[derive(Debug, Clone)]
pub struct MethodDefinition {
    /// 方法名称索引
    pub name_index: ConstantIndex,
    /// 返回类型索引
    pub return_type_index: ConstantIndex,
    /// 参数类型索引列表
    pub param_type_indices: Vec<ConstantIndex>,
    /// 参数名称索引列表
    pub param_name_indices: Vec<ConstantIndex>,
    /// 方法修饰符
    pub modifiers: MethodModifiers,
    /// 方法体（字节码）
    pub body: Option<CodeBody>,
    /// 局部变量表大小
    pub max_locals: u16,
    /// 操作数栈最大深度
    pub max_stack: u16,
}

/// 方法修饰符
#[derive(Debug, Clone, Copy, Default)]
pub struct MethodModifiers {
    pub is_public: bool,
    pub is_private: bool,
    pub is_protected: bool,
    pub is_static: bool,
    pub is_final: bool,
    pub is_abstract: bool,
    pub is_native: bool,
    pub is_override: bool,
}

/// 代码体
#[derive(Debug, Clone)]
pub struct CodeBody {
    /// 字节码指令
    pub instructions: Vec<Instruction>,
    /// 异常处理表
    pub exception_table: Vec<ExceptionHandler>,
    /// 行号表（用于调试）
    pub line_number_table: Vec<LineNumberEntry>,
}

/// 异常处理器
#[derive(Debug, Clone)]
pub struct ExceptionHandler {
    /// try块起始指令偏移
    pub start_pc: u32,
    /// try块结束指令偏移
    pub end_pc: u32,
    /// 处理代码偏移
    pub handler_pc: u32,
    /// 捕获的异常类型索引
    pub catch_type: ConstantIndex,
}

/// 行号表条目
#[derive(Debug, Clone)]
pub struct LineNumberEntry {
    /// 指令偏移
    pub pc: u32,
    /// 源码行号
    pub line: u32,
}

/// 全局变量定义
#[derive(Debug, Clone)]
pub struct GlobalVariable {
    /// 变量名称索引
    pub name_index: ConstantIndex,
    /// 变量类型索引
    pub type_index: ConstantIndex,
    /// 修饰符
    pub modifiers: FieldModifiers,
    /// 初始值索引（可选）
    pub initial_value: Option<ConstantIndex>,
}

/// 独立函数定义（非类方法）
#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    /// 函数名称索引
    pub name_index: ConstantIndex,
    /// 返回类型索引
    pub return_type_index: ConstantIndex,
    /// 参数类型索引列表
    pub param_type_indices: Vec<ConstantIndex>,
    /// 参数名称索引列表
    pub param_name_indices: Vec<ConstantIndex>,
    /// 函数修饰符
    pub modifiers: MethodModifiers,
    /// 函数体
    pub body: CodeBody,
    /// 局部变量表大小
    pub max_locals: u16,
    /// 操作数栈最大深度
    pub max_stack: u16,
}

impl BytecodeModule {
    /// 创建新的字节码模块
    pub fn new(name: String, target_platform: String) -> Self {
        Self {
            header: ModuleHeader {
                name,
                target_platform,
                timestamp: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs(),
                obfuscated: false,
                runtime_version: (CAYBC_VERSION_MAJOR, CAYBC_VERSION_MINOR),
                external_libs: Vec::new(),
            },
            constant_pool: ConstantPool::new(),
            type_definitions: Vec::new(),
            functions: Vec::new(),
            global_variables: Vec::new(),
            string_table: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    /// 添加类型定义
    pub fn add_type_definition(&mut self, type_def: TypeDefinition) {
        self.type_definitions.push(type_def);
    }

    /// 添加函数定义
    pub fn add_function(&mut self, function: FunctionDefinition) {
        self.functions.push(function);
    }

    /// 添加全局变量
    pub fn add_global_variable(&mut self, var: GlobalVariable) {
        self.global_variables.push(var);
    }

    /// 查找函数
    pub fn find_function(&self, name: &str) -> Option<&FunctionDefinition> {
        self.functions.iter().find(|f| {
            self.constant_pool
                .get_string(f.name_index)
                .map(|s| s == name)
                .unwrap_or(false)
        })
    }

    /// 查找类型定义
    pub fn find_type(&self, name: &str) -> Option<&TypeDefinition> {
        self.type_definitions.iter().find(|t| {
            self.constant_pool
                .get_string(t.name_index)
                .map(|s| s == name)
                .unwrap_or(false)
        })
    }

    /// 添加外部库依赖
    pub fn add_external_lib(&mut self, lib_name: String) {
        if !self.header.external_libs.contains(&lib_name) {
            self.header.external_libs.push(lib_name);
        }
    }
}

impl Default for BytecodeModule {
    fn default() -> Self {
        Self::new("unnamed".to_string(), "unknown".to_string())
    }
}
