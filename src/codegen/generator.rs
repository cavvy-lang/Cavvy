use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::types::Type;
use crate::error::cayResult;

/// Windows 控制台 UTF-8 代码页
const UTF8_CODEPAGE: i32 = 65001;

/// 平台抽象层 - 处理不同操作系统的差异
#[derive(Debug, Clone)]
pub struct PlatformAbstraction {
    pub target_os: String,
    pub features: Vec<String>,
    pub defines: Vec<String>,
    pub undefines: Vec<String>,
}

impl PlatformAbstraction {
    pub fn new(target_os: &str) -> Self {
        Self {
            target_os: target_os.to_string(),
            features: Vec::new(),
            defines: Vec::new(),
            undefines: Vec::new(),
        }
    }
    
    /// 添加平台特性
    pub fn with_feature(mut self, feature: &str) -> Self {
        self.features.push(feature.to_string());
        self
    }
    
    /// 添加宏定义
    pub fn with_define(mut self, define: &str) -> Self {
        self.defines.push(define.to_string());
        self
    }
    
    /// 取消宏定义
    pub fn with_undefine(mut self, undefine: &str) -> Self {
        self.undefines.push(undefine.to_string());
        self
    }
    
    /// 生成平台特定的初始化代码
    pub fn generate_platform_init(&self) -> String {
        match self.target_os.as_str() {
            "windows" => {
                if self.features.contains(&"console_utf8".to_string()) {
                    return format!("  call void @SetConsoleOutputCP(i32 {})\n", UTF8_CODEPAGE);
                }
            }
            "linux" | "macos" => {
                // Linux/macOS 使用 setlocale 设置 UTF-8
                if self.features.contains(&"console_utf8".to_string()) {
                    return "  call void @setlocale(i32 0, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @.str.locale, i32 0, i32 0))\n".to_string();
                }
            }
            _ => {}
        }
        String::new()
    }
    
    /// 生成平台特定的运行时声明
    pub fn generate_platform_declarations(&self) -> String {
        let mut declarations = String::new();
        
        match self.target_os.as_str() {
            "windows" => {
                declarations.push_str("declare dllimport void @SetConsoleOutputCP(i32)\n");
            }
            "linux" | "macos" => {
                declarations.push_str("declare i8* @setlocale(i32, i8*)\n");
                declarations.push_str("@.str.locale = private unnamed_addr constant [6 x i8] c\"C.UTF-8\"\\00\n");
            }
            _ => {}
        }
        
        declarations
    }
}

impl IRGenerator {
    /// 生成IR代码
    /// 
    /// # Arguments
    /// * `program` - AST程序
    /// * `source_file` - 源文件路径（用于源映射）
    pub fn generate(&mut self, program: &Program, source_file: &str) -> cayResult<String> {
        // 扁平化 namespace 声明
        let program = program.flatten_namespaces();

        // 设置源文件路径
        self.source_file = source_file.to_string();
        
        self.emit_header();

        // 设置 extern 声明并构建索引
        self.set_extern_declarations(program.extern_declarations.clone());

        // 设置顶层函数列表
        self.top_level_functions = program.top_level_functions.clone();

        // 设置类型别名
        for type_alias in &program.type_aliases {
            self.type_aliases.insert(type_alias.name.clone(), type_alias.target_type.clone());
        }

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

        // 首先计算所有类的实例布局（按继承顺序：父类先于子类）
        // 使用拓扑排序确保父类先于子类计算
        let mut computed = std::collections::HashSet::new();
        let classes: std::collections::HashMap<String, &crate::ast::ClassDecl> = program.classes.iter()
            .map(|c| (c.name.clone(), c))
            .collect();
        
        fn compute_layout_recursive<'a>(
            class: &'a crate::ast::ClassDecl,
            classes: &std::collections::HashMap<String, &'a crate::ast::ClassDecl>,
            computed: &mut std::collections::HashSet<String>,
            generator: &mut IRGenerator
        ) {
            // 构建基础限定名（不含泛型参数）用于追踪和存储
            let base_qname = if class.namespace_path.is_empty() {
                class.name.clone()
            } else {
                format!("{}::{}", class.namespace_path.join("::"), class.name)
            };
            if computed.contains(&base_qname) {
                return;
            }
            
            // 先计算父类
            if let Some(ref parent_name) = class.parent {
                if let Some(parent_class) = classes.get(parent_name) {
                    compute_layout_recursive(parent_class, classes, computed, generator);
                }
            }
            
            // 计算当前类（使用基础类名存储布局）
            let instance_fields: Vec<_> = class.members.iter()
                .filter_map(|m| match m {
                    ClassMember::Field(f) => Some(f.clone()),
                    _ => None,
                })
                .collect();
            generator.compute_class_layout(&base_qname, &instance_fields, class.parent.as_deref());
            computed.insert(base_qname);
        }
        
        for class in &program.classes {
            compute_layout_recursive(class, &classes, &mut computed, self);
        }

        // 计算 struct 的布局（值类型，无继承）
        for struct_decl in &program.structs {
            let qname = if struct_decl.namespace_path.is_empty() {
                struct_decl.name.clone()
            } else {
                format!("{}::{}", struct_decl.namespace_path.join("::"), struct_decl.name)
            };
            let instance_fields: Vec<_> = struct_decl.fields.iter().cloned().collect();
            self.compute_class_layout(&qname, &instance_fields, None);
        }

