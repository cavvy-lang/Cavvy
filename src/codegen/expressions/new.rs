//! new 表达式代码生成
//!
//! 处理对象创建和数组创建。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::cayResult;

/// 将类型中的泛型参数（`GenericParam` 或裸 `Object("T")`）替换为具体类型实参。
/// 用于将构造函数形参在 new 单态化时解析为具体类型。
fn substitute_generic_param(
    ty: &crate::types::Type,
    type_args: &[crate::types::Type],
    type_params: &[String],
) -> crate::types::Type {
    use crate::types::Type;
    let name = match ty {
        Type::GenericParam(n) => Some(n),
        Type::Object(n) => Some(n),
        _ => None,
    };
    if let Some(name) = name {
        if let Some(idx) = type_params.iter().position(|p| p == name) {
            if let Some(arg) = type_args.get(idx) {
                return arg.clone();
            }
        }
    }
    ty.clone()
}

/// 从具体类型中提取用于查找的类名字符串。
/// 用于单态化时将泛型类型参数（如 `A`）解析到的具体类型（如 `GlobalAlloc`）
/// 还原为可用于构造函数查找的类名。
fn generic_arg_class_name(ty: &crate::types::Type) -> Option<String> {
    use crate::types::Type;
    match ty {
        Type::Object(n) | Type::Struct(n) => Some(n.clone()),
        Type::Generic(base, _) => {
            Some(base.split('<').next().unwrap_or(base).trim_end().to_string())
        }
        _ => None,
    }
}

/// 简单解析类型实参字符串（用于从 new 表达式类名中提取泛型实参）。
/// 与 SpecializationCollector 的 parse_type_str 语义对齐，支持基本类型别名。
fn parse_type_arg_from_str(s: &str) -> crate::types::Type {
    use crate::types::Type;
    if s.ends_with("[]") {
        return Type::Array(Box::new(parse_type_arg_from_str(&s[..s.len() - 2],
        )));
    }
    match s {
        "int" => Type::Int32,
        "long" => Type::Int64,
        "float" => Type::Float32,
        "double" => Type::Float64,
        "bool" | "boolean" => Type::Bool,
        "string" | "String" => Type::String,
        "char" => Type::Char,
        _ => Type::Object(s.to_string()),
    }
}

