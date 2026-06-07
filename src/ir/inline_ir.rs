//! 内联 IR 支持
//!
//! 提供两种内联 IR 机制：
//! 1. **源码内联 IR**: 在 Cavvy 源码中用 `__ir { ... }` 语法嵌入原始 LLVM IR
//! 2. **编程内联 IR**: 在 IR Builder 中直接插入原始 IR 片段

use super::types::IrType;
use super::value::{IrInstruction, IrValue};

/// 内联 IR 块 - 表示 Cavvy 源码中的 `__ir { ... }` 块
///
/// # 语法
///
/// ```cay
/// __ir {
///     %result = add i32 %x, %y
///     ret i32 %result
/// }
/// ```
///
/// # 特性
///
/// - 支持引用外层 Cavvy 变量（通过名称绑定）
/// - 支持产生输出值供外层使用
/// - 自动进行简单的安全检查
#[derive(Debug, Clone)]
pub struct InlineIrBlock {
    /// 原始 IR 文本行
    pub raw_lines: Vec<String>,
    /// 输入绑定：Cavvy 变量名 → LLVM IR 值
    pub inputs: Vec<(String, IrValue)>,
    /// 输出绑定：LLVM IR 结果名 → Cavvy 变量类型
    pub outputs: Vec<(String, IrType)>,
    /// 源位置（行号）
    pub source_line: u32,
}

/// 内联 IR 解析器
///
/// 解析 Cavvy 源码中的 `__ir { ... }` 块，
/// 执行基本的安全验证，并生成 IR 指令。
pub struct InlineIrParser {
    /// 允许的函数白名单
    allowed_functions: Vec<String>,
    /// 是否允许所有 LLVM 指令（生产环境应为 false）
    allow_all: bool,
}

impl InlineIrParser {
    /// 创建新的内联 IR 解析器（生产模式 - 严格限制）
    pub fn new() -> Self {
        Self {
            allowed_functions: vec![
                "add".to_string(),
                "sub".to_string(),
                "mul".to_string(),
                "sdiv".to_string(),
                "srem".to_string(),
                "fadd".to_string(),
                "fsub".to_string(),
                "fmul".to_string(),
                "fdiv".to_string(),
                "frem".to_string(),
                "and".to_string(),
                "or".to_string(),
                "xor".to_string(),
                "shl".to_string(),
                "ashr".to_string(),
                "lshr".to_string(),
                "icmp".to_string(),
                "fcmp".to_string(),
                "sext".to_string(),
                "zext".to_string(),
                "trunc".to_string(),
                "sitofp".to_string(),
                "fptosi".to_string(),
                "fpext".to_string(),
                "fptrunc".to_string(),
                "bitcast".to_string(),
                "ptrtoint".to_string(),
                "inttoptr".to_string(),
                "getelementptr".to_string(),
                "alloca".to_string(),
                "load".to_string(),
                "store".to_string(),
                "call".to_string(),
                "ret".to_string(),
                "br".to_string(),
                "select".to_string(),
                "phi".to_string(),
                "switch".to_string(),
                "unreachable".to_string(),
            ],
            allow_all: false,
        }
    }

    /// 创建开发模式解析器（允许所有指令）
    pub fn new_unsafe() -> Self {
        Self {
            allowed_functions: Vec::new(),
            allow_all: true,
        }
    }

    /// 解析内联 IR 文本并验证安全性
    ///
    /// # Arguments
    /// * `ir_text` - `__ir { ... }` 中的原始文本
    /// * `available_inputs` - 可用的 Cavvy 变量及其 IR 值映射
    /// * `expected_outputs` - 期望的输出变量名和类型
    ///
    /// # Returns
    /// 验证后的内联 IR 块
    pub fn parse(
        &self,
        ir_text: &str,
        available_inputs: &[(String, IrValue)],
        expected_outputs: &[(String, IrType)],
    ) -> Result<InlineIrBlock, String> {
        // eprintln!("DEBUG InlineIrParser::parse: ir_text = '{}'", ir_text);

        let lines: Vec<String> = ir_text
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty() && !l.starts_with(';'))
            .collect();

        // eprintln!("DEBUG InlineIrParser::parse: lines = {:?}", lines);

        if lines.is_empty() {
            return Err("Inline IR block is empty".to_string());
        }

