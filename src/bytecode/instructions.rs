/// Cavvy字节码指令集
/// 基于栈的虚拟机指令集，类似于JVM但针对Cavvy语言特性优化

/// 指令类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    // ==================== 常量加载指令 ====================
    /// 加载常量池中的常量到操作数栈
    /// 操作数: u16 常量池索引
    Ldc = 0x01,
    /// 加载int常量（小整数，-128~127）
    /// 操作数: i8 立即数
    Iconst = 0x02,
    /// 加载long常量
    /// 操作数: i64
    Lconst = 0x03,
    /// 加载float常量
    /// 操作数: f32
    Fconst = 0x04,
    /// 加载double常量
    /// 操作数: f64
    Dconst = 0x05,
    /// 加载null
    AconstNull = 0x06,
    /// 加载int常量0
    Iconst0 = 0x07,
    /// 加载int常量1
    Iconst1 = 0x08,
    /// 加载int常量-1
    IconstM1 = 0x09,

    // ==================== 局部变量加载/存储指令 ====================
    /// 从局部变量表加载int
    /// 操作数: u16 局部变量索引
    Iload = 0x10,
    /// 从局部变量表加载long
    Lload = 0x11,
    /// 从局部变量表加载float
    Fload = 0x12,
    /// 从局部变量表加载double
    Dload = 0x13,
    /// 从局部变量表加载引用
    Aload = 0x14,
    /// 从局部变量表加载int（0-3索引优化）
    Iload0 = 0x15,
    Iload1 = 0x16,
    Iload2 = 0x17,
    Iload3 = 0x18,
    /// 从局部变量表加载引用（0-3索引优化）
    Aload0 = 0x19,
    Aload1 = 0x1A,
    Aload2 = 0x1B,
    Aload3 = 0x1C,

    /// 存储int到局部变量表
    /// 操作数: u16 局部变量索引
    Istore = 0x20,
    /// 存储long到局部变量表
    Lstore = 0x21,
    /// 存储float到局部变量表
    Fstore = 0x22,
    /// 存储double到局部变量表
    Dstore = 0x23,
    /// 存储引用到局部变量表
    Astore = 0x24,
    /// 存储int到局部变量表（0-3索引优化）
    Istore0 = 0x25,
    Istore1 = 0x26,
    Istore2 = 0x27,
    Istore3 = 0x28,
    /// 存储引用到局部变量表（0-3索引优化）
    Astore0 = 0x29,
    Astore1 = 0x2A,
    Astore2 = 0x2B,
    Astore3 = 0x2C,

    // ==================== 数组操作指令 ====================
    /// 创建一维数组
    /// 操作数: u16 元素类型索引
    Newarray = 0x30,
    /// 创建多维数组
    /// 操作数: u16 元素类型索引, u8 维度数
    Multianewarray = 0x31,
    /// 加载数组长度
    Arraylength = 0x32,
    /// 加载int数组元素
    Iaload = 0x33,
    /// 加载long数组元素
    Laload = 0x34,
    /// 加载float数组元素
    Faload = 0x35,
    /// 加载double数组元素
    Daload = 0x36,
    /// 加载引用数组元素
    Aaload = 0x37,
    /// 存储int到数组
    Iastore = 0x38,
    /// 存储long到数组
    Lastore = 0x39,
    /// 存储float到数组
    Fastore = 0x3A,
    /// 存储double到数组
    Dastore = 0x3B,
    /// 存储引用到数组
    Aastore = 0x3C,

    // ==================== 栈操作指令 ====================
    /// 弹出栈顶值
    Pop = 0x40,
    /// 弹出栈顶2个值（long/double）
    Pop2 = 0x41,
    /// 复制栈顶值
    Dup = 0x42,
    /// 复制栈顶值并插入到栈顶下方
    DupX1 = 0x43,
    /// 复制栈顶值并插入到栈顶下方2个位置
    DupX2 = 0x44,
    /// 交换栈顶两个值
    Swap = 0x45,

    // ==================== 算术运算指令 ====================
    /// int加法
    Iadd = 0x50,
    /// long加法
    Ladd = 0x51,
    /// float加法
    Fadd = 0x52,
    /// double加法
    Dadd = 0x53,
    /// int减法
    Isub = 0x54,
    /// long减法
    Lsub = 0x55,
    /// float减法
    Fsub = 0x56,
    /// double减法
    Dsub = 0x57,
    /// int乘法
    Imul = 0x58,
    /// long乘法
    Lmul = 0x59,
    /// float乘法
    Fmul = 0x5A,
    /// double乘法
    Dmul = 0x5B,
    /// int除法
    Idiv = 0x5C,
    /// long除法
    Ldiv = 0x5D,
    /// float除法
    Fdiv = 0x5E,
    /// double除法
    Ddiv = 0x5F,
    /// int取模
    Irem = 0x60,
    /// long取模
    Lrem = 0x61,
    /// float取模
    Frem = 0x62,
    /// double取模
    Drem = 0x63,
    /// int取负
    Ineg = 0x64,
    /// long取负
    Lneg = 0x65,
    /// float取负
    Fneg = 0x66,
    /// double取负
    Dneg = 0x67,

    // ==================== 位运算指令 ====================
    /// int左移
    Ishl = 0x70,
    /// long左移
    Lshl = 0x71,
    /// int右移（算术）
    Ishr = 0x72,
    /// long右移（算术）
    Lshr = 0x73,
    /// int无符号右移
    Iushr = 0x74,
    /// long无符号右移
    Lushr = 0x75,
    /// int按位与
    Iand = 0x76,
    /// long按位与
    Land = 0x77,
    /// int按位或
    Ior = 0x78,
    /// long按位或
    Lor = 0x79,
    /// int按位异或
    Ixor = 0x7A,
    /// long按位异或
    Lxor = 0x7B,

    // ==================== 类型转换指令 ====================
    /// int转long
    I2l = 0x80,
    /// int转float
    I2f = 0x81,
    /// int转double
    I2d = 0x82,
    /// long转int
    L2i = 0x83,
    /// long转float
    L2f = 0x84,
    /// long转double
    L2d = 0x85,
    /// float转int
    F2i = 0x86,
    /// float转long
    F2l = 0x87,
    /// float转double
    F2d = 0x88,
    /// double转int
    D2i = 0x89,
    /// double转long
    D2l = 0x8A,
    /// double转float
    D2f = 0x8B,
    /// int转byte
    I2b = 0x8C,
    /// int转char
    I2c = 0x8D,
    /// int转short
    I2s = 0x8E,

    // ==================== 比较指令 ====================
    /// long比较
    Lcmp = 0x90,
    /// float比较（处理NaN）
    Fcmpl = 0x91,
    Fcmpg = 0x92,
    /// double比较（处理NaN）
    Dcmpl = 0x93,
    Dcmpg = 0x94,

    // ==================== 条件跳转指令 ====================
    /// 等于0跳转
    /// 操作数: i16 偏移量
    Ifeq = 0xA0,
    /// 不等于0跳转
    Ifne = 0xA1,
    /// 小于0跳转
    Iflt = 0xA2,
    /// 大于等于0跳转
    Ifge = 0xA3,
    /// 大于0跳转
    Ifgt = 0xA4,
    /// 小于等于0跳转
    Ifle = 0xA5,
    /// int相等跳转
    IfIcmpeq = 0xA6,
    /// int不相等跳转
    IfIcmpne = 0xA7,
    /// int小于跳转
    IfIcmplt = 0xA8,
    /// int大于等于跳转
    IfIcmpge = 0xA9,
    /// int大于跳转
    IfIcmpgt = 0xAA,
    /// int小于等于跳转
    IfIcmple = 0xAB,
    /// 引用相等跳转
    IfAcmpeq = 0xAC,
    /// 引用不相等跳转
    IfAcmpne = 0xAD,
    /// 引用为null跳转
    Ifnull = 0xAE,
    /// 引用不为null跳转
    Ifnonnull = 0xAF,

    // ==================== 无条件跳转指令 ====================
    /// goto跳转
    /// 操作数: i16 偏移量
    Goto = 0xB0,
    /// 宽goto跳转（32位偏移）
    /// 操作数: i32 偏移量
    GotoW = 0xB1,
    /// 跳转到子程序
    Jsr = 0xB2,
    /// 从子程序返回
    Ret = 0xB3,
    /// 查表跳转（switch）
    Tableswitch = 0xB4,
    /// 查找跳转（switch，稀疏）
    Lookupswitch = 0xB5,

    // ==================== 方法调用指令 ====================
    /// 调用虚方法（动态分派）
    /// 操作数: u16 方法引用索引
    Invokevirtual = 0xC0,
    /// 调用静态方法
    /// 操作数: u16 方法引用索引
    Invokestatic = 0xC1,
    /// 调用构造函数或私有方法
    /// 操作数: u16 方法引用索引
    Invokespecial = 0xC2,
    /// 调用接口方法
    /// 操作数: u16 接口方法引用索引, u8 参数计数
    Invokeinterface = 0xC3,
    /// 调用动态方法（用于lambda等）
    /// 操作数: u16 调用点索引
    Invokedynamic = 0xC4,
    /// 调用顶层函数
    /// 操作数: u16 函数引用索引
    Invokefunction = 0xC5,

    // ==================== 对象操作指令 ====================
    /// 创建新对象
    /// 操作数: u16 类引用索引
    New = 0xD0,
    /// 创建新数组（对象类型）
    /// 操作数: u16 类引用索引
    Anewarray = 0xD1,
    /// 获取对象字段
    /// 操作数: u16 字段引用索引
    Getfield = 0xD2,
    /// 设置对象字段
    /// 操作数: u16 字段引用索引
    Putfield = 0xD3,
    /// 获取静态字段
    /// 操作数: u16 字段引用索引
    Getstatic = 0xD4,
    /// 设置静态字段
    /// 操作数: u16 字段引用索引
    Putstatic = 0xD5,
    /// 检查对象类型
    /// 操作数: u16 类引用索引
    Instanceof = 0xD6,
    /// 类型转换检查
    /// 操作数: u16 类引用索引
    Checkcast = 0xD7,

    // ==================== 返回指令 ====================
    /// 从void方法返回
    Return = 0xE0,
    /// 从int方法返回
    Ireturn = 0xE1,
    /// 从long方法返回
    Lreturn = 0xE2,
    /// 从float方法返回
    Freturn = 0xE3,
    /// 从double方法返回
    Dreturn = 0xE4,
    /// 从引用方法返回
    Areturn = 0xE5,

    // ==================== 同步指令 ====================
    /// 获取监视器锁
    Monitorenter = 0xF0,
    /// 释放监视器锁
    Monitorexit = 0xF1,

    // ==================== 扩展指令 ====================
    /// 宽索引前缀（用于16位以上的索引）
    Wide = 0xF8,
    /// 断点（调试）
    Breakpoint = 0xF9,
    /// 保留指令
    Impdep1 = 0xFA,
    Impdep2 = 0xFB,
    /// 无效指令
    Invalid = 0xFF,
}

