//! 二元表达式代码生成
//!
//! 处理算术运算、比较运算、位运算和逻辑运算。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::error::{SourceLocation, cayResult, codegen_error_at};

/// 检查类型是否为整数类型（不包括指针）
fn is_integer_type(ty: &str) -> bool {
    ty.starts_with("i") && !ty.ends_with("*")
}

/// 获取整数类型的位宽
fn get_int_bit_width(ty: &str) -> Option<u32> {
    if !ty.starts_with("i") || ty.ends_with("*") {
        return None;
    }
    ty[1..].parse().ok()
}

/// 生成整数类型转换指令
/// 从from_type转换到to_type，返回转换后的值
fn generate_int_cast(
    generator: &mut IRGenerator,
    from_type: &str,
    val: &str,
    to_bits: u32,
) -> String {
    // 指针类型不参与整数位宽转换，直接返回原值
    if from_type.ends_with("*") {
        return val.to_string();
    }
    let from_bits = get_int_bit_width(from_type).unwrap_or(32);

    if from_bits == to_bits {
        val.to_string()
    } else if from_bits < to_bits {
        // 扩展：使用sext
        let temp = generator.new_temp();
        generator.emit_line(&format!(
            "  {} = sext {} {} to i{}",
            temp, from_type, val, to_bits
        ));
        temp
    } else {
        // 截断：使用trunc
        let temp = generator.new_temp();
        generator.emit_line(&format!(
            "  {} = trunc {} {} to i{}",
            temp, from_type, val, to_bits
        ));
        temp
    }
}

