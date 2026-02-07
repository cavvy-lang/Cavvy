//! EOL LLVM IR 代码生成器主模块
//!
//! 本模块将 EOL AST 转换为 LLVM IR 代码。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::types::Type;
use crate::error::EolResult;

impl IRGenerator {
    /// 主入口：生成程序的 LLVM IR
    pub fn generate(&mut self, program: &Program) -> EolResult<String> {
        self.emit_header();

        // 找到主类并记录
        let mut main_class = None;

        // 生成所有类方法
        for class in &program.classes {
            if class.members.iter().any(|m| {
                if let crate::ast::ClassMember::Method(method) = m {
                    method.name == "main" &&
                    method.modifiers.contains(&crate::ast::Modifier::Public) &&
                    method.modifiers.contains(&crate::ast::Modifier::Static)
                } else {
                    false
                }
            }) {
                main_class = Some(class.name.clone());
            }
            self.generate_class(class)?;
        }

        // 生成C入口点main函数
        if let Some(class_name) = main_class {
            self.emit_raw("; C entry point");
            self.emit_raw(&format!("define i32 @main() {{"));
            self.emit_raw("entry:");
            // 添加这行：强制设置 UTF-8 代码页
            self.emit_raw("  call void @SetConsoleOutputCP(i32 65001)");
            self.emit_raw(&format!("  call void @{}.{:}()", class_name, "main"));
            self.emit_raw("  ret i32 0");
            self.emit_raw("}");
            self.emit_raw("");
        }

        // 在开头插入字符串常量声明
        let string_decls = self.get_string_declarations();
        if !string_decls.is_empty() {
            // 找到第一个空行，在运行时函数之前插入字符串常量
            let mut output = self.output.clone();
            // 在运行时函数之前插入字符串常量声明
            let insert_pos = output.find("define i8* @__eol_string_concat")
                .unwrap_or(output.len());
            output.insert_str(insert_pos, &string_decls);
            self.output = output;
        }

        Ok(self.output.clone())
    }

    /// 生成类代码
    fn generate_class(&mut self, class: &ClassDecl) -> EolResult<()> {
        for member in &class.members {
            match member {
                ClassMember::Method(method) => {
                    if !method.modifiers.contains(&Modifier::Native) {
                        self.generate_method(&class.name, method)?;
                    }
                }
                ClassMember::Field(_) => {
                    // 静态字段暂不实现
                }
            }
        }
        Ok(())
    }

    /// 生成方法代码
    fn generate_method(&mut self, class_name: &str, method: &MethodDecl) -> EolResult<()> {
        let fn_name = format!("{}.{}", class_name, method.name);
        self.current_function = fn_name.clone();
        self.current_class = class_name.to_string();
        self.current_return_type = self.type_to_llvm(&method.return_type);

        // 重置临时变量计数器
        self.temp_counter = 0;
        // 清除变量类型映射（每个方法都有自己的作用域）
        self.var_types.clear();
        // 清除循环栈
        self.loop_stack.clear();

        // 函数签名
        let ret_type = self.current_return_type.clone();
        let params: Vec<String> = method.params.iter()
            .map(|p| format!("{} %{}.{}", self.type_to_llvm(&p.param_type), class_name, p.name))
            .collect();

        self.emit_line(&format!("define {} @{}({}) {{",
            ret_type, fn_name, params.join(", ")));
        self.indent += 1;

        // 入口标签
        self.emit_line("entry:");

        // 为参数分配局部变量
        for param in &method.params {
            let param_type = self.type_to_llvm(&param.param_type);
            self.emit_line(&format!("  %{} = alloca {}", param.name, param_type));
            self.emit_line(&format!("  store {} %{}.{}, {}* %{}",
                param_type, class_name, param.name, param_type, param.name));
            // 存储参数类型信息
            self.var_types.insert(param.name.clone(), param_type);
        }

        // 生成方法体
        if let Some(body) = method.body.as_ref() {
            self.generate_block(body)?;
        }

        // 如果函数没有返回语句且返回void，添加隐式返回
        if method.return_type == Type::Void {
            self.emit_line("  ret void");
        }

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }
}

/// 默认实现
impl Default for IRGenerator {
    fn default() -> Self {
        Self::new()
    }
}
