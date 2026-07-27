/// 字节码混淆器
///
/// 注意：名称混淆、控制流混淆、垃圾代码插入、字符串加密等“保护”功能
/// 此前仅为占位实现 —— 名称混淆只记录映射而不修改常量池，字符串加密只写
/// 密钥而不加密任何字符串，控制流混淆插入的 iconst/Iadd 会改变操作数栈
/// 深度从而破坏程序语义，垃圾代码只 push 不 pop。这些假实现已全部移除。
/// 当前唯一可用的功能是移除调试信息；其余混淆入口一律返回明确错误，
/// 绝不让调用方误以为产物获得了保护。
use super::*;

/// 混淆错误
#[derive(Debug)]
pub enum ObfuscationError {
    /// 请求的混淆功能当前不可用
    NotAvailable(String),
}

impl std::fmt::Display for ObfuscationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObfuscationError::NotAvailable(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for ObfuscationError {}

/// 混淆不可用时的统一错误信息
const NOT_AVAILABLE_MSG: &str =
    "字节码混淆为实验性功能，当前版本不可用（此前的实现不能提供真实保护且可能破坏程序语义，已停用）";

/// 混淆选项
#[derive(Debug, Clone)]
pub struct ObfuscationOptions {
    /// 混淆符号名称
    pub obfuscate_names: bool,
    /// 混淆控制流
    pub obfuscate_control_flow: bool,
    /// 插入垃圾代码
    pub insert_junk_code: bool,
    /// 加密字符串
    pub encrypt_strings: bool,
    /// 打乱函数顺序
    pub shuffle_functions: bool,
    /// 移除调试信息
    pub strip_debug_info: bool,
}

impl Default for ObfuscationOptions {
    fn default() -> Self {
        Self {
            obfuscate_names: true,
            obfuscate_control_flow: true,
            insert_junk_code: false,
            encrypt_strings: true,
            shuffle_functions: false,
            strip_debug_info: true,
        }
    }
}

/// 字节码混淆器
pub struct BytecodeObfuscator {
    options: ObfuscationOptions,
}

impl BytecodeObfuscator {
    /// 创建新的混淆器
    pub fn new(options: ObfuscationOptions) -> Self {
        Self { options }
    }

    /// 混淆字节码模块
    ///
    /// 除“移除调试信息”外的混淆功能均为实验性、当前不可用：本方法总是
    /// 返回错误，且不修改模块（不会把 header.obfuscated 置为 true）。
    /// 调用方必须处理该错误，不得忽略后声称产物已被混淆。
    pub fn obfuscate(&mut self, _module: &mut BytecodeModule) -> Result<(), ObfuscationError> {
        Err(ObfuscationError::NotAvailable(format!(
            "{}（请求选项: {:?}）",
            NOT_AVAILABLE_MSG, self.options
        )))
    }

    /// 移除调试信息（当前唯一已实现的“混淆”功能，真实执行）
    pub fn strip_debug_info(&mut self, module: &mut BytecodeModule) {
        // 清除行号表
        for type_def in &mut module.type_definitions {
            for method in &mut type_def.methods {
                if let Some(ref mut body) = method.body {
                    body.line_number_table.clear();
                }
            }
        }

        for func in &mut module.functions {
            func.body.line_number_table.clear();
        }

        // 移除调试相关的元数据
        module.metadata.retain(|key, _| !key.starts_with("debug."));
    }
}

impl Default for BytecodeObfuscator {
    fn default() -> Self {
        Self::new(ObfuscationOptions::default())
    }
}

/// 混淆工具函数

/// 快速混淆字节码模块 —— 实验性功能，当前不可用，总是返回错误
pub fn quick_obfuscate(_module: &mut BytecodeModule) -> Result<(), ObfuscationError> {
    Err(ObfuscationError::NotAvailable(
        NOT_AVAILABLE_MSG.to_string(),
    ))
}

/// 深度混淆字节码模块 —— 实验性功能，当前不可用，总是返回错误
pub fn deep_obfuscate(_module: &mut BytecodeModule) -> Result<(), ObfuscationError> {
    Err(ObfuscationError::NotAvailable(
        NOT_AVAILABLE_MSG.to_string(),
    ))
}

/// 仅移除调试信息（真实执行）
pub fn strip_debug_info_only(module: &mut BytecodeModule) {
    let mut obfuscator = BytecodeObfuscator::new(ObfuscationOptions::default());
    obfuscator.strip_debug_info(module);
}