impl Opcode {
    /// 从字节解析操作码
    pub fn from_byte(byte: u8) -> Self {
        match byte {
            0x01 => Opcode::Ldc,
            0x02 => Opcode::Iconst,
            0x03 => Opcode::Lconst,
            0x04 => Opcode::Fconst,
            0x05 => Opcode::Dconst,
            0x06 => Opcode::AconstNull,
            0x07 => Opcode::Iconst0,
            0x08 => Opcode::Iconst1,
            0x09 => Opcode::IconstM1,
            0x10 => Opcode::Iload,
            0x11 => Opcode::Lload,
            0x12 => Opcode::Fload,
            0x13 => Opcode::Dload,
            0x14 => Opcode::Aload,
            0x15 => Opcode::Iload0,
            0x16 => Opcode::Iload1,
            0x17 => Opcode::Iload2,
            0x18 => Opcode::Iload3,
            0x19 => Opcode::Aload0,
            0x1A => Opcode::Aload1,
            0x1B => Opcode::Aload2,
            0x1C => Opcode::Aload3,
            0x20 => Opcode::Istore,
            0x21 => Opcode::Lstore,
            0x22 => Opcode::Fstore,
            0x23 => Opcode::Dstore,
            0x24 => Opcode::Astore,
            0x25 => Opcode::Istore0,
            0x26 => Opcode::Istore1,
            0x27 => Opcode::Istore2,
            0x28 => Opcode::Istore3,
            0x29 => Opcode::Astore0,
            0x2A => Opcode::Astore1,
            0x2B => Opcode::Astore2,
            0x2C => Opcode::Astore3,
            0x30 => Opcode::Newarray,
            0x31 => Opcode::Multianewarray,
            0x32 => Opcode::Arraylength,
            0x33 => Opcode::Iaload,
            0x34 => Opcode::Laload,
            0x35 => Opcode::Faload,
            0x36 => Opcode::Daload,
            0x37 => Opcode::Aaload,
            0x38 => Opcode::Iastore,
            0x39 => Opcode::Lastore,
            0x3A => Opcode::Fastore,
            0x3B => Opcode::Dastore,
            0x3C => Opcode::Aastore,
            0x40 => Opcode::Pop,
            0x41 => Opcode::Pop2,
            0x42 => Opcode::Dup,
            0x43 => Opcode::DupX1,
            0x44 => Opcode::DupX2,
            0x45 => Opcode::Swap,
            0x50 => Opcode::Iadd,
            0x51 => Opcode::Ladd,
            0x52 => Opcode::Fadd,
            0x53 => Opcode::Dadd,
            0x54 => Opcode::Isub,
            0x55 => Opcode::Lsub,
            0x56 => Opcode::Fsub,
            0x57 => Opcode::Dsub,
            0x58 => Opcode::Imul,
            0x59 => Opcode::Lmul,
            0x5A => Opcode::Fmul,
            0x5B => Opcode::Dmul,
            0x5C => Opcode::Idiv,
            0x5D => Opcode::Ldiv,
            0x5E => Opcode::Fdiv,
            0x5F => Opcode::Ddiv,
            0x60 => Opcode::Irem,
            0x61 => Opcode::Lrem,
            0x62 => Opcode::Frem,
            0x63 => Opcode::Drem,
            0x64 => Opcode::Ineg,
            0x65 => Opcode::Lneg,
            0x66 => Opcode::Fneg,
            0x67 => Opcode::Dneg,
            0x70 => Opcode::Ishl,
            0x71 => Opcode::Lshl,
            0x72 => Opcode::Ishr,
            0x73 => Opcode::Lshr,
            0x74 => Opcode::Iushr,
            0x75 => Opcode::Lushr,
            0x76 => Opcode::Iand,
            0x77 => Opcode::Land,
            0x78 => Opcode::Ior,
            0x79 => Opcode::Lor,
            0x7A => Opcode::Ixor,
            0x7B => Opcode::Lxor,
            0x80 => Opcode::I2l,
            0x81 => Opcode::I2f,
            0x82 => Opcode::I2d,
            0x83 => Opcode::L2i,
            0x84 => Opcode::L2f,
            0x85 => Opcode::L2d,
            0x86 => Opcode::F2i,
            0x87 => Opcode::F2l,
            0x88 => Opcode::F2d,
            0x89 => Opcode::D2i,
            0x8A => Opcode::D2l,
            0x8B => Opcode::D2f,
            0x8C => Opcode::I2b,
            0x8D => Opcode::I2c,
            0x8E => Opcode::I2s,
            0x90 => Opcode::Lcmp,
            0x91 => Opcode::Fcmpl,
            0x92 => Opcode::Fcmpg,
            0x93 => Opcode::Dcmpl,
            0x94 => Opcode::Dcmpg,
            0xA0 => Opcode::Ifeq,
            0xA1 => Opcode::Ifne,
            0xA2 => Opcode::Iflt,
            0xA3 => Opcode::Ifge,
            0xA4 => Opcode::Ifgt,
            0xA5 => Opcode::Ifle,
            0xA6 => Opcode::IfIcmpeq,
            0xA7 => Opcode::IfIcmpne,
            0xA8 => Opcode::IfIcmplt,
            0xA9 => Opcode::IfIcmpge,
            0xAA => Opcode::IfIcmpgt,
            0xAB => Opcode::IfIcmple,
            0xAC => Opcode::IfAcmpeq,
            0xAD => Opcode::IfAcmpne,
            0xAE => Opcode::Ifnull,
            0xAF => Opcode::Ifnonnull,
            0xB0 => Opcode::Goto,
            0xB1 => Opcode::GotoW,
            0xB2 => Opcode::Jsr,
            0xB3 => Opcode::Ret,
            0xB4 => Opcode::Tableswitch,
            0xB5 => Opcode::Lookupswitch,
            0xC0 => Opcode::Invokevirtual,
            0xC1 => Opcode::Invokestatic,
            0xC2 => Opcode::Invokespecial,
            0xC3 => Opcode::Invokeinterface,
            0xC4 => Opcode::Invokedynamic,
            0xC5 => Opcode::Invokefunction,
            0xD0 => Opcode::New,
            0xD1 => Opcode::Anewarray,
            0xD2 => Opcode::Getfield,
            0xD3 => Opcode::Putfield,
            0xD4 => Opcode::Getstatic,
            0xD5 => Opcode::Putstatic,
            0xD6 => Opcode::Instanceof,
            0xD7 => Opcode::Checkcast,
            0xE0 => Opcode::Return,
            0xE1 => Opcode::Ireturn,
            0xE2 => Opcode::Lreturn,
            0xE3 => Opcode::Freturn,
            0xE4 => Opcode::Dreturn,
            0xE5 => Opcode::Areturn,
            0xF0 => Opcode::Monitorenter,
            0xF1 => Opcode::Monitorexit,
            0xF8 => Opcode::Wide,
            0xF9 => Opcode::Breakpoint,
            0xFA => Opcode::Impdep1,
            0xFB => Opcode::Impdep2,
            _ => Opcode::Invalid,
        }
    }

