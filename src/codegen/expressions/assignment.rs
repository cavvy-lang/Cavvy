//! 赋值表达式代码生成
//!
//! 处理变量赋值、数组元素赋值和静态字段赋值。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::error::{cayResult, codegen_error_at};

impl IRGenerator {
    /// 生成赋值表达式代码
    ///
    /// # Arguments
    /// * `assign` - 赋值表达式
    pub fn generate_assignment(&mut self, assign: &AssignmentExpr) -> cayResult<String> {
        let value = self.generate_expression(&assign.value)?;
        let (value_type, val) = self.parse_typed_value(&value);

        match assign.target.as_ref() {
            Expr::MemberAccess(member) => {
                self.generate_member_assignment(member, &value_type, &val, &value)
            }
            Expr::Identifier(name) => self.generate_variable_assignment(
                name.as_ref(),
                &value_type,
                &val,
                &value,
                &assign.loc,
            ),
            Expr::ArrayAccess(arr_access) => {
                self.generate_array_assignment(arr_access, &value_type, &val, &value)
            }
            Expr::Unary(unary) if unary.op == UnaryOp::Deref => {
                // 解引用赋值: *p = value
                self.generate_deref_assignment(unary, &value_type, &val, &value)
            }
            _ => Err(codegen_error_at(
                assign.loc.clone(),
                "Invalid assignment target",
            )),
        }
    }

