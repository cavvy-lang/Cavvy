use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::types::Type;
use crate::error::cayResult;

impl IRGenerator {
    pub fn generate(&mut self, program: &Program) -> cayResult<String> {
        self.emit_header();

        let mut main_class = None;
        let mut main_method = None;
        let mut fallback_main_class = None;
        let mut fallback_main_method = None;
        let mut top_level_main = None;

        // 检查是否有顶层 main 函数
        for func in &program.top_level_functions {
            if func.name == "main" {
                top_level_main = Some(func.clone());
                break;
            }
        }

        // 首先计算所有类的实例布局
        for class in &program.classes {
            let instance_fields: Vec<_> = class.members.iter()
                .filter_map(|m| match m {
                    ClassMember::Field(f) => Some(f.clone()),
                    _ => None,
                })
                .collect();
            self.compute_class_layout(&class.name, &instance_fields);
        }

        for class in &program.classes {
            self.collect_static_fields(class)?;

            for member in &class.members {
                if let crate::ast::ClassMember::Method(method) = member {
                    if method.name == "main" &&
                       method.modifiers.contains(&crate::ast::Modifier::Public) &&
                       method.modifiers.contains(&crate::ast::Modifier::Static) {
                        if class.modifiers.contains(&crate::ast::Modifier::Main) {
                            main_class = Some(class.name.clone());
                            main_method = Some(method.clone());
                        } else if fallback_main_class.is_none() {
                            fallback_main_class = Some(class.name.clone());
                            fallback_main_method = Some(method.clone());
                        }
                    }
                }
            }
        }

        // 优先使用顶层 main 函数
        let use_top_level_main = top_level_main.is_some();

        if main_class.is_none() && !use_top_level_main {
            main_class = fallback_main_class;
            main_method = fallback_main_method;
        }

        self.emit_static_field_declarations();
        self.register_type_identifiers(program);

        // 生成顶层函数
        for func in &program.top_level_functions {
            self.generate_top_level_function(func)?;
        }

        for class in &program.classes {
            self.generate_class(class)?;
        }

        self.output.push_str(&self.code);

        // 生成 C entry point
        if use_top_level_main {
            // 使用顶层 main 函数
            let func = top_level_main.unwrap();
            self.output.push_str("; C entry point\n");
            self.output.push_str(&format!("define i32 @main() {{\n"));
            self.output.push_str("entry:\n");
            // 只在 Windows 平台上设置控制台代码页
            if cfg!(target_os = "windows") {
                self.output.push_str("  call void @SetConsoleOutputCP(i32 65001)\n");
            }
            self.generate_static_array_initialization();
            let main_fn_name = self.generate_top_level_function_name(&func.name);
            if func.return_type == Type::Void {
                self.output.push_str(&format!("  call void @{}()\n", main_fn_name));
                self.output.push_str("  ret i32 0\n");
            } else {
                self.output.push_str(&format!("  %ret = call i32 @{}()\n", main_fn_name));
                self.output.push_str("  ret i32 %ret\n");
            }
            self.output.push_str("}\n");
            self.output.push_str("\n");
        } else if let (Some(class_name), Some(main_method)) = (main_class, main_method) {
            self.output.push_str("; C entry point\n");
            self.output.push_str(&format!("define i32 @main() {{\n"));
            self.output.push_str("entry:\n");
            // 只在 Windows 平台上设置控制台代码页
            if cfg!(target_os = "windows") {
                self.output.push_str("  call void @SetConsoleOutputCP(i32 65001)\n");
            }
            self.generate_static_array_initialization();
            let main_fn_name = self.generate_method_name(&class_name, &main_method);
            self.output.push_str(&format!("  call void @{}()\n", main_fn_name));
            self.output.push_str("  ret i32 0\n");
            self.output.push_str("}\n");
            self.output.push_str("\n");
        }

        for lambda_code in &self.lambda_functions {
            self.output.push_str(lambda_code);
        }

        let string_decls = self.get_string_declarations();
        let type_id_decls = self.emit_type_id_declarations();

        let mut output = self.output.clone();
        let insert_pos = output.find("define i8* @__cay_string_concat")
            .unwrap_or(output.len());

        let mut decls = String::new();
        if !type_id_decls.is_empty() {
            decls.push_str(&type_id_decls);
            decls.push_str("\n");
        }
        if !string_decls.is_empty() {
            decls.push_str(&string_decls);
        }

        if !decls.is_empty() {
            output.insert_str(insert_pos, &decls);
        }

        self.output = output;

        Ok(self.output.clone())
    }

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

