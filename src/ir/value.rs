//! IR 值与指令系统
//!
//! 定义 IR 值、指令和终止指令，构成基本块的内容。

use super::types::IrType;
use serde::Serialize;
use std::fmt;

/// IR 值 - 表示计算中的值
///
/// 每个值可以是：
/// - 常量（字面量）
/// - 寄存器（SSA 临时变量）
/// - 全局引用
/// - 参数引用
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum IrValue {
    /// 整数常量
    IntConst(i64, IrType),
    /// 浮点常量
    FloatConst(f64, IrType),
    /// 布尔常量
    BoolConst(bool),
    /// 字符串常量引用
    StringConst(String),
    /// 空值常量
    NullConst(IrType),
    /// SSA 寄存器引用
    Register(String, IrType),
    /// 全局变量引用（如 @ClassName.fieldName）
    GlobalRef(String, IrType),
    /// 函数参数引用
    Param(String, IrType),
    /// 未定义值 (undef)
    Undef(IrType),
}

impl IrValue {
    /// 获取值的类型
    pub fn ir_type(&self) -> IrType {
        match self {
            IrValue::IntConst(_, ty) => ty.clone(),
            IrValue::FloatConst(_, ty) => ty.clone(),
            IrValue::BoolConst(_) => IrType::I1,
            IrValue::StringConst(_) => IrType::Pointer(Box::new(IrType::I8)),
            IrValue::NullConst(ty) => ty.clone(),
            IrValue::Register(_, ty) => ty.clone(),
            IrValue::GlobalRef(_, ty) => ty.clone(),
            IrValue::Param(_, ty) => ty.clone(),
            IrValue::Undef(ty) => ty.clone(),
        }
    }

    /// 获取值的 LLVM IR 文本表示
    pub fn to_llvm_str(&self) -> String {
        match self {
            IrValue::IntConst(v, ty) => format!("{} {}", ty.to_llvm_str(), v),
            IrValue::FloatConst(v, ty) => {
                if v.is_nan() {
                    match ty {
                        IrType::F32 => format!("float 0x{:08X}", f32::NAN.to_bits()),
                        IrType::F64 => format!("double 0x{:016X}", f64::NAN.to_bits()),
                        _ => format!("{} {}", ty.to_llvm_str(), v),
                    }
                } else if v.is_infinite() {
                    match ty {
                        IrType::F32 => {
                            let bits = if *v > 0.0 {
                                f32::INFINITY.to_bits()
                            } else {
                                f32::NEG_INFINITY.to_bits()
                            };
                            format!("float 0x{:08X}", bits)
                        }
                        IrType::F64 => {
                            let bits = if *v > 0.0 {
                                f64::INFINITY.to_bits()
                            } else {
                                f64::NEG_INFINITY.to_bits()
                            };
                            format!("double 0x{:016X}", bits)
                        }
                        _ => format!("{} {}", ty.to_llvm_str(), v),
                    }
                } else {
                    format!("{} {:.6e}", ty.to_llvm_str(), v)
                }
            }
            IrValue::BoolConst(v) => format!("i1 {}", if *v { 1 } else { 0 }),
            IrValue::StringConst(s) => {
                let escaped = s
                    .replace("\\", "\\\\")
                    .replace("\"", "\\\"")
                    .replace("\n", "\\0A")
                    .replace("\r", "\\0D")
                    .replace("\t", "\\09");
                format!("c\"{}\\00\"", escaped)
            }
            IrValue::NullConst(ty) => format!("{} null", ty.to_llvm_str()),
            IrValue::Register(name, _) => name.clone(),
            IrValue::GlobalRef(name, _) => name.clone(),
            IrValue::Param(name, _) => name.clone(),
            IrValue::Undef(ty) => format!("{} undef", ty.to_llvm_str()),
        }
    }