        for class in &program.classes {
            let qname = if class.namespace_path.is_empty() {
                class.name.clone()
            } else {
                format!("{}::{}", class.namespace_path.join("::"), class.name)
            };
            // 记录类的命名空间路径，用于方法名改编（使用限定名作为 key，避免同名类冲突）
            if !class.namespace_path.is_empty() {
                self.class_namespaces.insert(qname.clone(), class.namespace_path.clone());
            }
            self.collect_static_fields(class, &qname)?;

            for member in &class.members {
                if let crate::ast::ClassMember::Method(method) = member {
                    if method.name == "main" &&
                       method.modifiers.contains(&crate::ast::Modifier::Public) &&
                       method.modifiers.contains(&crate::ast::Modifier::Static) {
                        if class.modifiers.contains(&crate::ast::Modifier::Main) {
                            main_class = Some(qname.clone());
                            main_method = Some(method.clone());
                        } else if fallback_main_class.is_none() {
                            fallback_main_class = Some(qname.clone());
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

        // 收集所有 @Test 方法（用于测试模式）
        if self.test_mode {
            for class in &program.classes {
                for member in &class.members {
                    if let crate::ast::ClassMember::Method(method) = member {
                        if method.modifiers.contains(&crate::ast::Modifier::Test) {
                            let qname = if class.namespace_path.is_empty() {
                                class.name.clone()
                            } else {
                                format!("{}::{}", class.namespace_path.join("::"), class.name)
                            };
                            self.test_methods.push((qname, method.name.clone()));
                        }
                    }
                }
            }
        }

        self.emit_static_field_declarations();
        self.register_type_identifiers(&program);

        // 生成 extern 函数声明
        for extern_decl in &program.extern_declarations {
            self.generate_extern_declaration(extern_decl)?;
        }

        // 生成顶层函数
        for func in &program.top_level_functions {
            self.generate_top_level_function(func)?;
        }

        for class in &program.classes {
            self.generate_class(class)?;
        }

        // 生成 struct 方法
        for struct_decl in &program.structs {
            self.generate_struct_methods(struct_decl)?;
        }

        self.output.push_str(&self.code);

        // 生成跨平台 C entry point
        if use_top_level_main {
            // 使用顶层 main 函数
            let func = top_level_main.expect("use_top_level_main 为 true 时 top_level_main 应为 Some");
            let has_args = !func.params.is_empty();
            
            self.output.push_str("; Cross-platform C entry point\n");
            if has_args {
                // 带参数的 main 函数: main(String[] args) -> 接收 argc, argv
                self.output.push_str("define i32 @main(i32 %argc, i8** %argv) {\n");
            } else {
                // 无参数的 main 函数
                self.output.push_str(&format!("define i32 @main() {{\n"));
            }
            self.output.push_str("entry:\n");
            
            // 使用平台配置生成初始化代码
            let platform_init = self.generate_platform_init();
            if !platform_init.is_empty() {
                self.output.push_str(&platform_init);
            }
            
            self.generate_static_array_initialization();
            let main_fn_name = self.generate_top_level_function_name(&func.name);
            
            if has_args {
                // 将 argc, argv 转换为 String[]
                self.output.push_str("  ; Convert argc/argv to String[]\n");
                self.output.push_str("  %args_array = call i8** @__cay_create_string_array(i32 %argc)\n");
                self.output.push_str("  br label %args_loop_init\n\n");
                
                // 循环初始化
                self.output.push_str("args_loop_init:\n");
                self.output.push_str("  %i = alloca i32\n");
                self.output.push_str("  store i32 0, i32* %i\n");
                self.output.push_str("  br label %args_loop_cond\n\n");
                
                // 循环条件
                self.output.push_str("args_loop_cond:\n");
                self.output.push_str("  %i_val = load i32, i32* %i\n");
                self.output.push_str("  %cond = icmp slt i32 %i_val, %argc\n");
                self.output.push_str("  br i1 %cond, label %args_loop_body, label %args_loop_end\n\n");
                
                // 循环体
                self.output.push_str("args_loop_body:\n");
                self.output.push_str("  %idx = load i32, i32* %i\n");
                self.output.push_str("  %arg_ptr = getelementptr i8*, i8** %argv, i32 %idx\n");
                self.output.push_str("  %arg_cstr = load i8*, i8** %arg_ptr\n");
                self.output.push_str("  %arg_str = call i8* @__cay_cstr_to_string(i8* %arg_cstr)\n");
                self.output.push_str("  call void @__cay_array_set_ref(i8** %args_array, i32 %idx, i8* %arg_str)\n");
                self.output.push_str("  %next_i = add i32 %idx, 1\n");
                self.output.push_str("  store i32 %next_i, i32* %i\n");
                self.output.push_str("  br label %args_loop_cond\n\n");
                
                // 循环结束
                self.output.push_str("args_loop_end:\n");
                
                if func.return_type == Type::Void {
                    self.output.push_str(&format!("  call void @{}(i8** %args_array)\n", main_fn_name));
                    self.output.push_str("  ret i32 0\n");
                } else {
                    self.output.push_str(&format!("  %ret = call i32 @{}(i8** %args_array)\n", main_fn_name));
                    self.output.push_str("  ret i32 %ret\n");
                }
            } else if func.return_type == Type::Void {
                self.output.push_str(&format!("  call void @{}()\n", main_fn_name));
                self.output.push_str("  ret i32 0\n");
            } else {
                self.output.push_str(&format!("  %ret = call i32 @{}()\n", main_fn_name));
                self.output.push_str("  ret i32 %ret\n");
            }
            self.output.push_str("}\n");
            self.output.push_str("\n");
        } else if let (Some(class_name), Some(main_method)) = (main_class, main_method) {
            // 检查 main 方法是否有参数
            let has_args = !main_method.params.is_empty();
            let returns_int = main_method.return_type == Type::Int32;

            self.output.push_str("; C entry point\n");
            if has_args {
                // 带参数的 main 方法: main(String[] args)
                self.output.push_str("define i32 @main(i32 %argc, i8** %argv) {\n");
            } else {
                // 无参数的 main 方法
                self.output.push_str("define i32 @main() {\n");
            }
            self.output.push_str("entry:\n");
            // 只在 Windows 目标平台上设置控制台代码页
            if self.is_windows_target() {
                self.output.push_str(&format!("  call void @SetConsoleOutputCP(i32 {})\n", UTF8_CODEPAGE));
            }
            self.generate_static_array_initialization();
            let main_fn_name = self.generate_method_name(&class_name, &main_method);

            if has_args {
                // 将 argc, argv 转换为 String[]
                self.output.push_str("  ; Convert argc/argv to String[]\n");
                self.output.push_str("  %args_array = call i8** @__cay_create_string_array(i32 %argc)\n");
                self.output.push_str("  br label %args_loop_init\n\n");

                // 循环初始化
                self.output.push_str("args_loop_init:\n");
                self.output.push_str("  %i = alloca i32\n");
                self.output.push_str("  store i32 0, i32* %i\n");
                self.output.push_str("  br label %args_loop_cond\n\n");

                // 循环条件
                self.output.push_str("args_loop_cond:\n");
                self.output.push_str("  %i_val = load i32, i32* %i\n");
                self.output.push_str("  %cond = icmp slt i32 %i_val, %argc\n");
                self.output.push_str("  br i1 %cond, label %args_loop_body, label %args_loop_end\n\n");

                // 循环体
                self.output.push_str("args_loop_body:\n");
                self.output.push_str("  %idx = load i32, i32* %i\n");
                self.output.push_str("  %arg_ptr = getelementptr i8*, i8** %argv, i32 %idx\n");
                self.output.push_str("  %arg_cstr = load i8*, i8** %arg_ptr\n");
                self.output.push_str("  %arg_str = call i8* @__cay_cstr_to_string(i8* %arg_cstr)\n");
                self.output.push_str("  call void @__cay_array_set_ref(i8** %args_array, i32 %idx, i8* %arg_str)\n");
                self.output.push_str("  %next_i = add i32 %idx, 1\n");
                self.output.push_str("  store i32 %next_i, i32* %i\n");
                self.output.push_str("  br label %args_loop_cond\n\n");

                // 循环结束
                self.output.push_str("args_loop_end:\n");

                if returns_int {
                    self.output.push_str(&format!("  %ret = call i32 @{}(i8** %args_array)\n", main_fn_name));
                    self.output.push_str("  ret i32 %ret\n");
                } else {
                    self.output.push_str(&format!("  call void @{}(i8** %args_array)\n", main_fn_name));
                    self.output.push_str("  ret i32 0\n");
                }
            } else if returns_int {
                self.output.push_str(&format!("  %ret = call i32 @{}()\n", main_fn_name));
                self.output.push_str("  ret i32 %ret\n");
            } else {
                self.output.push_str(&format!("  call void @{}()\n", main_fn_name));
                self.output.push_str("  ret i32 0\n");
            }
            self.output.push_str("}\n");
            self.output.push_str("\n");
        }

        // 测试模式：生成 __cavvy_test_main 入口
        if self.test_mode && !self.test_methods.is_empty() {
            self.emit_test_main()?;
        }

        for lambda_code in &self.lambda_functions {
            self.output.push_str(lambda_code);
        }

        let string_decls = self.get_string_declarations();
        let type_id_decls = self.emit_type_id_declarations();

        let mut output = self.output.clone();
        let insert_pos = output.find("; --- END OF HEADER ---")
            .map(|p| p + "; --- END OF HEADER ---\n".len())
            .unwrap_or_else(|| {
                // Fallback: insert after target triple line
                output.find("target triple").map(|p| {
                    output[p..].find('\n').map(|n| p + n + 1).unwrap_or(p)
                }).unwrap_or(0)
            });

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

        // 如果有 extern 声明，添加调用约定属性
        if !program.extern_declarations.is_empty() {
            self.output.push_str(&self.generate_calling_convention_attributes());
        }

        // DWARF 调试元数据节点（必须在所有 define 之后）
        self.emit_debug_metadata();

        Ok(self.output.clone())
    }

    fn collect_static_fields(&mut self, class: &ClassDecl, qname: &str) -> cayResult<()> {
        for member in &class.members {
            if let ClassMember::Field(field) = member {
                if field.modifiers.contains(&Modifier::Static) {
                    self.register_static_field(qname, field)?;
                }
            }
        }
        Ok(())
    }

    fn register_static_field(&mut self, class_name: &str, field: &FieldDecl) -> cayResult<()> {
        let llvm_class = self.get_qualified_class_name(class_name);
        let full_name = format!("@{}.{}_s", llvm_class, field.name);
        // 对于数组类型，静态字段存储的是数组指针（指向元素数据）
        // 例如 int[] 存储为 i32*，指向 int 数组的数据
        let base_llvm_type = self.type_to_llvm(&field.field_type);
        let is_array = matches!(field.field_type, crate::types::Type::Array(_));
        // 数组类型的静态字段本身就是指针类型（如 i32*），不需要额外指针层
        // 静态字段声明为指针类型，存储数组数据地址
        let llvm_type = if is_array {
            base_llvm_type
        } else {
            base_llvm_type
        };
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

        // 提取简单类名用于 static_field_map key（与 self.current_class 查找一致）
        let simple_class = if class_name.contains("::") {
            class_name.split("::").last().expect("split 应始终产生至少一个元素").to_string()
        } else {
            class_name.to_string()
        };
        let key = format!("{}.{}", simple_class, field.name);
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
            Expr::Literal(lit_expr) => match &lit_expr.value {
                crate::ast::LiteralValue::Int32(n) => Some(n.to_string()),
                crate::ast::LiteralValue::Int64(n) => Some(n.to_string()),
                crate::ast::LiteralValue::Float32(f) => {
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
                crate::ast::LiteralValue::Float64(f) => {
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
                crate::ast::LiteralValue::Bool(b) => Some(if *b { "1".to_string() } else { "0".to_string() }),
                _ => None,
            }
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
                                // 包含8字节头部（长度+填充）+ 数据
                                let total_size = 8 + size_val as i64 * elem_size;

                                let calloc_temp = self.new_temp();
                                self.output.push_str(&format!(
                                    "  {} = call i8* @calloc(i64 1, i64 {})\n",
                                    calloc_temp, total_size
                                ));

                                // 存储长度（前4字节）
                                let len_ptr = self.new_temp();
                                self.output.push_str(&format!(
                                    "  {} = bitcast i8* {} to i32*\n",
                                    len_ptr, calloc_temp
                                ));
                                self.output.push_str(&format!(
                                    "  store i32 {}, i32* {}, align 4\n",
                                    size_val, len_ptr
                                ));

                                // 计算数据起始地址（跳过8字节长度头）
                                let data_ptr = self.new_temp();
                                self.output.push_str(&format!(
                                    "  {} = getelementptr i8, i8* {}, i64 8\n",
                                    data_ptr, calloc_temp
                                ));

                                // 将 i8* 转换为元素类型指针
                                let cast_temp = self.new_temp();
                                self.output.push_str(&format!(
                                    "  {} = bitcast i8* {} to {}*\n",
                                    cast_temp, data_ptr, elem_llvm_type
                                ));

                                // 存储到静态字段
                                // cast_temp 是元素类型指针（如 i32*），field.name 是全局变量（如 @Test.data_s）
                                // 生成: store i32* %t3, i32* @Test.data_s, align 8
                                self.output.push_str(&format!(
                                    "  store {}* {}, {}* {}, align 8\n",
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
            Expr::Literal(lit_expr) => match &lit_expr.value {
                crate::ast::LiteralValue::Int32(n) => Some(*n as i64),
                crate::ast::LiteralValue::Int64(n) => Some(*n),
                _ => None,
            }
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

    fn generate_class(&mut self, class: &ClassDecl) -> cayResult<()> {
        // 设置当前命名空间上下文——仅影响 TypeRegistry 的类名查找，不影响其他
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace = class.namespace_path.clone();
        }

        // 构建完整类名（包含泛型参数，用于 LLVM 名称改编）
        let full_class_name = if class.type_params.is_empty() {
            class.name.clone()
        } else {
            format!("{}<{}>", class.name, class.type_params.join(", "))
        };

        // 构建限定名（用于 LLVM 名称改编）
        let qname = if class.namespace_path.is_empty() {
            full_class_name
        } else {
            format!("{}::{}", class.namespace_path.join("::"), full_class_name)
        };

        // 生成 vtable 全局常量
        self.generate_vtable_global(&qname)?;

        // 检查是否有显式构造函数
        let has_explicit_ctor = class.members.iter().any(|m| matches!(m, ClassMember::Constructor(_)));
        
        // 收集有初始化器的字段
        let mut fields_with_init = Vec::new();
        for member in &class.members {
            if let ClassMember::Field(field) = member {
                if field.initializer.is_some() && !field.modifiers.contains(&Modifier::Static) {
                    fields_with_init.push(field.clone());
                }
            }
        }
        if !fields_with_init.is_empty() {
            self.field_initializers.insert(qname.clone(), fields_with_init);
        }

        for member in &class.members {
            match member {
                ClassMember::Method(method) => {
                    // 跳过 native 和 abstract 方法（它们没有方法体）
                    if !method.modifiers.contains(&Modifier::Native) 
                        && !method.modifiers.contains(&Modifier::Abstract) {
                        self.generate_method(&qname, method)?;
                    }
                }
                ClassMember::Field(field) => {
                    if !field.modifiers.contains(&Modifier::Static) {
                    }
                }
                ClassMember::Constructor(ctor) => {
                    self.generate_constructor(&qname, ctor)?;
                }
                ClassMember::Destructor(dtor) => {
                    self.generate_destructor(&qname, dtor)?;
                }
                ClassMember::InstanceInitializer(_block) => {
                }
                ClassMember::StaticInitializer(block) => {
                    self.generate_static_initializer(&qname, block)?;
                }
            }
        }
        
        // 如果没有显式构造函数，生成默认构造函数
        if !has_explicit_ctor {
            self.generate_default_constructor(&qname)?;
        }

        // 清除命名空间上下文
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace.clear();
        }

        Ok(())
    }

    /// 生成 struct 的所有方法（struct 是值类型，无构造/析构/静态初始化）
    fn generate_struct_methods(&mut self, struct_decl: &StructDecl) -> cayResult<()> {
        // 设置当前命名空间上下文
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace = struct_decl.namespace_path.clone();
        }

        let qname = if struct_decl.namespace_path.is_empty() {
            struct_decl.name.clone()
        } else {
            format!("{}::{}", struct_decl.namespace_path.join("::"), struct_decl.name)
        };

        for method in &struct_decl.methods {
            if !method.modifiers.contains(&Modifier::Native)
                && !method.modifiers.contains(&Modifier::Abstract) {
                self.generate_method(&qname, method)?;
            }
        }

        // 清除命名空间上下文
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace.clear();
        }

        Ok(())
    }

    /// 生成 vtable 全局常量
    /// 
    /// 为每个类生成一个 vtable 数组，包含所有虚方法的函数指针。
    /// vtable 结构：[slot_0, slot_1, ..., slot_N]
    /// 每个 slot 是一个 i8* 类型的函数指针。
    fn generate_vtable_global(&mut self, class_name: &str) -> cayResult<()> {
        // 从 TypeRegistry 获取 vtable 布局
        let vtable_layout = if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                class_info.vtable_layout.clone()
            } else {
                None
            }
        } else {
            None
        };

        let layout = match vtable_layout {
            Some(l) => l,
            None => {
                // 没有 vtable 布局（如 final 类或无虚方法的类），生成空 vtable
                // 仍然需要生成以保持一致性
                return Ok(());
            }
        };

        if layout.size == 0 {
            return Ok(());
        }

        let llvm_class = self.get_qualified_class_name(class_name);
        let vtable_name = format!("{}.vtable", llvm_class);

        // 收集 vtable 条目
        let mut entries = Vec::new();
        
        // 按槽位编号排序（确保与 vtable 布局一致）
        let mut sorted_slots: Vec<_> = layout.slots.iter().collect();
        sorted_slots.sort_by_key(|&(_, &slot)| slot);
        
        for (method_name, _slot) in &sorted_slots {
            // 查找方法的 LLVM 函数名
            // 需要在继承链中查找方法定义
            let fn_name_opt = self.find_method_in_hierarchy(class_name, method_name);
            if let Some((fn_name, ret_type, params)) = fn_name_opt {
                let ret_llvm = self.type_to_llvm(&ret_type);
                let mut fn_param_types = vec!["i8*".to_string()];
                for param in &params {
                    fn_param_types.push(self.type_to_llvm(&param));
                }
                let fn_ptr_type = format!("{} ({})", ret_llvm, fn_param_types.join(", "));
                entries.push(format!("i8* bitcast ({}* @{} to i8*)", fn_ptr_type, fn_name));
            }
        }

        if entries.is_empty() {
            return Ok(());
        }

        // 生成 vtable 全局常量
        // 类型：[N x i8*]
        let vtable_type = format!("[{} x i8*]", entries.len());
        self.emit_line(&format!("@{} = global {} [{}]", 
            vtable_name, vtable_type, entries.join(", ")));

        Ok(())
    }

    /// 在继承链中查找方法定义
    /// 返回 (函数名, 返回类型, 参数类型列表)
    /// 跳过抽象方法（无实现），因为抽象方法没有对应的 LLVM 函数定义
    fn find_method_in_hierarchy(&self, class_name: &str, method_name: &str) -> Option<(String, crate::types::Type, Vec<crate::types::Type>)> {
        if let Some(ref registry) = self.type_registry {
            let mut current = class_name.to_string();
            loop {
                if let Some(class_info) = registry.get_class(&current) {
                    if let Some(methods) = class_info.methods.get(method_name) {
                        // 找到方法定义，跳过抽象方法（无实现）
                        if let Some(method) = methods.iter().find(|m| !m.is_static && !m.is_native && !m.is_abstract) {
                            let llvm_class = self.get_qualified_class_name(&current);
                            let fn_name = if method.params.is_empty() {
                                format!("{}.{}", llvm_class, method_name)
                            } else {
                                let param_types: Vec<String> = method.params.iter()
                                    .map(|p| self.type_to_signature(&p.param_type))
                                    .collect();
                                format!("{}.__{}_{}", llvm_class, method_name, param_types.join("_"))
                            };
                            let param_types: Vec<crate::types::Type> = method.params.iter()
                                .map(|p| p.param_type.clone())
                                .collect();
                            return Some((fn_name, method.return_type.clone(), param_types));
                        }
                    }
                    // 在父类中继续查找
                    if let Some(ref parent) = class_info.parent {
                        current = parent.clone();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
        }
        None
    }

    fn generate_method(&mut self, class_name: &str, method: &MethodDecl) -> cayResult<()> {
        // 跳过 native 方法的定义（它们在运行时或由外部提供）
        if method.modifiers.contains(&Modifier::Native) {
            return Ok(());
        }

        let fn_name = self.generate_method_name(class_name, method);
        self.current_function = fn_name.clone();
        // 从可能包含 :: 的限定名中提取简单名用于 current_class
        let raw_class_name = if class_name.contains("::") {
            class_name.split("::").last().expect("split 应始终产生至少一个元素").to_string()
        } else {
            class_name.to_string()
        };
        // 提取简单类名（不含泛型参数）用于参数名生成
        self.current_class = if let Some(pos) = raw_class_name.find('<') {
            raw_class_name[..pos].to_string()
        } else {
            raw_class_name
        };
        self.current_return_type = self.type_to_llvm(&method.return_type);

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();
        
        // 设置当前函数参数顺序（用于内联IR）
        self.current_param_order = method.params.iter().map(|p| p.name.clone()).collect();

        let ret_type = self.current_return_type.clone();
        let is_static = method.modifiers.contains(&Modifier::Static);
        
        let mut params: Vec<String> = Vec::new();
        
        // 实例方法添加 this 参数
        if !is_static {
            params.push("i8* %this".to_string());
        }
        
        for param in &method.params {
            let param_llvm_type = if param.is_varargs {
                // 可变参数使用 i8* 指针类型（数组的内存地址）
                "i8*".to_string()
            } else {
                self.type_to_llvm(&param.param_type)
            };
            params.push(format!("{} %{}.{}", param_llvm_type, self.current_class, param.name));
        }

        self.emit_line(&format!("define {} @{}({}) {{",
            ret_type, fn_name, params.join(", ")));
        self.indent += 1;

        self.emit_line("entry:");
        
        // 进入函数作用域，确保变量名有正确的作用域后缀
        self.scope_manager.enter_scope();
        
        // 实例方法声明 this 变量
        if !is_static {
            let this_llvm_name = self.scope_manager.declare_var("this", "i8*");
            self.emit_line(&format!("  %{} = alloca i8*", this_llvm_name));
            self.emit_line(&format!("  store i8* %this, i8** %{}", this_llvm_name));
            self.var_types.insert("this".to_string(), "i8*".to_string());
        }

        for param in &method.params {
            if param.is_varargs {
                // 可变参数特殊处理
                // 从 Array(ElementType) 提取元素类型
                let elem_type = match &param.param_type {
                    crate::types::Type::Array(elem) => self.type_to_llvm(elem),
                    _ => self.type_to_llvm(&param.param_type),
                };
                // 数组类型是元素类型加 *（如 i8* -> i8**）
                let array_type = format!("{}*", elem_type);
                
                // 声明变量时使用数组类型（这样 generate_identifier 和数组访问能正确工作）
                let llvm_name = self.scope_manager.declare_var_with_flag(&param.name, &array_type, true);
                self.emit_line(&format!("  %{} = alloca {}", llvm_name, array_type));
                
                // 将 i8* 参数转换为正确的数组类型指针
                let cast_temp = self.new_temp();
                self.emit_line(&format!("  {} = bitcast i8* %{}.{} to {}",
                    cast_temp, self.current_class, param.name, array_type));
                self.emit_line(&format!("  store {} {}, {}* %{}",
                    array_type, cast_temp, array_type, llvm_name));
                
                self.var_types.insert(param.name.clone(), array_type.clone());
                // 存储Cavvy类型信息，用于准确的类型推断
                self.var_cay_types.insert(param.name.clone(), param.param_type.clone());
                // 如果参数类型是对象或泛型，记录其类名以便后续方法调用解析
                match &param.param_type {
                    crate::types::Type::Object(class_name) => {
                        self.var_class_map.insert(param.name.clone(), class_name.clone());
                    }
                    crate::types::Type::Generic(class_name, _) => {
                        self.var_class_map.insert(param.name.clone(), class_name.clone());
                    }
                    _ => {}
                }
            } else {
                let param_type = self.type_to_llvm(&param.param_type);
                let llvm_name = self.scope_manager.declare_var_with_flag(&param.name, &param_type, true);
                self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
                self.emit_line(&format!("  store {} %{}.{}, {}* %{}",
                    param_type, self.current_class, param.name, param_type, llvm_name));
                self.var_types.insert(param.name.clone(), param_type.clone());
                // 存储Cavvy类型信息，用于准确的类型推断
                self.var_cay_types.insert(param.name.clone(), param.param_type.clone());
                // 如果参数类型是对象或泛型，记录其类名以便后续方法调用解析
                match &param.param_type {
                    crate::types::Type::Object(class_name) => {
                        self.var_class_map.insert(param.name.clone(), class_name.clone());
                    }
                    crate::types::Type::Generic(class_name, _) => {
                        self.var_class_map.insert(param.name.clone(), class_name.clone());
                    }
                    _ => {}
                }
            }
        }

        if let Some(body) = method.body.as_ref() {
            // 设置源位置为方法体的位置
            self.set_source_from_loc(&body.loc, &self.source_file.clone());
            self.generate_block(body)?;
        }

        if method.return_type == Type::Void {
            self.emit_line("  ret void");
        }
        
        // 退出函数作用域
        self.scope_manager.exit_scope();

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    fn generate_constructor(&mut self, class_name: &str, ctor: &crate::ast::ConstructorDecl) -> cayResult<()> {
        let fn_name = self.generate_constructor_name(class_name, ctor);
        self.current_function = fn_name.clone();
        // 从可能包含 :: 的限定名中提取简单名用于 current_class
        let raw_class_name = if class_name.contains("::") {
            class_name.split("::").last().expect("split 应始终产生至少一个元素").to_string()
        } else {
            class_name.to_string()
        };
        // 提取简单类名（不含泛型参数）用于参数名生成
        self.current_class = if let Some(pos) = raw_class_name.find('<') {
            raw_class_name[..pos].to_string()
        } else {
            raw_class_name
        };
        self.current_return_type = "void".to_string();

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        let params: Vec<String> = ctor.params.iter()
            .map(|p| format!("{} %{}.{}_param", self.type_to_llvm(&p.param_type), self.current_class, p.name))
            .collect();

        let mut all_params = vec![format!("i8* %this")];
        all_params.extend(params);

        self.emit_line(&format!("define void @{}({}) {{",
            fn_name, all_params.join(", ")));
        self.indent += 1;

        self.emit_line("entry:");
        
        // 进入函数作用域，确保变量名有正确的作用域后缀
        self.scope_manager.enter_scope();

        let this_llvm_name = self.scope_manager.declare_var("this", "i8*");
        self.emit_line(&format!("  %{} = alloca i8*", this_llvm_name));
        self.emit_line(&format!("  store i8* %this, i8** %{}", this_llvm_name));
        self.var_types.insert("this".to_string(), "i8*".to_string());

        for param in &ctor.params {
            let param_type = self.type_to_llvm(&param.param_type);
            let llvm_name = self.scope_manager.declare_var_with_flag(&param.name, &param_type, true);
            self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
            self.emit_line(&format!("  store {} %{}.{}_param, {}* %{}",
                param_type, self.current_class, param.name, param_type, llvm_name));
            self.var_types.insert(param.name.clone(), param_type.clone());
            self.var_cay_types.insert(param.name.clone(), param.param_type.clone());
        }

        if let Some(ref call) = ctor.constructor_call {
            match call {
                crate::ast::ConstructorCall::This(args) => {
                    // 从类型注册表获取真实的构造函数参数类型签名
                    let fallback_types: Vec<String> = args.iter()
                        .map(|arg| self.infer_expr_type_for_ctor(arg))
                        .collect();
                    let param_types = self.get_constructor_param_signatures(
                        class_name,
                        args.len(),
                        &fallback_types,
                    );
                    let target_ctor_name = self.generate_constructor_call_name_with_types(class_name, &param_types);
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
                                // 从类型注册表获取真实的父类构造函数参数类型签名
                                let fallback_types: Vec<String> = args.iter()
                                    .map(|arg| self.infer_expr_type_for_ctor(arg))
                                    .collect();
                                let param_types = self.get_constructor_param_signatures(
                                    parent_name,
                                    args.len(),
                                    &fallback_types,
                                );
                                let parent_ctor_name = self.generate_constructor_call_name_with_types(parent_name, &param_types);
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

        // 生成字段初始化器代码（super/this 调用之后，构造函数体之前）
        self.generate_field_initializers(class_name)?;

        self.generate_block(&ctor.body)?;

        self.emit_line("  ret void");
        
        // 退出函数作用域
        self.scope_manager.exit_scope();

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }
    
    /// 生成默认构造函数（无参构造函数）
    fn generate_default_constructor(&mut self, class_name: &str) -> cayResult<()> {
        let llvm_class = self.get_qualified_class_name(class_name);
        let fn_name = format!("{}.__ctor", llvm_class);
        self.current_function = fn_name.clone();
        // 从可能包含 :: 的限定名中提取简单名用于 current_class
        let raw_class_name = if class_name.contains("::") {
            class_name.split("::").last().expect("split 应始终产生至少一个元素").to_string()
        } else {
            class_name.to_string()
        };
        // 提取简单类名（不含泛型参数）用于参数名生成
        self.current_class = if let Some(pos) = raw_class_name.find('<') {
            raw_class_name[..pos].to_string()
        } else {
            raw_class_name
        };
        self.current_return_type = "void".to_string();

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        self.emit_line(&format!("define void @{}(i8* %this) {{", fn_name));
        self.indent += 1;
        self.emit_line("entry:");
        
        // 进入函数作用域
        self.scope_manager.enter_scope();

        let this_llvm_name = self.scope_manager.declare_var("this", "i8*");
        self.emit_line(&format!("  %{} = alloca i8*", this_llvm_name));
        self.emit_line(&format!("  store i8* %this, i8** %{}", this_llvm_name));
        self.var_types.insert("this".to_string(), "i8*".to_string());
        
        // 如果有父类，调用父类的默认构造函数
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(ref parent_name) = class_info.parent {
                    let llvm_parent = self.get_qualified_class_name(parent_name);
                    let parent_ctor_name = format!("{}.__ctor", llvm_parent);
                    self.emit_line(&format!("  call void @{}(i8* %this)", parent_ctor_name));
                }
            }
        }

        // 生成字段初始化器代码
        self.generate_field_initializers(class_name)?;

        self.emit_line("  ret void");
        
        // 退出函数作用域
        self.scope_manager.exit_scope();

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    /// 生成当前类字段的初始化器代码（super/this 调用之后、构造函数体之前）
    /// 只处理当前类声明的字段，不处理父类字段（父类由自己的构造函数初始化）
    fn generate_field_initializers(&mut self, class_name: &str) -> cayResult<()> {
        let fields = match self.field_initializers.get(class_name) {
            Some(f) => f.clone(),
            None => return Ok(()),
        };

        for field in &fields {
            if let Some(ref initializer) = field.initializer {
                // 生成初始化器表达式的值（格式如 "i32 42" 或 "%t1"）
                let init_val = self.generate_expression(initializer)?;
                // 从 "i32 42" 中提取值部分 "42"，或保留 "%t1" 不变
                let val_str = if init_val.contains(' ') {
                    init_val.split_whitespace().last().unwrap_or(&init_val).to_string()
                } else {
                    init_val
                };

                // 存储到 this.field
                let this_info = self.scope_manager.lookup_var("this")
                    .map(|v| v.llvm_name.clone())
                    .unwrap_or_else(|| "this".to_string());
                // this 是 alloca，需要先 load 出实际指针
                // this_info 来自 scope_manager（无 %），new_temp 返回带 % 的名称
                let this_loaded = self.new_temp();
                self.emit_line(&format!("  {} = load i8*, i8** %{}", this_loaded, this_info));
                let field_offset = self.get_field_offset(class_name, &field.name)?;
                let field_llvm_type = self.type_to_llvm(&field.field_type);

                // 计算字段地址：this_loaded + offset
                // this_loaded 来自 new_temp()，已经包含 % 前缀
                let ptr_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = getelementptr i8, i8* {}, i64 {}",
                    ptr_temp, this_loaded, field_offset
                ));
                // bitcast 到字段类型指针
                let cast_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i8* {} to {}*",
                    cast_temp, ptr_temp, field_llvm_type
                ));
                // 存储值
                self.emit_line(&format!(
                    "  store {} {}, {}* {}",
                    field_llvm_type, val_str, field_llvm_type, cast_temp
                ));
            }
        }

        Ok(())
    }

    /// 获取字段在对象中的偏移量（字节）
    fn get_field_offset(&self, class_name: &str, field_name: &str) -> cayResult<i64> {
        if let Some(layout) = self.class_layouts.get(class_name) {
            if let Some(field) = layout.fields.get(field_name) {
                return Ok(field.offset as i64);
            }
        }
        // 布局查找失败时返回 0（不应发生）
        eprintln!("[WARNING] Cannot find field '{}' in class '{}' layout, using offset 0", field_name, class_name);
        Ok(0)
    }

    fn generate_destructor(&mut self, class_name: &str, dtor: &crate::ast::DestructorDecl) -> cayResult<()> {
        let llvm_class = self.get_qualified_class_name(class_name);
        let fn_name = format!("{}.__dtor", llvm_class);
        self.current_function = fn_name.clone();
        // 从可能包含 :: 的限定名中提取简单名用于 current_class
        let raw_class_name = if class_name.contains("::") {
            class_name.split("::").last().expect("split 应始终产生至少一个元素").to_string()
        } else {
            class_name.to_string()
        };
        // 提取简单类名（不含泛型参数）用于参数名生成
        self.current_class = if let Some(pos) = raw_class_name.find('<') {
            raw_class_name[..pos].to_string()
        } else {
            raw_class_name
        };
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
        let llvm_class = self.get_qualified_class_name(class_name);
        let fn_name = format!("{}.__static_init", llvm_class);
        self.current_function = fn_name.clone();
        // 从可能包含 :: 的限定名中提取简单名用于 current_class
        self.current_class = if class_name.contains("::") {
            class_name.split("::").last().expect("split 应始终产生至少一个元素").to_string()
        } else {
            class_name.to_string()
        };
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
        let cls = self.get_qualified_class_name(class_name);
        
        // 只对泛型类尝试从类型注册表获取构造函数信息
        if class_name.contains('<') {
            if let Some(ref registry) = self.type_registry {
                let base_class_name = if let Some(pos) = class_name.find('<') {
                    &class_name[..pos]
                } else {
                    class_name
                };
                
                if let Some(class_info) = registry.get_class(base_class_name) {
                    if !class_info.type_params.is_empty() {
                        // 获取当前构造函数参数的签名（用于匹配）
                        let ctor_sigs: Vec<String> = ctor.params.iter()
                            .map(|p| self.type_to_signature(&p.param_type))
                            .collect();
                        
                        // 找到参数数量和类型都匹配的构造函数
                        for ctor_info in &class_info.constructors {
                            if ctor_info.params.len() != ctor.params.len() {
                                continue;
                            }
                            
                            // 获取注册表中构造函数的参数签名
                            let info_sigs: Vec<String> = ctor_info.params.iter()
                                .map(|p| self.type_to_signature(&p.param_type))
                                .collect();
                            
                            // 比较签名是否匹配
                            if ctor_sigs == info_sigs {
                                if ctor_info.params.is_empty() {
                                    return format!("{}.__ctor", cls);
                                } else {
                                    return format!("{}.__ctor_{}", cls, info_sigs.join("_"));
                                }
                            }
                        }
                        
                        // 如果没有精确匹配，回退到参数数量匹配（第一个）
                        for ctor_info in &class_info.constructors {
                            if ctor_info.params.len() == ctor.params.len() {
                                let info_sigs: Vec<String> = ctor_info.params.iter()
                                    .map(|p| self.type_to_signature(&p.param_type))
                                    .collect();
                                if ctor_info.params.is_empty() {
                                    return format!("{}.__ctor", cls);
                                } else {
                                    return format!("{}.__ctor_{}", cls, info_sigs.join("_"));
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // 回退：使用 AST 中的参数类型
        if ctor.params.is_empty() {
            format!("{}.__ctor", cls)
        } else {
            let param_types: Vec<String> = ctor.params.iter()
                .map(|p| self.type_to_signature(&p.param_type))
                .collect();
            format!("{}.__ctor_{}", cls, param_types.join("_"))
        }
    }

    /// 生成构造函数调用名称（基于参数类型列表）
    pub fn generate_constructor_call_name_with_types(&self, class_name: &str, param_types: &[String]) -> String {
        let cls = self.get_qualified_class_name(class_name);
        if param_types.is_empty() {
            format!("{}.__ctor", cls)
        } else {
            format!("{}.__ctor_{}", cls, param_types.join("_"))
        }
    }
    
    /// 生成构造函数调用名称（基于参数数量 - 仅用于简单情况）
    pub fn generate_constructor_call_name(&self, class_name: &str, arg_count: usize) -> String {
        let cls = self.get_qualified_class_name(class_name);
        if arg_count == 0 {
            format!("{}.__ctor", cls)
        } else {
            let param_types: Vec<String> = (0..arg_count).map(|_| "i".to_string()).collect();
            format!("{}.__ctor_{}", cls, param_types.join("_"))
        }
    }

    /// 推断表达式类型（用于构造函数调用）
    fn infer_expr_type_for_ctor(&self, expr: &crate::ast::Expr) -> String {
        use crate::ast::*;
        
        match expr {
            Expr::Literal(lit_expr) => {
                match &lit_expr.value {
                    LiteralValue::Int32(_) => "i".to_string(),
                    LiteralValue::Int64(_) => "l".to_string(),
                    LiteralValue::Float32(_) => "f".to_string(),
                    LiteralValue::Float64(_) => "d".to_string(),
                    LiteralValue::Bool(_) => "b".to_string(),
                    LiteralValue::Char(_) => "c".to_string(),
                    LiteralValue::String(_) => "s".to_string(),
                    LiteralValue::Null => "o".to_string(),
                }
            }
            Expr::Identifier(ident) => {
                // 查找变量类型
                if let Some(cay_type) = self.var_cay_types.get(&ident.name) {
                    self.type_to_signature(cay_type)
                } else {
                    "i".to_string() // 默认int
                }
            }
            Expr::MemberAccess(member) => {
                // 尝试推断成员访问的类型
                if let Some(cay_type) = self.infer_member_type(member) {
                    self.type_to_signature(&cay_type)
                } else {
                    "i".to_string() // 默认int
                }
            }
            Expr::Binary(binary) => {
                self.infer_expr_type_for_ctor(&binary.left)
            }
            Expr::Unary(unary) => {
                self.infer_expr_type_for_ctor(&unary.operand)
            }
            Expr::Cast(cast) => {
                self.type_to_signature(&cast.target_type)
            }
            Expr::Call(call) => {
                // 尝试推断函数调用的返回类型
                if let Some(cay_type) = self.infer_call_return_type(call) {
                    self.type_to_signature(&cay_type)
                } else {
                    "i".to_string() // 默认int
                }
            }
            Expr::New(new_expr) => {
                // new 表达式返回对象类型
                format!("o{}", new_expr.class_name)
            }
            _ => "i".to_string(), // 默认int
        }
    }

    /// 推断成员访问的类型
    fn infer_member_type(&self, member: &crate::ast::MemberAccessExpr) -> Option<crate::types::Type> {
        // 获取对象类型
        let obj_type = self.infer_expr_type_for_arg(&member.object)?;

        match obj_type {
            crate::types::Type::Object(class_name) => {
                // 获取命名空间限定名
                let qualified_name = {
                    let ns = self.get_class_namespace(&class_name);
                    if !ns.is_empty() {
                        format!("{}::{}", ns.join("::"), class_name)
                    } else {
                        class_name.clone()
                    }
                };
                // 查找类字段（先试简单名，再试命名空间限定名）
                let class_layout = self.class_layouts.get(&class_name)
                    .or_else(|| self.class_layouts.get(&qualified_name));
                if let Some(class_info) = class_layout {
                    if let Some(field) = class_info.fields.get(&member.member) {
                        return Some(field.field_type.clone());
                    }
                }
                None
            }
            crate::types::Type::Array(_) if member.member == "length" => {
                // 数组的 length 属性返回 int
                Some(crate::types::Type::Int32)
            }
            _ => None,
        }
    }

    /// 推断表达式类型（辅助函数，用于构造函数参数类型推断）
    fn infer_expr_type_for_arg(&self, expr: &crate::ast::Expr) -> Option<crate::types::Type> {
        use crate::ast::*;

        match expr {
            Expr::Identifier(ident) => {
                self.var_cay_types.get(&ident.name).cloned()
            }
            Expr::Literal(lit_expr) => {
                match &lit_expr.value {
                    LiteralValue::Int32(_) => Some(crate::types::Type::Int32),
                    LiteralValue::Int64(_) => Some(crate::types::Type::Int64),
                    LiteralValue::Float32(_) => Some(crate::types::Type::Float32),
                    LiteralValue::Float64(_) => Some(crate::types::Type::Float64),
                    LiteralValue::Bool(_) => Some(crate::types::Type::Bool),
                    LiteralValue::Char(_) => Some(crate::types::Type::Char),
                    LiteralValue::String(_) => Some(crate::types::Type::String),
                    LiteralValue::Null => None,
                }
            }
            _ => None,
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
            let llvm_name = self.scope_manager.declare_var_with_flag(&param.name, &param_type, true);
            self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
            self.emit_line(&format!("  store {} %{}.param, {}* %{}",
                param_type, param.name, param_type, llvm_name));
            self.var_types.insert(param.name.clone(), param_type);
            // 同时保存Cavvy类型用于函数指针识别
            self.var_cay_types.insert(param.name.clone(), param.param_type.clone());
        }

        self.generate_block(&func.body)?;

        // 确保函数有返回指令 - 对于非 void 函数，如果没有显式 return，添加默认返回
        if func.return_type == Type::Void {
            self.emit_line("  ret void");
        } else {
            // 检查最后一条指令是否已经是 ret
            let last_lines: Vec<&str> = self.code.lines().rev().take(10).collect();
            let has_ret = last_lines.iter().any(|line| {
                let trimmed = line.trim();
                trimmed.starts_with("ret ") || trimmed == "ret"
            });
            if !has_ret {
                // 添加默认返回值
                let default_value = match &func.return_type {
                    Type::Int32 => "i32 0",
                    Type::Int64 => "i64 0",
                    Type::Float32 => "float 0.0",
                    Type::Float64 => "double 0.0",
                    Type::Bool => "i1 false",
                    Type::Char => "i8 0",
                    Type::String => "i8* null",
                    Type::Pointer(_) => "i8* null",
                    _ => "i32 0",
                };
                self.emit_line(&format!("  ret {}", default_value));
            }
        }

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    /// 生成 extern 函数声明
    fn generate_extern_declaration(&mut self, extern_decl: &crate::ast::ExternDecl) -> cayResult<()> {
        for func in &extern_decl.functions {
            self.generate_extern_function(extern_decl.calling_convention, func)?;
        }
        Ok(())
    }

    /// 生成单个 extern 函数声明
    fn generate_extern_function(&mut self, calling_conv: crate::ast::CallingConvention, func: &crate::ast::ExternFunction) -> cayResult<()> {
        // 跳过运行时提供的函数（这些函数的定义已经在运行时模块中生成）
        let runtime_functions = [
            "__cay_memcpy_byte",
            "__cay_memset_byte",
            "__cay_write_int",
            "__cay_read_int",
            "__cay_string_concat",
            "__cay_int_to_string",
            "__cay_float_to_string",
            "__cay_double_to_string",
            "__cay_char_to_string",
        ];
        if runtime_functions.contains(&func.name.as_str()) {
            return Ok(());
        }
        
        let ret_type = self.type_to_llvm(&func.return_type);

        // 构建参数列表，支持可变参数
        let params: Vec<String> = func.params.iter()
            .map(|p| {
                if p.is_varargs {
                    "...".to_string()
                } else {
                    self.type_to_llvm(&p.param_type)
                }
            })
            .collect();

        // 获取调用约定属性
        let cc_attr = self.calling_convention_to_llvm_attr(calling_conv);

        // 生成声明
        let decl = if params.is_empty() {
            if cc_attr.is_empty() {
                format!("declare {} @{}()\n", ret_type, func.name)
            } else {
                format!("declare {} @{}() {}\n", ret_type, func.name, cc_attr)
            }
        } else {
            if cc_attr.is_empty() {
                format!("declare {} @{}({})\n", ret_type, func.name, params.join(", "))
            } else {
                format!("declare {} @{}({}) {}\n", ret_type, func.name, params.join(", "), cc_attr)
            }
        };

        // 检查是否已经声明过该函数，避免重复声明（使用HashSet进行O(1)查找）
        // 构建函数签名键："函数名@返回类型@参数1@参数2@..."
        let func_signature = if params.is_empty() {
            format!("{}@{}@void", func.name, ret_type)
        } else {
            format!("{}@{}@{}", func.name, ret_type, params.join("@"))
        };
        
        if !self.is_extern_emitted(&func_signature) {
            self.emit_raw(&decl);
            self.mark_extern_emitted(func_signature);
        }

        Ok(())
    }

    /// 将调用约定转换为 LLVM 属性
    fn calling_convention_to_llvm_attr(&self, cc: crate::ast::CallingConvention) -> String {
        match cc {
            // Windows x64 平台使用 win64 调用约定
            crate::ast::CallingConvention::Cdecl => {
                if self.is_windows_target() {
                    "#4".to_string()  // win64
                } else {
                    "#0".to_string()  // cdecl
                }
            }
            crate::ast::CallingConvention::Stdcall => "#1".to_string(),
            crate::ast::CallingConvention::Fastcall => "#2".to_string(),
            crate::ast::CallingConvention::Sysv64 => "#3".to_string(),
            crate::ast::CallingConvention::Win64 => "#4".to_string(),
        }
    }

    /// 生成测试入口函数 `__cavvy_test_main`
    ///
    /// 该函数会逐个调用所有 `@Test` 注解的方法并打印结果。
    fn emit_test_main(&mut self) -> cayResult<()> {
        let test_count = self.test_methods.len();
        
        self.output.push_str("; ============================================================\n");
        self.output.push_str("; Test entry point (generated by --test mode)\n");
        self.output.push_str("; ============================================================\n\n");
        
        // 声明 printf 函数
        self.output.push_str("declare i32 @printf(i8*, ...) #0\n\n");
        
        // 生成测试入口函数
        self.output.push_str("define i32 @__cavvy_test_main() {\n");
        self.output.push_str("entry:\n");
        self.output.push_str(&format!("  ; {} test method(s)\n", test_count));
        
        // 分配 passed 和 failed 计数器
        self.output.push_str("  %passed = alloca i32\n");
        self.output.push_str("  %failed = alloca i32\n");
        self.output.push_str("  store i32 0, i32* %passed\n");
        self.output.push_str("  store i32 0, i32* %failed\n\n");
        
        // 打印 header
        self.output.push_str("  ; Print header\n");
        let header_len = self.escape_test_str("running %d tests...\\n").len() + 1;
        self.output.push_str(&format!(
            "  call i32 (i8*, ...) @printf(i8* getelementptr inbounds ([{} x i8], [{} x i8]* @str_test_header, i64 0, i64 0), i32 {})\n",
            header_len, header_len, test_count
        ));
        
        // 为每个 @Test 方法生成调用
        for (i, (class_name, method_name)) in self.test_methods.iter().enumerate() {
            let full_name = format!("{}::{}", class_name, method_name);
            let method_fn_name = format!("{}.{}", class_name, method_name);
            
            let ok_msg = format!("test {} ... ok\\n\\00", full_name);
            let ok_len = ok_msg.len();
            let ok_global = format!("@str_test_ok_{}", i);
            
            self.output.push_str(&format!("  ; Test: {}\n", full_name));
            self.output.push_str(&format!("  call void @{}()\n", method_fn_name));
            
            // 打印 "ok"
            self.output.push_str(&format!(
                "  call i32 (i8*, ...) @printf(i8* getelementptr inbounds ([{} x i8], [{} x i8]* {}, i64 0, i64 0))\n",
                ok_len, ok_len, ok_global
            ));
            
            // 增加 passed 计数
            self.output.push_str("  %p = load i32, i32* %passed\n");
            self.output.push_str("  %np = add i32 %p, 1\n");
            self.output.push_str("  store i32 %np, i32* %passed\n\n");
        }
        
        // 打印汇总
        self.output.push_str("  ; Print summary\n");
        self.output.push_str("  %final_passed = load i32, i32* %passed\n");
        self.output.push_str("  %final_failed = load i32, i32* %failed\n");
        
        let summary_len = self.escape_test_str("\\ntest result: %d passed; %d failed\\n").len() + 1;
        self.output.push_str(&format!(
            "  call i32 (i8*, ...) @printf(i8* getelementptr inbounds ([{} x i8], [{} x i8]* @str_test_summary, i64 0, i64 0), i32 %final_passed, i32 %final_failed)\n",
            summary_len, summary_len
        ));
        
        // 返回 0（成功）或 1（有失败）
        self.output.push_str("  %has_failures = icmp sgt i32 %final_failed, 0\n");
        self.output.push_str("  %exit_code = select i1 %has_failures, i32 1, i32 0\n");
        self.output.push_str("  ret i32 %exit_code\n");
        self.output.push_str("}\n\n");
        
        // 生成字符串常量
        self.output.push_str("; Test string constants\n");
        
        let header_str = self.escape_test_str("running %d tests...\\n");
        self.output.push_str(&format!(
            "@str_test_header = private constant [{} x i8] c\"{}\"\n",
            header_str.len() + 1, header_str
        ));
        
        for (i, (class_name, method_name)) in self.test_methods.iter().enumerate() {
            let full_name = format!("{}::{}", class_name, method_name);
            let ok_msg = self.escape_test_str(&format!("test {} ... ok\\n", full_name));
            self.output.push_str(&format!(
                "@str_test_ok_{} = private constant [{} x i8] c\"{}\\00\"\n",
                i, ok_msg.len() + 1, ok_msg
            ));
        }
        
        let summary_str = self.escape_test_str("\\ntest result: %d passed; %d failed\\n");
        self.output.push_str(&format!(
            "@str_test_summary = private constant [{} x i8] c\"{}\\00\"\n",
            summary_str.len() + 1, summary_str
        ));
        
        self.output.push_str("\n");
        
        Ok(())
    }
    
    /// 转义字符串中的特殊字符用于 LLVM IR 字符串常量
    fn escape_test_str(&self, s: &str) -> String {
        s.replace("\\n", "\\0A")
            .replace("\\00", "\\00")
            .replace("\\t", "\\09")
            .replace("\"", "\\22")
    }
}
