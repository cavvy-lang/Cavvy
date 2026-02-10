//! 表达式代码生成
use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::types::Type;
use crate::error::{cayResult, codegen_error};

impl IRGenerator {
    /// 生成表达式代码
    pub fn generate_expression(&mut self, expr: &Expr) -> cayResult<String> {
        match expr {
            Expr::Literal(lit) => self.generate_literal(lit),
            Expr::Identifier(name) => {
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
                    let llvm_name = self.scope_manager.get_llvm_name(name).unwrap_or_else(|| name.clone());
                    (scope_type, llvm_name)
                } else {
                    // 回退到旧系统
                    let var_type = self.var_types.get(name).cloned().unwrap_or_else(|| "i64".to_string());
                    (var_type, name.clone())
                };
                let align = self.get_type_align(&var_type);  // 获取正确的对齐
                self.emit_line(&format!("  {} = load {}, {}* %{}, align {}",
                    temp, var_type, var_type, llvm_name, align));
                Ok(format!("{} {}", var_type, temp))
            }
            Expr::Binary(bin) => self.generate_binary_expression(bin),
            Expr::Unary(unary) => self.generate_unary_expression(unary),
            Expr::Call(call) => self.generate_call_expression(call),
            Expr::Assignment(assign) => self.generate_assignment(assign),
            Expr::Cast(cast) => self.generate_cast_expression(cast),
            Expr::MemberAccess(member) => self.generate_member_access(member),
            Expr::New(new_expr) => self.generate_new_expression(new_expr),
            Expr::ArrayCreation(arr) => self.generate_array_creation(arr),
            Expr::ArrayAccess(arr) => self.generate_array_access(arr),
            Expr::ArrayInit(init) => self.generate_array_init(init),
            Expr::MethodRef(method_ref) => self.generate_method_ref(method_ref),
            Expr::Lambda(lambda) => self.generate_lambda(lambda),
        }
    }

    /// 生成字面量代码
    fn generate_literal(&mut self, lit: &LiteralValue) -> cayResult<String> {
        match lit {
            LiteralValue::Int32(val) => Ok(format!("i32 {}", val)),
            LiteralValue::Int64(val) => Ok(format!("i64 {}", val)),
            LiteralValue::Float32(val) => {
                // 对于float字面量，生成double常量
                // 类型转换逻辑会将其转换为float
                // 确保浮点数常量有小数点
                let formatted = if val.fract() == 0.0 {
                    format!("double {}.0", val)
                } else {
                    format!("double {}", val)
                };
                Ok(formatted)
            }
            LiteralValue::Float64(val) => {
                // 对于double，使用十进制表示
                // 确保浮点数常量有小数点
                let formatted = if val.fract() == 0.0 {
                    format!("double {}.0", val)
                } else {
                    format!("double {}", val)
                };
                Ok(formatted)
            }
            LiteralValue::Bool(val) => Ok(format!("i1 {}", if *val { 1 } else { 0 })),
            LiteralValue::String(s) => {
                let global_name = self.get_or_create_string_constant(s);
                let temp = self.new_temp();
                let len = s.len() + 1;
                self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                    temp, len, len, global_name));
                Ok(format!("i8* {}", temp))
            }
            LiteralValue::Char(c) => Ok(format!("i8 {}", *c as u8)),
            LiteralValue::Null => Ok("i64 0".to_string()),
        }
    }

    /// 提升整数操作数到相同类型
    fn promote_integer_operands(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str) -> (String, String, String) {
        // 检查是否为指针类型（如 i8*），指针类型不参与整数提升
        let left_is_ptr = left_type.ends_with('*');
        let right_is_ptr = right_type.ends_with('*');
        
        if left_is_ptr || right_is_ptr {
            // 指针类型不应该调用此函数，返回原值以避免错误
            return (left_type.to_string(), left_val.to_string(), right_val.to_string());
        }
        
        // char (i8) 类型在算术运算中需要提升到 i32
        let target_type = if left_type == "i8" || right_type == "i8" {
            "i32"
        } else if left_type == right_type {
            return (left_type.to_string(), left_val.to_string(), right_val.to_string());
        } else {
            // 确定提升后的类型（选择位数更大的类型）
            let left_bits: u32 = left_type.trim_start_matches('i').parse().unwrap_or(64);
            let right_bits: u32 = right_type.trim_start_matches('i').parse().unwrap_or(64);
            if left_bits >= right_bits { left_type } else { right_type }
        };
        
        // 提升左操作数
        let promoted_left = if left_type != target_type {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to {}", temp, left_type, left_val, target_type));
            temp
        } else {
            left_val.to_string()
        };
        
        // 提升右操作数
        let promoted_right = if right_type != target_type {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to {}", temp, right_type, right_val, target_type));
            temp
        } else {
            right_val.to_string()
        };
        
        (target_type.to_string(), promoted_left, promoted_right)
    }
    
    /// 提升浮点操作数到相同类型
    fn promote_float_operands(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str) -> (String, String, String) {
        if left_type == right_type {
            return (left_type.to_string(), left_val.to_string(), right_val.to_string());
        }

        // 确定提升后的类型（选择精度更高的类型：double > float）
        if left_type == "double" || right_type == "double" {
            let promoted_type = "double".to_string();
            let mut promoted_left = left_val.to_string();
            let mut promoted_right = right_val.to_string();

            if left_type == "float" {
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = fpext float {} to double", temp, left_val));
                promoted_left = temp;
            }

            if right_type == "float" {
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = fpext float {} to double", temp, right_val));
                promoted_right = temp;
            }

            (promoted_type, promoted_left, promoted_right)
        } else {
            // 两者都是float，无需提升
            (left_type.to_string(), left_val.to_string(), right_val.to_string())
        }
    }

    /// 处理整数和浮点数的混合运算，将整数转换为浮点数
    fn promote_mixed_operands(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str) -> Option<(String, String, String)> {
        // 检查是否是混合类型（整数 + 浮点数）
        let left_is_int = left_type.starts_with("i") && !left_type.ends_with("*");
        let right_is_int = right_type.starts_with("i") && !right_type.ends_with("*");
        let left_is_float = left_type == "float" || left_type == "double";
        let right_is_float = right_type == "float" || right_type == "double";

        if left_is_int && right_is_float {
            // 整数 + 浮点数：将整数转换为浮点数
            let promoted_type = if right_type == "double" { "double" } else { "float" };
            let converted_left = self.new_temp();
            if promoted_type == "double" {
                self.emit_line(&format!("  {} = sitofp {} {} to double", converted_left, left_type, left_val));
            } else {
                self.emit_line(&format!("  {} = sitofp {} {} to float", converted_left, left_type, left_val));
            }
            Some((promoted_type.to_string(), converted_left, right_val.to_string()))
        } else if left_is_float && right_is_int {
            // 浮点数 + 整数：将整数转换为浮点数
            let promoted_type = if left_type == "double" { "double" } else { "float" };
            let converted_right = self.new_temp();
            if promoted_type == "double" {
                self.emit_line(&format!("  {} = sitofp {} {} to double", converted_right, right_type, right_val));
            } else {
                self.emit_line(&format!("  {} = sitofp {} {} to float", converted_right, right_type, right_val));
            }
            Some((promoted_type.to_string(), left_val.to_string(), converted_right))
        } else {
            None
        }
    }
    
    /// 生成二元表达式代码
    fn generate_binary_expression(&mut self, bin: &BinaryExpr) -> cayResult<String> {
        let left = self.generate_expression(&bin.left)?;
        let right = self.generate_expression(&bin.right)?;
        
        // 解析类型和值
        let (left_type, left_val) = self.parse_typed_value(&left);
        let (right_type, right_val) = self.parse_typed_value(&right);
        
        let temp = self.new_temp();
        
        match bin.op {
            BinaryOp::Add => {
                // 字符串拼接处理
                if left_type == "i8*" && right_type == "i8*" {
                    // 调用内建的字符串拼接函数
                    self.emit_line(&format!("  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                        temp, left_val, right_val));
                    return Ok(format!("i8* {}", temp));
                } else if left_type == "i8*" && right_type == "i8" {
                    // 字符串 + char：先将char转换为字符串，然后拼接
                    let char_as_string = self.new_temp();
                    self.emit_line(&format!("  {} = call i8* @__cay_char_to_string(i8 {})",
                        char_as_string, right_val));
                    self.emit_line(&format!("  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                        temp, left_val, char_as_string));
                    return Ok(format!("i8* {}", temp));
                } else if left_type == "i8" && right_type == "i8*" {
                    // char + 字符串：先将char转换为字符串，然后拼接
                    let char_as_string = self.new_temp();
                    self.emit_line(&format!("  {} = call i8* @__cay_char_to_string(i8 {})",
                        char_as_string, left_val));
                    self.emit_line(&format!("  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                        temp, char_as_string, right_val));
                    return Ok(format!("i8* {}", temp));
                } else if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 整数加法，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = add {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
                    // 浮点数加法，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = fadd {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else if (left_type.starts_with("i") && (right_type == "float" || right_type == "double")) {
                    // 整数 + 浮点数：将整数转换为浮点数
                    let (promoted_type, promoted_right) = if right_type == "double" { ("double", right_val.to_string()) } else { ("float", right_val.to_string()) };
                    let converted_left = self.new_temp();
                    if promoted_type == "double" {
                        self.emit_line(&format!("  {} = sitofp {} {} to double", converted_left, left_type, left_val));
                    } else {
                        self.emit_line(&format!("  {} = sitofp {} {} to float", converted_left, left_type, left_val));
                    }
                    self.emit_line(&format!("  {} = fadd {} {}, {}",
                        temp, promoted_type, converted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else if ((left_type == "float" || left_type == "double") && right_type.starts_with("i")) {
                    // 浮点数 + 整数：将整数转换为浮点数
                    let (promoted_type, promoted_left) = if left_type == "double" { ("double", left_val.to_string()) } else { ("float", left_val.to_string()) };
                    let converted_right = self.new_temp();
                    if promoted_type == "double" {
                        self.emit_line(&format!("  {} = sitofp {} {} to double", converted_right, right_type, right_val));
                    } else {
                        self.emit_line(&format!("  {} = sitofp {} {} to float", converted_right, right_type, right_val));
                    }
                    self.emit_line(&format!("  {} = fadd {} {}, {}",
                        temp, promoted_type, promoted_left, converted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Unsupported addition types: {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Sub => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 整数减法，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = sub {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
                    // 浮点数减法，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = fsub {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(&left_type, &left_val, &right_type, &right_val) {
                    // 混合类型：整数和浮点数
                    self.emit_line(&format!("  {} = fsub {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Unsupported subtraction types: {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Mul => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 整数乘法，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = mul {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
                    // 浮点数乘法，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = fmul {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(&left_type, &left_val, &right_type, &right_val) {
                    // 混合类型：整数和浮点数
                    self.emit_line(&format!("  {} = fmul {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Unsupported multiplication types: {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Div => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 整数除法，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = sdiv {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
                    // 浮点数除法，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = fdiv {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(&left_type, &left_val, &right_type, &right_val) {
                    // 混合类型：整数和浮点数
                    self.emit_line(&format!("  {} = fdiv {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Unsupported division types: {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Mod => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 整数取模，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = srem {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Unsupported modulo types: {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Eq => {
                if left_type == "i8*" && right_type == "i8*" {
                    // 字符串比较
                    self.emit_line(&format!("  {} = icmp eq i8* {}, {}", temp, left_val, right_val));
                    return Ok(format!("i1 {}", temp));
                } else if left_type.starts_with("i") && right_type.starts_with("i") {
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = icmp eq {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
                    let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = fcmp oeq {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(&left_type, &left_val, &right_type, &right_val) {
                    // 混合类型：整数和浮点数
                    self.emit_line(&format!("  {} = fcmp oeq {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else {
                    return Err(codegen_error(format!("Unsupported equality comparison types: {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Ne => {
                if left_type == "i8*" && right_type == "i8*" {
                    self.emit_line(&format!("  {} = icmp ne i8* {}, {}", temp, left_val, right_val));
                    return Ok(format!("i1 {}", temp));
                } else if left_type.starts_with("i") && right_type.starts_with("i") {
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = icmp ne {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
                    let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = fcmp one {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(&left_type, &left_val, &right_type, &right_val) {
                    // 混合类型：整数和浮点数
                    self.emit_line(&format!("  {} = fcmp one {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else {
                    return Err(codegen_error(format!("Unsupported inequality comparison types: {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Lt => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = icmp slt {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
                    let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = fcmp olt {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(&left_type, &left_val, &right_type, &right_val) {
                    // 混合类型：整数和浮点数
                    self.emit_line(&format!("  {} = fcmp olt {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else {
                    return Err(codegen_error(format!("Unsupported less-than comparison types: {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Le => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = icmp sle {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
                    let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = fcmp ole {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(&left_type, &left_val, &right_type, &right_val) {
                    // 混合类型：整数和浮点数
                    self.emit_line(&format!("  {} = fcmp ole {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("i1 {}", temp));
                } else {
                    return Err(codegen_error(format!("Unsupported less-or-equal comparison types: {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Gt => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 整数大于比较，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = icmp sgt {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
                    // 浮点数大于比较，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = fcmp ogt {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(&left_type, &left_val, &right_type, &right_val) {
                    // 混合类型：整数和浮点数
                    self.emit_line(&format!("  {} = fcmp ogt {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                } else {
                    return Err(codegen_error(format!("Unsupported greater-than comparison types: {} and {}", left_type, right_type)));
                }
                return Ok(format!("i1 {}", temp));
            }
            BinaryOp::Ge => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 整数大于等于比较，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = icmp sge {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
                    // 浮点数大于等于比较，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = fcmp oge {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(&left_type, &left_val, &right_type, &right_val) {
                    // 混合类型：整数和浮点数
                    self.emit_line(&format!("  {} = fcmp oge {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
                } else {
                    return Err(codegen_error(format!("Unsupported greater-than-or-equal comparison types: {} and {}", left_type, right_type)));
                }
                return Ok(format!("i1 {}", temp));
            }
            BinaryOp::And => {
                self.emit_line(&format!("  {} = and {} {}, {}", 
                    temp, left_type, left_val, right_val));
                return Ok(format!("i1 {}", temp));
            }
            BinaryOp::Or => {
                self.emit_line(&format!("  {} = or {} {}, {}",
                    temp, left_type, left_val, right_val));
                return Ok(format!("i1 {}", temp));
            }
            BinaryOp::BitAnd => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 位与，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = and {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Bitwise AND requires integer operands, got {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::BitOr => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 位或，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = or {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Bitwise OR requires integer operands, got {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::BitXor => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 位异或，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = xor {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Bitwise XOR requires integer operands, got {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Shl => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 左移，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = shl {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Shift left requires integer operands, got {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::Shr => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 算术右移，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = ashr {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Arithmetic shift right requires integer operands, got {} and {}", left_type, right_type)));
                }
            }
            BinaryOp::UnsignedShr => {
                if left_type.starts_with("i") && right_type.starts_with("i") {
                    // 逻辑右移，需要类型提升
                    let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(&left_type, &left_val, &right_type, &right_val);
                    self.emit_line(&format!("  {} = lshr {} {}, {}",
                        temp, promoted_type, promoted_left, promoted_right));
                    return Ok(format!("{} {}", promoted_type, temp));
                } else {
                    return Err(codegen_error(format!("Unsigned shift right requires integer operands, got {} and {}", left_type, right_type)));
                }
            }
        }
    }

    /// 生成一元表达式代码
    fn generate_unary_expression(&mut self, unary: &UnaryExpr) -> cayResult<String> {
        let operand = self.generate_expression(&unary.operand)?;
        let (op_type, op_val) = self.parse_typed_value(&operand);
        let temp = self.new_temp();
        
        match unary.op {
            UnaryOp::Neg => {
                if op_type.starts_with("i") {
                    self.emit_line(&format!("  {} = sub {} 0, {}",
                        temp, op_type, op_val));
                } else {
                    self.emit_line(&format!("  {} = fneg {} {}",
                        temp, op_type, op_val));
                }
            }
            UnaryOp::Not => {
                self.emit_line(&format!("  {} = xor {} {}, 1",
                    temp, op_type, op_val));
                return Ok(format!("i1 {}", temp));
            }
            UnaryOp::BitNot => {
                // 位取反：xor 操作数与 -1
                if op_type.starts_with("i") {
                    self.emit_line(&format!("  {} = xor {} {}, -1",
                        temp, op_type, op_val));
                } else {
                    // 浮点数不支持位取反，但类型系统应该已经阻止了这种情况
                    return Err(codegen_error("Bitwise NOT not supported for floating point".to_string()));
                }
            }
            UnaryOp::PreInc | UnaryOp::PostInc | UnaryOp::PreDec | UnaryOp::PostDec => {
                // 自增/自减操作：需要先获取变量地址，加载值，计算，存储
                let is_inc = unary.op == UnaryOp::PreInc || unary.op == UnaryOp::PostInc;
                let is_pre = unary.op == UnaryOp::PreInc || unary.op == UnaryOp::PreDec;
                
                // 获取正确的变量类型和指针
                let (llvm_type, llvm_ptr) = self.get_lvalue_info(&unary.operand)?;
                
                // 加载当前值
                let load_temp = self.new_temp();
                self.emit_line(&format!("  {} = load {}, {}* {}, align {}",
                    load_temp, llvm_type, llvm_type, llvm_ptr, self.get_type_align(&llvm_type)));
                
                // 计算新值
                let new_temp = self.new_temp();
                let one = if llvm_type == "float" || llvm_type == "double" { "1.0" } else { "1" };
                if llvm_type == "float" || llvm_type == "double" {
                    if is_inc {
                        self.emit_line(&format!("  {} = fadd {} {}, {}",
                            new_temp, llvm_type, load_temp, one));
                    } else {
                        self.emit_line(&format!("  {} = fsub {} {}, {}",
                            new_temp, llvm_type, load_temp, one));
                    }
                } else {
                    if is_inc {
                        self.emit_line(&format!("  {} = add {} {}, {}",
                            new_temp, llvm_type, load_temp, one));
                    } else {
                        self.emit_line(&format!("  {} = sub {} {}, {}",
                            new_temp, llvm_type, load_temp, one));
                    }
                }
                
                // 存储新值
                self.emit_line(&format!("  store {} {}, {}* {}, align {}",
                    llvm_type, new_temp, llvm_type, llvm_ptr, self.get_type_align(&llvm_type)));
                
                // 前置返回新值，后缀返回旧值
                return if is_pre {
                    Ok(format!("{} {}", llvm_type, new_temp))
                } else {
                    Ok(format!("{} {}", llvm_type, load_temp))
                };
            }
        }
        
        Ok(format!("{} {}", op_type, temp))
    }

    /// 生成函数调用表达式代码
    fn generate_call_expression(&mut self, call: &CallExpr) -> cayResult<String> {
        // 处理 print 和 println 函数
        if let Expr::Identifier(name) = call.callee.as_ref() {
            if name == "print" {
                return self.generate_print_call(&call.args, false);
            }
            if name == "println" {
                return self.generate_print_call(&call.args, true);
            }
            if name == "readInt" {
                return self.generate_read_int_call(&call.args);
            }
            if name == "readFloat" {
                return self.generate_read_float_call(&call.args);
            }
            if name == "readLine" {
                return self.generate_read_line_call(&call.args);
            }
        }

        // 处理 String 方法调用: str.method(args)
        if let Expr::MemberAccess(member) = call.callee.as_ref() {
            // 检查是否是 String 方法调用
            if let Some(method_result) = self.try_generate_string_method_call(member, &call.args)? {
                return Ok(method_result);
            }
        }

        // 处理普通函数调用（支持方法重载和可变参数）
        // 先确定方法信息（类名和方法名）
        let (class_name, method_name) = match call.callee.as_ref() {
            Expr::Identifier(name) => {
                if !self.current_class.is_empty() {
                    (self.current_class.clone(), name.clone())
                } else {
                    (String::new(), name.clone())
                }
            }
            Expr::MemberAccess(member) => {
                if let Expr::Identifier(obj_name) = member.object.as_ref() {
                    let class_name = self.var_class_map.get(obj_name)
                        .cloned()
                        .unwrap_or_else(|| obj_name.clone());
                    (class_name, member.member.clone())
                } else {
                    return Err(codegen_error("Invalid method call".to_string()));
                }
            }
            _ => return Err(codegen_error("Invalid function call".to_string())),
        };

        // 检查是否是可变参数方法（根据方法名推断）
        let is_varargs_method = self.is_varargs_method(&class_name, &method_name);

        // 先生成参数以获取参数类型
        let mut arg_results = Vec::new();
        for arg in &call.args {
            arg_results.push(self.generate_expression(arg)?);
        }

        // 处理可变参数：将多余参数打包成数组
        let (processed_args, has_varargs_array) = if is_varargs_method {
            let packed = self.pack_varargs_args(&class_name, &method_name, &arg_results)?;
            // 如果原始参数多于固定参数数量，说明创建了数组
            let fixed_count = match method_name.as_str() {
                "sum" => 0,
                "printAll" => 1,
                "multiplyAndAdd" => 1,
                _ => 0,
            };
            let has_array = arg_results.len() > fixed_count;
            (packed, has_array)
        } else {
            (arg_results, false)
        };

        // 生成函数名 - 使用类型注册表获取方法定义的参数类型
        let fn_name = self.generate_function_name(&class_name, &method_name, &processed_args, has_varargs_array);

        // 转换参数类型
        let mut converted_args = Vec::new();
        for arg_str in &processed_args {
            // 保持参数类型不变，不进行转换
            converted_args.push(arg_str.clone());
        }

        // 获取方法的返回类型
        let ret_type = self.get_method_return_type(&class_name, &method_name, &processed_args, has_varargs_array);
        let llvm_ret_type = self.type_to_llvm(&ret_type);
        
        if llvm_ret_type == "void" {
            // void 方法调用不需要命名结果
            self.emit_line(&format!("  call void @{}({})",
                fn_name, converted_args.join(", ")));
            Ok("void %dummy".to_string())
        } else {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = call {} @{}({})",
                temp, llvm_ret_type, fn_name, converted_args.join(", ")));
            Ok(format!("{} {}", llvm_ret_type, temp))
        }
    }

    /// 生成函数名 - 优先使用类型注册表中方法定义的参数类型
    fn generate_function_name(&self, class_name: &str, method_name: &str, processed_args: &[String], has_varargs_array: bool) -> String {
        // 获取实际参数的类型签名
        let arg_types: Vec<String> = processed_args.iter()
            .enumerate()
            .map(|(idx, r)| {
                let (ty, _) = self.parse_typed_value(r);
                let is_varargs_array = has_varargs_array && idx == processed_args.len() - 1;
                let llvm_type = self.llvm_type_to_signature(&ty);
                if is_varargs_array {
                    "ai".to_string()
                } else {
                    llvm_type
                }
            })
            .collect();
        
        // 尝试从类型注册表获取方法信息
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    let arg_count = processed_args.len();
                    
                    // 首先尝试找到参数类型完全匹配的方法
                    for method in methods {
                        let param_count = method.params.len();
                        let is_varargs = method.params.last().map(|p| p.is_varargs).unwrap_or(false);
                        
                        if is_varargs {
                            // 可变参数方法
                            let fixed_count = param_count.saturating_sub(1);
                            if arg_count >= fixed_count {
                                // 检查固定参数类型是否匹配
                                let method_sig = self.build_function_name_from_method(class_name, method_name, &method.params, has_varargs_array);
                                let expected_sig = format!("{}.__{}_{}", class_name, method_name, arg_types.join("_"));
                                if method_sig == expected_sig {
                                    return method_sig;
                                }
                            }
                        } else if param_count == arg_count {
                            // 非可变参数方法：检查参数类型是否匹配
                            let method_sig = self.build_function_name_from_method(class_name, method_name, &method.params, has_varargs_array);
                            let expected_sig = format!("{}.__{}_{}", class_name, method_name, arg_types.join("_"));
                            if method_sig == expected_sig {
                                return method_sig;
                            }
                        }
                    }
                    
                    // 如果没有找到类型完全匹配的方法，回退到参数数量匹配
                    for method in methods {
                        let param_count = method.params.len();
                        let is_varargs = method.params.last().map(|p| p.is_varargs).unwrap_or(false);
                        
                        if is_varargs {
                            let fixed_count = param_count.saturating_sub(1);
                            if arg_count >= fixed_count {
                                return self.build_function_name_from_method(class_name, method_name, &method.params, has_varargs_array);
                            }
                        } else if param_count == arg_count {
                            return self.build_function_name_from_method(class_name, method_name, &method.params, has_varargs_array);
                        }
                    }
                }
            }
        }

        // 回退到使用实际参数类型生成函数名
        if arg_types.is_empty() {
            format!("{}.{}", class_name, method_name)
        } else {
            format!("{}.__{}_{}", class_name, method_name, arg_types.join("_"))
        }
    }

    /// 根据方法定义的参数类型构建函数名
    fn build_function_name_from_method(&self, class_name: &str, method_name: &str, params: &[crate::types::ParameterInfo], has_varargs_array: bool) -> String {
        if params.is_empty() {
            return format!("{}.{}", class_name, method_name);
        }

        let param_types: Vec<String> = params.iter()
            .enumerate()
            .map(|(idx, p)| {
                let is_last_varargs = has_varargs_array && idx == params.len() - 1 && p.is_varargs;
                self.param_type_to_signature(&p.param_type, is_last_varargs)
            })
            .collect();

        format!("{}.__{}_{}", class_name, method_name, param_types.join("_"))
    }

    /// 将参数类型转换为签名
    fn param_type_to_signature(&self, ty: &crate::types::Type, is_varargs_array: bool) -> String {
        if is_varargs_array {
            return "ai".to_string(); // 可变参数数组签名
        }

        match ty {
            crate::types::Type::Int32 => "i".to_string(),
            crate::types::Type::Int64 => "l".to_string(),
            crate::types::Type::Float32 => "f".to_string(),
            crate::types::Type::Float64 => "d".to_string(),
            crate::types::Type::Bool => "b".to_string(),
            crate::types::Type::String => "s".to_string(),
            crate::types::Type::Char => "c".to_string(),
            crate::types::Type::Object(name) => format!("o{}", name),
            crate::types::Type::Array(inner) => format!("a{}", self.param_type_to_signature(inner, false)),
            _ => "x".to_string(),
        }
    }

    /// 获取方法的返回类型
    fn get_method_return_type(&self, class_name: &str, method_name: &str, processed_args: &[String], has_varargs_array: bool) -> crate::types::Type {
        // 获取实际参数的类型签名
        let arg_types: Vec<String> = processed_args.iter()
            .enumerate()
            .map(|(idx, r)| {
                let (ty, _) = self.parse_typed_value(r);
                let is_varargs_array = has_varargs_array && idx == processed_args.len() - 1;
                let llvm_type = self.llvm_type_to_signature(&ty);
                if is_varargs_array {
                    "ai".to_string()
                } else {
                    llvm_type
                }
            })
            .collect();
        
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    let arg_count = processed_args.len();
                    
                    // 首先尝试找到参数类型完全匹配的方法
                    for method in methods {
                        let param_count = method.params.len();
                        let is_varargs = method.params.last().map(|p| p.is_varargs).unwrap_or(false);
                        
                        if is_varargs {
                            let fixed_count = param_count.saturating_sub(1);
                            if arg_count >= fixed_count {
                                let method_sig = self.build_function_name_from_method(class_name, method_name, &method.params, has_varargs_array);
                                let expected_sig = format!("{}.__{}_{}", class_name, method_name, arg_types.join("_"));
                                if method_sig == expected_sig {
                                    return method.return_type.clone();
                                }
                            }
                        } else if param_count == arg_count {
                            let method_sig = self.build_function_name_from_method(class_name, method_name, &method.params, has_varargs_array);
                            let expected_sig = format!("{}.__{}_{}", class_name, method_name, arg_types.join("_"));
                            if method_sig == expected_sig {
                                return method.return_type.clone();
                            }
                        }
                    }
                    
                    // 如果没有找到类型完全匹配的方法，回退到参数数量匹配
                    for method in methods {
                        let param_count = method.params.len();
                        let is_varargs = method.params.last().map(|p| p.is_varargs).unwrap_or(false);
                        
                        if is_varargs {
                            let fixed_count = param_count.saturating_sub(1);
                            if arg_count >= fixed_count {
                                return method.return_type.clone();
                            }
                        } else if param_count == arg_count {
                            return method.return_type.clone();
                        }
                    }
                }
            }
        }
        
        // 默认返回 i64 类型
        crate::types::Type::Int64
    }

    /// 检查方法是否是可变参数方法
    /// 查询类型注册表来确定方法是否真的是可变参数方法
    fn is_varargs_method(&self, class_name: &str, method_name: &str) -> bool {
        // 查询类型注册表
        if let Some(ref registry) = self.type_registry {
            if let Some(class_info) = registry.get_class(class_name) {
                if let Some(methods) = class_info.methods.get(method_name) {
                    // 检查是否有任何方法是可变参数的
                    for method in methods {
                        if method.params.last().map(|p| p.is_varargs).unwrap_or(false) {
                            return true;
                        }
                    }
                }
            }
        }
        // 默认返回false，避免将普通方法误认为可变参数方法
        false
    }

    /// 将可变参数打包成数组
    /// fixed_param_count: 固定参数的数量
    fn pack_varargs_args(&mut self, _class_name: &str, method_name: &str, arg_results: &[String]) -> cayResult<Vec<String>> {
        // 确定固定参数数量（这里需要根据实际方法定义来确定）
        let fixed_param_count = match method_name {
            "sum" => 0,  // sum(int... numbers) 没有固定参数
            "printAll" => 1,  // printAll(string prefix, int... numbers) 有1个固定参数
            "multiplyAndAdd" => 1,  // multiplyAndAdd(int multiplier, int... numbers) 有1个固定参数
            _ => 0,
        };

        if arg_results.len() <= fixed_param_count {
            // 参数数量不足或刚好，不需要打包
            return Ok(arg_results.to_vec());
        }

        // 分割固定参数和可变参数
        let fixed_args = &arg_results[..fixed_param_count];
        let varargs = &arg_results[fixed_param_count..];

        // 创建数组来存储可变参数
        let array_size = varargs.len();
        let array_type = "i32";  // 假设可变参数是 int 类型
        let array_ptr = self.new_temp();

        // 分配数组内存
        let elem_size = 4;  // i32 占 4 字节
        let total_size = array_size * elem_size;
        self.emit_line(&format!("  {} = call i8* @calloc(i64 1, i64 {})", array_ptr, total_size));

        // 将可变参数存入数组
        for (i, arg_str) in varargs.iter().enumerate() {
            let (arg_type, arg_val) = self.parse_typed_value(arg_str);
            let elem_ptr_i8 = self.new_temp();
            let elem_ptr_i32 = self.new_temp();
            let offset = i * elem_size;

            // 计算元素地址 (i8*)
            self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 {}", elem_ptr_i8, array_ptr, offset));

            // 将 i8* 转换为 i32*
            self.emit_line(&format!("  {} = bitcast i8* {} to i32*", elem_ptr_i32, elem_ptr_i8));

            // 将值转换为 i32 并存储
            if arg_type == "i64" {
                let truncated = self.new_temp();
                self.emit_line(&format!("  {} = trunc i64 {} to i32", truncated, arg_val));
                self.emit_line(&format!("  store i32 {}, i32* {}, align 4", truncated, elem_ptr_i32));
            } else if arg_type == "i32" {
                self.emit_line(&format!("  store i32 {}, i32* {}, align 4", arg_val, elem_ptr_i32));
            }
        }

        // 构建结果：固定参数 + 数组指针
        let mut result = fixed_args.to_vec();
        result.push(format!("i8* {}", array_ptr));

        Ok(result)
    }

    /// 将 LLVM 类型转换为方法签名
    fn llvm_type_to_signature(&self, llvm_type: &str) -> String {
        match llvm_type {
            "i32" => "i".to_string(),
            "i64" => "l".to_string(),
            "float" => "f".to_string(),
            "double" => "d".to_string(),
            "i1" => "b".to_string(),
            "i8*" => "s".to_string(),
            "i8" => "c".to_string(),
            t if t.ends_with("*") => "o".to_string(), // 对象/数组指针
            _ => "x".to_string(), // 未知类型
        }
    }

    /// 将 LLVM 类型转换为方法签名（支持可变参数数组类型）
    fn llvm_type_to_signature_with_varargs(&self, llvm_type: &str, is_varargs_array: bool) -> String {
        if is_varargs_array {
            // 可变参数数组使用 ai 签名（array of int）
            "ai".to_string()
        } else {
            self.llvm_type_to_signature(llvm_type)
        }
    }

    /// 尝试生成 String 方法调用代码
    /// 返回 Some(result) 如果成功处理，None 如果不是 String 方法
    fn try_generate_string_method_call(&mut self, member: &MemberAccessExpr, args: &[Expr]) -> cayResult<Option<String>> {
        // 生成对象表达式（字符串）
        let obj_result = self.generate_expression(&member.object)?;
        let (obj_type, obj_val) = self.parse_typed_value(&obj_result);

        // 检查对象是否是字符串类型 (i8*)
        if obj_type != "i8*" {
            return Ok(None);
        }

        let method_name = member.member.as_str();
        let temp = self.new_temp();

        match method_name {
            "length" => {
                // length() - 无参数，返回 i32
                if !args.is_empty() {
                    return Err(codegen_error("String.length() takes no arguments".to_string()));
                }
                self.emit_line(&format!("  {} = call i32 @__cay_string_length(i8* {})",
                    temp, obj_val));
                Ok(Some(format!("i32 {}", temp)))
            }
            "substring" => {
                // substring(beginIndex) 或 substring(beginIndex, endIndex)
                if args.is_empty() || args.len() > 2 {
                    return Err(codegen_error("String.substring() takes 1 or 2 arguments".to_string()));
                }

                // 生成 beginIndex 参数
                let begin_result = self.generate_expression(&args[0])?;
                let (begin_type, begin_val) = self.parse_typed_value(&begin_result);
                let begin_i32 = if begin_type == "i32" {
                    begin_val.to_string()
                } else {
                    let t = self.new_temp();
                    self.emit_line(&format!("  {} = trunc {} {} to i32", t, begin_type, begin_val));
                    t
                };

                // 生成 endIndex 参数
                let end_i32 = if args.len() == 2 {
                    let end_result = self.generate_expression(&args[1])?;
                    let (end_type, end_val) = self.parse_typed_value(&end_result);
                    if end_type == "i32" {
                        end_val.to_string()
                    } else {
                        let t = self.new_temp();
                        self.emit_line(&format!("  {} = trunc {} {} to i32", t, end_type, end_val));
                        t
                    }
                } else {
                    // substring(beginIndex) - 使用字符串长度作为 endIndex
                    let len_temp = self.new_temp();
                    self.emit_line(&format!("  {} = call i32 @__cay_string_length(i8* {})",
                        len_temp, obj_val));
                    len_temp
                };

                self.emit_line(&format!("  {} = call i8* @__cay_string_substring(i8* {}, i32 {}, i32 {})",
                    temp, obj_val, begin_i32, end_i32));
                Ok(Some(format!("i8* {}", temp)))
            }
            "indexOf" => {
                // indexOf(substr) - 返回子串首次出现的位置
                if args.len() != 1 {
                    return Err(codegen_error("String.indexOf() takes 1 argument".to_string()));
                }

                let substr_result = self.generate_expression(&args[0])?;
                let (substr_type, substr_val) = self.parse_typed_value(&substr_result);

                if substr_type != "i8*" {
                    return Err(codegen_error("String.indexOf() argument must be a string".to_string()));
                }

                self.emit_line(&format!("  {} = call i32 @__cay_string_indexof(i8* {}, i8* {})",
                    temp, obj_val, substr_val));
                Ok(Some(format!("i32 {}", temp)))
            }
            "charAt" => {
                // charAt(index) - 返回指定位置的字符
                if args.len() != 1 {
                    return Err(codegen_error("String.charAt() takes 1 argument".to_string()));
                }

                let index_result = self.generate_expression(&args[0])?;
                let (index_type, index_val) = self.parse_typed_value(&index_result);
                let index_i32 = if index_type == "i32" {
                    index_val.to_string()
                } else {
                    let t = self.new_temp();
                    self.emit_line(&format!("  {} = trunc {} {} to i32", t, index_type, index_val));
                    t
                };

                self.emit_line(&format!("  {} = call i8 @__cay_string_charat(i8* {}, i32 {})",
                    temp, obj_val, index_i32));
                Ok(Some(format!("i8 {}", temp)))
            }
            "replace" => {
                // replace(oldStr, newStr) - 替换所有出现的子串
                if args.len() != 2 {
                    return Err(codegen_error("String.replace() takes 2 arguments".to_string()));
                }

                let old_result = self.generate_expression(&args[0])?;
                let (old_type, old_val) = self.parse_typed_value(&old_result);
                let new_result = self.generate_expression(&args[1])?;
                let (new_type, new_val) = self.parse_typed_value(&new_result);

                if old_type != "i8*" || new_type != "i8*" {
                    return Err(codegen_error("String.replace() arguments must be strings".to_string()));
                }

                self.emit_line(&format!("  {} = call i8* @__cay_string_replace(i8* {}, i8* {}, i8* {})",
                    temp, obj_val, old_val, new_val));
                Ok(Some(format!("i8* {}", temp)))
            }
            _ => Ok(None), // 不是已知的 String 方法
        }
    }

    /// 生成 print/println 调用代码
    fn generate_print_call(&mut self, args: &[Expr], newline: bool) -> cayResult<String> {
        if args.is_empty() {
            // 无参数，仅打印换行符（如果是 println）或什么都不做（如果是 print）
            if newline {
                // 打印一个空字符串加上换行符
                let fmt_str = "\n";
                let fmt_name = self.get_or_create_string_constant(fmt_str);
                let fmt_len = fmt_str.len() + 1;
                let fmt_ptr = self.new_temp();
                self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                    fmt_ptr, fmt_len, fmt_len, fmt_name));
                self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {})", fmt_ptr));
            }
            // 对于 print 无参数，什么都不做
            return Ok("void".to_string());
        }
        
        let first_arg = &args[0];
        
        match first_arg {
            Expr::Literal(LiteralValue::String(s)) => {
                let global_name = self.get_or_create_string_constant(s);
                let fmt_str = if newline { "%s\n" } else { "%s" };
                let fmt_name = self.get_or_create_string_constant(fmt_str);
                let len = s.len() + 1;
                let fmt_len = fmt_str.len() + 1; // 加上null终止符
                
                let str_ptr = self.new_temp();
                let fmt_ptr = self.new_temp();
                
                self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                    str_ptr, len, len, global_name));
                self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                    fmt_ptr, fmt_len, fmt_len, fmt_name));
                
                self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i8* {})",
                    fmt_ptr, str_ptr));
            }
            Expr::Literal(LiteralValue::Int32(_)) | Expr::Literal(LiteralValue::Int64(_)) => {
                let value = self.generate_expression(first_arg)?;
                let (type_str, val) = self.parse_typed_value(&value);
                let i64_fmt = self.get_i64_format_specifier();
                let fmt_str = if newline { format!("{}\n", i64_fmt) } else { i64_fmt.to_string() };
                let fmt_name = self.get_or_create_string_constant(&fmt_str);
                let fmt_len = fmt_str.len() + 1;

                let fmt_ptr = self.new_temp();
                self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                    fmt_ptr, fmt_len, fmt_len, fmt_name));

                // 如果类型不是 i64，需要扩展
                let final_val = if type_str != "i64" {
                    let ext_temp = self.new_temp();
                    self.emit_line(&format!("  {} = sext {} {} to i64", ext_temp, type_str, val));
                    ext_temp
                } else {
                    val.to_string()
                };

                self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i64 {})",
                    fmt_ptr, final_val));
            }
            _ => {
                // 根据类型决定格式字符串
                let value = self.generate_expression(first_arg)?;
                let (type_str, val) = self.parse_typed_value(&value);
                
                if type_str == "i8*" {
                    // 字符串指针类型
                    let fmt_str = if newline { "%s\n" } else { "%s" };
                    let fmt_name = self.get_or_create_string_constant(fmt_str);
                    let fmt_len = fmt_str.len() + 1;
                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));
                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i8* {})",
                        fmt_ptr, val));
                } else if type_str.starts_with("i") && type_str != "i8*" {
                    // 整数类型（排除i8*）
                    // 需要将整数扩展为 i64 以匹配格式
                    let i64_fmt = self.get_i64_format_specifier();
                    let fmt_str = if newline { format!("{}\n", i64_fmt) } else { i64_fmt.to_string() };
                    let fmt_name = self.get_or_create_string_constant(&fmt_str);
                    let fmt_len = fmt_str.len() + 1;
                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));

                    // 如果类型不是 i64，需要扩展
                    let final_val = if type_str != "i64" {
                        let ext_temp = self.new_temp();
                        self.emit_line(&format!("  {} = sext {} {} to i64", ext_temp, type_str, val));
                        ext_temp
                    } else {
                        val.to_string()
                    };

                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, i64 {})",
                        fmt_ptr, final_val));
                } else if type_str == "double" || type_str == "float" {
                    // 浮点数类型
                    let fmt_str = if newline { "%f\n" } else { "%f" };
                    let fmt_name = self.get_or_create_string_constant(fmt_str);
                    let fmt_len = fmt_str.len() + 1;
                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));
                    
                    // 如果类型是float，需要转换为double
                    let final_val = if type_str == "float" {
                        let ext_temp = self.new_temp();
                        self.emit_line(&format!("  {} = fpext float {} to double", ext_temp, val));
                        ext_temp
                    } else {
                        val.to_string()
                    };
                    
                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, double {})",
                        fmt_ptr, final_val));
                } else {
                    // 默认作为字符串处理
                    let fmt_str = if newline { "%s\n" } else { "%s" };
                    let fmt_name = self.get_or_create_string_constant(fmt_str);
                    let fmt_len = fmt_str.len() + 1;
                    let fmt_ptr = self.new_temp();
                    self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
                        fmt_ptr, fmt_len, fmt_len, fmt_name));
                    self.emit_line(&format!("  call i32 (i8*, ...) @printf(i8* {}, {})",
                        fmt_ptr, value));
                }
            }
        }
        
        Ok("i64 0".to_string())
    }

    /// 生成 readInt 调用代码
    fn generate_read_int_call(&mut self, args: &[Expr]) -> cayResult<String> {
        // readInt 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error("readInt() takes no arguments".to_string()));
        }
        
        // 为输入缓冲区分配空间
        let buffer_size = 32; // 足够存储整数
        let buffer_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca [{} x i8], align 1", buffer_temp, buffer_size));
        
        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, buffer_size, buffer_size, buffer_temp));
        
        // 调用 scanf 读取整数
        let fmt_str = self.get_i64_format_specifier();
        let fmt_name = self.get_or_create_string_constant(fmt_str);
        let fmt_len = fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name));
        
        // 为整数结果分配空间
        let int_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca i64, align 8", int_temp));
        
        // 调用 scanf
        self.emit_line(&format!("  call i32 (i8*, ...) @scanf(i8* {}, i64* {})",
            fmt_ptr, int_temp));
        
        // 加载读取的整数值
        let result_temp = self.new_temp();
        self.emit_line(&format!("  {} = load i64, i64* {}, align 8", result_temp, int_temp));
        
        Ok(format!("i64 {}", result_temp))
    }

    /// 生成 readFloat 调用代码
    fn generate_read_float_call(&mut self, args: &[Expr]) -> cayResult<String> {
        // readFloat 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error("readFloat() takes no arguments".to_string()));
        }
        
        // 为浮点数结果分配空间
        let float_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca double, align 8", float_temp));
        
        // 调用 scanf 读取浮点数
        let fmt_str = "%lf";
        let fmt_name = self.get_or_create_string_constant(fmt_str);
        let fmt_len = fmt_str.len() + 1;
        let fmt_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            fmt_ptr, fmt_len, fmt_len, fmt_name));
        
        // 调用 scanf
        self.emit_line(&format!("  call i32 (i8*, ...) @scanf(i8* {}, double* {})",
            fmt_ptr, float_temp));
        
        // 加载读取的浮点数值
        let result_temp = self.new_temp();
        self.emit_line(&format!("  {} = load double, double* {}, align 8", result_temp, float_temp));
        
        Ok(format!("double {}", result_temp))
    }

    /// 生成 readLine 调用代码
    fn generate_read_line_call(&mut self, args: &[Expr]) -> cayResult<String> {
        // readLine 应该没有参数
        if !args.is_empty() {
            return Err(codegen_error("readLine() takes no arguments".to_string()));
        }
        
        // 为输入缓冲区分配空间（假设最大256字符）
        let buffer_size = 256;
        let buffer_temp = self.new_temp();
        self.emit_line(&format!("  {} = alloca [{} x i8], align 1", buffer_temp, buffer_size));
        
        // 获取缓冲区指针
        let buffer_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr [{} x i8], [{} x i8]* {}, i64 0, i64 0",
            buffer_ptr, buffer_size, buffer_size, buffer_temp));
        
        // 调用 fgets 读取一行
        let stdin_name = self.get_or_create_string_constant("stdin");
        let stdin_ptr = self.new_temp();
        self.emit_line(&format!("  {} = load i8*, i8** {}, align 8", stdin_ptr, stdin_name));
        
        self.emit_line(&format!("  call i8* @fgets(i8* {}, i32 {}, i8* {})",
            buffer_ptr, buffer_size, stdin_ptr));
        
        // 移除换行符（如果需要）
        // 这里我们直接返回缓冲区指针
        Ok(format!("i8* {}", buffer_ptr))
    }

    /// 生成赋值表达式代码
    fn generate_assignment(&mut self, assign: &AssignmentExpr) -> cayResult<String> {
        let value = self.generate_expression(&assign.value)?;
        let (value_type, val) = self.parse_typed_value(&value);
        
        match assign.target.as_ref() {
            Expr::MemberAccess(member) => {
                // 检查是否是静态字段赋值: ClassName.fieldName = value
                if let Expr::Identifier(class_name) = &*member.object {
                    let static_key = format!("{}.{}", class_name, member.member);
                    if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                        // 静态字段赋值
                        let align = self.get_type_align(&field_info.llvm_type);
                        
                        // 如果值类型与字段类型不匹配，需要转换
                        if value_type != field_info.llvm_type {
                            let temp = self.new_temp();
                            // 类型转换逻辑（简化版）
                            if value_type.starts_with("i") && field_info.llvm_type.starts_with("i") {
                                let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                                let to_bits: u32 = field_info.llvm_type.trim_start_matches('i').parse().unwrap_or(64);
                                if to_bits > from_bits {
                                    self.emit_line(&format!("  {} = sext {} {} to {}",
                                        temp, value_type, val, field_info.llvm_type));
                                } else {
                                    self.emit_line(&format!("  {} = trunc {} {} to {}",
                                        temp, value_type, val, field_info.llvm_type));
                                }
                                self.emit_line(&format!("  store {} {}, {}* {}, align {}", 
                                    field_info.llvm_type, temp, field_info.llvm_type, field_info.name, align));
                                return Ok(format!("{} {}", field_info.llvm_type, temp));
                            }
                        }
                        
                        // 类型匹配，直接存储
                        self.emit_line(&format!("  store {} {}, {}* {}, align {}", 
                            value_type, val, field_info.llvm_type, field_info.name, align));
                        return Ok(value);
                    }
                }
                Err(codegen_error("Invalid member access assignment target".to_string()))
            }
            Expr::Identifier(name) => {
                // 优先使用作用域管理器获取变量类型和 LLVM 名称
                let (var_type, llvm_name) = if let Some(scope_type) = self.scope_manager.get_var_type(name) {
                    let llvm_name = self.scope_manager.get_llvm_name(name).unwrap_or_else(|| name.clone());
                    (scope_type, llvm_name)
                } else {
                    // 检查是否是当前类的静态字段
                    if !self.current_class.is_empty() {
                        let static_key = format!("{}.{}", self.current_class, name);
                        if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                            let align = self.get_type_align(&field_info.llvm_type);
                            self.emit_line(&format!("  store {} {}, {}* {}, align {}",
                                field_info.llvm_type, val, field_info.llvm_type, field_info.name, align));
                            return Ok(value);
                        }
                    }
                    // 回退到旧系统
                    let var_type = self.var_types.get(name)
                        .ok_or_else(|| codegen_error(format!("Variable '{}' not found", name)))?
                        .clone();
                    (var_type, name.clone())
                };

                // 如果值类型与变量类型不匹配，需要转换
                if value_type != var_type {
                    let temp = self.new_temp();

                    // 浮点类型转换
                    if value_type == "double" && var_type == "float" {
                        // double -> float 转换
                        self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                        let align = self.get_type_align("float");
                        self.emit_line(&format!("  store float {}, float* %{}, align {}", temp, llvm_name, align));
                        return Ok(format!("float {}", temp));
                    } else if value_type == "float" && var_type == "double" {
                        // float -> double 转换
                        self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                        let align = self.get_type_align("double");
                        self.emit_line(&format!("  store double {}, double* %{}, align {}", temp, llvm_name, align));
                        return Ok(format!("double {}", temp));
                    }
                    // 整数到浮点数转换
                    else if value_type.starts_with("i") && (var_type == "float" || var_type == "double") {
                        // 整数 -> 浮点数转换
                        self.emit_line(&format!("  {} = sitofp {} {} to {}", temp, value_type, val, var_type));
                        let align = self.get_type_align(&var_type);
                        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                        return Ok(format!("{} {}", var_type, temp));
                    }
                    // 整数类型转换
                    else if value_type.starts_with("i") && var_type.starts_with("i") {
                        let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                        let to_bits: u32 = var_type.trim_start_matches('i').parse().unwrap_or(64);

                        if to_bits > from_bits {
                            // 符号扩展
                            self.emit_line(&format!("  {} = sext {} {} to {}",
                                temp, value_type, val, var_type));
                        } else {
                            // 截断
                            self.emit_line(&format!("  {} = trunc {} {} to {}",
                                temp, value_type, val, var_type));
                        }
                        let align = self.get_type_align(&var_type);
                        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, llvm_name, align));
                        return Ok(format!("{} {}", var_type, temp));
                    }
                }

                // 类型匹配，直接存储
                let align = self.get_type_align(&var_type);
                self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, val, var_type, llvm_name, align));
                Ok(value)
            }
            Expr::ArrayAccess(arr_access) => {
                // 获取数组元素指针
                let (elem_type, elem_ptr, _) = self.get_array_element_ptr(arr_access)?;

                // 如果值类型与元素类型不匹配，需要转换
                if value_type != elem_type {
                    let temp = self.new_temp();

                    // 浮点类型转换
                    if value_type == "double" && elem_type == "float" {
                        // double -> float 转换
                        self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                        let align = self.get_type_align(&elem_type);
                        self.emit_line(&format!("  store float {}, {}* {}, align {}", temp, elem_type, elem_ptr, align));
                        return Ok(format!("float {}", temp));
                    } else if value_type == "float" && elem_type == "double" {
                        // float -> double 转换
                        self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                        let align = self.get_type_align(&elem_type);
                        self.emit_line(&format!("  store double {}, {}* {}, align {}", temp, elem_type, elem_ptr, align));
                        return Ok(format!("double {}", temp));
                    }
                    // 整数到浮点数转换
                    else if value_type.starts_with("i") && (elem_type == "float" || elem_type == "double") {
                        // 整数 -> 浮点数转换
                        self.emit_line(&format!("  {} = sitofp {} {} to {}", temp, value_type, val, elem_type));
                        let align = self.get_type_align(&elem_type);
                        self.emit_line(&format!("  store {} {}, {}* {}, align {}", elem_type, temp, elem_type, elem_ptr, align));
                        return Ok(format!("{} {}", elem_type, temp));
                    }
                    // 整数类型转换
                    else if value_type.starts_with("i") && elem_type.starts_with("i") {
                        let from_bits: u32 = value_type.trim_start_matches('i').parse().unwrap_or(64);
                        let to_bits: u32 = elem_type.trim_start_matches('i').parse().unwrap_or(64);

                        if to_bits > from_bits {
                            // 符号扩展
                            self.emit_line(&format!("  {} = sext {} {} to {}",
                                temp, value_type, val, elem_type));
                        } else {
                            // 截断
                            self.emit_line(&format!("  {} = trunc {} {} to {}",
                                temp, value_type, val, elem_type));
                        }
                        let align = self.get_type_align(&elem_type);
                        self.emit_line(&format!("  store {} {}, {}* {}, align {}", elem_type, temp, elem_type, elem_ptr, align));
                        return Ok(format!("{} {}", elem_type, temp));
                    }
                }

                // 类型匹配，直接存储到数组元素
                let align = self.get_type_align(&elem_type);
                self.emit_line(&format!("  store {} {}, {}* {}, align {}", elem_type, val, elem_type, elem_ptr, align));
                Ok(value)
            }
            _ => Err(codegen_error("Invalid assignment target".to_string()))
        }
    }

    /// 生成类型转换表达式代码
    fn generate_cast_expression(&mut self, cast: &CastExpr) -> cayResult<String> {
        let expr_value = self.generate_expression(&cast.expr)?;
        let (from_type, val) = self.parse_typed_value(&expr_value);
        let to_type = self.type_to_llvm(&cast.target_type);
        
        let temp = self.new_temp();
        
        // 相同类型无需转换
        if from_type == to_type {
            return Ok(format!("{} {}", to_type, val));
        }
        
        // 指针类型转换 (bitcast)
        if from_type.ends_with("*") && to_type.ends_with("*") {
            self.emit_line(&format!("  {} = bitcast {} {} to {}",
                temp, from_type, val, to_type));
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 整数到整数
        if from_type.starts_with("i") && to_type.starts_with("i") && !from_type.ends_with("*") && !to_type.ends_with("*") {
            let from_bits: u32 = from_type.trim_start_matches('i').parse().unwrap_or(64);
            let to_bits: u32 = to_type.trim_start_matches('i').parse().unwrap_or(64);
            
            if to_bits > from_bits {
                // 符号扩展
                self.emit_line(&format!("  {} = sext {} {} to {}",
                    temp, from_type, val, to_type));
            } else {
                // 截断
                self.emit_line(&format!("  {} = trunc {} {} to {}",
                    temp, from_type, val, to_type));
            }
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 整数到浮点
        if from_type.starts_with("i") && !from_type.ends_with("*") && 
           (to_type == "float" || to_type == "double") {
            self.emit_line(&format!("  {} = sitofp {} {} to {}",
                temp, from_type, val, to_type));
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 浮点到整数
        if (from_type == "float" || from_type == "double") && 
           to_type.starts_with("i") && !to_type.ends_with("*") {
            self.emit_line(&format!("  {} = fptosi {} {} to {}",
                temp, from_type, val, to_type));
            return Ok(format!("{} {}", to_type, temp));
        }
        
        // 浮点到浮点
        if (from_type == "float" || from_type == "double") && 
           (to_type == "float" || to_type == "double") {
            if to_type == "double" {
                self.emit_line(&format!("  {} = fpext {} {} to {}",
                    temp, from_type, val, to_type));
            } else {
                self.emit_line(&format!("  {} = fptrunc {} {} to {}",
                    temp, from_type, val, to_type));
            }
            return Ok(format!("{} {}", to_type, temp));
        }
        // 浮点到字符串（float/double -> String）
        if (from_type == "float" || from_type == "double") && to_type == "i8*" {
            // 关键修复：C 的可变参数函数中，float 会被提升为 double
            // 所以即使原类型是 float，也必须 fpext 到 double 再传参
            let arg_val = if from_type == "float" {
                let promoted = self.new_temp();
                self.emit_line(&format!("  {} = fpext float {} to double", promoted, val));
                promoted
            } else {
                val.to_string()  // 已经是 double
            };

            // 调用专门的运行时函数来避免调用约定问题
            let result = self.new_temp();
            self.emit_line(&format!("  {} = call i8* @__cay_float_to_string(double {})",
                result, arg_val));

            return Ok(format!("{} {}", to_type, result));
        }
        
        // 字符到字符串（char -> String）- 必须在整数转字符串之前处理
        if from_type == "i8" && to_type == "i8*" {
            let result = self.new_temp();
            self.emit_line(&format!("  {} = call i8* @__cay_char_to_string(i8 {})",
                result, val));
            return Ok(format!("{} {}", to_type, result));
        }
        
        // 布尔到字符串（bool -> String）
        // 布尔可能是 i1 或 i8，需要处理两种情况
        if (from_type == "i1" || from_type == "i8") && to_type == "i8*" {
            let result = self.new_temp();
            let bool_val = if from_type == "i1" {
                val.to_string()
            } else {
                // 将 i8 截断为 i1
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = trunc i8 {} to i1", temp, val));
                temp
            };
            self.emit_line(&format!("  {} = call i8* @__cay_bool_to_string(i1 {})",
                result, bool_val));
            return Ok(format!("{} {}", to_type, result));
        }
        
        // 整数到字符串（int -> String）- 放在字符和布尔之后
        if from_type.starts_with("i") && !from_type.ends_with("*") && to_type == "i8*" {
            // 先将整数扩展到 i64（如果还不是的话），然后调用运行时函数
            let result = self.new_temp();
            let i64_val = if from_type == "i64" {
                val.to_string()
            } else {
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = sext {} {} to i64", temp, from_type, val));
                temp
            };
            self.emit_line(&format!("  {} = call i8* @__cay_int_to_string(i64 {})",
                result, i64_val));
            return Ok(format!("{} {}", to_type, result));
        }
        
        Err(codegen_error(format!("Unsupported cast from {} to {}", from_type, to_type)))
    }

    /// 生成成员访问表达式代码
    fn generate_member_access(&mut self, member: &MemberAccessExpr) -> cayResult<String> {
        // 检查是否是静态字段访问: ClassName.fieldName
        if let Expr::Identifier(class_name) = &*member.object {
            let static_key = format!("{}.{}", class_name, member.member);
            if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                // 静态字段访问 - 返回全局变量的指针
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = load {}, {}* {}, align {}", 
                    temp, field_info.llvm_type, field_info.llvm_type, field_info.name, 
                    self.get_type_align(&field_info.llvm_type)));
                return Ok(format!("{} {}", field_info.llvm_type, temp));
            }
        }
        
        // 特殊处理数组的 .length 属性
        if member.member == "length" {
            let obj = self.generate_expression(&member.object)?;
            let (obj_type, obj_val) = self.parse_typed_value(&obj);
            
            // 检查是否是数组类型（以 * 结尾）
            if obj_type.ends_with("*") {
                // 首先将数组指针转换为 i8*
                let obj_i8 = self.new_temp();
                self.emit_line(&format!("  {} = bitcast {} {} to i8*", obj_i8, obj_type, obj_val));
                
                // 数组长度存储在数组指针前面的 8 字节中
                // 计算长度地址：array_ptr - 8
                let len_ptr_i8 = self.new_temp();
                self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 -8", len_ptr_i8, obj_i8));
                
                // 将长度指针转换为 i32*
                let len_ptr = self.new_temp();
                self.emit_line(&format!("  {} = bitcast i8* {} to i32*", len_ptr, len_ptr_i8));
                
                // 加载长度（作为 i32）
                let len_val = self.new_temp();
                self.emit_line(&format!("  {} = load i32, i32* {}, align 4", len_val, len_ptr));
                
                return Ok(format!("i32 {}", len_val));
            }
        }
        
        // 目前仅支持将成员访问视为对象指针的占位符（返回 i8* ptr）
        // 生成对象表达式并返回其指针值
        let obj = self.generate_expression(&member.object)?;
        let (_t, val) = self.parse_typed_value(&obj);
        Ok(format!("i8* {}", val))
    }

    /// 生成 new 表达式代码
    fn generate_new_expression(&mut self, _new_expr: &NewExpr) -> cayResult<String> {
        // 简化实现：为对象分配一块固定大小的内存（8字节），返回 i8* 指针
        // 这对不依赖对象字段的示例（如 NestedCalls）是足够的
        let size = 8i64;
        let calloc_temp = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @calloc(i64 1, i64 {})", calloc_temp, size));
        let cast_temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i8*", cast_temp, calloc_temp));
        Ok(format!("i8* {}", cast_temp))
    }

    /// 生成数组创建表达式代码: new Type[size] 或 new Type[size1][size2]...
    fn generate_array_creation(&mut self, arr: &ArrayCreationExpr) -> cayResult<String> {
        if arr.sizes.len() == 1 {
            // 一维数组
            self.generate_1d_array_creation(&arr.element_type, &arr.sizes[0])
        } else {
            // 多维数组
            self.generate_md_array_creation(&arr.element_type, &arr.sizes)
        }
    }

    /// 生成一维数组创建
    /// 内存布局: [长度:i32][填充:i32][元素0][元素1]...[元素N-1]
    /// 返回的指针指向元素0，长度存储在指针前8字节
    fn generate_1d_array_creation(&mut self, element_type: &Type, size_expr: &Expr) -> cayResult<String> {
        // 生成数组大小表达式
        let size_val_expr = self.generate_expression(size_expr)?;
        let (size_type, size_val) = self.parse_typed_value(&size_val_expr);
        
        // 确保大小是整数类型
        if !size_type.starts_with("i") {
            return Err(codegen_error(format!("Array size must be integer, got {}", size_type)));
        }
        
        // 将大小转换为 i64（用于内存分配）
        let size_i64 = if size_type != "i64" {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to i64", temp, size_type, size_val));
            temp
        } else {
            size_val.to_string()
        };
        
        // 同时保存为 i32 用于存储长度
        let size_i32 = if size_type != "i32" {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = trunc {} {} to i32", temp, size_type, size_val));
            temp
        } else {
            size_val.to_string()
        };
        
        // 获取元素类型
        let elem_type = self.type_to_llvm(element_type);
        
        // 计算元素大小
        let elem_size = match element_type {
            Type::Int32 => 4,
            Type::Int64 => 8,
            Type::Float32 => 4,
            Type::Float64 => 8,
            Type::Bool => 1,
            Type::Char => 1,
            Type::String => 8, // 指针大小
            Type::Object(_) => 8, // 指针大小
            Type::Array(_) => 8, // 指针大小
            _ => 8, // 默认
        };
        
        // 计算数据字节数 = 大小 * 元素大小
        let data_bytes_temp = self.new_temp();
        self.emit_line(&format!("  {} = mul i64 {}, {}", data_bytes_temp, size_i64, elem_size));
        
        // 额外分配 8 字节用于存储长度（i32 + 填充）
        let total_bytes_temp = self.new_temp();
        self.emit_line(&format!("  {} = add i64 {}, 8", total_bytes_temp, data_bytes_temp));
        
        // 调用 calloc 分配内存（自动零初始化）
        let calloc_temp = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @calloc(i64 1, i64 {})", calloc_temp, total_bytes_temp));
        
        // 存储长度（前4字节）- calloc 已零初始化，只需设置长度
        let len_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", len_ptr, calloc_temp));
        self.emit_line(&format!("  store i32 {}, i32* {}, align 4", size_i32, len_ptr));
        
        // 计算数据起始地址（跳过8字节长度头）
        let data_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 8", data_ptr, calloc_temp));
        
        // 将 i8* 转换为元素类型指针
        let cast_temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to {}*", cast_temp, data_ptr, elem_type));
        
        // 返回数组指针（指向数据，长度在指针前8字节）
        Ok(format!("{}* {}", elem_type, cast_temp))
    }

    /// 生成多维数组创建: new Type[size1][size2]...[sizeN]
    fn generate_md_array_creation(&mut self, element_type: &Type, sizes: &[Expr]) -> cayResult<String> {
        // 多维数组实现：分配一个指针数组，每个指针指向子数组
        // 例如 new int[3][4][5]:
        // 1. 分配 3 个指针的数组 (int**)
        // 2. 循环 3 次，每次递归分配 [4][5] 的子数组
        // 3. 将子数组指针存入父数组

        if sizes.len() < 2 {
            return Err(codegen_error("Multidimensional array needs at least 2 dimensions".to_string()));
        }

        // 递归创建子数组类型（去掉第一维）
        let sub_sizes = &sizes[1..];

        // 生成第一维大小
        let first_size_expr = self.generate_expression(&sizes[0])?;
        let (first_size_type, first_size_val) = self.parse_typed_value(&first_size_expr);

        let first_size_i64 = if first_size_type != "i64" {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to i64", temp, first_size_type, first_size_val));
            temp
        } else {
            first_size_val.to_string()
        };

        // 获取元素类型的 LLVM 表示
        let elem_llvm_type = self.type_to_llvm(element_type);

        // 确定子数组的 LLVM 类型
        // 如果还有多个维度，子数组是指向更低维度的指针
        // 如果只剩一个维度，子数组是元素指针
        let sub_array_llvm_type = if sub_sizes.len() == 1 {
            format!("{}*", elem_llvm_type)
        } else {
            // 递归获取子数组类型
            format!("{}*", self.get_md_array_type(element_type, sub_sizes.len()))
        };

        // 分配指针数组 (elem_type** 用于存储子数组指针)
        let ptr_array_bytes = self.new_temp();
        self.emit_line(&format!("  {} = mul i64 {}, 8", ptr_array_bytes, first_size_i64));

        let calloc_ptr_array = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @calloc(i64 1, i64 {})", calloc_ptr_array, ptr_array_bytes));

        // 转换为正确的指针类型
        let ptr_array = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to {}*", ptr_array, calloc_ptr_array, sub_array_llvm_type));

        // 生成循环来分配每个子数组
        let loop_label = self.new_label("md_loop");
        let body_label = self.new_label("md_body");
        let end_label = self.new_label("md_end");

        // 循环变量 - 使用临时变量名避免冲突
        let loop_var = self.new_temp();
        self.emit_line(&format!("  {} = alloca i64", loop_var));
        self.emit_line(&format!("  store i64 0, i64* {}", loop_var));

        // 跳转到循环条件
        self.emit_line(&format!("  br label %{}", loop_label));

        // 循环条件
        self.emit_line(&format!("\n{}:", loop_label));
        let current_idx = self.new_temp();
        self.emit_line(&format!("  {} = load i64, i64* {}", current_idx, loop_var));
        let cond = self.new_temp();
        self.emit_line(&format!("  {} = icmp slt i64 {}, {}", cond, current_idx, first_size_i64));
        self.emit_line(&format!("  br i1 {}, label %{}, label %{}", cond, body_label, end_label));

        // 循环体
        self.emit_line(&format!("\n{}:", body_label));

        // 分配子数组
        let sub_array = if sub_sizes.len() == 1 {
            // 最后一维，创建一维数组
            self.generate_1d_array_creation(element_type, &sub_sizes[0])?
        } else {
            // 还有多个维度，递归创建多维数组
            self.generate_md_array_creation(element_type, sub_sizes)?
        };
        let (_, sub_array_val) = self.parse_typed_value(&sub_array);

        // 将子数组指针存入指针数组
        let elem_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr {}, {}* {}, i64 {}",
            elem_ptr, sub_array_llvm_type, sub_array_llvm_type, ptr_array, current_idx));

        self.emit_line(&format!("  store {} {}, {}* {}", sub_array_llvm_type, sub_array_val, sub_array_llvm_type, elem_ptr));

        // 增加循环变量
        let next_idx = self.new_temp();
        self.emit_line(&format!("  {} = add i64 {}, 1", next_idx, current_idx));
        self.emit_line(&format!("  store i64 {}, i64* {}", next_idx, loop_var));

        // 跳回循环条件
        self.emit_line(&format!("  br label %{}", loop_label));

        // 循环结束
        self.emit_line(&format!("\n{}:", end_label));

        // 返回指针数组
        Ok(format!("{}* {}", sub_array_llvm_type, ptr_array))
    }

    /// 获取多维数组类型的 LLVM 表示
    fn get_md_array_type(&self, element_type: &Type, dimensions: usize) -> String {
        let base = self.type_to_llvm(element_type);
        format!("{}{}", base, "*".repeat(dimensions))
    }

    /// 获取数组元素指针（用于赋值操作）
    fn get_array_element_ptr(&mut self, arr: &ArrayAccessExpr) -> cayResult<(String, String, String)> {
        // 生成数组表达式
        let array_expr = self.generate_expression(&arr.array)?;
        let (array_type, array_val) = self.parse_typed_value(&array_expr);

        // 生成索引表达式
        let index_expr = self.generate_expression(&arr.index)?;
        let (index_type, index_val) = self.parse_typed_value(&index_expr);

        // 确保索引是整数类型
        if !index_type.starts_with("i") {
            return Err(codegen_error(format!("Array index must be integer, got {}", index_type)));
        }

        // 将索引转换为 i64
        let index_i64 = if index_type != "i64" {
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to i64", temp, index_type, index_val));
            temp
        } else {
            index_val.to_string()
        };

        // 获取数组元素类型（去掉末尾的一个 *）
        // 例如: i32* -> i32, i32** -> i32*, i64* -> i64
        let elem_type = if array_type.ends_with("*") {
            // 找到最后一个 * 的位置，去掉它
            let len = array_type.len();
            array_type[..len-1].to_string()
        } else {
            // 如果不是指针类型，假设是 i64*（向后兼容）
            "i64".to_string()
        };

        // 计算元素地址
        let elem_ptr_temp = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr {}, {}* {}, i64 {}",
            elem_ptr_temp, elem_type, elem_type, array_val, index_i64));

        Ok((elem_type, elem_ptr_temp, index_i64))
    }
    
    /// 生成数组访问表达式代码: arr[index]
    fn generate_array_access(&mut self, arr: &ArrayAccessExpr) -> cayResult<String> {
        let (elem_type, elem_ptr_temp, _) = self.get_array_element_ptr(arr)?;
        
        // 加载元素值
        let elem_temp = self.new_temp();
        let align = self.get_type_align(&elem_type);
        self.emit_line(&format!("  {} = load {}, {}* {}, align {}", elem_temp, elem_type, elem_type, elem_ptr_temp, align));
        
        Ok(format!("{} {}", elem_type, elem_temp))
    }

    /// 生成数组初始化表达式代码: {1, 2, 3}
    /// 内存布局: [长度:i32][填充:i32][元素0][元素1]...[元素N-1]
    fn generate_array_init(&mut self, init: &ArrayInitExpr) -> cayResult<String> {
        if init.elements.is_empty() {
            return Err(codegen_error("Cannot generate code for empty array initializer".to_string()));
        }
        
        // 推断元素类型（从第一个元素）
        let first_elem = self.generate_expression(&init.elements[0])?;
        let (elem_llvm_type, _) = self.parse_typed_value(&first_elem);
        
        // 获取元素大小
        let elem_size = match elem_llvm_type.as_str() {
            "i1" => 1,
            "i8" => 1,
            "i32" => 4,
            "i64" => 8,
            "float" => 4,
            "double" => 8,
            _ => 8, // 指针类型
        };
        
        let num_elements = init.elements.len() as i64;
        
        // 计算数据字节数
        let data_bytes = num_elements * elem_size;
        // 额外分配 8 字节用于存储长度
        let total_bytes = data_bytes + 8;
        
        // 分配内存（使用 calloc 自动零初始化）
        let calloc_temp = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @calloc(i64 1, i64 {})", calloc_temp, total_bytes));
        
        // 存储长度（前4字节）- calloc 已零初始化，只需设置长度
        let len_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", len_ptr, calloc_temp));
        self.emit_line(&format!("  store i32 {}, i32* {}, align 4", num_elements, len_ptr));
        
        // 计算数据起始地址（跳过8字节长度头）
        let data_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 8", data_ptr, calloc_temp));
        
        // 转换为元素类型指针
        let cast_temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to {}*", cast_temp, data_ptr, elem_llvm_type));
        
        // 存储每个元素
        for (i, elem) in init.elements.iter().enumerate() {
            let elem_val = self.generate_expression(elem)?;
            let (_, val) = self.parse_typed_value(&elem_val);
            
            // 获取元素地址
            let elem_ptr = self.new_temp();
            self.emit_line(&format!("  {} = getelementptr {}, {}* {}, i64 {}", 
                elem_ptr, elem_llvm_type, elem_llvm_type, cast_temp, i));
            
            // 存储元素
            self.emit_line(&format!("  store {} {}, {}* {}", elem_llvm_type, val, elem_llvm_type, elem_ptr));
        }
        
        // 返回数组指针（指向数据，长度在指针前8字节）
        Ok(format!("{}* {}", elem_llvm_type, cast_temp))
    }

    /// 生成数组初始化表达式代码，使用指定的目标类型: {1, 2, 3}
    /// 内存布局: [长度:i32][填充:i32][元素0][元素1]...[元素N-1]
    pub fn generate_array_init_with_type(&mut self, init: &ArrayInitExpr, target_type: &Type) -> cayResult<String> {
        if init.elements.is_empty() {
            return Err(codegen_error("Cannot generate code for empty array initializer".to_string()));
        }

        // 从目标类型获取元素类型
        let elem_llvm_type = if let Type::Array(elem_type) = target_type {
            self.type_to_llvm(elem_type)
        } else {
            // 如果目标类型不是数组，使用第一个元素的类型
            let first_elem = self.generate_expression(&init.elements[0])?;
            let (elem_type, _) = self.parse_typed_value(&first_elem);
            elem_type
        };

        // 获取元素大小
        let elem_size = match elem_llvm_type.as_str() {
            "i1" => 1,
            "i8" => 1,
            "i32" => 4,
            "i64" => 8,
            "float" => 4,
            "double" => 8,
            _ => 8, // 指针类型
        };

        let num_elements = init.elements.len() as i64;

        // 计算数据字节数
        let data_bytes = num_elements * elem_size;
        // 额外分配 8 字节用于存储长度
        let total_bytes = data_bytes + 8;

        // 分配内存（使用 calloc 自动零初始化）
        let calloc_temp = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @calloc(i64 1, i64 {})", calloc_temp, total_bytes));

        // 存储长度（前4字节）- calloc 已零初始化，只需设置长度
        let len_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", len_ptr, calloc_temp));
        self.emit_line(&format!("  store i32 {}, i32* {}, align 4", num_elements, len_ptr));

        // 计算数据起始地址（跳过8字节长度头）
        let data_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 8", data_ptr, calloc_temp));

        // 转换为元素类型指针
        let cast_temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to {}*", cast_temp, data_ptr, elem_llvm_type));

        // 存储每个元素
        for (i, elem) in init.elements.iter().enumerate() {
            let elem_val = self.generate_expression(elem)?;
            let (elem_value_type, val) = self.parse_typed_value(&elem_val);

            // 如果需要，进行类型转换
            let final_val = if elem_value_type != elem_llvm_type {
                let temp = self.new_temp();
                // 整数到浮点数转换
                if elem_value_type.starts_with("i") && (elem_llvm_type == "float" || elem_llvm_type == "double") {
                    self.emit_line(&format!("  {} = sitofp {} {} to {}",
                        temp, elem_value_type, val, elem_llvm_type));
                }
                // 浮点数到整数转换
                else if (elem_value_type == "float" || elem_value_type == "double") && elem_llvm_type.starts_with("i") {
                    self.emit_line(&format!("  {} = fptosi {} {} to {}",
                        temp, elem_value_type, val, elem_llvm_type));
                }
                // 浮点数类型转换
                else if elem_value_type == "double" && elem_llvm_type == "float" {
                    self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                }
                else if elem_value_type == "float" && elem_llvm_type == "double" {
                    self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                }
                // 整数类型转换
                else if elem_value_type.starts_with("i") && elem_llvm_type.starts_with("i") {
                    let from_bits: u32 = elem_value_type.trim_start_matches('i').parse().unwrap_or(64);
                    let to_bits: u32 = elem_llvm_type.trim_start_matches('i').parse().unwrap_or(64);
                    if to_bits > from_bits {
                        self.emit_line(&format!("  {} = sext {} {} to {}",
                            temp, elem_value_type, val, elem_llvm_type));
                    } else {
                        self.emit_line(&format!("  {} = trunc {} {} to {}",
                            temp, elem_value_type, val, elem_llvm_type));
                    }
                }
                else {
                    // 无法转换，直接使用原值
                    self.emit_line(&format!("  {} = add {} {}, 0", temp, elem_value_type, val));
                }
                temp
            } else {
                val.to_string()
            };

            // 获取元素地址
            let elem_ptr = self.new_temp();
            self.emit_line(&format!("  {} = getelementptr {}, {}* {}, i64 {}",
                elem_ptr, elem_llvm_type, elem_llvm_type, cast_temp, i));

            // 存储元素
            self.emit_line(&format!("  store {} {}, {}* {}", elem_llvm_type, final_val, elem_llvm_type, elem_ptr));
        }

        // 返回数组指针（指向数据，长度在指针前8字节）
        Ok(format!("{}* {}", elem_llvm_type, cast_temp))
    }

    /// 生成方法引用表达式代码
    /// 方法引用: ClassName::methodName 或 obj::methodName
    fn generate_method_ref(&mut self, method_ref: &MethodRefExpr) -> cayResult<String> {
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

    /// 生成 Lambda 表达式代码
    /// Lambda: (params) -> { body }
    fn generate_lambda(&mut self, lambda: &LambdaExpr) -> cayResult<String> {
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

    /// 获取左值的类型和 LLVM 指针表示
    /// 返回 (类型字符串, 指针字符串)
    fn get_lvalue_info(&mut self, expr: &Expr) -> cayResult<(String, String)> {
        match expr {
            Expr::Identifier(name) => {
                // 优先使用作用域管理器获取变量类型
                let (var_type, llvm_name) = if let Some(scope_type) = self.scope_manager.get_var_type(name) {
                    let llvm_name = self.scope_manager.get_llvm_name(name).unwrap_or_else(|| name.clone());
                    (scope_type, llvm_name)
                } else {
                    // 检查是否是当前类的静态字段
                    if !self.current_class.is_empty() {
                        let static_key = format!("{}.{}", self.current_class, name);
                        if let Some(field_info) = self.static_field_map.get(&static_key).cloned() {
                            return Ok((field_info.llvm_type, field_info.name));
                        }
                    }
                    // 回退到旧系统
                    let var_type = self.var_types.get(name)
                        .ok_or_else(|| codegen_error(format!("Variable '{}' not found", name)))?
                        .clone();
                    (var_type, name.clone())
                };
                Ok((var_type, format!("%{}", llvm_name)))
            }
            Expr::ArrayAccess(arr) => {
                let (elem_type, elem_ptr, _) = self.get_array_element_ptr(arr)?;
                Ok((elem_type, elem_ptr))
            }
            _ => Err(codegen_error("Invalid lvalue expression".to_string()))
        }
    }
}
