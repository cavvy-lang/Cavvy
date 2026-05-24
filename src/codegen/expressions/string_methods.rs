//! String 方法调用代码生成
//!
//! 处理 String 类型的方法调用（length, substring, indexOf, charAt, replace）。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error};

impl IRGenerator {
    /// 尝试生成 String 方法调用代码
    /// 返回 Some(result) 如果成功处理，None 如果不是 String 方法
    ///
    /// # Arguments
    /// * `member` - 成员访问表达式
    /// * `args` - 参数列表
    pub fn try_generate_string_method_call(&mut self, member: &MemberAccessExpr, args: &[Expr]) -> cayResult<Option<String>> {
        // 生成对象表达式
        let obj_result = self.generate_expression(&member.object)?;
        let (obj_type, obj_val) = self.parse_typed_value(&obj_result);

        // 类型检查：先用 Cavvy 类型，再用 LLVM 类型回退（String 底层是 i8*）
        if let Some(obj_cay_type) = self.get_expression_type(&member.object) {
            if !matches!(obj_cay_type, crate::types::Type::String) {
                return Ok(None);
            }
        } else if obj_type != "i8*" {
            // Cavvy 类型不可用且 LLVM 不是 i8*（String 的底层表示）
            return Ok(None);
        }

        let method_name = member.member.as_str();
        let temp = self.new_temp();

        match method_name {
            "length" => {
                // length() - 无参数，返回 i32
                if !args.is_empty() {
                    return Err(codegen_error("String.length() takes no arguments".to_string()));
                }
                self.emit_line(&format!("  {} = call i32 @__cay_string_length(i8* {})",
                    temp, obj_val));
                Ok(Some(format!("i32 {}", temp)))
            }
            "substring" => {
                // substring(beginIndex) 或 substring(beginIndex, endIndex)
                if args.is_empty() || args.len() > 2 {
                    return Err(codegen_error("String.substring() takes 1 or 2 arguments".to_string()));
                }

                // 生成 beginIndex 参数
                let begin_result = self.generate_expression(&args[0])?;
                let (begin_type, begin_val) = self.parse_typed_value(&begin_result);
                let begin_i32 = if begin_type == "i32" {
                    begin_val.to_string()
                } else {
                    let t = self.new_temp();
                    self.emit_line(&format!("  {} = trunc {} {} to i32", t, begin_type, begin_val));
                    t
                };

                // 生成 endIndex 参数
                let end_i32 = if args.len() == 2 {
                    let end_result = self.generate_expression(&args[1])?;
                    let (end_type, end_val) = self.parse_typed_value(&end_result);
                    if end_type == "i32" {
                        end_val.to_string()
                    } else {
                        let t = self.new_temp();
                        self.emit_line(&format!("  {} = trunc {} {} to i32", t, end_type, end_val));
                        t
                    }
                } else {
                    // substring(beginIndex) - 使用字符串长度作为 endIndex
                    let len_temp = self.new_temp();
                    self.emit_line(&format!("  {} = call i32 @__cay_string_length(i8* {})",
                        len_temp, obj_val));
                    len_temp
                };

                self.emit_line(&format!("  {} = call i8* @__cay_string_substring(i8* {}, i32 {}, i32 {})",
                    temp, obj_val, begin_i32, end_i32));
                Ok(Some(format!("i8* {}", temp)))
            }
            "indexOf" => {
                // indexOf(substr) 或 indexOf(substr, startIndex)
                if args.len() < 1 || args.len() > 2 {
                    return Err(codegen_error("String.indexOf() takes 1 or 2 arguments".to_string()));
                }

                let substr_result = self.generate_expression(&args[0])?;
                let (substr_type, substr_val) = self.parse_typed_value(&substr_result);

                if substr_type != "i8*" {
                    return Err(codegen_error("String.indexOf() argument must be a string".to_string()));
                }

                if args.len() == 2 {
                    // 带起始位置的 indexOf(str, start)
                    let start_result = self.generate_expression(&args[1])?;
                    let (start_type, start_val) = self.parse_typed_value(&start_result);
                    if start_type != "i32" {
                        return Err(codegen_error("String.indexOf() second argument must be int".to_string()));
                    }
                    self.emit_line(&format!("  {} = call i32 @__cay_string_indexof_from(i8* {}, i8* {}, i32 {})",
                        temp, obj_val, substr_val, start_val));
                } else {
                    self.emit_line(&format!("  {} = call i32 @__cay_string_indexof(i8* {}, i8* {})",
                        temp, obj_val, substr_val));
                }
                Ok(Some(format!("i32 {}", temp)))
            }
            "lastIndexOf" => {
                // lastIndexOf(substr) - 返回子串最后一次出现的位置
                if args.len() != 1 {
                    return Err(codegen_error("String.lastIndexOf() takes 1 argument".to_string()));
                }

                let substr_result = self.generate_expression(&args[0])?;
                let (substr_type, substr_val) = self.parse_typed_value(&substr_result);

                if substr_type != "i8*" {
                    return Err(codegen_error("String.lastIndexOf() argument must be a string".to_string()));
                }

                self.emit_line(&format!("  {} = call i32 @__cay_string_lastindexof(i8* {}, i8* {})",
                    temp, obj_val, substr_val));
                Ok(Some(format!("i32 {}", temp)))
            }
            "charAt" => {
                // charAt(index) - 返回指定位置的字符
                if args.len() != 1 {
                    return Err(codegen_error("String.charAt() takes 1 argument".to_string()));
                }

                let index_result = self.generate_expression(&args[0])?;
                let (index_type, index_val) = self.parse_typed_value(&index_result);
                let index_i32 = if index_type == "i32" {
                    index_val.to_string()
                } else {
                    let t = self.new_temp();
                    self.emit_line(&format!("  {} = trunc {} {} to i32", t, index_type, index_val));
                    t
                };

                self.emit_line(&format!("  {} = call i8 @__cay_string_charat(i8* {}, i32 {})",
                    temp, obj_val, index_i32));
                Ok(Some(format!("i8 {}", temp)))
            }
            "replace" => {
                // replace(oldStr, newStr) - 替换所有出现的子串
                if args.len() != 2 {
                    return Err(codegen_error("String.replace() takes 2 arguments".to_string()));
                }

                let old_result = self.generate_expression(&args[0])?;
                let (old_type, old_val) = self.parse_typed_value(&old_result);
                let new_result = self.generate_expression(&args[1])?;
                let (new_type, new_val) = self.parse_typed_value(&new_result);

                if old_type != "i8*" || new_type != "i8*" {
                    return Err(codegen_error("String.replace() arguments must be strings".to_string()));
                }

                self.emit_line(&format!("  {} = call i8* @__cay_string_replace(i8* {}, i8* {}, i8* {})",
                    temp, obj_val, old_val, new_val));
                Ok(Some(format!("i8* {}", temp)))
            }
            "isEmpty" => {
                // isEmpty() - 无参数，返回 boolean (i1)
                if !args.is_empty() {
                    return Err(codegen_error("String.isEmpty() takes no arguments".to_string()));
                }
                self.emit_line(&format!("  {} = call i1 @__cay_string_isempty(i8* {})",
                    temp, obj_val));
                Ok(Some(format!("i1 {}", temp)))
            }
            "equals" => {
                // equals(other) - 比较两个字符串是否相等，返回 boolean (i1)
                if args.len() != 1 {
                    return Err(codegen_error("String.equals() takes 1 argument".to_string()));
                }

                let other_result = self.generate_expression(&args[0])?;
                let (other_type, other_val) = self.parse_typed_value(&other_result);

                if other_type != "i8*" {
                    return Err(codegen_error("String.equals() argument must be a string".to_string()));
                }

                self.emit_line(&format!("  {} = call i1 @__cay_string_equals(i8* {}, i8* {})",
                    temp, obj_val, other_val));
                Ok(Some(format!("i1 {}", temp)))
            }
            "equalsIgnoreCase" => {
                // equalsIgnoreCase(other) - 忽略大小写比较两个字符串，返回 boolean (i1)
                if args.len() != 1 {
                    return Err(codegen_error("String.equalsIgnoreCase() takes 1 argument".to_string()));
                }

                let other_result = self.generate_expression(&args[0])?;
                let (other_type, other_val) = self.parse_typed_value(&other_result);

                if other_type != "i8*" {
                    return Err(codegen_error("String.equalsIgnoreCase() argument must be a string".to_string()));
                }

                self.emit_line(&format!("  {} = call i1 @__cay_string_equals_ignorecase(i8* {}, i8* {})",
                    temp, obj_val, other_val));
                Ok(Some(format!("i1 {}", temp)))
            }
            "c_str" => {
                // c_str() - 返回C字符串指针 (i8*)
                // 在Cavvy中，String本身就是i8*，所以直接返回
                if !args.is_empty() {
                    return Err(codegen_error("String.c_str() takes no arguments".to_string()));
                }
                // String在Cavvy内部就是i8*，直接返回对象值
                Ok(Some(format!("i8* {}", obj_val)))
            }
            "startsWith" => {
                // startsWith(prefix) - 检查字符串是否以指定前缀开头
                if args.len() != 1 {
                    return Err(codegen_error("String.startsWith() takes 1 argument".to_string()));
                }

                let prefix_result = self.generate_expression(&args[0])?;
                let (prefix_type, prefix_val) = self.parse_typed_value(&prefix_result);

                if prefix_type != "i8*" {
                    return Err(codegen_error("String.startsWith() argument must be a string".to_string()));
                }

                self.emit_line(&format!("  {} = call i1 @__cay_string_startswith(i8* {}, i8* {})",
                    temp, obj_val, prefix_val));
                Ok(Some(format!("i1 {}", temp)))
            }
            "endsWith" => {
                // endsWith(suffix) - 检查字符串是否以指定后缀结尾
                if args.len() != 1 {
                    return Err(codegen_error("String.endsWith() takes 1 argument".to_string()));
                }

                let suffix_result = self.generate_expression(&args[0])?;
                let (suffix_type, suffix_val) = self.parse_typed_value(&suffix_result);

                if suffix_type != "i8*" {
                    return Err(codegen_error("String.endsWith() argument must be a string".to_string()));
                }

                self.emit_line(&format!("  {} = call i1 @__cay_string_endswith(i8* {}, i8* {})",
                    temp, obj_val, suffix_val));
                Ok(Some(format!("i1 {}", temp)))
            }
            "trim" => {
                if !args.is_empty() {
                    return Err(codegen_error("String.trim() takes no arguments".to_string()));
                }
                self.emit_line(&format!("  {} = call i8* @__cay_string_trim(i8* {})",
                    temp, obj_val));
                Ok(Some(format!("i8* {}", temp)))
            }
            _ => Ok(None), // 不是已知的 String 方法
        }
    }
}
