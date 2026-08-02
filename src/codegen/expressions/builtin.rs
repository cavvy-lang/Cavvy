//! 内置函数调用代码生成
//!
//! 处理 print/println/readInt/readFloat/readLine 等内置函数。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};

/// readLine 缓冲区大小（字节）
const READ_LINE_BUFFER_SIZE: i32 = 1024;

/// 格式化字符串占位符类型
#[derive(Debug, Clone)]
enum Placeholder {
    CStyle(String), // %d, %s, %f 等
    Sequential,     // {}
    Named(String),  // {name}
}

/// 输出流类型
#[derive(Debug, Clone, Copy)]
pub enum PrintStream {
    Stdout,
    Stderr,
}

impl PrintStream {
    fn is_stderr(self) -> bool {
        matches!(self, PrintStream::Stderr)
    }
}

impl IRGenerator {
    /// 生成 print/println/eprint/eprintln 调用代码
    ///
    /// 支持两种调用方式：
    /// 1. 单参数：print("Hello") 或 println(123)
    /// 2. Format 字符串：print("Value: %d", value) 或 println("Name: %s, Age: %d", name, age)
    ///
    /// 支持的格式说明符：
    /// - %d, %i: 整数 (int/long)
    /// - %f: 浮点数 (float/double)
    /// - %s: 字符串
    /// - %%: 字面量 %
    ///
    /// # Arguments
    /// * `args` - 参数列表
    /// * `newline` - 是否打印换行符
    /// * `stream` - 输出流（标准输出或标准错误）
    /// * `loc` - 源码位置
    pub fn generate_print_call(
        &mut self,
        args: &[Expr],
        newline: bool,
        stream: PrintStream,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.is_empty() {
            // 无参数，仅打印换行符（如果是 println/eprintln）或什么都不做（如果是 print/eprint）
            if newline {
                let fmt_str = "\n";
                let fmt_name = self.get_or_create_string_constant(fmt_str);
                let fmt_len = fmt_str.len() + 1;
                let fmt_ptr = self.new_temp();
                self.emit_line(&format!(
                    "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                    fmt_ptr, fmt_len, fmt_len, fmt_name
                ));
                self.emit_stream_call(stream, &fmt_ptr, &[]);
            }
            return Ok("void".to_string());
        }

        // 如果只有一个参数，使用原有的简单处理方式
        if args.len() == 1 {
            return self.generate_simple_print(&args[0], newline, stream);
        }

        // 多个参数：第一个参数是 format 字符串
        self.generate_format_print(args, newline, stream, loc)
    }

    /// 获取 stderr 文件指针
    fn emit_stderr_ptr(&mut self,
    ) -> String {
        let stderr_ptr = self.new_temp();
        if self.is_windows_target() {
            self.emit_line(&format!(
                "  {} = call i8* @__acrt_iob_func(i32 2)",
                stderr_ptr
            ));
        } else {
            self.emit_line(&format!(
                "  {} = load i8*, i8** @stderr, align 8",
                stderr_ptr
            ));
        }
        stderr_ptr
    }

    /// 根据输出流生成打印调用
    fn emit_stream_call(
        &mut self,
        stream: PrintStream,
        fmt_ptr: &str,
        args: &[String],
    ) {
        match stream {
            PrintStream::Stdout => {
                let mut call_args = vec![format!("i8* {}", fmt_ptr)];
                call_args.extend(args.iter().cloned());
                self.emit_line(&format!(
                    "  call {} (i8*, ...) @printf({})",
                    self.get_extern_ret_type("printf", "i32"),
                    call_args.join(", ")
                ));
            }
            PrintStream::Stderr => {
                let stderr_ptr = self.emit_stderr_ptr();
                let mut call_args = vec![format!("i8* {}", stderr_ptr), format!("i8* {}", fmt_ptr)];
                call_args.extend(args.iter().cloned());
                self.emit_line(&format!(
                    "  call {} (i8*, i8*, ...) @fprintf({})",
                    self.get_extern_ret_type("fprintf", "i32"),
                    call_args.join(", ")
                ));
            }
        }
    }