        // 验证每条指令
        for (i, line) in lines.iter().enumerate() {
            self.validate_instruction(line, i + 1)?;
        }

        // 构建输入绑定
        let inputs: Vec<(String, IrValue)> = available_inputs.to_vec();

        Ok(InlineIrBlock {
            raw_lines: lines,
            inputs,
            outputs: expected_outputs.to_vec(),
            source_line: 0,
        })
    }

    /// 将内联 IR 块转换为 IR 指令
    pub fn to_instruction(&self, block: &InlineIrBlock) -> IrInstruction {
        let output_values: Vec<IrValue> = block
            .outputs
            .iter()
            .map(|(name, ty)| IrValue::Register(name.clone(), ty.clone()))
            .collect();

        let input_values: Vec<IrValue> = block.inputs.iter().map(|(_, val)| val.clone()).collect();

        IrInstruction::InlineIr {
            lines: block.raw_lines.clone(),
            outputs: output_values,
            inputs: input_values,
        }
    }

    /// 验证单条 LLVM IR 指令的安全性
    fn validate_instruction(&self, line: &str, line_num: usize) -> Result<(), String> {
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            return Ok(());
        }

        // 检查是否是标签定义
        if line.ends_with(':') {
            return Ok(());
        }

        // 提取指令操作码
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.is_empty() {
            return Ok(());
        }

        // 处理赋值形式: %result = opcode ...
        let opcode_idx = if parts.len() > 2 && parts[1] == "=" {
            2
        } else {
            0
        };

        let opcode = parts.get(opcode_idx).unwrap_or(&"");

        // 检查是否在允许列表中（仅在非 unsafe 模式下）
        if !self.allow_all && !self.allowed_functions.iter().any(|f| f == opcode) {
            return Err(format!(
                "Line {}: Disallowed LLVM instruction '{}' in inline IR. \
                 Only arithmetic, conversion, and basic control flow instructions are permitted.",
                line_num, opcode
            ));
        }

        // 特殊安全检查（始终执行）
        self.security_check(line, opcode, line_num)?;

        Ok(())
    }

    /// 安全策略检查
    fn security_check(&self, _line: &str, opcode: &str, line_num: usize) -> Result<(), String> {
        // 安全检查始终执行，不受 allow_all 影响
        match opcode {
            "call" => {
                // 检查不调用危险的 C 函数
                let dangerous = [
                    "system", "exec", "popen", "fork", "vfork", "dlopen", "dlsym", "dlclose",
                    "mmap", "munmap", "mprotect",
                ];
                for d in &dangerous {
                    if _line.contains(d) {
                        return Err(format!(
                            "Line {}: Dangerous function '{}' is not allowed in inline IR",
                            line_num, d
                        ));
                    }
                }
            }
            "alloca" => {
                // 允许栈分配，但在生产环境可以限制大小
            }
            "store" | "load" => {
                // 允许内存操作，注意指针安全
            }
            _ => {}
        }
        Ok(())
    }
}

impl Default for InlineIrParser {
    fn default() -> Self {
        Self::new()
    }
}

