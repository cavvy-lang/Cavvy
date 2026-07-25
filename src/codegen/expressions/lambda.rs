//! Lambda 表达式代码生成
//!
//! 处理 Lambda 表达式和方法引用。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::SourceLocation;
use crate::miette_diagnostic::{CayResult, ErrorCodes, codegen_error_at};
use crate::types::Type;
use std::collections::HashSet;

impl IRGenerator {
    /// 收集 Lambda 表达式中引用的外部变量（自由变量）
    /// 返回: Vec<(变量名, LLVM类型, Cayvy类型)>
    fn collect_free_variables(&self, lambda: &LambdaExpr) -> Vec<(String, String, Type)> {
        let mut free_vars = Vec::new();
        let mut param_names: HashSet<String> =
            lambda.params.iter().map(|p| p.name.clone()).collect();

        // 收集 lambda 体中引用的所有标识符
        let mut referenced = HashSet::new();
        match &lambda.body {
            LambdaBody::Expr(expr) => {
                Self::collect_identifiers(expr, &mut referenced);
            }
            LambdaBody::Block(block) => {
                for stmt in &block.statements {
                    Self::collect_identifiers_from_stmt(stmt, &mut referenced);
                }
            }
        }

        // 筛选出外部变量（不在参数列表中且在当前作用域可访问）
        for name in referenced {
            if param_names.contains(&name) {
                continue; // 跳过 lambda 参数
            }
            // 检查是否是当前作用域中的变量
            if let Some(llvm_type) = self.scope_manager.get_var_type(&name) {
                let cay_type = self
                    .var_cay_types
                    .get(&name)
                    .cloned()
                    .unwrap_or(Type::Int32);
                free_vars.push((name, llvm_type, cay_type));
            }
        }

        free_vars
    }

