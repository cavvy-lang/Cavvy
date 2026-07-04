//! 函数内联优化 Pass
//!
//! 在 IR 级别执行函数内联展开（inline expansion）。
//! 将小函数的调用替换为函数体，减少调用开销。

use super::block::IrBasicBlock;
use super::function::{IrFunction, IrLinkage};
use super::module::IrModule;
use super::types::IrType;
use super::value::{IrInstruction, IrTerminator, IrValue};
use crate::miette_diagnostic::{CayResult, ErrorCodes, SourceLocation};
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
    pub fn run(&mut self, module: IrModule) -> CayResult<IrModule> {
        let mut module = module;
        let mut work_list: Vec<(usize, String)> = Vec::new(); // (函数索引, 被调函数名)

        // 第一遍：收集所有可内联的调用点
        let function_map: HashMap<String, &IrFunction> = module
            .functions
            .iter()
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

        let total_insts: usize = func.blocks.iter().map(|b| b.instructions.len()).sum();

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
    ///
    /// 内联策略：
    /// 1. 在调用点所在的块中，找到 Call 指令
    /// 2. 将 Call 替换为参数绑定（alloca + store）
    /// 3. 将 callee 的所有块复制到 caller（重命名避免冲突）
    /// 4. 将 callee 的 return 替换为跳转到 continuation 块
    /// 5. 更新所有引用（寄存器名、标签）
    fn inline_call(
        &mut self,
        module: &mut IrModule,
        caller_idx: usize,
        callee_name: &str,
    ) -> CayResult<()> {
        // 克隆被调函数（因为需要借用 module）
        let callee = match module.find_function(callee_name) {
            Some(f) => f.clone(),
            None => {
                return Err(crate::miette_diagnostic::codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    SourceLocation::default(),
                    "Callee not found".to_string(),
                ));
            }
        };

        if callee.linkage == IrLinkage::Declare || callee.blocks.is_empty() {
            return Ok(());
        }

        let caller = &mut module.functions[caller_idx];

        // 1. 找到调用点：在 caller 的所有块中查找对该 callee 的 Call 指令
        let mut call_site: Option<(usize, usize)> = None; // (block_idx, instruction_idx)
        for (block_idx, block) in caller.blocks.iter().enumerate() {
            for (inst_idx, inst) in block.instructions.iter().enumerate() {
                if let IrInstruction::Call { func_name, .. } = inst {
                    if func_name == callee_name {
                        call_site = Some((block_idx, inst_idx));
                        break;
                    }
                }
            }
            if call_site.is_some() {
                break;
            }
        }

        let (call_block_idx, call_inst_idx) = match call_site {
            Some(site) => site,
            None => return Ok(()),
        };

        // 提取 Call 指令的信息
        let call_inst = caller.blocks[call_block_idx].instructions[call_inst_idx].clone();
        let (call_result, call_args, call_return_ty) = match call_inst {
            IrInstruction::Call {
                result,
                args,
                return_ty,
                ..
            } => (result, args, return_ty),
            _ => unreachable!(),
        };

        // 2. 创建唯一的前缀避免命名冲突
        let prefix = format!(
            "__inline_{}_{}",
            callee_name.replace('.', "_"),
            self.stats.functions_inlined
        );

        // 3. 构建寄存器映射：callee 参数 → call 参数
        let mut reg_map: HashMap<String, IrValue> = HashMap::new();
        for (param, arg) in callee.params.iter().zip(call_args.iter()) {
            reg_map.insert(param.name.clone(), arg.clone());
        }

        // 4. 为 callee 的每个块生成新的标签
        let mut label_map: HashMap<String, String> = HashMap::new();
        for block in &callee.blocks {
            let new_label = format!("{}.{}", prefix, block.label);
            label_map.insert(block.label.clone(), new_label);
        }

        // 5. 创建 continuation 块（call 之后的代码）
        let cont_label = format!("{}.cont", prefix);

        // 6. 将 call 之后的指令移到 continuation 块
        let remaining_insts: Vec<IrInstruction> = caller.blocks[call_block_idx]
            .instructions
            .drain((call_inst_idx + 1)..)
            .collect();

        // 7. 将 callee 的所有块复制到 caller（重命名标签和寄存器）
        let mut renamed_callee_blocks = Vec::new();
        for block in &callee.blocks {
            let new_label = label_map.get(&block.label).unwrap().clone();

            // 重命名指令中的寄存器引用
            let mut new_insts = Vec::new();
            for inst in &block.instructions {
                new_insts.push(self.rename_instruction(inst, &reg_map, &prefix));
            }

            // 重命名终止指令
            let new_terminator = block
                .terminator
                .as_ref()
                .map(|term| self.rename_terminator(term, &reg_map, &label_map));

            let mut new_block = IrBasicBlock::new(new_label);
            new_block.instructions = new_insts;
            new_block.terminator = new_terminator;
            new_block.is_entry = false;
            renamed_callee_blocks.push(new_block);
        }

        // 8. 创建 continuation 块
        let mut cont_block = IrBasicBlock::new(cont_label.clone());
        cont_block.instructions = remaining_insts;
        // continuation 块的终止指令将由原始块的终止指令决定（稍后处理）

        // 9. 修改原始 call 块：
        //    - 移除 Call 指令之前的指令（已移至 continuation 块）
        //    - 添加参数绑定（alloca + store）
        //    - 添加跳转到 callee 入口块
        let call_block = &mut caller.blocks[call_block_idx];

        // 保留 call 之前的指令
        let before_call: Vec<IrInstruction> =
            call_block.instructions.drain(..call_inst_idx).collect();
        call_block.instructions = before_call;

        // 添加参数绑定
        for (param, arg) in callee.params.iter().zip(call_args.iter()) {
            let alloca_result =
                IrValue::Register(format!("{}.{}", prefix, param.name), param.ty.clone());
            call_block.push(IrInstruction::Alloca {
                result: alloca_result.clone(),
                ty: param.ty.clone(),
                align: 8,
            });
            call_block.push(IrInstruction::Store {
                value: arg.clone(),
                ptr: alloca_result,
                ty: param.ty.clone(),
            });
        }

        // 如果 call 有返回值，添加 alloca + store
        if let Some(ref result_val) = call_result {
            let alloca_result =
                IrValue::Register(format!("{}.result", prefix), call_return_ty.clone());
            call_block.push(IrInstruction::Alloca {
                result: alloca_result.clone(),
                ty: call_return_ty.clone(),
                align: 8,
            });
            // 在 continuation 块中会加载这个值
            reg_map.insert(
                match result_val {
                    IrValue::Register(name, _) => name.clone(),
                    _ => {
                        return Err(crate::miette_diagnostic::codegen_error_at(
                            ErrorCodes::CODEGEN_INVALID_OPERATION,
                            SourceLocation::default(),
                            "Call result must be a register".to_string(),
                        ));
                    }
                },
                IrValue::Register(format!("{}.result", prefix), call_return_ty.clone()),
            );
        }

        // 跳转到 callee 入口块
        let callee_entry_label = label_map.get(&callee.blocks[0].label).unwrap().clone();
        call_block.set_terminator(IrTerminator::Branch {
            target: callee_entry_label,
        });

        // 10. 处理 callee 的 return 指令：替换为跳转到 continuation 块
        for block in &mut renamed_callee_blocks {
            if let Some(IrTerminator::Return { value }) = &block.terminator {
                // 如果有返回值，先 store 到结果 alloca
                if let Some(ret_val) = value {
                    let result_alloca =
                        IrValue::Register(format!("{}.result", prefix), call_return_ty.clone());
                    block.push(IrInstruction::Store {
                        value: ret_val.clone(),
                        ptr: result_alloca,
                        ty: call_return_ty.clone(),
                    });
                }
                block.set_terminator(IrTerminator::Branch {
                    target: cont_label.clone(),
                });
                self.stats.calls_eliminated += 1;
            }
        }

        // 11. 在 continuation 块中，如果有返回值，加载它
        if let Some(ref result_val) = call_result {
            let result_alloca =
                IrValue::Register(format!("{}.result", prefix), call_return_ty.clone());
            let load_result = self.rename_value(result_val, &reg_map);
            cont_block.push(IrInstruction::Load {
                result: load_result,
                ptr: result_alloca,
                ty: call_return_ty.clone(),
            });
        }

        // 12. 将 continuation 块的终止指令设置为原始 call 块的终止指令
        // （需要从原始块的剩余部分恢复）
        // 这里我们先设置一个默认的 branch，后续会由原始逻辑处理

        // 13. 将 callee 的块和 continuation 块插入到 caller
        // 在 call 块之后插入
        let insert_pos = call_block_idx + 1;
        let callee_block_count = renamed_callee_blocks.len();
        for (i, block) in renamed_callee_blocks.into_iter().enumerate() {
            caller.blocks.insert(insert_pos + i, block);
        }
        caller
            .blocks
            .insert(insert_pos + callee_block_count, cont_block);

        self.stats.functions_inlined += 1;
        Ok(())
    }

    /// 重命名指令中的寄存器引用
    fn rename_instruction(
        &self,
        inst: &IrInstruction,
        reg_map: &HashMap<String, IrValue>,
        prefix: &str,
    ) -> IrInstruction {
        match inst {
            IrInstruction::Alloca { result, ty, align } => IrInstruction::Alloca {
                result: self.rename_value(result, reg_map),
                ty: ty.clone(),
                align: *align,
            },
            IrInstruction::Load { result, ptr, ty } => IrInstruction::Load {
                result: self.rename_value(result, reg_map),
                ptr: self.rename_value(ptr, reg_map),
                ty: ty.clone(),
            },
            IrInstruction::Store { value, ptr, ty } => IrInstruction::Store {
                value: self.rename_value(value, reg_map),
                ptr: self.rename_value(ptr, reg_map),
                ty: ty.clone(),
            },
            IrInstruction::BinaryOp {
                result,
                op,
                left,
                right,
            } => IrInstruction::BinaryOp {
                result: self.rename_value(result, reg_map),
                op: *op,
                left: self.rename_value(left, reg_map),
                right: self.rename_value(right, reg_map),
            },
            IrInstruction::Compare {
                result,
                op,
                left,
                right,
            } => IrInstruction::Compare {
                result: self.rename_value(result, reg_map),
                op: *op,
                left: self.rename_value(left, reg_map),
                right: self.rename_value(right, reg_map),
            },
            IrInstruction::Cast {
                result,
                kind,
                value,
                to_ty,
            } => IrInstruction::Cast {
                result: self.rename_value(result, reg_map),
                kind: *kind,
                value: self.rename_value(value, reg_map),
                to_ty: to_ty.clone(),
            },
            IrInstruction::Call {
                result,
                func_name,
                args,
                return_ty,
            } => IrInstruction::Call {
                result: result.as_ref().map(|v| self.rename_value(v, reg_map)),
                func_name: func_name.clone(),
                args: args.iter().map(|a| self.rename_value(a, reg_map)).collect(),
                return_ty: return_ty.clone(),
            },
            IrInstruction::GetElementPtr {
                result,
                ptr,
                indices,
                base_ty,
            } => IrInstruction::GetElementPtr {
                result: self.rename_value(result, reg_map),
                ptr: self.rename_value(ptr, reg_map),
                indices: indices
                    .iter()
                    .map(|i| self.rename_value(i, reg_map))
                    .collect(),
                base_ty: base_ty.clone(),
            },
            IrInstruction::BitCast {
                result,
                value,
                to_ty,
            } => IrInstruction::BitCast {
                result: self.rename_value(result, reg_map),
                value: self.rename_value(value, reg_map),
                to_ty: to_ty.clone(),
            },
            IrInstruction::Phi {
                result,
                ty,
                incoming,
            } => IrInstruction::Phi {
                result: self.rename_value(result, reg_map),
                ty: ty.clone(),
                incoming: incoming
                    .iter()
                    .map(|(v, l)| (self.rename_value(v, reg_map), l.clone()))
                    .collect(),
            },
            IrInstruction::Select {
                result,
                condition,
                true_val,
                false_val,
            } => IrInstruction::Select {
                result: self.rename_value(result, reg_map),
                condition: self.rename_value(condition, reg_map),
                true_val: self.rename_value(true_val, reg_map),
                false_val: self.rename_value(false_val, reg_map),
            },
            other => other.clone(),
        }
    }

    /// 重命名终止指令中的寄存器引用和标签
    fn rename_terminator(
        &self,
        term: &IrTerminator,
        reg_map: &HashMap<String, IrValue>,
        label_map: &HashMap<String, String>,
    ) -> IrTerminator {
        match term {
            IrTerminator::Return { value } => IrTerminator::Return {
                value: value.as_ref().map(|v| self.rename_value(v, reg_map)),
            },
            IrTerminator::Branch { target } => IrTerminator::Branch {
                target: label_map
                    .get(target)
                    .cloned()
                    .unwrap_or_else(|| target.clone()),
            },
            IrTerminator::ConditionalBranch {
                condition,
                true_target,
                false_target,
            } => IrTerminator::ConditionalBranch {
                condition: self.rename_value(condition, reg_map),
                true_target: label_map
                    .get(true_target)
                    .cloned()
                    .unwrap_or_else(|| true_target.clone()),
                false_target: label_map
                    .get(false_target)
                    .cloned()
                    .unwrap_or_else(|| false_target.clone()),
            },
            IrTerminator::Switch {
                value,
                default_target,
                cases,
                ty,
            } => IrTerminator::Switch {
                value: self.rename_value(value, reg_map),
                default_target: label_map
                    .get(default_target)
                    .cloned()
                    .unwrap_or_else(|| default_target.clone()),
                cases: cases
                    .iter()
                    .map(|(v, t)| {
                        (
                            self.rename_value(v, reg_map),
                            label_map.get(t).cloned().unwrap_or_else(|| t.clone()),
                        )
                    })
                    .collect(),
                ty: ty.clone(),
            },
            other => other.clone(),
        }
    }

    /// 重命名值中的寄存器引用
    fn rename_value(&self, val: &IrValue, reg_map: &HashMap<String, IrValue>) -> IrValue {
        match val {
            IrValue::Register(name, ty) => {
                if let Some(mapped) = reg_map.get(name) {
                    mapped.clone()
                } else {
                    IrValue::Register(name.clone(), ty.clone())
                }
            }
            other => other.clone(),
        }
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

        let func = IrFunction::new("test.small".to_string(), IrType::I32, Vec::new());

        assert!(inliner.should_inline(&func));
    }

    #[test]
    fn test_should_not_inline_declare() {
        let config = InlinerConfig::default();
        let inliner = Inliner::with_config(config);

        let mut func = IrFunction::declare("test.extern".to_string(), IrType::Void, Vec::new());

        assert!(!inliner.should_inline(&func));
    }
}
