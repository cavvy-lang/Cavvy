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
mod string_charat;
mod string_replace;

impl IRGenerator {
    /// 发射IR头部（外部声明和运行时函数）
    pub fn emit_header(&mut self) {
        self.emit_raw("; cay (Ethernos Object Language) Generated LLVM IR");
        
        // 根据目标平台设置目标三元组
        let target_triple = if cfg!(target_os = "windows") {
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

        // 声明外部函数 (printf 和标准C库函数)
        self.emit_raw("declare i32 @printf(i8*, ...)");
        self.emit_raw("declare i32 @scanf(i8*, ...)");
        
        // 只在 Windows 平台上声明 Windows API 函数
        if cfg!(target_os = "windows") {
            self.emit_raw("declare void @SetConsoleOutputCP(i32)");
        }
        
        self.emit_raw("declare i64 @strlen(i8*)");
        self.emit_raw("declare i8* @calloc(i64, i64)");
        self.emit_raw("declare void @exit(i32)");
        self.emit_raw("declare void @llvm.memcpy.p0i8.p0i8.i64(i8* noalias nocapture writeonly, i8* noalias nocapture readonly, i64, i1 immarg)");
        self.emit_raw("declare i32 @snprintf(i8*, i64, i8*, ...)");
        self.emit_raw("@.str.float_fmt = private unnamed_addr constant [3 x i8] c\"%f\\00\", align 1");
        self.emit_raw("@.str.int_fmt = private unnamed_addr constant [5 x i8] c\"%lld\\00\", align 1");
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
        self.emit_string_charat_runtime();
        self.emit_string_replace_runtime();
    }
}