    /// 从表达式中收集标识符
    fn collect_identifiers(expr: &Expr, result: &mut HashSet<String>) {
        match expr {
            Expr::Identifier(name) => {
                result.insert(name.as_ref().to_string());
            }
            Expr::Binary(bin) => {
                Self::collect_identifiers(&bin.left, result);
                Self::collect_identifiers(&bin.right, result);
            }
            Expr::Unary(unary) => {
                Self::collect_identifiers(&unary.operand, result);
            }
            Expr::Call(call) => {
                Self::collect_identifiers(&call.callee, result);
                for arg in &call.args {
                    Self::collect_identifiers(arg, result);
                }
            }
            Expr::MemberAccess(member) => {
                Self::collect_identifiers(&member.object, result);
            }
            Expr::ArrayAccess(arr) => {
                Self::collect_identifiers(&arr.array, result);
                Self::collect_identifiers(&arr.index, result);
            }
            Expr::Ternary(ternary) => {
                Self::collect_identifiers(&ternary.condition, result);
                Self::collect_identifiers(&ternary.true_branch, result);
                Self::collect_identifiers(&ternary.false_branch, result);
            }
            Expr::Lambda(lambda) => {
                // 嵌套 lambda：收集其体中的标识符，但排除其参数
                let mut nested_params: HashSet<String> =
                    lambda.params.iter().map(|p| p.name.clone()).collect();
                match &lambda.body {
                    LambdaBody::Expr(expr) => {
                        let mut nested_refs = HashSet::new();
                        Self::collect_identifiers(expr, &mut nested_refs);
                        for r in nested_refs {
                            if !nested_params.contains(&r) {
                                result.insert(r);
                            }
                        }
                    }
                    LambdaBody::Block(block) => {
                        let mut nested_refs = HashSet::new();
                        for stmt in &block.statements {
                            Self::collect_identifiers_from_stmt(stmt, &mut nested_refs);
                        }
                        for r in nested_refs {
                            if !nested_params.contains(&r) {
                                result.insert(r);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// 从语句中收集标识符
    fn collect_identifiers_from_stmt(stmt: &Stmt, result: &mut HashSet<String>) {
        match stmt {
            Stmt::Expr(expr) => {
                Self::collect_identifiers(expr, result);
            }
            Stmt::VarDecl(decl) => {
                if let Some(init) = &decl.initializer {
                    Self::collect_identifiers(init, result);
                }
            }
            Stmt::Return(Some(expr)) => {
                Self::collect_identifiers(expr, result);
            }
            Stmt::If(if_stmt) => {
                Self::collect_identifiers(&if_stmt.condition, result);
                Self::collect_identifiers_from_stmt(&if_stmt.then_branch, result);
                if let Some(else_branch) = &if_stmt.else_branch {
                    Self::collect_identifiers_from_stmt(else_branch, result);
                }
            }
            Stmt::While(while_stmt) => {
                Self::collect_identifiers(&while_stmt.condition, result);
                Self::collect_identifiers_from_stmt(&while_stmt.body, result);
            }
            Stmt::For(for_stmt) => {
                if let Some(init) = &for_stmt.init {
                    Self::collect_identifiers_from_stmt(init, result);
                }
                if let Some(cond) = &for_stmt.condition {
                    Self::collect_identifiers(cond, result);
                }
                if let Some(update) = &for_stmt.update {
                    Self::collect_identifiers(update, result);
                }
                Self::collect_identifiers_from_stmt(&for_stmt.body, result);
            }
            Stmt::Block(block) => {
                for stmt in &block.statements {
                    Self::collect_identifiers_from_stmt(stmt, result);
                }
            }
            _ => {}
        }
    }

    /// 生成 Lambda 表达式代码
    /// Lambda: (params) -> { body }
    /// 支持闭包捕获：通过环境指针传递外部变量
    ///
    /// # Arguments
    /// * `lambda` - Lambda 表达式
    /// * `var_name` - 变量名（用于记录环境指针）
    pub fn generate_lambda(
        &mut self,
        lambda: &LambdaExpr,
        var_name: Option<&str>,
    ) -> CayResult<String> {
        // 生成唯一的 Lambda 函数名（使用独立计数器避免嵌套lambda重复）
        let current_class = self.current_class.clone();
        let lambda_idx = self.lambda_counter;
        self.lambda_counter += 1;
        let lambda_name = format!("__lambda_{}_{}", current_class, lambda_idx);

        // 推断返回类型
        let return_type = self.infer_lambda_return_type(lambda)?;
        let llvm_return_type = self.type_to_llvm(&return_type);

        // 收集闭包捕获的外部变量
        let captures = self.collect_free_variables(lambda);
        let has_captures = !captures.is_empty();

        // 保存当前代码缓冲区
        let saved_code = std::mem::take(&mut self.code);
        let saved_temp_counter = self.temp_counter;
        let saved_return_type = self.current_return_type.clone();

        // 重置临时变量计数器并设置返回类型
        self.temp_counter = 0;
        self.current_return_type = llvm_return_type.clone();

        // 生成 Lambda 参数类型（显式参数 + 环境指针）
        let mut param_types = Vec::new();
        let mut param_names = Vec::new();

        for (i, param) in lambda.params.iter().enumerate() {
            let param_type = param
                .param_type
                .as_ref()
                .map(|t| self.type_to_llvm(t))
                .unwrap_or_else(|| "i64".to_string());
            param_types.push(format!("{} %param{}", param_type, i));
            param_names.push((
                param.name.clone(),
                param_type,
                format!("%param{}", i),
                param.param_type.clone(),
            ));
        }

        // 添加环境指针参数（如果有捕获变量）
        if has_captures {
            param_types.push("i8* %env".to_string());
        }

        // 记录捕获信息，供调用时使用
        let capture_info: Vec<(String, Type)> = captures
            .iter()
            .map(|(name, _llvm, cay)| (name.clone(), cay.clone()))
            .collect();
        self.lambda_captures
            .insert(lambda_name.clone(), capture_info);

        // 生成 Lambda 函数头
        self.emit_line(&format!(
            "\ndefine {} @{}({}) {{",
            llvm_return_type,
            lambda_name,
            param_types.join(", ")
        ));
        self.emit_line("entry:");

        // 创建新的作用域
        self.scope_manager.enter_scope();

        // 添加显式参数到作用域
        for (name, ty, llvm_name, cay_type) in &param_names {
            let declared_llvm_name = self.scope_manager.declare_var(name, ty);
            if let Some(cay_type) = cay_type {
                self.var_cay_types.insert(name.clone(), cay_type.clone());
                match cay_type {
                    Type::Object(class_name) | Type::Struct(class_name) => {
                        self.var_class_map.insert(name.clone(), class_name.clone());
                    }
                    Type::Generic(class_name, args) => {
                        let rendered = format!(
                            "{}<{}>",
                            class_name,
                            args.iter()
                                .map(|arg| arg.to_string())
                                .collect::<Vec<_>>()
                                .join(", ")
                        );
                        self.var_class_map.insert(name.clone(), rendered);
                    }
                    _ => {}
                }
            }
            self.emit_line(&format!(
                "  %{} = alloca {}, align {}",
                declared_llvm_name,
                ty,
                self.get_type_align(ty)
            ));
            self.emit_line(&format!(
                "  store {} {}, {}* %{}, align {}",
                ty,
                llvm_name,
                ty,
                declared_llvm_name,
                self.get_type_align(ty)
            ));
        }

        // 如果有捕获变量，从环境指针中加载它们
        if has_captures {
            // 计算环境结构体中每个字段的偏移
            let mut current_offset: i64 = 0;
            for (var_name, llvm_type, _cay_type) in &captures {
                let type_size = self.get_type_size(llvm_type);
                let align = self.get_type_align(llvm_type) as i64;
                // 对齐偏移
                current_offset = (current_offset + align - 1) / align * align;

                // 计算字段地址：env + offset
                let field_ptr_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = getelementptr i8, i8* %env, i32 {}",
                    field_ptr_temp, current_offset
                ));

                // 将 i8* 转换为字段类型指针
                let typed_ptr_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i8* {} to {}*",
                    typed_ptr_temp, field_ptr_temp, llvm_type
                ));

                // 加载字段值
                let field_val_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = load {}, {}* {}, align {}",
                    field_val_temp, llvm_type, llvm_type, typed_ptr_temp, align
                ));

                // 为捕获变量分配局部存储
                let declared_name = self.scope_manager.declare_var(var_name, llvm_type);
                self.emit_line(&format!(
                    "  %{} = alloca {}, align {}",
                    declared_name, llvm_type, align
                ));
                self.emit_line(&format!(
                    "  store {} {}, {}* %{}, align {}",
                    llvm_type, field_val_temp, llvm_type, declared_name, align
                ));

                current_offset += type_size;
            }
        }

        // 生成 Lambda 体
        self.generate_lambda_body(lambda, &return_type, &llvm_return_type)?;

        // 退出作用域
        self.scope_manager.exit_scope();

        self.emit_line("}\n");

        // 获取 Lambda 函数代码
        let lambda_code = std::mem::take(&mut self.code);

        // 恢复之前的代码缓冲区
        self.code = saved_code;
        self.temp_counter = saved_temp_counter;
        self.current_return_type = saved_return_type;

        // 将 Lambda 函数代码存储到全局函数列表
        self.lambda_functions.push(lambda_code);

        // 如果有捕获变量，分配环境结构体并填充
        let env_ptr_temp = if has_captures {
            // 确保 malloc 已声明
            if !self.is_extern_emitted("malloc@i8*@i64") {
                self.emit_raw("declare i8* @malloc(i64)");
                self.mark_extern_emitted("malloc@i8*@i64".to_string());
            }

            // 计算环境结构体总大小
            let mut total_size: i64 = 0;
            for (_var_name, llvm_type, _cay_type) in &captures {
                let type_size = self.get_type_size(llvm_type);
                let align = self.get_type_align(llvm_type) as i64;
                total_size = (total_size + align - 1) / align * align;
                total_size += type_size;
            }

            // 分配环境结构体
            let env_ptr = self.new_temp();
            self.emit_line(&format!(
                "  {} = call i8* @malloc(i64 {})",
                env_ptr, total_size
            ));

            // 填充环境结构体
            let mut current_offset: i64 = 0;
            for (var_name, llvm_type, _cay_type) in &captures {
                let type_size = self.get_type_size(llvm_type);
                let align = self.get_type_align(llvm_type) as i64;
                current_offset = (current_offset + align - 1) / align * align;

                // 获取变量值
                let var_llvm_name = self
                    .scope_manager
                    .get_llvm_name(var_name)
                    .unwrap_or_else(|| var_name.to_string());
                let var_val_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = load {}, {}* %{}, align {}",
                    var_val_temp, llvm_type, llvm_type, var_llvm_name, align
                ));

                // 计算字段地址
                let field_ptr_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = getelementptr i8, i8* {}, i32 {}",
                    field_ptr_temp, env_ptr, current_offset
                ));

