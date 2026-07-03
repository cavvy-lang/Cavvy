//! CodeGen 与 IR Builder 协作桥
//!
//! 提供两个IR生成系统之间的安全协作机制：
//! - CodeGen (IRGenerator): 主代码生成管线，直接生成LLVM IR文本
//! - IR Builder (IrBuilder): 结构化IR构建管线，支持高级特性如内联IR
//!
//! # 架构设计
//!
//! ```text
//! ┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
//! │   CodeGen       │────▶│   Bridge         │────▶│   IR Builder    │
//! │ (IRGenerator)   │     │ (InlineIrBridge) │     │ (IrBuilder)     │
//! └─────────────────┘     └──────────────────┘     └─────────────────┘
//!        │                                               │
//!        │                                               ▼
//!        │                                        ┌─────────────────┐
//!        │                                        │  InlineIrBlock  │
//!        │                                        │  (解析&验证)    │
//!        │                                        └─────────────────┘
//!        │                                               │
//!        │◀──────────────────────────────────────────────┘
//!        │              (返回LLVM IR文本)
//!        ▼
//! ┌─────────────────┐
//! │  合并到主IR输出  │
//! └─────────────────┘
//! ```
//!
//! # 安全保证
//!
//! - 变量作用域隔离：内联IR只能访问当前作用域内可见的变量
//! - 类型安全检查：验证内联IR中的类型与Cavvy类型系统兼容
//! - 指令白名单：只允许安全的LLVM指令
//! - 资源泄漏防护：所有临时资源都有RAII管理

use crate::ast::InlineIrStmt;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::cayResult;
use crate::types::Type;
use std::collections::HashMap;

/// 内联IR处理结果
#[derive(Debug, Clone)]
pub struct InlineIrResult {
    /// 生成的LLVM IR文本行
    pub llvm_ir_lines: Vec<String>,
    /// 使用的输入变量映射 (Cavvy变量名 -> LLVM寄存器名)
    pub input_mappings: HashMap<String, String>,
    /// 产生的输出变量映射 (LLVM寄存器名 -> Cavvy变量名)
    pub output_mappings: HashMap<String, String>,
}

/// CodeGen与IR Builder协作桥
///
/// 负责协调两个IR生成系统之间的通信和数据转换
pub struct InlineIrBridge {
    /// 内联IR解析器
    parser: crate::ir::inline_ir::InlineIrParser,
}

impl InlineIrBridge {
    /// 创建新的协作桥实例
    pub fn new() -> Self {
        Self {
            parser: crate::ir::inline_ir::InlineIrParser::new(),
        }
    }

