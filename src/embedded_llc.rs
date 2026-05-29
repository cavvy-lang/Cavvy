//! 嵌入式 LLVM LLC 编译器模块
//!
//! 该模块通过 llvm-sys 提供内嵌的 llc 功能，避免调用外部进程，
//! 从而提高编译速度和可靠性。
//!
//! 时间复杂度: O(n) 其中 n 为 IR 代码行数
//! 空间复杂度: O(m) 其中 m 为目标代码大小

use std::path::Path;
use std::ffi::CString;

/// 编译选项
#[derive(Debug, Clone)]
pub struct EmbeddedLlcOptions {
    /// 优化级别 (0-3)
    pub opt_level: u32,
    /// 目标三元组
    pub target_triple: String,
    /// CPU 型号
    pub cpu: Option<String>,
    /// 特性字符串
    pub features: Option<String>,
    /// 位置无关代码
    pub position_independent: bool,
}

impl Default for EmbeddedLlcOptions {
    fn default() -> Self {
        Self {
            opt_level: 2,
            target_triple: get_default_target_triple(),
            cpu: None,
            features: None,
            position_independent: false,
        }
    }
}

/// 获取默认目标三元组
fn get_default_target_triple() -> String {
    if cfg!(target_os = "windows") {
        "x86_64-w64-mingw32".to_string()
    } else if cfg!(target_os = "linux") {
        "x86_64-unknown-linux-gnu".to_string()
    } else if cfg!(target_os = "macos") {
        "x86_64-apple-darwin".to_string()
    } else {
        "x86_64-unknown-linux-gnu".to_string()
    }
}

/// 检查嵌入式 LLVM 支持是否可用
/// 现在 llvm-sys 是默认依赖，此函数始终返回 true
pub fn is_embedded_llvm_available() -> bool {
    true
}

