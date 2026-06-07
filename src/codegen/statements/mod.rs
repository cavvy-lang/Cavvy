//! 语句代码生成模块
//!
//! 本模块将语句代码生成拆分为多个子模块以提高可维护性。

mod block;
mod if_stmt;
mod jump_stmt;
mod loops;
mod return_stmt;
mod scope_stmt;
mod statement;
mod switch_stmt;
mod var_decl;

pub use block::*;
pub use if_stmt::*;
pub use jump_stmt::*;
pub use loops::*;
pub use return_stmt::*;
pub use scope_stmt::*;
pub use statement::*;
pub use switch_stmt::*;
pub use var_decl::*;
