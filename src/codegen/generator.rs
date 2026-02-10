//! cay LLVM IR 代码生成器主模块
//!
//! 本模块将 cay AST 转换为 LLVM IR 代码。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::types::Type;
use crate::error::cayResult;

impl IRGenerator {
    /// 主入口：生成程序的 LLVM IR
    pub fn generate(&mut self, program: &Program) -> cayResult<String> {
        self.emit_header();

        // 找到主类和main方法
        // 优先选择带有 @main 标记的类，如果没有则选择第一个有 main 方法的类
        let mut main_class = None;
        let mut main_method = None;
        let mut fallback_main_class = None;
        let mut fallback_main_method = None;

        // 第一遍：收集所有静态字段，并找到main方法
        for class in &program.classes {
            self.collect_static_fields(class)?;

            // 查找main方法
            for member in &class.members {
                if let crate::ast::ClassMember::Method(method) = member {
                    if method.name == "main" &&
                       method.modifiers.contains(&crate::ast::Modifier::Public) &&
                       method.modifiers.contains(&crate::ast::Modifier::Static) {
                        // 检查类是否有 @main 标记
                        if class.modifiers.contains(&crate::ast::Modifier::Main) {
                            main_class = Some(class.name.clone());
                            main_method = Some(method.clone());
                        } else if fallback_main_class.is_none() {
                            // 保存第一个找到的 main 作为回退
                            fallback_main_class = Some(class.name.clone());
                            fallback_main_method = Some(method.clone());
                        }
                    }
                }
            }
        }

        // 如果没有找到带 @main 标记的类，使用回退
        if main_class.is_none() {
            main_class = fallback_main_class;
            main_method = fallback_main_method;
        }

        // 生成静态字段的全局变量声明
        self.emit_static_field_declarations();

        // 第二遍：生成所有类方法定义（define）
        // 注意：不需要前向声明，因为所有函数都在同一个模块中定义
        for class in &program.classes {
            self.generate_class(class)?;
        }

        // 将代码缓冲区追加到输出（包含函数定义）
        self.output.push_str(&self.code);

        // 生成C入口点main函数（在函数定义之后）
        if let (Some(class_name), Some(main_method)) = (main_class, main_method) {
            self.output.push_str("; C entry point\n");
            self.output.push_str(&format!("define i32 @main() {{\n"));
            self.output.push_str("entry:\n");
            // 添加这行：强制设置 UTF-8 代码页
            self.output.push_str("  call void @SetConsoleOutputCP(i32 65001)\n");

            // 初始化静态数组字段
            self.generate_static_array_initialization();

            // 根据main方法的参数生成正确的函数名
            let main_fn_name = self.generate_method_name(&class_name, &main_method);
            self.output.push_str(&format!("  call void @{}()\n", main_fn_name));
            self.output.push_str("  ret i32 0\n");
            self.output.push_str("}\n");
            self.output.push_str("\n");
        }

        // 添加所有 Lambda 函数
        for lambda_code in &self.lambda_functions {
            self.output.push_str(lambda_code);
        }

        // 在开头插入字符串常量声明
        let string_decls = self.get_string_declarations();
        if !string_decls.is_empty() {
            // 找到第一个空行，在运行时函数之前插入字符串常量
            let mut output = self.output.clone();
            // 在运行时函数之前插入字符串常量声明
            let insert_pos = output.find("define i8* @__cay_string_concat")
                .unwrap_or(output.len());
            output.insert_str(insert_pos, &string_decls);
            self.output = output;
        }

        Ok(self.output.clone())
    }

    /// 收集类的静态字段
    fn collect_static_fields(&mut self, class: &ClassDecl) -> cayResult<()> {
        for member in &class.members {
            if let ClassMember::Field(field) = member {
                if field.modifiers.contains(&Modifier::Static) {
                    self.register_static_field(&class.name, field)?;
                }
            }
        }
        Ok(())
    }

    /// 注册静态字段
    fn register_static_field(&mut self, class_name: &str, field: &FieldDecl) -> cayResult<()> {
        let full_name = format!("@{}.{}", class_name, field.name);
        let llvm_type = self.type_to_llvm(&field.field_type);
        let size = field.field_type.size_in_bytes();

        let field_info = crate::codegen::context::StaticFieldInfo {
            name: full_name.clone(),
            llvm_type: llvm_type.clone(),
            size,
            field_type: field.field_type.clone(),
            initializer: field.initializer.clone(),
            class_name: class_name.to_string(),
            field_name: field.name.clone(),
        };

        let key = format!("{}.{}", class_name, field.name);
        self.static_field_map.insert(key, field_info.clone());
        self.static_fields.push(field_info);

        Ok(())
    }

    /// 生成静态字段的全局变量声明
    fn emit_static_field_declarations(&mut self) {
        if self.static_fields.is_empty() {
            return;
        }

        self.emit_raw("; 静态字段声明（零初始化）");
        // 克隆字段列表以避免借用问题
        let fields: Vec<_> = self.static_fields.clone();
        for field in fields {
            let align = self.get_type_align(&field.llvm_type);
            // 使用 zeroinitializer 实现零初始化
            self.emit_raw(&format!(
                "{} = private global {} zeroinitializer, align {}",
                field.name, field.llvm_type, align
            ));
        }
        self.emit_raw("");
    }