/// 构建内联 IR 的便捷宏风格函数
///
/// 用于在 IR Builder 中创建内联 IR 指令。
///
/// # Example
///
/// ```ignore
/// let inst = inline_ir!(
///     "%result = add i32 %x, %y",
///     "ret i32 %result"
/// );
/// ```
#[macro_export]
macro_rules! inline_ir {
    ($($line:expr),* $(,)?) => {
        IrInstruction::InlineIr {
            lines: vec![$($line.to_string()),*],
            outputs: Vec::new(),
            inputs: Vec::new(),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::types::IrType;
    use crate::ir::value::IrValue;

    #[test]
    fn test_valid_inline_ir() {
        let parser = InlineIrParser::new();
        let ir = "%result = add i32 %x, %y\nret i32 %result";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_dangerous_call() {
        let parser = InlineIrParser::new();
        let ir = "call void @system(i8* %cmd)";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("system"));
    }

    #[test]
    fn test_disallowed_opcode() {
        let parser = InlineIrParser::new();
        let ir = "invoke void @foo() to label %cont unwind label %lpad";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_err());
    }

    // ============================================================
    // 算术运算指令测试
    // ============================================================

    #[test]
    fn test_inline_ir_add() {
        let parser = InlineIrParser::new();
        let ir = "%r = add i32 %a, %b";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_sub() {
        let parser = InlineIrParser::new();
        let ir = "%r = sub i32 %a, %b";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_mul() {
        let parser = InlineIrParser::new();
        let ir = "%r = mul i32 %a, %b";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_sdiv() {
        let parser = InlineIrParser::new();
        let ir = "%r = sdiv i32 %a, %b";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_srem() {
        let parser = InlineIrParser::new();
        let ir = "%r = srem i32 %a, %b";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_float_ops() {
        let parser = InlineIrParser::new();
        let ops = vec!["fadd", "fsub", "fmul", "fdiv", "frem"];
        for op in ops {
            let ir = format!("%r = {} double %a, %b", op);
            let result = parser.parse(&ir, &[], &[]);
            assert!(result.is_ok(), "{} should be allowed", op);
        }
    }

    // ============================================================
    // 位运算指令测试
    // ============================================================

    #[test]
    fn test_inline_ir_bitwise_ops() {
        let parser = InlineIrParser::new();
        let ops = vec!["and", "or", "xor", "shl", "ashr", "lshr"];
        for op in ops {
            let ir = format!("%r = {} i32 %a, %b", op);
            let result = parser.parse(&ir, &[], &[]);
            assert!(result.is_ok(), "{} should be allowed", op);
        }
    }

    // ============================================================
    // 比较指令测试
    // ============================================================

    #[test]
    fn test_inline_ir_icmp() {
        let parser = InlineIrParser::new();
        let ir = "%r = icmp eq i32 %a, %b";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_fcmp() {
        let parser = InlineIrParser::new();
        let ir = "%r = fcmp oeq double %a, %b";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    // ============================================================
    // 类型转换指令测试
    // ============================================================

    #[test]
    fn test_inline_ir_type_conversions() {
        let parser = InlineIrParser::new();
        let conversions = vec![
            ("sext", "i8", "i32"),
            ("zext", "i8", "i32"),
            ("trunc", "i32", "i8"),
            ("sitofp", "i32", "double"),
            ("fptosi", "double", "i32"),
            ("fpext", "float", "double"),
            ("fptrunc", "double", "float"),
            ("bitcast", "i32", "float"),
        ];
        for (op, from, to) in conversions {
            let ir = format!("%r = {} {} %v to {}", op, from, to);
            let result = parser.parse(&ir, &[], &[]);
            assert!(result.is_ok(), "{} should be allowed", op);
        }
    }

    #[test]
    fn test_inline_ir_ptr_conversions() {
        let parser = InlineIrParser::new();
        let ir = "%r = ptrtoint i8* %p to i64";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());

        let ir2 = "%r = inttoptr i64 %v to i8*";
        let result2 = parser.parse(ir2, &[], &[]);
        assert!(result2.is_ok());
    }

    // ============================================================
    // 内存操作指令测试
    // ============================================================

    #[test]
    fn test_inline_ir_alloca() {
        let parser = InlineIrParser::new();
        let ir = "%p = alloca i32";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_load() {
        let parser = InlineIrParser::new();
        let ir = "%v = load i32, i32* %p";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_store() {
        let parser = InlineIrParser::new();
        let ir = "store i32 %v, i32* %p";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_getelementptr() {
        let parser = InlineIrParser::new();
        let ir = "%p2 = getelementptr i32, i32* %p, i64 0";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    // ============================================================
    // 控制流指令测试
    // ============================================================

    #[test]
    fn test_inline_ir_ret() {
        let parser = InlineIrParser::new();
        let ir = "ret i32 %r";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_br() {
        let parser = InlineIrParser::new();
        let ir = "br label %block";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());

        let ir2 = "br i1 %cond, label %true, label %false";
        let result2 = parser.parse(ir2, &[], &[]);
        assert!(result2.is_ok());
    }

    #[test]
    fn test_inline_ir_switch() {
        let parser = InlineIrParser::new();
        let ir = "switch i32 %val, label %default [ i32 0, label %case0 ]";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_unreachable() {
        let parser = InlineIrParser::new();
        let ir = "unreachable";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    // ============================================================
    // 其他指令测试
    // ============================================================

    #[test]
    fn test_inline_ir_select() {
        let parser = InlineIrParser::new();
        let ir = "%r = select i1 %cond, i32 %a, i32 %b";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_phi() {
        let parser = InlineIrParser::new();
        let ir = "%r = phi i32 [ %a, %block1 ], [ %b, %block2 ]";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_call() {
        let parser = InlineIrParser::new();
        let ir = "%r = call i32 @foo(i32 %a)";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    // ============================================================
    // 输入输出绑定测试
    // ============================================================

    #[test]
    fn test_inline_ir_with_inputs() {
        let parser = InlineIrParser::new();
        let inputs = vec![
            (
                "x".to_string(),
                IrValue::Register("%x".to_string(), IrType::I32),
            ),
            (
                "y".to_string(),
                IrValue::Register("%y".to_string(), IrType::I32),
            ),
        ];
        let ir = "%r = add i32 %x, %y";
        let result = parser.parse(ir, &inputs, &[]);
        assert!(result.is_ok());

        let block = result.unwrap();
        assert_eq!(block.inputs.len(), 2);
    }

    #[test]
    fn test_inline_ir_with_outputs() {
        let parser = InlineIrParser::new();
        let outputs = vec![("%r".to_string(), IrType::I32)];
        let ir = "%r = add i32 %a, %b";
        let result = parser.parse(ir, &[], &outputs);
        assert!(result.is_ok());

        let block = result.unwrap();
        assert_eq!(block.outputs.len(), 1);
    }

    #[test]
    fn test_inline_ir_to_instruction() {
        let parser = InlineIrParser::new();
        let outputs = vec![("%r".to_string(), IrType::I32)];
        let ir = "%r = add i32 %a, %b";
        let block = parser.parse(ir, &[], &outputs).unwrap();

        let inst = parser.to_instruction(&block);
        match inst {
            crate::ir::value::IrInstruction::InlineIr {
                lines,
                outputs,
                inputs,
            } => {
                assert_eq!(lines.len(), 1);
                assert_eq!(outputs.len(), 1);
                assert!(inputs.is_empty());
            }
            _ => panic!("Expected InlineIr instruction"),
        }
    }

    // ============================================================
    // 安全检查测试
    // ============================================================

    #[test]
    fn test_inline_ir_rejects_exec() {
        let parser = InlineIrParser::new();
        let ir = "call i32 @exec(i8* %cmd)";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("exec"));
    }

    #[test]
    fn test_inline_ir_rejects_popen() {
        let parser = InlineIrParser::new();
        let ir = "call i32 @popen(i8* %cmd, i8* %mode)";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("popen"));
    }

    #[test]
    fn test_inline_ir_rejects_fork() {
        let parser = InlineIrParser::new();
        let ir = "call i32 @fork()";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("fork"));
    }

    #[test]
    fn test_inline_ir_rejects_dlopen() {
        let parser = InlineIrParser::new();
        let ir = "call i8* @dlopen(i8* %path, i32 %flags)";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("dlopen"));
    }

    #[test]
    fn test_inline_ir_rejects_mmap() {
        let parser = InlineIrParser::new();
        let ir = "call i8* @mmap(i8* %addr, i64 %len, i32 %prot, i32 %flags, i32 %fd, i64 %off)";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("mmap"));
    }

    #[test]
    fn test_inline_ir_empty_block() {
        let parser = InlineIrParser::new();
        let ir = "";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_inline_ir_comments_ignored() {
        let parser = InlineIrParser::new();
        let ir = "; this is a comment\n%r = add i32 %a, %b\n; another comment";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_label_allowed() {
        let parser = InlineIrParser::new();
        let ir = "entry:\n  %r = add i32 %a, %b";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    // ============================================================
    // unsafe 模式测试
    // ============================================================

    #[test]
    fn test_inline_ir_unsafe_mode_allows_any() {
        let parser = InlineIrParser::new_unsafe();
        let ir = "invoke void @foo() to label %cont unwind label %lpad";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_inline_ir_unsafe_mode_allows_dangerous_call() {
        let parser = InlineIrParser::new_unsafe();
        let ir = "call i32 @system(i8* %cmd)";
        let result = parser.parse(ir, &[], &[]);
        // unsafe 模式下仍然进行安全检查
        assert!(result.is_err());
    }

    // ============================================================
    // 默认实现测试
    // ============================================================

    #[test]
    fn test_inline_ir_parser_default() {
        let parser: InlineIrParser = Default::default();
        let ir = "%r = add i32 %a, %b";
        let result = parser.parse(ir, &[], &[]);
        assert!(result.is_ok());
    }
}
