//! Lambda 表达式代码生成
//!
//! 处理 Lambda 表达式和方法引用。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成 Lambda 表达式代码
    /// Lambda: (params) -> { body }
    ///
    /// # Arguments
    /// * `lambda` - Lambda 表达式
    pub fn generate_lambda(&mut self, lambda: &LambdaExpr) -> cayResult<String> {
        // Lambda 表达式需要生成一个匿名函数
        // 由于 LLVM IR 的复杂性，这里采用简化实现

        // 生成唯一的 Lambda 函数名
        let current_class = self.current_class.clone();
        let temp = self.new_temp().replace("%", "");
        let lambda_name = format!("__lambda_{}_{}", current_class, temp);

        // 保存当前代码缓冲区
        let saved_code = std::mem::take(&mut self.code);
        let saved_temp_counter = self.temp_counter;

        // 重置临时变量计数器
        self.temp_counter = 0;

        // 生成 Lambda 参数类型
        let mut param_types = Vec::new();
        let mut param_names = Vec::new();

        for (i, param) in lambda.params.iter().enumerate() {
            let param_type = param.param_type.as_ref()
                .map(|t| self.type_to_llvm(t))
                .unwrap_or_else(|| "i64".to_string());
            param_types.push(format!("{} %param{}", param_type, i));
            param_names.push((param.name.clone(), param_type, format!("%param{}", i)));
        }

        // 确定返回类型（简化处理，假设返回 i64）
        let return_type = "i64";

        // 生成 Lambda 函数头
        self.emit_line(&format!("\ndefine {} @{}({}) {{", return_type, lambda_name, param_types.join(", ")));
        self.emit_line("entry:");

        // 创建新的作用域
        self.scope_manager.enter_scope();

        // 添加参数到作用域
        for (name, ty, llvm_name) in &param_names {
            let local_temp = self.new_temp();
            self.emit_line(&format!("  {} = alloca {}, align {}", local_temp, ty, self.get_type_align(ty)));
            self.emit_line(&format!("  store {} {}, {}* {}, align {}", ty, llvm_name, ty, local_temp, self.get_type_align(ty)));
            self.scope_manager.declare_var(name, ty);
        }

        // 生成 Lambda 体
        let _result: Result<(), crate::error::cayError> = match &lambda.body {
            LambdaBody::Expr(expr) => {
                let val = self.generate_expression(expr)?;
                let (_, val_str) = self.parse_typed_value(&val);
                // 确保返回 i64
                if val.starts_with("i32") {
                    let temp = self.new_temp();
                    self.emit_line(&format!("  {} = sext i32 {} to i64", temp, val_str));
                    self.emit_line(&format!("  ret i64 {}", temp));
                } else {
                    self.emit_line(&format!("  ret i64 {}", val_str));
                }
                Ok(())
            }
            LambdaBody::Block(block) => {
                // 生成块中的语句
                for stmt in &block.statements {
                    self.generate_statement(stmt)?;
                }
                // 如果没有显式 return，返回 0
                self.emit_line("  ret i64 0");
                Ok(())
            }
        };

        // 退出作用域
        self.scope_manager.exit_scope();

        self.emit_line("}\n");

        // 获取 Lambda 函数代码
        let lambda_code = std::mem::take(&mut self.code);

        // 恢复之前的代码缓冲区
        self.code = saved_code;
        self.temp_counter = saved_temp_counter;

        // 将 Lambda 函数代码存储到全局函数列表
        self.lambda_functions.push(lambda_code);

        // 返回函数指针
        let temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast void (i64)* @{} to i8*", temp, lambda_name));

        Ok(format!("i8* {}", temp))
    }

    /// 生成方法引用表达式代码
    /// 方法引用: ClassName::methodName 或 obj::methodName
    ///
    /// # Arguments
    /// * `method_ref` - 方法引用表达式
    pub fn generate_method_ref(&mut self, method_ref: &MethodRefExpr) -> cayResult<String> {
        // 方法引用在 cay 中暂时作为函数指针处理
        // 返回函数指针（i8* 作为占位符）
        let temp = self.new_temp();

        if let Some(ref class_name) = method_ref.class_name {
            // 静态方法引用: ClassName::methodName
            // 生成函数名
            let fn_name = format!("{}.{}", class_name, method_ref.method_name);

            // 使用 bitcast 获取函数指针
            self.emit_line(&format!("  {} = bitcast void (i64)* @{} to i8*", temp, fn_name));
        } else if let Some(_object) = &method_ref.object {
            // 实例方法引用: obj::methodName
            // 暂时不支持，返回空指针
            self.emit_line(&format!("  {} = inttoptr i64 0 to i8*", temp));
        } else {
            // 未知类型，返回空指针
            self.emit_line(&format!("  {} = inttoptr i64 0 to i8*", temp));
        }

        Ok(format!("i8* {}", temp))
    }
}