    /// 处理内联IR语句
    ///
    /// # 流程
    /// 1. 从CodeGen获取当前作用域的变量信息
    /// 2. 解析内联IR文本，验证安全性
    /// 3. 将Cavvy变量名映射到LLVM寄存器名
    /// 4. 生成最终的LLVM IR文本
    ///
    /// # 参数
    /// * `codegen` - CodeGen生成器上下文
    /// * `inline_ir` - 内联IR语句AST节点
    ///
    /// # 返回
    /// 处理结果，包含生成的LLVM IR文本和变量映射
    ///
    /// # 复杂度
    /// - 时间: O(n*m)，n为内联IR行数，m为作用域变量数
    /// - 空间: O(n+m)
    pub fn process_inline_ir(
        &self,
        codegen: &IRGenerator,
        inline_ir: &InlineIrStmt,
    ) -> cayResult<InlineIrResult> {
        // 1. 收集当前作用域的可用变量
        let available_vars = self.collect_scope_variables(codegen);

        // 2. 准备IR Builder的输入
        let ir_inputs: Vec<(String, crate::ir::value::IrValue)> = available_vars
            .iter()
            .map(|(name, llvm_name, ty)| {
                let ir_ty = self.convert_type_to_ir(ty);
                let ir_value = crate::ir::value::IrValue::Register(llvm_name.clone(), ir_ty);
                (name.clone(), ir_value)
            })
            .collect();

        // 3. 解析内联IR文本
        // eprintln!("DEBUG bridge: inline_ir.raw_lines = {:?}", inline_ir.raw_lines);
        let raw_text = inline_ir.raw_lines.join("\n");
        // eprintln!("DEBUG bridge: raw_text = '{}'", raw_text);
        let parsed_block = self.parser.parse(&raw_text, &ir_inputs, &[]).map_err(|e| {
            crate::miette_diagnostic::cayError::CodeGen {
                code: "E5004".to_string(),
                file: None,
                line: 0,
                column: 0,
                message: format!("Inline IR parse error: {}", e),
                suggestion: "Check your inline IR syntax and variable references".to_string(),
                is_warning: false,
            }
        })?;

        // 4. 生成变量映射（支持变量名和参数索引）
        let mut input_mappings = HashMap::new();

        // 4.1 添加变量名映射
        for (name, llvm_name, _) in &available_vars {
            input_mappings.insert(name.clone(), llvm_name.clone());
        }

        // 4.2 为所有可见变量分配索引（%0, %1, %2...）
        // 顺序：参数按声明顺序，然后是局部变量
        let param_order = codegen.get_current_param_order();
        let mut idx = 0;

        // 先映射参数
        for param_name in param_order.iter() {
            if let Some((_, llvm_name, _)) = available_vars
                .iter()
                .find(|(name, _, _)| name == param_name)
            {
                input_mappings.insert(idx.to_string(), llvm_name.clone());
                idx += 1;
            }
        }

        // 再映射局部变量（非参数）
        for (name, llvm_name, _) in &available_vars {
            if !param_order.contains(name) {
                input_mappings.insert(idx.to_string(), llvm_name.clone());
                idx += 1;
            }
        }

        // 5. 转换解析后的IR为LLVM IR文本
        let llvm_ir_lines = self.convert_to_llvm_ir(&parsed_block, &input_mappings);

        // 6. 构建输出变量映射：将内联IR的输出绑定到Cavvy变量
        // 输出绑定格式：LLVM IR结果名 -> Cavvy变量类型
        // 这些输出值可以在后续的Cavvy代码中通过变量名引用
        let mut output_mappings = HashMap::new();
        for (llvm_result_name, _ir_type) in &parsed_block.outputs {
            // 将LLVM结果名映射回Cavvy变量名
            // 输出变量使用 %result_N 格式，在内联IR中定义
            output_mappings.insert(llvm_result_name.clone(), llvm_result_name.clone());
        }

        Ok(InlineIrResult {
            llvm_ir_lines,
            input_mappings,
            output_mappings,
        })
    }

    /// 收集当前作用域的变量信息
    ///
    /// 从CodeGen的作用域管理器中提取所有可见变量
    fn collect_scope_variables(&self, codegen: &IRGenerator) -> Vec<(String, String, Type)> {
        let mut vars = Vec::new();
        let class_name = codegen.get_current_class();

        // 获取参数变量
        for (name, var_scope) in codegen.get_all_scope_vars() {
            let cay_type = codegen.get_var_cay_type(&name).unwrap_or(Type::Void);

            // 对于参数，使用原始LLVM参数名（如 TestInlineIrBasic.a）
            // 而不是alloca创建的变量名（如 a_s1）
            let llvm_name = if var_scope.is_parameter {
                format!("{}.{}", class_name, name)
            } else {
                var_scope.llvm_name.clone()
            };

            vars.push((name, llvm_name, cay_type));
        }

        vars
    }

    /// 将Cavvy类型转换为IR类型
    fn convert_type_to_ir(&self, ty: &Type) -> crate::ir::types::IrType {
        match ty {
            Type::Int32 => crate::ir::types::IrType::I32,
            Type::Int64 => crate::ir::types::IrType::I64,
            Type::Float32 => crate::ir::types::IrType::F32,
            Type::Float64 => crate::ir::types::IrType::F64,
            Type::Bool => crate::ir::types::IrType::I1,
            Type::Char => crate::ir::types::IrType::I8,
            Type::String => {
                crate::ir::types::IrType::Pointer(Box::new(crate::ir::types::IrType::I8))
            }
            Type::Array(elem_ty) => {
                crate::ir::types::IrType::Pointer(Box::new(self.convert_type_to_ir(elem_ty)))
            }
            Type::Object(class_name) => {
                crate::ir::types::IrType::Pointer(Box::new(crate::ir::types::IrType::Struct {
                    name: class_name.clone(),
                    fields: Vec::new(),
                }))
            }
            _ => crate::ir::types::IrType::I32, // 默认i32
        }
    }

