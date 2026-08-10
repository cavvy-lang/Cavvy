use crate::ast::*;
use crate::codegen::context::{IRGenerator, ScopeManager};
use crate::codegen::specialization::MAX_SELF_NESTING_DEPTH;
use crate::miette_diagnostic::{CayResult, ErrorCodes, SourceLocation, codegen_error_at};
use crate::types::Type;

/// 计算类型 `ty` 中类 `base` 的最大自嵌套深度。
/// 例如 `ArrayList<int>` 深度为 1，`ArrayList<ArrayList<int>>` 深度为 2。
fn nesting_depth(ty: &Type, base: &str) -> usize {
    match ty {
        Type::Generic(name, args) => {
            let candidate = name.split("::").last().unwrap_or(name);
            let inner_max = args
                .iter()
                .map(|a| nesting_depth(a, base))
                .max()
                .unwrap_or(0);
            if candidate == base {
                1 + inner_max
            } else {
                inner_max
            }
        }
        Type::Array(inner) | Type::Pointer(inner) => nesting_depth(inner, base),
        Type::Function(ft) => std::cmp::max(
            nesting_depth(&ft.return_type, base),
            ft.params
                .iter()
                .map(|p| nesting_depth(p, base))
                .max()
                .unwrap_or(0),
        ),
        _ => 0,
    }
}

/// ROADMAP 5.3.x 智能指针种类，用于 `__dtor` 注入分发。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmartPtrKind {
    /// 独占/作用域指针：__dtor 直接释放 __owned 指向的对象。
    Owned,
    /// 引用计数指针：__dtor 原子递减引用计数，归零时释放对象与计数块。
    Rc,
    /// 弱引用指针：__dtor 递减弱引用计数，归零且强引用计数为 0 时释放计数块。
    WeakPtr,
    /// 可选值容器：__dtor 在 hasValue 为真时析构 value 字段。
    Optional,
}

/// 从可能包含 `::` 的限定名中提取简单名（`a::b::C` -> `C`）
fn simple_class_name(qualified: &str) -> &str {
    qualified.rsplit("::").next().unwrap_or(qualified)
}