impl IRGenerator {
    /// 生成 new 表达式代码
    ///
    /// # Arguments
    /// * `new_expr` - new 表达式
    pub fn generate_new_expression(&mut self, new_expr: &NewExpr) -> cayResult<String> {
        // 消费期望目标类型（单次使用，避免泄漏到参数中的嵌套 new 表达式）
        let expected_type = self.pending_new_expected_type.take();

        // 提取基础类名（不含泛型参数）用于类型注册表查找
        let mut base_class_name = if let Some(pos) = new_expr.class_name.find('<') {
            new_expr.class_name[..pos].to_string()
        } else {
            new_expr.class_name.clone()
        };

        // 单态化：若基础类名本身是当前上下文的泛型类型参数（如 HashMap<K,V,A>
        // 方法体内的 `new A()`），先经 generic_type_args 解析为具体类型名。
        // 否则会生成对未定义构造函数 `@_ZN1AE.__ctor` 的调用。
        let mut class_name = new_expr.class_name.clone();
        if let Some(concrete) = self.generic_type_args.get(&base_class_name).cloned() {
            if let Some(concrete_name) = generic_arg_class_name(&concrete) {
                base_class_name = concrete_name.clone();
                class_name = concrete_name;
            }
        }

        // 检查是否是 struct 类型
        let is_struct = self
            .type_registry
            .as_ref()
            .and_then(|r| r.get_struct(&base_class_name))
            .is_some();

        if is_struct {
            return self.generate_struct_new_expression(new_expr);
        }

        // 如果类是命名空间限定的，解析到TypeRegistry中获取规范名称
        let registry_name = if base_class_name.contains("::") {
            if let Some(ref registry) = self.type_registry {
                if let Some(class_info) = registry.get_class(&base_class_name) {
                    class_info.name.clone()
                } else {
                    base_class_name.clone()
                }
            } else {
                base_class_name.clone()
            }
        } else {
            base_class_name.clone()
        };

        // 单态化：解析该 new 表达式的具体类型参数。
        // 优先使用 new 表达式自带的显式类型参数（如 `new Box<int>()`），
        // 否则回退到期望目标类型（如变量声明 `Box<int> b = new Box(42)`）。
        let concrete_type_args: Option<Vec<crate::types::Type>> = {
            let class_type_params = self
                .type_registry
                .as_ref()
                .and_then(|r| r.get_class(&base_class_name))
                .map(|c| c.type_params.clone())
                .unwrap_or_default();
            if class_type_params.is_empty() {
                None
            } else {
                // 候选类型实参：优先期望目标类型（如变量声明 Optional<int>），
                // 其次退回类型参数在当前 generic_type_args 中的映射（特化方法体内
                // 声明类型仍写作 Optional<T> 的情况）。
                let candidate: Option<Vec<crate::types::Type>> =
                    if let Some(crate::types::Type::Generic(exp_base, exp_args)) = &expected_type {
                        let exp_base_simple = exp_base
                            .split('<')
                            .next()
                            .unwrap_or(exp_base)
                            .rsplit("::")
                            .next()
                            .unwrap_or(exp_base);
                        if exp_base_simple == base_class_name && !exp_args.is_empty() {
                            Some(exp_args.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                    .or_else(|| {
                        // 优先从 new 表达式显式书写的类型实参解析（如
                        // `new HashMap<T, bool, A>()`）。这在泛型类体内比按目标类
                        // 类型参数名查找更通用，因为实参可能是外层类类型参数
                        // （如 HashSet<T,A> 中的 T/A），与目标类参数名（K,V,A）不同。
                        if let Some(lt_pos) = class_name.find('<') {
                            let gt_pos = class_name.rfind('>').unwrap_or(class_name.len());
                            let args_str = &class_name[lt_pos + 1..gt_pos];
                            let parsed: Vec<crate::types::Type> =
                                crate::codegen::specialization::split_top_level_type_args(args_str)
                                    .iter()
                                    .map(|s| parse_type_arg_from_str(s.trim()))
                                    .collect();
                            let resolved: Vec<_> = parsed
                                .iter()
                                .map(|t| self.resolve_type_arg_concrete(t))
                                .collect();
                            if resolved.iter().all(|t| self.type_arg_is_concrete(t))
                                && resolved.len() == class_type_params.len()
                            {
                                return Some(resolved);
                            }
                        }

                        // 原有回退：按目标类类型参数名从 generic_type_args 查找
                        let m: Vec<crate::types::Type> = class_type_params
                            .iter()
                            .filter_map(|p| self.generic_type_args.get(&p.name).cloned())
                            .collect();
                        if m.len() == class_type_params.len() {
                            Some(m)
                        } else {
                            None
                        }
                    });
                // 将候选实参经 generic_type_args 解析为具体类型（如 Object("T") -> int），
                // 仅当全部具体时才单态化，否则退回类型擦除基础模板。
                candidate
                    .map(|args| {
                        args.iter()
                            .map(|t| self.resolve_type_arg_concrete(t))
                            .collect::<Vec<_>>()
                    })
                    .filter(|args| args.iter().all(|t| self.type_arg_is_concrete(t)))
            }
        };

        // 计算用于代码生成的规范类名：若已解析出具体类型参数，则使用特化名
        // （如 Box<int>），使构造函数名、vtable 与已生成的特化版本保持一致。
        let canonical_name = if let Some(ref args) = concrete_type_args {
            let args_str: Vec<String> = args.iter().map(|t| format!("{}", t)).collect();
            format!("{}<{}>", base_class_name, args_str.join(", "))
        } else {
            class_name.clone()
        };
        let type_id_value = self.get_type_id_value(&registry_name).unwrap_or(0);

        // 获取类布局信息，确定对象大小（使用基础类名查找）
        let obj_size = self
            .get_class_layout(&registry_name)
            .map(|layout| layout.total_size as i64)
            .unwrap_or(8i64); // 默认最小大小

        let calloc_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = call i8* @calloc(i64 1, i64 {})",
            calloc_temp, obj_size
        ));

        // calloc 失败保护：返回 null 而非崩溃（跳过构造函数调用）
        let is_null_new = self.new_temp();
        self.emit_line(&format!(
            "  {} = icmp eq i8* {}, null",
            is_null_new, calloc_temp
        ));
        let new_ok = self.new_label("new.ok");
        let new_fail = self.new_label("new.fail");
        let new_merge = self.new_label("new.merge");

        // 结果槽（alloca 必须在 br 之前）
        let new_result_slot = self.new_temp();
        self.emit_line(&format!("  {} = alloca i8*", new_result_slot));

        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            is_null_new, new_fail, new_ok
        ));