    /// 获取值的不带类型前缀的原始表示（用于 ret 等上下文）
    pub fn to_raw_str(&self) -> String {
        match self {
            IrValue::IntConst(v, _) => v.to_string(),
            IrValue::FloatConst(v, ty) => {
                if v.is_nan() {
                    match ty {
                        IrType::F32 => format!("0x{:08X}", f32::NAN.to_bits()),
                        IrType::F64 => format!("0x{:016X}", f64::NAN.to_bits()),
                        _ => format!("{:.6e}", v),
                    }
                } else if v.is_infinite() {
                    match ty {
                        IrType::F32 => {
                            let bits = if *v > 0.0 {
                                f32::INFINITY.to_bits()
                            } else {
                                f32::NEG_INFINITY.to_bits()
                            };
                            format!("0x{:08X}", bits)
                        }
                        IrType::F64 => {
                            let bits = if *v > 0.0 {
                                f64::INFINITY.to_bits()
                            } else {
                                f64::NEG_INFINITY.to_bits()
                            };
                            format!("0x{:016X}", bits)
                        }
                        _ => format!("{:.6e}", v),
                    }
                } else {
                    format!("{:.6e}", v)
                }
            }
            IrValue::BoolConst(v) => {
                if *v {
                    "1".to_string()
                } else {
                    "0".to_string()
                }
            }
            IrValue::StringConst(s) => format!("@.str.{}", s.len()), // 返回字符串常量名称
            IrValue::NullConst(_) => "null".to_string(),
            IrValue::Register(name, _) => name.clone(),
            IrValue::GlobalRef(name, _) => name.clone(),
            IrValue::Param(name, _) => name.clone(),
            IrValue::Undef(_) => "undef".to_string(),
        }
    }

    /// 是否是寄存器引用
    pub fn is_register(&self) -> bool {
        matches!(self, IrValue::Register(_, _))
    }

    /// 是否是常量
    pub fn is_const(&self) -> bool {
        matches!(
            self,
            IrValue::IntConst(_, _)
                | IrValue::FloatConst(_, _)
                | IrValue::BoolConst(_)
                | IrValue::StringConst(_)
                | IrValue::NullConst(_)
                | IrValue::Undef(_)
        )
    }
}

impl fmt::Display for IrValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_llvm_str())
    }
}

/// 二元运算操作符
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum IrBinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    And,
    Or,
    Xor,
    Shl,
    Shr,
    LShr,
    FAdd,
    FSub,
    FMul,
    FDiv,
    FRem,
}

impl IrBinaryOp {
    pub fn to_llvm_str(&self) -> &'static str {
        match self {
            IrBinaryOp::Add => "add",
            IrBinaryOp::Sub => "sub",
            IrBinaryOp::Mul => "mul",
            IrBinaryOp::Div => "sdiv",
            IrBinaryOp::Mod => "srem",
            IrBinaryOp::And => "and",
            IrBinaryOp::Or => "or",
            IrBinaryOp::Xor => "xor",
            IrBinaryOp::Shl => "shl",
            IrBinaryOp::Shr => "ashr",
            IrBinaryOp::LShr => "lshr",
            IrBinaryOp::FAdd => "fadd",
            IrBinaryOp::FSub => "fsub",
            IrBinaryOp::FMul => "fmul",
            IrBinaryOp::FDiv => "fdiv",
            IrBinaryOp::FRem => "frem",
        }
    }
}

/// 类型转换操作
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum IrCastKind {
    /// 符号扩展 (sext)
    SignExt,
    /// 零扩展 (zext)
    ZeroExt,
    /// 截断 (trunc)
    Trunc,
    /// 整数到浮点 (sitofp)
    IntToFloat,
    /// 浮点到整数 (fptosi)
    FloatToInt,
    /// 浮点扩展 (fpext)
    FloatExt,
    /// 浮点截断 (fptrunc)
    FloatTrunc,
    /// 位转换 (bitcast)
    BitCast,
    /// 指针到整数 (ptrtoint)
    PtrToInt,
    /// 整数到指针 (inttoptr)
    IntToPtr,
}

