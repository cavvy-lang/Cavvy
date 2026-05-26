use crate::types::{Type, ParameterInfo, ClassInfo, MethodInfo};
use crate::error::SourceLocation;
use std::fmt;
use std::hash::{Hash, Hasher};

/// 提供位置信息的trait
pub trait HasLocation {
    fn location(&self) -> &SourceLocation;
}

#[derive(Debug, Clone)]
pub struct Program {
    pub classes: Vec<ClassDecl>,
    pub structs: Vec<StructDecl>,              // 用户自定义 struct 声明
    pub enums: Vec<EnumDecl>,                  // 用户自定义 enum 声明
    pub interfaces: Vec<InterfaceDecl>,
    pub top_level_functions: Vec<TopLevelFunction>,
    pub extern_declarations: Vec<ExternDecl>,  // FFI extern 声明
    pub type_aliases: Vec<TypeAliasDecl>,      // 类型别名声明 (type X = Y)
    pub namespace_path: Option<Vec<String>>,   // 文件级 namespace 路径 (namespace std;)
    pub namespace_decls: Vec<NamespaceDecl>,   // 块级 namespace 声明
    pub using_decls: Vec<UsingDecl>,           // using 声明
}

/// namespace 块级声明 - namespace std { ... }
#[derive(Debug, Clone)]
pub struct NamespaceDecl {
    pub path: Vec<String>,               // 命名空间路径，如 ["std", "io"]
    pub classes: Vec<ClassDecl>,
    pub structs: Vec<StructDecl>,
    pub enums: Vec<EnumDecl>,
    pub interfaces: Vec<InterfaceDecl>,
    pub top_level_functions: Vec<TopLevelFunction>,
    pub extern_declarations: Vec<ExternDecl>,
    pub type_aliases: Vec<TypeAliasDecl>,
    pub nested_namespaces: Vec<NamespaceDecl>,  // 嵌套 namespace
    pub loc: SourceLocation,
}

/// using 声明 - using std::StringBuilder;
#[derive(Debug, Clone)]
pub struct UsingDecl {
    pub path: Vec<String>,  // 完整路径，如 ["std", "StringBuilder"]，最后一个元素是要导入的名字
    pub loc: SourceLocation,
}

/// 类型别名声明 - type Name = Type;
#[derive(Debug, Clone)]
pub struct TypeAliasDecl {
    pub name: String,
    pub target_type: Type,
    pub namespace_path: Vec<String>,  // 所属命名空间路径
    pub loc: SourceLocation,
}

/// 顶层函数声明（类外函数）
#[derive(Debug, Clone)]
pub struct TopLevelFunction {
    pub name: String,
    pub modifiers: Vec<Modifier>,
    pub return_type: Type,
    pub params: Vec<ParameterInfo>,
    pub body: Block,
    pub namespace_path: Vec<String>,  // 所属命名空间路径
    pub loc: SourceLocation,
}

/// Extern 声明 - FFI 外部函数声明
#[derive(Debug, Clone)]
pub struct ExternDecl {
    pub calling_convention: CallingConvention,  // 调用约定
    pub functions: Vec<ExternFunction>,         // 声明的函数列表
    pub namespace_path: Vec<String>,            // 所属命名空间路径
    pub loc: SourceLocation,
}

/// 外部函数声明
#[derive(Debug, Clone)]
pub struct ExternFunction {
    pub name: String,           // 外部C函数名
    pub alias: Option<String>,  // 别名（用于Cavvy代码中调用）
    pub return_type: Type,
    pub params: Vec<ParameterInfo>,
    pub loc: SourceLocation,
}

/// 调用约定
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallingConvention {
    Cdecl,      // 默认 C 调用约定
    Stdcall,    // Windows stdcall
    Fastcall,   // fastcall
    Sysv64,     // System V AMD64 ABI (Linux/macOS)
    Win64,      // Windows x64 calling convention
}

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: String,
    pub modifiers: Vec<Modifier>,
    pub methods: Vec<MethodDecl>,
    pub namespace_path: Vec<String>,  // 所属命名空间路径
    pub loc: SourceLocation,
}

