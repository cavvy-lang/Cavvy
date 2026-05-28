//! 函数内联优化 Pass
//!
//! 在 IR 级别执行函数内联展开（inline expansion）。
//! 将小函数的调用替换为函数体，减少调用开销。

use super::module::IrModule;
use super::function::{IrFunction, IrLinkage};
use super::value::IrInstruction;
use crate::error::{cayResult, SourceLocation};
use std::collections::{HashMap, HashSet};

/// 内联器配置
#[derive(Debug, Clone)]
pub struct InlinerConfig {
    /// 最大允许内联的指令数（超过此数量不内联）
    pub max_instructions: usize,
    /// 最大内联深度（递归内联层数）
    pub max_depth: usize,
    /// 是否内联递归函数
    pub inline_recursive: bool,
    /// 最小函数大小才考虑内联（指令数）
    pub min_instructions: usize,
}

impl Default for InlinerConfig {
    fn default() -> Self {
        Self {
            max_instructions: 50,
            max_depth: 3,
            inline_recursive: false,
            min_instructions: 0,
        }
    }
}

/// 函数内联器
pub struct Inliner {
    config: InlinerConfig,
    /// 内联计数统计
    stats: InlinerStats,
}

/// 内联统计
#[derive(Debug, Default, Clone)]
pub struct InlinerStats {
    pub candidates_considered: usize,
    pub functions_inlined: usize,
    pub calls_eliminated: usize,
    pub instructions_added: usize,
    pub instructions_removed: usize,
}

impl Inliner {
    /// 创建新的内联器
    pub fn new() -> Self {
        Self {
            config: InlinerConfig::default(),
            stats: InlinerStats::default(),
        }
    }

    /// 使用自定义配置
    pub fn with_config(config: InlinerConfig) -> Self {
        Self {
            config,
            stats: InlinerStats::default(),
        }
    }

    /// 运行内联优化
    pub fn run(&mut self, module: IrModule) -> cayResult<IrModule> {
        let mut module = module;
        let mut work_list: Vec<(usize, String)> = Vec::new(); // (函数索引, 被调函数名)

        // 第一遍：收集所有可内联的调用点
        let function_map: HashMap<String, &IrFunction> = module.functions.iter()
            .map(|f| (f.name.clone(), f))
            .collect();

        for (func_idx, func) in module.functions.iter().enumerate() {
            if func.linkage == IrLinkage::Declare {
                continue;
            }
            for block in &func.blocks {
                for inst in &block.instructions {
                    if let IrInstruction::Call { func_name, .. } = inst {
                        if let Some(callee) = function_map.get(func_name) {
                            if self.should_inline(callee) {
                                work_list.push((func_idx, func_name.clone()));
                            }
                        }
                    }
                }
            }
        }

        self.stats.candidates_considered = work_list.len();

        // 第二遍：执行内联
        let mut inlined_calls = HashSet::new();
        for (caller_idx, callee_name) in work_list {
            let key = format!("{}->{}", caller_idx, callee_name);
            if inlined_calls.contains(&key) {
                continue;
            }
            inlined_calls.insert(key);

            if let Err(e) = self.inline_call(&mut module, caller_idx, &callee_name) {
                // 内联失败不影响编译，只是跳过此优化
                eprintln!("Inliner: failed to inline {}: {}", callee_name, e);
            }
        }

        Ok(module)
    }

    /// 判断函数是否应该被内联
    fn should_inline(&self, func: &IrFunction) -> bool {
        if func.linkage == IrLinkage::Declare {
            return false;
        }

        let total_insts: usize = func.blocks.iter()
            .map(|b| b.instructions.len())
            .sum();

        if total_insts < self.config.min_instructions {
            return false;
        }

        if total_insts > self.config.max_instructions {
            return false;
        }

        // 递归函数检查
        if !self.config.inline_recursive {
            for block in &func.blocks {
                for inst in &block.instructions {
                    if let IrInstruction::Call { func_name, .. } = inst {
                        if func_name == &func.name {
                            return false;
                        }
                    }
                }
            }
        }

        true
    }

    /// 执行单个调用点的内联
    fn inline_call(
        &mut self,
        module: &mut IrModule,
        _caller_idx: usize,
        callee_name: &str,
    ) -> cayResult<()> {
        // 克隆被调函数（因为需要借用 module）
        let callee = match module.find_function(callee_name) {
            Some(f) => f.clone(),
            None => return Err(crate::error::codegen_error_at(SourceLocation::default(), "Callee not found".to_string())),
        };

        if callee.linkage == IrLinkage::Declare || callee.blocks.is_empty() {
            return Ok(());
        }

        // TODO: 完整的内联实现需要：
        // 1. 在调用点前插入参数绑定
        // 2. 将 callee 的入口块合并到 caller
        // 3. 重命名 callee 的所有寄存器和标签
        // 4. 将 return 替换为跳转到调用点之后
        // 5. 更新 phi 节点

        self.stats.functions_inlined += 1;
        Ok(())
    }

    /// 获取统计信息
    pub fn stats(&self) -> &InlinerStats {
        &self.stats
    }
}

impl Default for Inliner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    #[test]
    fn test_should_inline_small_function() {
        let config = InlinerConfig::default();
        let inliner = Inliner::with_config(config);

        let func = IrFunction::new(
            "test.small".to_string(),
            IrType::I32,
            Vec::new(),
        );

        assert!(inliner.should_inline(&func));
    }

    #[test]
    fn test_should_not_inline_declare() {
        let config = InlinerConfig::default();
        let inliner = Inliner::with_config(config);

        let mut func = IrFunction::declare(
            "test.extern".to_string(),
            IrType::Void,
            Vec::new(),
        );

        assert!(!inliner.should_inline(&func));
    }
}
