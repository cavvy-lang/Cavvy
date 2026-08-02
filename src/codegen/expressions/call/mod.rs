//! 函数调用表达式代码生成
//!
//! 处理函数调用、内置函数（print/read）、String 方法调用和可变参数。

mod dispatch;
mod emit;
mod extern_call;
mod function_name;
mod helpers;
mod main;
mod member_ptr;
mod method_generic;
mod resolution;
mod special_calls;
mod target;
mod varargs;

pub use main::*;