    fn register_static_field(&mut self, class_name: &str, field: &FieldDecl) -> cayResult<()> {
        let full_name = format!("@{}.{}_s", class_name, field.name);
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

    fn emit_static_field_declarations(&mut self) {
        if self.static_fields.is_empty() {
            return;
        }

        self.emit_raw("; Static field declarations");
        let fields: Vec<_> = self.static_fields.clone();
        for field in fields {
            let align = self.get_type_align(&field.llvm_type);
            
            let init_value = if let Some(init) = &field.initializer {
                self.evaluate_const_initializer(init, &field.llvm_type)
            } else {
                None
            };
            
            if let Some(val) = init_value {
                self.emit_raw(&format!(
                    "{} = private global {} {}, align {}",
                    field.name, field.llvm_type, val, align
                ));
            } else {
                self.emit_raw(&format!(
                    "{} = private global {} zeroinitializer, align {}",
                    field.name, field.llvm_type, align
                ));
            }
        }
        self.emit_raw("");
    }

    fn register_type_identifiers(&mut self, program: &Program) {
        for interface in &program.interfaces {
            self.register_type_id(&interface.name, None, Vec::new());
        }
        for class in &program.classes {
            let parent_name = class.parent.as_deref();
            let interfaces = class.interfaces.clone();
            self.register_type_id(&class.name, parent_name, interfaces);
        }
    }

    fn evaluate_const_initializer(&self, expr: &Expr, llvm_type: &str) -> Option<String> {
        match expr {
            Expr::Literal(crate::ast::LiteralValue::Int32(n)) => Some(n.to_string()),
            Expr::Literal(crate::ast::LiteralValue::Int64(n)) => Some(n.to_string()),
            Expr::Literal(crate::ast::LiteralValue::Float32(f)) => {
                if f.is_nan() {
                    Some("0x7FC00000".to_string())
                } else if f.is_infinite() {
                    if *f > 0.0 {
                        Some("0x7F800000".to_string())
                    } else {
                        Some("0xFF800000".to_string())
                    }
                } else {
                    Some(format!("{:.6e}", f))
                }
            }
            Expr::Literal(crate::ast::LiteralValue::Float64(f)) => {
                if f.is_nan() {
                    Some("0x7FF8000000000000".to_string())
                } else if f.is_infinite() {
                    if *f > 0.0 {
                        Some("0x7FF0000000000000".to_string())
                    } else {
                        Some("0xFFF0000000000000".to_string())
                    }
                } else {
                    Some(format!("{:.6e}", f))
                }
            }
            Expr::Literal(crate::ast::LiteralValue::Bool(b)) => Some(if *b { "1".to_string() } else { "0".to_string() }),
            Expr::Binary(binary) => {
                let left = self.evaluate_const_int(&binary.left)?;
                let right = self.evaluate_const_int(&binary.right)?;
                let result = match binary.op {
                    crate::ast::BinaryOp::Add => left + right,
                    crate::ast::BinaryOp::Sub => left - right,
                    crate::ast::BinaryOp::Mul => left * right,
                    crate::ast::BinaryOp::Div => if right != 0 { left / right } else { return None },
                    _ => return None,
                };
                Some(result.to_string())
            }
            _ => None,
        }
    }

    fn generate_static_array_initialization(&mut self) {
        let fields: Vec<_> = self.static_fields.clone();
        for field in fields {
            if let Type::Array(elem_type) = &field.field_type {
                if let Some(init) = &field.initializer {
                    if let Expr::ArrayCreation(array_creation) = init {
                        if !array_creation.sizes.is_empty() {
                            if let Some(size_val) = self.evaluate_const_int(&array_creation.sizes[0]) {
                                let elem_llvm_type = self.type_to_llvm(elem_type);
                                let elem_size = self.get_type_size(&elem_llvm_type);
                                let total_size = size_val as i64 * elem_size;

                                let calloc_temp = self.new_temp();
                                self.output.push_str(&format!(
                                    "  {} = call i8* @calloc(i64 1, i64 {})\n",
                                    calloc_temp, total_size
                                ));

                                let cast_temp = self.new_temp();
                                self.output.push_str(&format!(
                                    "  {} = bitcast i8* {} to {}*\n",
                                    cast_temp, calloc_temp, elem_llvm_type
                                ));

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

    fn get_type_size(&self, llvm_type: &str) -> i64 {
        match llvm_type {
            "i1" => 1,
            "i8" => 1,
            "i32" => 4,
            "i64" => 8,
            "float" => 4,
            "double" => 8,
            _ => 8,
        }
    }

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

    fn generate_method_declaration(&mut self, class_name: &str, method: &MethodDecl) -> cayResult<()> {
        let fn_name = self.generate_method_name(class_name, method);
        let ret_type = self.type_to_llvm(&method.return_type);

        let decl = if method.params.is_empty() {
            format!("declare {} @{}()\n", ret_type, fn_name)
        } else {
            let params: Vec<String> = method.params.iter()
                .map(|p| self.type_to_llvm(&p.param_type))
                .collect();
            format!("declare {} @{}({})\n", ret_type, fn_name, params.join(", "))
        };
        
        if !self.method_declarations.contains(&decl) {
            self.method_declarations.push(decl);
        }
        Ok(())
    }

    fn generate_class(&mut self, class: &ClassDecl) -> cayResult<()> {
        for member in &class.members {
            match member {
                ClassMember::Method(method) => {
                    if !method.modifiers.contains(&Modifier::Native) {
                        self.generate_method(&class.name, method)?;
                    }
                }
                ClassMember::Field(field) => {
                    if !field.modifiers.contains(&Modifier::Static) {
                    }
                }
                ClassMember::Constructor(ctor) => {
                    self.generate_constructor(&class.name, ctor)?;
                }
                ClassMember::Destructor(dtor) => {
                    self.generate_destructor(&class.name, dtor)?;
                }
                ClassMember::InstanceInitializer(_block) => {
                }
                ClassMember::StaticInitializer(block) => {
                    self.generate_static_initializer(&class.name, block)?;
                }
            }
        }
        Ok(())
    }

    fn generate_method(&mut self, class_name: &str, method: &MethodDecl) -> cayResult<()> {
        let fn_name = self.generate_method_name(class_name, method);
        self.current_function = fn_name.clone();
        self.current_class = class_name.to_string();
        self.current_return_type = self.type_to_llvm(&method.return_type);

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        let ret_type = self.current_return_type.clone();
        let is_static = method.modifiers.contains(&Modifier::Static);
        
        let mut params: Vec<String> = Vec::new();
        
        // 实例方法添加 this 参数
        if !is_static {
            params.push("i8* %this".to_string());
        }
        
        for param in &method.params {
            params.push(format!("{} %{}.{}", self.type_to_llvm(&param.param_type), class_name, param.name));
        }

        self.emit_line(&format!("define {} @{}({}) {{",
            ret_type, fn_name, params.join(", ")));
        self.indent += 1;

        self.emit_line("entry:");
        
        // 实例方法声明 this 变量
        if !is_static {
            // 使用 this_ptr 作为变量名，避免与参数 %this 冲突
            let this_llvm_name = self.scope_manager.declare_var("this_ptr", "i8*");
            self.emit_line(&format!("  %{} = alloca i8*", this_llvm_name));
            self.emit_line(&format!("  store i8* %this, i8** %{}", this_llvm_name));
            self.var_types.insert("this".to_string(), "i8*".to_string());
        }

        for param in &method.params {
            let param_type = self.type_to_llvm(&param.param_type);
            let llvm_name = self.scope_manager.declare_var(&param.name, &param_type);
            self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
            self.emit_line(&format!("  store {} %{}.{}, {}* %{}",
                param_type, class_name, param.name, param_type, llvm_name));
            self.var_types.insert(param.name.clone(), param_type);
        }

        if let Some(body) = method.body.as_ref() {
            self.generate_block(body)?;
        }

        if method.return_type == Type::Void {
            self.emit_line("  ret void");
        }

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    fn generate_constructor(&mut self, class_name: &str, ctor: &crate::ast::ConstructorDecl) -> cayResult<()> {
        let fn_name = self.generate_constructor_name(class_name, ctor);
        self.current_function = fn_name.clone();
        self.current_class = class_name.to_string();
        self.current_return_type = "void".to_string();

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        let params: Vec<String> = ctor.params.iter()
            .map(|p| format!("{} %{}.{}_param", self.type_to_llvm(&p.param_type), class_name, p.name))
            .collect();

        let mut all_params = vec![format!("i8* %this")];
        all_params.extend(params);

        self.emit_line(&format!("define void @{}({}) {{",
            fn_name, all_params.join(", ")));
        self.indent += 1;

        self.emit_line("entry:");

        let this_llvm_name = self.scope_manager.declare_var("this", "i8*");
        self.emit_line(&format!("  %{} = alloca i8*", this_llvm_name));
        self.emit_line(&format!("  store i8* %this, i8** %{}", this_llvm_name));
        self.var_types.insert("this".to_string(), "i8*".to_string());

        for param in &ctor.params {
            let param_type = self.type_to_llvm(&param.param_type);
            let llvm_name = self.scope_manager.declare_var(&param.name, &param_type);
            self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
            self.emit_line(&format!("  store {} %{}.{}_param, {}* %{}",
                param_type, class_name, param.name, param_type, llvm_name));
            self.var_types.insert(param.name.clone(), param_type);
        }

        if let Some(ref call) = ctor.constructor_call {
            match call {
                crate::ast::ConstructorCall::This(args) => {
                    let target_ctor_name = self.generate_constructor_call_name(class_name, args.len());
                    let mut arg_strs = vec!["i8* %this".to_string()];
                    for arg in args {
                        let arg_val = self.generate_expression(arg)?;
                        arg_strs.push(arg_val);
                    }
                    self.emit_line(&format!("  call void @{}({})",
                        target_ctor_name, arg_strs.join(", ")));
                }
                crate::ast::ConstructorCall::Super(args) => {
                    if let Some(ref registry) = self.type_registry {
                        if let Some(class_info) = registry.get_class(class_name) {
                            if let Some(ref parent_name) = class_info.parent {
                                let parent_ctor_name = format!("{}.__ctor", parent_name);
                                let mut arg_strs = vec!["i8* %this".to_string()];
                                for arg in args {
                                    let arg_val = self.generate_expression(arg)?;
                                    arg_strs.push(arg_val);
                                }
                                self.emit_line(&format!("  call void @{}({})",
                                    parent_ctor_name, arg_strs.join(", ")));
                            }
                        }
                    }
                }
            }
        }

        self.generate_block(&ctor.body)?;

        self.emit_line("  ret void");

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    fn generate_destructor(&mut self, class_name: &str, dtor: &crate::ast::DestructorDecl) -> cayResult<()> {
        let fn_name = format!("{}.__dtor", class_name);
        self.current_function = fn_name.clone();
        self.current_class = class_name.to_string();
        self.current_return_type = "void".to_string();

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        self.emit_line(&format!("define void @{}(i8* %this) {{", fn_name));
        self.indent += 1;

        self.emit_line("entry:");

        let this_llvm_name = self.scope_manager.declare_var("this", "i8*");
        self.emit_line(&format!("  %{} = alloca i8*", this_llvm_name));
        self.emit_line(&format!("  store i8* %this, i8** %{}", this_llvm_name));
        self.var_types.insert("this".to_string(), "i8*".to_string());

        self.generate_block(&dtor.body)?;

        self.emit_line("  ret void");

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    fn generate_static_initializer(&mut self, class_name: &str, block: &crate::ast::Block) -> cayResult<()> {
        let fn_name = format!("{}.__static_init", class_name);
        self.current_function = fn_name.clone();
        self.current_class = class_name.to_string();
        self.current_return_type = "void".to_string();

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        self.emit_line(&format!("define void @{}() {{", fn_name));
        self.indent += 1;

        self.emit_line("entry:");

        self.generate_block(block)?;

        self.emit_line("  ret void");

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    fn generate_constructor_name(&self, class_name: &str, ctor: &crate::ast::ConstructorDecl) -> String {
        if ctor.params.is_empty() {
            format!("{}.__ctor", class_name)
        } else {
            let param_types: Vec<String> = ctor.params.iter()
                .map(|p| self.type_to_signature(&p.param_type))
                .collect();
            format!("{}.__ctor_{}", class_name, param_types.join("_"))
        }
    }

    fn generate_constructor_call_name(&self, class_name: &str, arg_count: usize) -> String {
        if arg_count == 0 {
            format!("{}.__ctor", class_name)
        } else {
            let param_types: Vec<String> = (0..arg_count).map(|_| "i".to_string()).collect();
            format!("{}.__ctor_{}", class_name, param_types.join("_"))
        }
    }

    /// 生成顶层函数
    fn generate_top_level_function(&mut self, func: &crate::ast::TopLevelFunction) -> cayResult<()> {
        let fn_name = self.generate_top_level_function_name(&func.name);
        self.current_function = fn_name.clone();
        self.current_class = String::new(); // 顶层函数没有类
        self.current_return_type = self.type_to_llvm(&func.return_type);

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        let ret_type = self.current_return_type.clone();
        let params: Vec<String> = func.params.iter()
            .map(|p| format!("{} %{}.param", self.type_to_llvm(&p.param_type), p.name))
            .collect();

        self.emit_line(&format!("define {} @{}({}) {{",
            ret_type, fn_name, params.join(", ")));
        self.indent += 1;

        self.emit_line("entry:");

        for param in &func.params {
            let param_type = self.type_to_llvm(&param.param_type);
            let llvm_name = self.scope_manager.declare_var(&param.name, &param_type);
            self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
            self.emit_line(&format!("  store {} %{}.param, {}* %{}",
                param_type, param.name, param_type, llvm_name));
            self.var_types.insert(param.name.clone(), param_type);
        }

        self.generate_block(&func.body)?;

        if func.return_type == Type::Void {
            self.emit_line("  ret void");
        }

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }
}