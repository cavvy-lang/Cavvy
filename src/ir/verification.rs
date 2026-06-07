//! IR 验证器
//!
//! 对 IR 模块进行结构完整性验证，确保：
//! - 所有基本块都有终止指令
//! - 所有跳转目标存在
//! - 所有 SSA 值在使用前定义
//! - 类型一致性
//! - 无孤立基本块（可选）

use super::function::{IrFunction, IrLinkage};
use super::module::IrModule;
use super::types::IrType;
use super::value::{IrInstruction, IrTerminator, IrValue};
use std::collections::HashSet;

/// IR 验证器
pub struct IrVerifier {
    /// 收集到的错误
    errors: Vec<String>,
    /// 收集到的警告
    warnings: Vec<String>,
    /// 是否严格模式（警告也视为错误）
    strict: bool,
}

/// 验证结果
#[derive(Debug)]
pub struct VerificationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl IrVerifier {
    /// 创建新的验证器
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            warnings: Vec::new(),
            strict: false,
        }
    }

    /// 严格模式
    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    /// 验证整个模块
    pub fn verify(&mut self, module: &IrModule) -> VerificationResult {
        self.errors.clear();
        self.warnings.clear();

        // 验证每个函数
        let mut function_names = HashSet::new();
        for func in &module.functions {
            // 检查重复函数名
            if !function_names.insert(&func.name) {
                self.error(&format!("Duplicate function name: {}", func.name));
            }

            self.verify_function(func);
        }

        // 验证外部声明不冲突
        let mut extern_names = HashSet::new();
        for decl in &module.extern_declarations {
            if !extern_names.insert(&decl.name) {
                self.warning(&format!("Duplicate extern declaration: {}", decl.name));
            }
        }

        VerificationResult {
            is_valid: self.errors.is_empty() && (!self.strict || self.warnings.is_empty()),
            errors: self.errors.clone(),
            warnings: self.warnings.clone(),
        }
    }

    /// 验证单个函数
    fn verify_function(&mut self, func: &IrFunction) {
        let ctx = format!("function '{}'", func.name);

        // 声明函数跳过
        if func.linkage == IrLinkage::Declare {
            if !func.blocks.is_empty() {
                self.error(&format!("{}: declared function has blocks", ctx));
            }
            return;
        }

        if func.blocks.is_empty() {
            self.error(&format!("{}: no basic blocks", ctx));
            return;
        }

        // 收集所有块标签
        let block_labels: HashSet<&str> = func.blocks.iter().map(|b| b.label.as_str()).collect();

        // 验证入口块
        if !func.blocks[0].is_entry {
            self.error(&format!(
                "{}: first block '{}' is not marked as entry",
                ctx, func.blocks[0].label
            ));
        }

        // 收集所有定义的 SSA 值
        let mut defined_values: HashSet<String> = HashSet::new();
        // 参数总是已定义的
        for param in &func.params {
            defined_values.insert(format!("%{}", param.name));
        }

        // 验证每个基本块
        for block in &func.blocks {
            let block_ctx = format!("{}: block '{}'", ctx, block.label);

            // 检查终止指令
            if !block.is_complete() {
                self.error(&format!("{}: missing terminator", block_ctx));
            }

            // 验证指令
            for inst in &block.instructions {
                self.verify_instruction(inst, &block_ctx, &defined_values, &block_labels);

                // 记录此指令产生的值
                if let Some(result) = inst.result() {
                    let name = result.to_llvm_str();
                    defined_values.insert(name);
                }

                // 检查 InlineIr 的内容不为空
                if let IrInstruction::InlineIr { lines, .. } = inst {
                    if lines.is_empty() {
                        self.warning(&format!("{}: empty inline IR instruction", block_ctx));
                    }
                }
            }

            // 验证终止指令的跳转目标
            if let Some(term) = &block.terminator {
                self.verify_terminator(term, &block_ctx, &block_labels);
            }
        }

        // 检查是否有不可达的块（除了入口块）
        if block_labels.len() > 1 {
            let reachable = self.compute_reachable_blocks(func);
            let unreachable_blocks: Vec<String> = func
                .blocks
                .iter()
                .filter(|b| !b.is_entry && !reachable.contains(&b.label.as_str()))
                .map(|b| format!("{}: block '{}' is unreachable", ctx, b.label))
                .collect();
            for msg in unreachable_blocks {
                self.warning(&msg);
            }
        }
    }

    /// 验证单条指令
    fn verify_instruction(
        &mut self,
        inst: &IrInstruction,
        ctx: &str,
        defined: &HashSet<String>,
        _block_labels: &HashSet<&str>,
    ) {
        // 验证所有输入值是否已定义
        for input in inst.inputs() {
            let name = input.to_llvm_str();
            // 常量、全局引用、字符串常量不需要检查
            if input.is_const() || matches!(input, IrValue::GlobalRef(_, _)) {
                continue;
            }
            if name.starts_with('%') && !defined.contains(&name) {
                self.error(&format!("{}: use of undefined value '{}'", ctx, name));
            }
        }
    }

    /// 验证终止指令
    fn verify_terminator(&mut self, term: &IrTerminator, ctx: &str, block_labels: &HashSet<&str>) {
        match term {
            IrTerminator::Return { value } => {
                if let Some(val) = value {
                    if matches!(val.ir_type(), IrType::Void) {
                        self.error(&format!("{}: ret with void type should use ret void", ctx));
                    }
                }
            }

            IrTerminator::Branch { target } => {
                if !block_labels.contains(target.as_str()) {
                    self.error(&format!("{}: branch to unknown label '{}'", ctx, target));
                }
            }

            IrTerminator::ConditionalBranch {
                condition: _,
                true_target,
                false_target,
            } => {
                if !block_labels.contains(true_target.as_str()) {
                    self.error(&format!(
                        "{}: branch to unknown true target '{}'",
                        ctx, true_target
                    ));
                }
                if !block_labels.contains(false_target.as_str()) {
                    self.error(&format!(
                        "{}: branch to unknown false target '{}'",
                        ctx, false_target
                    ));
                }
            }

            IrTerminator::Switch {
                default_target,
                cases,
                ..
            } => {
                if !block_labels.contains(default_target.as_str()) {
                    self.error(&format!(
                        "{}: switch to unknown default target '{}'",
                        ctx, default_target
                    ));
                }
                for (_, case_target) in cases {
                    if !block_labels.contains(case_target.as_str()) {
                        self.error(&format!(
                            "{}: switch to unknown case target '{}'",
                            ctx, case_target
                        ));
                    }
                }
            }

            IrTerminator::Unreachable => {
                // 总是有效的
            }
        }
    }

    /// 计算从入口块可达的基本块集合
    fn compute_reachable_blocks<'a>(&self, func: &'a IrFunction) -> HashSet<&'a str> {
        let mut visited = HashSet::new();
        let mut queue = Vec::new();

        if let Some(entry) = func.entry_block() {
            queue.push(entry.label.as_str());
        }

        while let Some(label) = queue.pop() {
            if !visited.insert(label) {
                continue;
            }

            if let Some(block) = func.find_block(label) {
                for successor in block.successor_labels() {
                    if !visited.contains(successor) {
                        queue.push(successor);
                    }
                }
            }
        }

        visited
    }

    fn error(&mut self, msg: &str) {
        self.errors.push(msg.to_string());
    }

    fn warning(&mut self, msg: &str) {
        self.warnings.push(msg.to_string());
    }
}

