//! 二元表达式代码生成
//!
//! 处理算术运算、比较运算、位运算和逻辑运算。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::{cayResult, codegen_error};

impl IRGenerator {
    /// 生成二元表达式代码
    ///
    /// # Arguments
    /// * `bin` - 二元表达式
    pub fn generate_binary_expression(&mut self, bin: &BinaryExpr) -> cayResult<String> {
        let left = self.generate_expression(&bin.left)?;
        let right = self.generate_expression(&bin.right)?;
        
        // 解析类型和值
        let (left_type, left_val) = self.parse_typed_value(&left);
        let (right_type, right_val) = self.parse_typed_value(&right);
        
        let temp = self.new_temp();
        
        match bin.op {
            BinaryOp::Add => self.generate_add(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Sub => self.generate_sub(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Mul => self.generate_mul(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Div => self.generate_div(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Mod => self.generate_mod(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Eq => self.generate_eq(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Ne => self.generate_ne(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Lt => self.generate_lt(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Le => self.generate_le(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Gt => self.generate_gt(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Ge => self.generate_ge(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::And => self.generate_and(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Or => self.generate_or(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::BitAnd => self.generate_bitand(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::BitOr => self.generate_bitor(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::BitXor => self.generate_bitxor(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Shl => self.generate_shl(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::Shr => self.generate_shr(&left_type, &left_val, &right_type, &right_val, &temp),
            BinaryOp::UnsignedShr => self.generate_ushr(&left_type, &left_val, &right_type, &right_val, &temp),
        }
    }

    /// 生成加法表达式
    fn generate_add(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
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
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = add {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
            // 浮点数加法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = fadd {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if left_type.starts_with("i") && (right_type == "float" || right_type == "double") {
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
        } else if (left_type == "float" || left_type == "double") && right_type.starts_with("i") {
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

    /// 生成减法表达式
    fn generate_sub(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 整数减法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = sub {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
            // 浮点数减法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = fsub {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(left_type, left_val, right_type, right_val) {
            // 混合类型：整数和浮点数
            self.emit_line(&format!("  {} = fsub {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error(format!("Unsupported subtraction types: {} and {}", left_type, right_type)));
        }
    }

    /// 生成乘法表达式
    fn generate_mul(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 整数乘法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = mul {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
            // 浮点数乘法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = fmul {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(left_type, left_val, right_type, right_val) {
            // 混合类型：整数和浮点数
            self.emit_line(&format!("  {} = fmul {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error(format!("Unsupported multiplication types: {} and {}", left_type, right_type)));
        }
    }

    /// 生成除法表达式
    fn generate_div(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 整数除法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            // 运行时除零检查
            self.generate_division_by_zero_check(&promoted_type, &promoted_right)?;
            self.emit_line(&format!("  {} = sdiv {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
            // 浮点数除法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = fdiv {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(left_type, left_val, right_type, right_val) {
            // 混合类型：整数和浮点数
            self.emit_line(&format!("  {} = fdiv {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error(format!("Unsupported division types: {} and {}", left_type, right_type)));
        }
    }

    /// 生成取模表达式
    fn generate_mod(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 整数取模，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            // 运行时除零检查（取模也需要检查）
            self.generate_division_by_zero_check(&promoted_type, &promoted_right)?;
            self.emit_line(&format!("  {} = srem {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error(format!("Unsupported modulo types: {} and {}", left_type, right_type)));
        }
    }

    /// 生成等于比较表达式
    fn generate_eq(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type == "i8*" && right_type == "i8*" {
            // 字符串比较
            self.emit_line(&format!("  {} = icmp eq i8* {}, {}", temp, left_val, right_val));
            return Ok(format!("i1 {}", temp));
        } else if left_type.starts_with("i") && right_type.starts_with("i") {
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = icmp eq {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
            let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = fcmp oeq {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(left_type, left_val, right_type, right_val) {
            // 混合类型：整数和浮点数
            self.emit_line(&format!("  {} = fcmp oeq {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else {
            return Err(codegen_error(format!("Unsupported equality comparison types: {} and {}", left_type, right_type)));
        }
    }

    /// 生成不等于比较表达式
    fn generate_ne(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type == "i8*" && right_type == "i8*" {
            self.emit_line(&format!("  {} = icmp ne i8* {}, {}", temp, left_val, right_val));
            return Ok(format!("i1 {}", temp));
        } else if left_type.starts_with("i") && right_type.starts_with("i") {
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = icmp ne {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
            let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = fcmp one {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(left_type, left_val, right_type, right_val) {
            // 混合类型：整数和浮点数
            self.emit_line(&format!("  {} = fcmp one {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else {
            return Err(codegen_error(format!("Unsupported inequality comparison types: {} and {}", left_type, right_type)));
        }
    }

    /// 生成小于比较表达式
    fn generate_lt(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = icmp slt {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
            let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = fcmp olt {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(left_type, left_val, right_type, right_val) {
            // 混合类型：整数和浮点数
            self.emit_line(&format!("  {} = fcmp olt {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else {
            return Err(codegen_error(format!("Unsupported less-than comparison types: {} and {}", left_type, right_type)));
        }
    }

    /// 生成小于等于比较表达式
    fn generate_le(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = icmp sle {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
            let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = fcmp ole {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(left_type, left_val, right_type, right_val) {
            // 混合类型：整数和浮点数
            self.emit_line(&format!("  {} = fcmp ole {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("i1 {}", temp));
        } else {
            return Err(codegen_error(format!("Unsupported less-or-equal comparison types: {} and {}", left_type, right_type)));
        }
    }

    /// 生成大于比较表达式
    fn generate_gt(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 整数大于比较，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = icmp sgt {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
        } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
            // 浮点数大于比较，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = fcmp ogt {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
        } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(left_type, left_val, right_type, right_val) {
            // 混合类型：整数和浮点数
            self.emit_line(&format!("  {} = fcmp ogt {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
        } else {
            return Err(codegen_error(format!("Unsupported greater-than comparison types: {} and {}", left_type, right_type)));
        }
        Ok(format!("i1 {}", temp))
    }

    /// 生成大于等于比较表达式
    fn generate_ge(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 整数大于等于比较，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = icmp sge {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
        } else if (left_type == "float" || left_type == "double") && (right_type == "float" || right_type == "double") {
            // 浮点数大于等于比较，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = fcmp oge {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
        } else if let Some((promoted_type, promoted_left, promoted_right)) = self.promote_mixed_operands(left_type, left_val, right_type, right_val) {
            // 混合类型：整数和浮点数
            self.emit_line(&format!("  {} = fcmp oge {} {}, {}", temp, promoted_type, promoted_left, promoted_right));
        } else {
            return Err(codegen_error(format!("Unsupported greater-than-or-equal comparison types: {} and {}", left_type, right_type)));
        }
        Ok(format!("i1 {}", temp))
    }

    /// 生成逻辑与表达式
    fn generate_and(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        self.emit_line(&format!("  {} = and {} {}, {}", 
            temp, left_type, left_val, right_val));
        Ok(format!("i1 {}", temp))
    }

    /// 生成逻辑或表达式
    fn generate_or(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        self.emit_line(&format!("  {} = or {} {}, {}",
            temp, left_type, left_val, right_val));
        Ok(format!("i1 {}", temp))
    }

    /// 生成位与表达式
    fn generate_bitand(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 位与，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = and {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error(format!("Bitwise AND requires integer operands, got {} and {}", left_type, right_type)));
        }
    }

    /// 生成位或表达式
    fn generate_bitor(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 位或，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = or {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error(format!("Bitwise OR requires integer operands, got {} and {}", left_type, right_type)));
        }
    }

    /// 生成位异或表达式
    fn generate_bitxor(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 位异或，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = xor {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error(format!("Bitwise XOR requires integer operands, got {} and {}", left_type, right_type)));
        }
    }

    /// 生成左移表达式
    fn generate_shl(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 左移，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = shl {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error(format!("Shift left requires integer operands, got {} and {}", left_type, right_type)));
        }
    }

    /// 生成算术右移表达式
    fn generate_shr(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 算术右移，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = ashr {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error(format!("Arithmetic shift right requires integer operands, got {} and {}", left_type, right_type)));
        }
    }

    /// 生成逻辑右移表达式
    fn generate_ushr(&mut self, left_type: &str, left_val: &str, right_type: &str, right_val: &str, temp: &str) -> cayResult<String> {
        if left_type.starts_with("i") && right_type.starts_with("i") {
            // 逻辑右移，需要类型提升
            let (promoted_type, promoted_left, promoted_right) = self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!("  {} = lshr {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error(format!("Unsigned shift right requires integer operands, got {} and {}", left_type, right_type)));
        }
    }
}
