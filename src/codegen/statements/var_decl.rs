//! 变量声明代码生成
//!
//! 处理变量声明和初始化的代码生成。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::types::Type;
use crate::error::EolResult;

impl IRGenerator {
    /// 生成变量声明代码
    pub fn generate_var_decl(&mut self, var: &VarDecl) -> EolResult<()> {
        let var_type = self.type_to_llvm(&var.var_type);
        let align = self.get_type_align(&var_type);  // 获取对齐

        // 使用作用域管理器生成唯一的 LLVM 变量名
        let llvm_name = self.scope_manager.declare_var(&var.name, &var_type);

        self.emit_line(&format!("  %{} = alloca {}, align {}", llvm_name, var_type, align));
        // 同时存储到旧系统以保持兼容性
        self.var_types.insert(var.name.clone(), var_type.clone());
        // 如果变量类型是对象，记录其类名以便后续方法调用解析
        if let Type::Object(class_name) = &var.var_type {
            self.var_class_map.insert(var.name.clone(), class_name.clone());
        }

        if let Some(init) = var.initializer.as_ref() {
            let value = self.generate_expression(init)?;
            let (value_type, val) = self.parse_typed_value(&value);

            // 如果值类型与变量类型不匹配，需要转换
            if value_type != var_type {
                let temp = self.new_temp();

                // 浮点类型转换
                if value_type == "double" && var_type == "float" {
                    // double -> float 转换
                    self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                    let align = self.get_type_align("float");
                    self.emit_line(&format!("  store float {}, float* %{}, align {}", temp, llvm_name, align));
                } else if value_type == "float" && var_type == "double" {
                    // float -> double 转换
                    self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                    let align = self.get_type_align("double");
                    self.emit_line(&format!("  store double {}, double* %{}, align {}", temp, llvm_name, align));
                }
                // 指针类型转换 (bitcast)
                else if value_type.ends_with("*") && var_type.ends_with("*") {
                    self.emit_line(&format!("  {} = bitcast {} {} to {}",
                        temp, value_type, val, var_type));
                    self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                }
                // 整数类型转换
                else if value_type.starts_with("i") && var_type.starts_with("i") && !value_type.ends_with("*") && !var_type.ends_with("*") {
                    let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                    let to_bits: u32 = var_type.trim_start_matches('i').parse().unwrap_or(64);

                    if to_bits > from_bits {
                        // 符号扩展
                        self.emit_line(&format!("  {} = sext {} {} to {}",
                            temp, value_type, val, var_type));
                    } else {
                        // 截断
                        self.emit_line(&format!("  {} = trunc {} {} to {}",
                            temp, value_type, val, var_type));
                    }
                    self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                } else {
                    // 类型不兼容，直接存储（可能会出错）
                    self.emit_line(&format!("  store {}, {}* %{}",
                        value, var_type, llvm_name));
                }
            } else {
                // 类型匹配，直接存储
                self.emit_line(&format!("  store {}, {}* %{}",
                    value, var_type, llvm_name));
            }
        }

        Ok(())
    }
}