                // 转换为字段类型指针
                let typed_ptr_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i8* {} to {}*",
                    typed_ptr_temp, field_ptr_temp, llvm_type
                ));

                // 存储变量值到环境结构体
                self.emit_line(&format!(
                    "  store {} {}, {}* {}, align {}",
                    llvm_type, var_val_temp, llvm_type, typed_ptr_temp, align
                ));

                current_offset += type_size;
            }

            // 记录环境指针（如果提供了变量名）
            if let Some(vn) = var_name {
                self.lambda_envs.insert(vn.to_string(), env_ptr.clone());
            }

            Some(env_ptr)
        } else {
            None
        };

        // 返回函数指针（bitcast 为 i8*）
        let func_ptr_temp = self.new_temp();
        let llvm_param_types: Vec<String> = param_types
            .iter()
            .map(|p| p.split_whitespace().next().unwrap_or("i64").to_string())
            .collect();
        self.emit_line(&format!(
            "  {} = bitcast {} ({})* @{} to i8*",
            func_ptr_temp,
            llvm_return_type,
            llvm_param_types.join(", "),
            lambda_name
        ));

        // 将函数指针和环境指针打包成一个结构体
        // 结构体: { i8* func_ptr, i8* env_ptr }
        // 即使没有捕获变量，也打包结构体（环境指针为 null），以保持统一的调用约定

        // 确保 malloc 已声明
        if !self.is_extern_emitted("malloc@i8*@i64") {
            self.emit_raw("declare i8* @malloc(i64)");
            self.mark_extern_emitted("malloc@i8*@i64".to_string());
        }

        let struct_ptr = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @malloc(i64 16)", struct_ptr));

        // 存储函数指针到结构体偏移0
        let func_ptr_slot = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8**",
            func_ptr_slot, struct_ptr
        ));
        self.emit_line(&format!(
            "  store i8* {}, i8** {}, align 8",
            func_ptr_temp, func_ptr_slot
        ));

        // 存储环境指针到结构体偏移8（如果没有捕获变量，则为 null）
        let env_ptr = env_ptr_temp.unwrap_or_else(|| "null".to_string());
        let env_ptr_slot_temp = self.new_temp();
        self.emit_line(&format!(
            "  {} = getelementptr i8, i8* {}, i64 8",
            env_ptr_slot_temp, struct_ptr
        ));
        let env_ptr_slot_cast = self.new_temp();
        self.emit_line(&format!(
            "  {} = bitcast i8* {} to i8**",
            env_ptr_slot_cast, env_ptr_slot_temp
        ));
        self.emit_line(&format!(
            "  store i8* {}, i8** {}, align 8",
            env_ptr, env_ptr_slot_cast
        ));

        Ok(format!("i8* {}", struct_ptr))
    }

    /// 推断 Lambda 表达式的返回类型
    fn infer_lambda_return_type(&self, lambda: &LambdaExpr) -> CayResult<Type> {
        match &lambda.body {
            LambdaBody::Expr(expr) => {
                // 对于表达式体，推断表达式类型
                self.infer_expr_type(expr)
            }
            LambdaBody::Block(block) => {
                // 对于块体，查找 return 语句
                for stmt in &block.statements {
                    if let Stmt::Return(Some(ret_expr)) = stmt {
                        return self.infer_expr_type(ret_expr);
                    }
                }
                // 没有 return 语句，返回 void
                Ok(Type::Void)
            }
        }
    }

    /// 推断表达式类型（用于 Lambda 返回类型推断）
    fn infer_expr_type(&self, expr: &Expr) -> CayResult<Type> {
        match expr {
            Expr::Literal(lit_expr) => match &lit_expr.value {
                LiteralValue::Int32(_) => Ok(Type::Int32),
                LiteralValue::Int64(_) => Ok(Type::Int64),
                LiteralValue::Float32(_) => Ok(Type::Float32),
                LiteralValue::Float64(_) => Ok(Type::Float64),
                LiteralValue::String(_) => Ok(Type::String),
                LiteralValue::Bool(_) => Ok(Type::Bool),
                LiteralValue::Char(_) => Ok(Type::Char),
                LiteralValue::Null => Ok(Type::Object("Object".to_string())),
            },
            Expr::Binary(bin) => {
                // 对于二元表达式，根据操作符推断
                let left_type = self.infer_expr_type(&bin.left)?;
                let right_type = self.infer_expr_type(&bin.right)?;

                match bin.op {
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Mod => {
                        // 数值运算返回较宽的类型
                        Ok(self.promote_types(&left_type, &right_type))
                    }
                    BinaryOp::Eq
                    | BinaryOp::Ne
                    | BinaryOp::Lt
                    | BinaryOp::Le
                    | BinaryOp::Gt
                    | BinaryOp::Ge
                    | BinaryOp::And
                    | BinaryOp::Or => Ok(Type::Bool),
                    _ => Ok(left_type),
                }
            }
            Expr::Unary(unary) => self.infer_expr_type(&unary.operand),
            Expr::Call(call) => {
                // 尝试从类型注册表获取方法返回类型
                if let Some(ref registry) = self.type_registry {
                    if let Expr::Identifier(name) = call.callee.as_ref() {
                        if !self.current_class.is_empty() {
                            if let Some(method_info) =
                                registry.get_method(&self.current_class, name.as_ref())
                            {
                                return Ok(method_info.return_type.clone());
                            }
                        }
                    }
                }
                // 默认返回 int
                Ok(Type::Int32)
            }
            Expr::Identifier(name) => {
                // 从变量类型映射中查找
                if let Some(llvm_type) = self.var_types.get(name.as_ref()) {
                    Self::map_llvm_to_cay_type(llvm_type)
                        .map(Ok)
                        .unwrap_or(Ok(Type::Int32))
                } else {
                    Ok(Type::Int32)
                }
            }
            _ => Ok(Type::Int32),
        }
    }

    /// 类型提升：选择两个类型中较宽的一个
    fn promote_types(&self, left: &Type, right: &Type) -> Type {
        use Type::*;
        match (left, right) {
            (Float64, _) | (_, Float64) => Float64,
            (Float32, _) | (_, Float32) => Float32,
            (Int64, _) | (_, Int64) => Int64,
            _ => Int32,
        }
    }

    /// 生成 Lambda 体代码
    fn generate_lambda_body(
        &mut self,
        lambda: &LambdaExpr,
        _return_type: &Type,
        llvm_return_type: &str,
    ) -> CayResult<()> {
        match &lambda.body {
            LambdaBody::Expr(expr) => {
                let val = self.generate_expression(expr)?;
                let (value_type, val_str) = self.parse_typed_value(&val);

                // 如果表达式类型与返回类型不匹配，进行转换
                if value_type != llvm_return_type {
                    let converted = self.convert_type(
                        &val_str,
                        &value_type,
                        llvm_return_type,
                        lambda.loc.clone(),
                    )?;
                    self.emit_line(&format!("  ret {} {}", llvm_return_type, converted));
                } else {
                    self.emit_line(&format!("  ret {} {}", llvm_return_type, val_str));
                }
                Ok(())
            }
            LambdaBody::Block(block) => {
                // 生成块中的语句
                let mut has_return = false;
                for stmt in &block.statements {
                    if matches!(stmt, Stmt::Return(_)) {
                        has_return = true;
                    }
                    self.generate_statement(stmt)?;
                }

                // 如果没有显式 return，添加默认返回
                if !has_return {
                    if llvm_return_type == "void" {
                        self.emit_line("  ret void");
                    } else {
                        let default_val = self.get_default_value_for_type(llvm_return_type);
                        self.emit_line(&format!("  ret {} {}", llvm_return_type, default_val));
                    }
                }
                Ok(())
            }
        }
    }

    /// 获取类型的默认值
    fn get_default_value_for_type(&self, llvm_type: &str) -> String {
        match llvm_type {
            "i32" | "i64" | "i8" | "i1" => "0".to_string(),
            "float" | "double" => "0.0".to_string(),
            t if t.ends_with("*") => "null".to_string(),
            _ => "0".to_string(),
        }
    }

    /// 类型转换辅助函数
    fn convert_type(
        &mut self,
        val: &str,
        from_type: &str,
        to_type: &str,
        loc: SourceLocation,
    ) -> CayResult<String> {
        if from_type == to_type {
            return Ok(val.to_string());
        }

        let temp = self.new_temp();

        // 指针到整数的转换（ptrtoint）- 必须优先于整数检查
        if from_type.ends_with("*") && to_type.starts_with("i") && !to_type.ends_with("*") {
            self.emit_line(&format!(
                "  {} = ptrtoint {} {} to {}",
                temp, from_type, val, to_type
            ));
            return Ok(temp);
        }

        // 整数到指针的转换（inttoptr）- 必须优先于整数检查
        // 只有当源类型是整数且目标是任意指针类型时才转换
        if from_type.starts_with("i") && !from_type.ends_with("*") && to_type.ends_with("*") {
            // 使用 i64 作为中间类型（指针大小）
            if from_type != "i64" {
                let i64_temp = self.new_temp();
                self.emit_line(&format!(
                    "  {} = sext {} {} to i64",
                    i64_temp, from_type, val
                ));
                self.emit_line(&format!(
                    "  {} = inttoptr i64 {} to {}",
                    temp, i64_temp, to_type
                ));
            } else {
                self.emit_line(&format!(
                    "  {} = inttoptr {} {} to {}",
                    temp, from_type, val, to_type
                ));
            }
            return Ok(temp);
        }

        // 指针类型转换（bitcast）- 必须优先于其他检查
        if from_type.ends_with("*") && to_type.ends_with("*") {
            self.emit_line(&format!(
                "  {} = bitcast {} {} to {}",
                temp, from_type, val, to_type
            ));
            return Ok(temp);
        }

        // 整数类型之间的转换（严格排除指针）
        let is_from_ptr = from_type.ends_with("*");
        let is_to_ptr = to_type.ends_with("*");
        let is_from_int = from_type.starts_with("i") && !is_from_ptr;
        let is_to_int = to_type.starts_with("i") && !is_to_ptr;

        if is_from_int && is_to_int {
            let from_bits: u32 = from_type.trim_start_matches('i').parse().unwrap_or(64);
            let to_bits: u32 = to_type.trim_start_matches('i').parse().unwrap_or(64);

            if to_bits > from_bits {
                self.emit_line(&format!(
                    "  {} = sext {} {} to {}",
                    temp, from_type, val, to_type
                ));
            } else {
                self.emit_line(&format!(
                    "  {} = trunc {} {} to {}",
                    temp, from_type, val, to_type
                ));
            }
            return Ok(temp);
        }

        // 整数到浮点数转换（严格排除指针）
        if is_from_int && (to_type == "float" || to_type == "double") {
            self.emit_line(&format!(
                "  {} = sitofp {} {} to {}",
                temp, from_type, val, to_type
            ));
            return Ok(temp);
        }

        // 浮点数到整数转换（严格排除指针）
        if (from_type == "float" || from_type == "double") && is_to_int {
            self.emit_line(&format!(
                "  {} = fptosi {} {} to {}",
                temp, from_type, val, to_type
            ));
            return Ok(temp);
        }

        // 浮点数类型转换
        if from_type == "double" && to_type == "float" {
            self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
            return Ok(temp);
        }
        if from_type == "float" && to_type == "double" {
            self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
            return Ok(temp);
        }

        // 其他不支持的转换
        Err(codegen_error_at(
            ErrorCodes::CODEGEN_INVALID_OPERATION,
            loc,
            format!(
                "Unsupported type conversion from {} to {}",
                from_type, to_type
            ),
        ))
    }

    /// 将 LLVM 类型映射到 Cayvy 类型（静态辅助函数）
    fn map_llvm_to_cay_type(llvm_type: &str) -> Option<Type> {
        match llvm_type {
            "i32" => Some(Type::Int32),
            "i64" => Some(Type::Int64),
            "float" => Some(Type::Float32),
            "double" => Some(Type::Float64),
            "i1" => Some(Type::Bool),
            "i8" => Some(Type::Char),
            "i8*" => Some(Type::String),
            "void" => Some(Type::Void),
            _ => {
                // 检查是否是对象指针类型
                if llvm_type.starts_with("%") && llvm_type.ends_with("*") {
                    let class_name = llvm_type.trim_start_matches('%').trim_end_matches('*');
                    Some(Type::Object(class_name.to_string()))
                } else {
                    None
                }
            }
        }
    }

    /// 生成方法引用表达式代码
    /// 方法引用: ClassName::methodName 或 obj::methodName
    ///
    /// # Arguments
    /// * `method_ref` - 方法引用表达式
    pub fn generate_method_ref(&mut self, method_ref: &MethodRefExpr) -> CayResult<String> {
        let temp = self.new_temp();

        if let Some(ref class_name) = method_ref.class_name {
            // 静态方法引用: ClassName::methodName
            // 尝试从类型注册表获取方法签名
            let method_info = self
                .type_registry
                .as_ref()
                .and_then(|registry| registry.get_method(class_name, &method_ref.method_name))
                .cloned();

            // 函数名必须使用与定义处一致的 Itanium 改编名，
            // 否则会产生 use of undefined value（定义是 @_ZN... 形式）
            let (fn_name, return_type, param_types) = if let Some(method_info) = method_info {
                let ret = self.type_to_llvm(&method_info.return_type);
                let cay_params: Vec<crate::types::Type> = method_info
                    .params
                    .iter()
                    .map(|p| p.param_type.clone())
                    .collect();
                let params: Vec<String> = cay_params
                    .iter()
                    .map(|t| self.type_to_llvm(t))
                    .collect();
                let mangled = self.mangle_itanium_method(
                    class_name,
                    &method_ref.method_name,
                    &cay_params,
                    false,
                    false,
                );
                (mangled, ret, params)
            } else {
                (
                    format!("{}.{}", class_name, method_ref.method_name),
                    "i32".to_string(),
                    vec![],
                )
            };

            // 生成函数指针类型
            let fn_ptr_type = format!("{} ({})", return_type, param_types.join(", "));

            // 生成 bitcast
            self.emit_line(&format!(
                "  {} = bitcast {} ({})* @{} to i8*",
                temp,
                return_type,
                param_types.join(", "),
                fn_name
            ));

            Ok(format!("i8* {}", temp))
        } else {
            // 实例方法引用: obj::methodName
            // 需要生成一个闭包，包含对象指针和方法指针
            // 简化处理：直接返回方法指针
            let fn_name = format!("{}.{}", self.current_class, method_ref.method_name);
            self.emit_line(&format!(
                "  {} = bitcast void (i8*)* @{} to i8*",
                temp, fn_name
            ));
            Ok(format!("i8* {}", temp))
        }
    }
}