    /// 将解析后的内联IR块转换为LLVM IR文本
    ///
    /// 进行变量名替换，将Cavvy变量名替换为LLVM寄存器名
    fn convert_to_llvm_ir(
        &self,
        block: &crate::ir::inline_ir::InlineIrBlock,
        var_mappings: &HashMap<String, String>,
    ) -> Vec<String> {
        let mut result = Vec::new();

        for line in &block.raw_lines {
            let processed_line = self.replace_variables(line, var_mappings);
            result.push(processed_line);
        }

        result
    }

    /// 替换行中的变量引用
    ///
    /// 将 `%varname` 替换为实际的LLVM寄存器名 `%varname_s1`
    ///
    /// # 算法
    /// 使用简单的词法分析，识别 `%identifier` 模式并替换
    ///
    /// # 复杂度
    /// - 时间: O(n * m * k)，n为行长度，m为映射数，k为平均变量名长度
    /// - 空间: O(n)
    fn replace_variables(&self, line: &str, mappings: &HashMap<String, String>) -> String {
        let mut result = String::with_capacity(line.len() * 2);
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;

        while i < chars.len() {
            if chars[i] == '%' {
                // 检查是否是寄存器引用
                let start = i + 1;
                let mut end = start;

                // 读取标识符: [a-zA-Z0-9_]+
                while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
                    end += 1;
                }

                if start < end {
                    let var_name: String = chars[start..end].iter().collect();

                    // 查找映射
                    if let Some(llvm_name) = mappings.get(&var_name) {
                        result.push('%');
                        result.push_str(llvm_name);
                    } else {
                        // 未找到映射，保持原样
                        result.push('%');
                        result.push_str(&var_name);
                    }
                    i = end;
                } else {
                    // 单独的 %
                    result.push('%');
                    i += 1;
                }
            } else {
                result.push(chars[i]);
                i += 1;
            }
        }

        result
    }
}

impl Default for InlineIrBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// 为IRGenerator添加协作桥支持
pub trait InlineIrBridgeSupport {
    /// 使用协作桥处理内联IR
    fn process_inline_ir_with_bridge(&self, inline_ir: &InlineIrStmt) -> cayResult<InlineIrResult>;
}

impl InlineIrBridgeSupport for IRGenerator {
    fn process_inline_ir_with_bridge(&self, inline_ir: &InlineIrStmt) -> cayResult<InlineIrResult> {
        let bridge = InlineIrBridge::new();
        bridge.process_inline_ir(self, inline_ir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variable_replacement() {
        let bridge = InlineIrBridge::new();
        let mut mappings = HashMap::new();
        mappings.insert("a".to_string(), "a_s1".to_string());
        mappings.insert("result".to_string(), "result_s2".to_string());

        let line = "%sum = add i32 %a, %b";
        let result = bridge.replace_variables(line, &mappings);
        assert_eq!(result, "%sum = add i32 %a_s1, %b");

        let line2 = "store i32 %sum, i32* %result";
        let result2 = bridge.replace_variables(line2, &mappings);
        assert_eq!(result2, "store i32 %sum, i32* %result_s2");
    }

    #[test]
    fn test_variable_replacement_with_numbers() {
        let bridge = InlineIrBridge::new();
        let mut mappings = HashMap::new();
        mappings.insert("0".to_string(), "a_s1".to_string());
        mappings.insert("1".to_string(), "b_s1".to_string());
        mappings.insert("2".to_string(), "result_s2".to_string());

        let line = "%sum = add i32 %0, %1";
        let result = bridge.replace_variables(line, &mappings);
        assert_eq!(result, "%sum = add i32 %a_s1, %b_s1");

        let line2 = "store i32 %sum, i32* %2";
        let result2 = bridge.replace_variables(line2, &mappings);
        assert_eq!(result2, "store i32 %sum, i32* %result_s2");
    }

    #[test]
    fn test_no_replacement_for_unknown_vars() {
        let bridge = InlineIrBridge::new();
        let mappings = HashMap::new();

        let line = "%sum = add i32 %a, %b";
        let result = bridge.replace_variables(line, &mappings);
        assert_eq!(result, "%sum = add i32 %a, %b");
    }
}
