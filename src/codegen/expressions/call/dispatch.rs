//! 函数调用表达式代码生成 - 特殊调用分发
//!
//! 处理 generate_call_expression 入口处的特殊调用形式分发：
//! 内建函数、省略 new 的类实例化、命名空间式静态调用、
//! String/数组/基本类型的特殊成员调用以及 extern 函数调用。
//! 命中时直接生成完整调用结果，未命中时返回 None 交回主流程。

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;

impl IRGenerator {
    /// 尝试生成内建函数调用：print/println/eprint/eprintln、exit、panic/abort、
    /// read 系列以及 __cay_ 运行时辅助函数。
    /// 命中时返回 Some(结果)，未命中返回 None。
    pub(crate) fn try_generate_builtin_call(
        &mut self,
        call: &CallExpr,
        name: &str,
    ) -> Option<CayResult<String>> {
        // 处理 print、println、eprint、eprintln 函数
        match name {
            "print" => Some(self.generate_print_call(
                &call.args,
                false,
                crate::codegen::expressions::PrintStream::Stdout,
                &call.loc,
            )),
            "println" => Some(self.generate_print_call(
                &call.args,
                true,
                crate::codegen::expressions::PrintStream::Stdout,
                &call.loc,
            )),
            "eprint" => Some(self.generate_print_call(
                &call.args,
                false,
                crate::codegen::expressions::PrintStream::Stderr,
                &call.loc,
            )),
            "eprintln" => Some(self.generate_print_call(
                &call.args,
                true,
                crate::codegen::expressions::PrintStream::Stderr,
                &call.loc,
            )),
            "exit" => Some(self.generate_exit_call(&call.args, &call.loc)),
            // 6.1.0: panic/abort 内置函数
            "panic" | "abort" => Some(self.generate_panic_call(&call.args, &call.loc)),
            "readInt" => Some(self.generate_read_int_call(&call.args, &call.loc)),
            "readLong" => Some(self.generate_read_long_call(&call.args, &call.loc)),
            "readFloat" => Some(self.generate_read_float_call(&call.args, &call.loc)),
            "readDouble" => Some(self.generate_read_double_call(&call.args, &call.loc)),
            "readLine" => Some(self.generate_read_line_call(&call.args, &call.loc)),
            "readChar" => Some(self.generate_read_char_call(&call.args, &call.loc)),
            // 运行时辅助函数
            "__cay_read_ptr" => Some(self.generate_read_ptr_call(&call.args, &call.loc)),
            "__cay_ptr_to_string" => Some(self.generate_ptr_to_string_call(&call.args, &call.loc)),
            "__cay_write_ptr" => Some(self.generate_write_ptr_call(&call.args, &call.loc)),
            "__cay_write_int" => Some(self.generate_write_int_call(&call.args, &call.loc)),
            "__cay_read_int" => Some(self.generate_cay_read_int_call(&call.args, &call.loc)),
            "__cay_array_base" => Some(self.generate_array_base_call(&call.args, &call.loc)),
            _ => None,
        }
    }

    /// 5.3.0: 支持省略 new 的类实例化 ClassName(args) / ClassName<T>(args)
    /// 当标识符是类名且不被局部变量/函数遮蔽时，生成 new 表达式代码
    pub(crate) fn try_generate_class_instantiation_call(
        &mut self,
        call: &CallExpr,
        name: &str,
    ) -> Option<CayResult<String>> {
        let class_name = self.try_resolve_class_instantiation(name)?;
        Some(self.generate_new_expression(&NewExpr {
            class_name,
            args: call.args.clone(),
            loc: call.loc.clone(),
        }))
    }

    /// 5.3.0: 支持命名空间式静态类方法调用 ClassName::staticMethod(args)
    /// 当标识符形如 ClassName::methodName 且前缀为类、后缀为静态方法时，
    /// 将其重写为 ClassName.staticMethod(args) 进行代码生成
    pub(crate) fn try_generate_scoped_static_call(
        &mut self,
        call: &CallExpr,
        name: &str,
    ) -> Option<CayResult<String>> {
        let (class_name, method_name) = self.try_resolve_static_method_call(name)?;
        let member_call = CallExpr {
            callee: Box::new(Expr::MemberAccess(MemberAccessExpr {
                object: Box::new(Expr::Identifier(IdentifierExpr {
                    name: class_name,
                    loc: call.callee.location().clone(),
                })),
                member: method_name,
                loc: call.callee.location().clone(),
            })),
            args: call.args.clone(),
            loc: call.loc.clone(),
        };
        Some(self.generate_call_expression(&member_call))
    }

