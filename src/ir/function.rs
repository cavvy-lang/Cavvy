//! IR 函数
//!
//! 表示 IR 层面的函数定义，包含基本块列表和元数据。

use super::block::IrBasicBlock;
use super::types::IrType;
use super::value::IrValue;

/// 函数链接类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrLinkage {
    /// 外部可见
    External,
    /// 内部（模块私有）
    Internal,
    /// 私有
    Private,
    /// 声明（无函数体）
    Declare,
}

/// 函数参数
#[derive(Debug, Clone)]
pub struct IrParam {
    pub name: String,
    pub ty: IrType,
}

/// IR 函数
///
/// 代表一个完整的函数定义，包含：
/// - 函数名
/// - 返回类型
/// - 参数列表
/// - 基本块列表（CFG）
/// - 元数据
#[derive(Debug, Clone)]
pub struct IrFunction {
    /// 函数名（包含完整的类和命名空间信息）
    pub name: String,
    /// 返回类型
    pub return_type: IrType,
    /// 参数列表
    pub params: Vec<IrParam>,
    /// 基本块列表（第一个块是入口块）
    pub blocks: Vec<IrBasicBlock>,
    /// 链接类型
    pub linkage: IrLinkage,
    /// 是否是静态方法（无 this 指针）
    pub is_static: bool,
    /// 调用约定
    pub calling_convention: Option<String>,
    /// 局部变量数量（用于栈帧大小估计）
    pub local_count: u32,
    /// 临时寄存器计数器
    pub temp_counter: u32,
}

impl IrFunction {
    /// 创建新函数
    pub fn new(name: String, return_type: IrType, params: Vec<IrParam>) -> Self {
        Self {
            name,
            return_type,
            params,
            blocks: vec![IrBasicBlock::entry()],
            linkage: IrLinkage::External,
            is_static: false,
            calling_convention: None,
            local_count: 0,
            temp_counter: 0,
        }
    }

    /// 创建声明（无函数体）
    pub fn declare(name: String, return_type: IrType, params: Vec<IrParam>) -> Self {
        Self {
            name,
            return_type,
            params,
            blocks: Vec::new(),
            linkage: IrLinkage::Declare,
            is_static: false,
            calling_convention: None,
            local_count: 0,
            temp_counter: 0,
        }
    }

    /// 获取入口基本块
    pub fn entry_block(&self) -> Option<&IrBasicBlock> {
        self.blocks.first()
    }

    /// 获取入口基本块（可变引用）
    pub fn entry_block_mut(&mut self) -> Option<&mut IrBasicBlock> {
        self.blocks.first_mut()
    }

    /// 添加基本块
    pub fn add_block(&mut self, block: IrBasicBlock) -> &mut IrBasicBlock {
        self.blocks.push(block);
        self.blocks.last_mut().expect("push 后 last_mut 应始终成功")
    }

    /// 查找基本块
    pub fn find_block(&self, label: &str) -> Option<&IrBasicBlock> {
        self.blocks.iter().find(|b| b.label == label)
    }

    /// 查找基本块（可变引用）
    pub fn find_block_mut(&mut self, label: &str) -> Option<&mut IrBasicBlock> {
        self.blocks.iter_mut().find(|b| b.label == label)
    }

    /// 获取当前基本块（最后一个）
    pub fn current_block(&self) -> Option<&IrBasicBlock> {
        self.blocks.last()
    }

    /// 获取当前基本块（可变引用）
    pub fn current_block_mut(&mut self) -> Option<&mut IrBasicBlock> {
        self.blocks.last_mut()
    }

    /// 生成新的临时寄存器名
    pub fn new_temp(&mut self) -> IrValue {
        let name = format!("%t{}", self.temp_counter);
        self.temp_counter += 1;
        IrValue::Register(name, IrType::I32) // 类型在使用时更新
    }

    /// 生成带类型的临时寄存器
    pub fn new_typed_temp(&mut self, ty: IrType) -> IrValue {
        let name = format!("%t{}", self.temp_counter);
        self.temp_counter += 1;
        IrValue::Register(name, ty)
    }

    /// 获取所有基本块的标签
    pub fn block_labels(&self) -> Vec<&str> {
        self.blocks.iter().map(|b| b.label.as_str()).collect()
    }

    /// 验证函数的结构完整性
    pub fn verify(&self) -> Result<(), String> {
        if self.linkage == IrLinkage::Declare {
            return Ok(());
        }

        if self.blocks.is_empty() {
            return Err(format!("Function '{}' has no blocks", self.name));
        }

        // 检查入口块
        if !self.blocks[0].is_entry {
            return Err(format!("Function '{}' first block is not entry", self.name));
        }

        // 检查所有块都有终止指令
        for block in &self.blocks {
            if !block.is_complete() {
                return Err(format!(
                    "Block '{}' in function '{}' is missing terminator",
                    block.label, self.name
                ));
            }
        }

        // 检查跳转目标存在
        let labels: Vec<&str> = self.block_labels();
        for block in &self.blocks {
            for target in block.successor_labels() {
                if !labels.contains(&target) {
                    return Err(format!(
                        "Block '{}' in function '{}' branches to non-existent block '{}'",
                        block.label, self.name, target
                    ));
                }
            }
        }

        Ok(())
    }

    /// 获取函数签名（用于类型匹配）
    pub fn signature(&self) -> String {
        let param_strs: Vec<String> = self.params.iter()
            .map(|p| p.ty.to_llvm_str())
            .collect();
        format!("{}({})->{}", self.name, param_strs.join(","), self.return_type.to_llvm_str())
    }
}