/// 使用嵌入式 LLVM 将 IR 编译为目标文件
///
/// # Arguments
/// * `ir_content` - LLVM IR 代码内容
/// * `output_path` - 输出目标文件路径
/// * `options` - 编译选项
///
/// # Returns
/// 成功返回 Ok(())，失败返回 Err(String)
///
/// # 复杂度
/// 时间: O(n) - 线性扫描 IR 代码
/// 空间: O(m) - 与目标代码大小成正比
pub fn compile_ir_to_object(
    ir_content: &str,
    output_path: &Path,
    options: &EmbeddedLlcOptions,
) -> Result<(), String> {
    use llvm_sys::core::*;
    use llvm_sys::target::*;
    use llvm_sys::target_machine::*;
    use llvm_sys::ir_reader::*;
    use llvm_sys::analysis::*;
    use std::ptr;

    // 初始化 LLVM
    unsafe {
        LLVM_InitializeAllTargets();
        LLVM_InitializeAllTargetMCs();
        LLVM_InitializeAllAsmPrinters();
        LLVM_InitializeAllAsmParsers();
        LLVM_InitializeAllDisassemblers();
    }

    // 创建 LLVM 上下文
    let context = unsafe { LLVMContextCreate() };
    if context.is_null() {
        return Err("无法创建 LLVM 上下文".to_string());
    }

    // 使用 RAII 确保上下文被释放
    struct ContextGuard(*mut llvm_sys::LLVMContext);
    impl Drop for ContextGuard {
        fn drop(&mut self) {
            unsafe { LLVMContextDispose(self.0) }
        }
    }
    let _context_guard = ContextGuard(context);

    // 将 IR 内容转换为 CString
    let ir_cstring = CString::new(ir_content)
        .map_err(|e| format!("IR 内容包含空字节: {}", e))?;

    // 创建内存缓冲区
    let buffer = unsafe {
        LLVMCreateMemoryBufferWithMemoryRangeCopy(
            ir_cstring.as_ptr() as *const i8,
            ir_content.len(),
            b"cavvy_ir\0".as_ptr() as *const i8,
        )
    };
    if buffer.is_null() {
        return Err("无法创建内存缓冲区".to_string());
    }

    struct BufferGuard(*mut llvm_sys::LLVMMemoryBuffer);
    impl Drop for BufferGuard {
        fn drop(&mut self) {
            unsafe { LLVMDisposeMemoryBuffer(self.0) }
        }
    }
    let _buffer_guard = BufferGuard(buffer);

    // 解析 IR - 使用 LLVMParseIRInContext (在 ir_reader 模块中)
    let mut module: *mut llvm_sys::LLVMModule = ptr::null_mut();
    let mut error_msg: *mut i8 = ptr::null_mut();

    let parse_result = unsafe {
        LLVMParseIRInContext(
            context,
            buffer,
            &mut module,
            &mut error_msg,
        )
    };

    if parse_result != 0 {
        let error = if !error_msg.is_null() {
            let msg = unsafe {
                let c_str = std::ffi::CStr::from_ptr(error_msg);
                c_str.to_string_lossy().to_string()
            };
            unsafe { LLVMDisposeMessage(error_msg) };
            msg
        } else {
            "未知解析错误".to_string()
        };
        return Err(format!("IR 解析失败: {}", error));
    }

    struct ModuleGuard(*mut llvm_sys::LLVMModule);
    impl Drop for ModuleGuard {
        fn drop(&mut self) {
            unsafe { LLVMDisposeModule(self.0) }
        }
    }
    let _module_guard = ModuleGuard(module);

    // 验证模块
    let mut verify_msg: *mut i8 = ptr::null_mut();
    let verify_result = unsafe {
        LLVMVerifyModule(
            module,
            LLVMVerifierFailureAction::LLVMReturnStatusAction,
            &mut verify_msg,
        )
    };

    if verify_result != 0 {
        let error = if !verify_msg.is_null() {
            let msg = unsafe {
                let c_str = std::ffi::CStr::from_ptr(verify_msg);
                c_str.to_string_lossy().to_string()
            };
            unsafe { LLVMDisposeMessage(verify_msg) };
            msg
        } else {
            "模块验证失败".to_string()
        };
        return Err(format!("IR 验证失败: {}", error));
    }

    if !verify_msg.is_null() {
        unsafe { LLVMDisposeMessage(verify_msg) };
    }

    // 获取目标三元组
    let target_triple = CString::new(options.target_triple.as_str())
        .map_err(|e| format!("目标三元组包含空字节: {}", e))?;

    // 查找目标 - 使用 target_machine 模块中的函数
    let mut target: *mut llvm_sys::target_machine::LLVMTarget = ptr::null_mut();
    let mut target_error: *mut i8 = ptr::null_mut();

    let lookup_result = unsafe {
        LLVMGetTargetFromTriple(
            target_triple.as_ptr(),
            &mut target,
            &mut target_error,
        )
    };

    if lookup_result != 0 {
        let error = if !target_error.is_null() {
            let msg = unsafe {
                let c_str = std::ffi::CStr::from_ptr(target_error);
                c_str.to_string_lossy().to_string()
            };
            unsafe { LLVMDisposeMessage(target_error) };
            msg
        } else {
            format!("不支持的目标: {}", options.target_triple)
        };
        return Err(format!("目标查找失败: {}", error));
    }

    if target.is_null() {
        return Err("无法获取目标".to_string());
    }

    // 设置 CPU
    let cpu = options.cpu.as_ref()
        .map(|s| CString::new(s.as_str()).ok())
        .flatten()
        .unwrap_or_else(|| CString::new("generic").unwrap());

    // 设置特性
    let features = options.features.as_ref()
        .map(|s| CString::new(s.as_str()).ok())
        .flatten()
        .unwrap_or_else(|| CString::new("").unwrap());

    // 创建目标机器
    let reloc_mode = if options.position_independent {
        LLVMRelocMode::LLVMRelocPIC
    } else {
        LLVMRelocMode::LLVMRelocDefault
    };

    // 转换优化级别
    let opt_level = match options.opt_level {
        0 => LLVMCodeGenOptLevel::LLVMCodeGenLevelNone,
        1 => LLVMCodeGenOptLevel::LLVMCodeGenLevelLess,
        2 => LLVMCodeGenOptLevel::LLVMCodeGenLevelDefault,
        3 => LLVMCodeGenOptLevel::LLVMCodeGenLevelAggressive,
        _ => LLVMCodeGenOptLevel::LLVMCodeGenLevelDefault,
    };

    let target_machine = unsafe {
        LLVMCreateTargetMachine(
            target,
            target_triple.as_ptr(),
            cpu.as_ptr(),
            features.as_ptr(),
            opt_level,
            reloc_mode,
            LLVMCodeModel::LLVMCodeModelDefault,
        )
    };

    if target_machine.is_null() {
        return Err("无法创建目标机器".to_string());
    }

    // 设置模块的数据布局
    unsafe {
        let data_layout = LLVMCreateTargetDataLayout(target_machine);
        if !data_layout.is_null() {
            LLVMSetModuleDataLayout(module, data_layout);
            LLVMDisposeTargetData(data_layout);
        }
    }

    // 设置目标三元组
    unsafe {
        LLVMSetTarget(module, target_triple.as_ptr());
    }

    // 输出文件路径
    let output_cstring = CString::new(
        output_path.to_str()
            .ok_or("无效输出路径")?
    ).map_err(|e| format!("输出路径包含空字节: {}", e))?;

    // 编译为目标文件
    let mut codegen_error: *mut i8 = ptr::null_mut();
    let emit_result = unsafe {
        LLVMTargetMachineEmitToFile(
            target_machine,
            module,
            output_cstring.as_ptr() as *mut i8,
            LLVMCodeGenFileType::LLVMObjectFile,
            &mut codegen_error,
        )
    };

    // 释放目标机器
    unsafe { LLVMDisposeTargetMachine(target_machine); }

    if emit_result != 0 {
        let error = if !codegen_error.is_null() {
            let msg = unsafe {
                let c_str = std::ffi::CStr::from_ptr(codegen_error);
                c_str.to_string_lossy().to_string()
            };
            unsafe { LLVMDisposeMessage(codegen_error) };
            msg
        } else {
            "代码生成失败".to_string()
        };
        return Err(format!("目标文件生成失败: {}", error));
    }

    if !codegen_error.is_null() {
        unsafe { LLVMDisposeMessage(codegen_error) };
    }

    Ok(())
}

