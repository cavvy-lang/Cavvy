//! 表达式代码生成主入口
//!
//! 这是表达式代码生成的统一入口点，根据表达式类型分发到具体的处理函数。

use crate::codegen::context::IRGenerator;
use crate::ast::*;
use crate::error::cayResult;

impl IRGenerator {
    /// 生成表达式代码的主入口
    ///
    /// 根据表达式类型，将生成任务分发给相应的子模块。
    ///
    /// # Arguments
    /// * `expr` - AST 表达式节点
    ///
    /// # Returns
    /// 格式为 "type value" 的 LLVM IR 值字符串
    pub fn generate_expression(&mut self, expr: &Expr) -> cayResult<String> {
        match expr {
            // 字面量
            Expr::Literal(lit) => self.generate_literal(lit),
            
            // 标识符（变量访问）
            Expr::Identifier(name) => self.generate_identifier(name),
            
            // 二元表达式
            Expr::Binary(bin) => self.generate_binary_expression(bin),
            
            // 一元表达式
            Expr::Unary(unary) => self.generate_unary_expression(unary),
            
            // 函数/方法调用
            Expr::Call(call) => self.generate_call_expression(call),
            
            // 赋值表达式
            Expr::Assignment(assign) => self.generate_assignment(assign),
            
            // 类型转换
            Expr::Cast(cast) => self.generate_cast_expression(cast),
            
            // 成员访问
            Expr::MemberAccess(member) => self.generate_member_access(member),
            
            // new 表达式
            Expr::New(new_expr) => self.generate_new_expression(new_expr),
            
            // 数组创建
            Expr::ArrayCreation(arr) => self.generate_array_creation(arr),
            
            // 数组访问
            Expr::ArrayAccess(arr) => self.generate_array_access(arr),
            
            // 数组初始化
            Expr::ArrayInit(init) => self.generate_array_init(init),
            
            // 方法引用
            Expr::MethodRef(method_ref) => self.generate_method_ref(method_ref),
            
            // Lambda 表达式
            Expr::Lambda(lambda) => self.generate_lambda(lambda),
            
            // 三元运算符
            Expr::Ternary(ternary) => self.generate_ternary_expression(ternary),
            
            // instanceof
            Expr::InstanceOf(instanceof) => self.generate_instanceof_expression(instanceof),
        }
    }
}
