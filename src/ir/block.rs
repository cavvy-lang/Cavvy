//! IR 基本块
//!
//! 基本块是 IR 的基本执行单元，包含一系列指令和一个终止指令。

use super::value::{IrInstruction, IrTerminator};
use std::fmt;

/// IR 基本块
///
/// 基本块具有：
/// - 唯一的标签名
/// - 一系列非终止指令
/// - 恰好一个终止指令（return / br / switch / unreachable）
#[derive(Debug, Clone)]
pub struct IrBasicBlock {
    /// 基本块标签（如 "entry", "while.cond.0"）
    pub label: String,
    /// 非终止指令列表
    pub instructions: Vec<IrInstruction>,
    /// 终止指令（必须存在）
    pub terminator: Option<IrTerminator>,
    /// 是否是入口块
    pub is_entry: bool,
}

impl IrBasicBlock {
    /// 创建新的基本块
    pub fn new(label: String) -> Self {
        Self {
            label,
            instructions: Vec::new(),
            terminator: None,
            is_entry: false,
        }
    }

    /// 创建入口基本块
    pub fn entry() -> Self {
        Self {
            label: "entry".to_string(),
            instructions: Vec::new(),
            terminator: None,
            is_entry: true,
        }
    }

    /// 添加指令
    pub fn push(&mut self, inst: IrInstruction) {
        self.instructions.push(inst);
    }

    /// 设置终止指令
    pub fn set_terminator(&mut self, term: IrTerminator) {
        self.terminator = Some(term);
    }

    /// 检查块是否完整（有终止指令）
    pub fn is_complete(&self) -> bool {
        self.terminator.is_some()
    }

    /// 获取所有前驱块的标签（通过分析跳转目标得出，需要在函数级别分析）
    /// 这里只返回当前块跳转到的目标
    pub fn successor_labels(&self) -> Vec<&str> {
        match &self.terminator {
            Some(IrTerminator::Return { .. }) => vec![],
            Some(IrTerminator::Branch { target }) => vec![target.as_str()],
            Some(IrTerminator::ConditionalBranch {
                true_target,
                false_target,
                ..
            }) => {
                vec![true_target.as_str(), false_target.as_str()]
            }
            Some(IrTerminator::Switch {
                default_target,
                cases,
                ..
            }) => {
                let mut targets: Vec<&str> = cases.iter().map(|(_, t)| t.as_str()).collect();
                targets.push(default_target.as_str());
                targets
            }
            Some(IrTerminator::Unreachable) => vec![],
            None => vec![],
        }
    }
}

impl fmt::Display for IrBasicBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}:", self.label)?;
        for inst in &self.instructions {
            writeln!(f, "  {:?}", inst)?;
        }
        if let Some(term) = &self.terminator {
            writeln!(f, "  {:?}", term)?;
        }
        Ok(())
    }
}
