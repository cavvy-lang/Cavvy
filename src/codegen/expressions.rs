//! 表达式代码生成
use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::types::Type;
use crate::error::{EolResult, codegen_error};

impl IRGenerator {
    /// 生成表达式代码
    pub fn generate_expression(&mut self, expr: &Expr) -> EolResult<String> {
        match expr {
            Expr::Literal(lit) => self.generate_literal(lit),
            Expr::Identifier(name) => {
                let temp = self.new_temp();
                let var_type = self.var_types.get(name).cloned().unwrap_or_else(|| "i64".to_string());
                let align = self.get_type_align(&var_type);  // 获取正确的对齐
                self.emit_line(&format!("  {} = load {}, {}* %{}, align {}", 
                    temp, var_type, var_type, name, align));
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
        }
    }

    /// 生成字面量代码
    fn generate_literal(&mut self, lit: &LiteralValue) -> EolResult<String> {
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
        if left_type == right_type {
            return (left_type.to_string(), left_val.to_string(), right_val.to_string());
        }
        
        // 确定提升后的类型（选择位数更大的类型）
        let left_bits: u32 = left_type.trim_start_matches('i').parse().unwrap_or(64);
        let right_bits: u32 = right_type.trim_start_matches('i').parse().unwrap_or(64);
        
        if left_bits >= right_bits {
            // 提升右操作数到左操作数的类型
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to {}", temp, right_type, right_val, left_type));
            (left_type.to_string(), left_val.to_string(), temp)
        } else {
            // 提升左操作数到右操作数的类型
            let temp = self.new_temp();
            self.emit_line(&format!("  {} = sext {} {} to {}", temp, left_type, left_val, right_type));
            (right_type.to_string(), temp, right_val.to_string())
        }
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
    
    /// 生成二元表达式代码
    fn generate_binary_expression(&mut self, bin: &BinaryExpr) -> EolResult<String> {
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
                    self.emit_line(&format!("  {} = call i8* @__eol_string_concat(i8* {}, i8* {})",
                        temp, left_val, right_val));
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
    fn generate_unary_expression(&mut self, unary: &UnaryExpr) -> EolResult<String> {
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
            UnaryOp::PreInc | UnaryOp::PostInc => {
                // i++ 或 ++i
                let one = if op_type.starts_with("i") { "1" } else { "1.0" };
                if op_type.starts_with("i") {
                    self.emit_line(&format!("  {} = add {} {}, {}",
                        temp, op_type, op_val, one));
                } else {
                    self.emit_line(&format!("  {} = fadd {} {}, {}",
                        temp, op_type, op_val, one));
                }
                // 存储回变量
                if let Expr::Identifier(name) = unary.operand.as_ref() {
                    self.emit_line(&format!("  store {} {}, {}* %{}",
                        op_type, temp, op_type, name));
                }
                // 前置返回新值，后置返回旧值
                if unary.op == UnaryOp::PreInc {
                    return Ok(format!("{} {}", op_type, temp));
                } else {
                    return Ok(format!("{} {}", op_type, op_val));
                }
            }
            UnaryOp::PreDec | UnaryOp::PostDec => {
                // i-- 或 --i
                let one = if op_type.starts_with("i") { "1" } else { "1.0" };
                if op_type.starts_with("i") {
                    self.emit_line(&format!("  {} = sub {} {}, {}",
                        temp, op_type, op_val, one));
                } else {
                    self.emit_line(&format!("  {} = fsub {} {}, {}",
                        temp, op_type, op_val, one));
                }
                // 存储回变量
                if let Expr::Identifier(name) = unary.operand.as_ref() {
                    self.emit_line(&format!("  store {} {}, {}* %{}",
                        op_type, temp, op_type, name));
                }
                // 前置返回新值，后置返回旧值
                if unary.op == UnaryOp::PreDec {
                    return Ok(format!("{} {}", op_type, temp));
                } else {
                    return Ok(format!("{} {}", op_type, op_val));
                }
            }
        }
        
        Ok(format!("{} {}", op_type, temp))
    }

    /// 生成函数调用表达式代码
    fn generate_call_expression(&mut self, call: &CallExpr) -> EolResult<String> {
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
        
        // 处理普通函数调用
        let fn_name = match call.callee.as_ref() {
            Expr::Identifier(name) => {
                // 如果是当前类的方法，添加类名前缀
                if !self.current_class.is_empty() {
                    format!("{}.{}", self.current_class, name)
                } else {
                    name.clone()
                }
            }
            Expr::MemberAccess(member) => {
                if let Expr::Identifier(obj_name) = member.object.as_ref() {
                    // 如果 obj_name 对应一个已知的变量类名，使用该类名，否则假设 obj_name 本身是类名
                    if let Some(class_name) = self.var_class_map.get(obj_name) {
                        format!("{}.{}", class_name, member.member)
                    } else {
                        format!("{}.{}", obj_name, member.member)
                    }
                } else {
                    return Err(codegen_error("Invalid method call".to_string()));
                }
            }
            _ => return Err(codegen_error("Invalid function call".to_string())),
        };
        
        // 生成参数并转换为i64（假设所有方法参数都是i64）
        let mut converted_args = Vec::new();
        for arg in &call.args {
            let arg_str = self.generate_expression(arg)?;
            let (arg_type, arg_val) = self.parse_typed_value(&arg_str);
            
            // 如果参数是i32，转换为i64
            if arg_type == "i32" {
                let temp = self.new_temp();
                self.emit_line(&format!("  {} = sext i32 {} to i64", temp, arg_val));
                converted_args.push(format!("i64 {}", temp));
            } else {
                converted_args.push(arg_str);
            }
        }
        
        let temp = self.new_temp();
        self.emit_line(&format!("  {} = call i64 @{}({})",
            temp, fn_name, converted_args.join(", ")));
        
        Ok(format!("i64 {}", temp))
    }

    /// 生成 print/println 调用代码
    fn generate_print_call(&mut self, args: &[Expr], newline: bool) -> EolResult<String> {
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
                let fmt_str = if newline { "%ld\n" } else { "%ld" };
                let fmt_name = self.get_or_create_string_constant(fmt_str);
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
                    // 需要将整数扩展为 i64 以匹配 %ld 格式
                    let fmt_str = if newline { "%ld\n" } else { "%ld" };
                    let fmt_name = self.get_or_create_string_constant(fmt_str);
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
    fn generate_read_int_call(&mut self, args: &[Expr]) -> EolResult<String> {
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
        let fmt_str = "%ld";
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
    fn generate_read_float_call(&mut self, args: &[Expr]) -> EolResult<String> {
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
    fn generate_read_line_call(&mut self, args: &[Expr]) -> EolResult<String> {
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
    fn generate_assignment(&mut self, assign: &AssignmentExpr) -> EolResult<String> {
        let value = self.generate_expression(&assign.value)?;
        let (value_type, val) = self.parse_typed_value(&value);
        
        match assign.target.as_ref() {
            Expr::Identifier(name) => {
                // 获取变量的实际类型（克隆以避免借用问题）
                let var_type = self.var_types.get(name)
                    .ok_or_else(|| codegen_error(format!("Variable '{}' not found", name)))?
                    .clone();
                
                // 如果值类型与变量类型不匹配，需要转换
                if value_type != var_type {
                    let temp = self.new_temp();
                    
                    // 浮点类型转换
                    if value_type == "double" && var_type == "float" {
                        // double -> float 转换
                        self.emit_line(&format!("  {} = fptrunc double {} to float", temp, val));
                        let align = self.get_type_align("float");
                        self.emit_line(&format!("  store float {}, float* %{}, align {}", temp, name, align));
                        return Ok(format!("float {}", temp));
                    } else if value_type == "float" && var_type == "double" {
                        // float -> double 转换
                        self.emit_line(&format!("  {} = fpext float {} to double", temp, val));
                        let align = self.get_type_align("double");
                        self.emit_line(&format!("  store double {}, double* %{}, align {}", temp, name, align));
                        return Ok(format!("double {}", temp));
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
                        self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, temp, var_type, name, align));
                        return Ok(format!("{} {}", var_type, temp));
                    }
                }
                
                // 类型匹配，直接存储
                let align = self.get_type_align(&var_type);
                self.emit_line(&format!("  store {} {}, {}* %{}, align {}", var_type, val, var_type, name, align));
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
    fn generate_cast_expression(&mut self, cast: &CastExpr) -> EolResult<String> {
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
            self.emit_line(&format!("  {} = call i8* @__eol_float_to_string(double {})",
                result, arg_val));

            return Ok(format!("{} {}", to_type, result));
        }
        Err(codegen_error(format!("Unsupported cast from {} to {}", from_type, to_type)))
    }

    /// 生成成员访问表达式代码
    fn generate_member_access(&mut self, member: &MemberAccessExpr) -> EolResult<String> {
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
    fn generate_new_expression(&mut self, new_expr: &NewExpr) -> EolResult<String> {
        // 简化实现：为对象分配一块固定大小的内存（8字节），返回 i8* 指针
        // 这对不依赖对象字段的示例（如 NestedCalls）是足够的
        let size = 8i64;
        let malloc_temp = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @malloc(i64 {})", malloc_temp, size));
        let cast_temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i8*", cast_temp, malloc_temp));
        Ok(format!("i8* {}", cast_temp))
    }

    /// 生成数组创建表达式代码: new Type[size] 或 new Type[size1][size2]...
    fn generate_array_creation(&mut self, arr: &ArrayCreationExpr) -> EolResult<String> {
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
    fn generate_1d_array_creation(&mut self, element_type: &Type, size_expr: &Expr) -> EolResult<String> {
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
        
        // 调用 malloc 分配内存
        let malloc_temp = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @malloc(i64 {})", malloc_temp, total_bytes_temp));
        
        // 存储长度（前4字节）
        let len_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", len_ptr, malloc_temp));
        self.emit_line(&format!("  store i32 {}, i32* {}, align 4", size_i32, len_ptr));
        
        // 计算数据起始地址（跳过8字节长度头）
        let data_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 8", data_ptr, malloc_temp));
        
        // 将 i8* 转换为元素类型指针
        let cast_temp = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to {}*", cast_temp, data_ptr, elem_type));
        
        // 返回数组指针（指向数据，长度在指针前8字节）
        Ok(format!("{}* {}", elem_type, cast_temp))
    }

    /// 生成多维数组创建: new Type[size1][size2]...[sizeN]
    fn generate_md_array_creation(&mut self, element_type: &Type, sizes: &[Expr]) -> EolResult<String> {
        // 多维数组实现：分配一个指针数组，每个指针指向子数组
        // 例如 new int[3][4]:
        // 1. 分配 3 个指针的数组 (int**)
        // 2. 循环 3 次，每次分配 4 个 int 的数组
        // 3. 将子数组指针存入父数组
        
        if sizes.len() < 2 {
            return Err(codegen_error("Multidimensional array needs at least 2 dimensions".to_string()));
        }
        
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
        
        // 分配指针数组 (elem_type** 用于存储子数组指针)
        let ptr_array_bytes = self.new_temp();
        self.emit_line(&format!("  {} = mul i64 {}, 8", ptr_array_bytes, first_size_i64));
        
        let malloc_ptr_array = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @malloc(i64 {})", malloc_ptr_array, ptr_array_bytes));
        
        // 转换为正确的指针类型 (elem_type**)
        let ptr_array = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to {}**", ptr_array, malloc_ptr_array, elem_llvm_type));
        
        // 生成循环来分配每个子数组
        let loop_label = self.new_label("md_array_loop");
        let body_label = self.new_label("md_array_body");
        let end_label = self.new_label("md_array_end");
        
        // 循环变量
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
        let sub_array = self.generate_1d_array_creation(element_type, &sizes[1])?;
        let (sub_array_type, sub_array_val) = self.parse_typed_value(&sub_array);
        
        // 将子数组指针存入指针数组
        let elem_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr {}*, {}** {}, i64 {}", 
            elem_ptr, elem_llvm_type, elem_llvm_type, ptr_array, current_idx));
        
        self.emit_line(&format!("  store {}* {}, {}** {}", elem_llvm_type, sub_array_val, elem_llvm_type, elem_ptr));
        
        // 增加循环变量
        let next_idx = self.new_temp();
        self.emit_line(&format!("  {} = add i64 {}, 1", next_idx, current_idx));
        self.emit_line(&format!("  store i64 {}, i64* {}", next_idx, loop_var));
        
        // 跳回循环条件
        self.emit_line(&format!("  br label %{}", loop_label));
        
        // 循环结束
        self.emit_line(&format!("\n{}:", end_label));
        
        // 返回指针数组 (elem_type**)
        Ok(format!("{}** {}", elem_llvm_type, ptr_array))
    }

    /// 获取数组元素指针（用于赋值操作）
    fn get_array_element_ptr(&mut self, arr: &ArrayAccessExpr) -> EolResult<(String, String, String)> {
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
    fn generate_array_access(&mut self, arr: &ArrayAccessExpr) -> EolResult<String> {
        let (elem_type, elem_ptr_temp, _) = self.get_array_element_ptr(arr)?;
        
        // 加载元素值
        let elem_temp = self.new_temp();
        let align = self.get_type_align(&elem_type);
        self.emit_line(&format!("  {} = load {}, {}* {}, align {}", elem_temp, elem_type, elem_type, elem_ptr_temp, align));
        
        Ok(format!("{} {}", elem_type, elem_temp))
    }

    /// 生成数组初始化表达式代码: {1, 2, 3}
    /// 内存布局: [长度:i32][填充:i32][元素0][元素1]...[元素N-1]
    fn generate_array_init(&mut self, init: &ArrayInitExpr) -> EolResult<String> {
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
        
        // 分配内存
        let malloc_temp = self.new_temp();
        self.emit_line(&format!("  {} = call i8* @malloc(i64 {})", malloc_temp, total_bytes));
        
        // 存储长度（前4字节）
        let len_ptr = self.new_temp();
        self.emit_line(&format!("  {} = bitcast i8* {} to i32*", len_ptr, malloc_temp));
        self.emit_line(&format!("  store i32 {}, i32* {}, align 4", num_elements, len_ptr));
        
        // 计算数据起始地址（跳过8字节长度头）
        let data_ptr = self.new_temp();
        self.emit_line(&format!("  {} = getelementptr i8, i8* {}, i64 8", data_ptr, malloc_temp));
        
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
}