impl IrCastKind {
    pub fn to_llvm_str(&self) -> &'static str {
        match self {
            IrCastKind::SignExt => "sext",
            IrCastKind::ZeroExt => "zext",
            IrCastKind::Trunc => "trunc",
            IrCastKind::IntToFloat => "sitofp",
            IrCastKind::FloatToInt => "fptosi",
            IrCastKind::FloatExt => "fpext",
            IrCastKind::FloatTrunc => "fptrunc",
            IrCastKind::BitCast => "bitcast",
            IrCastKind::PtrToInt => "ptrtoint",
            IrCastKind::IntToPtr => "inttoptr",
        }
    }
}

/// 比较操作符
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum IrCmpOp {
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge,
    Ult,
    Ule,
    Ugt,
    Uge,
    FEq,
    FNe,
    FLt,
    FLe,
    FGt,
    FGe,
}

impl IrCmpOp {
    pub fn to_llvm_str(&self, is_float: bool) -> &'static str {
        match self {
            IrCmpOp::Eq => "eq",
            IrCmpOp::Ne => "ne",
            IrCmpOp::Slt => "slt",
            IrCmpOp::Sle => "sle",
            IrCmpOp::Sgt => "sgt",
            IrCmpOp::Sge => "sge",
            IrCmpOp::Ult => "ult",
            IrCmpOp::Ule => "ule",
            IrCmpOp::Ugt => "ugt",
            IrCmpOp::Uge => "uge",
            IrCmpOp::FEq => {
                if is_float {
                    "oeq"
                } else {
                    "eq"
                }
            }
            IrCmpOp::FNe => {
                if is_float {
                    "one"
                } else {
                    "ne"
                }
            }
            IrCmpOp::FLt => {
                if is_float {
                    "olt"
                } else {
                    "slt"
                }
            }
            IrCmpOp::FLe => {
                if is_float {
                    "ole"
                } else {
                    "sle"
                }
            }
            IrCmpOp::FGt => {
                if is_float {
                    "ogt"
                } else {
                    "sgt"
                }
            }
            IrCmpOp::FGe => {
                if is_float {
                    "oge"
                } else {
                    "sge"
                }
            }
        }
    }
}

/// IR 指令 - 基本块内的非终止指令
#[derive(Debug, Clone, Serialize)]
pub enum IrInstruction {
    /// alloca: 栈上分配内存
    Alloca {
        result: IrValue,
        ty: IrType,
        align: u32,
    },

    /// load: 从内存加载值
    Load {
        result: IrValue,
        ptr: IrValue,
        ty: IrType,
    },

    /// store: 将值存储到内存
    Store {
        value: IrValue,
        ptr: IrValue,
        ty: IrType,
    },

    /// 二元运算
    BinaryOp {
        result: IrValue,
        op: IrBinaryOp,
        left: IrValue,
        right: IrValue,
    },

    /// 比较运算
    Compare {
        result: IrValue,
        op: IrCmpOp,
        left: IrValue,
        right: IrValue,
    },

    /// 类型转换
    Cast {
        result: IrValue,
        kind: IrCastKind,
        value: IrValue,
        to_ty: IrType,
    },

    /// 函数调用
    Call {
        result: Option<IrValue>,
        func_name: String,
        args: Vec<IrValue>,
        return_ty: IrType,
    },

    /// getelementptr: 指针计算
    GetElementPtr {
        result: IrValue,
        ptr: IrValue,
        indices: Vec<IrValue>,
        base_ty: IrType,
    },

    /// bitcast: 位转换
    BitCast {
        result: IrValue,
        value: IrValue,
        to_ty: IrType,
    },

    /// phi 节点: SSA φ函数
    Phi {
        result: IrValue,
        ty: IrType,
        incoming: Vec<(IrValue, String)>,
    },