        self.emit_line(&format!("\n{}:", new_fail));
        self.emit_line(&format!("  store i8* null, i8** {}", new_result_slot));
        self.emit_line(&format!("  br label %{}", new_merge));

        self.emit_line(&format!("\n{}:", new_ok));

        let type_id_ptr = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i32*",
            type_id_ptr, calloc_temp
        ));
        self.emit_line(&format!(
            "  store i32 {}, i32* {}",
            type_id_value, type_id_ptr
        ));

        // 存储 vtable 指针到 offset 8（type_id 之后）
        // 使用完整特化类名生成独立 vtable（如 Box<int>）
        let llvm_class = self.get_qualified_class_name(&canonical_name);
        let vtable_name = format!("{}.vtable", llvm_class);
        let vtable_ptr_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            vtable_ptr_temp, calloc_temp
        ));
        let vtable_global_temp = self.new_temp();
        let vtable_size = self
            .type_registry
            .as_ref()
            .and_then(|r| r.get_class(&registry_name))
            .and_then(|c| c.vtable_layout.as_ref())
            .map(|v| v.size)
            .unwrap_or(0);
        if vtable_size > 0 {
            self.emit_line(&format!(
                "  {} = bitcast [{} x i8*]* @{} to i8*",
                vtable_global_temp, vtable_size, vtable_name
            ));
            self.emit_line(&format!(
                "  store i8* {}, i8* {}",
                vtable_global_temp, vtable_ptr_temp
            ));
        } else {
            self.emit_line(&format!("  store i8* null, i8* {}", vtable_ptr_temp));
        }

        // 调用构造函数
        let fallback_types: Vec<String> = new_expr
            .args
            .iter()
            .map(|arg| self.infer_argument_type(arg))
            .collect();
        let param_types = self.get_constructor_param_signatures(
            &registry_name,
            new_expr.args.len(),
            &fallback_types,
        );

        let ctor_info_opt = self
            .type_registry
            .as_ref()
            .and_then(|r| r.get_class(&registry_name))
            .and_then(|c| {
                c.constructors
                    .iter()
                    .find(|ctor| {
                        let sigs: Vec<String> = ctor
                            .params
                            .iter()
                            .map(|p| self.type_to_signature(&p.param_type))
                            .collect();
                        sigs == param_types
                    })
                    .cloned()
            });

        // 类的类型参数名列表（用于将构造函数参数中的泛型参数替换为具体类型）
        let class_type_params: Vec<String> = self
            .type_registry
            .as_ref()
            .and_then(|r| r.get_class(&registry_name))
            .map(|c| c.type_params.iter().map(|p| p.name.clone()).collect())
            .unwrap_or_default();

        let mut arg_values = Vec::new();
        for (idx, arg) in new_expr.args.iter().enumerate() {
            let arg_val = self.generate_expression(arg)?;
            let is_generic_param = ctor_info_opt
                .as_ref()
                .and_then(|ctor| ctor.params.get(idx))
                .map(|p| matches!(p.param_type, crate::types::Type::GenericParam(_)))
                .unwrap_or(false);
            // 已单态化（已知具体类型参数）时，构造函数形参已是具体类型，
            // 直接按具体类型传参，无需装箱为 i8*。
            let concrete_param = if is_generic_param {
                concrete_type_args.as_ref().and_then(|type_args| {
                    ctor_info_opt
                        .as_ref()
                        .and_then(|ctor| ctor.params.get(idx))
                        .map(|p| substitute_generic_param(&p.param_type, type_args, &class_type_params))
                })
            } else {
                None
            };
            if let Some(concrete_ty) = concrete_param {
                let concrete_llvm = self.type_to_llvm(&concrete_ty);
                let (arg_ty, arg_v) = self.parse_typed_value(&arg_val);
                arg_values.push(self.convert_arg_type(&arg_ty, &arg_v, &concrete_llvm));
            } else if is_generic_param {
                let inferred_type = self.infer_argument_type(arg);
                let boxed_val = self.box_value_for_generic(&arg_val, &inferred_type)?;
                arg_values.push(boxed_val);
            } else {
                arg_values.push(arg_val);
            }
        }

        let ctor_name =
            self.generate_constructor_call_name_with_types(&canonical_name, &param_types);

        let mut arg_strs = vec![format!("i8* {}", calloc_temp)];
        arg_strs.extend(arg_values);
        self.emit_line(&format!(
            "  call void @{}({})",
            ctor_name,
            arg_strs.join(", ")
        ));

        let cast_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8*",
            cast_temp, calloc_temp
        ));
        self.emit_line(&format!(
            "  store i8* {}, i8** {}",
            cast_temp, new_result_slot
        ));
        self.emit_line(&format!("  br label %{}", new_merge));

        self.emit_line(&format!("\n{}:", new_merge));
        let result_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = load i8*, i8** {}",
            result_temp, new_result_slot
        ));
        Ok(format!("i8* {}", result_temp))
    }

    /// 生成 struct 的 new 表达式（栈分配，值类型）
    ///
    /// struct 是值类型：
    /// 1. 使用 alloca 在栈上分配内存
    /// 2. 不需要 type_id 和 vtable
    /// 3. 调用构造函数初始化字段
    /// 4. 返回 %struct.Name* 指针
    fn generate_struct_new_expression(&mut self, new_expr: &NewExpr) -> cayResult<String> {
        let struct_name = &new_expr.class_name;
        let base_name = if let Some(pos) = struct_name.find('<') {
            struct_name[..pos].to_string()
        } else {
            struct_name.clone()
        };

        // 获取 struct 布局信息
        let struct_layout = self.get_struct_layout(&base_name);
        let llvm_struct_type = format!("%struct.{}", base_name);

        // 栈分配 struct
        let alloca_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = alloca {}",
            alloca_temp, llvm_struct_type
        ));

        // 推断构造函数参数
        let fallback_types: Vec<String> = new_expr
            .args
            .iter()
            .map(|arg| self.infer_argument_type(arg))
            .collect();
        let param_types = self.get_constructor_param_signatures(
            &base_name,
            new_expr.args.len(),
            &fallback_types,
        );

        // 生成参数值
        let mut arg_values = Vec::new();
        for arg in &new_expr.args {
            let arg_val = self.generate_expression(arg)?;
            arg_values.push(arg_val);
        }

        // 生成构造函数名
        let ctor_name =
            self.generate_constructor_call_name_with_types(struct_name, &param_types);

        // 调用构造函数（传 struct 指针作为 this）
        if !param_types.is_empty() {
            let mut arg_strs = vec![format!("{}* {}", llvm_struct_type, alloca_temp)];
            arg_strs.extend(arg_values);
            self.emit_line(&format!(
                "  call void @{}({})",
                ctor_name,
                arg_strs.join(", ")
            ));
        }
        // 无参 struct 不调用构造函数（栈分配已清零，字段为默认值）

        // 返回 struct 指针
        Ok(format!("{}* {}", llvm_struct_type, alloca_temp))
    }

    /// 推断参数类型（返回类型签名）
    fn infer_argument_type(&self, expr: &Expr) -> String {
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
                if let Some(cay_type) = self.infer_member_access_type(member) {
                    self.type_to_signature(&cay_type)
                } else {
                    "i".to_string() // 默认int
                }
            }
            Expr::Binary(binary) => {
                // 二元表达式的类型通常是左操作数的类型
                self.infer_argument_type(&binary.left)
            }
            Expr::Unary(unary) => self.infer_argument_type(&unary.operand),
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

    /// 推断成员访问表达式的类型
    fn infer_member_access_type(&self, member: &MemberAccessExpr) -> Option<crate::types::Type> {
        use crate::types::{FunctionType, Type};

        // 获取对象类型或类名
        let obj_type = self.infer_expr_type_for_member(&member.object)?;

        match obj_type {
            Type::Object(class_name) => {
                // 获取命名空间限定名（用于在 class_layouts 中查找）
                let qualified_name = {
                    let ns = self.get_class_namespace(&class_name);
                    if !ns.is_empty() {
                        format!("{}::{}", ns.join("::"), class_name)
                    } else {
                        class_name.clone()
                    }
                };
                // 首先查找类字段（先试简单名，再试命名空间限定名）
                let class_layout = self
                    .class_layouts
                    .get(&class_name)
                    .or_else(|| self.class_layouts.get(&qualified_name));
                if let Some(class_info) = class_layout {
                    if let Some(field) = class_info.fields.get(&member.member) {
                        return Some(field.field_type.clone());
                    }
                }
                // 然后查找静态方法（如 MathUtils.multiply）
                if let Some(ref registry) = self.type_registry {
                    let registry_class = registry
                        .get_class(&class_name)
                        .or_else(|| registry.get_class(&qualified_name));
                    if let Some(class_info) = registry_class {
                        // 查找静态方法
                        for (method_name, methods) in &class_info.methods {
                            if method_name == &member.member {
                                for method in methods {
                                    if method.is_static {
                                        // 返回函数指针类型
                                        let param_types = method
                                            .params
                                            .iter()
                                            .map(|p| p.param_type.clone())
                                            .collect();
                                        return Some(Type::Function(Box::new(FunctionType {
                                            params: param_types,
                                            return_type: Box::new(method.return_type.clone()),
                                            is_static: true,
                                            is_closure: false,
                                        })));
                                    }
                                }
                            }
                        }
                    }
                }
                None
            }
            Type::Array(_) if member.member == "length" => {
                // 数组的 length 属性返回 int
                Some(Type::Int32)
            }
            _ => None,
        }
    }

    /// 推断表达式类型（用于成员访问类型推断）
    fn infer_expr_type_for_member(&self, expr: &Expr) -> Option<crate::types::Type> {
        use crate::types::Type;

        match expr {
            Expr::Identifier(ident) => {
                // 首先检查是否是变量
                if let Some(var_type) = self.var_cay_types.get(&ident.name) {
                    return Some(var_type.clone());
                }
                // 特殊处理 "this"
                if ident.name == "this" {
                    return Some(Type::Object(self.current_class.clone()));
                }
                // 检查是否是类名（静态方法调用如 MathUtils.multiply）
                if let Some(ref registry) = self.type_registry {
                    if registry.class_exists(&ident.name) {
                        return Some(Type::Object(ident.name.clone()));
                    }
                }
                None
            }
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::Int32(_) => Some(Type::Int32),
                LiteralValue::Int64(_) => Some(Type::Int64),
                LiteralValue::Float32(_) => Some(Type::Float32),
                LiteralValue::Float64(_) => Some(Type::Float64),
                LiteralValue::Bool(_) => Some(Type::Bool),
                LiteralValue::Char(_) => Some(Type::Char),
                LiteralValue::String(_) => Some(Type::String),
                LiteralValue::Null => None,
            },
            _ => None,
        }
    }

    /// 将值装箱为 i8* 以传递给泛型参数
    ///
    /// 使用带类型标记的装箱格式：{ i8 type_tag, i8[7] padding, i64 data }
    /// type_tag: 0=int, 1=long, 2=float, 3=double, 4=bool, 5=char, 6=object
    ///
    /// # Arguments
    /// * `value` - 原始值（格式如 "i32 42" 或 "i8* %t1"）
    /// * `type_sig` - 类型签名（如 "i", "l", "f", "d", "b", "o" 等）
    ///
    /// # Returns
    /// 装箱后的值（格式为 "i8* %tN"）
    fn box_value_for_generic(&mut self, value: &str, type_sig: &str) -> cayResult<String> {
        // 解析值字符串，提取类型和值
        let parts: Vec<&str> = value.split_whitespace().collect();
        if parts.len() < 2 {
            return Ok(value.to_string());
        }

        let llvm_type = parts[0];
        let llvm_val = parts[1];

        // 如果已经是 i8*，直接返回（假设已经是装箱格式或对象指针）
        if llvm_type == "i8*" {
            return Ok(value.to_string());
        }

        // 将值装箱为 i8*：通过将值转换为 i64，再使用 inttoptr 转为 i8*
        // 这种方式与 convert_arg_type 中的装箱逻辑保持一致
        let boxed = self.new_temp();

        match type_sig {
            "i" => {
                // i32 -> i64 (符号扩展) -> i8*
                let ext_val = self.new_temp();
                self.emit_line(&format!("  {} = sext i32 {} to i64", ext_val, llvm_val));
                self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", boxed, ext_val));
            }
            "l" => {
                // i64 -> i8*
                self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", boxed, llvm_val));
            }
            "f" => {
                // float -> double -> i64 -> i8*
                let ext_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = fpext float {} to double",
                    ext_val, llvm_val
                ));
                let bitcast_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast double {} to i64",
                    bitcast_val, ext_val
                ));
                self.emit_line(&format!(
                    "  {} = inttoptr i64 {} to i8*",
                    boxed, bitcast_val
                ));
            }
            "d" => {
                // double -> i64 -> i8*
                let bitcast_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast double {} to i64",
                    bitcast_val, llvm_val
                ));
                self.emit_line(&format!(
                    "  {} = inttoptr i64 {} to i8*",
                    boxed, bitcast_val
                ));
            }
            "b" => {
                // i1 -> i8 -> i64 -> i8*
                let ext_i8 = self.new_temp();
                self.emit_line(&format!("  {} = zext i1 {} to i8", ext_i8, llvm_val));
                let ext_i64 = self.new_temp();
                self.emit_line(&format!("  {} = zext i8 {} to i64", ext_i64, ext_i8));
                self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", boxed, ext_i64));
            }
            "c" => {
                // i8 -> i64 -> i8*
                let ext_val = self.new_temp();
                self.emit_line(&format!("  {} = sext i8 {} to i64", ext_val, llvm_val));
                self.emit_line(&format!("  {} = inttoptr i64 {} to i8*", boxed, ext_val));
            }
            _ => {
                // 其他类型（对象指针），直接使用
                return Ok(value.to_string());
            }
        }

        Ok(format!("i8* {}", boxed))
    }

    /// 从装箱值中解箱为具体类型
    ///
    /// # Arguments
    /// * `boxed_val` - 装箱值（格式如 "i8* %t1"）
    /// * `target_type_sig` - 目标类型签名
    ///
    /// # Returns
    /// 解箱后的值（格式如 "i32 %tN"）
    fn unbox_value_for_generic(
        &mut self,
        boxed_val: &str,
        target_type_sig: &str,
    ) -> cayResult<String> {
        // 解析装箱值
        let parts: Vec<&str> = boxed_val.split_whitespace().collect();
        if parts.len() < 2 {
            return Ok(boxed_val.to_string());
        }

        let llvm_type = parts[0];
        let box_ptr = parts[1];

        // 如果已经是具体类型，直接返回
        if llvm_type != "i8*" {
            return Ok(boxed_val.to_string());
        }

        // 新的装箱格式：i8* 是通过 inttoptr 从 i64 转换而来的
        // 解箱：i8* -> i64 (ptrtoint) -> 具体类型
        let int_val = self.new_temp();
        self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, box_ptr));

        // 根据目标类型转换
        let result = match target_type_sig {
            "i" => {
                // i64 -> i32
                let trunc_val = self.new_temp();
                self.emit_line(&format!("  {} = trunc i64 {} to i32", trunc_val, int_val));
                format!("i32 {}", trunc_val)
            }
            "l" => {
                format!("i64 {}", int_val)
            }
            "f" => {
                // i64 -> double -> float
                let double_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i64 {} to double",
                    double_val, int_val
                ));
                let float_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = fptrunc double {} to float",
                    float_val, double_val
                ));
                format!("float {}", float_val)
            }
            "d" => {
                // i64 -> double
                let bitcast_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i64 {} to double",
                    bitcast_val, int_val
                ));
                format!("double {}", bitcast_val)
            }
            "b" => {
                // i64 -> i8 -> i1
                let trunc_i8 = self.new_temp();
                self.emit_line(&format!("  {} = trunc i64 {} to i8", trunc_i8, int_val));
                let trunc_i1 = self.new_temp();
                self.emit_line(&format!("  {} = trunc i8 {} to i1", trunc_i1, trunc_i8));
                format!("i1 {}", trunc_i1)
            }
            "c" => {
                // i64 -> i8
                let trunc_val = self.new_temp();
                self.emit_line(&format!("  {} = trunc i64 {} to i8", trunc_val, int_val));
                format!("i8 {}", trunc_val)
            }
            s if s.starts_with('o') || s.starts_with('g') => {
                // 对象类型，i8* 直接就是对象指针
                format!("i8* {}", box_ptr)
            }
            _ => {
                format!("i64 {}", int_val)
            }
        };

        Ok(result)
    }
}
