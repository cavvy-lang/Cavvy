//! 运行时支持函数声明模块
//!
//! 本模块声明所有 cay 运行时支持函数的 LLVM IR `declare`。
//! 运行时函数本体已从 LLVM IR 移入预编译的 C 静态链接库 `libcayrt.a`。
//! 编译时通过 `-lcayrt` 链接。

use crate::codegen::context::IRGenerator;

// 子模块声明（保留用于 future 扩展）
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
    /// 发射IR头部（外部声明和运行时函数声明）
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
                    declarations.push_str("declare dllimport void @SetConsoleOutputCP(i32)\n");
                    if config.is_defined("WINDOWS_SPECIFIC") {
                        declarations.push_str("declare void @WindowsSpecificInit()\n");
                    }
                }
                "linux" | "macos" => {
                    if config.is_feature_enabled("console_utf8") {
                        declarations.push_str("declare i8* @setlocale(i32, i8*)\n");
                        declarations.push_str(&"@.str.locale = private unnamed_addr constant [6 x i8] c\"C.UTF-8\"\00\n".to_string());
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
            "declare void @SetConsoleOutputCP(i32)\n".to_string()
        } else {
            "".to_string()
        };

        // 发射宏定义
        if let Some(config) = &self.platform_config {
            let mut has_macros = false;
            let defines = config.defines.clone();
            let undefines = config.undefines.clone();

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

        // 声明外部C库函数
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

        // llvm.memcpy 内部函数声明（用户代码需要使用）
        self.emit_raw("declare void @llvm.memcpy.p0i8.p0i8.i64(i8* noalias nocapture writeonly, i8* noalias nocapture readonly, i64, i1 immarg)");

        // Windows平台使用 __acrt_iob_func 获取stdin
        if target_triple.contains("windows") || target_triple.contains("mingw") {
            if !self.is_extern_emitted("__acrt_iob_func@i8*@i32") {
                self.emit_raw("declare i8* @__acrt_iob_func(i32)");
                self.mark_extern_emitted("__acrt_iob_func@i8*@i32".to_string());
            }
        } else {
            self.emit_raw("@stdin = external global i8*");
        }
        self.emit_raw("");

        // ================================================================
        // 分配器类型定义（运行时函数声明需要这些类型）
        // 结构体布局必须与 libcayrt.a 中的定义一致
        // ================================================================
        self.emit_raw("%GlobalAlloc = type { i8 }");
        self.emit_raw("%ArenaAllocator = type { i8*, i8*, i8*, %ArenaAllocator* }");
        self.emit_raw("%StackAllocator = type { i8*, i64 }");
        self.emit_raw("");

        // ================================================================
        // 运行时函数声明 (函数本体在 libcayrt.a 中)
        // ================================================================
        self.emit_runtime_declarations();

        // 插入标记，供 generator.rs 定位声明插入点
        self.emit_raw("; --- END OF HEADER ---");
    }

    /// 声明所有运行时函数（来自 libcayrt.a）
    fn emit_runtime_declarations(&mut self) {
        // 字符串操作
        self.emit_raw("declare i8* @__cay_string_concat(i8*, i8*)");
        self.emit_raw("declare i32 @__cay_string_length(i8*)");
        self.emit_raw("declare i8* @__cay_string_substring(i8*, i32, i32)");
        self.emit_raw("declare i32 @__cay_string_indexof(i8*, i8*)");
        self.emit_raw("declare i32 @__cay_string_indexof_from(i8*, i8*, i32)");
        self.emit_raw("declare i32 @__cay_string_lastindexof(i8*, i8*)");
        self.emit_raw("declare i1 @__cay_string_startswith(i8*, i8*)");
        self.emit_raw("declare i1 @__cay_string_endswith(i8*, i8*)");
        self.emit_raw("declare i8 @__cay_string_charat(i8*, i32)");
        self.emit_raw("declare i8* @__cay_string_replace(i8*, i8*, i8*)");
        self.emit_raw("declare i1 @__cay_string_isempty(i8*)");
        self.emit_raw("declare i1 @__cay_string_equals(i8*, i8*)");
        self.emit_raw("declare i1 @__cay_string_equals_ignorecase(i8*, i8*)");
        self.emit_raw("declare i8* @__cay_string_trim(i8*)");
        self.emit_raw("declare i8* @__cay_string_to_lower(i8*)");
        self.emit_raw("declare i8* @__cay_string_to_upper(i8*)");
        self.emit_raw("declare i1 @__cay_string_contains(i8*, i8*)");
        self.emit_raw("declare i32 @__cay_string_compareto(i8*, i8*)");

        // 类型转换
        self.emit_raw("declare i8* @__cay_int_to_string(i32)");
        self.emit_raw("declare i8* @__cay_long_to_string(i64)");
        self.emit_raw("declare i8* @__cay_float_to_string(float)");
        self.emit_raw("declare i8* @__cay_double_to_string(double)");
        self.emit_raw("declare i8* @__cay_bool_to_string(i1)");
        self.emit_raw("declare i8* @__cay_char_to_string(i8)");

        // 指针/缓冲区操作
        self.emit_raw("declare i64 @__cay_read_ptr(i64)");
        self.emit_raw("declare i8* @__cay_ptr_to_string(i64)");
        self.emit_raw("declare void @__cay_write_ptr(i64, i64)");
        self.emit_raw("declare void @__cay_write_int(i64, i32)");
        self.emit_raw("declare i32 @__cay_read_int(i64)");
        self.emit_raw("declare void @__cay_write_byte(i64, i32)");
        self.emit_raw("declare i8* @__cay_buffer_to_string(i64, i32)");

        // 内存操作
        self.emit_raw("declare void @__cay_memset_byte(i64, i32, i32)");
        self.emit_raw("declare void @__cay_memcpy_byte(i64, i64, i32)");

        // 数组/参数操作
        self.emit_raw("declare i8** @__cay_create_string_array(i32)");
        self.emit_raw("declare i8* @__cay_cstr_to_string(i8*)");
        self.emit_raw("declare void @__cay_array_set_ref(i8**, i32, i8*)");
        self.emit_raw("declare i8* @__cay_array_get_ref(i8**, i32)");
        self.emit_raw("declare i32 @__cay_array_length(i8**)");

        // 分配器
        self.emit_raw("declare %GlobalAlloc* @__cay_global_alloc_get()");
        self.emit_raw("declare %ArenaAllocator* @__cay_arena_new(i64)");
        self.emit_raw("declare i8* @__cay_arena_alloc(%ArenaAllocator*, i64, i64)");
        self.emit_raw("declare void @__cay_arena_reset(%ArenaAllocator*)");
        self.emit_raw("declare void @__cay_arena_free(%ArenaAllocator*)");

        self.emit_raw("");
    }
}