    /// select: 三元选择
    Select {
        result: IrValue,
        condition: IrValue,
        true_val: IrValue,
        false_val: IrValue,
    },

    /// 内联 IR: 嵌入原始 LLVM IR 文本
    /// 用于 __ir { ... } 块和需要直接控制 IR 生成的场景
    InlineIr {
        /// 原始 LLVM IR 文本行
        lines: Vec<String>,
        /// 此内联 IR 产生的输出值（供后续指令引用）
        outputs: Vec<IrValue>,
        /// 此内联 IR 引用的输入值（必须在此指令前已定义）
        inputs: Vec<IrValue>,
    },

    /// 注释（仅用于调试，不产生代码）
    Comment { text: String },

    /// 源位置信息（用于调试）
    SourceLocation { line: u32, column: u32 },

    /// 变量声明标记（用于作用域管理）
    VarDecl {
        name: String,
        alloca_reg: IrValue,
        ty: IrType,
    },
}

impl IrInstruction {
    /// 获取指令产生的结果值（如果有）
    pub fn result(&self) -> Option<&IrValue> {
        match self {
            IrInstruction::Alloca { result, .. } => Some(result),
            IrInstruction::Load { result, .. } => Some(result),
            IrInstruction::Store { .. } => None,
            IrInstruction::BinaryOp { result, .. } => Some(result),
            IrInstruction::Compare { result, .. } => Some(result),
            IrInstruction::Cast { result, .. } => Some(result),
            IrInstruction::Call { result, .. } => result.as_ref(),
            IrInstruction::GetElementPtr { result, .. } => Some(result),
            IrInstruction::BitCast { result, .. } => Some(result),
            IrInstruction::Phi { result, .. } => Some(result),
            IrInstruction::Select { result, .. } => Some(result),
            IrInstruction::InlineIr { outputs, .. } => outputs.first(),
            IrInstruction::Comment { .. } => None,
            IrInstruction::SourceLocation { .. } => None,
            IrInstruction::VarDecl { .. } => None,
        }
    }

    /// 获取指令引用的所有输入值
    pub fn inputs(&self) -> Vec<&IrValue> {
        match self {
            IrInstruction::Alloca { .. } => vec![],
            IrInstruction::Load { ptr, .. } => vec![ptr],
            IrInstruction::Store { value, ptr, .. } => vec![value, ptr],
            IrInstruction::BinaryOp { left, right, .. } => vec![left, right],
            IrInstruction::Compare { left, right, .. } => vec![left, right],
            IrInstruction::Cast { value, .. } => vec![value],
            IrInstruction::Call { args, .. } => args.iter().collect(),
            IrInstruction::GetElementPtr { ptr, indices, .. } => {
                let mut v: Vec<&IrValue> = vec![ptr];
                v.extend(indices.iter());
                v
            }
            IrInstruction::BitCast { value, .. } => vec![value],
            IrInstruction::Phi { incoming, .. } => incoming.iter().map(|(v, _)| v).collect(),
            IrInstruction::Select {
                condition,
                true_val,
                false_val,
                ..
            } => vec![condition, true_val, false_val],
            IrInstruction::InlineIr { inputs, .. } => inputs.iter().collect(),
            IrInstruction::Comment { .. } => vec![],
            IrInstruction::SourceLocation { .. } => vec![],
            IrInstruction::VarDecl { .. } => vec![],
        }
    }
}

/// IR 终止指令 - 基本块的结束指令
#[derive(Debug, Clone, Serialize)]
pub enum IrTerminator {
    /// ret void 或 ret <value>
    Return { value: Option<IrValue> },

    /// 无条件跳转
    Branch { target: String },

    /// 条件跳转
    ConditionalBranch {
        condition: IrValue,
        true_target: String,
        false_target: String,
    },

    /// switch 跳转
    Switch {
        value: IrValue,
        default_target: String,
        cases: Vec<(IrValue, String)>,
        ty: IrType,
    },

    /// 不可达指令
    Unreachable,
}
