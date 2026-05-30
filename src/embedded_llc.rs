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
            // Windows 上默认启用 PIC 以避免重定位截断错误
            position_independent: cfg!(target_os = "windows"),
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
/// 
/// 注意: 在 Windows 上，内嵌 llc 可能生成 ELF 格式的目标文件而不是 COFF，
/// 这会导致链接失败。因此 Windows 上建议使用外部 llc。
pub fn is_embedded_llvm_available() -> bool {
    true
}
/// 检查当前平台是否支持内嵌 llc 生成正确的目标文件格式
/// 
/// Windows 上 llvm-sys 可能生成 ELF 而不是 COFF，导致链接失败
pub fn is_embedded_llc_supported() -> bool {
    // Windows 上现在支持内嵌 llc，因为我们使用正确的目标初始化
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
    // // eprintln!("  [DEBUG] 开始 LLVM 初始化...");
    // 初始化 LLVM - 初始化所有目标（包括 MinGW）
    unsafe {
        // // eprintln!("  [DEBUG] 调用 LLVM_InitializeAllTargets...");
        LLVM_InitializeAllTargets();
        // // eprintln!("  [DEBUG] 调用 LLVM_InitializeAllTargetInfos...");
        LLVM_InitializeAllTargetInfos();
        // // eprintln!("  [DEBUG] 调用 LLVM_InitializeAllTargetMCs...");
        LLVM_InitializeAllTargetMCs();
        // // eprintln!("  [DEBUG] 调用 LLVM_InitializeAllAsmPrinters...");
        LLVM_InitializeAllAsmPrinters();
        // // eprintln!("  [DEBUG] 调用 LLVM_InitializeAllAsmParsers...");
        LLVM_InitializeAllAsmParsers();
        // // eprintln!("  [DEBUG] 调用 LLVM_InitializeAllDisassemblers...");
        LLVM_InitializeAllDisassemblers();
        
        // 额外初始化 x86 目标（MinGW 需要）
        // // eprintln!("  [DEBUG] 初始化 x86 目标...");
        llvm_sys::target::LLVMInitializeX86Target();
        llvm_sys::target::LLVMInitializeX86TargetInfo();
        llvm_sys::target::LLVMInitializeX86TargetMC();
        llvm_sys::target::LLVMInitializeX86AsmPrinter();
        llvm_sys::target::LLVMInitializeX86AsmParser();
    }
    // // eprintln!("  [DEBUG] LLVM 初始化完成");
    
    // 规范化目标三元组 - 将 x86_64-w64-mingw32 转换为 x86_64-pc-windows-gnu
    // 这样 LLVM 才能正确识别为 COFF 格式（MinGW 使用 COFF）
    let normalized_triple_str = if options.target_triple.contains("mingw") {
        "x86_64-pc-windows-gnu".to_string()
    } else {
        options.target_triple.clone()
    };
    
    // // eprintln!("  [DEBUG] 原始目标三元组: {}", options.target_triple);
    // // eprintln!("  [DEBUG] 规范化后的目标三元组: {}", normalized_triple_str);
    // // eprintln!("  [DEBUG] 创建上下文...");
    // 创建 LLVM 上下文
    let context = unsafe { LLVMContextCreate() };
    if context.is_null() {
        return Err("无法创建 LLVM 上下文".to_string());
    }
    // // eprintln!("  [DEBUG] LLVM 上下文创建成功: {:?}", context);
    // 使用 RAII 确保上下文被释放
    struct ContextGuard(*mut llvm_sys::LLVMContext);
    impl Drop for ContextGuard {
        fn drop(&mut self) {
            unsafe { LLVMContextDispose(self.0) }
        }
    }
    let _context_guard = ContextGuard(context);
    // // eprintln!("  [DEBUG] IR 内容长度: {} 字节", ir_content.len());
    // // eprintln!("  [DEBUG] 创建内存缓冲区...");
    // 创建内存缓冲区 - 直接使用 ir_content 的指针，避免 CString 转换
    // 注意: LLVMCreateMemoryBufferWithMemoryRangeCopy 会复制数据
    let buffer_name = CString::new("cavvy_ir").unwrap();
    let buffer = unsafe {
        LLVMCreateMemoryBufferWithMemoryRangeCopy(
            ir_content.as_ptr() as *const libc::c_char,
            ir_content.len(),
            buffer_name.as_ptr(),
        )
    };
    if buffer.is_null() {
        return Err("无法创建内存缓冲区".to_string());
    }
    // // eprintln!("  [DEBUG] 内存缓冲区创建成功: {:?}", buffer);
    struct BufferGuard(*mut llvm_sys::LLVMMemoryBuffer);
    impl Drop for BufferGuard {
        fn drop(&mut self) {
            unsafe { LLVMDisposeMemoryBuffer(self.0) }
        }
    }
    let _buffer_guard = BufferGuard(buffer);
    // // eprintln!("  [DEBUG] 开始解析 IR...");
    // 解析 IR - 使用 LLVMParseIRInContext2 (新版API，LLVMParseIRInContext 已弃用)
    // 注意: LLVMParseIRInContext2 返回 LLVMBool，0 表示成功
    let mut module: *mut llvm_sys::LLVMModule = ptr::null_mut();
    let mut error_msg: *mut i8 = ptr::null_mut();
    let parse_result = unsafe {
        LLVMParseIRInContext2(
            context,
            buffer,
            &mut module,
            &mut error_msg,
        )
    };
    // // eprintln!("  [DEBUG] IR 解析结果: {}", parse_result);
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
    // // eprintln!("  [DEBUG] IR 解析成功，模块指针: {:?}", module);
    struct ModuleGuard(*mut llvm_sys::LLVMModule);
    impl Drop for ModuleGuard {
        fn drop(&mut self) {
            unsafe { LLVMDisposeModule(self.0) }
        }
    }
    let _module_guard = ModuleGuard(module);
    // // eprintln!("  [DEBUG] 开始验证模块...");
    // 验证模块
    let mut verify_msg: *mut i8 = ptr::null_mut();
    let verify_result = unsafe {
        LLVMVerifyModule(
            module,
            LLVMVerifierFailureAction::LLVMReturnStatusAction,
            &mut verify_msg,
        )
    };

    // eprintln!("  [DEBUG] 模块验证结果: {}", verify_result);

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

    // 注意: 当使用 LLVMReturnStatusAction 且验证成功时，
    // verify_msg 应该为 null。如果非 null，可能是内存损坏或LLVM版本差异。
    // 暂时跳过释放，避免崩溃。这是一个已知的LLVM C API问题。
    if !verify_msg.is_null() {
        eprintln!("  [WARN] verify_msg 非空但验证成功，跳过释放以避免崩溃");
    }

    // eprintln!("  [DEBUG] 模块验证成功");

    // 获取规范化后的目标三元组
    let target_triple = CString::new(normalized_triple_str.as_str())
        .map_err(|e| format!("目标三元组包含空字节: {}", e))?;

    // eprintln!("  [DEBUG] 查找目标平台...");

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

    // eprintln!("  [DEBUG] 目标查找结果: {}", lookup_result);
    
    // 获取目标名称
    if !target.is_null() {
        let target_name = unsafe {
            let name_ptr = LLVMGetTargetName(target);
            std::ffi::CStr::from_ptr(name_ptr).to_string_lossy().to_string()
        };
        let target_desc = unsafe {
            let desc_ptr = LLVMGetTargetDescription(target);
            std::ffi::CStr::from_ptr(desc_ptr).to_string_lossy().to_string()
        };
        // eprintln!("  [DEBUG] 目标名称: {}, 描述: {}", target_name, target_desc);
    }

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

    // eprintln!("  [DEBUG] 目标平台查找成功，创建目标机器...");

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
    // Windows 目标需要 PIC 模式以避免 32 位绝对重定位截断错误
    let is_windows = options.target_triple.contains("windows") || options.target_triple.contains("mingw");
    let reloc_mode = if options.position_independent || is_windows {
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

    // eprintln!("  [DEBUG] 创建目标机器: opt_level={:?}, reloc_mode={:?}", opt_level, reloc_mode);

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

    // eprintln!("  [DEBUG] 目标机器创建成功: {:?}", target_machine);
    
    // 首先设置模块的目标三元组（在设置数据布局之前）
    // eprintln!("  [DEBUG] 设置模块目标三元组...");
    unsafe {
        LLVMSetTarget(module, target_triple.as_ptr());
    }
    
    // 获取并设置模块的数据布局
    // eprintln!("  [DEBUG] 设置模块数据布局...");
    unsafe {
        let data_layout = LLVMCreateTargetDataLayout(target_machine);
        // eprintln!("  [DEBUG] 数据布局: {:?}", data_layout);
        if !data_layout.is_null() {
            LLVMSetModuleDataLayout(module, data_layout);
            LLVMDisposeTargetData(data_layout);
        }
    }

    // 输出文件路径
    let output_cstring = CString::new(
        output_path.to_str()
            .ok_or("无效输出路径")?
    ).map_err(|e| format!("输出路径包含空字节: {}", e))?;

    // eprintln!("  [DEBUG] 开始编译为目标文件: {:?}", output_path);

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

    // eprintln!("  [DEBUG] 编译结果: {}", emit_result);

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
        // Windows 上默认启用 PIC 以避免重定位截断错误
        assert_eq!(opts.position_independent, cfg!(target_os = "windows"));
        assert!(opts.cpu.is_none());
        assert!(opts.features.is_none());
    }
}
