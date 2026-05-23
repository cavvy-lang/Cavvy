//! 运行时支持函数生成模块
//!
//! 本模块包含所有 cay 运行时支持函数的 LLVM IR 生成。
//! 每个运行时函数都有独立的子模块。

use crate::codegen::context::IRGenerator;

// 子模块声明
mod string_concat;
mod float_to_string;
mod int_to_string;
mod bool_to_string;
mod char_to_string;
mod string_length;
mod string_substring;
mod string_indexof;
mod string_lastindexof;
mod string_startswith;
mod string_endswith;
mod string_charat;
mod string_replace;
mod string_isempty;
mod string_equals;
mod buffer_to_string;
mod ptr_operations;
mod args_support;

impl IRGenerator {
    /// 发射IR头部（外部声明和运行时函数）
    pub fn emit_header(&mut self) {
        self.emit_raw("; cay (Ethernos Object Language) Generated LLVM IR");
        
        // 根据目标平台设置目标三元组
        let target_triple = if let Some(config) = &self.platform_config {
            match config.target_os.as_str() {
                "windows" => "x86_64-w64-mingw32",
                "linux" => "x86_64-unknown-linux-gnu",
                "macos" => "x86_64-apple-darwin",
                _ => "x86_64-unknown-linux-gnu"
            }
        } else if cfg!(target_os = "windows") {
            "x86_64-w64-mingw32"
        } else if cfg!(target_os = "linux") {
            "x86_64-unknown-linux-gnu"
        } else if cfg!(target_os = "macos") {
            "x86_64-apple-darwin"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        self.emit_raw(&format!("target triple = \"{}\"", target_triple));
        self.emit_raw("");

        // DWARF 调试信息模块级引用
        self.emit_debug_header();

        // 声明外部函数 (printf 和标准C库函数)
        // 注意：这些声明会被标记为已发射，以避免与用户代码中的重复声明冲突
        // 签名格式: 函数名@返回类型@参数1@参数2@...@...
        if !self.is_extern_emitted("printf@i32@i8*@...") {
            self.emit_raw("declare i32 @printf(i8*, ...)");
            self.mark_extern_emitted("printf@i32@i8*@...".to_string());
        }
        if !self.is_extern_emitted("scanf@i32@i8*@...") {
            self.emit_raw("declare i32 @scanf(i8*, ...)");
            self.mark_extern_emitted("scanf@i32@i8*@...".to_string());
        }
        
        // 根据平台配置声明平台特定函数
        let platform_declarations = if let Some(config) = &self.platform_config {
            let mut declarations = String::new();
            match config.target_os.as_str() {
                "windows" => {
                    // Windows 平台总是声明 SetConsoleOutputCP，因为 generator.rs 中总是调用它
                    declarations.push_str("declare dllimport void @SetConsoleOutputCP(i32)\n");
                    if config.is_defined("WINDOWS_SPECIFIC") {
                        declarations.push_str("declare void @WindowsSpecificInit()\n");
                    }
                }
                "linux" | "macos" => {
                    if config.is_feature_enabled("console_utf8") {
                        declarations.push_str("declare i8* @setlocale(i32, i8*)\n");
                        declarations.push_str("@.str.locale = private unnamed_addr constant [6 x i8] c\"C.UTF-8\"\00\n");
                    }
                    if config.is_defined("LINUX_SPECIFIC") {
                        declarations.push_str("declare void @LinuxSpecificInit()\n");
                    }
                    if config.is_defined("MACOS_SPECIFIC") {
                        declarations.push_str("declare void @MacOSSpecificInit()\n");
                    }
                }
                _ => {}
            }
            declarations
        } else if self.target_triple.contains("windows") || self.target_triple.contains("mingw32") {
            // 向后兼容：如果没有平台配置，使用目标三元组判断
            "declare void @SetConsoleOutputCP(i32)\n".to_string()
        } else {
            "".to_string()
        };
        
        // 发射宏定义
        if let Some(config) = &self.platform_config {
            let mut has_macros = false;
            let defines = config.defines.clone(); // 克隆以避免借用冲突
            let undefines = config.undefines.clone(); // 克隆以避免借用冲突
            
            for define in &defines {
                if !undefines.contains(define) {
                    self.emit_raw(&format!("; #define {}", define));
                    has_macros = true;
                }
            }
            if has_macros {
                self.emit_raw("");
            }
        }

        // 发射平台特定声明
        if !platform_declarations.is_empty() {
            self.emit_raw(&platform_declarations);
        }
        
        // 声明外部C库函数（使用签名检查避免重复声明）
        // 签名格式: 函数名@返回类型@参数1@参数2@...
        let extern_decls = vec![
            ("strlen", "i64", vec!["i8*"], "declare i64 @strlen(i8*)"),
            ("strcmp", "i32", vec!["i8*", "i8*"], "declare i32 @strcmp(i8*, i8*)"),
            ("calloc", "i8*", vec!["i64", "i64"], "declare i8* @calloc(i64, i64)"),
            ("exit", "void", vec!["i32"], "declare void @exit(i32)"),
            ("atoi", "i32", vec!["i8*"], "declare i32 @atoi(i8*)"),
            ("snprintf", "i32", vec!["i8*", "i64", "i8*", "..."], "declare i32 @snprintf(i8*, i64, i8*, ...)"),
            ("fgets", "i8*", vec!["i8*", "i32", "i8*"], "declare i8* @fgets(i8*, i32, i8*)"),
        ];
        
        for (name, ret, params, decl) in extern_decls {
            let sig = if params.contains(&"...") {
                format!("{}@{}@{}@...", name, ret, params[..params.len()-1].join("@"))
            } else if params.is_empty() {
                format!("{}@{}@void", name, ret)
            } else {
                format!("{}@{}@{}" , name, ret, params.join("@"))
            };
            if !self.is_extern_emitted(&sig) {
                self.emit_raw(decl);
                self.mark_extern_emitted(sig);
            }
        }
        
        // llvm.memcpy 是内部函数，不需要检查重复
        self.emit_raw("declare void @llvm.memcpy.p0i8.p0i8.i64(i8* noalias nocapture writeonly, i8* noalias nocapture readonly, i64, i1 immarg)");
        
        // Windows平台使用 __acrt_iob_func 获取stdin, Linux/macOS使用外部全局变量
        if target_triple.contains("windows") || target_triple.contains("mingw") {
            if !self.is_extern_emitted("__acrt_iob_func@i8*@i32") {
                self.emit_raw("declare i8* @__acrt_iob_func(i32)");
                self.mark_extern_emitted("__acrt_iob_func@i8*@i32".to_string());
            }
        } else {
            // stdin 是全局变量，不是函数，使用不同的检查机制
            self.emit_raw("@stdin = external global i8*");
        }
        self.emit_raw("@.str.float_fmt = private unnamed_addr constant [3 x i8] c\"%f\\00\", align 1");
        self.emit_raw("@.str.double_fmt = private unnamed_addr constant [4 x i8] c\"%lf\\00\", align 1");
        self.emit_raw("@.str.int_fmt = private unnamed_addr constant [3 x i8] c\"%d\\00\", align 1");
        self.emit_raw("@.str.long_fmt = private unnamed_addr constant [5 x i8] c\"%lld\\00\", align 1");
        self.emit_raw("@.str.true_str = private unnamed_addr constant [5 x i8] c\"true\\00\", align 1");
        self.emit_raw("@.str.false_str = private unnamed_addr constant [6 x i8] c\"false\\00\", align 1");
        self.emit_raw("");

        // 空字符串常量（用于 null 安全）
        self.emit_raw("@.cay_empty_str = private unnamed_addr constant [1 x i8] c\"\\00\", align 1");
        self.emit_raw("");

        // 生成运行时函数
        self.emit_string_concat_runtime();
        self.emit_float_to_string_runtime();
        self.emit_int_to_string_runtime();
        self.emit_bool_to_string_runtime();
        self.emit_char_to_string_runtime();
        self.emit_string_length_runtime();
        self.emit_string_substring_runtime();
        self.emit_string_indexof_runtime();
        self.emit_string_lastindexof_runtime();
        self.emit_string_startswith_runtime();
        self.emit_string_endswith_runtime();
        self.emit_string_charat_runtime();
        self.emit_string_replace_runtime();
        self.emit_string_isempty_runtime();
        self.emit_string_equals_runtime();
        self.emit_buffer_to_string_runtime();

        // 生成指针操作运行时函数
        self.emit_read_ptr_runtime();
        self.emit_ptr_to_string_runtime();
        self.emit_write_ptr_runtime();
        self.emit_write_int_runtime();
        self.emit_read_int_runtime();
        self.emit_write_byte_runtime();

        // 生成命令行参数支持运行时函数
        self.emit_args_support_runtime();

        // 生成内存操作函数
        self.emit_memory_runtime();
    }
    
    /// 生成内存操作运行时函数
    fn emit_memory_runtime(&mut self) {
        // __cay_memset_byte: 按字节设置内存 (使用i64指针参数，兼容现有代码)
        self.emit_raw("define void @__cay_memset_byte(i64 %ptr, i32 %value, i32 %n) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 空指针安全检查");
        self.emit_raw("  %is_null = icmp eq i64 %ptr, 0");
        self.emit_raw("  br i1 %is_null, label %return, label %do_memset");
        self.emit_raw("");
        self.emit_raw("do_memset:");
        self.emit_raw("  %ptr_i8 = inttoptr i64 %ptr to i8*");
        self.emit_raw("  %val_i8 = trunc i32 %value to i8");
        self.emit_raw("  %n_i64 = sext i32 %n to i64");
        self.emit_raw("  call void @llvm.memset.p0i8.i64(i8* %ptr_i8, i8 %val_i8, i64 %n_i64, i1 false)");
        self.emit_raw("  ret void");
        self.emit_raw("");
        self.emit_raw("return:");
        self.emit_raw("  ret void");
        self.emit_raw("}");
        self.emit_raw("");

        // __cay_memcpy_byte: 按字节复制内存 (使用i64指针参数，兼容现有代码)
        self.emit_raw("define void @__cay_memcpy_byte(i64 %dest, i64 %src, i32 %n) {");
        self.emit_raw("entry:");
        self.emit_raw("  ; 空指针安全检查");
        self.emit_raw("  %dest_null = icmp eq i64 %dest, 0");
        self.emit_raw("  %src_null = icmp eq i64 %src, 0");
        self.emit_raw("  %either_null = or i1 %dest_null, %src_null");
        self.emit_raw("  br i1 %either_null, label %return, label %do_memcpy");
        self.emit_raw("");
        self.emit_raw("do_memcpy:");
        self.emit_raw("  %dest_i8 = inttoptr i64 %dest to i8*");
        self.emit_raw("  %src_i8 = inttoptr i64 %src to i8*");
        self.emit_raw("  %n_i64 = sext i32 %n to i64");
        self.emit_raw("  call void @llvm.memcpy.p0i8.p0i8.i64(i8* %dest_i8, i8* %src_i8, i64 %n_i64, i1 false)");
        self.emit_raw("  ret void");
        self.emit_raw("");
        self.emit_raw("return:");
        self.emit_raw("  ret void");
        self.emit_raw("}");
        self.emit_raw("");

        // 声明 llvm.memset
        self.emit_raw("declare void @llvm.memset.p0i8.i64(i8* nocapture writeonly, i8, i64, i1 immarg)");
        self.emit_raw("");
    }
}