/// 从 ir2exe 优化级别字符串转换为数值
pub fn parse_opt_level(opt: &str) -> u32 {
    match opt {
        "-O0" => 0,
        "-O1" => 1,
        "-O2" => 2,
        "-O3" => 3,
        "-Os" => 2, // 大小优化使用 O2
        "-Oz" => 2,
        _ => 2,
    }
}

/// 从 ir2exe 选项创建嵌入式 LLC 选项
pub fn options_from_ir2exe(
    opt_level_str: &str,
    target: &str,
    march: Option<&str>,
    position_independent: bool,
) -> EmbeddedLlcOptions {
    EmbeddedLlcOptions {
        opt_level: parse_opt_level(opt_level_str),
        target_triple: target.to_string(),
        cpu: march.map(|s| s.to_string()),
        features: None,
        position_independent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_opt_level() {
        assert_eq!(parse_opt_level("-O0"), 0);
        assert_eq!(parse_opt_level("-O1"), 1);
        assert_eq!(parse_opt_level("-O2"), 2);
        assert_eq!(parse_opt_level("-O3"), 3);
        assert_eq!(parse_opt_level("-Os"), 2);
        assert_eq!(parse_opt_level("-Oz"), 2);
        assert_eq!(parse_opt_level("-O4"), 2); // 未知级别默认 O2
    }

    #[test]
    fn test_is_embedded_llvm_available() {
        // llvm-sys 现在是默认依赖，应该始终可用
        assert!(is_embedded_llvm_available());
    }

    #[test]
    fn test_embedded_llc_options_default() {
        let opts = EmbeddedLlcOptions::default();
        assert_eq!(opts.opt_level, 2);
        assert!(!opts.position_independent);
        assert!(opts.cpu.is_none());
        assert!(opts.features.is_none());
    }
}