    /// 生成静态数组字段的初始化代码（在 main 函数中调用）
    fn generate_static_array_initialization(&mut self) {
        let fields: Vec<_> = self.static_fields.clone();
        for field in fields {
            // 只处理数组类型的静态字段
            if let Type::Array(elem_type) = &field.field_type {
                if let Some(init) = &field.initializer {
                    // 检查是否是 new Type[size]() 形式的初始化
                    if let Expr::ArrayCreation(array_creation) = init {
                        if !array_creation.sizes.is_empty() {
                            // 尝试计算数组大小（仅支持一维数组）
                            if let Some(size_val) = self.evaluate_const_int(&array_creation.sizes[0]) {
                                let elem_llvm_type = self.type_to_llvm(elem_type);
                                let elem_size = self.get_type_size(&elem_llvm_type);
                                let total_size = size_val as i64 * elem_size;

                                // 分配内存: calloc(1, total_size)
                                let calloc_temp = self.new_temp();
                                self.output.push_str(&format!(
                                    "  {} = call i8* @calloc(i64 1, i64 {})\n",
                                    calloc_temp, total_size
                                ));

                                // 将 i8* 转换为元素类型指针
                                let cast_temp = self.new_temp();
                                self.output.push_str(&format!(
                                    "  {} = bitcast i8* {} to {}*\n",
                                    cast_temp, calloc_temp, elem_llvm_type
                                ));

                                // 存储到静态字段
                                self.output.push_str(&format!(
                                    "  store {}* {}, {}** {}, align 8\n",
                                    elem_llvm_type, cast_temp, elem_llvm_type, field.name
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    /// 计算常量整数表达式的值
    fn evaluate_const_int(&self, expr: &Expr) -> Option<i64> {
        match expr {
            Expr::Literal(crate::ast::LiteralValue::Int32(n)) => Some(*n as i64),
            Expr::Literal(crate::ast::LiteralValue::Int64(n)) => Some(*n),
            Expr::Binary(binary) => {
                let left = self.evaluate_const_int(&binary.left)?;
                let right = self.evaluate_const_int(&binary.right)?;
                match binary.op {
                    crate::ast::BinaryOp::Add => Some(left + right),
                    crate::ast::BinaryOp::Sub => Some(left - right),
                    crate::ast::BinaryOp::Mul => Some(left * right),
                    crate::ast::BinaryOp::Div => if right != 0 { Some(left / right) } else { None },
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// 获取 LLVM 类型的大小（字节）
    fn get_type_size(&self, llvm_type: &str) -> i64 {
        match llvm_type {
            "i1" => 1,
            "i8" => 1,
            "i32" => 4,
            "i64" => 8,
            "float" => 4,
            "double" => 8,
            _ => 8, // 指针类型
        }
    }

    /// 生成类方法声明（前向声明）
    fn generate_class_declarations(&mut self, class: &ClassDecl) -> cayResult<()> {
        for member in &class.members {
            if let ClassMember::Method(method) = member {
                if !method.modifiers.contains(&Modifier::Native) {
                    self.generate_method_declaration(&class.name, method)?;
                }
            }
        }
        Ok(())
    }

    /// 生成方法声明（declare）
    fn generate_method_declaration(&mut self, class_name: &str, method: &MethodDecl) -> cayResult<()> {
        let fn_name = self.generate_method_name(class_name, method);
        let ret_type = self.type_to_llvm(&method.return_type);

        // 生成函数声明（declare）
        let decl = if method.params.is_empty() {
            format!("declare {} @{}()\n", ret_type, fn_name)
        } else {
            let params: Vec<String> = method.params.iter()
                .map(|p| self.type_to_llvm(&p.param_type))
                .collect();
            format!("declare {} @{}({})\n", ret_type, fn_name, params.join(", "))
        };
        
        // 避免重复声明
        if !self.method_declarations.contains(&decl) {
            self.method_declarations.push(decl);
        }
        Ok(())
    }

    /// 生成类代码（方法定义）
    fn generate_class(&mut self, class: &ClassDecl) -> cayResult<()> {
        for member in &class.members {
            match member {
                ClassMember::Method(method) => {
                    if !method.modifiers.contains(&Modifier::Native) {
                        self.generate_method(&class.name, method)?;
                    }
                }
                ClassMember::Field(field) => {
                    // 静态字段已在前面处理，这里处理实例字段（暂不实现）
                    if !field.modifiers.contains(&Modifier::Static) {
                        // 实例字段暂不实现
                    }
                }
            }
        }
        Ok(())
    }

    /// 生成方法代码
    fn generate_method(&mut self, class_name: &str, method: &MethodDecl) -> cayResult<()> {
        // 生成带参数签名的方法名以支持重载
        let fn_name = self.generate_method_name(class_name, method);
        self.current_function = fn_name.clone();
        self.current_class = class_name.to_string();
        self.current_return_type = self.type_to_llvm(&method.return_type);

        // 重置临时变量计数器
        self.temp_counter = 0;
        // 清除变量类型映射（每个方法都有自己的作用域）
        self.var_types.clear();
        // 重置作用域管理器
        self.scope_manager.reset();
        // 清除循环栈
        self.loop_stack.clear();
        // 注意：不要清除代码缓冲区，让它累积所有方法的代码

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
            // 使用作用域管理器声明参数变量
            let llvm_name = self.scope_manager.declare_var(&param.name, &param_type);
            self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
            self.emit_line(&format!("  store {} %{}.{}, {}* %{}",
                param_type, class_name, param.name, param_type, llvm_name));
            // 同时存储到旧系统以保持兼容性
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
