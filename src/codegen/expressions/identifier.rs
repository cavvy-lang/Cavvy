//! 标识符表达式代码生成
//!
//! 处理变量访问、静态字段访问和隐式 this 访问。

use crate::codegen::context::IRGenerator;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成标识符表达式代码
    ///
    /// # Arguments
    /// * `name` - 标识符名称
    pub fn generate_identifier(&mut self, name: &str) -> cayResult<String> {
        // 特殊处理 super 标识符
        if name == "super" {
            // super 应该只用于 super.methodName() 调用
            // 如果单独使用，返回 this 指针
            if let Some(this_llvm_name) = self.scope_manager.get_llvm_name("this") {
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = load i8*, i8** %{}, align 8",
                    temp, this_llvm_name));
                return Ok(format!("i8* {}", temp));
            }
            return Ok("i8* null".to_string());
        }

        // 检查是否是顶层函数 - 返回函数指针
        if self.is_top_level_function(name) {
            // 顶层函数作为函数指针使用
            let func_ptr_type = self.get_top_level_function_type(name);
            let llvm_func_type = self.type_to_llvm(&func_ptr_type);
            let func_name = format!("__toplevel_{}", name);
            // 返回函数指针 - 在 LLVM IR 中直接使用函数名作为指针
            return Ok(format!("{} @{}", llvm_func_type, func_name));
        }

        // 检查是否是类名（静态成员访问的上下文）
        if let Some(ref registry) = self.type_registry {
            if registry.class_exists(name) {
                // 类名不应该单独作为表达式使用
                // 返回一个占位符，实际使用应该在 MemberAccess 中处理
                return Ok("i8* null".to_string());
            }
        }

        // 检查是否是当前类的静态字段
        if !self.current_class.is_empty() {
            let static_key = format!("{}.{}", self.current_class, name);
            if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                let temp = self.new_temp();
                let align = self.get_type_align(&field_info.llvm_type);
                self.emit_line(&format!("  {} = load {}, {}* {}, align {}",
                    temp, field_info.llvm_type, field_info.llvm_type, field_info.name, align));
                return Ok(format!("{} {}", field_info.llvm_type, temp));
            }
        }

        // 检查是否是局部变量
        let is_local_var = self.scope_manager.get_var_type(name).is_some()
            || self.var_types.contains_key(name);
        
        if is_local_var {
            let temp = self.new_temp();
            // 优先使用作用域管理器获取变量类型和 LLVM 名称
            let (var_type, llvm_name) = if let Some(scope_type) = self.scope_manager.get_var_type(name) {
                let llvm_name = self.scope_manager.get_llvm_name(name).unwrap_or_else(|| name.to_string());
                (scope_type, llvm_name)
            } else {
                // 回退到旧系统
                let var_type = self.var_types.get(name).cloned().unwrap_or_else(|| "i64".to_string());
                (var_type, name.to_string())
            };
            let align = self.get_type_align(&var_type);  // 获取正确的对齐
            self.emit_line(&format!("  {} = load {}, {}* %{}, align {}",
                temp, var_type, var_type, llvm_name, align));
            return Ok(format!("{} {}", var_type, temp));
        }

        // 尝试作为实例字段访问（隐式 this）
        if !self.current_class.is_empty() {
            if let Some(field_info) = self.get_instance_field(&self.current_class, name).cloned() {
                // 获取 this 指针
                let this_llvm_name = self.scope_manager.get_llvm_name("this")
                    .unwrap_or_else(|| "this_s1".to_string());
                let this_temp = self.new_temp();
                self.emit_line(&format!("  {} = load i8*, i8** %{}, align 8",
                    this_temp, this_llvm_name));

                // 计算字段地址: this + offset
                let field_ptr_i8 = self.new_temp();
                self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 {}",
                    field_ptr_i8, this_temp, field_info.offset));
                
                // 将字段指针转换为正确类型的指针
                // 注意：如果llvm_type已经是指针类型（如i8**），则不需要再加*
                let field_ptr = self.new_temp();
                let ptr_type = if field_info.llvm_type.ends_with('*') {
                    field_info.llvm_type.clone()
                } else {
                    format!("{}*", field_info.llvm_type)
                };
                self.emit_line(&format!("  {} = bitcast i8* {} to {}", 
                    field_ptr, field_ptr_i8, ptr_type));
                
                // 加载字段值
                let field_val = self.new_temp();
                self.emit_line(&format!("  {} = load {}, {} {}, align {}", 
                    field_val, field_info.llvm_type, ptr_type, field_ptr,
                    self.get_type_align(&field_info.llvm_type)));
                
                return Ok(format!("{} {}", field_info.llvm_type, field_val));
            }
        }

        // 未定义的变量，回退到旧行为（可能会报错）
        let temp = self.new_temp();
        let var_type = self.var_types.get(name).cloned().unwrap_or_else(|| "i64".to_string());
        let align = self.get_type_align(&var_type);
        self.emit_line(&format!("  {} = load {}, {}* %{}, align {}",
            temp, var_type, var_type, name, align));
        Ok(format!("{} {}", var_type, temp))
    }
}
