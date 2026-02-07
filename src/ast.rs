use crate::types::{Type, ParameterInfo, ClassInfo, MethodInfo};
use crate::error::SourceLocation;

#[derive(Debug, Clone)]
pub struct Program {
    pub classes: Vec<ClassDecl>,
}

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub name: String,
    pub modifiers: Vec<Modifier>,
    pub parent: Option<String>,
    pub members: Vec<ClassMember>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub enum ClassMember {
    Method(MethodDecl),
    Field(FieldDecl),
}

#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub name: String,
    pub modifiers: Vec<Modifier>,
    pub return_type: Type,
    pub params: Vec<ParameterInfo>,
    pub body: Option<Block>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct FieldDecl {
    pub name: String,
    pub field_type: Type,
    pub modifiers: Vec<Modifier>,
    pub initializer: Option<Expr>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Modifier {
    Public,
    Private,
    Protected,
    Static,
    Final,
    Abstract,
    Native,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Stmt>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Expr(Expr),
    VarDecl(VarDecl),
    Return(Option<Expr>),
    If(IfStmt),
    While(WhileStmt),
    For(ForStmt),
    DoWhile(DoWhileStmt),
    Switch(SwitchStmt),
    Block(Block),
    Break,
    Continue,
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub name: String,
    pub var_type: Type,
    pub initializer: Option<Expr>,
    pub is_final: bool,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct IfStmt {
    pub condition: Expr,
    pub then_branch: Box<Stmt>,
    pub else_branch: Option<Box<Stmt>>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct WhileStmt {
    pub condition: Expr,
    pub body: Box<Stmt>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct ForStmt {
    pub init: Option<Box<Stmt>>,
    pub condition: Option<Expr>,
    pub update: Option<Expr>,
    pub body: Box<Stmt>,
    pub loc: SourceLocation,
}

/// do-while 循环语句
#[derive(Debug, Clone)]
pub struct DoWhileStmt {
    pub condition: Expr,
    pub body: Box<Stmt>,
    pub loc: SourceLocation,
}

/// switch case 分支
#[derive(Debug, Clone)]
pub struct Case {
    pub value: i64,
    pub body: Vec<Stmt>,
}

/// switch 语句
#[derive(Debug, Clone)]
pub struct SwitchStmt {
    pub expr: Expr,
    pub cases: Vec<Case>,
    pub default: Option<Vec<Stmt>>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Literal(LiteralValue),
    Identifier(String),
    Binary(BinaryExpr),
    Unary(UnaryExpr),
    Call(CallExpr),
    MemberAccess(MemberAccessExpr),
    New(NewExpr),
    Assignment(AssignmentExpr),
    Cast(CastExpr),
    ArrayCreation(ArrayCreationExpr),
    ArrayAccess(ArrayAccessExpr),
    ArrayInit(ArrayInitExpr),  // 数组初始化: {1, 2, 3}
}

#[derive(Debug, Clone)]
pub enum LiteralValue {
    Int32(i32),
    Int64(i64),
    Float32(f32),
    Float64(f64),
    String(String),
    Bool(bool),
    Char(char),
    Null,
}

#[derive(Debug, Clone)]
pub struct BinaryExpr {
    pub left: Box<Expr>,
    pub op: BinaryOp,
    pub right: Box<Expr>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    UnsignedShr,
}

#[derive(Debug, Clone)]
pub struct UnaryExpr {
    pub op: UnaryOp,
    pub operand: Box<Expr>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
    BitNot,
    PreInc,
    PreDec,
    PostInc,
    PostDec,
}

#[derive(Debug, Clone)]
pub struct CallExpr {
    pub callee: Box<Expr>,
    pub args: Vec<Expr>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct MemberAccessExpr {
    pub object: Box<Expr>,
    pub member: String,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct NewExpr {
    pub class_name: String,
    pub args: Vec<Expr>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct AssignmentExpr {
    pub target: Box<Expr>,
    pub value: Box<Expr>,
    pub op: AssignOp,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
}

#[derive(Debug, Clone)]
pub struct CastExpr {
    pub expr: Box<Expr>,
    pub target_type: Type,
    pub loc: SourceLocation,
}

/// 数组创建表达式: new Type[size] 或 new Type[size1][size2]...
#[derive(Debug, Clone)]
pub struct ArrayCreationExpr {
    pub element_type: Type,
    pub sizes: Vec<Expr>,  // 支持多维数组，每个维度的大小
    pub loc: SourceLocation,
}

/// 数组初始化表达式: {1, 2, 3}
#[derive(Debug, Clone)]
pub struct ArrayInitExpr {
    pub elements: Vec<Expr>,
    pub loc: SourceLocation,
}

/// 数组访问表达式: arr[index]
#[derive(Debug, Clone)]
pub struct ArrayAccessExpr {
    pub array: Box<Expr>,
    pub index: Box<Expr>,
    pub loc: SourceLocation,
}

impl Program {
    pub fn find_main_class(&self) -> Option<&ClassDecl> {
        self.classes.iter().find(|c| {
            c.members.iter().any(|m| {
                if let ClassMember::Method(method) = m {
                    method.name == "main" 
                        && method.modifiers.contains(&Modifier::Public)
                        && method.modifiers.contains(&Modifier::Static)
                        && method.params.is_empty()
                        && method.return_type == Type::Void
                } else {
                    false
                }
            })
        })
    }
}