    /// 生成成员赋值（静态字段或实例字段赋值）
    fn generate_member_assignment(
        &mut self,
        member: &MemberAccessExpr,
        value_type: &str,
        val: &str,
        value: &str,
    ) -> cayResult<String> {
        // 检查是否是静态字段赋值: ClassName.fieldName = value
        if let Expr::Identifier(class_name) = &*member.object {
            let static_key = format!("{}.{}", class_name, member.member);
            if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                // 静态字段赋值
                let align = self.get_type_align(&field_info.llvm_type);

                // 检查是否是数组类型
                let is_array = matches!(field_info.field_type, crate::types::Type::Array(_));

                // 对于数组类型，值类型应该是元素类型指针（如 i32*）
                // 静态字段类型也是元素类型指针（如 i32*），应该直接匹配
                if is_array && value_type == field_info.llvm_type {
                    self.emit_line(&format!(
                        "  store {} {}, {}* {}, align {}",
                        value_type, val, field_info.llvm_type, field_info.name, align
                    ));
                    return Ok(value.to_string());
                }

                // 如果值类型与字段类型不匹配，需要转换
                if value_type != field_info.llvm_type {
                    let temp = self.new_temp();
                    // 完整类型转换逻辑
                    // 整数类型之间的转换
                    if value_type.starts_with("i")
                        && field_info.llvm_type.starts_with("i")
                        && !value_type.ends_with("*")
                        && !field_info.llvm_type.ends_with("*")
                    {
                        let from_bits: u32 =
                            value_type.trim_start_matches('i').parse().unwrap_or(64);
                        let to_bits: u32 = field_info
                            .llvm_type
                            .trim_start_matches('i')
                            .parse()
                            .unwrap_or(64);
                        if to_bits > from_bits {
                            self.emit_line(&format!(
                                "  {} = sext {} {} to {}",
                                temp, value_type, val, field_info.llvm_type
                            ));
                        } else {
                            self.emit_line(&format!(
                                "  {} = trunc {} {} to {}",
                                temp, value_type, val, field_info.llvm_type
                            ));
                        }
                        self.emit_line(&format!(
                            "  store {} {}, {}* {}, align {}",
                            field_info.llvm_type,
                            temp,
                            field_info.llvm_type,
                            field_info.name,
                            align
                        ));
                        return Ok(format!("{} {}", field_info.llvm_type, temp));
                    }
                    // 整数到浮点数转换
                    else if value_type.starts_with("i")
                        && !value_type.ends_with("*")
                        && (field_info.llvm_type == "float" || field_info.llvm_type == "double")
                    {
                        self.emit_line(&format!(
                            "  {} = sitofp {} {} to {}",
                            temp, value_type, val, field_info.llvm_type
                        ));
                        self.emit_line(&format!(
                            "  store {} {}, {}* {}, align {}",
                            field_info.llvm_type,
                            temp,
                            field_info.llvm_type,
                            field_info.name,
                            align
                        ));
                        return Ok(format!("{} {}", field_info.llvm_type, temp));
                    }
                    // 浮点数到整数转换
                    else if (value_type == "float" || value_type == "double")
                        && field_info.llvm_type.starts_with("i")
                        && !field_info.llvm_type.ends_with("*")
                    {
                        self.emit_line(&format!(
                            "  {} = fptosi {} {} to {}",
                            temp, value_type, val, field_info.llvm_type
                        ));
                        self.emit_line(&format!(
                            "  store {} {}, {}* {}, align {}",
                            field_info.llvm_type,
                            temp,
                            field_info.llvm_type,
                            field_info.name,
                            align
                        ));
                        return Ok(format!("{} {}", field_info.llvm_type, temp));
                    }
                    // 浮点数类型转换
                    else if value_type == "double" && field_info.llvm_type == "float" {
                        self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                        self.emit_line(&format!(
                            "  store {} {}, {}* {}, align {}",
                            field_info.llvm_type,
                            temp,
                            field_info.llvm_type,
                            field_info.name,
                            align
                        ));
                        return Ok(format!("{} {}", field_info.llvm_type, temp));
                    } else if value_type == "float" && field_info.llvm_type == "double" {
                        self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                        self.emit_line(&format!(
                            "  store {} {}, {}* {}, align {}",
                            field_info.llvm_type,
                            temp,
                            field_info.llvm_type,
                            field_info.name,
                            align
                        ));
                        return Ok(format!("{} {}", field_info.llvm_type, temp));
                    }
                    // 指针类型转换（bitcast）
                    else if value_type.ends_with("*") && field_info.llvm_type.ends_with("*") {
                        self.emit_line(&format!(
                            "  {} = bitcast {} {} to {}",
                            temp, value_type, val, field_info.llvm_type
                        ));
                        self.emit_line(&format!(
                            "  store {} {}, {}* {}, align {}",
                            field_info.llvm_type,
                            temp,
                            field_info.llvm_type,
                            field_info.name,
                            align
                        ));
                        return Ok(format!("{} {}", field_info.llvm_type, temp));
                    }
                }

                // 类型匹配，直接存储
                self.emit_line(&format!(
                    "  store {} {}, {}* {}, align {}",
                    value_type, val, field_info.llvm_type, field_info.name, align
                ));
                return Ok(value.to_string());
            }
        }

        // 处理实例字段赋值: this.fieldName = value 或 obj.fieldName = value 或 obj.field1.field2 = value