    /// 尝试生成成员访问形式的特殊调用：
    /// String 方法、数组 length()、String.valueOf、Integer.parseInt、基本类型 toString。
    /// 命中时返回 Ok(Some(结果))，未命中返回 Ok(None) 交回主流程。
    pub(crate) fn try_generate_special_member_call(
        &mut self,
        call: &CallExpr,
        member: &MemberAccessExpr,
    ) -> CayResult<Option<String>> {
        // 检查是否是 String 方法调用
        if let Some(method_result) = self.try_generate_string_method_call(member, &call.args)? {
            return Ok(Some(method_result));
        }

        // 处理数组的 length() 方法调用（作为 length 属性的语法糖）
        if member.member == "length" && call.args.is_empty() {
            // 检查对象是否是数组类型
            if let Some(var_type) = self.get_expression_type(&member.object) {
                if matches!(var_type, crate::types::Type::Array(_)) {
                    // 将 length() 转换为 length 属性访问
                    return self.generate_array_length_access(&member.object).map(Some);
                }
            }
        }

        // 处理 String.valueOf() 静态方法
        if let Expr::Identifier(class_name) = member.object.as_ref() {
            if class_name == "String" && member.member == "valueOf" {
                return self
                    .generate_string_valueof_call(&call.args, &call.loc)
                    .map(Some);
            }
        }

        // 处理 Integer.parseInt() 静态方法
        if let Expr::Identifier(class_name) = member.object.as_ref() {
            if class_name == "Integer" && member.member == "parseInt" {
                return self
                    .generate_integer_parseint_call(&call.args, &call.loc)
                    .map(Some);
            }
        }

        // 处理基本类型的 toString() 方法调用
        if member.member == "toString" && call.args.is_empty() {
            if let Some(result) = self.try_generate_primitive_tostring_call(member)? {
                return Ok(Some(result));
            }
        }

        Ok(None)
    }

    /// 处理基本类型的 toString() 方法调用。
    /// 注意：对象表达式的求值与临时变量分配在类型匹配之前发生，
    /// 即使类型不匹配也会保留这些副作用（与原实现一致）。
    fn try_generate_primitive_tostring_call(
        &mut self,
        member: &MemberAccessExpr,
    ) -> CayResult<Option<String>> {
        let Some(obj_type) = self.get_expression_type(&member.object) else {
            return Ok(None);
        };
        let obj_val = self.generate_expression(&member.object)?;
        let (_, val) = self.parse_typed_value(&obj_val);
        let temp = self.new_temp();
        match obj_type {
            crate::types::Type::Int32 => {
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_int_to_string(i32 {})",
                    temp, val
                ));
                Ok(Some(format!("i8* {}", temp)))
            }
            crate::types::Type::Int64 => {
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_long_to_string(i64 {})",
                    temp, val
                ));
                Ok(Some(format!("i8* {}", temp)))
            }
            crate::types::Type::Float32 => {
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_float_to_string(float {})",
                    temp, val
                ));
                Ok(Some(format!("i8* {}", temp)))
            }
            crate::types::Type::Float64 => {
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_double_to_string(double {})",
                    temp, val
                ));
                Ok(Some(format!("i8* {}", temp)))
            }
            crate::types::Type::Bool => {
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_bool_to_string(i1 {})",
                    temp, val
                ));
                Ok(Some(format!("i8* {}", temp)))
            }
            crate::types::Type::Char => {
                self.emit_line(&format!(
                    "  {} = call i8* @__cay_char_to_string(i8 {})",
                    temp, val
                ));
                Ok(Some(format!("i8* {}", temp)))
            }
            _ => Ok(None),
        }
    }

    /// 处理标识符形式的 extern 函数调用
    pub(crate) fn try_generate_extern_identifier_call(
        &mut self,
        call: &CallExpr,
    ) -> Option<CayResult<String>> {
        if let Expr::Identifier(name) = call.callee.as_ref() {
            let func_name = name.as_ref();
            if self.is_extern_function(func_name) {
                return Some(self.generate_extern_function_call(func_name, &call.args, &call.loc));
            }
        }
        None
    }
}
