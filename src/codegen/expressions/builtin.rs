//! 内置函数调用代码生成
//!
//! 处理 print/println/readInt/readFloat/readLine 等内置函数。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error_at};

/// readLine 缓冲区大小（字节）
const READ_LINE_BUFFER_SIZE: i32 = 1024;

/// 格式化字符串占位符类型
#[derive(Debug, Clone)]
enum Placeholder {
    CStyle(String),      // %d, %s, %f 等
    Sequential,          // {}
    Named(String),       // {name}
}

impl IRGenerator {
    /// 生成 print/println 调用代码
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
    /// * `loc` - 源码位置
    pub fn generate_print_call(&mut self, args: &[Expr], newline: bool, loc: &crate::error::SourceLocation) -> cayResult<String> {
        if args.is_empty() {
            // 无参数，仅打印换行符（如果是 println）或什么都不做（如果是 print）
            if newline {
                let fmt_str = "\n";
                let fmt_name = self.get_or_create_string_constant(fmt_str);
                let fmt_len = fmt_str.len() + 1;
                let fmt_ptr = self.new_temp();
                self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                    fmt_ptr, fmt_len, fmt_len, fmt_name));
                self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {})", fmt_ptr));
            }
            return Ok("void".to_string());
        }

        // 如果只有一个参数，使用原有的简单处理方式
        if args.len() == 1 {
            return self.generate_simple_print(&args[0], newline);
        }

        // 多个参数：第一个参数是 format 字符串
        self.generate_format_print(args, newline, loc)
    }

    /// 生成简单的单参数打印（保持向后兼容）
    fn generate_simple_print(&mut self, arg: &Expr, newline: bool) -> cayResult<String> {
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

                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        str_ptr, len, len, global_name));
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));

                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i8* {})",
                        fmt_ptr, str_ptr));
                }
                LiteralValue::Int32(_) | LiteralValue::Int64(_) => {
                    let value = self.generate_expression(arg)?;
                    let (type_str, val) = self.parse_typed_value(&value);
                    let i64_fmt = self.get_i64_format_specifier();
                    let fmt_str = if newline { format!("{}\n", i64_fmt) } else { i64_fmt.to_string() };
                    let fmt_name = self.get_or_create_string_constant(&fmt_str);
                    let fmt_len = fmt_str.len() + 1;

                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));

                    let final_val = if type_str != "i64" {
                        let ext_temp = self.new_temp();
                        self.emit_line(&format!("  {} = sext {} {} to i64", ext_temp, type_str, val));
                        ext_temp
                    } else {
                        val.to_string()
                    };

                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i64 {})",
                        fmt_ptr, final_val));
                }
                _ => {
                    let value = self.generate_expression(arg)?;
                    let (type_str, val) = self.parse_typed_value(&value);

                    if type_str == "i8*" {
                        let fmt_str = if newline { "%s\n" } else { "%s" };
                        let fmt_name = self.get_or_create_string_constant(fmt_str);
                        let fmt_len = fmt_str.len() + 1;
                        let fmt_ptr = self.new_temp();
                        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                            fmt_ptr, fmt_len, fmt_len, fmt_name));
                        self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i8* {})",
                            fmt_ptr, val));
                    } else if type_str == "i1" {
                        // 布尔类型：调用 __cay_bool_to_string 转换为字符串后打印
                        let str_temp = self.new_temp();
                        self.emit_line(&format!("  {} = call i8* @__cay_bool_to_string(i1 {})", str_temp, val));
                        let fmt_str = if newline { "%s\n" } else { "%s" };
                        let fmt_name = self.get_or_create_string_constant(fmt_str);
                        let fmt_len = fmt_str.len() + 1;
                        let fmt_ptr = self.new_temp();
                        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                            fmt_ptr, fmt_len, fmt_len, fmt_name));
                        self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i8* {})",
                            fmt_ptr, str_temp));
                    } else if type_str.starts_with("i") && !type_str.ends_with("*") {
                        let i64_fmt = self.get_i64_format_specifier();
                        let fmt_str = if newline { format!("{}\n", i64_fmt) } else { i64_fmt.to_string() };
                        let fmt_name = self.get_or_create_string_constant(&fmt_str);
                        let fmt_len = fmt_str.len() + 1;
                        let fmt_ptr = self.new_temp();
                        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                            fmt_ptr, fmt_len, fmt_len, fmt_name));

                        let final_val = if type_str != "i64" {
                            let ext_temp = self.new_temp();
                            self.emit_line(&format!("  {} = sext {} {} to i64", ext_temp, type_str, val));
                            ext_temp
                        } else {
                            val.to_string()
                        };

                        self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i64 {})",
                            fmt_ptr, final_val));
                    } else if type_str == "double" || type_str == "float" {
                        let fmt_str = if newline { "%f\n" } else { "%f" };
                        let fmt_name = self.get_or_create_string_constant(fmt_str);
                        let fmt_len = fmt_str.len() + 1;
                        let fmt_ptr = self.new_temp();
                        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                            fmt_ptr, fmt_len, fmt_len, fmt_name));

                        let final_val = if type_str == "float" {
                            let ext_temp = self.new_temp();
                            self.emit_line(&format!("  {} = fpext float {} to double", ext_temp, val));
                            ext_temp
                        } else {
                            val.to_string()
                        };

                        self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, double {})",
                            fmt_ptr, final_val));
                    } else {
                        let fmt_str = if newline { "%s\n" } else { "%s" };
                        let fmt_name = self.get_or_create_string_constant(fmt_str);
                        let fmt_len = fmt_str.len() + 1;
                        let fmt_ptr = self.new_temp();
                        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                            fmt_ptr, fmt_len, fmt_len, fmt_name));
                        self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, {})",
                            fmt_ptr, value));
                    }
                }
            }
            _ => {
                let value = self.generate_expression(arg)?;
                let (type_str, val) = self.parse_typed_value(&value);

                if type_str == "i8*" {
                    let fmt_str = if newline { "%s\n" } else { "%s" };
                    let fmt_name = self.get_or_create_string_constant(fmt_str);
                    let fmt_len = fmt_str.len() + 1;
                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));
                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i8* {})",
                        fmt_ptr, val));
                } else if type_str == "i1" {
                    // 布尔类型：调用 __cay_bool_to_string 转换为字符串后打印
                    let str_temp = self.new_temp();
                    self.emit_line(&format!("  {} = call i8* @__cay_bool_to_string(i1 {})", str_temp, val));
                    let fmt_str = if newline { "%s\n" } else { "%s" };
                    let fmt_name = self.get_or_create_string_constant(fmt_str);
                    let fmt_len = fmt_str.len() + 1;
                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));
                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i8* {})",
                        fmt_ptr, str_temp));
                } else if type_str.starts_with("i") && !type_str.ends_with("*") {
                    let i64_fmt = self.get_i64_format_specifier();
                    let fmt_str = if newline { format!("{}\n", i64_fmt) } else { i64_fmt.to_string() };
                    let fmt_name = self.get_or_create_string_constant(&fmt_str);
                    let fmt_len = fmt_str.len() + 1;
                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));

                    let final_val = if type_str != "i64" {
                        let ext_temp = self.new_temp();
                        self.emit_line(&format!("  {} = sext {} {} to i64", ext_temp, type_str, val));
                        ext_temp
                    } else {
                        val.to_string()
                    };

                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i64 {})",
                        fmt_ptr, final_val));
                } else if type_str == "double" || type_str == "float" {
                    let fmt_str = if newline { "%f\n" } else { "%f" };
                    let fmt_name = self.get_or_create_string_constant(fmt_str);
                    let fmt_len = fmt_str.len() + 1;
                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));

                    let final_val = if type_str == "float" {
                        let ext_temp = self.new_temp();
                        self.emit_line(&format!("  {} = fpext float {} to double", ext_temp, val));
                        ext_temp
                    } else {
                        val.to_string()
                    };

                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, double {})",
                        fmt_ptr, final_val));
                } else {
                    let fmt_str = if newline { "%s\n" } else { "%s" };
                    let fmt_name = self.get_or_create_string_constant(fmt_str);
                    let fmt_len = fmt_str.len() + 1;
                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));
                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, {})",
                        fmt_ptr, value));
                }
            }
        }

        Ok("i64 0".to_string())
    }

    /// 生成 format 字符串打印（支持多个参数）
    ///
    /// 支持三种占位符格式：
    /// 1. C风格: %d, %s, %f 等
    /// 2. 顺序占位符: {} - 按顺序填充
    /// 3. 标签占位符: {name} - 通过变量名引用（仅适用于变量参数）
    fn generate_format_print(&mut self, args: &[Expr], newline: bool, loc: &crate::error::SourceLocation) -> cayResult<String> {
        // 第一个参数必须是 format 字符串
        let format_arg = &args[0];
        let format_str = match format_arg {
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::String(s) => s.clone(),
                _ => {
                    // 如果第一个参数不是字符串字面量，回退到简单打印第一个参数
                    return self.generate_simple_print(format_arg, newline);
                }
            }
            _ => {
                // 如果第一个参数不是字符串字面量，回退到简单打印第一个参数
                return self.generate_simple_print(format_arg, newline);
            }
        };

        // 解析 format 字符串
        let placeholders = self.parse_format_string(&format_str);

        // 检查参数数量是否匹配
        if placeholders.len() != args.len() - 1 {
            return Err(codegen_error_at(loc.clone(), format!(
                "Format string expects {} arguments, but {} provided",
                placeholders.len(),
                args.len() - 1
            )));
        }

        // 首先生成所有参数的值并确定其类型
        let mut arg_types_and_values: Vec<(String, String)> = Vec::new();
        for i in 1..args.len() {
            let value = self.generate_expression(&args[i])?;
            let (type_str, val) = self.parse_typed_value(&value);
            arg_types_and_values.push((type_str, val));
        }

        // 将新格式转换为 C printf 格式（根据参数类型选择合适的格式说明符）
        let (c_format_str, arg_mapping) = self.convert_to_c_format_with_types(
            &format_str, &placeholders, &arg_types_and_values
        );

        // 构建最终的 format 字符串（添加换行符如果需要）
        let final_fmt_str = if newline {
            c_format_str + "\n"
        } else {
            c_format_str
        };

        let fmt_name = self.get_or_create_string_constant(&final_fmt_str);
        let fmt_len = final_fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name));

        // 根据新的映射顺序生成参数值
        let mut arg_values: Vec<(String, String)> = Vec::new();

        for &arg_idx in &arg_mapping {
            let (type_str, val) = &arg_types_and_values[arg_idx - 1];
            let placeholder = &placeholders[arg_idx - 1];
            let (final_type, final_val) = self.convert_for_placeholder(type_str, val, placeholder);
            arg_values.push((final_type, final_val));
        }

        // 构建 printf 调用
        let mut call_args = vec![format!("i8* {}", fmt_ptr)];
        for (typ, val) in &arg_values {
            call_args.push(format!("{} {}", typ, val));
        }

        self.emit_line(&format!("  call i32 (i8*, ...) @printf({})",
            call_args.join(", ")));

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

    /// 根据参数类型推断合适的格式说明符
    fn infer_format_spec(&self, type_str: &str) -> &'static str {
        match type_str {
            "i8*" => "%s",           // 字符串
            "i32" | "i8" | "i16" => "%d",  // 整数
            "i64" => "%lld",         // 长整数
            "float" | "double" => "%f",    // 浮点数
            _ => "%s",               // 默认作为字符串
        }
    }

    /// 将新格式字符串转换为 C printf 格式（根据参数类型选择格式说明符）
    /// 返回转换后的字符串和参数映射（新索引 -> 原索引）
    fn convert_to_c_format_with_types(
        &self,
        fmt: &str,
        placeholders: &[Placeholder],
        arg_types: &[(String, String)]
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
                        // {} - 根据参数类型选择格式说明符
                        chars.next();
                        if placeholder_idx < arg_types.len() {
                            let type_str = &arg_types[placeholder_idx].0;
                            let spec = self.infer_format_spec(type_str);
                            result.push_str(spec);
                        } else {
                            result.push_str("%s"); // 默认
                        }
                        arg_mapping.push(placeholder_idx + 1);
                        placeholder_idx += 1;
                    } else if next.is_ascii_alphabetic() || next == '_' {
                        // {name} - 命名占位符，同样根据类型选择格式
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
                            if placeholder_idx < arg_types.len() {
                                let type_str = &arg_types[placeholder_idx].0;
                                let spec = self.infer_format_spec(type_str);
                                result.push_str(spec);
                            } else {
                                result.push_str("%s");
                            }
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
    fn convert_for_placeholder(&mut self, type_str: &str, val: &str, placeholder: &Placeholder) -> (String, String) {
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
                    self.emit_line(&format!("  {} = sext {} {} to i64", ext_temp, type_str, val));
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
                    self.emit_line(&format!("  {} = sext {} {} to i64", ext_temp, type_str, val));
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
                self.emit_line(&format!("  {} = call i8* @__cay_char_to_string(i8 {})", str_temp, val));
                ("i8*".to_string(), str_temp)
            }
            "i32" => {
                // 32位整数
                let str_temp = self.new_temp();
                self.emit_line(&format!("  {} = call i8* @__cay_int_to_string(i32 {})", str_temp, val));
                ("i8*".to_string(), str_temp)
            }
            "i64" => {
                // 64位整数
                let str_temp = self.new_temp();
                self.emit_line(&format!("  {} = call i8* @__cay_long_to_string(i64 {})", str_temp, val));
                ("i8*".to_string(), str_temp)
            }
            "float" => {
                // 浮点数
                let str_temp = self.new_temp();
                self.emit_line(&format!("  {} = call i8* @__cay_float_to_string(float {})", str_temp, val));
                ("i8*".to_string(), str_temp)
            }
            "double" => {
                // 双精度浮点数
                let str_temp = self.new_temp();
                self.emit_line(&format!("  {} = call i8* @__cay_double_to_string(double {})", str_temp, val));
                ("i8*".to_string(), str_temp)
            }
            "i1" => {
                // 布尔类型
                let str_temp = self.new_temp();
                self.emit_line(&format!("  {} = call i8* @__cay_bool_to_string(i1 {})", str_temp, val));
                ("i8*".to_string(), str_temp)
            }
            _ => {
                // 其他类型（包括指针），尝试直接使用
                if type_str.ends_with("*") {
                    (type_str.to_string(), val.to_string())
                } else {
                    // 未知类型，默认作为 i64 处理
                    let str_temp = self.new_temp();
                    self.emit_line(&format!("  {} = call i8* @__cay_long_to_string(i64 {})", str_temp, val));
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
    pub fn generate_read_int_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        // readInt 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error_at(loc.clone(), "readInt() takes no arguments".to_string()));
        }

        // 为输入缓冲区分配空间
        let buffer_size = 32; // 足够存储整数
        let buffer_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca [{} x i8], align 1", buffer_temp, buffer_size));

        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, buffer_size, buffer_size, buffer_temp));

        // 调用 scanf 读取整数
        let fmt_str = self.get_i64_format_specifier();
        let fmt_name = self.get_or_create_string_constant(fmt_str);
        let fmt_len = fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name));

        // 为整数结果分配空间
        let int_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca i64, align 8", int_temp));

        // 调用 scanf
        self.emit_line(&format!("  call i32 (i8*, ...) @scanf(i8* {}, i64* {})",
            fmt_ptr, int_temp));

        // 加载读取的整数值
        let result_temp = self.new_temp();
        self.emit_line(&format!("  {} = load i64, i64* {}, align 8", result_temp, int_temp));

        Ok(format!("i64 {}", result_temp))
    }

    /// 生成 readFloat 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_float_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        // readFloat 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error_at(loc.clone(), "readFloat() takes no arguments".to_string()));
        }

        // 为输入缓冲区分配空间
        let buffer_size = 64;
        let buffer_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca [{} x i8], align 1", buffer_temp, buffer_size));

        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, buffer_size, buffer_size, buffer_temp));

        // 调用 scanf 读取浮点数
        let fmt_str = "%f";
        let fmt_name = self.get_or_create_string_constant(fmt_str);
        let fmt_len = fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name));

        // 为浮点数结果分配空间
        let float_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca float, align 4", float_temp));

        // 调用 scanf
        self.emit_line(&format!("  call i32 (i8*, ...) @scanf(i8* {}, float* {})",
            fmt_ptr, float_temp));

        // 加载读取的浮点数值
        let result_temp = self.new_temp();
        self.emit_line(&format!("  {} = load float, float* {}, align 4", result_temp, float_temp));

        Ok(format!("float {}", result_temp))
    }

    /// 生成 readDouble 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_double_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        // readDouble 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error_at(loc.clone(), "readDouble() takes no arguments".to_string()));
        }

        // 为输入缓冲区分配空间
        let buffer_size = 64;
        let buffer_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca [{} x i8], align 1", buffer_temp, buffer_size));

        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, buffer_size, buffer_size, buffer_temp));

        // 调用 scanf 读取双精度浮点数
        let fmt_str = "%lf";
        let fmt_name = self.get_or_create_string_constant(fmt_str);
        let fmt_len = fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name));

        // 为双精度浮点数结果分配空间
        let double_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca double, align 8", double_temp));

        // 调用 scanf
        self.emit_line(&format!("  call i32 (i8*, ...) @scanf(i8* {}, double* {})",
            fmt_ptr, double_temp));

        // 加载读取的双精度浮点数值
        let result_temp = self.new_temp();
        self.emit_line(&format!("  {} = load double, double* {}, align 8", result_temp, double_temp));

        Ok(format!("double {}", result_temp))
    }

    /// 生成 readLong 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_long_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        // readLong 与 readInt 相同，都返回 i64
        self.generate_read_int_call(args, loc)
    }

    /// 生成 readChar 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_char_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        // readChar 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error_at(loc.clone(), "readChar() takes no arguments".to_string()));
        }

        // 为输入缓冲区分配空间
        let buffer_size = 8;
        let buffer_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca [{} x i8], align 1", buffer_temp, buffer_size));

        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, buffer_size, buffer_size, buffer_temp));

        // 调用 scanf 读取字符
        let fmt_str = " %c";  // 空格跳过空白字符
        let fmt_name = self.get_or_create_string_constant(fmt_str);
        let fmt_len = fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name));

        // 为字符结果分配空间
        let char_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca i8, align 1", char_temp));

        // 调用 scanf
        self.emit_line(&format!("  call i32 (i8*, ...) @scanf(i8* {}, i8* {})",
            fmt_ptr, char_temp));

        // 加载读取的字符值
        let result_temp = self.new_temp();
        self.emit_line(&format!("  {} = load i8, i8* {}, align 1", result_temp, char_temp));

        Ok(format!("i8 {}", result_temp))
    }

    /// 生成 readLine 调用代码
    ///
    /// # Arguments
    /// * `args` - 参数列表（应该为空）
    /// * `loc` - 源码位置
    pub fn generate_read_line_call(&mut self, args: &[Expr], loc: &crate::error::SourceLocation) -> cayResult<String> {
        // readLine 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error_at(loc.clone(), "readLine() takes no arguments".to_string()));
        }

        // 分配缓冲区
        let buffer_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca [{} x i8], align 1", buffer_temp, READ_LINE_BUFFER_SIZE));

        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, READ_LINE_BUFFER_SIZE, READ_LINE_BUFFER_SIZE, buffer_temp));

        // 获取 stdin
        let stdin_ptr = self.new_temp();
        if self.is_windows_target() {
            // Windows: 使用 __acrt_iob_func(0) 获取 stdin
            self.emit_line(&format!("  {} = call i8* @__acrt_iob_func(i32 0)", stdin_ptr));
        } else {
            // Linux/macOS: stdin 是外部全局变量
            self.emit_line(&format!("  {} = load i8*, i8** @stdin, align 8", stdin_ptr));
        }

        // 调用 fgets
        self.emit_line(&format!("  call i8* @fgets(i8* {}, i32 {}, i8* {})",
            buffer_ptr, READ_LINE_BUFFER_SIZE, stdin_ptr));

        // 返回缓冲区指针
        Ok(format!("i8* {}", buffer_ptr))
    }
}