    /// 生成 exit 调用代码
    ///
    /// 接受一个整数参数作为退出码，调用 C 标准库的 exit 函数终止程序。
    pub fn generate_exit_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        if args.len() != 1 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("exit() takes exactly 1 argument, but got {}", args.len()),
            ));
        }

        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);

        // 将参数转换为 i32 类型的退出码
        let exit_code = if arg_type == "i32" {
            arg_val.to_string()
        } else if arg_type == "i1" {
            // 布尔类型：true -> 1, false -> 0
            let ext_temp = self.new_temp();
            self.emit_line(&format!("  {} = zext i1 {} to i32", ext_temp, arg_val));
            ext_temp
        } else if arg_type.starts_with("i") && !arg_type.ends_with("*") {
            // 其他整数类型：先扩展到 i64 再截断到 i32，保证有符号语义
            let ext_temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to i64", ext_temp, arg_type, arg_val));
            let trunc_temp = self.new_temp();
            self.emit_line(&format!("  {} = trunc i64 {} to i32", trunc_temp, ext_temp));
            trunc_temp
        } else if arg_type == "float" || arg_type == "double" {
            // 浮点类型：转换为 i32
            let double_val = if arg_type == "float" {
                let ext_temp = self.new_temp();
                self.emit_line(&format!("  {} = fpext float {} to double", ext_temp, arg_val));
                ext_temp
            } else {
                arg_val.to_string()
            };
            let conv_temp = self.new_temp();
            self.emit_line(&format!("  {} = fptosi double {} to i32", conv_temp, double_val));
            conv_temp
        } else {
            // 指针或其他类型：先转换为 i64 再截断到 i32
            let ptr_temp = self.new_temp();
            self.emit_line(&format!("  {} = ptrtoint {} {} to i64", ptr_temp, arg_type, arg_val));
            let trunc_temp = self.new_temp();
            self.emit_line(&format!("  {} = trunc i64 {} to i32", trunc_temp, ptr_temp));
            trunc_temp
        };

        self.emit_line(&format!("  call void @exit(i32 {})", exit_code)
        );

        Ok("void".to_string())
    }

    /// 6.1.0: 生成 panic/abort 调用代码
    ///
    /// 接受一个字符串参数作为错误消息，打印到 stderr 后调用 abort() 终止程序。
    /// - `--no-panic` 编译选项：此处直接报编译错误（panic 与 abort 同路径，一并禁用）。
    /// - `-g`（debug_info）下在 abort 前额外打印原生调用栈回溯：
    ///   Linux/macOS 用 glibc `backtrace`/`backtrace_symbols_fd`（无需额外链接库），
    ///   Windows 用 `RtlCaptureStackBackTrace` 打印帧地址（不做符号化，避免 dbghelp 依赖）。
    pub fn generate_panic_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // --no-panic：panic()/abort() 转为编译错误
        if let Some(config) = self.get_platform_config() {
            if config.no_panic {
                return Err(codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    loc.clone(),
                    "panic()/abort() is disabled: compiled with --no-panic".to_string(),
                ));
            }
        }

        if args.len() != 1 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("panic() takes exactly 1 argument, but got {}", args.len()),
            ));
        }

        let arg_result = self.generate_expression(&args[0])?;
        let (arg_type, arg_val) = self.parse_typed_value(&arg_result);

        // panic 参数必须是字符串（i8*）
        if arg_type != "i8*" {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!("panic() requires a String argument, got {}", arg_type),
            ));
        }

        // 确保 abort 已声明
        if !self.is_extern_emitted("abort@void") {
            self.emit_raw("declare void @abort()");
            self.mark_extern_emitted("abort@void".to_string());
        }

        // 打印 "panic: message\n" 到 stderr
        let prefix = "panic: ";
        let prefix_global = self.get_or_create_string_constant(prefix);
        let prefix_len = prefix.len() + 1;
        let newline = "\n";
        let newline_global = self.get_or_create_string_constant(newline);
        let newline_len = newline.len() + 1;

        let prefix_ptr = self.new_temp();
        let newline_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            prefix_ptr, prefix_len, prefix_len, prefix_global
        ));
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            newline_ptr, newline_len, newline_len, newline_global
        ));

        let stderr_ptr = self.emit_stderr_ptr();
        let panic_fmt = "%s%s%s";
        let panic_fmt_global = self.get_or_create_string_constant(panic_fmt);
        let panic_fmt_len = panic_fmt.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, panic_fmt_len, panic_fmt_len, panic_fmt_global
        ));

        self.emit_line(&format!(
            "  call i32 (i8*, i8*, ...) @fprintf(i8* {}, i8* {}, i8* {}, i8* {}, i8* {})",
            stderr_ptr, fmt_ptr, prefix_ptr, arg_val, newline_ptr
        ));

        // Debug 模式（-g）：abort 前打印调用栈回溯（此刻栈帧仍完整）
        if self.debug_info {
            self.emit_panic_backtrace();
        }

        // 调用 abort 终止程序
        self.emit_line("  call void @abort()");

        Ok("void".to_string())
    }

    /// Debug 模式 panic 回溯：在 panic 发射点内联调用平台回溯 API。
    ///
    /// Linux/macOS：glibc `backtrace` + `backtrace_symbols_fd`（直接写 fd 2，无需缓冲格式化）；
    /// Windows：`RtlCaptureStackBackTrace` 收集帧地址后用 fprintf 循环打印（地址级，不符号化）。
    fn emit_panic_backtrace(&mut self) {
        const BT_DEPTH: usize = 64;

        // 回溯标题
        let header = "stack backtrace:\n";
        let header_global = self.get_or_create_string_constant(header);
        let header_len = header.len() + 1;
        let header_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            header_ptr, header_len, header_len, header_global
        ));
        let stderr_for_header = self.emit_stderr_ptr();
        self.emit_line(&format!(
            "  call i32 (i8*, i8*, ...) @fprintf(i8* {}, i8* {})",
            stderr_for_header, header_ptr
        ));

        // 帧缓冲
        let buf = self.new_temp();
        self.emit_line(&format!("  {} = alloca [{} x i8*]", buf, BT_DEPTH));
        let buf_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8*], [{} x i8*]* {}, i64 0, i64 0",
            buf_ptr, BT_DEPTH, BT_DEPTH, buf
        ));

        if !self.is_windows_target() {
            // Linux/macOS：glibc execinfo
            if !self.is_extern_emitted("backtrace@i32") {
                self.emit_raw("declare i32 @backtrace(i8**, i32)");
                self.mark_extern_emitted("backtrace@i32".to_string());
            }
            if !self.is_extern_emitted("backtrace_symbols_fd@void") {
                self.emit_raw("declare void @backtrace_symbols_fd(i8**, i32, i32)");
                self.mark_extern_emitted("backtrace_symbols_fd@void".to_string());
            }
            let n = self.new_temp();
            self.emit_line(&format!(
                "  {} = call i32 @backtrace(i8** {}, i32 {})",
                n, buf_ptr, BT_DEPTH
            ));
            // fd 2 = stderr
            self.emit_line(&format!(
                "  call void @backtrace_symbols_fd(i8** {}, i32 {}, i32 2)",
                buf_ptr, n
            ));
        } else {
            // Windows：kernel32 RtlCaptureStackBackTrace + 地址打印
            if !self.is_extern_emitted("RtlCaptureStackBackTrace@i32") {
                self.emit_raw("declare i32 @RtlCaptureStackBackTrace(i32, i32, i8**, i32*)");
                self.mark_extern_emitted("RtlCaptureStackBackTrace@i32".to_string());
            }
            let n = self.new_temp();
            self.emit_line(&format!(
                "  {} = call i32 @RtlCaptureStackBackTrace(i32 0, i32 {}, i8** {}, i32* null)",
                n, BT_DEPTH, buf_ptr
            ));

            // for (i = 0; i < n; i++) fprintf(stderr, "  #%d %p\n", i, frames[i])
            let fmt = "  #%d %p\n";
            let fmt_global = self.get_or_create_string_constant(fmt);
            let fmt_len = fmt.len() + 1;
            let fmt_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                fmt_ptr, fmt_len, fmt_len, fmt_global
            ));

            let idx_var = self.new_temp();
            self.emit_line(&format!("  {} = alloca i32", idx_var));
            self.emit_line(&format!("  store i32 0, i32* {}", idx_var));

            let cond_label = self.new_label("bt.cond");
            let body_label = self.new_label("bt.body");
            let done_label = self.new_label("bt.done");

            self.emit_line(&format!("  br label %{}", cond_label));
            self.emit_line(&format!("{}:", cond_label));
            let i_val = self.new_temp();
            self.emit_line(&format!("  {} = load i32, i32* {}", i_val, idx_var));
            let cmp = self.new_temp();
            self.emit_line(&format!("  {} = icmp slt i32 {}, {}", cmp, i_val, n));
            self.emit_line(&format!(
                "  br i1 {}, label %{}, label %{}",
                cmp, body_label, done_label
            ));
            self.emit_line(&format!("{}:", body_label));
            let idx64 = self.new_temp();
            self.emit_line(&format!("  {} = sext i32 {} to i64", idx64, i_val));
            let slot = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr [{} x i8*], [{} x i8*]* {}, i64 0, i64 {}",
                slot, BT_DEPTH, BT_DEPTH, buf, idx64
            ));
            let addr = self.new_temp();
            self.emit_line(&format!("  {} = load i8*, i8** {}", addr, slot));
            let stderr_loop = self.emit_stderr_ptr();
            self.emit_line(&format!(
                "  call i32 (i8*, i8*, ...) @fprintf(i8* {}, i8* {}, i32 {}, i8* {})",
                stderr_loop, fmt_ptr, i_val, addr
            ));
            let next = self.new_temp();
            self.emit_line(&format!("  {} = add i32 {}, 1", next, i_val));
            self.emit_line(&format!("  store i32 {}, i32* {}", next, idx_var));
            self.emit_line(&format!("  br label %{}", cond_label));
            self.emit_line(&format!("{}:", done_label));
        }
    }

    /// 生成简单的单参数打印（保持向后兼容）
    fn generate_simple_print(
        &mut self,
        arg: &Expr,
        newline: bool,
        stream: PrintStream,
    ) -> CayResult<String> {
        match arg {
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::String(s) => {
                    let global_name = self.get_or_create_string_constant(s);
                    let fmt_str = if newline { "%s\n" } else { "%s" };
                    let fmt_name = self.get_or_create_string_constant(fmt_str);
                    let len = s.len() + 1;
                    let fmt_len = fmt_str.len() + 1;

                    let str_ptr = self.new_temp();
                    let fmt_ptr = self.new_temp();

                    self.emit_line(&format!(
                        "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        str_ptr, len, len, global_name
                    ));
                    self.emit_line(&format!(
                        "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name
                    ));

                    self.emit_stream_call(stream, &fmt_ptr, &[format!("i8* {}", str_ptr)]);
                }
                LiteralValue::Int32(_) | LiteralValue::Int64(_) => {
                    let value = self.generate_expression(arg)?;
                    let (type_str, val) = self.parse_typed_value(&value);
                    let i64_fmt = self.get_i64_format_specifier();
                    let fmt_str = if newline {
                        format!("{}\n", i64_fmt)
                    } else {
                        i64_fmt.to_string()
                    };
                    let fmt_name = self.get_or_create_string_constant(&fmt_str);
                    let fmt_len = fmt_str.len() + 1;

                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name
                    ));

                    let final_val = if type_str != "i64" {
                        let ext_temp = self.new_temp();
                        self.emit_line(&format!(
                            "  {} = sext {} {} to i64",
                            ext_temp, type_str, val
                        ));
                        ext_temp
                    } else {
                        val.to_string()
                    };

                    self.emit_stream_call(stream, &fmt_ptr, &[format!("i64 {}", final_val)]);
                }
                _ => {
                    self.generate_simple_print_expr(arg, newline, stream)?;
                }
            },
            _ => {
                self.generate_simple_print_expr(arg, newline, stream)?;
            }
        }

        Ok("i64 0".to_string())
    }

    /// 生成单参数表达式的打印（非字符串/整数字面量）
    fn generate_simple_print_expr(
        &mut self,
        arg: &Expr,
        newline: bool,
        stream: PrintStream,
    ) -> CayResult<()> {
        let value = self.generate_expression(arg)?;
        let (type_str, val) = self.parse_typed_value(&value);

        if type_str == "i8*" {
            let fmt_str = if newline { "%s\n" } else { "%s" };
            let fmt_name = self.get_or_create_string_constant(fmt_str);
            let fmt_len = fmt_str.len() + 1;
            let fmt_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                fmt_ptr, fmt_len, fmt_len, fmt_name
            ));
            self.emit_stream_call(stream, &fmt_ptr, &[format!("i8* {}", val)]);
        } else if type_str == "i1" {
            // 布尔类型：调用 __cay_bool_to_string 转换为字符串后打印
            let str_temp = self.new_temp();
            self.emit_line(&format!(
                "  {} = call i8* @__cay_bool_to_string(i1 {})",
                str_temp, val
            ));
            let fmt_str = if newline { "%s\n" } else { "%s" };
            let fmt_name = self.get_or_create_string_constant(fmt_str);
            let fmt_len = fmt_str.len() + 1;
            let fmt_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                fmt_ptr, fmt_len, fmt_len, fmt_name
            ));
            self.emit_stream_call(stream, &fmt_ptr, &[format!("i8* {}", str_temp)]);
        } else if type_str.starts_with("i") && !type_str.ends_with("*") {
            let i64_fmt = self.get_i64_format_specifier();
            let fmt_str = if newline {
                format!("{}\n", i64_fmt)
            } else {
                i64_fmt.to_string()
            };
            let fmt_name = self.get_or_create_string_constant(&fmt_str);
            let fmt_len = fmt_str.len() + 1;
            let fmt_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                fmt_ptr, fmt_len, fmt_len, fmt_name
            ));

            let final_val = if type_str != "i64" {
                let ext_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = sext {} {} to i64",
                    ext_temp, type_str, val
                ));
                ext_temp
            } else {
                val.to_string()
            };

            self.emit_stream_call(stream, &fmt_ptr, &[format!("i64 {}", final_val)]);
        } else if type_str == "double" || type_str == "float" {
            let fmt_str = if newline { "%f\n" } else { "%f" };
            let fmt_name = self.get_or_create_string_constant(fmt_str);
            let fmt_len = fmt_str.len() + 1;
            let fmt_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                fmt_ptr, fmt_len, fmt_len, fmt_name
            ));

            let final_val = if type_str == "float" {
                let ext_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = fpext float {} to double",
                    ext_temp, val
                ));
                ext_temp
            } else {
                val.to_string()
            };

            self.emit_stream_call(stream, &fmt_ptr, &[format!("double {}", final_val)]);
        } else {
            let fmt_str = if newline { "%s\n" } else { "%s" };
            let fmt_name = self.get_or_create_string_constant(fmt_str);
            let fmt_len = fmt_str.len() + 1;
            let fmt_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                fmt_ptr, fmt_len, fmt_len, fmt_name
            ));
            self.emit_stream_call(stream, &fmt_ptr, &[value]);
        }

        Ok(())
    }

    /// 生成 format 字符串打印（支持多个参数）
    ///
    /// 支持三种占位符格式：
    /// 1. C风格: %d, %s, %f 等
    /// 2. 顺序占位符: {} - 按顺序填充
    /// 3. 标签占位符: {name} - 通过变量名引用（仅适用于变量参数）
    fn generate_format_print(
        &mut self,
        args: &[Expr],
        newline: bool,
        stream: PrintStream,
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // 第一个参数必须是 format 字符串
        let format_arg = &args[0];
        let format_str = match format_arg {
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::String(s) => s.clone(),
                _ => {
                    // 如果第一个参数不是字符串字面量，回退到简单打印第一个参数
                    return self.generate_simple_print(format_arg, newline, stream);
                }
            },
            _ => {
                // 如果第一个参数不是字符串字面量，回退到简单打印第一个参数
                return self.generate_simple_print(format_arg, newline, stream);
            }
        };

        // 解析 format 字符串
        let placeholders = self.parse_format_string(&format_str);

        // 检查参数数量是否匹配
        if placeholders.len() != args.len() - 1 {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                format!(
                    "Format string expects {} arguments, but {} provided",
                    placeholders.len(),
                    args.len() - 1
                ),
            ));
        }

        // 首先生成所有参数的值并确定其类型
        let mut arg_types_and_values: Vec<(String, String)> = Vec::new();
        for i in 1..args.len() {
            let value = self.generate_expression(&args[i])?;
            let (type_str, val) = self.parse_typed_value(&value);
            arg_types_and_values.push((type_str, val));
        }

        // 将新格式转换为 C printf 格式（根据参数类型选择合适的格式说明符）
        let (c_format_str, arg_mapping) =
            self.convert_to_c_format_with_types(&format_str, &placeholders, &arg_types_and_values);

        // 构建最终的 format 字符串（添加换行符如果需要）
        let final_fmt_str = if newline {
            c_format_str + "\n"
        } else {
            c_format_str
        };

        let fmt_name = self.get_or_create_string_constant(&final_fmt_str);
        let fmt_len = final_fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name
        ));

        // 根据新的映射顺序生成参数值
        let mut arg_values: Vec<String> = Vec::new();

        for &arg_idx in &arg_mapping {
            let (type_str, val) = &arg_types_and_values[arg_idx - 1];
            let placeholder = &placeholders[arg_idx - 1];
            let (final_type, final_val) = self.convert_for_placeholder(type_str, val, placeholder);
            arg_values.push(format!("{} {}", final_type, final_val));
        }

        // 构建输出流调用
        self.emit_stream_call(stream, &fmt_ptr, &arg_values);

        Ok("i64 0".to_string())
    }

    /// 解析 format 字符串，提取占位符
    /// 返回占位符列表和参数映射
    fn parse_format_string(&self, fmt: &str) -> Vec<Placeholder> {
        let mut placeholders = Vec::new();
        let mut chars = fmt.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '%' {
                if let Some(&next) = chars.peek() {
                    if next == '%' {
                        // %% - 转义的字面量 %
                        chars.next();
                    } else {
                        // C风格格式说明符
                        let mut spec = String::from("%");

                        // 收集格式说明符的其余部分
                        while let Some(&ch) = chars.peek() {
                            if ch.is_ascii_alphabetic() || ch == '*' {
                                spec.push(ch);
                                chars.next();
                                break;
                            } else {
                                spec.push(ch);
                                chars.next();
                            }
                        }

                        placeholders.push(Placeholder::CStyle(spec));
                    }
                }
            } else if c == '{' {
                // 顺序或命名占位符
                if let Some(&next) = chars.peek() {
                    if next == '}' {
                        // {} - 顺序占位符
                        chars.next(); // 消费 }
                        placeholders.push(Placeholder::Sequential);
                    } else if next.is_ascii_alphabetic() || next == '_' {
                        // {name} - 命名占位符
                        let mut name = String::new();
                        while let Some(&ch) = chars.peek() {
                            if ch.is_ascii_alphanumeric() || ch == '_' {
                                name.push(ch);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        // 期望 }
                        if let Some(&'}') = chars.peek() {
                            chars.next(); // 消费 }
                            placeholders.push(Placeholder::Named(name));
                        }
                    } else if next == '{' {
                        // {{ - 转义的字面量 {
                        chars.next();
                    }
                    // 其他情况忽略
                }
            }
        }

        placeholders
    }

    /// 将新格式字符串转换为 C printf 格式（根据参数类型选择格式说明符）
    /// 返回转换后的字符串和参数映射（新索引 -> 原索引）
    fn convert_to_c_format_with_types(
        &self,
        fmt: &str,
        placeholders: &[Placeholder],
        arg_types: &[(String, String)],
    ) -> (String, Vec<usize>) {
        let mut result = String::new();
        let mut arg_mapping: Vec<usize> = Vec::new();
        let mut placeholder_idx = 0;
        let mut chars = fmt.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '%' {
                if let Some(&next) = chars.peek() {
                    if next == '%' {
                        // %%
                        result.push(c);
                        result.push(chars.next().expect("peek 返回 Some 后 next 应也返回 Some"));
                    } else {
                        // C风格 - 保持原样
                        result.push(c);
                        while let Some(&ch) = chars.peek() {
                            result.push(ch);
                            chars.next();
                            if ch.is_ascii_alphabetic() || ch == '*' {
                                break;
                            }
                        }
                        arg_mapping.push(placeholder_idx + 1);
                        placeholder_idx += 1;
                    }
                }
            } else if c == '{' {
                if let Some(&next) = chars.peek() {
                    if next == '}' {
                        // {} - 转换为 %s，因为参数值会被 convert_for_placeholder 统一转为字符串
                        chars.next();
                        result.push_str("%s");
                        arg_mapping.push(placeholder_idx + 1);
                        placeholder_idx += 1;
                    } else if next.is_ascii_alphabetic() || next == '_' {
                        // {name} - 命名占位符，同样统一转为字符串
                        let mut name = String::new();
                        while let Some(&ch) = chars.peek() {
                            if ch.is_ascii_alphanumeric() || ch == '_' {
                                name.push(ch);
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        if let Some(&'}') = chars.peek() {
                            chars.next();
                            result.push_str("%s");
                            arg_mapping.push(placeholder_idx + 1);
                            placeholder_idx += 1;
                        } else {
                            result.push(c);
                            result.push_str(&name);
                        }
                    } else if next == '{' {
                        // {{ - 转义为 {
                        chars.next();
                        result.push('{');
                    } else {
                        result.push(c);
                    }
                } else {
                    result.push(c);
                }
            } else {
                result.push(c);
            }
        }

        (result, arg_mapping)
    }

    /// 根据占位符类型转换值
    fn convert_for_placeholder(
        &mut self,
        type_str: &str,
        val: &str,
        placeholder: &Placeholder,
    ) -> (String, String) {
        match placeholder {
            Placeholder::CStyle(spec) => self.convert_for_format(type_str, val, spec),
            Placeholder::Sequential | Placeholder::Named(_) => {
                // {} 和 {name} 默认作为字符串处理
                if type_str == "i8*" {
                    ("i8*".to_string(), val.to_string())
                } else {
                    // 非字符串类型需要转换为字符串
                    self.convert_to_string(type_str, val)
                }
            }
        }
    }

    /// 根据格式说明符转换值类型
    fn convert_for_format(&mut self, type_str: &str, val: &str, spec: &str) -> (String, String) {
        match spec {
            "%d" | "%i" => {
                // 整数格式 - 转换为 i64
                if type_str == "i64" {
                    ("i64".to_string(), val.to_string())
                } else if type_str.starts_with("i") && !type_str.ends_with("*") {
                    let ext_temp = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = sext {} {} to i64",
                        ext_temp, type_str, val
                    ));
                    ("i64".to_string(), ext_temp)
                } else {
                    // 其他类型（包括指针），尝试作为 i64
                    ("i64".to_string(), val.to_string())
                }
            }
            "%f" | "%e" | "%g" | "%E" | "%G" => {
                // 浮点格式 - 转换为 double
                if type_str == "double" {
                    ("double".to_string(), val.to_string())
                } else if type_str == "float" {
                    let ext_temp = self.new_temp();
                    self.emit_line(&format!("  {} = fpext float {} to double", ext_temp, val));
                    ("double".to_string(), ext_temp)
                } else {
                    // 其他类型，尝试作为 double
                    ("double".to_string(), val.to_string())
                }
            }
            "%s" => {
                // 字符串格式 - 必须是 i8*
                if type_str == "i8*" {
                    ("i8*".to_string(), val.to_string())
                } else {
                    // 非字符串类型需要转换为字符串
                    self.convert_to_string(type_str, val)
                }
            }
            "%c" => {
                // 字符格式 - 转换为 i32
                if type_str == "i32" {
                    ("i32".to_string(), val.to_string())
                } else if type_str == "i8" {
                    let ext_temp = self.new_temp();
                    self.emit_line(&format!("  {} = sext i8 {} to i32", ext_temp, val));
                    ("i32".to_string(), ext_temp)
                } else {
                    ("i32".to_string(), val.to_string())
                }
            }
            "%x" | "%X" | "%o" | "%u" => {
                // 无符号整数 - 转换为 i64
                if type_str == "i64" {
                    ("i64".to_string(), val.to_string())
                } else if type_str.starts_with("i") && !type_str.ends_with("*") {
                    let ext_temp = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = sext {} {} to i64",
                        ext_temp, type_str, val
                    ));
                    ("i64".to_string(), ext_temp)
                } else {
                    ("i64".to_string(), val.to_string())
                }
            }
            "%p" => {
                // 指针格式
                (type_str.to_string(), val.to_string())
            }
            _ => {
                // 未知的格式说明符，使用原类型
                (type_str.to_string(), val.to_string())
            }
        }
    }

    /// 将值转换为字符串类型
    /// 根据值的类型调用相应的运行时转换函数
    fn convert_to_string(&mut self, type_str: &str, val: &str) -> (String, String) {
        match type_str {
            "i8" => {
                // 字符类型
                let str_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_char_to_string(i8 {})",
                    str_temp, val
                ));
                ("i8*".to_string(), str_temp)
            }
            "i32" => {
                // 32位整数
                let str_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_int_to_string(i32 {})",
                    str_temp, val
                ));
                ("i8*".to_string(), str_temp)
            }
            "i64" => {
                // 64位整数
                let str_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_long_to_string(i64 {})",
                    str_temp, val
                ));
                ("i8*".to_string(), str_temp)
            }
            "float" => {
                // 浮点数
                let str_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_float_to_string(float {})",
                    str_temp, val
                ));
                ("i8*".to_string(), str_temp)
            }
            "double" => {
                // 双精度浮点数
                let str_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_double_to_string(double {})",
                    str_temp, val
                ));
                ("i8*".to_string(), str_temp)
            }
            "i1" => {
                // 布尔类型
                let str_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_bool_to_string(i1 {})",
                    str_temp, val
                ));
                ("i8*".to_string(), str_temp)
            }
            _ => {
                // 其他类型（包括指针），尝试直接使用
                if type_str.ends_with("*") {
                    (type_str.to_string(), val.to_string())
                } else {
                    // 未知类型，默认作为 i64 处理
                    let str_temp = self.new_temp();
                    self.emit_line(&format!(
                        "  {} = call i8* @__cay_long_to_string(i64 {})",
                        str_temp, val
                    ));
                    ("i8*".to_string(), str_temp)
                }
            }
        }
    }

    /// 生成 readInt 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_int_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // readInt 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "readInt() takes no arguments".to_string(),
            ));
        }

        // 为输入缓冲区分配空间
        let buffer_size = 32; // 足够存储整数
        let buffer_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = alloca [{} x i8], align 1",
            buffer_temp, buffer_size
        ));

        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, buffer_size, buffer_size, buffer_temp
        ));

        // 调用 scanf 读取整数
        let fmt_str = self.get_i64_format_specifier();
        let fmt_name = self.get_or_create_string_constant(fmt_str);
        let fmt_len = fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name
        ));

        // 为整数结果分配空间
        let int_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca i64, align 8", int_temp));

        // 调用 scanf
        self.emit_line(&format!(
            "  call i32 (i8*, ...) @scanf(i8* {}, i64* {})",
            fmt_ptr, int_temp
        ));

        // 加载读取的整数值
        let result_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i64, i64* {}, align 8",
            result_temp, int_temp
        ));

        Ok(format!("i64 {}", result_temp))
    }

    /// 生成 readFloat 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_float_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // readFloat 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "readFloat() takes no arguments".to_string(),
            ));
        }

        // 为输入缓冲区分配空间
        let buffer_size = 64;
        let buffer_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = alloca [{} x i8], align 1",
            buffer_temp, buffer_size
        ));

        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, buffer_size, buffer_size, buffer_temp
        ));

        // 调用 scanf 读取浮点数
        let fmt_str = "%f";
        let fmt_name = self.get_or_create_string_constant(fmt_str);
        let fmt_len = fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name
        ));

        // 为浮点数结果分配空间
        let float_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca float, align 4", float_temp));

        // 调用 scanf
        self.emit_line(&format!(
            "  call i32 (i8*, ...) @scanf(i8* {}, float* {})",
            fmt_ptr, float_temp
        ));

        // 加载读取的浮点数值
        let result_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load float, float* {}, align 4",
            result_temp, float_temp
        ));

        Ok(format!("float {}", result_temp))
    }

    /// 生成 readDouble 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_double_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // readDouble 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "readDouble() takes no arguments".to_string(),
            ));
        }

        // 为输入缓冲区分配空间
        let buffer_size = 64;
        let buffer_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = alloca [{} x i8], align 1",
            buffer_temp, buffer_size
        ));

        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, buffer_size, buffer_size, buffer_temp
        ));

        // 调用 scanf 读取双精度浮点数
        let fmt_str = "%lf";
        let fmt_name = self.get_or_create_string_constant(fmt_str);
        let fmt_len = fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name
        ));

        // 为双精度浮点数结果分配空间
        let double_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca double, align 8", double_temp));

        // 调用 scanf
        self.emit_line(&format!(
            "  call i32 (i8*, ...) @scanf(i8* {}, double* {})",
            fmt_ptr, double_temp
        ));

        // 加载读取的双精度浮点数值
        let result_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load double, double* {}, align 8",
            result_temp, double_temp
        ));

        Ok(format!("double {}", result_temp))
    }

    /// 生成 readLong 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_long_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // readLong 与 readInt 相同，都返回 i64
        self.generate_read_int_call(args, loc)
    }

    /// 生成 readChar 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_char_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // readChar 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "readChar() takes no arguments".to_string(),
            ));
        }

        // 为输入缓冲区分配空间
        let buffer_size = 8;
        let buffer_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = alloca [{} x i8], align 1",
            buffer_temp, buffer_size
        ));

        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, buffer_size, buffer_size, buffer_temp
        ));

        // 调用 scanf 读取字符
        let fmt_str = " %c"; // 空格跳过空白字符
        let fmt_name = self.get_or_create_string_constant(fmt_str);
        let fmt_len = fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name
        ));

        // 为字符结果分配空间
        let char_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca i8, align 1", char_temp));

        // 调用 scanf
        self.emit_line(&format!(
            "  call i32 (i8*, ...) @scanf(i8* {}, i8* {})",
            fmt_ptr, char_temp
        ));

        // 加载读取的字符值
        let result_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8, i8* {}, align 1",
            result_temp, char_temp
        ));

        Ok(format!("i8 {}", result_temp))
    }

    /// 生成 readLine 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_line_call(
        &mut self,
        args: &[Expr],
        loc: &crate::miette_diagnostic::SourceLocation,
    ) -> CayResult<String> {
        // readLine 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error_at(
                ErrorCodes::CODEGEN_INVALID_OPERATION,
                loc.clone(),
                "readLine() takes no arguments".to_string(),
            ));
        }

        // 分配缓冲区
        let buffer_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = alloca [{} x i8], align 1",
            buffer_temp, READ_LINE_BUFFER_SIZE
        ));

        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, READ_LINE_BUFFER_SIZE, READ_LINE_BUFFER_SIZE, buffer_temp
        ));

        // 获取 stdin
        let stdin_ptr = self.new_temp();
        if self.is_windows_target() {
            // Windows: 使用 __acrt_iob_func(0) 获取 stdin
            self.emit_line(&format!(
                "  {} = call i8* @__acrt_iob_func(i32 0)",
                stdin_ptr
            ));
        } else {
            // Linux/macOS: stdin 是外部全局变量
            self.emit_line(&format!("  {} = load i8*, i8** @stdin, align 8", stdin_ptr));
        }

        // 调用 fgets
        self.emit_line(&format!(
            "  call i8* @fgets(i8* {}, i32 {}, i8* {})",
            buffer_ptr, READ_LINE_BUFFER_SIZE, stdin_ptr
        ));

        // 返回缓冲区指针
        Ok(format!("i8* {}", buffer_ptr))
    }
}