    /// 获取操作码的字节值
    pub fn to_byte(self) -> u8 {
        self as u8
    }

    /// 获取指令的操作数大小（字节）
    /// 返回None表示变长指令
    pub fn operand_size(self) -> Option<usize> {
        match self {
            // 无操作数指令
            Opcode::AconstNull
            | Opcode::Iconst0
            | Opcode::Iconst1
            | Opcode::IconstM1
            | Opcode::Iload0
            | Opcode::Iload1
            | Opcode::Iload2
            | Opcode::Iload3
            | Opcode::Aload0
            | Opcode::Aload1
            | Opcode::Aload2
            | Opcode::Aload3
            | Opcode::Istore0
            | Opcode::Istore1
            | Opcode::Istore2
            | Opcode::Istore3
            | Opcode::Astore0
            | Opcode::Astore1
            | Opcode::Astore2
            | Opcode::Astore3
            | Opcode::Arraylength
            | Opcode::Iaload
            | Opcode::Laload
            | Opcode::Faload
            | Opcode::Daload
            | Opcode::Aaload
            | Opcode::Iastore
            | Opcode::Lastore
            | Opcode::Fastore
            | Opcode::Dastore
            | Opcode::Aastore
            | Opcode::Pop
            | Opcode::Pop2
            | Opcode::Dup
            | Opcode::DupX1
            | Opcode::DupX2
            | Opcode::Swap
            | Opcode::Iadd
            | Opcode::Ladd
            | Opcode::Fadd
            | Opcode::Dadd
            | Opcode::Isub
            | Opcode::Lsub
            | Opcode::Fsub
            | Opcode::Dsub
            | Opcode::Imul
            | Opcode::Lmul
            | Opcode::Fmul
            | Opcode::Dmul
            | Opcode::Idiv
            | Opcode::Ldiv
            | Opcode::Fdiv
            | Opcode::Ddiv
            | Opcode::Irem
            | Opcode::Lrem
            | Opcode::Frem
            | Opcode::Drem
            | Opcode::Ineg
            | Opcode::Lneg
            | Opcode::Fneg
            | Opcode::Dneg
            | Opcode::Ishl
            | Opcode::Lshl
            | Opcode::Ishr
            | Opcode::Lshr
            | Opcode::Iushr
            | Opcode::Lushr
            | Opcode::Iand
            | Opcode::Land
            | Opcode::Ior
            | Opcode::Lor
            | Opcode::Ixor
            | Opcode::Lxor
            | Opcode::I2l
            | Opcode::I2f
            | Opcode::I2d
            | Opcode::L2i
            | Opcode::L2f
            | Opcode::L2d
            | Opcode::F2i
            | Opcode::F2l
            | Opcode::F2d
            | Opcode::D2i
            | Opcode::D2l
            | Opcode::D2f
            | Opcode::I2b
            | Opcode::I2c
            | Opcode::I2s
            | Opcode::Lcmp
            | Opcode::Fcmpl
            | Opcode::Fcmpg
            | Opcode::Dcmpl
            | Opcode::Dcmpg
            | Opcode::Return
            | Opcode::Ireturn
            | Opcode::Lreturn
            | Opcode::Freturn
            | Opcode::Dreturn
            | Opcode::Areturn
            | Opcode::Monitorenter
            | Opcode::Monitorexit
            | Opcode::Breakpoint
            | Opcode::Impdep1
            | Opcode::Impdep2
            | Opcode::Invalid => Some(0),

            // 1字节操作数
            Opcode::Iconst => Some(1),

            // 2字节操作数
            Opcode::Ldc
            | Opcode::Iload
            | Opcode::Lload
            | Opcode::Fload
            | Opcode::Dload
            | Opcode::Aload
            | Opcode::Istore
            | Opcode::Lstore
            | Opcode::Fstore
            | Opcode::Dstore
            | Opcode::Astore
            | Opcode::Newarray
            | Opcode::Anewarray
            | Opcode::Ifeq
            | Opcode::Ifne
            | Opcode::Iflt
            | Opcode::Ifge
            | Opcode::Ifgt
            | Opcode::Ifle
            | Opcode::IfIcmpeq
            | Opcode::IfIcmpne
            | Opcode::IfIcmplt
            | Opcode::IfIcmpge
            | Opcode::IfIcmpgt
            | Opcode::IfIcmple
            | Opcode::IfAcmpeq
            | Opcode::IfAcmpne
            | Opcode::Ifnull
            | Opcode::Ifnonnull
            | Opcode::Goto
            | Opcode::Jsr
            | Opcode::Invokevirtual
            | Opcode::Invokestatic
            | Opcode::Invokespecial
            | Opcode::Invokefunction
            | Opcode::New
            | Opcode::Getfield
            | Opcode::Putfield
            | Opcode::Getstatic
            | Opcode::Putstatic
            | Opcode::Instanceof
            | Opcode::Checkcast => Some(2),

            // 4字节操作数
            Opcode::Lconst | Opcode::Fconst | Opcode::GotoW => Some(4),

            // 8字节操作数
            Opcode::Dconst => Some(8),

            // 变长指令
            Opcode::Multianewarray => Some(3), // 2字节类型索引 + 1字节维度
            Opcode::Invokeinterface => Some(4), // 2字节索引 + 1字节计数 + 1字节填充
            Opcode::Invokedynamic => Some(4),  // 2字节索引 + 2字节填充
            Opcode::Tableswitch | Opcode::Lookupswitch | Opcode::Wide | Opcode::Ret => None,
        }
    }
}