impl IRGenerator {
    /// 生成二元表达式代码
    ///
    /// # Arguments
    /// * `bin` - 二元表达式
    pub fn generate_binary_expression(&mut self, bin: &BinaryExpr) -> cayResult<String> {
        let left = self.generate_expression(&bin.left)?;

        // 短路求值：&& 和 || 只先生成左侧，右侧在条件分支中惰性生成
        if bin.op == BinaryOp::And {
            let (left_type, left_val) = self.parse_typed_value(&left);
            let temp = self.new_temp();
            return self.generate_short_circuit_and(&left_type, &left_val, &bin.right, &temp);
        }
        if bin.op == BinaryOp::Or {
            let (left_type, left_val) = self.parse_typed_value(&left);
            let temp = self.new_temp();
            return self.generate_short_circuit_or(&left_type, &left_val, &bin.right, &temp);
        }

        let right = self.generate_expression(&bin.right)?;

        // 解析类型和值
        let (left_type, left_val) = self.parse_typed_value(&left);
        let (right_type, right_val) = self.parse_typed_value(&right);

        let temp = self.new_temp();
        let loc = bin.loc.clone();

        match bin.op {
            BinaryOp::Add => {
                self.generate_add(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Sub => {
                self.generate_sub(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Mul => {
                self.generate_mul(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Div => {
                self.generate_div(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Mod => {
                self.generate_mod(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Eq => {
                self.generate_eq(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Ne => {
                self.generate_ne(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Lt => {
                self.generate_lt(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Le => {
                self.generate_le(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Gt => {
                self.generate_gt(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Ge => {
                self.generate_ge(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::BitAnd => {
                self.generate_bitand(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::BitOr => {
                self.generate_bitor(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::BitXor => {
                self.generate_bitxor(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Shl => {
                self.generate_shl(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::Shr => {
                self.generate_shr(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            BinaryOp::UnsignedShr => {
                self.generate_ushr(&left_type, &left_val, &right_type, &right_val, &temp, &loc)
            }
            _ => unreachable!(), // And/Or handled above with short-circuit
        }
    }

    /// 生成加法表达式
    fn generate_add(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        // 字符串拼接处理
        if left_type == "i8*" && right_type == "i8*" {
            // 调用内建的字符串拼接函数
            self.emit_line(&format!(
                "  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                temp, left_val, right_val
            ));
            return Ok(format!("i8* {}", temp));
        } else if left_type == "i8*" && right_type == "i8" {
            // 字符串 + char：先将char转换为字符串，然后拼接
            let char_as_string = self.new_temp();
            self.emit_line(&format!(
                "  {} = call i8* @__cay_char_to_string(i8 {})",
                char_as_string, right_val
            ));
            self.emit_line(&format!(
                "  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                temp, left_val, char_as_string
            ));
            return Ok(format!("i8* {}", temp));
        } else if left_type == "i8" && right_type == "i8*" {
            // char + 字符串：先将char转换为字符串，然后拼接
            let char_as_string = self.new_temp();
            self.emit_line(&format!(
                "  {} = call i8* @__cay_char_to_string(i8 {})",
                char_as_string, left_val
            ));
            self.emit_line(&format!(
                "  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                temp, char_as_string, right_val
            ));
            return Ok(format!("i8* {}", temp));
        } else if left_type == "i8*" && right_type == "i1" {
            // 字符串 + 布尔：先将布尔转换为字符串，然后拼接
            let bool_as_string = self.new_temp();
            self.emit_line(&format!(
                "  {} = call i8* @__cay_bool_to_string(i1 {})",
                bool_as_string, right_val
            ));
            self.emit_line(&format!(
                "  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                temp, left_val, bool_as_string
            ));
            return Ok(format!("i8* {}", temp));
        } else if left_type == "i1" && right_type == "i8*" {
            // 布尔 + 字符串：先将布尔转换为字符串，然后拼接
            let bool_as_string = self.new_temp();
            self.emit_line(&format!(
                "  {} = call i8* @__cay_bool_to_string(i1 {})",
                bool_as_string, left_val
            ));
            self.emit_line(&format!(
                "  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                temp, bool_as_string, right_val
            ));
            return Ok(format!("i8* {}", temp));
        } else if left_type == "i8*" && is_integer_type(right_type) {
            // 字符串 + 整数：先将整数转换为i32，然后转换为字符串，然后拼接
            let int_as_string = self.new_temp();
            // 将任意整数类型转换为i32（使用正确的转换指令：sext或trunc）
            let int_val = generate_int_cast(self, right_type, right_val, 32);
            self.emit_line(&format!(
                "  {} = call i8* @__cay_int_to_string(i32 {})",
                int_as_string, int_val
            ));
            self.emit_line(&format!(
                "  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                temp, left_val, int_as_string
            ));
            return Ok(format!("i8* {}", temp));
        } else if is_integer_type(left_type) && right_type == "i8*" {
            // 整数 + 字符串：先将整数转换为i32，然后转换为字符串，然后拼接
            let int_as_string = self.new_temp();
            // 将任意整数类型转换为i32（使用正确的转换指令：sext或trunc）
            let int_val = generate_int_cast(self, left_type, left_val, 32);
            self.emit_line(&format!(
                "  {} = call i8* @__cay_int_to_string(i32 {})",
                int_as_string, int_val
            ));
            self.emit_line(&format!(
                "  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                temp, int_as_string, right_val
            ));
            return Ok(format!("i8* {}", temp));
        } else if left_type == "i8*" && (right_type == "float" || right_type == "double") {
            // 字符串 + 浮点数：先将浮点数转换为字符串，然后拼接
            let float_as_string = self.new_temp();
            // 根据类型选择正确的转换函数
            if right_type == "float" {
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_float_to_string(float {})",
                    float_as_string, right_val
                ));
            } else {
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_double_to_string(double {})",
                    float_as_string, right_val
                ));
            }
            self.emit_line(&format!(
                "  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                temp, left_val, float_as_string
            ));
            return Ok(format!("i8* {}", temp));
        } else if (left_type == "float" || left_type == "double") && right_type == "i8*" {
            // 浮点数 + 字符串：先将浮点数转换为字符串，然后拼接
            let float_as_string = self.new_temp();
            // 根据类型选择正确的转换函数
            if left_type == "float" {
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_float_to_string(float {})",
                    float_as_string, left_val
                ));
            } else {
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_double_to_string(double {})",
                    float_as_string, left_val
                ));
            }
            self.emit_line(&format!(
                "  {} = call i8* @__cay_string_concat(i8* {}, i8* {})",
                temp, float_as_string, right_val
            ));
            return Ok(format!("i8* {}", temp));
        } else if is_integer_type(left_type) && is_integer_type(right_type) {
            // 整数加法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = add {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if (left_type == "float" || left_type == "double")
            && (right_type == "float" || right_type == "double")
        {
            // 浮点数加法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = fadd {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if is_integer_type(left_type) && (right_type == "float" || right_type == "double") {
            // 整数 + 浮点数：将整数转换为浮点数
            let (promoted_type, promoted_right) = if right_type == "double" {
                ("double", right_val.to_string())
            } else {
                ("float", right_val.to_string())
            };
            let converted_left = self.new_temp();
            if promoted_type == "double" {
                self.emit_line(&format!(
                    "  {} = sitofp {} {} to double",
                    converted_left, left_type, left_val
                ));
            } else {
                self.emit_line(&format!(
                    "  {} = sitofp {} {} to float",
                    converted_left, left_type, left_val
                ));
            }
            self.emit_line(&format!(
                "  {} = fadd {} {}, {}",
                temp, promoted_type, converted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if (left_type == "float" || left_type == "double") && is_integer_type(right_type) {
            // 浮点数 + 整数：将整数转换为浮点数
            let (promoted_type, promoted_left) = if left_type == "double" {
                ("double", left_val.to_string())
            } else {
                ("float", left_val.to_string())
            };
            let converted_right = self.new_temp();
            if promoted_type == "double" {
                self.emit_line(&format!(
                    "  {} = sitofp {} {} to double",
                    converted_right, right_type, right_val
                ));
            } else {
                self.emit_line(&format!(
                    "  {} = sitofp {} {} to float",
                    converted_right, right_type, right_val
                ));
            }
            self.emit_line(&format!(
                "  {} = fadd {} {}, {}",
                temp, promoted_type, promoted_left, converted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsupported addition types: {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成减法表达式
    fn generate_sub(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 整数减法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = sub {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if (left_type == "float" || left_type == "double")
            && (right_type == "float" || right_type == "double")
        {
            // 浮点数减法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = fsub {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) =
            self.promote_mixed_operands(left_type, left_val, right_type, right_val)
        {
            // 混合类型：整数和浮点数
            self.emit_line(&format!(
                "  {} = fsub {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsupported subtraction types: {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成乘法表达式
    fn generate_mul(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 整数乘法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = mul {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if (left_type == "float" || left_type == "double")
            && (right_type == "float" || right_type == "double")
        {
            // 浮点数乘法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = fmul {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) =
            self.promote_mixed_operands(left_type, left_val, right_type, right_val)
        {
            // 混合类型：整数和浮点数
            self.emit_line(&format!(
                "  {} = fmul {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsupported multiplication types: {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成除法表达式
    fn generate_div(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 整数除法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            // 运行时除零检查
            self.generate_division_by_zero_check(&promoted_type, &promoted_right)?;
            self.emit_line(&format!(
                "  {} = sdiv {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if (left_type == "float" || left_type == "double")
            && (right_type == "float" || right_type == "double")
        {
            // 浮点数除法，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = fdiv {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) =
            self.promote_mixed_operands(left_type, left_val, right_type, right_val)
        {
            // 混合类型：整数和浮点数
            self.emit_line(&format!(
                "  {} = fdiv {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsupported division types: {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成取模表达式
    fn generate_mod(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 整数取模，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            // 运行时除零检查（取模也需要检查）
            self.generate_division_by_zero_check(&promoted_type, &promoted_right)?;
            self.emit_line(&format!(
                "  {} = srem {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!("Unsupported modulo types: {} and {}", left_type, right_type),
            ));
        }
    }

    /// 生成等于比较表达式
    fn generate_eq(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        // 处理任意指针类型的比较（包括 i8*, i64*, i32* 等）
        if left_type.ends_with("*") && right_type.ends_with("*") {
            self.emit_line(&format!(
                "  {} = icmp eq {} {}, {}",
                temp, left_type, left_val, right_val
            ));
            return Ok(format!("i1 {}", temp));
        } else if left_type.ends_with("*") && (right_val == "0" || right_val == "null") {
            // 指针与 null/0 比较
            self.emit_line(&format!(
                "  {} = icmp eq {} {}, null",
                temp, left_type, left_val
            ));
            return Ok(format!("i1 {}", temp));
        } else if right_type.ends_with("*") && (left_val == "0" || left_val == "null") {
            // null/0 与指针比较
            self.emit_line(&format!(
                "  {} = icmp eq {} {}, null",
                temp, right_type, right_val
            ));
            return Ok(format!("i1 {}", temp));
        } else if is_integer_type(left_type) && is_integer_type(right_type) {
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = icmp eq {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else if (left_type == "float" || left_type == "double")
            && (right_type == "float" || right_type == "double")
        {
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = fcmp oeq {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) =
            self.promote_mixed_operands(left_type, left_val, right_type, right_val)
        {
            // 混合类型：整数和浮点数
            self.emit_line(&format!(
                "  {} = fcmp oeq {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else if left_type == "{ i32, i64 }" && right_type == "{ i32, i64 }" {
            // 枚举类型比较：比较 discriminant (i32 部分)
            let left_disc = self.new_temp();
            let right_disc = self.new_temp();
            self.emit_line(&format!(
                "  {} = extractvalue {{ i32, i64 }} {}, 0",
                left_disc, left_val
            ));
            self.emit_line(&format!(
                "  {} = extractvalue {{ i32, i64 }} {}, 0",
                right_disc, right_val
            ));
            self.emit_line(&format!(
                "  {} = icmp eq i32 {}, {}",
                temp, left_disc, right_disc
            ));
            return Ok(format!("i1 {}", temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsupported equality comparison types: {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成不等于比较表达式
    fn generate_ne(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        // 处理任意指针类型的比较（包括 i8*, i64*, i32* 等）
        if left_type.ends_with("*") && right_type.ends_with("*") {
            self.emit_line(&format!(
                "  {} = icmp ne {} {}, {}",
                temp, left_type, left_val, right_val
            ));
            return Ok(format!("i1 {}", temp));
        } else if left_type.ends_with("*") && (right_val == "0" || right_val == "null") {
            // 指针与 null/0 比较
            self.emit_line(&format!(
                "  {} = icmp ne {} {}, null",
                temp, left_type, left_val
            ));
            return Ok(format!("i1 {}", temp));
        } else if right_type.ends_with("*") && (left_val == "0" || left_val == "null") {
            // null/0 与指针比较
            self.emit_line(&format!(
                "  {} = icmp ne {} {}, null",
                temp, right_type, right_val
            ));
            return Ok(format!("i1 {}", temp));
        } else if is_integer_type(left_type) && is_integer_type(right_type) {
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = icmp ne {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else if (left_type == "float" || left_type == "double")
            && (right_type == "float" || right_type == "double")
        {
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = fcmp one {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) =
            self.promote_mixed_operands(left_type, left_val, right_type, right_val)
        {
            // 混合类型：整数和浮点数
            self.emit_line(&format!(
                "  {} = fcmp one {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else if left_type == "{ i32, i64 }" && right_type == "{ i32, i64 }" {
            // 枚举类型比较：比较 discriminant (i32 部分)
            let left_disc = self.new_temp();
            let right_disc = self.new_temp();
            self.emit_line(&format!(
                "  {} = extractvalue {{ i32, i64 }} {}, 0",
                left_disc, left_val
            ));
            self.emit_line(&format!(
                "  {} = extractvalue {{ i32, i64 }} {}, 0",
                right_disc, right_val
            ));
            self.emit_line(&format!(
                "  {} = icmp ne i32 {}, {}",
                temp, left_disc, right_disc
            ));
            return Ok(format!("i1 {}", temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsupported inequality comparison types: {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成小于比较表达式
    fn generate_lt(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = icmp slt {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else if (left_type == "float" || left_type == "double")
            && (right_type == "float" || right_type == "double")
        {
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = fcmp olt {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) =
            self.promote_mixed_operands(left_type, left_val, right_type, right_val)
        {
            // 混合类型：整数和浮点数
            self.emit_line(&format!(
                "  {} = fcmp olt {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsupported less-than comparison types: {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成小于等于比较表达式
    fn generate_le(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = icmp sle {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else if (left_type == "float" || left_type == "double")
            && (right_type == "float" || right_type == "double")
        {
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = fcmp ole {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else if let Some((promoted_type, promoted_left, promoted_right)) =
            self.promote_mixed_operands(left_type, left_val, right_type, right_val)
        {
            // 混合类型：整数和浮点数
            self.emit_line(&format!(
                "  {} = fcmp ole {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("i1 {}", temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsupported less-or-equal comparison types: {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成大于比较表达式
    fn generate_gt(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 整数大于比较，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = icmp sgt {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
        } else if (left_type == "float" || left_type == "double")
            && (right_type == "float" || right_type == "double")
        {
            // 浮点数大于比较，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = fcmp ogt {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
        } else if let Some((promoted_type, promoted_left, promoted_right)) =
            self.promote_mixed_operands(left_type, left_val, right_type, right_val)
        {
            // 混合类型：整数和浮点数
            self.emit_line(&format!(
                "  {} = fcmp ogt {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsupported greater-than comparison types: {} and {}",
                    left_type, right_type
                ),
            ));
        }
        Ok(format!("i1 {}", temp))
    }

    /// 生成大于等于比较表达式
    fn generate_ge(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 整数大于等于比较，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = icmp sge {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
        } else if (left_type == "float" || left_type == "double")
            && (right_type == "float" || right_type == "double")
        {
            // 浮点数大于等于比较，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_float_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = fcmp oge {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
        } else if let Some((promoted_type, promoted_left, promoted_right)) =
            self.promote_mixed_operands(left_type, left_val, right_type, right_val)
        {
            // 混合类型：整数和浮点数
            self.emit_line(&format!(
                "  {} = fcmp oge {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsupported greater-than-or-equal comparison types: {} and {}",
                    left_type, right_type
                ),
            ));
        }
        Ok(format!("i1 {}", temp))
    }

    /// 将任意类型转换为 i1（用于短路求值的条件判断）
    fn convert_to_i1(&mut self, ty: &str, val: &str) -> String {
        if ty == "i1" {
            val.to_string()
        } else {
            let cmp_temp = self.new_temp();
            self.emit_line(&format!("  {} = icmp ne {} {}, 0", cmp_temp, ty, val));
            cmp_temp
        }
    }

    /// 生成短路求值的 && 表达式
    ///
    /// 左侧已求值，右侧在左侧为 true 时才惰性求值。
    /// 使用 alloca + store + load 模式生成 phi-free 的控制流。
    fn generate_short_circuit_and(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_expr: &Expr,
        temp: &str,
    ) -> cayResult<String> {
        let left_i1 = self.convert_to_i1(left_type, left_val);
        let eval_right = self.new_label("and.eval");
        let set_false = self.new_label("and.false");
        let merge = self.new_label("and.merge");

        let slot = self.new_temp();
        self.emit_line(&format!("  {} = alloca i1", slot));
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            left_i1, eval_right, set_false
        ));

        // 左侧为 false：直接存储 0，跳到合并点
        self.emit_line(&format!("\n{}:", set_false));
        self.emit_line(&format!("  store i1 0, i1* {}", slot));
        self.emit_line(&format!("  br label %{}", merge));

        // 左侧为 true：惰性求值右侧（关键！右侧在此块内才生成）
        self.emit_line(&format!("\n{}:", eval_right));
        let right_result = self.generate_expression(right_expr)?;
        let (right_type, right_val) = self.parse_typed_value(&right_result);
        let right_i1 = self.convert_to_i1(&right_type, &right_val);
        self.emit_line(&format!("  store i1 {}, i1* {}", right_i1, slot));
        self.emit_line(&format!("  br label %{}", merge));

        // 合并点
        self.emit_line(&format!("\n{}:", merge));
        self.emit_line(&format!("  {} = load i1, i1* {}", temp, slot));

        Ok(format!("i1 {}", temp))
    }

    /// 生成短路求值的 || 表达式
    ///
    /// 左侧已求值，右侧在左侧为 false 时才惰性求值。
    fn generate_short_circuit_or(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_expr: &Expr,
        temp: &str,
    ) -> cayResult<String> {
        let left_i1 = self.convert_to_i1(left_type, left_val);
        let eval_right = self.new_label("or.eval");
        let set_true = self.new_label("or.true");
        let merge = self.new_label("or.merge");

        let slot = self.new_temp();
        self.emit_line(&format!("  {} = alloca i1", slot));
        self.emit_line(&format!(
            "  br i1 {}, label %{}, label %{}",
            left_i1, set_true, eval_right
        ));

        // 左侧为 true：直接存储 1，跳到合并点
        self.emit_line(&format!("\n{}:", set_true));
        self.emit_line(&format!("  store i1 1, i1* {}", slot));
        self.emit_line(&format!("  br label %{}", merge));

        // 左侧为 false：惰性求值右侧
        self.emit_line(&format!("\n{}:", eval_right));
        let right_result = self.generate_expression(right_expr)?;
        let (right_type, right_val) = self.parse_typed_value(&right_result);
        let right_i1 = self.convert_to_i1(&right_type, &right_val);
        self.emit_line(&format!("  store i1 {}, i1* {}", right_i1, slot));
        self.emit_line(&format!("  br label %{}", merge));

        // 合并点
        self.emit_line(&format!("\n{}:", merge));
        self.emit_line(&format!("  {} = load i1, i1* {}", temp, slot));

        Ok(format!("i1 {}", temp))
    }

    /// 生成位与表达式
    fn generate_bitand(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 位与，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = and {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Bitwise AND requires integer operands, got {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成位或表达式
    fn generate_bitor(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 位或，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = or {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Bitwise OR requires integer operands, got {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成位异或表达式
    fn generate_bitxor(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 位异或，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = xor {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Bitwise XOR requires integer operands, got {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成左移表达式
    fn generate_shl(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 左移，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = shl {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Shift left requires integer operands, got {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成算术右移表达式
    fn generate_shr(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 算术右移，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = ashr {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Arithmetic shift right requires integer operands, got {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }

    /// 生成逻辑右移表达式
    fn generate_ushr(
        &mut self,
        left_type: &str,
        left_val: &str,
        right_type: &str,
        right_val: &str,
        temp: &str,
        loc: &SourceLocation,
    ) -> cayResult<String> {
        if is_integer_type(left_type) && is_integer_type(right_type) {
            // 逻辑右移，需要类型提升
            let (promoted_type, promoted_left, promoted_right) =
                self.promote_integer_operands(left_type, left_val, right_type, right_val);
            self.emit_line(&format!(
                "  {} = lshr {} {}, {}",
                temp, promoted_type, promoted_left, promoted_right
            ));
            return Ok(format!("{} {}", promoted_type, temp));
        } else {
            return Err(codegen_error_at(
                loc.clone(),
                format!(
                    "Unsigned shift right requires integer operands, got {} and {}",
                    left_type, right_type
                ),
            ));
        }
    }
}
