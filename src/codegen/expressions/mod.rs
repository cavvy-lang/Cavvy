//! 表达式代码生成子模块
//!
//! 本模块将表达式代码生成功能拆分为多个子模块，提高可维护性。
//!
//! # 模块结构
//!
//! - `main`: 表达式生成主入口，负责分发到具体模块
//! - `utils`: 通用工具函数（类型提升、左值信息等）
//! - `literal`: 字面量处理
//! - `identifier`: 标识符/变量访问
//! - `binary`: 二元表达式
//! - `unary`: 一元表达式
//! - `call`: 函数/方法调用
//! - `builtin`: 内置函数（print/read 等）
//! - `string_methods`: String 方法调用
//! - `array`: 数组创建、访问、初始化
//! - `cast`: 类型转换
//! - `member`: 成员访问
//! - `assignment`: 赋值表达式
//! - `new`: new 表达式
//! - `lambda`: Lambda 表达式和方法引用
//! - `ternary`: 三元运算符
//! - `instanceof`: instanceof 表达式

// 工具模块（需要最先加载）
mod utils;

// 基础表达式
mod main;
mod literal;
mod identifier;

// 运算符
mod binary;
mod unary;

// 调用相关
mod call;
mod builtin;
mod string_methods;

// 数组
mod array;

// 类型相关
mod cast;
mod member;
mod assignment;
mod new;

// 高级特性
mod lambda;
mod ternary;
mod instanceof;