/// 指令结构
#[derive(Debug, Clone)]
pub struct Instruction {
    /// 操作码
    pub opcode: Opcode,
    /// 操作数（变长）
    pub operands: Vec<u8>,
}

impl Instruction {
    /// 创建无操作数指令
    pub fn new(opcode: Opcode) -> Self {
        Self {
            opcode,
            operands: Vec::new(),
        }
    }

    /// 创建带操作数的指令
    pub fn with_operands(opcode: Opcode, operands: Vec<u8>) -> Self {
        Self { opcode, operands }
    }

    /// 创建加载常量指令
    pub fn ldc(index: u16) -> Self {
        Self::with_operands(Opcode::Ldc, index.to_le_bytes().to_vec())
    }

    /// 创建加载int常量指令
    pub fn iconst(value: i8) -> Self {
        Self::with_operands(Opcode::Iconst, vec![value as u8])
    }

    /// 创建加载局部变量指令
    pub fn iload(index: u16) -> Self {
        Self::with_operands(Opcode::Iload, index.to_le_bytes().to_vec())
    }

    /// 创建存储局部变量指令
    pub fn istore(index: u16) -> Self {
        Self::with_operands(Opcode::Istore, index.to_le_bytes().to_vec())
    }

    /// 创建条件跳转指令
    pub fn ifeq(offset: i16) -> Self {
        Self::with_operands(Opcode::Ifeq, offset.to_le_bytes().to_vec())
    }

    /// 创建goto指令
    pub fn goto(offset: i16) -> Self {
        Self::with_operands(Opcode::Goto, offset.to_le_bytes().to_vec())
    }

    /// 创建方法调用指令
    pub fn invokestatic(index: u16) -> Self {
        Self::with_operands(Opcode::Invokestatic, index.to_le_bytes().to_vec())
    }

    /// 获取指令的总大小（操作码 + 操作数）
    pub fn size(&self) -> usize {
        1 + self.operands.len()
    }

    /// 将指令编码为字节
    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = vec![self.opcode.to_byte()];
        bytes.extend_from_slice(&self.operands);
        bytes
    }

    /// 从字节流解码指令
    pub fn decode(bytes: &[u8], offset: usize) -> Option<(Self, usize)> {
        if offset >= bytes.len() {
            return None;
        }

        let opcode = Opcode::from_byte(bytes[offset]);
        let operand_size = opcode.operand_size()?;

        if offset + 1 + operand_size > bytes.len() {
            return None;
        }

        let operands = if operand_size > 0 {
            bytes[offset + 1..offset + 1 + operand_size].to_vec()
        } else {
            Vec::new()
        };

        Some((Self { opcode, operands }, 1 + operand_size))
    }
}