        // 尝试使用 get_nested_field_pointer 处理链式成员访问
        if let Ok((field_llvm_type, field_ptr)) = self.get_nested_field_pointer(member) {
            let align = self.get_type_align(&field_llvm_type);

            // 计算指针类型：如果llvm_type已经是指针类型，则不需要再加*
            let ptr_type = if field_llvm_type.ends_with('*') {
                field_llvm_type.clone()
            } else {
                format!("{}*", field_llvm_type)
            };

            // 如果值类型与字段类型不匹配，需要转换
            let final_val = if value_type != field_llvm_type {
                let temp = self.new_temp();
                // 整数类型之间的转换
                if value_type.starts_with("i")
                    && field_llvm_type.starts_with("i")
                    && !value_type.ends_with("*")
                    && !field_llvm_type.ends_with("*")
                {
                    let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                    let to_bits: u32 = field_llvm_type
                        .trim_start_matches('i')
                        .parse()
                        .unwrap_or(64);
                    if to_bits > from_bits {
                        self.emit_line(&format!(
                            "  {} = sext {} {} to {}",
                            temp, value_type, val, field_llvm_type
                        ));
                    } else {
                        self.emit_line(&format!(
                            "  {} = trunc {} {} to {}",
                            temp, value_type, val, field_llvm_type
                        ));
                    }
                    temp
                }
                // 整数到浮点数转换
                else if value_type.starts_with("i")
                    && !value_type.ends_with("*")
                    && (field_llvm_type == "float" || field_llvm_type == "double")
                {
                    self.emit_line(&format!(
                        "  {} = sitofp {} {} to {}",
                        temp, value_type, val, field_llvm_type
                    ));
                    temp
                }
                // 浮点数到整数转换
                else if (value_type == "float" || value_type == "double")
                    && field_llvm_type.starts_with("i")
                    && !field_llvm_type.ends_with("*")
                {
                    self.emit_line(&format!(
                        "  {} = fptosi {} {} to {}",
                        temp, value_type, val, field_llvm_type
                    ));
                    temp
                }
                // 浮点数类型转换
                else if value_type == "double" && field_llvm_type == "float" {
                    self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                    temp
                } else if value_type == "float" && field_llvm_type == "double" {
                    self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                    temp
                }
                // 整数到指针转换（用于字符串等引用类型）
                else if value_type.starts_with("i")
                    && !value_type.ends_with("*")
                    && field_llvm_type.ends_with("*")
                {
                    self.emit_line(&format!(
                        "  {} = inttoptr {} {} to {}",
                        temp, value_type, val, field_llvm_type
                    ));
                    temp
                } else {
                    // 其他不支持的类型转换，报错
                    return Err(codegen_error_at(
                        member.loc.clone(),
                        format!(
                            "Cannot convert {} to {} for field assignment",
                            value_type, field_llvm_type
                        ),
                    ));
                }
            } else {
                val.to_string()
            };

            // 存储值到字段
            self.emit_line(&format!(
                "  store {} {}, {} {}, align {}",
                field_llvm_type, final_val, ptr_type, field_ptr, align
            ));
            return Ok(value.to_string());
        }