impl Default for IrVerifier {
    fn default() -> Self {
        Self::new()
    }
}

/// 便捷函数：快速验证模块
pub fn verify_module(module: &IrModule) -> VerificationResult {
    IrVerifier::new().verify(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;

    #[test]
    fn test_valid_empty_function() {
        let module = IrModule::new("test".to_string(), "x86_64-unknown-linux-gnu".to_string());
        let result = verify_module(&module);
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_function_missing_terminator() {
        let mut module = IrModule::new("test".to_string(), "x86_64-unknown-linux-gnu".to_string());
        let func = IrFunction::new("test.f".to_string(), IrType::Void, Vec::new());
        module.add_function(func);

        let result = IrVerifier::new().verify(&module);
        assert!(!result.is_valid);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("missing terminator"))
        );
    }

    #[test]
    fn test_function_with_valid_return() {
        let mut module = IrModule::new("test".to_string(), "x86_64-unknown-linux-gnu".to_string());
        let mut func = IrFunction::new("test.f".to_string(), IrType::Void, Vec::new());

        let entry = func.entry_block_mut().unwrap();
        entry.set_terminator(IrTerminator::Return { value: None });

        module.add_function(func);

        let result = IrVerifier::new().verify(&module);
        assert!(result.is_valid);
    }
}