/// 泛型特化：替换类型中的泛型参数为实际类型
fn substitute_type_params(
    ty: &Type,
    type_args: &[Type],
    type_params: &[crate::types::TypeParamInfo],
) -> Type {
    match ty {
        Type::GenericParam(name) => {
            if let Some(idx) = type_params.iter().position(|p| &p.name == name) {
                if let Some(type_arg) = type_args.get(idx) {
                    return type_arg.clone();
                }
            }
            ty.clone()
        }
        // 解析器将裸类型参数（如 `T`）表示为 Object("T")，而语义分析仅在
        // 类型注册表副本中将其替换为 GenericParam。代码生成使用的是 AST 原始副本，
        // 因此这里必须同样处理 Object 形式，否则特化方法/字段/构造函数会保留 "T"
        // 并被降级为 i8*，导致 Box<int>.get() 返回 i8* 而非 i32。
        Type::Object(name) => {
            if let Some(idx) = type_params.iter().position(|p| &p.name == name) {
                if let Some(type_arg) = type_args.get(idx) {
                    return type_arg.clone();
                }
            }
            ty.clone()
        }
        Type::Array(inner) => Type::Array(Box::new(substitute_type_params(
            inner,
            type_args,
            type_params,
        ))),
        Type::Pointer(inner) => Type::Pointer(Box::new(substitute_type_params(
            inner,
            type_args,
            type_params,
        ))),
        Type::Function(func_type) => {
            let new_return = substitute_type_params(&func_type.return_type, type_args, type_params);
            let new_params: Vec<Type> = func_type
                .params
                .iter()
                .map(|p| substitute_type_params(p, type_args, type_params))
                .collect();
            Type::Function(Box::new(crate::types::FunctionType {
                return_type: Box::new(new_return),
                params: new_params,
                is_static: func_type.is_static,
                is_closure: func_type.is_closure,
            }))
        }
        Type::Generic(base, args) => {
            let new_args = args
                .iter()
                .map(|a| substitute_type_params(a, type_args, type_params))
                .collect();
            Type::Generic(base.clone(), new_args)
        }
        _ => ty.clone(),
    }
}

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
        let mut init = String::new();

        match self.target_os.as_str() {
            "windows" => {
                if self.features.contains(&"console_utf8".to_string()) {
                    init.push_str(&format!("  call void @SetConsoleOutputCP(i32 {})\n", UTF8_CODEPAGE));
                }
            }
            "linux" | "macos" => {
                // Linux/macOS 使用 setlocale 设置 UTF-8
                if self.features.contains(&"console_utf8".to_string()) {
                    init.push_str("  call void @setlocale(i32 0, i8* getelementptr inbounds ([6 x i8], [6 x i8]* @.str.locale, i32 0, i32 0))\n");
                }
            }
            _ => {}
        }

        init
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
                declarations.push_str(
                    "@.str.locale = private unnamed_addr constant [6 x i8] c\"C.UTF-8\"\\00\n",
                );
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
    pub fn generate(&mut self, program: &Program, source_file: &str) -> CayResult<String> {
        // 扁平化 namespace 声明
        let program = program.flatten_namespaces();

        // 设置源文件路径
        self.source_file = source_file.to_string();

        // 先设置 extern 声明，这样 emit_header 中的运行时声明可以检查用户是否已声明
        self.set_extern_declarations(program.extern_declarations.clone());

        self.emit_header();

        // 设置顶层函数列表
        self.top_level_functions = program.top_level_functions.clone();

        // 设置类型别名
        for type_alias in &program.type_aliases {
            self.type_aliases
                .insert(type_alias.name.clone(), type_alias.target_type.clone());
        }

        // ===== 泛型特化：收集所有特化需求 =====
        let mut collector = crate::codegen::specialization::SpecializationCollector::new();
        collector.collect_from_program(&program);
        self.specializations = collector.instances.clone();

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
        let classes: std::collections::HashMap<String, &crate::ast::ClassDecl> = program
            .classes
            .iter()
            .map(|c| (c.name.clone(), c))
            .collect();

        fn compute_layout_recursive<'a>(
            class: &'a crate::ast::ClassDecl,
            classes: &std::collections::HashMap<String, &'a crate::ast::ClassDecl>,
            computed: &mut std::collections::HashSet<String>,
            generator: &mut IRGenerator,
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
            let instance_fields: Vec<_> = class
                .members
                .iter()
                .filter_map(|m| match m {
                    ClassMember::Field(f) => Some(f.clone()),
                    _ => None,
                })
                .collect();
            generator.compute_class_layout(&base_qname, &instance_fields, class.parent.as_deref());
            computed.insert(base_qname.clone());

            // ===== 泛型特化：为每个特化版本计算布局 =====
            if !class.type_params.is_empty() {
                let mut instances = generator
                    .specializations
                    .get(&base_qname)
                    .cloned()
                    .unwrap_or_default();
                // 同 generate_class：按裸类名合并，兼容命名空间内的泛型类。
                if base_qname != class.name {
                    if let Some(extra) = generator.specializations.get(&class.name) {
                        instances.extend(extra.iter().cloned());
                    }
                }
                let type_param_infos: Vec<crate::types::TypeParamInfo> = class
                    .type_params
                    .iter()
                    .map(|p| crate::types::TypeParamInfo {
                        name: p.name.clone(),
                        bound: p.bound.clone(),
                        default_type: p.default_type.clone(),
                    })
                    .collect();
                for instance in instances {
                    let specialized_name = instance.specialized_name();
                    // 设置类型参数映射
                    let mapping = instance.type_param_mapping(&type_param_infos);
                    let old_mapping = std::mem::replace(&mut generator.generic_type_args, mapping);
                    let resolved_type_args = instance.resolve_type_args(&type_param_infos);

                    // 计算特化版本的布局（字段类型已替换）
                    let specialized_fields: Vec<_> = instance_fields
                        .iter()
                        .map(|f| {
                            let mut field = f.clone();
                            field.field_type = substitute_type_params(
                                &field.field_type,
                                &resolved_type_args,
                                &type_param_infos,
                            );
                            field
                        })
                        .collect();
                    generator.compute_class_layout(
                        &specialized_name,
                        &specialized_fields,
                        class.parent.as_deref(),
                    );

                    generator.generic_type_args = old_mapping;
                }
            }
        }

        for class in &program.classes {
            compute_layout_recursive(class, &classes, &mut computed, self);
        }

        // 计算 struct 的布局（值类型，无继承，无对象头）
        // 对泛型 struct 还需为每个特化实例计算单态化布局。
        for struct_decl in &program.structs {
            let base_qname = if struct_decl.namespace_path.is_empty() {
                struct_decl.name.clone()
            } else {
                format!(
                    "{}::{}",
                    struct_decl.namespace_path.join("::"),
                    struct_decl.name
                )
            };
            let full_struct_name = if struct_decl.type_params.is_empty() {
                base_qname.clone()
            } else {
                let type_param_names: Vec<String> =
                    struct_decl.type_params.iter().map(|p| p.name.clone()).collect();
                if struct_decl.namespace_path.is_empty() {
                    format!("{}<{}>", struct_decl.name, type_param_names.join(", "))
                } else {
                    format!(
                        "{}::{}<{}>",
                        struct_decl.namespace_path.join("::"),
                        struct_decl.name,
                        type_param_names.join(", ")
                    )
                }
            };

            let instance_fields: Vec<_> = struct_decl.fields.iter().cloned().collect();
            self.compute_struct_layout(&full_struct_name, &instance_fields);

            // 泛型 struct：为每个特化版本计算布局
            if !struct_decl.type_params.is_empty() {
                let mut instances = self
                    .specializations
                    .get(&base_qname)
                    .cloned()
                    .unwrap_or_default();
                // 兼容特化收集器以裸 struct 名为键的情况
                if base_qname != struct_decl.name {
                    if let Some(extra) = self.specializations.get(&struct_decl.name) {
                        instances.extend(extra.iter().cloned());
                    }
                }
                let type_param_infos: Vec<crate::types::TypeParamInfo> = struct_decl
                    .type_params
                    .iter()
                    .map(|p| crate::types::TypeParamInfo {
                        name: p.name.clone(),
                        bound: p.bound.clone(),
                        default_type: p.default_type.clone(),
                    })
                    .collect();
                for instance in instances {
                    let specialized_name = instance.specialized_name();
                    let resolved_type_args = instance.resolve_type_args(&type_param_infos);
                    let specialized_fields: Vec<_> = instance_fields
                        .iter()
                        .map(|f| {
                            let mut field = f.clone();
                            field.field_type = substitute_type_params(
                                &field.field_type,
                                &resolved_type_args,
                                &type_param_infos,
                            );
                            field
                        })
                        .collect();
                    self.compute_struct_layout(&specialized_name, &specialized_fields);
                }
            }
        }

        for class in &program.classes {
            let qname = if class.namespace_path.is_empty() {
                class.name.clone()
            } else {
                format!("{}::{}", class.namespace_path.join("::"), class.name)
            };
            // 记录类的命名空间路径，用于方法名改编（使用限定名作为 key，避免同名类冲突）
            if !class.namespace_path.is_empty() {
                self.class_namespaces
                    .insert(qname.clone(), class.namespace_path.clone());
            }
            // 缓存类定义（用于显式特化查找原始类）
            self.classes_cache.insert(class.name.clone(), class.clone());
            self.collect_static_fields(class, &qname)?;

            for member in &class.members {
                if let crate::ast::ClassMember::Method(method) = member {
                    if method.name == "main"
                        && method.modifiers.contains(&crate::ast::Modifier::Public)
                        && method.modifiers.contains(&crate::ast::Modifier::Static)
                    {
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

        // 生成 struct 类型定义（值类型，必须在函数定义之前）
        let struct_type_defs = self.emit_struct_type_definitions();
        if !struct_type_defs.is_empty() {
            self.output.push_str("; Struct type definitions\n");
            self.output.push_str(&struct_type_defs);
        }

        // 生成 extern 函数声明
        for extern_decl in &program.extern_declarations {
            self.generate_extern_declaration(extern_decl)?;
        }

        // 生成顶层函数
        for func in &program.top_level_functions {
            self.generate_top_level_function(func)?;
        }

        // 先收集显式特化信息，用于在自动单态化时跳过
        for spec_class in &program.specialize_classes {
            let type_args_str: Vec<String> = spec_class
                .type_args
                .iter()
                .map(|t| format!("{}", t))
                .collect();
            let spec_key = type_args_str.join(", ");
            self.explicit_specializations
                .entry(spec_class.base_name.clone())
                .or_insert_with(std::collections::HashSet::new)
                .insert(spec_key);
        }

        // 为内置 Object 根类生成默认构造函数（无 AST 定义，但所有类默认继承它）
        self.generate_default_constructor("Object")?;

        for class in &program.classes {
            self.generate_class(class)?;
        }

        // 生成显式特化类
        for spec_class in &program.specialize_classes {
            self.generate_specialize_class(spec_class)?;
        }

        // 生成 struct 方法
        for struct_decl in &program.structs {
            self.generate_struct_methods(struct_decl)?;
        }

        // 生成 enum 方法
        for enum_decl in &program.enums {
            self.generate_enum_methods(enum_decl)?;
        }

        self.output.push_str(&self.code);

        // 生成跨平台 C entry point
        if use_top_level_main {
            // 使用顶层 main 函数
            let func =
                top_level_main.expect("use_top_level_main 为 true 时 top_level_main 应为 Some");
            let has_args = !func.params.is_empty();

            self.output.push_str("; Cross-platform C entry point\n");
            if has_args {
                // 带参数的 main 函数: main(String[] args) -> 接收 argc, argv
                self.output
                    .push_str("define i32 @main(i32 %argc, i8** %argv) {\n");
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
            self.generate_static_string_initialization()?;
            let main_fn_name = self.generate_top_level_function_name(&func.name);

            if has_args {
                // 将 argc, argv 转换为 String[]
                self.output.push_str("  ; Convert argc/argv to String[]\n");
                self.output
                    .push_str("  %args_array = call i8** @__cay_create_string_array(i32 %argc)\n");
                self.output.push_str("  br label %args_loop_init\n\n");

                // 循环初始化
                self.output.push_str("args_loop_init:\n");
                self.output.push_str("  %i = alloca i32\n");
                self.output.push_str("  store i32 0, i32* %i\n");
                self.output.push_str("  br label %args_loop_cond\n\n");

                // 循环条件
                self.output.push_str("args_loop_cond:\n");
                self.output.push_str("  %i_val = load i32, i32* %i\n");
                self.output
                    .push_str("  %cond = icmp slt i32 %i_val, %argc\n");
                self.output
                    .push_str("  br i1 %cond, label %args_loop_body, label %args_loop_end\n\n");

                // 循环体
                self.output.push_str("args_loop_body:\n");
                self.output.push_str("  %idx = load i32, i32* %i\n");
                self.output
                    .push_str("  %arg_ptr = getelementptr i8*, i8** %argv, i32 %idx\n");
                self.output
                    .push_str("  %arg_cstr = load i8*, i8** %arg_ptr\n");
                self.output
                    .push_str("  %arg_str = call i8* @__cay_cstr_to_string(i8* %arg_cstr)\n");
                self.output.push_str(
                    "  call void @__cay_array_set_ref(i8** %args_array, i32 %idx, i8* %arg_str)\n",
                );
                self.output.push_str("  %next_i = add i32 %idx, 1\n");
                self.output.push_str("  store i32 %next_i, i32* %i\n");
                self.output.push_str("  br label %args_loop_cond\n\n");

                // 循环结束
                self.output.push_str("args_loop_end:\n");

                if func.return_type == Type::Void {
                    self.output.push_str(&format!(
                        "  call void @{}(i8** %args_array)\n",
                        main_fn_name
                    ));
                    self.output.push_str("  ret i32 0\n");
                } else {
                    self.output.push_str(&format!(
                        "  %ret = call i32 @{}(i8** %args_array)\n",
                        main_fn_name
                    ));
                    self.output.push_str("  ret i32 %ret\n");
                }
            } else if func.return_type == Type::Void {
                self.output
                    .push_str(&format!("  call void @{}()\n", main_fn_name));
                self.output.push_str("  ret i32 0\n");
            } else {
                self.output
                    .push_str(&format!("  %ret = call i32 @{}()\n", main_fn_name));
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
                self.output
                    .push_str("define i32 @main(i32 %argc, i8** %argv) {\n");
            } else {
                // 无参数的 main 方法
                self.output.push_str("define i32 @main() {\n");
            }
            self.output.push_str("entry:\n");
            // 只在 Windows 目标平台上设置控制台代码页
            if self.is_windows_target() {
                self.output.push_str(&format!(
                    "  call void @SetConsoleOutputCP(i32 {})\n",
                    UTF8_CODEPAGE
                ));
            }
            self.generate_static_array_initialization();
            self.generate_static_string_initialization()?;
            let main_fn_name = self.generate_method_name(&class_name, &main_method);

            if has_args {
                // 将 argc, argv 转换为 String[]
                self.output.push_str("  ; Convert argc/argv to String[]\n");
                self.output
                    .push_str("  %args_array = call i8** @__cay_create_string_array(i32 %argc)\n");
                self.output.push_str("  br label %args_loop_init\n\n");

                // 循环初始化
                self.output.push_str("args_loop_init:\n");
                self.output.push_str("  %i = alloca i32\n");
                self.output.push_str("  store i32 0, i32* %i\n");
                self.output.push_str("  br label %args_loop_cond\n\n");

                // 循环条件
                self.output.push_str("args_loop_cond:\n");
                self.output.push_str("  %i_val = load i32, i32* %i\n");
                self.output
                    .push_str("  %cond = icmp slt i32 %i_val, %argc\n");
                self.output
                    .push_str("  br i1 %cond, label %args_loop_body, label %args_loop_end\n\n");

                // 循环体
                self.output.push_str("args_loop_body:\n");
                self.output.push_str("  %idx = load i32, i32* %i\n");
                self.output
                    .push_str("  %arg_ptr = getelementptr i8*, i8** %argv, i32 %idx\n");
                self.output
                    .push_str("  %arg_cstr = load i8*, i8** %arg_ptr\n");
                self.output
                    .push_str("  %arg_str = call i8* @__cay_cstr_to_string(i8* %arg_cstr)\n");
                self.output.push_str(
                    "  call void @__cay_array_set_ref(i8** %args_array, i32 %idx, i8* %arg_str)\n",
                );
                self.output.push_str("  %next_i = add i32 %idx, 1\n");
                self.output.push_str("  store i32 %next_i, i32* %i\n");
                self.output.push_str("  br label %args_loop_cond\n\n");

                // 循环结束
                self.output.push_str("args_loop_end:\n");

                if returns_int {
                    self.output.push_str(&format!(
                        "  %ret = call i32 @{}(i8** %args_array)\n",
                        main_fn_name
                    ));
                    self.output.push_str("  ret i32 %ret\n");
                } else {
                    self.output.push_str(&format!(
                        "  call void @{}(i8** %args_array)\n",
                        main_fn_name
                    ));
                    self.output.push_str("  ret i32 0\n");
                }
            } else if returns_int {
                self.output
                    .push_str(&format!("  %ret = call i32 @{}()\n", main_fn_name));
                self.output.push_str("  ret i32 %ret\n");
            } else {
                self.output
                    .push_str(&format!("  call void @{}()\n", main_fn_name));
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

        // 所有 define（含懒生成特化）已确定，此时补发跨 TU 析构的 declare。
        self.flush_pending_dtor_declares();

        let string_decls = self.get_string_declarations();
        let type_id_decls = self.emit_type_id_declarations();

        let mut output = self.output.clone();
        let insert_pos = output
            .find("; --- END OF HEADER ---")
            .map(|p| p + "; --- END OF HEADER ---\n".len())
            .unwrap_or_else(|| {
                // Fallback: insert after target triple line
                output
                    .find("target triple")
                    .map(|p| output[p..].find('\n').map(|n| p + n + 1).unwrap_or(p))
                    .unwrap_or(0)
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
            self.output
                .push_str(&self.generate_calling_convention_attributes());
        }

        // DWARF 调试元数据节点（必须在所有 define 之后）
        self.emit_debug_metadata();

        // 添加链接库元数据（用于 #link 指令）
        self.emit_link_libraries_metadata(&program.link_libraries);

        // 代码生成警告由调用方（Compiler）统一收集和打印，避免重复输出

        Ok(self.output.clone())
    }

    /// 生成链接库元数据注释
    /// 这些注释将被 cayc 提取并传递给链接器
    fn emit_link_libraries_metadata(&mut self, link_libraries: &[crate::ast::LinkLibraryDecl]) {
        if link_libraries.is_empty() {
            return;
        }

        self.output
            .push_str("\n; Link libraries metadata (generated by #link directives)\n");
        for lib in link_libraries {
            if lib.is_system {
                self.output.push_str(&format!("; !link <{}>\n", lib.name));
            } else {
                self.output.push_str(&format!("; !link \"{}\"\n", lib.name));
            }
        }
    }

    fn collect_static_fields(&mut self, class: &ClassDecl, qname: &str) -> CayResult<()> {
        for member in &class.members {
            if let ClassMember::Field(field) = member {
                if field.modifiers.contains(&Modifier::Static) {
                    self.register_static_field(qname, field)?;
                }
            }
        }
        Ok(())
    }

    fn register_static_field(&mut self, class_name: &str, field: &FieldDecl) -> CayResult<()> {
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
        let simple_class = simple_class_name(class_name).to_string();
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
                    // LLVM 要求 float 常量若以十进制书写必须在单精度下精确可表示，
                    // 否则（如 0.7f）会报 "floating point constant invalid for type"。
                    // 统一采用 LLVM 的十六进制浮点格式：将 float 扩展为 double 后取其
                    // 64 位比特模式，形如 0xXXXXXXXXXXXXXXXX，对所有值都合法。
                    Some(format!("0x{:016X}", (*f as f64).to_bits()))
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
                crate::ast::LiteralValue::Bool(b) => {
                    Some(if *b { "1".to_string() } else { "0".to_string() })
                }
                _ => None,
            },
            Expr::Binary(binary) => {
                let left = self.evaluate_const_int(&binary.left)?;
                let right = self.evaluate_const_int(&binary.right)?;
                let result = match binary.op {
                    crate::ast::BinaryOp::Add => left + right,
                    crate::ast::BinaryOp::Sub => left - right,
                    crate::ast::BinaryOp::Mul => left * right,
                    crate::ast::BinaryOp::Div => {
                        if right != 0 {
                            left / right
                        } else {
                            return None;
                        }
                    }
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
                            if let Some(size_val) =
                                self.evaluate_const_int(&array_creation.sizes[0])
                            {
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

    /// 生成静态 string 字段的运行时初始化代码。
    ///
    /// 字符串是 i8* 指针，无法像 int 那样作为 LLVM global 的常量初始化器
    /// （emit_static_field_declarations 中会退化为 zeroinitializer），
    /// 因此与静态数组一样，在 main 入口处求值初始化表达式并 store 到全局变量。
    /// 支持任意返回 String 的初始化表达式（字面量、拼接、静态方法调用等）。
    fn generate_static_string_initialization(&mut self) -> CayResult<()> {
        let fields: Vec<_> = self.static_fields.clone();
        for field in fields {
            if field.field_type != Type::String {
                continue;
            }
            let Some(init) = &field.initializer else {
                continue;
            };
            let init = init.clone();
            // generate_expression 通过 emit_line 写入 self.code，而此处
            // self.code 已刷入 self.output（此后不再使用）。记录写入起点，
            // 截取本次生成的指令转投到 self.output（@main 函数体）中。
            let code_start = self.code.len();
            let value = self.generate_expression(&init)?;
            let emitted = self.code[code_start..].to_string();
            self.code.truncate(code_start);
            self.output.push_str(&emitted);

            let (value_type, val) = self.parse_typed_value(&value);
            if value_type == "i8*" {
                self.output.push_str(&format!(
                    "  store i8* {}, i8** {}, align 8\n",
                    val, field.name
                ));
            } else {
                return Err(crate::miette_diagnostic::codegen_error_at(
                    ErrorCodes::CODEGEN_TYPE_CONVERSION_ERROR,
                    init.location().clone(),
                    format!(
                        "Static string field initializer must evaluate to string, got LLVM type {}",
                        value_type
                    ),
                ));
            }
        }
        Ok(())
    }

    fn evaluate_const_int(&self, expr: &Expr) -> Option<i64> {
        match expr {
            Expr::Literal(lit_expr) => match &lit_expr.value {
                crate::ast::LiteralValue::Int32(n) => Some(*n as i64),
                crate::ast::LiteralValue::Int64(n) => Some(*n),
                _ => None,
            },
            Expr::Binary(binary) => {
                let left = self.evaluate_const_int(&binary.left)?;
                let right = self.evaluate_const_int(&binary.right)?;
                match binary.op {
                    crate::ast::BinaryOp::Add => Some(left + right),
                    crate::ast::BinaryOp::Sub => Some(left - right),
                    crate::ast::BinaryOp::Mul => Some(left * right),
                    crate::ast::BinaryOp::Div => {
                        if right != 0 {
                            Some(left / right)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn generate_class(&mut self, class: &ClassDecl) -> CayResult<()> {
        // 设置当前命名空间上下文——仅影响 TypeRegistry 的类名查找，不影响其他
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace = class.namespace_path.clone();
        }

        // 构建完整类名（包含泛型参数，用于 LLVM 名称改编）
        let full_class_name = if class.type_params.is_empty() {
            class.name.clone()
        } else {
            let type_param_names: Vec<String> =
                class.type_params.iter().map(|p| p.name.clone()).collect();
            format!("{}<{}>", class.name, type_param_names.join(", "))
        };

        // 构建限定名（用于 LLVM 名称改编）
        let qname = if class.namespace_path.is_empty() {
            full_class_name.clone()
        } else {
            format!("{}::{}", class.namespace_path.join("::"), full_class_name)
        };

        // 构建基础限定名（不含泛型参数）
        let base_qname = if class.namespace_path.is_empty() {
            class.name.clone()
        } else {
            format!("{}::{}", class.namespace_path.join("::"), class.name)
        };

        // 生成 vtable 全局常量
        // 泛型基础模板（如 Box<T>）本身不会被实例化，只有具体特化版本（Box<int>）
        // 才需要 vtable。为基础模板生成 vtable 会在解析方法签名中的类型参数 T 时
        // 触发"未解析泛型参数"警告，且该 vtable 永远不会被引用，故跳过。
        if class.type_params.is_empty() {
            self.generate_vtable_global(&qname)?;
        }

        let is_interop = class.modifiers.contains(&Modifier::Interop);

        // 检查是否有显式构造函数
        let has_explicit_ctor = class
            .members
            .iter()
            .any(|m| matches!(m, ClassMember::Constructor(_)));

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
            self.field_initializers
                .insert(qname.clone(), fields_with_init);
        }

        // 编译期单态化：如果类有泛型参数，不生成原始的类型擦除版本
        // 只为具体类型参数的特化实例生成代码
        let is_generic = !class.type_params.is_empty();
        if !is_generic {
            // 非泛型类：生成原始类的方法
            for member in &class.members {
                match member {
                    ClassMember::Method(method) => {
                        // 带方法级类型参数的实例方法不做类型擦除发射，
                        // 在每个调用点按推断出的方法级类型实参懒单态化
                        // （见 call/method_generic.rs）。
                        if !method.type_params.is_empty()
                            && !method.modifiers.contains(&Modifier::Static)
                        {
                            continue;
                        }
                        // native/abstract 方法通过 generate_method 统一处理声明/跳过逻辑
                        self.generate_method(&qname, method)?;
                    }
                    ClassMember::Field(field) => if !field.modifiers.contains(&Modifier::Static) {},
                    ClassMember::Constructor(ctor) => {
                        self.generate_constructor(&qname, ctor)?;
                    }
                    ClassMember::Destructor(dtor) => {
                        self.generate_destructor(&qname, dtor)?;
                    }
                    ClassMember::InstanceInitializer(_block) => {}
                    ClassMember::StaticInitializer(block) => {
                        self.generate_static_initializer(&qname, block)?;
                    }
                }
            }

            // 如果没有显式构造函数，生成默认构造函数。
            // interop 类由外部 C++ 实现提供构造语义，不生成 Cavvy 默认构造函数，
            // 避免链接时与外部定义重复。
            if !has_explicit_ctor && !is_interop {
                self.generate_default_constructor(&qname)?;
            }
        }

        // ===== 泛型特化：为每个特化版本生成方法 =====
        if is_generic {
            let mut instances = self
                .specializations
                .get(&base_qname)
                .cloned()
                .unwrap_or_default();
            // 特化收集器以源码中书写的类名为键（裸名，如 "Optional"），而此处
            // base_qname 含命名空间（如 "std::Optional"）。二者不一致时按裸类名合并，
            // 确保命名空间内的泛型类（如 std::Optional）也能找到其特化实例。
            if base_qname != class.name {
                if let Some(extra) = self.specializations.get(&class.name) {
                    instances.extend(extra.iter().cloned());
                }
            }
            for instance in instances {
                self.generate_class_specialization_instance(
                    class,
                    &base_qname,
                    has_explicit_ctor,
                    &instance,
                )?;
            }
        }

        // 清除命名空间上下文
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace.clear();
        }

        Ok(())
    }

    /// 为泛型类的一个特化实例生成 vtable、方法、构造/析构函数。
    ///
    /// 既供 `generate_class` 的预收集特化循环使用，也供方法级泛型单态化
    /// 在方法体中遇到「仅在方法级类型实参替换后才具体」的泛型实例化
    /// （如 `map<U>` 体内的 `new Result<U, E>()`）时懒生成。
    fn generate_class_specialization_instance(
        &mut self,
        class: &ClassDecl,
        base_qname: &str,
        has_explicit_ctor: bool,
        instance: &crate::codegen::specialization::SpecializationInstance,
    ) -> CayResult<()> {
        let type_param_infos: Vec<crate::types::TypeParamInfo> = class
            .type_params
            .iter()
            .map(|p| crate::types::TypeParamInfo {
                name: p.name.clone(),
                bound: p.bound.clone(),
                default_type: p.default_type.clone(),
            })
            .collect();
        let specialized_name = instance.specialized_name();
        let llvm_specialized = instance.llvm_specialized_name();
        let resolved_type_args = instance.resolve_type_args(&type_param_infos);

        // 检查是否已生成过此特化版本
        let spec_key = format!("{}", llvm_specialized);
        if self.generated_specializations.contains(&spec_key) {
            return Ok(());
        }

        // 检查是否有显式特化覆盖此类型组合
        let type_args_str: Vec<String> = resolved_type_args
            .iter()
            .map(|t| format!("{}", t))
            .collect();
        let explicit_key = type_args_str.join(", ");
        if let Some(explicit_set) = self.explicit_specializations.get(&class.name) {
            if explicit_set.contains(&explicit_key) {
                // 跳过自动生成，由显式特化负责生成
                self.generated_specializations.insert(spec_key);
                return Ok(());
            }
        }
        self.generated_specializations.insert(spec_key);

        // 设置类型参数映射
        let mapping = instance.type_param_mapping(&type_param_infos);
        let old_mapping = std::mem::replace(&mut self.generic_type_args, mapping);

        // 生成特化版本的 vtable
        self.generate_vtable_global(&specialized_name)?;

        // 为特化版本生成方法。
        // 若方法返回类型（替换后）会引用超出 MAX_SELF_NESTING_DEPTH 的同类自嵌套
        // 特化，则跳过该方法体生成。这防止了如 ArrayList<ArrayList<int>>::filled3D
        // 的方法体引用未收集的 ArrayList<ArrayList<ArrayList<ArrayList<int>>>>>
        // 等深层特化，避免 IR 链接阶段出现未定义 vtable/构造函数。
        let base_class_simple = simple_class_name(base_qname).to_string();
        for member in &class.members {
            match member {
                ClassMember::Method(method) => {
                    if !method.modifiers.contains(&Modifier::Native)
                        && !method.modifiers.contains(&Modifier::Abstract)
                    {
                        // 带方法级类型参数的实例方法不做类型擦除发射：
                        // 其方法级类型参数（如 U）在类级特化上下文中无法解析，
                        // 擦除副本会导致闭包返回类型/字段布局不匹配。
                        // 它们在每个调用点按推断出的方法级类型实参懒单态化
                        // （见 call/method_generic.rs）。
                        if !method.type_params.is_empty()
                            && !method.modifiers.contains(&Modifier::Static)
                        {
                            continue;
                        }
                        // 创建特化版本的方法（替换参数和返回类型中的泛型参数）
                        let mut specialized_method = method.clone();
                        specialized_method.return_type = substitute_type_params(
                            &method.return_type,
                            &resolved_type_args,
                            &type_param_infos,
                        );
                        specialized_method.params = method
                            .params
                            .iter()
                            .map(|p| crate::types::ParameterInfo {
                                name: p.name.clone(),
                                param_type: substitute_type_params(
                                    &p.param_type,
                                    &resolved_type_args,
                                    &type_param_infos,
                                ),
                                is_varargs: p.is_varargs,
                            })
                            .collect();

                        // 抑制会引用未收集深层特化的方法体生成。
                        if nesting_depth(&specialized_method.return_type, &base_class_simple)
                            > MAX_SELF_NESTING_DEPTH
                        {
                            continue;
                        }

                        self.generate_method(&specialized_name, &specialized_method)?;
                    }
                }
                ClassMember::Constructor(ctor) => {
                    let mut specialized_ctor = ctor.clone();
                    specialized_ctor.params = ctor
                        .params
                        .iter()
                        .map(|p| crate::types::ParameterInfo {
                            name: p.name.clone(),
                            param_type: substitute_type_params(
                                &p.param_type,
                                &resolved_type_args,
                                &type_param_infos,
                            ),
                            is_varargs: p.is_varargs,
                        })
                        .collect();
                    self.generate_constructor(&specialized_name, &specialized_ctor)?;
                }
                ClassMember::Destructor(dtor) => {
                    self.generate_destructor(&specialized_name, dtor)?;
                }
                _ => {}
            }
        }

        // 如果没有显式构造函数，生成默认构造函数
        if !has_explicit_ctor {
            self.generate_default_constructor(&specialized_name)?;
        }

        // 恢复类型参数映射
        self.generic_type_args = old_mapping;

        Ok(())
    }

    /// 在隔离的代码缓冲区中执行懒单态化生成。
    ///
    /// 方法级泛型（`method<U>(...)`）的特化副本与其体内引用的泛型类特化
    /// 都是在「某个函数体生成到一半」时被触发懒生成的。直接生成会把新的
    /// `define` 交错进未完成的函数体。此处仿照 lambda 的做法：换出当前
    /// 代码缓冲区与全部函数级状态，生成完毕恢复调用方状态，生成的函数定义
    /// 推入 lambda_functions，在最终输出阶段统一追加到 IR 末尾。
    pub(crate) fn with_deferred_codegen(&mut self, f: impl FnOnce(&mut Self)) {
        let saved_code = std::mem::take(&mut self.code);
        let saved_scope_manager =
            std::mem::replace(&mut self.scope_manager, ScopeManager::new());
        let saved_var_types = std::mem::take(&mut self.var_types);
        let saved_var_cay_types = std::mem::take(&mut self.var_cay_types);
        let saved_var_class_map = std::mem::take(&mut self.var_class_map);
        let saved_loop_stack = std::mem::take(&mut self.loop_stack);
        let saved_param_order = std::mem::take(&mut self.current_param_order);
        let saved_generic_args = std::mem::take(&mut self.generic_type_args);
        let saved_pending_expected = self.pending_new_expected_type.take();
        let saved_temp_counter = self.temp_counter;
        let saved_label_counter = self.label_counter;
        let saved_indent = self.indent;
        let saved_current_function = std::mem::take(&mut self.current_function);
        let saved_current_class = std::mem::take(&mut self.current_class);
        let saved_current_class_specialized = self.current_class_specialized.take();
        let saved_current_return_type = std::mem::take(&mut self.current_return_type);
        let saved_current_cay_return_type = self.current_function_cay_return_type.take();
        let saved_namespace = self
            .type_registry
            .as_ref()
            .map(|r| r.current_namespace.clone())
            .unwrap_or_default();

        f(self);

        let mut generated = std::mem::take(&mut self.code);
        self.code = saved_code;
        self.scope_manager = saved_scope_manager;
        self.var_types = saved_var_types;
        self.var_cay_types = saved_var_cay_types;
        self.var_class_map = saved_var_class_map;
        self.loop_stack = saved_loop_stack;
        self.current_param_order = saved_param_order;
        self.generic_type_args = saved_generic_args;
        self.pending_new_expected_type = saved_pending_expected;
        self.temp_counter = saved_temp_counter;
        self.label_counter = saved_label_counter;
        self.indent = saved_indent;
        self.current_function = saved_current_function;
        self.current_class = saved_current_class;
        self.current_class_specialized = saved_current_class_specialized;
        self.current_return_type = saved_current_return_type;
        self.current_function_cay_return_type = saved_current_cay_return_type;
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace = saved_namespace;
        }

        // ensure_free_declared 通过扫描 self.code 去重，而懒生成发生在换出的
        // 隔离缓冲区中，会把 declare 写进延迟片段，与主缓冲区中的声明重复
        // （invalid redefinition of function 'free'）。此处把声明从延迟片段
        // 剥除，恢复主缓冲区后统一走 ensure_free_declared 去重。
        const FREE_DECL: &str = "declare void @free(i8*)\n";
        let deferred_needs_free = generated.contains(FREE_DECL);
        if deferred_needs_free {
            generated = generated.replace(FREE_DECL, "");
        }

        if !generated.trim().is_empty() {
            self.lambda_functions.push(generated);
        }
        if deferred_needs_free {
            self.ensure_free_declared();
        }
    }

    /// 懒生成泛型类的特化实例（vtable、方法、构造/析构函数），已生成则直接返回。
    ///
    /// 方法级泛型的特化方法体中可能出现「仅在方法级类型实参替换后才具体」的
    /// 泛型实例化（如 `map<U>` 体内的 `new Result<U, E>()`），AST 特化收集器
    /// 无法预先收集这些实例，故在代码生成到该 new 表达式时懒生成对应特化。
    /// 实参未完全具体（仍含 GenericParam）或类 AST 不可用时静默跳过——
    /// 此时与原行为一致（引用未生成符号）。
    pub(crate) fn ensure_generic_class_specialization_generated(
        &mut self,
        base_class_name: &str,
        type_args: &[crate::types::Type],
    ) {
        if type_args.is_empty() {
            return;
        }
        // 全部实参解析为具体类型，否则无法单态化
        let resolved: Vec<crate::types::Type> = type_args
            .iter()
            .map(|t| self.resolve_type_arg_concrete(t))
            .collect();
        if !resolved.iter().all(|t| self.type_arg_is_concrete(t)) {
            return;
        }

        // 定位类 AST（classes_cache 以裸类名为键）
        let bare = base_class_name.rsplit("::").next().unwrap_or(base_class_name);
        let class_decl = self
            .classes_cache
            .get(base_class_name)
            .or_else(|| self.classes_cache.get(bare))
            .cloned();
        let Some(class_decl) = class_decl else {
            return;
        };
        if class_decl.type_params.is_empty() {
            return;
        }

        // 特化收集器对同一实例存在两种记账形式：使用点直接收集的实例用裸名
        //（namespace_path 为空，如 "HashMap<String, int>"），依赖收集的实例带
        // 命名空间（如 "std::ArrayList<ArrayList<int>>"）。两种形式的
        // specialized_name/llvm 名不同，布局键与去重键都必须两种形式都查。
        let bare_instance = crate::codegen::specialization::SpecializationInstance {
            base_class_name: class_decl.name.clone(),
            namespace_path: Vec::new(),
            type_args: resolved.clone(),
        };
        let ns_instance = crate::codegen::specialization::SpecializationInstance {
            base_class_name: class_decl.name.clone(),
            namespace_path: class_decl.namespace_path.clone(),
            type_args: resolved,
        };
        let bare_llvm = bare_instance.llvm_specialized_name();
        let ns_llvm = ns_instance.llvm_specialized_name();

        // 任一形式已生成，直接返回
        if self.generated_specializations.contains(&bare_llvm)
            || self.generated_specializations.contains(&ns_llvm)
        {
            return;
        }
        // 任一形式已被收集器登记：预收集路径（generate_class）会负责生成，
        // 此处不能重复生成——否则 vtable 符号与 generated_methods 记账互相干扰，
        // 且懒生成上下文缺少对应布局时会产出截断的函数体。
        let already_registered = self
            .specializations
            .values()
            .flatten()
            .any(|inst| inst.llvm_specialized_name() == bare_llvm || inst.llvm_specialized_name() == ns_llvm);
        if already_registered {
            return;
        }

        // 真正未被收集的实例（方法级泛型体内的实例化）：选择命名形式。
        // 优先使用已有布局的形式；都没有布局时用限定名形式并现场计算布局，
        // 否则特化构造函数中的 this.field 赋值会因布局缺失而失败。
        let instance = if self
            .class_layouts
            .contains_key(&bare_instance.specialized_name())
        {
            bare_instance
        } else {
            ns_instance
        };
        let specialized_name = instance.specialized_name();
        if !self.class_layouts.contains_key(&specialized_name) {
            let type_param_infos: Vec<crate::types::TypeParamInfo> = class_decl
                .type_params
                .iter()
                .map(|p| crate::types::TypeParamInfo {
                    name: p.name.clone(),
                    bound: p.bound.clone(),
                    default_type: p.default_type.clone(),
                })
                .collect();
            let instance_fields: Vec<_> = class_decl
                .members
                .iter()
                .filter_map(|m| match m {
                    ClassMember::Field(f) => Some(f.clone()),
                    _ => None,
                })
                .collect();
            let mapping = instance.type_param_mapping(&type_param_infos);
            let old_mapping = std::mem::replace(&mut self.generic_type_args, mapping);
            let resolved_type_args = instance.resolve_type_args(&type_param_infos);
            let specialized_fields: Vec<_> = instance_fields
                .iter()
                .map(|f| {
                    let mut field = f.clone();
                    field.field_type = substitute_type_params(
                        &field.field_type,
                        &resolved_type_args,
                        &type_param_infos,
                    );
                    field
                })
                .collect();
            self.compute_class_layout(
                &specialized_name,
                &specialized_fields,
                class_decl.parent.as_deref(),
            );
            self.generic_type_args = old_mapping;
        }

        // 登记到特化实例表，保持与预收集路径一致的记账
        let base_qname = if class_decl.namespace_path.is_empty() {
            class_decl.name.clone()
        } else {
            format!(
                "{}::{}",
                class_decl.namespace_path.join("::"),
                class_decl.name
            )
        };
        self.specializations
            .entry(base_qname.clone())
            .or_default()
            .insert(instance.clone());

        let has_explicit_ctor = class_decl
            .members
            .iter()
            .any(|m| matches!(m, ClassMember::Constructor(_)));
        let ns = class_decl.namespace_path.clone();
        self.with_deferred_codegen(move |s| {
            if let Some(ref mut registry) = s.type_registry {
                registry.current_namespace = ns;
            }
            let _ = s.generate_class_specialization_instance(
                &class_decl,
                &base_qname,
                has_explicit_ctor,
                &instance,
            );
        });
    }

    /// 生成显式特化类
    ///
    /// 为显式特化声明生成 LLVM IR：
    /// 1. 查找原始泛型类
    /// 2. 将显式特化的成员覆盖原始成员
    /// 3. 生成特化版本的 vtable、方法和构造函数
    fn generate_specialize_class(&mut self, spec: &SpecializeClassDecl) -> CayResult<()> {
        // 设置命名空间上下文
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace = spec.namespace_path.clone();
        }

        // 查找原始泛型类
        let base_class = match self.classes_cache.get(&spec.base_name) {
            Some(c) => c.clone(),
            None => {
                return Err(crate::miette_diagnostic::codegen_error_at(
                    ErrorCodes::CODEGEN_INVALID_OPERATION,
                    spec.loc.clone(),
                    format!("找不到显式特化的基础类 '{}'", spec.base_name),
                ));
            }
        };

        // 将 AST 类型参数声明转换为内部类型参数信息
        let type_param_infos: Vec<crate::types::TypeParamInfo> = base_class
            .type_params
            .iter()
            .map(|p| crate::types::TypeParamInfo {
                name: p.name.clone(),
                bound: p.bound.clone(),
                default_type: p.default_type.clone(),
            })
            .collect();

        // 解析最终类型参数（填充默认值）
        let resolved_type_args: Vec<Type> = type_param_infos
            .iter()
            .enumerate()
            .map(|(idx, param)| {
                if let Some(type_arg) = spec.type_args.get(idx) {
                    type_arg.clone()
                } else if let Some(default) = &param.default_type {
                    default.clone()
                } else {
                    Type::GenericParam(param.name.clone())
                }
            })
            .collect();

        // 构建特化类名，如 Box<int>
        let type_args_str: Vec<String> = resolved_type_args
            .iter()
            .map(|t| t.display_name())
            .collect();
        let specialized_name = format!("{}<{ }>", spec.base_name, type_args_str.join(", "));

        // 构建类型参数映射
        let mut mapping = std::collections::HashMap::new();
        for (idx, param) in type_param_infos.iter().enumerate() {
            if let Some(type_arg) = resolved_type_args.get(idx) {
                mapping.insert(param.name.clone(), type_arg.clone());
            }
        }
        let old_mapping = std::mem::replace(&mut self.generic_type_args, mapping);

        // 标记此特化版本已生成
        let spec_key = format!("{}", specialized_name);
        self.generated_specializations.insert(spec_key);

        // 生成特化版本的 vtable
        self.generate_vtable_global(&specialized_name)?;

        // 合并成员：显式特化成员覆盖原始成员
        let mut merged_members: std::collections::HashMap<String, ClassMember> =
            std::collections::HashMap::new();

        // 先添加原始成员
        for member in &base_class.members {
            let key = match member {
                ClassMember::Method(m) => format!("method:{}", m.name),
                ClassMember::Constructor(c) => format!("constructor:{}", c.params.len()),
                ClassMember::Field(f) => format!("field:{}", f.name),
                ClassMember::Destructor(_) => "destructor".to_string(),
                _ => continue,
            };
            merged_members.insert(key, member.clone());
        }

        // 显式特化成员覆盖
        for member in &spec.members {
            let key = match member {
                ClassMember::Method(m) => format!("method:{}", m.name),
                ClassMember::Constructor(c) => format!("constructor:{}", c.params.len()),
                ClassMember::Field(f) => format!("field:{}", f.name),
                ClassMember::Destructor(_) => "destructor".to_string(),
                _ => continue,
            };
            merged_members.insert(key, member.clone());
        }

        // 检查是否有显式构造函数
        let has_explicit_ctor = merged_members
            .values()
            .any(|m| matches!(m, ClassMember::Constructor(_)));

        // 生成合并后的成员
        for member in merged_members.values() {
            match member {
                ClassMember::Method(method) => {
                    if !method.modifiers.contains(&Modifier::Native)
                        && !method.modifiers.contains(&Modifier::Abstract)
                    {
                        let mut specialized_method = method.clone();
                        specialized_method.return_type = substitute_type_params(
                            &method.return_type,
                            &resolved_type_args,
                            &type_param_infos,
                        );
                        specialized_method.params = method
                            .params
                            .iter()
                            .map(|p| crate::types::ParameterInfo {
                                name: p.name.clone(),
                                param_type: substitute_type_params(
                                    &p.param_type,
                                    &resolved_type_args,
                                    &type_param_infos,
                                ),
                                is_varargs: p.is_varargs,
                            })
                            .collect();
                        self.generate_method(&specialized_name, &specialized_method)?;
                    }
                }
                ClassMember::Constructor(ctor) => {
                    let mut specialized_ctor = ctor.clone();
                    specialized_ctor.params = ctor
                        .params
                        .iter()
                        .map(|p| crate::types::ParameterInfo {
                            name: p.name.clone(),
                            param_type: substitute_type_params(
                                &p.param_type,
                                &resolved_type_args,
                                &type_param_infos,
                            ),
                            is_varargs: p.is_varargs,
                        })
                        .collect();
                    self.generate_constructor(&specialized_name, &specialized_ctor)?;
                }
                _ => {}
            }
        }

        // 如果没有显式构造函数，生成默认构造函数
        if !has_explicit_ctor {
            self.generate_default_constructor(&specialized_name)?;
        }

        // 恢复类型参数映射
        self.generic_type_args = old_mapping;

        // 清除命名空间上下文
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace.clear();
        }

        Ok(())
    }

    /// 生成 struct 的所有方法（struct 是值类型，无构造/析构/静态初始化）
    ///
    /// 对泛型 struct 进行完整单态化：不为类型参数未解析的基础模板生成代码，
    /// 只为 SpecializationCollector 收集到的每个具体特化实例生成构造函数和方法。
    fn generate_struct_methods(&mut self, struct_decl: &StructDecl) -> CayResult<()> {
        // 设置当前命名空间上下文
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace = struct_decl.namespace_path.clone();
        }

        let base_qname = if struct_decl.namespace_path.is_empty() {
            struct_decl.name.clone()
        } else {
            format!(
                "{}::{}",
                struct_decl.namespace_path.join("::"),
                struct_decl.name
            )
        };
        let full_struct_name = if struct_decl.type_params.is_empty() {
            base_qname.clone()
        } else {
            let type_param_names: Vec<String> =
                struct_decl.type_params.iter().map(|p| p.name.clone()).collect();
            if struct_decl.namespace_path.is_empty() {
                format!("{}<{}>", struct_decl.name, type_param_names.join(", "))
            } else {
                format!(
                    "{}::{}<{}>",
                    struct_decl.namespace_path.join("::"),
                    struct_decl.name,
                    type_param_names.join(", ")
                )
            }
        };

        let is_generic = !struct_decl.type_params.is_empty();
        let has_explicit_ctor = struct_decl
            .constructors
            .iter()
            .any(|c| !c.modifiers.contains(&Modifier::Native) && !c.modifiers.contains(&Modifier::Abstract));

        let type_param_infos: Vec<crate::types::TypeParamInfo> = struct_decl
            .type_params
            .iter()
            .map(|p| crate::types::TypeParamInfo {
                name: p.name.clone(),
                bound: p.bound.clone(),
                default_type: p.default_type.clone(),
            })
            .collect();

        if !is_generic {
            // 非泛型 struct：直接生成原始定义的方法
            for ctor in &struct_decl.constructors {
                if !ctor.modifiers.contains(&Modifier::Native)
                    && !ctor.modifiers.contains(&Modifier::Abstract)
                {
                    self.generate_struct_constructor(&full_struct_name, ctor)?;
                }
            }
            if !has_explicit_ctor {
                self.generate_struct_default_constructor(&full_struct_name)?;
            }

            for method in &struct_decl.methods {
                if !method.modifiers.contains(&Modifier::Native)
                    && !method.modifiers.contains(&Modifier::Abstract)
                {
                    self.generate_method(&full_struct_name, method)?;
                }
            }
        } else {
            // 泛型 struct：为每个收集到的特化实例生成单态化代码
            let mut instances = self
                .specializations
                .get(&base_qname)
                .cloned()
                .unwrap_or_default();
            if base_qname != struct_decl.name {
                if let Some(extra) = self.specializations.get(&struct_decl.name) {
                    instances.extend(extra.iter().cloned());
                }
            }

            for instance in instances {
                let specialized_name = instance.specialized_name();
                let resolved_type_args = instance.resolve_type_args(&type_param_infos);

                // 设置类型参数映射，供方法体中的泛型解析使用
                let mapping = instance.type_param_mapping(&type_param_infos);
                let old_mapping = std::mem::replace(&mut self.generic_type_args, mapping);

                for ctor in &struct_decl.constructors {
                    if !ctor.modifiers.contains(&Modifier::Native)
                        && !ctor.modifiers.contains(&Modifier::Abstract)
                    {
                        let mut specialized_ctor = ctor.clone();
                        specialized_ctor.params = ctor
                            .params
                            .iter()
                            .map(|p| crate::types::ParameterInfo {
                                name: p.name.clone(),
                                param_type: substitute_type_params(
                                    &p.param_type,
                                    &resolved_type_args,
                                    &type_param_infos,
                                ),
                                is_varargs: p.is_varargs,
                            })
                            .collect();
                        self.generate_struct_constructor(&specialized_name, &specialized_ctor)?;
                    }
                }
                if !has_explicit_ctor {
                    self.generate_struct_default_constructor(&specialized_name)?;
                }

                for method in &struct_decl.methods {
                    if !method.modifiers.contains(&Modifier::Native)
                        && !method.modifiers.contains(&Modifier::Abstract)
                    {
                        let mut specialized_method = method.clone();
                        specialized_method.return_type = substitute_type_params(
                            &method.return_type,
                            &resolved_type_args,
                            &type_param_infos,
                        );
                        specialized_method.params = method
                            .params
                            .iter()
                            .map(|p| crate::types::ParameterInfo {
                                name: p.name.clone(),
                                param_type: substitute_type_params(
                                    &p.param_type,
                                    &resolved_type_args,
                                    &type_param_infos,
                                ),
                                is_varargs: p.is_varargs,
                            })
                            .collect();
                        self.generate_method(&specialized_name, &specialized_method)?;
                    }
                }

                self.generic_type_args = old_mapping;
            }
        }

        // 清除命名空间上下文
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace.clear();
        }

        Ok(())
    }

    /// 生成 enum 的所有方法。
    ///
    /// enum 是值类型，表示为 { i32, i64 }；实例方法接收 { i32, i64 }* 作为 this。
    fn generate_enum_methods(&mut self, enum_decl: &EnumDecl) -> CayResult<()> {
        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace = enum_decl.namespace_path.clone();
        }

        let base_qname = if enum_decl.namespace_path.is_empty() {
            enum_decl.name.clone()
        } else {
            format!(
                "{}::{}",
                enum_decl.namespace_path.join("::"),
                enum_decl.name
            )
        };
        let full_enum_name = if enum_decl.type_params.is_empty() {
            base_qname.clone()
        } else {
            let type_param_names: Vec<String> =
                enum_decl.type_params.iter().map(|p| p.name.clone()).collect();
            if enum_decl.namespace_path.is_empty() {
                format!("{}<{ }>", enum_decl.name, type_param_names.join(", "))
            } else {
                format!(
                    "{}::{}<{ }>",
                    enum_decl.namespace_path.join("::"),
                    enum_decl.name,
                    type_param_names.join(", ")
                )
            }
        };

        let is_generic = !enum_decl.type_params.is_empty();
        let type_param_infos: Vec<crate::types::TypeParamInfo> = enum_decl
            .type_params
            .iter()
            .map(|p| crate::types::TypeParamInfo {
                name: p.name.clone(),
                bound: p.bound.clone(),
                default_type: p.default_type.clone(),
            })
            .collect();

        if !is_generic {
            for method in &enum_decl.methods {
                if !method.modifiers.contains(&Modifier::Native)
                    && !method.modifiers.contains(&Modifier::Abstract)
                {
                    self.generate_method(&full_enum_name, method)?;
                }
            }
        } else {
            // 泛型 enum：为每个收集到的特化实例生成单态化方法
            let mut instances = self
                .specializations
                .get(&base_qname)
                .cloned()
                .unwrap_or_default();
            if base_qname != enum_decl.name {
                if let Some(extra) = self.specializations.get(&enum_decl.name) {
                    instances.extend(extra.iter().cloned());
                }
            }

            for instance in instances {
                let specialized_name = instance.specialized_name();
                let resolved_type_args = instance.resolve_type_args(&type_param_infos);

                let mapping = instance.type_param_mapping(&type_param_infos);
                let old_mapping = std::mem::replace(&mut self.generic_type_args, mapping);

                for method in &enum_decl.methods {
                    if !method.modifiers.contains(&Modifier::Native)
                        && !method.modifiers.contains(&Modifier::Abstract)
                    {
                        let mut specialized_method = method.clone();
                        specialized_method.return_type = substitute_type_params(
                            &method.return_type,
                            &resolved_type_args,
                            &type_param_infos,
                        );
                        specialized_method.params = method
                            .params
                            .iter()
                            .map(|p| crate::types::ParameterInfo {
                                name: p.name.clone(),
                                param_type: substitute_type_params(
                                    &p.param_type,
                                    &resolved_type_args,
                                    &type_param_infos,
                                ),
                                is_varargs: p.is_varargs,
                            })
                            .collect();
                        self.generate_method(&specialized_name, &specialized_method)?;
                    }
                }

                self.generic_type_args = old_mapping;
            }
        }

        if let Some(ref mut registry) = self.type_registry {
            registry.current_namespace.clear();
        }

        Ok(())
    }

    /// 生成 struct 构造函数
    fn generate_struct_constructor(
        &mut self,
        struct_name: &str,
        ctor: &crate::ast::ConstructorDecl,
    ) -> CayResult<()> {
        let fn_name = self.generate_constructor_name(struct_name, ctor);

        if ctor.modifiers.contains(&crate::ast::Modifier::Native)
            || ctor.modifiers.contains(&crate::ast::Modifier::Abstract)
        {
            if self.generated_methods.contains(&fn_name) {
                return Ok(());
            }
            self.generated_methods.insert(fn_name.clone());

            let llvm_struct_type = format!("%struct.{}", struct_name);
            let mut params = vec![format!("{}*", llvm_struct_type)];
            for p in &ctor.params {
                params.push(self.type_to_llvm(&p.param_type));
            }
            let cc_attr = self.calling_convention_to_llvm_attr(crate::ast::CallingConvention::Cdecl);
            let decl = if cc_attr.is_empty() {
                format!("declare void @{}({})\n", fn_name, params.join(", "))
            } else {
                format!(
                    "declare void @{}({}) {}\n",
                    fn_name, params.join(", "), cc_attr
                )
            };
            let sig = format!("{}@void@{}", fn_name, params.join("@"));
            if !self.is_extern_emitted(&sig) {
                self.emit_raw(&decl);
                self.mark_extern_emitted(sig);
            }
            return Ok(());
        }

        if self.generated_methods.contains(&fn_name) {
            return Ok(());
        }
        self.generated_methods.insert(fn_name.clone());

        self.current_function = fn_name.clone();
        let raw_struct_name = simple_class_name(struct_name).to_string();
        self.current_class = if let Some(pos) = raw_struct_name.find('<') {
            raw_struct_name[..pos].to_string()
        } else {
            raw_struct_name
        };
        self.current_class_specialized = if struct_name.contains('<') {
            Some(struct_name.to_string())
        } else {
            None
        };
        self.current_return_type = "void".to_string();

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        // struct 的 LLVM 类型名必须将泛型参数与命名空间字符转换为合法标识符
        let llvm_struct_type_name = self.struct_llvm_type_name(struct_name);
        let llvm_struct_type = format!("%struct.{}", llvm_struct_type_name);
        let llvm_struct_ptr = format!("{}*", llvm_struct_type);

        let params: Vec<String> = ctor
            .params
            .iter()
            .map(|p| {
                format!(
                    "{} %{}.{}",
                    self.type_to_llvm(&p.param_type),
                    self.current_class,
                    p.name
                )
            })
            .collect();

        let mut all_params = vec![format!("{} %this", llvm_struct_ptr)];
        all_params.extend(params);

        self.emit_line(&format!(
            "define void @{}({}) {{",
            fn_name,
            all_params.join(", ")
        ));
        self.indent += 1;

        self.emit_line("entry:");
        self.scope_manager.enter_scope();

        let this_llvm_name = self.scope_manager.declare_var("this", &llvm_struct_ptr);
        self.emit_line(&format!(
            "  %{} = alloca {}",
            this_llvm_name, llvm_struct_ptr
        ));
        self.emit_line(&format!(
            "  store {} %this, {}* %{}",
            llvm_struct_ptr, llvm_struct_ptr, this_llvm_name
        ));
        self.var_types.insert("this".to_string(), llvm_struct_ptr.clone());

        for param in &ctor.params {
            let param_type = self.type_to_llvm(&param.param_type);
            let llvm_name =
                self.scope_manager
                    .declare_var_with_flag(&param.name, &param_type, true);
            self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
            // 值类型语义：struct 参数按值传递，入口拷贝到本地独立存储
            if let Some(struct_name) = self.extract_struct_name_from_ptr_type(&param_type) {
                let llvm_struct_type = format!("%struct.{}", struct_name);
                let local_copy = self.new_temp();
                self.emit_line(&format!("  {} = alloca {}", local_copy, llvm_struct_type));
                let src_param = format!("%{}.{}", self.current_class, param.name);
                self.emit_struct_memcpy(&local_copy, &src_param, &struct_name);
                self.emit_line(&format!(
                    "  store {} {}, {}* %{}",
                    param_type, local_copy, param_type, llvm_name
                ));
            } else {
                self.emit_line(&format!(
                    "  store {0} %{1}.{2}, {0}* %{3}",
                    param_type, self.current_class, param.name, llvm_name
                ));
            }
            self.var_types.insert(param.name.clone(), param_type.clone());
            self.var_cay_types
                .insert(param.name.clone(), param.param_type.clone());
        }

        self.generate_block(&ctor.body)?;

        self.emit_all_scope_dtors();
        self.emit_line("  ret void");

        self.scope_manager.exit_scope();
        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    /// 生成 struct 默认构造函数（无参）
    fn generate_struct_default_constructor(&mut self, struct_name: &str) -> CayResult<()> {
        let fn_name = self.mangle_itanium_method(struct_name, "C1", &[], true, false, false);

        if self.generated_methods.contains(&fn_name) {
            return Ok(());
        }
        self.generated_methods.insert(fn_name.clone());

        self.current_function = fn_name.clone();
        let raw_struct_name = simple_class_name(struct_name).to_string();
        self.current_class = if let Some(pos) = raw_struct_name.find('<') {
            raw_struct_name[..pos].to_string()
        } else {
            raw_struct_name
        };
        self.current_return_type = "void".to_string();

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        // struct 的 LLVM 类型名必须将泛型参数与命名空间字符转换为合法标识符
        let llvm_struct_type_name = self.struct_llvm_type_name(struct_name);
        let llvm_struct_type = format!("%struct.{}", llvm_struct_type_name);
        let llvm_struct_ptr = format!("{}*", llvm_struct_type);

        self.emit_line(&format!(
            "define void @{}({} %this) {{",
            fn_name, llvm_struct_ptr
        ));
        self.indent += 1;
        self.emit_line("entry:");
        self.scope_manager.enter_scope();

        let this_llvm_name = self.scope_manager.declare_var("this", &llvm_struct_ptr);
        self.emit_line(&format!(
            "  %{} = alloca {}",
            this_llvm_name, llvm_struct_ptr
        ));
        self.emit_line(&format!(
            "  store {} %this, {}* %{}",
            llvm_struct_ptr, llvm_struct_ptr, this_llvm_name
        ));
        self.var_types.insert("this".to_string(), llvm_struct_ptr);

        self.emit_all_scope_dtors();
        self.emit_line("  ret void");

        self.scope_manager.exit_scope();
        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    /// 生成 vtable 全局常量
    ///
    /// 为每个类生成一个 vtable 数组，包含所有虚方法的函数指针。
    /// vtable 结构：[slot_0, slot_1, ..., slot_N]
    /// 每个 slot 是一个 i8* 类型的函数指针。
    fn generate_vtable_global(&mut self, class_name: &str) -> CayResult<()> {
        let llvm_class = self.get_qualified_class_name(class_name);
        let vtable_name = format!("{}.vtable", llvm_class);

        // 防止重复生成 vtable
        if self.generated_vtables.contains(&vtable_name) {
            return Ok(());
        }

        // 从 TypeRegistry 获取 vtable 布局
        // 对于泛型类，需要提取基础类名（不含泛型参数）进行查找
        let base_class_name = if let Some(pos) = class_name.find('<') {
            &class_name[..pos]
        } else {
            class_name
        };

        let vtable_layout = if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                class_info.vtable_layout.clone()
            } else if let Some(class_info) = registry.get_class(base_class_name) {
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

        // 收集 vtable 条目。接口槽位是全局分配的，可能导致当前类 vtable 中存在空洞。
        let mut entries = vec!["i8* null".to_string(); layout.size];

        // 按槽位编号排序（确保与 vtable 布局一致）
        let mut sorted_slots: Vec<_> = layout.slots.iter().collect();
        sorted_slots.sort_by_key(|&(_, &slot)| slot);

        for (method_sig, slot) in &sorted_slots {
            // 带方法级类型参数的实例泛型方法不发射类型擦除副本
            // （它们在每个调用点单态化，见 call/method_generic.rs），
            // 其 vtable 槽位保持 null；泛型方法的调用始终直接分派。
            let slot_method_name = method_sig.split('(').next().unwrap_or(method_sig);
            if self.method_has_method_level_type_params(class_name, slot_method_name) {
                continue;
            }
            // 查找方法的 LLVM 函数名
            // 需要在继承链中查找方法定义（使用方法签名支持重载）
            let fn_name_opt = self.find_method_in_hierarchy(class_name, method_sig);
            if let Some((fn_name, ret_type, params)) = fn_name_opt {
                let ret_llvm = self.type_to_llvm(&ret_type);
                let mut fn_param_types = vec!["i8*".to_string()];
                for param in &params {
                    fn_param_types.push(self.type_to_llvm(&param));
                }
                let fn_ptr_type = format!("{} ({})", ret_llvm, fn_param_types.join(", "));
                if **slot < entries.len() {
                    entries[**slot] = format!("i8* bitcast ({}* @{} to i8*)", fn_ptr_type, fn_name);
                }
            }
        }

        if layout.size == 0 {
            return Ok(());
        }

        // 生成 vtable 全局常量
        // 类型：[N x i8*]
        // linkonce_odr：同一类的声明（.cayh）与实现（.cay）可能在不同编译单元
        // 各自生成 vtable，由链接器去重（C++ vague linkage 思路）。
        let vtable_type = format!("[{} x i8*]", entries.len());
        self.emit_line(&format!(
            "@{} = linkonce_odr global {} [{}]",
            vtable_name,
            vtable_type,
            entries.join(", ")
        ));

        // 标记已生成
        self.generated_vtables.insert(vtable_name);

        Ok(())
    }

    /// 在继承链中查找方法定义
    /// 使用方法签名（方法名+参数类型）作为键，支持重载方法
    /// 返回 (函数名, 返回类型, 参数类型列表)
    /// 跳过抽象方法（无实现），因为抽象方法没有对应的 LLVM 函数定义
    ///
    /// 当槽位键是泛型接口实例化（如 `$iface$Into<IOError>$into`）时，
    /// 按接口类型实参推导出的期望返回类型消歧仅返回类型不同的重载集合
    /// （如 `Into<IOError>::into` 与 `Into<ParseError>::into`），确保 vtable
    /// 每个特化槽位填入正确的函数指针。
    fn find_method_in_hierarchy(
        &self,
        class_name: &str,
        method_sig: &str,
    ) -> Option<(String, crate::types::Type, Vec<crate::types::Type>)> {
        // 解析接口槽位键，提取接口名与类型实参（用于消歧仅返回类型不同的重载）
        let (interface_name, interface_type_args) =
            crate::types::TypeRegistry::parse_interface_slot_key_type_args(method_sig)
                .unwrap_or(("", Vec::new()));

        let method_sig =
            crate::types::TypeRegistry::interface_vtable_key_method_signature(method_sig)
                .unwrap_or(method_sig);

        // 解析方法签名：方法名(参数类型1,参数类型2,...)
        // 注意：参数类型字符串本身可能包含逗号（如 Generic("ArrayList", [GenericParam("T")])），
        // 因此拆分逗号时必须忽略嵌套在 () / [] / {} 内部的逗号。
        let (method_name, param_types_str) = if let Some(pos) = method_sig.find('(') {
            let name = &method_sig[..pos];
            let params = &method_sig[pos + 1..method_sig.len() - 1]; // 去掉结尾的 )
            (
                name,
                if params.is_empty() {
                    Vec::new()
                } else {
                    let mut parts = Vec::new();
                    let mut depth = 0i32;
                    let mut start = 0;
                    for (i, c) in params.char_indices() {
                        match c {
                            '(' | '[' | '{' => depth += 1,
                            ')' | ']' | '}' => depth -= 1,
                            ',' if depth == 0 => {
                                parts.push(params[start..i].to_string());
                                start = i + 1;
                            }
                            _ => {}
                        }
                    }
                    if start < params.len() {
                        parts.push(params[start..].to_string());
                    }
                    parts
                },
            )
        } else {
            (method_sig, Vec::new())
        };

        // 当槽位键带接口类型实参时，按接口定义推导期望返回类型。
        // 例如 Into<T> 的 into() 返回 T，槽位键 $iface$Into<IOError>$into
        // 期望返回类型为 IOError。用于从多个仅返回类型不同的 into() 重载中
        // 选出与该接口实例化匹配的那个。
        let expected_return_type: Option<crate::types::Type> = if !interface_type_args.is_empty() {
            self.compute_expected_return_type_for_interface_slot(
                interface_name,
                &interface_type_args,
                method_name,
            )
        } else {
            None
        };

        if let Some(ref registry) = self.type_registry {
            let mut current = class_name.to_string();
            loop {
                // 对于泛型类，需要提取基础类名（不含泛型参数）进行查找
                let base_current = if let Some(pos) = current.find('<') {
                    &current[..pos]
                } else {
                    &current
                };
                let class_info_opt = registry
                    .get_class(&current)
                    .or_else(|| registry.get_class(base_current));
                if let Some(class_info) = class_info_opt {
                    // 如果当前类名是泛型特化（如 Pair<int, String>），构建类型参数映射，
                    // 用于将方法签名中的泛型参数替换为具体类型。
                    let type_arg_mapping: std::collections::HashMap<String, crate::types::Type> =
                        if let Some(lt_pos) = current.find('<') {
                            let gt_pos = current.rfind('>').unwrap_or(current.len());
                            let args_str = &current[lt_pos + 1..gt_pos];
                            let type_args: Vec<crate::types::Type> =
                                crate::codegen::specialization::split_top_level_type_args(args_str)
                                    .iter()
                                    .map(|s| {
                                        crate::codegen::specialization::parse_type_str(s.trim())
                                    })
                                    .collect();
                            class_info
                                .type_params
                                .iter()
                                .zip(type_args.iter())
                                .map(|(p, t)| (p.name.clone(), t.clone()))
                                .collect()
                        } else {
                            std::collections::HashMap::new()
                        };

                    if let Some(methods) = class_info.methods.get(method_name) {
                        // 收集当前类中所有参数匹配的方法
                        let mut matched: Vec<(
                            String,
                            crate::types::Type,
                            Vec<crate::types::Type>,
                        )> = Vec::new();
                        for method in methods {
                            // 跳过抽象方法（无实现）
                            if method.is_static || method.is_native || method.is_abstract {
                                continue;
                            }

                            // vtable 布局在语义分析阶段基于基础类（如 std::vector<T>）构建，
                            // 方法签名中的类型参数保持为 GenericParam("T") 形式。
                            // 因此匹配时应使用原始参数类型，而非特化后的具体类型。
                            let original_param_types: Vec<crate::types::Type> = method
                                .params
                                .iter()
                                .map(|p| p.param_type.clone())
                                .collect();
                            let method_param_types: Vec<String> = original_param_types
                                .iter()
                                .map(|t| format!("{:?}", t))
                                .collect();

                            let is_match = if param_types_str.is_empty() {
                                method_param_types.is_empty()
                            } else {
                                method_param_types == param_types_str
                            };

                            if is_match {
                                let substituted_param_types: Vec<crate::types::Type> =
                                    original_param_types
                                        .iter()
                                        .map(|t| {
                                            crate::types::substitute_type_params(
                                                t,
                                                &type_arg_mapping,
                                            )
                                        })
                                        .collect();
                                let substituted_return_type =
                                    crate::types::substitute_type_params(
                                        &method.return_type,
                                        &type_arg_mapping,
                                    );
                                let fn_name = self.mangle_method_with_return_disambiguation(
                                    &current,
                                    method_name,
                                    &substituted_param_types,
                                    &substituted_return_type,
                                    &method.loc, false
                                );
                                matched.push((
                                    fn_name,
                                    substituted_return_type,
                                    substituted_param_types,
                                ));
                            }
                        }

                        // 若有期望返回类型且有多个匹配，按返回类型消歧
                        if matched.len() > 1 {
                            if let Some(ref expected) = expected_return_type {
                                let chosen = matched.iter().find(|(_, ret, _)| {
                                    if ret == expected {
                                        return true;
                                    }
                                    let ret_name = ret.display_name();
                                    let exp_name = expected.display_name();
                                    ret_name == exp_name
                                        || ret_name.rsplit("::").next().unwrap_or(&ret_name)
                                            == exp_name.rsplit("::").next().unwrap_or(&exp_name)
                                });
                                if let Some(m) = chosen {
                                    return Some(m.clone());
                                }
                            }
                        }
                        // 无需消歧或消歧失败时，返回首个匹配（保持既有行为）
                        if let Some(m) = matched.into_iter().next() {
                            return Some(m);
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

    /// 根据接口槽位键中的类型实参，推导接口方法的期望返回类型。
    ///
    /// 例如 `Into<T>` 的 `into()` 方法返回 `T`，当槽位键为
    /// `$iface$Into<IOError>$into` 时，类型实参为 `[IOError]`，
    /// 将接口类型参数 `T` 替换为 `IOError` 后，期望返回类型为 `IOError`。
    fn compute_expected_return_type_for_interface_slot(
        &self,
        interface_name: &str,
        interface_type_args: &[crate::types::Type],
        method_name: &str,
    ) -> Option<crate::types::Type> {
        let registry = self.type_registry.as_ref()?;
        let interface_info = registry.get_interface(interface_name).or_else(|| {
            let bare = interface_name.rsplit("::").next().unwrap_or(interface_name);
            registry.get_interface(bare)
        })?;
        let method = interface_info.methods.get(method_name)?;
        let mapping: std::collections::HashMap<String, crate::types::Type> = interface_info
            .type_params
            .iter()
            .zip(interface_type_args.iter())
            .map(|(p, t)| (p.name.clone(), t.clone()))
            .collect();
        Some(crate::types::substitute_type_params(
            &method.return_type,
            &mapping,
        ))
    }

    fn generate_method(&mut self, class_name: &str, method: &MethodDecl) -> CayResult<()> {
        let fn_name = self.generate_method_name(class_name, method);
        self.generate_method_with_name(class_name, method, fn_name)
    }

    /// 判断同 TU 内是否存在该 native 方法的实现（.cayh 声明类 + .cay 实现类
    /// 合并场景）：registry 中存在同名、同签名、static 性一致且非 native 的条目。
    /// 查找失败（如泛型特化名）保守返回 false，维持原有 declare 行为。
    fn class_method_has_local_impl(&self, class_name: &str, method: &MethodDecl) -> bool {
        let is_static = method.modifiers.contains(&Modifier::Static);
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(candidates) = class_info.methods.get(&method.name) {
                    return candidates.iter().any(|m| {
                        !m.is_native
                            && m.is_static == is_static
                            && m.params.len() == method.params.len()
                            && m.params.iter().zip(method.params.iter()).all(|(a, b)| {
                                a.param_type == b.param_type && a.is_varargs == b.is_varargs
                            })
                    });
                }
            }
        }
        false
    }

    /// 判断同 TU 内是否存在该 native/abstract 构造函数的实现（.cayh 声明类 +
    /// .cay 实现类合并场景）：registry 中存在同签名且非 native 的构造条目。
    /// 查找失败保守返回 false，维持原有 declare 行为。
    fn class_ctor_has_local_impl(&self, class_name: &str, ctor: &crate::ast::ConstructorDecl) -> bool {
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                return class_info.constructors.iter().any(|c| {
                    !c.is_native
                        && c.params.len() == ctor.params.len()
                        && c.params.iter().zip(ctor.params.iter()).all(|(a, b)| {
                            a.param_type == b.param_type && a.is_varargs == b.is_varargs
                        })
                });
            }
        }
        false
    }

    /// 以显式指定的函数名生成方法体。
    ///
    /// 与 `generate_method` 的唯一差别是函数名由调用方给出而非从方法签名推导，
    /// 供方法级泛型（`method<U>(...)`）的单态化副本使用——其名字中包含
    /// 方法级类型实参（见 `mangle_method_with_type_args`）。
    pub(crate) fn generate_method_with_name(
        &mut self,
        class_name: &str,
        method: &MethodDecl,
        fn_name: String,
    ) -> CayResult<()> {

        // native 方法不生成实现体，但必须在 IR 中声明以便调用点引用
        if method.modifiers.contains(&Modifier::Native) {
            // 实现已生成（同 TU 内声明类与实现类合并的场景，实现先行）则无需再声明
            if self.generated_methods.contains(&fn_name) {
                return Ok(());
            }
            // 同 TU 内存在该方法的实现（.cayh 声明类 + .cay 实现类已合并进
            // registry，实现条目覆盖了 native 声明条目）时跳过 declare：
            // 捆绑的 llvm-minimal 不接受同一模块内 declare+define 同名函数。
            if self.class_method_has_local_impl(class_name, method) {
                return Ok(());
            }
            // 注意：不向 generated_methods 插入——native 仅是声明，不能遮蔽
            // 同 TU 内随后的实现定义（.cayh 声明类先于实现类处理的顺序）。

            let ret_type = self.type_to_llvm(&method.return_type);
            // this 前缀必须与定义侧签名一致（见下方 generate_method_with_name 的
            // 非 native 分支）：static 方法无 this；struct/enum 方法的 this 不是 i8*。
            // 此前无条件加 i8* 会导致 native static 声明与静态定义签名不匹配，
            // 跨 TU 链接（.cayh 声明 + .cay 实现）时失败。
            let is_static = method.modifiers.contains(&Modifier::Static);
            let mut params: Vec<String> = Vec::new();
            if !is_static {
                let this_ptr_type = if self.is_struct_type(class_name) {
                    format!("%struct.{}*", self.struct_llvm_type_name(class_name))
                } else if self.is_enum_type(class_name) {
                    "{ i32, i64 }*".to_string()
                } else {
                    "i8*".to_string()
                };
                params.push(this_ptr_type);
            }
            for p in &method.params {
                params.push(self.type_to_llvm(&p.param_type));
            }
            let cc_attr = self.calling_convention_to_llvm_attr(crate::ast::CallingConvention::Cdecl);
            let decl = if cc_attr.is_empty() {
                format!("declare {} @{}({})\n", ret_type, fn_name, params.join(", "))
            } else {
                format!(
                    "declare {} @{}({}) {}\n",
                    ret_type, fn_name, params.join(", "), cc_attr
                )
            };
            let sig = format!("{}@{}@{}", fn_name, ret_type, params.join("@"));
            if !self.is_extern_emitted(&sig) {
                self.emit_raw(&decl);
                self.mark_extern_emitted(sig);
            }
            return Ok(());
        }

        // 跳过 abstract 方法（无实现，也无外部符号）
        if method.modifiers.contains(&Modifier::Abstract) {
            return Ok(());
        }

        // 防止重复生成相同名称的方法（泛型特化可能产生同名方法）
        if self.generated_methods.contains(&fn_name) {
            return Ok(());
        }
        self.generated_methods.insert(fn_name.clone());

        self.current_function = fn_name.clone();
        // 从可能包含 :: 的限定名中提取简单名用于 current_class
        let raw_class_name = simple_class_name(class_name).to_string();
        // 提取简单类名（不含泛型参数）用于参数名生成
        self.current_class = if let Some(pos) = raw_class_name.find('<') {
            raw_class_name[..pos].to_string()
        } else {
            raw_class_name
        };
        // 泛型特化方法：保留完整特化名（含命名空间与类型实参）以定位单态化字段布局。
        self.current_class_specialized = if class_name.contains('<') {
            Some(class_name.to_string())
        } else {
            None
        };
        self.current_return_type = self.type_to_llvm(&method.return_type);
        self.current_function_cay_return_type = Some(method.return_type.clone());

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        // 设置当前函数参数顺序（用于内联IR）
        self.current_param_order = method.params.iter().map(|p| p.name.clone()).collect();

        let ret_type = self.current_return_type.clone();
        let is_static = method.modifiers.contains(&Modifier::Static);

        let mut params: Vec<String> = Vec::new();

        // 判断是否是 struct / enum 方法（决定 this 指针类型）
        let is_struct_method = self.is_struct_type(class_name);
        let is_enum_method = self.is_enum_type(class_name);
        // this 元素类型：class 为 i8*（对象地址），struct 为 %struct.Name（结构体值类型），
        // enum 为 { i32, i64 }。this 指针类型为对应元素类型加 *。
        // 对于泛型 struct，必须使用 struct_llvm_type_name 生成合法 LLVM 标识符。
        let (this_elem_type, this_ptr_type) = if is_struct_method {
            let llvm_struct_type_name = self.struct_llvm_type_name(class_name);
            let elem = format!("%struct.{}", llvm_struct_type_name);
            let ptr = format!("{}*", elem);
            (elem, ptr)
        } else if is_enum_method {
            ("{ i32, i64 }".to_string(), "{ i32, i64 }*".to_string())
        } else {
            ("i8*".to_string(), "i8*".to_string())
        };

        // 实例方法添加 this 参数
        // 对于 class 类型，this 是 i8*；对于 struct 类型，this 是指向 struct 的指针。
        // this 不应当有双重间接 (i8**)，C++ ABI 只需要一层指针。
        if !is_static {
            params.push(format!("{} %this", this_ptr_type));
        }

        for param in &method.params {
            let param_llvm_type = if param.is_varargs {
                // 可变参数使用 i8* 指针类型（数组的内存地址）
                "i8*".to_string()
            } else {
                self.type_to_llvm(&param.param_type)
            };
            params.push(format!(
                "{} %{}.{}",
                param_llvm_type, self.current_class, param.name
            ));
        }

        // enum/class 定义可能被多个编译单元同时包含（如 .cayh 声明文件、
        // 多文件各自 #include <std/...> 的场景），其方法在每个 TU 各生成一份
        // 定义；用 linkonce_odr 让链接器去重（C++ vague linkage 思路，
        // 仿 Object 默认构造、析构与 vtable 的先例）。
        let linkage = if is_enum_method || !is_struct_method {
            "linkonce_odr "
        } else {
            ""
        };
        self.emit_line(&format!(
            "define {}{} @{}({}) {{",
            linkage,
            ret_type,
            fn_name,
            params.join(", ")
        ));
        self.indent += 1;

        self.emit_line("entry:");

        // 进入函数作用域，确保变量名有正确的作用域后缀
        self.scope_manager.enter_scope();

        // 实例方法声明 this 变量
        // class 方法中 this 在 alloca 中存储为 i8*；struct 方法中存储为 %struct.Name*。
        // 两者都是指向对象/结构体的单层指针，与字段访问代码保持一致。
        if !is_static {
            let this_llvm_name = self
                .scope_manager
                .declare_var("this", &this_ptr_type);
            self.emit_line(&format!(
                "  %{} = alloca {}",
                this_llvm_name, this_ptr_type
            ));
            self.emit_line(&format!(
                "  store {} %this, {}* %{}",
                this_ptr_type, this_ptr_type, this_llvm_name
            ));
            // DWARF 调试信息: 声明 this 指针
            self.emit_dbg_declare("this", &this_llvm_name, &this_ptr_type, method.loc.line, None);
            self.var_types
                .insert("this".to_string(), this_ptr_type.clone());
            // 存储 this 的 Cavvy 类型信息，用于准确的类型推断
            let this_cay_type = crate::types::Type::Object(class_name.to_string());
            self.var_cay_types.insert("this".to_string(), this_cay_type);
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
                let llvm_name =
                    self.scope_manager
                        .declare_var_with_flag(&param.name, &array_type, true);
                self.emit_line(&format!("  %{} = alloca {}", llvm_name, array_type));

                // 将 i8* 参数转换为正确的数组类型指针
                let cast_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i8* %{}.{} to {}",
                    cast_temp, self.current_class, param.name, array_type
                ));
                self.emit_line(&format!(
                    "  store {} {}, {}* %{}",
                    array_type, cast_temp, array_type, llvm_name
                ));

                self.var_types
                    .insert(param.name.clone(), array_type.clone());
                // 存储Cavvy类型信息，用于准确的类型推断
                self.var_cay_types
                    .insert(param.name.clone(), param.param_type.clone());
                // 如果参数类型是对象或泛型，记录其类名以便后续方法调用解析
                match &param.param_type {
                    crate::types::Type::Object(class_name) => {
                        self.var_class_map
                            .insert(param.name.clone(), class_name.clone());
                    }
                    crate::types::Type::Generic(class_name, type_args) => {
                        // 与局部变量声明一致：类型实参全部具体时记录完整特化名
                        // （如 "Pair<int, int>"），使泛型 struct/类的参数上的方法调用
                        // 能解析到单态化版本；否则退回基础类名。
                        let resolved: Vec<crate::types::Type> = type_args
                            .iter()
                            .map(|t| self.resolve_type_arg_concrete(t))
                            .collect();
                        let all_concrete = !resolved.is_empty()
                            && resolved.iter().all(|t| self.type_arg_is_concrete(t));
                        if all_concrete {
                            let args_str: Vec<String> =
                                resolved.iter().map(|t| t.display_name()).collect();
                            self.var_class_map.insert(
                                param.name.clone(),
                                format!("{}<{ }>", class_name, args_str.join(", ")),
                            );
                        } else {
                            self.var_class_map
                                .insert(param.name.clone(), class_name.clone());
                        }
                    }
                    _ => {}
                }
            } else {
                let param_type = self.type_to_llvm(&param.param_type);
                let llvm_name =
                    self.scope_manager
                        .declare_var_with_flag(&param.name, &param_type, true);
                self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
                // 值类型语义：struct 参数按值传递。调用方传入的是指向其存储的指针，
                // 函数入口必须将值拷贝到本地独立存储，否则函数内对参数的修改会
                // 影响调用方的对象（引用语义）。
                if let Some(struct_name) = self.extract_struct_name_from_ptr_type(&param_type) {
                    let llvm_struct_type = format!("%struct.{}", struct_name);
                    let local_copy = self.new_temp();
                    self.emit_line(&format!("  {} = alloca {}", local_copy, llvm_struct_type));
                    let src_param = format!("%{}.{}", self.current_class, param.name);
                    self.emit_struct_memcpy(&local_copy, &src_param, &struct_name);
                    self.emit_line(&format!(
                        "  store {} {}, {}* %{}",
                        param_type, local_copy, param_type, llvm_name
                    ));
                } else {
                    self.emit_line(&format!(
                        "  store {} %{}.{}, {}* %{}",
                        param_type, self.current_class, param.name, param_type, llvm_name
                    ));
                }
                // DWARF 调试信息: 声明参数变量
                self.emit_dbg_declare(
                    &param.name,
                    &llvm_name,
                    &param_type,
                    method.loc.line,
                    None,
                );
                self.var_types
                    .insert(param.name.clone(), param_type.clone());
                // 存储Cavvy类型信息，用于准确的类型推断
                self.var_cay_types
                    .insert(param.name.clone(), param.param_type.clone());
                // 如果参数类型是对象或泛型，记录其类名以便后续方法调用解析
                match &param.param_type {
                    crate::types::Type::Object(class_name) => {
                        self.var_class_map
                            .insert(param.name.clone(), class_name.clone());
                    }
                    crate::types::Type::Generic(class_name, type_args) => {
                        // 与局部变量声明一致：类型实参全部具体时记录完整特化名
                        // （如 "Pair<int, int>"），使泛型 struct/类的参数上的方法调用
                        // 能解析到单态化版本；否则退回基础类名。
                        let resolved: Vec<crate::types::Type> = type_args
                            .iter()
                            .map(|t| self.resolve_type_arg_concrete(t))
                            .collect();
                        let all_concrete = !resolved.is_empty()
                            && resolved.iter().all(|t| self.type_arg_is_concrete(t));
                        if all_concrete {
                            let args_str: Vec<String> =
                                resolved.iter().map(|t| t.display_name()).collect();
                            self.var_class_map.insert(
                                param.name.clone(),
                                format!("{}<{ }>", class_name, args_str.join(", ")),
                            );
                        } else {
                            self.var_class_map
                                .insert(param.name.clone(), class_name.clone());
                        }
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
            // ROADMAP 5.3.x 自动 RAII：void 方法默认返回前析构所有未退出作用域。
            self.emit_all_scope_dtors();
            self.emit_line("  ret void");
        }

        // 退出函数作用域
        self.scope_manager.exit_scope();

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    fn generate_constructor(
        &mut self,
        class_name: &str,
        ctor: &crate::ast::ConstructorDecl,
    ) -> CayResult<()> {
        let fn_name = self.generate_constructor_name(class_name, ctor);

        // native/abstract 构造函数由外部（如 C++ 互操作对象）提供实现，
        // Cavvy 不生成其 LLVM 函数体，但需要在 IR 中声明以便 new 表达式调用。
        if ctor.modifiers.contains(&crate::ast::Modifier::Native)
            || ctor.modifiers.contains(&crate::ast::Modifier::Abstract)
        {
            // 实现已生成（同 TU 声明/实现合并、实现先行）则无需再声明；
            // 不向 generated_methods 插入，避免遮蔽同 TU 随后的实现定义。
            if self.generated_methods.contains(&fn_name) {
                return Ok(());
            }
            // 同 TU 内存在该构造函数的实现（.cayh 声明类 + 实现类合并进
            // registry，实现条目覆盖了 native 声明条目）时跳过 declare：
            // 捆绑的 llvm-minimal 不接受同一模块内 declare+define 同名函数。
            if self.class_ctor_has_local_impl(class_name, ctor) {
                return Ok(());
            }

            let mut params = vec!["i8*".to_string()];
            for p in &ctor.params {
                params.push(self.type_to_llvm(&p.param_type));
            }
            let cc_attr = self.calling_convention_to_llvm_attr(crate::ast::CallingConvention::Cdecl);
            let decl = if cc_attr.is_empty() {
                format!("declare void @{}({})\n", fn_name, params.join(", "))
            } else {
                format!(
                    "declare void @{}({}) {}\n",
                    fn_name, params.join(", "), cc_attr
                )
            };
            let sig = format!("{}@void@{}", fn_name, params.join("@"));
            if !self.is_extern_emitted(&sig) {
                self.emit_raw(&decl);
                self.mark_extern_emitted(sig);
            }
            return Ok(());
        }


        // 防止重复生成相同名称的构造函数（泛型特化可能产生同名构造函数）
        if self.generated_methods.contains(&fn_name) {
            return Ok(());
        }
        self.generated_methods.insert(fn_name.clone());

        self.current_function = fn_name.clone();
        // 从可能包含 :: 的限定名中提取简单名用于 current_class
        let raw_class_name = simple_class_name(class_name).to_string();
        // 提取简单类名（不含泛型参数）用于参数名生成
        self.current_class = if let Some(pos) = raw_class_name.find('<') {
            raw_class_name[..pos].to_string()
        } else {
            raw_class_name
        };
        // 泛型特化方法：保留完整特化名（含命名空间与类型实参）以定位单态化字段布局。
        self.current_class_specialized = if class_name.contains('<') {
            Some(class_name.to_string())
        } else {
            None
        };
        self.current_return_type = "void".to_string();

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        let params: Vec<String> = ctor
            .params
            .iter()
            .map(|p| {
                format!(
                    "{} %{}.{}_param",
                    self.type_to_llvm(&p.param_type),
                    self.current_class,
                    p.name
                )
            })
            .collect();

        let mut all_params = vec![format!("i8* %this")];
        all_params.extend(params);

        self.emit_line(&format!(
            "define void @{}({}) {{",
            fn_name,
            all_params.join(", ")
        ));
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
            let llvm_name =
                self.scope_manager
                    .declare_var_with_flag(&param.name, &param_type, true);
            self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
            // 值类型语义：struct 参数按值传递，入口拷贝到本地独立存储
            if let Some(struct_name) = self.extract_struct_name_from_ptr_type(&param_type) {
                let llvm_struct_type = format!("%struct.{}", struct_name);
                let local_copy = self.new_temp();
                self.emit_line(&format!("  {} = alloca {}", local_copy, llvm_struct_type));
                let src_param = format!("%{}.{}_param", self.current_class, param.name);
                self.emit_struct_memcpy(&local_copy, &src_param, &struct_name);
                self.emit_line(&format!(
                    "  store {} {}, {}* %{}",
                    param_type, local_copy, param_type, llvm_name
                ));
            } else {
                self.emit_line(&format!(
                    "  store {} %{}.{}_param, {}* %{}",
                    param_type, self.current_class, param.name, param_type, llvm_name
                ));
            }
            self.var_types
                .insert(param.name.clone(), param_type.clone());
            self.var_cay_types
                .insert(param.name.clone(), param.param_type.clone());
        }

        if let Some(ref call) = ctor.constructor_call {
            match call {
                crate::ast::ConstructorCall::This(args) => {
                    // 从类型注册表获取真实的构造函数参数类型（用于 Itanium ABI mangling）
                    let fallback_types: Vec<String> = args
                        .iter()
                        .map(|arg| self.infer_expr_type_for_ctor(arg))
                        .collect();
                    let param_types = self.get_constructor_param_types(
                        class_name,
                        args.len(),
                        &fallback_types,
                    );
                    let target_ctor_name =
                        self.generate_constructor_call_name_with_types(class_name, &param_types);
                    let mut arg_strs = vec!["i8* %this".to_string()];
                    for arg in args {
                        let arg_val = self.generate_expression(arg)?;
                        arg_strs.push(arg_val);
                    }
                    self.emit_line(&format!(
                        "  call void @{}({})",
                        target_ctor_name,
                        arg_strs.join(", ")
                    ));
                }
                crate::ast::ConstructorCall::Super(args) => {
                    if let Some(ref registry) = self.type_registry {
                        if let Some(class_info) = registry.get_class(class_name) {
                            if let Some(ref parent_name) = class_info.parent {
                                // 从类型注册表获取真实的父类构造函数参数类型（用于 Itanium ABI mangling）
                                let fallback_types: Vec<String> = args
                                    .iter()
                                    .map(|arg| self.infer_expr_type_for_ctor(arg))
                                    .collect();
                                let param_types = self.get_constructor_param_types(
                                    parent_name,
                                    args.len(),
                                    &fallback_types,
                                );
                                let parent_ctor_name = self
                                    .generate_constructor_call_name_with_types(
                                        parent_name,
                                        &param_types,
                                    );
                                let mut arg_strs = vec!["i8* %this".to_string()];
                                for arg in args {
                                    let arg_val = self.generate_expression(arg)?;
                                    arg_strs.push(arg_val);
                                }
                                self.emit_line(&format!(
                                    "  call void @{}({})",
                                    parent_ctor_name,
                                    arg_strs.join(", ")
                                ));
                            }
                        }
                    }
                }
            }
        }

        // 生成字段初始化器代码（super/this 调用之后，构造函数体之前）
        self.generate_field_initializers(class_name)?;

        self.generate_block(&ctor.body)?;

        // ROADMAP 5.3.x 自动 RAII：构造函数返回前析构所有未退出作用域。
        self.emit_all_scope_dtors();

        self.emit_line("  ret void");

        // 退出函数作用域
        self.scope_manager.exit_scope();

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    /// 生成默认构造函数（无参构造函数）
    fn generate_default_constructor(&mut self, class_name: &str) -> CayResult<()> {
        let fn_name = self.mangle_itanium_method(class_name, "C1", &[], true, false, false);

        // 防止重复生成相同名称的默认构造函数（泛型特化可能产生同名构造函数）
        if self.generated_methods.contains(&fn_name) {
            return Ok(());
        }
        self.generated_methods.insert(fn_name.clone());

        self.current_function = fn_name.clone();
        // 从可能包含 :: 的限定名中提取简单名用于 current_class
        let raw_class_name = simple_class_name(class_name).to_string();
        // 提取简单类名（不含泛型参数）用于参数名生成
        self.current_class = if let Some(pos) = raw_class_name.find('<') {
            raw_class_name[..pos].to_string()
        } else {
            raw_class_name
        };
        // 泛型特化方法：保留完整特化名（含命名空间与类型实参）以定位单态化字段布局。
        self.current_class_specialized = if class_name.contains('<') {
            Some(class_name.to_string())
        } else {
            None
        };
        self.current_return_type = "void".to_string();

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        // 隐式默认构造函数在每个编译单元都会生成（C++ 中隐式构造等同 inline），
        // 统一使用 linkonce_odr 让链接器去重——多文件编译（.cayh 声明文件模型）
        // 时同名默认构造不会冲突；实现文件中的显式构造是强符号，天然优先。
        self.emit_line(&format!(
            "define linkonce_odr void @{}(i8* %this) {{",
            fn_name
        ));
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
                    let parent_ctor_name =
                        self.mangle_itanium_method(parent_name, "C1", &[], true, false, false);
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
    fn generate_field_initializers(&mut self, class_name: &str) -> CayResult<()> {
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
                    init_val
                        .split_whitespace()
                        .last()
                        .unwrap_or(&init_val)
                        .to_string()
                } else {
                    init_val
                };

                // 存储到 this.field
                let this_info = self
                    .scope_manager
                    .lookup_var("this")
                    .map(|v| v.llvm_name.clone())
                    .unwrap_or_else(|| "this".to_string());
                // this 是 alloca，需要先 load 出实际指针
                // this_info 来自 scope_manager（无 %），new_temp 返回带 % 的名称
                let this_loaded = self.new_temp();
                self.emit_line(&format!(
                    "  {} = load i8*, i8** %{}",
                    this_loaded, this_info
                ));
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
    fn get_field_offset(&self, class_name: &str, field_name: &str) -> CayResult<i64> {
        if let Some(layout) = self.class_layouts.get(class_name) {
            if let Some(field) = layout.fields.get(field_name) {
                return Ok(field.offset as i64);
            }
        }
        // 布局查找失败必须硬报错：若静默按偏移 0 生成代码，
        // 会产生读写 this+0 的错误机器码
        Err(codegen_error_at(
            ErrorCodes::CODEGEN_SYMBOL_NOT_FOUND,
            SourceLocation::new(
                Some(self.source_file.clone()),
                self.source_line.max(1),
                self.source_column.max(1),
            ),
            format!(
                "找不到类 '{}' 中字段 '{}' 的布局信息，无法生成字段访问代码",
                class_name, field_name
            ),
        ))
    }

    fn generate_destructor(
        &mut self,
        class_name: &str,
        dtor: &crate::ast::DestructorDecl,
    ) -> CayResult<()> {
        let llvm_class = self.get_qualified_class_name(class_name);
        let fn_name = self.mangle_itanium_method(class_name, "D1", &[], false, true, false);

        // native 析构不生成实现体（符号由外部 C++ 实现提供），
        // 但必须在 IR 中声明以便 RAII 调用点引用（仿 native 方法/构造）。
        // 实现已生成（同 TU 声明/实现合并、实现先行）则跳过；且不向
        // generated_methods 插入，避免遮蔽同 TU 内随后的实现定义。
        if dtor.modifiers.contains(&Modifier::Native) {
            if self.generated_methods.contains(&fn_name) {
                return Ok(());
            }
            // 同 TU 内存在该析构的实现（.cayh 声明类 + 实现类合并场景）时跳过
            // declare：捆绑的 llvm-minimal 不接受同模块 declare+define 同名函数。
            if let Some(ref registry) = self.type_registry {
                if registry
                    .get_class(class_name)
                    .map_or(false, |c| c.has_destructor && !c.destructor_is_native)
                {
                    return Ok(());
                }
            }
            let cc_attr = self.calling_convention_to_llvm_attr(crate::ast::CallingConvention::Cdecl);
            let decl = if cc_attr.is_empty() {
                format!("declare void @{}(i8*)\n", fn_name)
            } else {
                format!("declare void @{}(i8*) {}\n", fn_name, cc_attr)
            };
            let sig = format!("{}@void@i8*", fn_name);
            if !self.is_extern_emitted(&sig) {
                self.emit_raw(&decl);
                self.mark_extern_emitted(sig);
            }
            return Ok(());
        }

        // 防止重复生成相同名称的析构函数（泛型特化可能产生同名析构函数）
        if self.generated_methods.contains(&fn_name) {
            return Ok(());
        }
        self.generated_methods.insert(fn_name.clone());

        self.current_function = fn_name.clone();
        // 从可能包含 :: 的限定名中提取简单名用于 current_class
        let raw_class_name = simple_class_name(class_name).to_string();
        // 提取简单类名（不含泛型参数）用于参数名生成
        self.current_class = if let Some(pos) = raw_class_name.find('<') {
            raw_class_name[..pos].to_string()
        } else {
            raw_class_name
        };
        // 泛型特化方法：保留完整特化名（含命名空间与类型实参）以定位单态化字段布局。
        self.current_class_specialized = if class_name.contains('<') {
            Some(class_name.to_string())
        } else {
            None
        };
        self.current_return_type = "void".to_string();

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        self.emit_line(&format!("define void @{}(i8* %this) {{", fn_name));
        self.indent += 1;

        self.emit_line("entry:");

        // 进入函数作用域，确保 this 的 alloca 名不会与参数 %this 冲突。
        // 构造函数（generate_constructor）在声明 this 前已调用 enter_scope，
        // 析构函数必须保持一致，否则 %this = alloca i8* 会重复定义参数 %this。
        self.scope_manager.enter_scope();

        let this_llvm_name = self.scope_manager.declare_var("this", "i8*");
        self.emit_line(&format!("  %{} = alloca i8*", this_llvm_name));
        self.emit_line(&format!("  store i8* %this, i8** %{}", this_llvm_name));
        self.var_types.insert("this".to_string(), "i8*".to_string());

        // ROADMAP 5.3.x 自动 RAII：对 ArrayList<T> 在调用用户析构体之前
        // 先析构其中拥有的元素，避免嵌套容器内存泄漏。
        self.emit_arraylist_dtor_injection(class_name)?;

        self.generate_block(&dtor.body)?;

        // 退出函数作用域（与构造函数 enter_scope/exit_scope 对称）
        self.scope_manager.exit_scope();

        // ROADMAP 5.3.x 自动 RAII：析构函数返回前析构所有未退出作用域。
        self.emit_all_scope_dtors();

        // ROADMAP 5.3.x 智能指针注入：特化 UniquePtr/ScopedPtr/Rc 的 __dtor
        // 在返回前自动调用托管 T 的 __dtor 并释放内存。
        self.emit_smart_ptr_dtor_injection(class_name)?;

        self.emit_line("  ret void");

        self.indent -= 1;
        self.emit_line("}");
        self.emit_line("");

        Ok(())
    }

    /// ROADMAP 5.3.x 智能指针特化 `__dtor` 注入：
    /// 为 `UniquePtr<T>` / `ScopedPtr<T>` / `Rc<T>` 的特化析构函数末尾自动插入
    /// 对托管对象 `T` 的析构与内存释放。`WeakPtr<T>` 不托管对象，不注入。
    ///
    /// 注入逻辑在 `generate_destructor` 生成用户析构体之后、`ret void` 之前
    /// 执行。仅对泛型特化类生效；非泛型类或不在名单内的类无任何影响。
    fn emit_smart_ptr_dtor_injection(
        &mut self,
        class_name: &str,
    ) -> CayResult<()> {
        // 仅处理泛型特化类；提取基础类名（去掉类型实参）。
        let base_name = if let Some(pos) = class_name.find('<') {
            &class_name[..pos]
        } else {
            return Ok(());
        };

        let kind = match base_name {
            "UniquePtr" | "std::UniquePtr" => SmartPtrKind::Owned,
            "ScopedPtr" | "std::ScopedPtr" => SmartPtrKind::Owned,
            "Rc" | "std::Rc" => SmartPtrKind::Rc,
            "WeakPtr" | "std::WeakPtr" => SmartPtrKind::WeakPtr,
            "Optional" | "std::Optional" => SmartPtrKind::Optional,
            _ => return Ok(()),
        };

        // 若当前基本块已被终止（如用户析构体以 return 结束），无法追加指令。
        if self.current_block_terminated() {
            return Ok(());
        }

        // 解析类型参数 T。
        let t_type = self
            .generic_type_args
            .get("T")
            .cloned()
            .unwrap_or(crate::types::Type::Void);
        if t_type == crate::types::Type::Void {
            return Ok(());
        }

        // 确保 free 声明存在。
        self.ensure_free_declared();

        match kind {
            SmartPtrKind::Owned => {
                self.emit_owned_drop_injection(class_name, &t_type)?;
            }
            SmartPtrKind::Rc => {
                self.emit_rc_drop_injection(class_name, &t_type)?;
            }
            SmartPtrKind::WeakPtr => {
                self.emit_weakptr_drop_injection(class_name)?;
            }
            SmartPtrKind::Optional => {
                self.emit_optional_drop_injection(class_name, &t_type)?;
            }
        }

        Ok(())
    }

    /// 为 `ArrayList<T, A>` 特化析构函数注入元素析构逻辑。
    ///
    /// 在调用用户编写的析构体之前执行：遍历 `this.data[0..this.size)`，对其中
    /// 每个元素，若元素类型 `T` 带析构函数，则调用其 `__dtor`。这样嵌套
    /// `ArrayList` 的析构会递归释放内层对象，避免内存泄漏。
    ///
    /// 安全性前提：调用方已经把 add 进 ArrayList 的局部对象变量从当前作用域
    /// 析构候选中移除（见 `codegen/expressions/call/main.rs` 中的 add 调用处理），
    /// 否则会出现 double-free。
    fn emit_arraylist_dtor_injection(&mut self, class_name: &str) -> CayResult<()> {
        // 仅对 ArrayList 特化生效。
        let base_name = if let Some(pos) = class_name.find('<') {
            &class_name[..pos]
        } else {
            return Ok(());
        };
        if base_name != "ArrayList" && base_name != "std::ArrayList" {
            return Ok(());
        }

        if self.current_block_terminated() {
            return Ok(());
        }

        let t_type = self
            .generic_type_args
            .get("T")
            .cloned()
            .unwrap_or(crate::types::Type::Void);
        if t_type == crate::types::Type::Void {
            return Ok(());
        }

        // 只有 T 带析构函数时才需要注入；原始类型直接跳过。
        let Some(t_class) = self.type_has_destructor(&t_type) else {
            return Ok(());
        };

        let dtor_fn = self.mangle_itanium_method(&t_class, "D1", &[], false, true, false);

        let data_offset = self.get_field_offset(class_name, "data")?;
        let size_offset = self.get_field_offset(class_name, "size")?;

        // 加载 this.size
        let size_gep = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* %this, i64 {}",
            size_gep, size_offset
        ));
        let size_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", size_ptr, size_gep));
        let size_val = self.new_temp();
        self.emit_line(&format!("  {} = load i32, i32* {}", size_val, size_ptr));

        // 加载 this.data（T[] 的 i8* 数组头指针）
        let data_gep = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* %this, i64 {}",
            data_gep, data_offset
        ));
        let data_ptr_slot = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8**",
            data_ptr_slot, data_gep
        ));
        let data_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8** {}",
            data_ptr, data_ptr_slot
        ));

        // data 为 null 则跳过。
        let is_null = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i8* {}, null",
            is_null, data_ptr
        ));
        let skip_label = self.new_label("arraylist.dtor.skip");
        let loop_header_label = self.new_label("arraylist.dtor.loop.header");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            is_null, skip_label, loop_header_label
        ));
        self.emit_line(&format!("{}:", skip_label));
        let end_label = self.new_label("arraylist.dtor.end");
        self.emit_line(&format!("  br label %{}", end_label));

        // 循环：for (int i = 0; i < size; i++)
        self.emit_line(&format!("{}:", loop_header_label));
        let counter_ptr = self.new_temp();
        self.emit_line(&format!("  {} = alloca i32", counter_ptr));
        self.emit_line(&format!("  store i32 0, i32* {}", counter_ptr));
        let loop_check_label = self.new_label("arraylist.dtor.loop.check");
        let loop_body_label = self.new_label("arraylist.dtor.loop.body");
        self.emit_line(&format!("  br label %{}", loop_check_label));

        self.emit_line(&format!("{}:", loop_check_label));
        let i_val = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i32, i32* {}",
            i_val, counter_ptr
        ));
        let cmp = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp slt i32 {}, {}",
            cmp, i_val, size_val
        ));
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            cmp, loop_body_label, end_label
        ));

        self.emit_line(&format!("{}:", loop_body_label));
        // ArrayList 的 `data` 字段存储的是数组元素首地址（已跳过 [i32 length,
        // i32 padding] 头），因此直接使用 data_ptr 作为元素基址即可。
        let data_start = data_ptr;
        // offset = i * sizeof(T)。T 带析构函数时必为对象/指针类型，槽位 8 字节。
        let elem_size = t_type.size_in_bytes().max(1) as i64;
        let i64_val = self.new_temp();
        self.emit_line(&format!(
            "  {} = sext i32 {} to i64",
            i64_val, i_val
        ));
        let byte_offset = self.new_temp();
        self.emit_line(&format!(
            "  {} = mul i64 {}, {}",
            byte_offset, i64_val, elem_size
        ));
        let elem_slot = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 {}",
            elem_slot, data_start, byte_offset
        ));
        let elem_ptr_slot = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8**",
            elem_ptr_slot, elem_slot
        ));
        let elem_obj = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8** {}",
            elem_obj, elem_ptr_slot
        ));

        // 元素指针为 null 则跳过。
        let elem_null = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i8* {}, null",
            elem_null, elem_obj
        ));
        let elem_skip_label = self.new_label("arraylist.dtor.elem.skip");
        let elem_done_label = self.new_label("arraylist.dtor.elem.done");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            elem_null, elem_skip_label, elem_done_label
        ));
        self.emit_line(&format!("{}:", elem_skip_label));
        self.emit_line(&format!("  br label %{}", elem_done_label));

        self.emit_line(&format!("{}:", elem_done_label));
        self.emit_line(&format!(
            "  call void @{}(i8* {})",
            dtor_fn, elem_obj
        ));

        // i++
        let next_i = self.new_temp();
        self.emit_line(&format!(
            "  {} = add i32 {}, 1",
            next_i, i_val
        ));
        self.emit_line(&format!(
            "  store i32 {}, i32* {}",
            next_i, counter_ptr
        ));
        self.emit_line(&format!("  br label %{}", loop_check_label));

        self.emit_line(&format!("{}:", end_label));

        Ok(())
    }

    /// 为 `UniquePtr<T>` / `ScopedPtr<T>` 注入 `__owned` 字段的析构与释放。
    ///
    /// 生成 IR（概念）：
    /// ```llvm
    /// %obj_i64 = load i64, i64* %__owned_field
    /// %obj     = inttoptr i64 %obj_i64 to i8*
    /// %is_null = icmp eq i8* %obj, null
    /// br i1 %is_null, label %drop.end, label %drop.body
    /// drop.body:
    ///   call void @T.__dtor(i8* %obj)   ; 若 T 有析构函数
    ///   call void @free(i8* %obj)
    ///   br label %drop.end
    /// drop.end:
    /// ```
    fn emit_owned_drop_injection(
        &mut self,
        class_name: &str,
        t_type: &Type,
    ) -> CayResult<()> {
        let owned_offset = self.get_field_offset(class_name, "__owned")?;

        let field_gep = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* %this, i64 {}",
            field_gep, owned_offset
        ));
        let field_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i64*",
            field_ptr, field_gep
        ));
        let owned_i64 = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i64, i64* {}",
            owned_i64, field_ptr
        ));
        let obj = self.new_temp();
        self.emit_line(&format!(
            "  {} = inttoptr i64 {} to i8*",
            obj, owned_i64
        ));

        let is_null = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i8* {}, null",
            is_null, obj
        ));
        let drop_body = self.new_label("drop.body");
        let drop_end = self.new_label("drop.end");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            is_null, drop_end, drop_body
        ));
        self.emit_line(&format!("{}:", drop_body));

        // 若 T 是带析构函数的类，调用其 __dtor。
        if let Some(t_class) = self.type_has_destructor(t_type) {
            let dtor_fn = self.mangle_itanium_method(&t_class, "D1", &[], false, true, false);
            self.emit_line(&format!(
                "  call void @{}(i8* {})",
                dtor_fn, obj
            ));
        }

        self.emit_line(&format!(
            "  call void @free(i8* {})",
            obj
        ));
        self.emit_line(&format!(
            "  br label %{}",
            drop_end
        ));
        self.emit_line(&format!("{}:", drop_end));

        Ok(())
    }

    /// 为 `Rc<T>` 注入引用计数递减与条件释放。
    ///
    /// 控制块布局：[i64 refcount, i64 weak_count, i64 object_ptr]。
    /// 当强引用计数归零时：调用 T.__dtor、free(obj)，并在 weak_count 为 0 时释放控制块。
    /// 当强引用计数仍大于 0 时：调用 `__cay_rc_check_cycle` 进行 best-effort 循环检测。
    fn emit_rc_drop_injection(
        &mut self,
        class_name: &str,
        t_type: &Type,
    ) -> CayResult<()> {
        let owned_offset = self.get_field_offset(class_name, "__owned")?;
        let rc_offset = self.get_field_offset(class_name, "__refcount_ptr")?;

        // 加载引用计数指针。
        let rc_field_gep = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* %this, i64 {}",
            rc_field_gep, rc_offset
        ));
        let rc_field_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i64*",
            rc_field_ptr, rc_field_gep
        ));
        let rc_i64 = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i64, i64* {}",
            rc_i64, rc_field_ptr
        ));
        let rc_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = inttoptr i64 {} to i64*",
            rc_ptr, rc_i64
        ));

        // 若控制块指针为空（对象已被 move/置空），直接跳过析构逻辑。
        let rc_null = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i64 {}, 0",
            rc_null, rc_i64
        ));
        let do_drop_label = self.new_label("rc.drop");
        let end_label = self.new_label("rc.end");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            rc_null, end_label, do_drop_label
        ));
        self.emit_line(&format!("{}:", do_drop_label));

        // 原子递减并获取旧值。
        let old_count = self.new_temp();
        self.emit_line(&format!(
            "  {} = atomicrmw sub i64* {}, i64 1 seq_cst",
            old_count, rc_ptr
        ));
        let should_free = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i64 {}, 1",
            should_free, old_count
        ));
        let free_label = self.new_label("rc.free");
        let check_cycle_label = self.new_label("rc.check");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            should_free, free_label, check_cycle_label
        ));
        self.emit_line(&format!("{}:", free_label));

        // 强引用归零：注销运行时跟踪（仅在 --detect-cycles 时）。
        let detect_enabled = self
            .platform_config
            .as_ref()
            .map(|c| c.detect_cycles)
            .unwrap_or(false);
        if detect_enabled {
            let rc_i8_for_unregister = self.new_temp();
            self.emit_line(&format!(
                "  {} = inttoptr i64 {} to i8*",
                rc_i8_for_unregister, rc_i64
            ));
            self.emit_line(&format!(
                "  call void @__cay_rc_unregister(i8* {})",
                rc_i8_for_unregister
            ));
        }

        // 将控制块指针转为 i8* 供后续释放与字段访问使用。
        let rc_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = inttoptr i64 {} to i8*",
            rc_i8, rc_i64
        ));

        // 加载托管对象指针（控制块 offset 16）。
        let obj_i64 = self.new_temp();
        self.emit_line(&format!(
            "  %obj_field_ptr = bitcast i8* {} to i64*",
            rc_i8
        ));
        let obj_field_gep2 = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i64, i64* %obj_field_ptr, i64 2",
            obj_field_gep2
        ));
        self.emit_line(&format!(
            "  {} = load i64, i64* {}",
            obj_i64, obj_field_gep2
        ));
        let obj = self.new_temp();
        self.emit_line(&format!(
            "  {} = inttoptr i64 {} to i8*",
            obj, obj_i64
        ));

        // 若 T 有析构函数，先调用。
        if let Some(t_class) = self.type_has_destructor(t_type) {
            let dtor_fn = self.mangle_itanium_method(&t_class, "D1", &[], false, true, false);
            self.emit_line(&format!(
                "  call void @{}(i8* {})",
                dtor_fn, obj
            ));
        }

        // 释放托管对象。
        self.emit_line(&format!(
            "  call void @free(i8* {})",
            obj
        ));

        // 读取 weak_count；为 0 时才释放控制块。
        let weak_count_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i64, i64* %obj_field_ptr, i64 1",
            weak_count_ptr
        ));
        let weak_count = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i64, i64* {}",
            weak_count, weak_count_ptr
        ));
        let weak_zero = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i64 {}, 0",
            weak_zero, weak_count
        ));
        let free_block_label = self.new_label("rc.free_block");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            weak_zero, free_block_label, end_label
        ));
        self.emit_line(&format!("{}:", free_block_label));
        self.emit_line(&format!(
            "  call void @free(i8* {})",
            rc_i8
        ));
        self.emit_line(&format!(
            "  br label %{}",
            end_label
        ));

        // 强引用未归零：best-effort 循环检测（仅在 --detect-cycles 时）。
        self.emit_line(&format!("{}:", check_cycle_label));
        if detect_enabled {
            let rc_i8_for_check = self.new_temp();
            self.emit_line(&format!(
                "  {} = inttoptr i64 {} to i8*",
                rc_i8_for_check, rc_i64
            ));
            self.emit_line(&format!(
                "  call void @__cay_rc_check_cycle(i8* {})",
                rc_i8_for_check
            ));
        }
        self.emit_line(&format!(
            "  br label %{}",
            end_label
        ));
        self.emit_line(&format!("{}:", end_label));

        Ok(())
    }

    /// 为 `WeakPtr<T>` 注入弱引用计数递减与条件释放。
    ///
    /// 控制块布局：[i64 refcount, i64 weak_count, i64 object_ptr]。
    /// 当弱引用计数归零且强引用计数为 0 时释放控制块。
    fn emit_weakptr_drop_injection(&mut self, class_name: &str) -> CayResult<()> {
        let rc_offset = self.get_field_offset(class_name, "__refcount_ptr")?;

        // 加载引用计数指针。
        let rc_field_gep = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* %this, i64 {}",
            rc_field_gep, rc_offset
        ));
        let rc_field_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i64*",
            rc_field_ptr, rc_field_gep
        ));
        let rc_i64 = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i64, i64* {}",
            rc_i64, rc_field_ptr
        ));

        // 若控制块指针为空，直接跳过。
        let rc_null = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i64 {}, 0",
            rc_null, rc_i64
        ));
        let do_drop_label = self.new_label("weak.drop");
        let end_label = self.new_label("weak.end");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            rc_null, end_label, do_drop_label
        ));
        self.emit_line(&format!("{}:", do_drop_label));

        let rc_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = inttoptr i64 {} to i64*",
            rc_ptr, rc_i64
        ));

        // 原子递减 weak_count 并获取旧值。
        let weak_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i64, i64* {}, i64 1",
            weak_ptr, rc_ptr
        ));
        let old_weak = self.new_temp();
        self.emit_line(&format!(
            "  {} = atomicrmw sub i64* {}, i64 1 seq_cst",
            old_weak, weak_ptr
        ));
        let weak_zero = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i64 {}, 1",
            weak_zero, old_weak
        ));
        let check_ref_label = self.new_label("weak.check_ref");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            weak_zero, check_ref_label, end_label
        ));
        self.emit_line(&format!("{}:", check_ref_label));

        // weak_count 刚刚归零：若 refcount 也为 0 则释放控制块。
        let refcount = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i64, i64* {}",
            refcount, rc_ptr
        ));
        let ref_zero = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i64 {}, 0",
            ref_zero, refcount
        ));
        let free_block_label = self.new_label("weak.free_block");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            ref_zero, free_block_label, end_label
        ));
        self.emit_line(&format!("{}:", free_block_label));
        let rc_i8 = self.new_temp();
        self.emit_line(&format!(
            "  {} = inttoptr i64 {} to i8*",
            rc_i8, rc_i64
        ));
        self.emit_line(&format!(
            "  call void @free(i8* {})",
            rc_i8
        ));
        self.emit_line(&format!(
            "  br label %{}",
            end_label
        ));
        self.emit_line(&format!("{}:", end_label));

        Ok(())
    }

    /// 为 `Optional<T>` 注入条件析构：当 `hasValue` 为真时调用 `value` 字段的 `__dtor`。
    ///
    /// 生成 IR（概念）：
    /// ```llvm
    /// %has = load i1, i1* %hasValue_field
    /// br i1 %has, label %opt.drop, label %opt.end
    /// opt.drop:
    ///   %val = load i8*, i8** %value_field
    ///   call void @T.__dtor(i8* %val)
    ///   br label %opt.end
    /// opt.end:
    /// ```
    fn emit_optional_drop_injection(
        &mut self,
        class_name: &str,
        t_type: &Type,
    ) -> CayResult<()> {
        // 仅当 T 是带析构函数的类时才需要注入。
        let Some(t_class) = self.type_has_destructor(t_type) else {
            return Ok(());
        };

        let has_value_offset = self.get_field_offset(class_name, "hasValue")?;
        let value_offset = self.get_field_offset(class_name, "value")?;

        let has_gep = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* %this, i64 {}",
            has_gep, has_value_offset
        ));
        let has_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i1*",
            has_ptr, has_gep
        ));
        let has_value = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i1, i1* {}",
            has_value, has_ptr
        ));

        let drop_label = self.new_label("opt.drop");
        let end_label = self.new_label("opt.end");
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            has_value, drop_label, end_label
        ));
        self.emit_line(&format!("{}:", drop_label));

        let value_gep = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* %this, i64 {}",
            value_gep, value_offset
        ));
        let value_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8**",
            value_ptr, value_gep
        ));
        let value = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8** {}",
            value, value_ptr
        ));

        let dtor_fn = self.mangle_itanium_method(&t_class, "D1", &[], false, true, false);
        self.emit_line(&format!(
            "  call void @{}(i8* {})",
            dtor_fn, value
        ));
        self.emit_line(&format!(
            "  br label %{}",
            end_label
        ));
        self.emit_line(&format!("{}:", end_label));

        Ok(())
    }

    /// 判断某个 Cavvy 类型是否是声明了析构函数的类，返回其可用于调用 __dtor 的类名。
    /// 对 Object 类型返回原类名；对 Generic 类型返回完整的 `Base<Args>` 字符串，
    /// 以便定位到正确的特化类析构函数。
    /// 注意：必须使用 display_name()（与 SpecializationInstance::specialized_name 一致），
    /// 不能用 Display/to_string()——后者把 String 渲染为 "string"，
    /// 会生成如 MutexGuard<string> 的错误析构函数名。
    pub(crate) fn type_has_destructor(
        &self,
        ty: &Type,
    ) -> Option<String> {
        use crate::types::Type;
        let base_name = match ty {
            Type::Object(name) => name.as_str(),
            Type::Generic(name, _) => name.as_str(),
            _ => return None,
        };
        self.type_registry
            .as_ref()
            .and_then(|r| r.get_class(base_name))
            .filter(|c| c.has_destructor)
            .map(|_| ty.display_name())
    }

    fn generate_static_initializer(
        &mut self,
        class_name: &str,
        block: &crate::ast::Block,
    ) -> CayResult<()> {
        let llvm_class = self.get_qualified_class_name(class_name);
        let fn_name = format!("{}.__static_init", llvm_class);
        self.current_function = fn_name.clone();
        // 从可能包含 :: 的限定名中提取简单名用于 current_class
        self.current_class = simple_class_name(class_name).to_string();
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

    /// 判断构造函数形参类型在单态化后是否匹配。
    /// 泛型参数（GenericParam）作为通配符可匹配任意具体类型，
    /// 其余类型按结构递归比较。
    fn constructor_param_type_matches(
        &self,
        base: &crate::types::Type,
        specialized: &crate::types::Type,
    ) -> bool {
        use crate::types::Type;
        match (base, specialized) {
            (Type::GenericParam(_), _) => true,
            (Type::Void, Type::Void)
            | (Type::Int32, Type::Int32)
            | (Type::Int64, Type::Int64)
            | (Type::Float32, Type::Float32)
            | (Type::Float64, Type::Float64)
            | (Type::Bool, Type::Bool)
            | (Type::String, Type::String)
            | (Type::Char, Type::Char) => true,
            (Type::Object(a), Type::Object(b)) => a == b,
            (Type::Array(a), Type::Array(b)) => {
                self.constructor_param_type_matches(a, b)
            }
            (Type::Pointer(a), Type::Pointer(b)) => {
                self.constructor_param_type_matches(a, b)
            }
            (Type::Generic(name_a, args_a), Type::Generic(name_b, args_b)) => {
                if name_a != name_b || args_a.len() != args_b.len() {
                    return false;
                }
                args_a
                    .iter()
                    .zip(args_b.iter())
                    .all(|(a, b)| self.constructor_param_type_matches(a, b))
            }
            _ => false,
        }
    }

    fn generate_constructor_name(
        &self,
        class_name: &str,
        ctor: &crate::ast::ConstructorDecl,
    ) -> String {
        // 使用 Itanium ABI C1 前缀
        let param_types: Vec<crate::types::Type> = if class_name.contains('<') {
            if let Some(ref registry) = self.type_registry {
                let base_class_name = if let Some(pos) = class_name.find('<') {
                    &class_name[..pos]
                } else {
                    class_name
                };
                if let Some(class_info) = registry.get_class(base_class_name) {
                    if !class_info.type_params.is_empty() {
                        // 先按签名精确匹配
                        let ctor_sigs: Vec<String> = ctor.params.iter().map(|p| self.type_to_signature(&p.param_type)).collect();
                        for ctor_info in &class_info.constructors {
                            if ctor_info.params.len() != ctor.params.len() { continue; }
                            let info_sigs: Vec<String> = ctor_info.params.iter().map(|p| self.type_to_signature(&p.param_type)).collect();
                            if ctor_sigs == info_sigs {
                                return self.mangle_itanium_method(
                                    class_name, "C1",
                                    &ctor.params.iter().map(|p| p.param_type.clone()).collect::<Vec<_>>(),
                                    true, false, false
                                );
                            }
                        }
                        // 精确匹配失败时，按泛型通配规则匹配（处理泛型参数被替换后的情况）
                        for ctor_info in &class_info.constructors {
                            if ctor_info.params.len() != ctor.params.len() { continue; }
                            let all_match = ctor_info
                                .params
                                .iter()
                                .zip(ctor.params.iter())
                                .all(|(info_param, ctor_param)| {
                                    self.constructor_param_type_matches(
                                        &info_param.param_type,
                                        &ctor_param.param_type,
                                    )
                                });
                            if all_match {
                                return self.mangle_itanium_method(
                                    class_name, "C1",
                                    &ctor.params.iter().map(|p| p.param_type.clone()).collect::<Vec<_>>(),
                                    true, false, false
                                );
                            }
                        }
                    }
                }
            }
            ctor.params.iter().map(|p| p.param_type.clone()).collect()
        } else {
            ctor.params.iter().map(|p| p.param_type.clone()).collect()
        };
        self.mangle_itanium_method(class_name, "C1", &param_types, true, false, false)
    }

    /// 生成构造函数调用名称（基于参数类型列表）
    pub fn generate_constructor_call_name_with_types(
        &self,
        class_name: &str,
        param_types: &[crate::types::Type],
    ) -> String {
        self.mangle_itanium_method(class_name, "C1", param_types, true, false, false)
    }

    /// 生成构造函数调用名称（基于参数数量 - 仅用于简单情况，参数类型全部假定为 int）
    pub fn generate_constructor_call_name(&self, class_name: &str, arg_count: usize) -> String {
        let param_types: Vec<crate::types::Type> =
            (0..arg_count).map(|_| crate::types::Type::Int32).collect();
        self.mangle_itanium_method(class_name, "C1", &param_types, true, false, false)
    }

    /// 推断表达式类型（用于构造函数调用）
    fn infer_expr_type_for_ctor(&self, expr: &crate::ast::Expr) -> String {
        use crate::ast::*;

        match expr {
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::Int32(_) => "i".to_string(),
                LiteralValue::Int64(_) => "l".to_string(),
                LiteralValue::Float32(_) => "f".to_string(),
                LiteralValue::Float64(_) => "d".to_string(),
                LiteralValue::Bool(_) => "b".to_string(),
                LiteralValue::Char(_) => "c".to_string(),
                LiteralValue::String(_) => "s".to_string(),
                LiteralValue::Null => "o".to_string(),
            },
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
            Expr::Binary(binary) => self.infer_expr_type_for_ctor(&binary.left),
            Expr::Unary(unary) => self.infer_expr_type_for_ctor(&unary.operand),
            Expr::Cast(cast) => self.type_to_signature(&cast.target_type),
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
    fn infer_member_type(
        &self,
        member: &crate::ast::MemberAccessExpr,
    ) -> Option<crate::types::Type> {
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
                let class_layout = self
                    .class_layouts
                    .get(&class_name)
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
            Expr::Identifier(ident) => self.var_cay_types.get(&ident.name).cloned(),
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::Int32(_) => Some(crate::types::Type::Int32),
                LiteralValue::Int64(_) => Some(crate::types::Type::Int64),
                LiteralValue::Float32(_) => Some(crate::types::Type::Float32),
                LiteralValue::Float64(_) => Some(crate::types::Type::Float64),
                LiteralValue::Bool(_) => Some(crate::types::Type::Bool),
                LiteralValue::Char(_) => Some(crate::types::Type::Char),
                LiteralValue::String(_) => Some(crate::types::Type::String),
                LiteralValue::Null => None,
            },
            _ => None,
        }
    }

    /// 生成顶层函数
    fn generate_top_level_function(
        &mut self,
        func: &crate::ast::TopLevelFunction,
    ) -> CayResult<()> {
        let fn_name = self.generate_top_level_function_name(&func.name);
        self.current_function = fn_name.clone();
        self.current_class = String::new(); // 顶层函数没有类
        self.current_return_type = self.type_to_llvm(&func.return_type);
        self.current_function_cay_return_type = Some(func.return_type.clone());

        self.temp_counter = 0;
        self.var_types.clear();
        self.scope_manager.reset();
        self.loop_stack.clear();

        let ret_type = self.current_return_type.clone();
        let params: Vec<String> = func
            .params
            .iter()
            .map(|p| format!("{} %{}.param", self.type_to_llvm(&p.param_type), p.name))
            .collect();

        self.emit_line(&format!(
            "define {} @{}({}) {{",
            ret_type,
            fn_name,
            params.join(", ")
        ));
        self.indent += 1;

        self.emit_line("entry:");

        for param in &func.params {
            let param_type = self.type_to_llvm(&param.param_type);
            let llvm_name =
                self.scope_manager
                    .declare_var_with_flag(&param.name, &param_type, true);
            self.emit_line(&format!("  %{} = alloca {}", llvm_name, param_type));
            // 值类型语义：struct 参数按值传递，入口拷贝到本地独立存储
            if let Some(struct_name) = self.extract_struct_name_from_ptr_type(&param_type) {
                let llvm_struct_type = format!("%struct.{}", struct_name);
                let local_copy = self.new_temp();
                self.emit_line(&format!("  {} = alloca {}", local_copy, llvm_struct_type));
                let src_param = format!("%{}.param", param.name);
                self.emit_struct_memcpy(&local_copy, &src_param, &struct_name);
                self.emit_line(&format!(
                    "  store {} {}, {}* %{}",
                    param_type, local_copy, param_type, llvm_name
                ));
            } else {
                self.emit_line(&format!(
                    "  store {} %{}.param, {}* %{}",
                    param_type, param.name, param_type, llvm_name
                ));
            }
            self.var_types.insert(param.name.clone(), param_type);
            // 同时保存Cavvy类型用于函数指针识别
            self.var_cay_types
                .insert(param.name.clone(), param.param_type.clone());
        }

        self.generate_block(&func.body)?;

        // ROADMAP 5.3.x 自动 RAII：函数体末尾默认返回前，析构所有尚未退出的
        // 作用域（显式 return 已在 generate_return_statement 中处理）。
        self.emit_all_scope_dtors();

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
    fn generate_extern_declaration(
        &mut self,
        extern_decl: &crate::ast::ExternDecl,
    ) -> CayResult<()> {
        for func in &extern_decl.functions {
            self.generate_extern_function(extern_decl.calling_convention, func)?;
        }
        Ok(())
    }

    /// 生成单个 extern 函数声明
    fn generate_extern_function(
        &mut self,
        calling_conv: crate::ast::CallingConvention,
        func: &crate::ast::ExternFunction,
    ) -> CayResult<()> {
        let ret_type = self.type_to_llvm(&func.return_type);

        // 构建参数列表，支持可变参数
        let params: Vec<String> = func
            .params
            .iter()
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
                format!(
                    "declare {} @{}({})\n",
                    ret_type,
                    func.name,
                    params.join(", ")
                )
            } else {
                format!(
                    "declare {} @{}({}) {}\n",
                    ret_type,
                    func.name,
                    params.join(", "),
                    cc_attr
                )
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
    pub(crate) fn calling_convention_to_llvm_attr(
        &self,
        cc: crate::ast::CallingConvention,
    ) -> String {
        match cc {
            // Windows x64 平台使用 win64 调用约定
            crate::ast::CallingConvention::Cdecl => {
                if self.is_windows_target() {
                    "#4".to_string() // win64
                } else {
                    "#0".to_string() // cdecl
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
    fn emit_test_main(&mut self) -> CayResult<()> {
        let test_count = self.test_methods.len();

        self.output
            .push_str("; ============================================================\n");
        self.output
            .push_str("; Test entry point (generated by --test mode)\n");
        self.output
            .push_str("; ============================================================\n\n");

        // 声明 printf 函数（根据用户 extern 声明动态适配返回类型）
        let printf_ret = self.get_extern_ret_type("printf", "i32");
        if printf_ret == "void" {
            self.output
                .push_str("declare void @printf(i8*, ...) #0\n\n");
        } else {
            self.output.push_str("declare i32 @printf(i8*, ...) #0\n\n");
        }

        // 生成测试入口函数
        self.output.push_str("define i32 @__cavvy_test_main() {\n");
        self.output.push_str("entry:\n");
        self.output
            .push_str(&format!("  ; {} test method(s)\n", test_count));

        // 分配 passed 和 failed 计数器
        self.output.push_str("  %passed = alloca i32\n");
        self.output.push_str("  %failed = alloca i32\n");
        self.output.push_str("  store i32 0, i32* %passed\n");
        self.output.push_str("  store i32 0, i32* %failed\n\n");

        // 打印 header
        self.output.push_str("  ; Print header\n");
        let header_len = self.escape_test_str("running %d tests...\\n").len() + 1;
        self.output.push_str(&format!(
            "  call {} (i8*, ...) @printf(i8* getelementptr inbounds ([{} x i8], [{} x i8]* @str_test_header, i64 0, i64 0), i32 {})\n",
            printf_ret, header_len, header_len, test_count
        ));

        // 为每个 @Test 方法生成调用
        for (i, (class_name, method_name)) in self.test_methods.iter().enumerate() {
            let full_name = format!("{}::{}", class_name, method_name);
            let method_fn_name = format!("{}.{}", class_name, method_name);

            let ok_msg = format!("test {} ... ok\\n\\00", full_name);
            let ok_len = ok_msg.len();
            let ok_global = format!("@str_test_ok_{}", i);

            self.output.push_str(&format!("  ; Test: {}\n", full_name));
            self.output
                .push_str(&format!("  call void @{}()\n", method_fn_name));

            // 打印 "ok"
            self.output.push_str(&format!(
                "  call {} (i8*, ...) @printf(i8* getelementptr inbounds ([{} x i8], [{} x i8]* {}, i64 0, i64 0))\n",
                printf_ret, ok_len, ok_len, ok_global
            ));

            // 增加 passed 计数
            self.output.push_str("  %p = load i32, i32* %passed\n");
            self.output.push_str("  %np = add i32 %p, 1\n");
            self.output.push_str("  store i32 %np, i32* %passed\n\n");
        }

        // 打印汇总
        self.output.push_str("  ; Print summary\n");
        self.output
            .push_str("  %final_passed = load i32, i32* %passed\n");
        self.output
            .push_str("  %final_failed = load i32, i32* %failed\n");

        let summary_len = self
            .escape_test_str("\\ntest result: %d passed; %d failed\\n")
            .len()
            + 1;
        self.output.push_str(&format!(
            "  call {} (i8*, ...) @printf(i8* getelementptr inbounds ([{} x i8], [{} x i8]* @str_test_summary, i64 0, i64 0), i32 %final_passed, i32 %final_failed)\n",
            printf_ret, summary_len, summary_len
        ));

        // 返回 0（成功）或 1（有失败）
        self.output
            .push_str("  %has_failures = icmp sgt i32 %final_failed, 0\n");
        self.output
            .push_str("  %exit_code = select i1 %has_failures, i32 1, i32 0\n");
        self.output.push_str("  ret i32 %exit_code\n");
        self.output.push_str("}\n\n");

        // 生成字符串常量
        self.output.push_str("; Test string constants\n");

        let header_str = self.escape_test_str("running %d tests...\\n");
        self.output.push_str(&format!(
            "@str_test_header = private constant [{} x i8] c\"{}\"\n",
            header_str.len() + 1,
            header_str
        ));

        for (i, (class_name, method_name)) in self.test_methods.iter().enumerate() {
            let full_name = format!("{}::{}", class_name, method_name);
            let ok_msg = self.escape_test_str(&format!("test {} ... ok\\n", full_name));
            self.output.push_str(&format!(
                "@str_test_ok_{} = private constant [{} x i8] c\"{}\\00\"\n",
                i,
                ok_msg.len() + 1,
                ok_msg
            ));
        }

        let summary_str = self.escape_test_str("\\ntest result: %d passed; %d failed\\n");
        self.output.push_str(&format!(
            "@str_test_summary = private constant [{} x i8] c\"{}\\00\"\n",
            summary_str.len() + 1,
            summary_str
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