        Err(codegen_error_at(
            member.loc.clone(),
            "Invalid member access assignment target",
        ))
    }

    /// 生成变量赋值
    fn generate_variable_assignment(
        &mut self,
        name: &str,
        value_type: &str,
        val: &str,
        value: &str,
        loc: &crate::error::SourceLocation,
    ) -> cayResult<String> {
        // 优先使用作用域管理器获取变量类型和 LLVM 名称
        let (var_type, llvm_name) = if let Some(scope_type) = self.scope_manager.get_var_type(name)
        {
            let llvm_name = self
                .scope_manager
                .get_llvm_name(name)
                .unwrap_or_else(|| name.to_string());
            (scope_type, llvm_name)
        } else {
            // 检查是否是当前类的静态字段
            if !self.current_class.is_empty() {
                let static_key = format!("{}.{}", self.current_class, name);
                if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                    let align = self.get_type_align(&field_info.llvm_type);
                    self.emit_line(&format!(
                        "  store {} {}, {}* {}, align {}",
                        field_info.llvm_type, val, field_info.llvm_type, field_info.name, align
                    ));
                    return Ok(value.to_string());
                }
            }
            // 回退到旧系统
            let var_type = self
                .var_types
                .get(name)
                .ok_or_else(|| {
                    codegen_error_at(loc.clone(), format!("Variable '{}' not found", name))
                })?
                .clone();
            (var_type, name.to_string())
        };

        // 如果值类型与变量类型不匹配，需要转换
        if value_type != var_type {
            return self
                .generate_assignment_with_conversion(&var_type, &llvm_name, value_type, val);
        }

        // 类型匹配，直接存储
        let align = self.get_type_align(&var_type);
        self.emit_line(&format!(
            "  store {} {}, {}* %{}, align {}",
            var_type, val, var_type, llvm_name, align
        ));
        Ok(value.to_string())
    }

    /// 生成数组元素赋值
    fn generate_array_assignment(
        &mut self,
        arr_access: &ArrayAccessExpr,
        value_type: &str,
        val: &str,
        value: &str,
    ) -> cayResult<String> {
        // 获取数组元素指针
        let (elem_type, elem_ptr, _) = self.get_array_element_ptr(arr_access)?;

        // 如果值类型与元素类型不匹配，需要转换
        if value_type != elem_type {
            return self.generate_array_assignment_with_conversion(
                &elem_type, &elem_ptr, value_type, val, value,
            );
        }

        // 类型匹配，直接存储到数组元素
        let align = self.get_type_align(&elem_type);
        self.emit_line(&format!(
            "  store {} {}, {}* {}, align {}",
            elem_type, val, elem_type, elem_ptr, align
        ));
        Ok(value.to_string())
    }

    /// 生成带类型转换的变量赋值
    fn generate_assignment_with_conversion(
        &mut self,
        var_type: &str,
        llvm_name: &str,
        value_type: &str,
        val: &str,
    ) -> cayResult<String> {
        let temp = self.new_temp();

        // 浮点类型转换
        if value_type == "double" && var_type == "float" {
            // double -> float 转换
            self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
            let align = self.get_type_align("float");
            self.emit_line(&format!(
                "  store float {}, float* %{}, align {}",
                temp, llvm_name, align
            ));
            return Ok(format!("float {}", temp));
        } else if value_type == "float" && var_type == "double" {
            // float -> double 转换
            self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
            let align = self.get_type_align("double");
            self.emit_line(&format!(
                "  store double {}, double* %{}, align {}",
                temp, llvm_name, align
            ));
            return Ok(format!("double {}", temp));
        }
        // 整数到浮点数转换
        else if value_type.starts_with("i")
            && !value_type.ends_with("*")
            && (var_type == "float" || var_type == "double")
        {
            // 整数 -> 浮点数转换
            self.emit_line(&format!(
                "  {} = sitofp {} {} to {}",
                temp, value_type, val, var_type
            ));
            let align = self.get_type_align(var_type);
            self.emit_line(&format!(
                "  store {} {}, {}* %{}, align {}",
                var_type, temp, var_type, llvm_name, align
            ));
            return Ok(format!("{} {}", var_type, temp));
        }
        // 指针到整数转换 (ptrtoint)
        else if value_type.ends_with("*") && var_type.starts_with("i") && !var_type.ends_with("*")
        {
            self.emit_line(&format!(
                "  {} = ptrtoint {} {} to {}",
                temp, value_type, val, var_type
            ));
            let align = self.get_type_align(var_type);
            self.emit_line(&format!(
                "  store {} {}, {}* %{}, align {}",
                var_type, temp, var_type, llvm_name, align
            ));
            return Ok(format!("{} {}", var_type, temp));
        }
        // 整数到指针转换 (inttoptr)
        else if value_type.starts_with("i")
            && !value_type.ends_with("*")
            && var_type.ends_with("*")
        {
            self.emit_line(&format!(
                "  {} = inttoptr {} {} to {}",
                temp, value_type, val, var_type
            ));
            let align = self.get_type_align(var_type);
            self.emit_line(&format!(
                "  store {} {}, {}* %{}, align {}",
                var_type, temp, var_type, llvm_name, align
            ));
            return Ok(format!("{} {}", var_type, temp));
        }
        // i8* 解箱转换（用于泛型类型返回值）
        else if value_type == "i8*" {
            // i8* -> i1 (bool)
            if var_type == "i1" {
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                let trunc_i8 = self.new_temp();
                self.emit_line(&format!("  {} = trunc i64 {} to i8", trunc_i8, int_val));
                self.emit_line(&format!("  {} = trunc i8 {} to i1", temp, trunc_i8));
                let align = self.get_type_align(var_type);
                self.emit_line(&format!(
                    "  store i1 {}, i1* %{}, align {}",
                    temp, llvm_name, align
                ));
                return Ok(format!("i1 {}", temp));
            }
            // i8* -> i8 (char)
            else if var_type == "i8" {
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                self.emit_line(&format!("  {} = trunc i64 {} to i8", temp, int_val));
                let align = self.get_type_align(var_type);
                self.emit_line(&format!(
                    "  store i8 {}, i8* %{}, align {}",
                    temp, llvm_name, align
                ));
                return Ok(format!("i8 {}", temp));
            }
            // i8* -> i16
            else if var_type == "i16" {
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                self.emit_line(&format!("  {} = trunc i64 {} to i16", temp, int_val));
                let align = self.get_type_align(var_type);
                self.emit_line(&format!(
                    "  store i16 {}, i16* %{}, align {}",
                    temp, llvm_name, align
                ));
                return Ok(format!("i16 {}", temp));
            }
            // i8* -> i32
            else if var_type == "i32" {
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                self.emit_line(&format!("  {} = trunc i64 {} to i32", temp, int_val));
                let align = self.get_type_align(var_type);
                self.emit_line(&format!(
                    "  store i32 {}, i32* %{}, align {}",
                    temp, llvm_name, align
                ));
                return Ok(format!("i32 {}", temp));
            }
            // i8* -> i64
            else if var_type == "i64" {
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", temp, val));
                let align = self.get_type_align(var_type);
                self.emit_line(&format!(
                    "  store i64 {}, i64* %{}, align {}",
                    temp, llvm_name, align
                ));
                return Ok(format!("i64 {}", temp));
            }
            // i8* -> float
            else if var_type == "float" {
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                let double_val = self.new_temp();
                self.emit_line(&format!(
                    "  {} = bitcast i64 {} to double",
                    double_val, int_val
                ));
                self.emit_line(&format!(
                    "  {} = fptrunc double {} to float",
                    temp, double_val
                ));
                let align = self.get_type_align(var_type);
                self.emit_line(&format!(
                    "  store float {}, float* %{}, align {}",
                    temp, llvm_name, align
                ));
                return Ok(format!("float {}", temp));
            }
            // i8* -> double
            else if var_type == "double" {
                let int_val = self.new_temp();
                self.emit_line(&format!("  {} = ptrtoint i8* {} to i64", int_val, val));
                self.emit_line(&format!("  {} = bitcast i64 {} to double", temp, int_val));
                let align = self.get_type_align(var_type);
                self.emit_line(&format!(
                    "  store double {}, double* %{}, align {}",
                    temp, llvm_name, align
                ));
                return Ok(format!("double {}", temp));
            }
            // i8* -> 其他指针类型
            else if var_type.ends_with("*") {
                self.emit_line(&format!("  {} = bitcast i8* {} to {}", temp, val, var_type));
                let align = self.get_type_align(var_type);
                self.emit_line(&format!(
                    "  store {} {}, {}* %{}, align {}",
                    var_type, temp, var_type, llvm_name, align
                ));
                return Ok(format!("{} {}", var_type, temp));
            }
        }
        // 整数类型转换
        else if value_type.starts_with("i")
            && var_type.starts_with("i")
            && !value_type.ends_with("*")
            && !var_type.ends_with("*")
        {
            let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
            let to_bits: u32 = var_type.trim_start_matches('i').parse().unwrap_or(64);

            if to_bits > from_bits {
                // 符号扩展
                self.emit_line(&format!(
                    "  {} = sext {} {} to {}",
                    temp, value_type, val, var_type
                ));
            } else {
                // 截断
                self.emit_line(&format!(
                    "  {} = trunc {} {} to {}",
                    temp, value_type, val, var_type
                ));
            }
            let align = self.get_type_align(var_type);
            self.emit_line(&format!(
                "  store {} {}, {}* %{}, align {}",
                var_type, temp, var_type, llvm_name, align
            ));
            return Ok(format!("{} {}", var_type, temp));
        }

        // 默认情况：直接存储
        let align = self.get_type_align(var_type);
        self.emit_line(&format!(
            "  store {} {}, {}* %{}, align {}",
            var_type, val, var_type, llvm_name, align
        ));
        Ok(format!("{} {}", var_type, val))
    }

    /// 生成带类型转换的数组元素赋值
    fn generate_array_assignment_with_conversion(
        &mut self,
        elem_type: &str,
        elem_ptr: &str,
        value_type: &str,
        val: &str,
        value: &str,
    ) -> cayResult<String> {
        let temp = self.new_temp();

        // 浮点类型转换
        if value_type == "double" && elem_type == "float" {
            // double -> float 转换
            self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
            let align = self.get_type_align(elem_type);
            self.emit_line(&format!(
                "  store float {}, {}* {}, align {}",
                temp, elem_type, elem_ptr, align
            ));
            return Ok(format!("float {}", temp));
        } else if value_type == "float" && elem_type == "double" {
            // float -> double 转换
            self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
            let align = self.get_type_align(elem_type);
            self.emit_line(&format!(
                "  store double {}, {}* {}, align {}",
                temp, elem_type, elem_ptr, align
            ));
            return Ok(format!("double {}", temp));
        }
        // 整数到浮点数转换
        else if value_type.starts_with("i")
            && !value_type.ends_with("*")
            && (elem_type == "float" || elem_type == "double")
        {
            // 整数 -> 浮点数转换
            self.emit_line(&format!(
                "  {} = sitofp {} {} to {}",
                temp, value_type, val, elem_type
            ));
            let align = self.get_type_align(elem_type);
            self.emit_line(&format!(
                "  store {} {}, {}* {}, align {}",
                elem_type, temp, elem_type, elem_ptr, align
            ));
            return Ok(format!("{} {}", elem_type, temp));
        }
        // 指针到整数转换 (ptrtoint)
        else if value_type.ends_with("*")
            && elem_type.starts_with("i")
            && !elem_type.ends_with("*")
        {
            self.emit_line(&format!(
                "  {} = ptrtoint {} {} to {}",
                temp, value_type, val, elem_type
            ));
            let align = self.get_type_align(elem_type);
            self.emit_line(&format!(
                "  store {} {}, {}* {}, align {}",
                elem_type, temp, elem_type, elem_ptr, align
            ));
            return Ok(format!("{} {}", elem_type, temp));
        }
        // 整数到指针转换 (inttoptr)
        else if value_type.starts_with("i")
            && !value_type.ends_with("*")
            && elem_type.ends_with("*")
        {
            self.emit_line(&format!(
                "  {} = inttoptr {} {} to {}",
                temp, value_type, val, elem_type
            ));
            let align = self.get_type_align(elem_type);
            self.emit_line(&format!(
                "  store {} {}, {}* {}, align {}",
                elem_type, temp, elem_type, elem_ptr, align
            ));
            return Ok(format!("{} {}", elem_type, temp));
        }
        // 整数类型转换
        else if value_type.starts_with("i")
            && elem_type.starts_with("i")
            && !value_type.ends_with("*")
            && !elem_type.ends_with("*")
        {
            let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
            let to_bits: u32 = elem_type.trim_start_matches('i').parse().unwrap_or(64);

            if to_bits > from_bits {
                // 符号扩展
                self.emit_line(&format!(
                    "  {} = sext {} {} to {}",
                    temp, value_type, val, elem_type
                ));
            } else {
                // 截断
                self.emit_line(&format!(
                    "  {} = trunc {} {} to {}",
                    temp, value_type, val, elem_type
                ));
            }
            let align = self.get_type_align(elem_type);
            self.emit_line(&format!(
                "  store {} {}, {}* {}, align {}",
                elem_type, temp, elem_type, elem_ptr, align
            ));
            return Ok(format!("{} {}", elem_type, temp));
        }

        // 默认情况：直接存储
        let align = self.get_type_align(elem_type);
        self.emit_line(&format!(
            "  store {} {}, {}* {}, align {}",
            elem_type, val, elem_type, elem_ptr, align
        ));
        Ok(value.to_string())
    }

    /// 生成解引用赋值代码 (*ptr = value)
    ///
    /// # Arguments
    /// * `unary` - 解引用一元表达式
    /// * `value_type` - 值的LLVM类型
    /// * `val` - 值的LLVM表示
    /// * `value` - 原始值字符串
    fn generate_deref_assignment(
        &mut self,
        unary: &UnaryExpr,
        value_type: &str,
        val: &str,
        value: &str,
    ) -> cayResult<String> {
        // 生成指针表达式的代码
        let ptr_result = self.generate_expression(&unary.operand)?;
        let (ptr_type, ptr_val) = self.parse_typed_value(&ptr_result);

        // 确保操作数是指针类型
        if !ptr_type.ends_with('*') {
            return Err(codegen_error_at(
                unary.loc.clone(),
                format!("Cannot dereference non-pointer type: {}", ptr_type),
            ));
        }

        // 提取元素类型（去掉末尾的*）
        let elem_type = &ptr_type[..ptr_type.len() - 1];

        // 如果值类型与元素类型不匹配，需要转换
        let final_val = if value_type != elem_type {
            let temp = self.new_temp();

            // 浮点类型转换
            if value_type == "double" && elem_type == "float" {
                self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
            } else if value_type == "float" && elem_type == "double" {
                self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
            }
            // 整数到浮点数转换
            else if value_type.starts_with("i")
                && !value_type.ends_with("*")
                && (elem_type == "float" || elem_type == "double")
            {
                self.emit_line(&format!(
                    "  {} = sitofp {} {} to {}",
                    temp, value_type, val, elem_type
                ));
            }
            // 浮点数到整数转换
            else if (value_type == "float" || value_type == "double")
                && elem_type.starts_with("i")
                && !elem_type.ends_with("*")
            {
                self.emit_line(&format!(
                    "  {} = fptosi {} {} to {}",
                    temp, value_type, val, elem_type
                ));
            }
            // 整数类型转换
            else if value_type.starts_with("i")
                && elem_type.starts_with("i")
                && !value_type.ends_with("*")
                && !elem_type.ends_with("*")
            {
                let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                let to_bits: u32 = elem_type.trim_start_matches('i').parse().unwrap_or(64);
                if to_bits > from_bits {
                    self.emit_line(&format!(
                        "  {} = sext {} {} to {}",
                        temp, value_type, val, elem_type
                    ));
                } else {
                    self.emit_line(&format!(
                        "  {} = trunc {} {} to {}",
                        temp, value_type, val, elem_type
                    ));
                }
            }
            // 指针类型转换
            else if value_type.ends_with("*") && elem_type.ends_with("*") {
                self.emit_line(&format!(
                    "  {} = bitcast {} {} to {}",
                    temp, value_type, val, elem_type
                ));
            } else {
                return Err(codegen_error_at(
                    unary.loc.clone(),
                    format!(
                        "Cannot convert {} to {} for dereference assignment",
                        value_type, elem_type
                    ),
                ));
            }
            temp
        } else {
            val.to_string()
        };

        // 存储值到指针指向的地址
        let align = self.get_type_align(elem_type);
        self.emit_line(&format!(
            "  store {} {}, {} {}, align {}",
            elem_type, final_val, ptr_type, ptr_val, align
        ));

        Ok(value.to_string())
    }
}
