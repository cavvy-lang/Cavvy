//! 标识符表达式代码生成
//!
//! 处理变量访问和静态字段访问。

use crate::codegen::context::IRGenerator;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成标识符表达式代码
    ///
    /// # Arguments
    /// * `name` - 标识符名称
    pub fn generate_identifier(&mut self, name: &str) -> cayResult<String> {
        // 检查是否是类名（静态成员访问的上下文）
        if let Some(ref registry) = self.type_registry {
            if registry.class_exists(name) {
                // 类名不应该单独作为表达式使用
                // 返回一个占位符，实际使用应该在 MemberAccess 中处理
                return Ok(format!("i64 0"));
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
        Ok(format!("{} {}", var_type, temp))
    }
}
