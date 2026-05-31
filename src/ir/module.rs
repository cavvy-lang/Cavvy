//! IR 模块
//!
//! 顶层 IR 结构，包含函数、全局变量、字符串常量和类型声明。

use super::function::IrFunction;
use super::types::IrType;
use super::value::IrValue;
use std::collections::HashMap;

/// 全局变量定义
#[derive(Debug, Clone)]
pub struct IrGlobal {
    pub name: String,
    pub ty: IrType,
    pub initializer: Option<IrValue>,
    pub is_constant: bool,
    pub linkage: IrGlobalLinkage,
}

/// 全局变量链接类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrGlobalLinkage {
    External,
    Internal,
    Private,
}

/// 字符串常量
#[derive(Debug, Clone)]
pub struct IrStringConstant {
    pub name: String,
    pub value: String,
}

/// 外部函数声明
#[derive(Debug, Clone)]
pub struct IrExternDecl {
    pub name: String,
    pub return_type: IrType,
    pub params: Vec<(String, IrType)>,
    pub calling_convention: Option<String>,
    pub is_varargs: bool,
}

/// IR 模块
///
/// 代表编译单元的顶层 IR，是编译的中间产出。
/// 可以从 .cay 源文件生成，也可以序列化为 .cayir 文件。
#[derive(Debug, Clone)]
pub struct IrModule {
    /// 模块名称
    pub name: String,
    /// 目标平台三元组
    pub target_triple: String,
    /// 函数列表
    pub functions: Vec<IrFunction>,
    /// 外部函数声明
    pub extern_declarations: Vec<IrExternDecl>,
    /// 全局变量
    pub globals: Vec<IrGlobal>,
    /// 字符串常量映射 (name -> content)
    pub string_constants: HashMap<String, String>,
    /// 类型声明（结构体等）
    pub type_declarations: Vec<IrTypeDecl>,
    /// 字符串计数器
    string_counter: u64,
    /// 全局计数器（用于生成唯一名称）
    global_counter: u64,
}

/// 类型声明（结构体定义）
#[derive(Debug, Clone)]
pub struct IrTypeDecl {
    pub name: String,
    pub fields: Vec<(String, IrType)>,
}

impl IrModule {
    /// 创建新模块
    pub fn new(name: String, target_triple: String) -> Self {
        Self {
            name,
            target_triple,
            functions: Vec::new(),
            extern_declarations: Vec::new(),
            globals: Vec::new(),
            string_constants: HashMap::new(),
            type_declarations: Vec::new(),
            string_counter: 0,
            global_counter: 0,
        }
    }

    /// 添加函数
    pub fn add_function(&mut self, func: IrFunction) -> &mut IrFunction {
        self.functions.push(func);
        self.functions.last_mut().expect("push 后 last_mut 应始终成功")
    }

    /// 添加外部声明
    pub fn add_extern(&mut self, decl: IrExternDecl) {
        // 避免重复声明
        if !self.extern_declarations.iter().any(|d| d.name == decl.name) {
            self.extern_declarations.push(decl);
        }
    }

    /// 添加全局变量
    pub fn add_global(&mut self, global: IrGlobal) {
        self.globals.push(global);
    }

    /// 添加或获取字符串常量
    pub fn add_string(&mut self, value: &str) -> String {
        if let Some(name) = self.string_constants.iter()
            .find(|(_, v)| *v == value)
            .map(|(n, _)| n.clone())
        {
            return name;
        }

        let name = format!("@.str.{}", self.string_counter);
        self.string_counter += 1;
        self.string_constants.insert(name.clone(), value.to_string());
        name
    }

    /// 生成新的全局唯一名称
    pub fn new_global_name(&mut self, prefix: &str) -> String {
        let name = format!("@{}.{}", prefix, self.global_counter);
        self.global_counter += 1;
        name
    }

    /// 添加类型声明
    pub fn add_type_decl(&mut self, decl: IrTypeDecl) {
        self.type_declarations.push(decl);
    }

    /// 查找函数
    pub fn find_function(&self, name: &str) -> Option<&IrFunction> {
        self.functions.iter().find(|f| f.name == name)
    }

    /// 查找函数（可变引用）
    pub fn find_function_mut(&mut self, name: &str) -> Option<&mut IrFunction> {
        self.functions.iter_mut().find(|f| f.name == name)
    }

    /// 查找外部声明
    pub fn find_extern(&self, name: &str) -> Option<&IrExternDecl> {
        self.extern_declarations.iter().find(|e| e.name == name)
    }

    /// 验证整个模块的结构完整性
    pub fn verify(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();

        for func in &self.functions {
            if let Err(e) = func.verify() {
                errors.push(e);
            }
        }

        // 检查重复函数名
        let mut names = HashMap::new();
        for func in &self.functions {
            if let Some(prev) = names.insert(&func.name, func.signature()) {
                if prev != func.signature() {
                    errors.push(format!(
                        "Duplicate function '{}' with different signature", func.name
                    ));
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// 统计信息
    pub fn stats(&self) -> IrModuleStats {
        let mut stats = IrModuleStats::default();
        stats.function_count = self.functions.len();
        stats.extern_count = self.extern_declarations.len();
        stats.global_count = self.globals.len();
        stats.string_count = self.string_constants.len();

        for func in &self.functions {
            stats.block_count += func.blocks.len();
            for block in &func.blocks {
                stats.instruction_count += block.instructions.len();
            }
        }

        stats
    }
}

/// 模块统计信息
#[derive(Debug, Default, Clone)]
pub struct IrModuleStats {
    pub function_count: usize,
    pub extern_count: usize,
    pub global_count: usize,
    pub string_count: usize,
    pub block_count: usize,
    pub instruction_count: usize,
}