/// 用户自定义 struct 声明 - 值类型，栈分配
/// struct Point { int x; int y; }
#[derive(Debug, Clone)]
pub struct StructDecl {
    pub name: String,
    pub modifiers: Vec<Modifier>,
    pub fields: Vec<FieldDecl>,
    pub methods: Vec<MethodDecl>,
    pub namespace_path: Vec<String>,  // 所属命名空间路径
    pub loc: SourceLocation,
}

/// 用户自定义 enum 声明 - tagged union / ADT
/// enum Option<T> { Some(T), None }
#[derive(Debug, Clone)]
pub struct EnumDecl {
    pub name: String,
    pub modifiers: Vec<Modifier>,
    pub type_params: Vec<String>,          // 泛型类型参数：["T"]
    pub variants: Vec<EnumVariant>,
    pub namespace_path: Vec<String>,       // 所属命名空间路径
    pub loc: SourceLocation,
}

/// enum 的 variant - 可以携带数据
#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
    pub payload_type: Option<Type>,    // variant 携带的数据类型
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub name: String,
    pub modifiers: Vec<Modifier>,
    pub parent: Option<String>,
    pub interfaces: Vec<String>,  // 实现的接口列表
    pub members: Vec<ClassMember>,
    pub namespace_path: Vec<String>,  // 所属命名空间路径
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub enum ClassMember {
    Method(MethodDecl),
    Field(FieldDecl),
    Constructor(ConstructorDecl),
    Destructor(DestructorDecl),
    InstanceInitializer(Block),  // 实例初始化块 { ... }
    StaticInitializer(Block),    // 静态初始化块 static { ... }
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

/// 构造函数声明
#[derive(Debug, Clone)]
pub struct ConstructorDecl {
    pub modifiers: Vec<Modifier>,
    pub params: Vec<crate::types::ParameterInfo>,
    pub body: Block,
    pub constructor_call: Option<ConstructorCall>, // this() 或 super() 调用
    pub loc: SourceLocation,
}

/// 构造函数调用（this() 或 super()）
#[derive(Debug, Clone)]
pub enum ConstructorCall {
    This(Vec<Expr>),   // this(args)
    Super(Vec<Expr>),  // super(args)
}

/// 析构函数声明
#[derive(Debug, Clone)]
pub struct DestructorDecl {
    pub modifiers: Vec<Modifier>,
    pub body: Block,
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
    Main,      // 标记主类，用于解决多main冲突
    Override,  // @Override 注解，标记方法重写
    Test,      // @Test 注解，标记测试方法
    FreeFunction,  // @FreeFunction 注解，将类方法导出为可直接调用的顶层函数
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
    Scope(ScopeStmt),  // 0.5.0.0: scope 栈分配块
    Break(Option<String>),  // 可选的标签
    Continue(Option<String>),  // 可选的标签
    InlineIr(InlineIrStmt),  // 内联IR语句块
}

/// 内联IR语句 - __ir { ... }
#[derive(Debug, Clone)]
pub struct InlineIrStmt {
    pub raw_lines: Vec<String>,  // IR文本行
    pub loc: SourceLocation,
}

/// 0.5.0.0: scope 语句 - 栈作用域分配块
/// 用于在栈上分配临时对象，支持 RAII 模式
#[derive(Debug, Clone)]
pub struct ScopeStmt {
    pub body: Block,
    pub loc: SourceLocation,
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
    pub label: Option<String>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct ForStmt {
    pub init: Option<Box<Stmt>>,
    pub condition: Option<Expr>,
    pub update: Option<Expr>,
    pub body: Box<Stmt>,
    pub label: Option<String>,
    pub loc: SourceLocation,
}

/// do-while 循环语句
#[derive(Debug, Clone)]
pub struct DoWhileStmt {
    pub condition: Expr,
    pub body: Box<Stmt>,
    pub label: Option<String>,
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
    Literal(LiteralExpr),
    Identifier(IdentifierExpr),
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
    MethodRef(MethodRefExpr),  // 方法引用: ClassName::methodName
    Lambda(LambdaExpr),        // Lambda 表达式: (params) -> { body }
    Ternary(TernaryExpr),      // 三元运算符: condition ? true_expr : false_expr
    InstanceOf(InstanceOfExpr), // instanceof 运算符: obj instanceof Type
    Alloc(AllocExpr),          // 0.5.0.0: 内存分配表达式: __cay_alloc(size)
    Dealloc(DeallocExpr),      // 0.5.0.0: 内存释放表达式: __cay_free(ptr)
}

impl HasLocation for Expr {
    fn location(&self) -> &SourceLocation {
        match self {
            Expr::Literal(lit) => &lit.loc,
            Expr::Identifier(id) => &id.loc,
            Expr::Binary(bin) => &bin.loc,
            Expr::Unary(unary) => &unary.loc,
            Expr::Call(call) => &call.loc,
            Expr::MemberAccess(member) => &member.loc,
            Expr::New(new) => &new.loc,
            Expr::Assignment(assign) => &assign.loc,
            Expr::Cast(cast) => &cast.loc,
            Expr::ArrayCreation(arr) => &arr.loc,
            Expr::ArrayAccess(arr) => &arr.loc,
            Expr::ArrayInit(arr) => &arr.loc,
            Expr::MethodRef(method) => &method.loc,
            Expr::Lambda(lambda) => &lambda.loc,
            Expr::Ternary(ternary) => &ternary.loc,
            Expr::InstanceOf(instance) => &instance.loc,
            Expr::Alloc(alloc) => &alloc.loc,
            Expr::Dealloc(dealloc) => &dealloc.loc,
        }
    }
}

/// 0.5.0.0: 内存分配表达式
#[derive(Debug, Clone)]
pub struct AllocExpr {
    pub size: Box<Expr>,
    pub align: Option<Box<Expr>>,
    pub loc: SourceLocation,
}

/// 0.5.0.0: 内存释放表达式
#[derive(Debug, Clone)]
pub struct DeallocExpr {
    pub ptr: Box<Expr>,
    pub loc: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdentifierExpr {
    pub name: String,
    pub loc: SourceLocation,
}

impl fmt::Display for IdentifierExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl AsRef<str> for IdentifierExpr {
    fn as_ref(&self) -> &str {
        &self.name
    }
}

impl PartialEq<str> for IdentifierExpr {
    fn eq(&self, other: &str) -> bool {
        self.name == other
    }
}

impl PartialEq<IdentifierExpr> for str {
    fn eq(&self, other: &IdentifierExpr) -> bool {
        self == other.name
    }
}

impl IdentifierExpr {
    /// 返回名称的字符串切片
    pub fn as_str(&self) -> &str {
        &self.name
    }
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
pub struct LiteralExpr {
    pub value: LiteralValue,
    pub loc: SourceLocation,
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
    AddressOf,  // &variable - 取地址
    Deref,      // *pointer - 解引用
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

/// 数组创建表达式: new Type[size] 或 new Type[size1][size2]... 或 new Type[size]()
#[derive(Debug, Clone)]
pub struct ArrayCreationExpr {
    pub element_type: Type,
    pub sizes: Vec<Expr>,  // 支持多维数组，每个维度的大小
    pub zero_init: bool,   // 是否零初始化 new Type[size]()
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

/// 方法引用表达式: ClassName::methodName 或 obj::methodName
#[derive(Debug, Clone)]
pub struct MethodRefExpr {
    pub class_name: Option<String>,  // 类名（静态方法引用）
    pub object: Option<Box<Expr>>,   // 对象表达式（实例方法引用）
    pub method_name: String,
    pub loc: SourceLocation,
}

/// Lambda 表达式: (params) -> { body }
#[derive(Debug, Clone)]
pub struct LambdaExpr {
    pub params: Vec<LambdaParam>,
    pub body: LambdaBody,
    pub loc: SourceLocation,
}

/// Lambda 参数
#[derive(Debug, Clone)]
pub struct LambdaParam {
    pub name: String,
    pub param_type: Option<Type>,  // 可选的类型注解
}

/// Lambda 体（可以是表达式或语句块）
#[derive(Debug, Clone)]
pub enum LambdaBody {
    Expr(Box<Expr>),      // 单表达式: (x) -> x * 2
    Block(Block),         // 语句块: (x) -> { return x * 2; }
}

/// 三元运算符表达式: condition ? true_expr : false_expr
#[derive(Debug, Clone)]
pub struct TernaryExpr {
    pub condition: Box<Expr>,
    pub true_branch: Box<Expr>,
    pub false_branch: Box<Expr>,
    pub loc: SourceLocation,
}

/// instanceof 表达式: obj instanceof Type
#[derive(Debug, Clone)]
pub struct InstanceOfExpr {
    pub expr: Box<Expr>,
    pub target_type: crate::types::Type,
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

    /// 扁平化 namespace 声明：将块级 namespace 和文件级 namespace_path 应用到所有声明
    pub fn flatten_namespaces(&self) -> Program {
        let mut classes = self.classes.clone();
        let mut structs = self.structs.clone();
        let mut enums = self.enums.clone();
        let mut interfaces = self.interfaces.clone();
        let mut top_level_functions = self.top_level_functions.clone();
        let mut extern_declarations = self.extern_declarations.clone();
        let mut type_aliases = self.type_aliases.clone();
        let file_namespace = self.namespace_path.clone();

        // 调试信息
        // eprintln!("[DEBUG] flatten_namespaces:");
        // eprintln!("  - self.classes count: {}", self.classes.len());
        // for class in &self.classes {
        //     eprintln!("    - self.classes: {}, namespace_path: {:?}", class.name, class.namespace_path);
        // }
        // eprintln!("  - self.namespace_decls count: {}", self.namespace_decls.len());
        // for (i, ns) in self.namespace_decls.iter().enumerate() {
        //     eprintln!("  - namespace_decls[{}].path: {:?}", i, ns.path);
        //     eprintln!("  - namespace_decls[{}].classes count: {}", i, ns.classes.len());
        //     for class in &ns.classes {
        //         eprintln!("    - class: {}, namespace_path: {:?}", class.name, class.namespace_path);
        //     }
        //     eprintln!("  - namespace_decls[{}].nested_namespaces count: {}", i, ns.nested_namespaces.len());
        //     for (j, nested) in ns.nested_namespaces.iter().enumerate() {
        //         eprintln!("    - nested[{}].path: {:?}", j, nested.path);
        //         eprintln!("    - nested[{}].classes count: {}", j, nested.classes.len());
        //         for class in &nested.classes {
        //             eprintln!("      - class: {}, namespace_path: {:?}", class.name, class.namespace_path);
        //         }
        //     }
        // }

        // 递归扁平化块级 namespace
        fn flatten_ns(
            ns: &NamespaceDecl,
            parent_path: &[String],
            classes: &mut Vec<ClassDecl>,
            structs: &mut Vec<StructDecl>,
            enums: &mut Vec<EnumDecl>,
            interfaces: &mut Vec<InterfaceDecl>,
            top_level_functions: &mut Vec<TopLevelFunction>,
            extern_declarations: &mut Vec<ExternDecl>,
            type_aliases: &mut Vec<TypeAliasDecl>,
            depth: usize,
        ) {
            let mut full_path = parent_path.to_vec();
            full_path.extend(ns.path.clone());

            // eprintln!("[DEBUG] flatten_ns depth={} ns.path={:?} parent_path={:?} full_path={:?}", 
            //     depth, ns.path, parent_path, full_path);

            for mut class in ns.classes.clone() {
                // 如果类已经有 namespace_path（来自 #include 的文件），则只使用现有的 namespace_path
                // 否则使用 full_path
                if class.namespace_path.is_empty() {
                    class.namespace_path = full_path.clone();
                }
                // eprintln!("[DEBUG]   Adding class: {} with namespace_path: {:?}", class.name, class.namespace_path);
                classes.push(class);
            }
            for mut s in ns.structs.clone() {
                if s.namespace_path.is_empty() {
                    s.namespace_path = full_path.clone();
                }
                structs.push(s);
            }
            for mut e in ns.enums.clone() {
                if e.namespace_path.is_empty() {
                    e.namespace_path = full_path.clone();
                }
                enums.push(e);
            }
            for mut interface in ns.interfaces.clone() {
                if interface.namespace_path.is_empty() {
                    interface.namespace_path = full_path.clone();
                }
                interfaces.push(interface);
            }
            for mut func in ns.top_level_functions.clone() {
                if func.namespace_path.is_empty() {
                    func.namespace_path = full_path.clone();
                }
                top_level_functions.push(func);
            }
            for mut extern_decl in ns.extern_declarations.clone() {
                if extern_decl.namespace_path.is_empty() {
                    extern_decl.namespace_path = full_path.clone();
                }
                extern_declarations.push(extern_decl);
            }
            for mut alias in ns.type_aliases.clone() {
                if alias.namespace_path.is_empty() {
                    alias.namespace_path = full_path.clone();
                }
                type_aliases.push(alias);
            }
            for nested in &ns.nested_namespaces {
                // 对于嵌套的 namespace，如果它的 path 和当前 ns 的 path 相同，
                // 则使用 parent_path 作为基础，避免重复
                let nested_parent_path = if nested.path == ns.path {
                    parent_path.to_vec()
                } else {
                    full_path.clone()
                };
                // eprintln!("[DEBUG]   Processing nested namespace at depth {}: nested.path={:?}, ns.path={:?}, using parent_path={:?}", 
                //     depth, nested.path, ns.path, nested_parent_path);
                flatten_ns(nested, &nested_parent_path, classes, structs, enums, interfaces, top_level_functions, extern_declarations, type_aliases, depth + 1);
            }
        }

        for (i, ns) in self.namespace_decls.iter().enumerate() {
            // eprintln!("[DEBUG] Processing namespace_decls[{}]", i);
            flatten_ns(ns, &[], &mut classes, &mut structs, &mut enums, &mut interfaces, &mut top_level_functions, &mut extern_declarations, &mut type_aliases, 0);
        }

        // 如果有文件级 namespace，设置给所有没有 namespace_path 的全局声明
        if let Some(ref ns_path) = file_namespace {
            for class in &mut classes {
                if class.namespace_path.is_empty() {
                    class.namespace_path = ns_path.clone();
                }
            }
            for s in &mut structs {
                if s.namespace_path.is_empty() {
                    s.namespace_path = ns_path.clone();
                }
            }
            for e in &mut enums {
                if e.namespace_path.is_empty() {
                    e.namespace_path = ns_path.clone();
                }
            }
            for interface in &mut interfaces {
                if interface.namespace_path.is_empty() {
                    interface.namespace_path = ns_path.clone();
                }
            }
            for func in &mut top_level_functions {
                if func.namespace_path.is_empty() {
                    func.namespace_path = ns_path.clone();
                }
            }
            for extern_decl in &mut extern_declarations {
                if extern_decl.namespace_path.is_empty() {
                    extern_decl.namespace_path = ns_path.clone();
                }
            }
            for alias in &mut type_aliases {
                if alias.namespace_path.is_empty() {
                    alias.namespace_path = ns_path.clone();
                }
            }
        }

        Program {
            classes,
            structs,
            enums,
            interfaces,
            top_level_functions,
            extern_declarations,
            type_aliases,
            namespace_path: file_namespace,
            namespace_decls: Vec::new(),
            using_decls: self.using_decls.clone(),
        }
    }
}

impl Default for Program {
    fn default() -> Self {
        Self {
            classes: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
            interfaces: Vec::new(),
            top_level_functions: Vec::new(),
            extern_declarations: Vec::new(),
            type_aliases: Vec::new(),
            namespace_path: None,
            namespace_decls: Vec::new(),
            using_decls: Vec::new(),
        }
    }
}
