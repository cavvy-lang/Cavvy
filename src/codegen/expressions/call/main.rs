//! 函数调用表达式代码生成 - 主入口
//!
//! 处理函数调用表达式的主分发逻辑。
//! 各分支的具体实现拆分到同目录模块：
//! - dispatch.rs: 入口特殊调用分发（内建函数、类实例化、特殊成员调用、extern）
//! - target.rs:   调用目标解析（类名/方法名/接收者/静态标记）
//! - emit.rs:     参数构建、this 指针处理与调用 IR 发射

use crate::ast::*;
use crate::codegen::context::IRGenerator;
use crate::miette_diagnostic::CayResult;

use super::target::CallTargetResolution;

impl IRGenerator {
    /// 生成函数调用表达式代码
    ///
    /// # Arguments
    /// * `call` - 函数调用表达式
    pub fn generate_call_expression(&mut self, call: &CallExpr) -> CayResult<String> {
        // 捕获变量声明为泛型静态工厂调用（如 `Optional<int> x = Optional.of(42)`）设置的
        // 期望类型，供下方将静态方法调用特化到具体单态化版本时推断类型参数。
        // 在生成任何子表达式（可能清除该字段）之前读取。
        let expected_static_type = self.pending_new_expected_type.clone();
        // 进入本次调用前的泛型类型参数映射快照。下方静态工厂特化与接收者泛型安装
        // 都会修改该映射，调用结束时恢复到此快照，避免类型参数泄漏到后续代码。
        let entry_generic_args = self.generic_type_args.clone();

        // 处理标识符形式的特殊调用：内建函数、省略 new 的类实例化、
        // 命名空间式静态类方法调用
        if let Expr::Identifier(name) = call.callee.as_ref() {
            if let Some(result) = self.try_generate_builtin_call(call, name.as_str()) {
                return result;
            }
            if let Some(result) = self.try_generate_class_instantiation_call(call, name.as_ref()) {
                return result;
            }
            if let Some(result) = self.try_generate_scoped_static_call(call, name.as_ref()) {
                return result;
            }
        }

        // 处理成员访问形式的特殊调用：String 方法、数组 length()、
        // String.valueOf、Integer.parseInt、基本类型 toString
        if let Expr::MemberAccess(member) = call.callee.as_ref() {
            if let Some(result) = self.try_generate_special_member_call(call, member)? {
                return Ok(result);
            }
        }

        // 处理 extern 函数调用
        if let Some(result) = self.try_generate_extern_identifier_call(call) {
            return result;
        }

        // 处理普通函数调用（支持方法重载和可变参数）
        // 先确定方法信息（类名和方法名）
        // 对于实例方法调用，还需要保存对象表达式以获取 this 指针
        // is_static_call 表示是否是类名.方法名() 形式的静态方法调用
        let (class_name, method_name, obj_expr, is_static_call) =
            match self.resolve_call_target(call)? {
                CallTargetResolution::Resolved(
                    class_name,
                    method_name,
                    obj_expr,
                    is_static_call,
                ) => (class_name, method_name, obj_expr, is_static_call),
                CallTargetResolution::Generated(result) => return Ok(result),
            };

        // 泛型静态工厂调用的单态化与未解析泛型参数替换
        let class_name =
            self.specialize_call_class_name(class_name, is_static_call, &expected_static_type);

        // 安装接收者的具体泛型类型参数映射，使方法签名中的泛型参数（如参数/返回
        // 类型 T）能在参数类型解析与转换之前就解析为具体类型。静态泛型工厂调用的
        // 映射已在特化阶段按期望类型安装。调用结束后恢复到进入本函数时的快照。
        self.install_class_generic_args(&class_name);
        self.install_receiver_generic_args(&obj_expr);

        // 检查是否有命名参数需要重排
        let resolved_args = self.reorder_named_args(call, &class_name, &method_name)?;
        let actual_args: &[Expr] = resolved_args.as_deref().unwrap_or(&call.args);

        // 检查是否是可变参数方法
        let is_varargs_method = self.is_varargs_method(&class_name, &method_name);

        // 生成参数表达式，处理 ArrayList.add 的 RAII 所有权转移与可变参数打包
        let (processed_args, has_varargs_array) = self.generate_and_pack_args(
            &class_name,
            &method_name,
            actual_args,
            is_varargs_method,
        )?;

        // 检查是否是实例方法（需要传递 this）
        // 如果是 Class.method() 形式的静态方法调用，即使存在同名实例方法，也不传递 this
        let is_instance_method = if is_static_call {
            false
        } else {
            self.is_instance_method(&class_name, &method_name)
        };

        // 判断目标类型是否是 struct，决定 this 指针类型。
        // 对泛型特化使用完整 struct 类型名（如 Pair_int__String_），
        // 避免链式调用时退回到未定义的基名（如 Pair）。
        let is_struct_target = self.is_struct_type(&class_name);
        let this_llvm_type = if is_struct_target {
            let llvm_struct_name = self.struct_llvm_type_name(&class_name);
            format!("%struct.{}*", llvm_struct_name)
        } else {
            "i8*".to_string()
        };

        // 为实例方法添加 this 参数
        let mut final_args = Vec::new();
        let cached_obj_val = self.push_this_arg(
            &obj_expr,
            is_instance_method,
            is_struct_target,
            &this_llvm_type,
            &mut final_args,
        )?;

        // 获取方法的参数类型信息以进行必要的类型转换
        let param_types = self.get_method_param_types(
            &class_name,
            &method_name,
            &processed_args,
            has_varargs_array,
        );

        // 添加其他参数（根据需要进行类型转换）
        self.append_converted_args(&processed_args, &param_types, &mut final_args);

        // 生成函数名 - 使用类型注册表获取方法定义的参数类型
        // 注意：函数名不包含 this 参数，this 只在 IR 调用时传递
        let fn_name = self.generate_function_name(
            &class_name,
            &method_name,
            &processed_args,
            has_varargs_array,
        );

        // 获取方法的返回类型
        let ret_type = self.get_method_return_type(
            &class_name,
            &method_name,
            &processed_args,
            has_varargs_array,
        );
        let llvm_ret_type = self.type_to_llvm(&ret_type);

        // 预先计算 this 指针值（用于 vtable 分派和直接调用都可能需要）
        let resolved_this_val =
            self.resolve_this_value(is_static_call, &obj_expr, &cached_obj_val);

        // 发射调用 IR（vtable 动态分派或直接调用）
        let call_result = self.emit_method_call(
            &class_name,
            &method_name,
            is_static_call,
            is_instance_method,
            resolved_this_val,
            &param_types,
            &processed_args,
            &fn_name,
            &ret_type,
            &llvm_ret_type,
            &final_args,
        );

        // 恢复调用前的泛型类型参数映射
        self.generic_type_args = entry_generic_args;
        Ok(call_result)
    }
}
