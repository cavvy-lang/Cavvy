/// 字节码序列化器
/// 负责将BytecodeModule序列化为二进制格式，以及从二进制格式反序列化
use super::*;

/// 字节码文件扩展名
pub const BYTECODE_EXTENSION: &str = "caybc";

/// 字节码序列化错误
#[derive(Debug)]
pub enum SerializationError {
    IoError(std::io::Error),
    InvalidMagic,
    InvalidVersion,
    InvalidFormat(String),
}

impl std::fmt::Display for SerializationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SerializationError::IoError(e) => write!(f, "IO error: {}", e),
            SerializationError::InvalidMagic => write!(f, "Invalid bytecode magic number"),
            SerializationError::InvalidVersion => write!(f, "Invalid bytecode version"),
            SerializationError::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
        }
    }
}

impl std::error::Error for SerializationError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SerializationError::IoError(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for SerializationError {
    fn from(e: std::io::Error) -> Self {
        SerializationError::IoError(e)
    }
}

/// 序列化字节码模块到文件
pub fn serialize_to_file(module: &BytecodeModule, path: &str) -> Result<(), SerializationError> {
    let bytes = serialize(module);
    std::fs::write(path, bytes)?;
    Ok(())
}

/// 序列化字节码模块为字节数组
pub fn serialize(module: &BytecodeModule) -> Vec<u8> {
    let mut bytes = Vec::new();

    // 1. 写入魔数
    bytes.extend_from_slice(CAYBC_MAGIC);

    // 2. 写入版本号
    bytes.extend_from_slice(&CAYBC_VERSION_MAJOR.to_le_bytes());
    bytes.extend_from_slice(&CAYBC_VERSION_MINOR.to_le_bytes());

    // 3. 写入头部信息
    serialize_header(&mut bytes, &module.header);

    // 4. 写入常量池
    bytes.extend_from_slice(&module.constant_pool.serialize());

    // 5. 写入类型定义
    serialize_type_definitions(&mut bytes, &module.type_definitions);

    // 6. 写入函数定义
    serialize_function_definitions(&mut bytes, &module.functions);

    // 7. 写入全局变量
    serialize_global_variables(&mut bytes, &module.global_variables);

    // 8. 写入字符串表
    serialize_string_table(&mut bytes, &module.string_table);

    // 9. 写入元数据
    serialize_metadata(&mut bytes, &module.metadata);

    bytes
}

/// 序列化头部信息
fn serialize_header(bytes: &mut Vec<u8>, header: &ModuleHeader) {
    // 模块名称
    serialize_string(bytes, &header.name);
    // 目标平台
    serialize_string(bytes, &header.target_platform);
    // 时间戳
    bytes.extend_from_slice(&header.timestamp.to_le_bytes());
    // 混淆标志
    bytes.push(if header.obfuscated { 1 } else { 0 });
    // 运行时版本
    bytes.extend_from_slice(&header.runtime_version.0.to_le_bytes());
    bytes.extend_from_slice(&header.runtime_version.1.to_le_bytes());
    // 外部库依赖
    serialize_string_vec(bytes, &header.external_libs);
}

/// 序列化字符串
fn serialize_string(bytes: &mut Vec<u8>, s: &str) {
    let s_bytes = s.as_bytes();
    bytes.extend_from_slice(&(s_bytes.len() as u32).to_le_bytes());
    bytes.extend_from_slice(s_bytes);
}

/// 序列化字符串数组
fn serialize_string_vec(bytes: &mut Vec<u8>, vec: &[String]) {
    bytes.extend_from_slice(&(vec.len() as u32).to_le_bytes());
    for s in vec {
        serialize_string(bytes, s);
    }
}

/// 序列化类型定义列表
fn serialize_type_definitions(bytes: &mut Vec<u8>, types: &[TypeDefinition]) {
    bytes.extend_from_slice(&(types.len() as u32).to_le_bytes());
    for type_def in types {
        serialize_type_definition(bytes, type_def);
    }
}

/// 序列化单个类型定义
fn serialize_type_definition(bytes: &mut Vec<u8>, type_def: &TypeDefinition) {
    // 名称索引
    bytes.extend_from_slice(&type_def.name_index.to_le_bytes());
    // 父类索引
    match type_def.parent_index {
        Some(idx) => {
            bytes.push(1);
            bytes.extend_from_slice(&idx.to_le_bytes());
        }
        None => bytes.push(0),
    }
    // 接口索引列表
    bytes.extend_from_slice(&(type_def.interface_indices.len() as u16).to_le_bytes());
    for idx in &type_def.interface_indices {
        bytes.extend_from_slice(&idx.to_le_bytes());
    }
    // 修饰符
    serialize_type_modifiers(bytes, &type_def.modifiers);
    // 字段列表
    bytes.extend_from_slice(&(type_def.fields.len() as u16).to_le_bytes());
    for field in &type_def.fields {
        serialize_field_definition(bytes, field);
    }
    // 方法列表
    bytes.extend_from_slice(&(type_def.methods.len() as u16).to_le_bytes());
    for method in &type_def.methods {
        serialize_method_definition(bytes, method);
    }
}

/// 序列化类型修饰符
fn serialize_type_modifiers(bytes: &mut Vec<u8>, modifiers: &TypeModifiers) {
    let mut flags: u8 = 0;
    if modifiers.is_public {
        flags |= 0x01;
    }
    if modifiers.is_final {
        flags |= 0x02;
    }
    if modifiers.is_abstract {
        flags |= 0x04;
    }
    if modifiers.is_interface {
        flags |= 0x08;
    }
    bytes.push(flags);
}

/// 序列化字段定义
fn serialize_field_definition(bytes: &mut Vec<u8>, field: &FieldDefinition) {
    bytes.extend_from_slice(&field.name_index.to_le_bytes());
    bytes.extend_from_slice(&field.type_index.to_le_bytes());
    serialize_field_modifiers(bytes, &field.modifiers);
    match field.initial_value {
        Some(idx) => {
            bytes.push(1);
            bytes.extend_from_slice(&idx.to_le_bytes());
        }
        None => bytes.push(0),
    }
}

/// 序列化字段修饰符
fn serialize_field_modifiers(bytes: &mut Vec<u8>, modifiers: &FieldModifiers) {
    let mut flags: u8 = 0;
    if modifiers.is_public {
        flags |= 0x01;
    }
    if modifiers.is_private {
        flags |= 0x02;
    }
    if modifiers.is_protected {
        flags |= 0x04;
    }
    if modifiers.is_static {
        flags |= 0x08;
    }
    if modifiers.is_final {
        flags |= 0x10;
    }
    bytes.push(flags);
}

/// 序列化方法定义
fn serialize_method_definition(bytes: &mut Vec<u8>, method: &MethodDefinition) {
    bytes.extend_from_slice(&method.name_index.to_le_bytes());
    bytes.extend_from_slice(&method.return_type_index.to_le_bytes());
    // 参数类型
    bytes.extend_from_slice(&(method.param_type_indices.len() as u16).to_le_bytes());
    for idx in &method.param_type_indices {
        bytes.extend_from_slice(&idx.to_le_bytes());
    }
    // 参数名称
    bytes.extend_from_slice(&(method.param_name_indices.len() as u16).to_le_bytes());
    for idx in &method.param_name_indices {
        bytes.extend_from_slice(&idx.to_le_bytes());
    }
    // 修饰符
    serialize_method_modifiers(bytes, &method.modifiers);
    // 方法体
    match &method.body {
        Some(body) => {
            bytes.push(1);
            serialize_code_body(bytes, body);
        }
        None => bytes.push(0),
    }
    // 局部变量表大小和操作数栈深度
    bytes.extend_from_slice(&method.max_locals.to_le_bytes());
    bytes.extend_from_slice(&method.max_stack.to_le_bytes());
}

/// 序列化方法修饰符
fn serialize_method_modifiers(bytes: &mut Vec<u8>, modifiers: &MethodModifiers) {
    let mut flags: u16 = 0;
    if modifiers.is_public {
        flags |= 0x0001;
    }
    if modifiers.is_private {
        flags |= 0x0002;
    }
    if modifiers.is_protected {
        flags |= 0x0004;
    }
    if modifiers.is_static {
        flags |= 0x0008;
    }
    if modifiers.is_final {
        flags |= 0x0010;
    }
    if modifiers.is_abstract {
        flags |= 0x0020;
    }
    if modifiers.is_native {
        flags |= 0x0040;
    }
    if modifiers.is_override {
        flags |= 0x0080;
    }
    bytes.extend_from_slice(&flags.to_le_bytes());
}

/// 序列化代码体
fn serialize_code_body(bytes: &mut Vec<u8>, body: &CodeBody) {
    // 指令序列
    bytes.extend_from_slice(&(body.instructions.len() as u32).to_le_bytes());
    for instr in &body.instructions {
        bytes.push(instr.opcode.to_byte());
        bytes.extend_from_slice(&instr.operands);
    }
    // 异常处理表
    bytes.extend_from_slice(&(body.exception_table.len() as u16).to_le_bytes());
    for handler in &body.exception_table {
        bytes.extend_from_slice(&handler.start_pc.to_le_bytes());
        bytes.extend_from_slice(&handler.end_pc.to_le_bytes());
        bytes.extend_from_slice(&handler.handler_pc.to_le_bytes());
        bytes.extend_from_slice(&handler.catch_type.to_le_bytes());
    }
    // 行号表
    bytes.extend_from_slice(&(body.line_number_table.len() as u16).to_le_bytes());
    for entry in &body.line_number_table {
        bytes.extend_from_slice(&entry.pc.to_le_bytes());
        bytes.extend_from_slice(&entry.line.to_le_bytes());
    }
}

/// 序列化函数定义列表
fn serialize_function_definitions(bytes: &mut Vec<u8>, functions: &[FunctionDefinition]) {
    bytes.extend_from_slice(&(functions.len() as u32).to_le_bytes());
    for func in functions {
        serialize_function_definition(bytes, func);
    }
}

/// 序列化单个函数定义
fn serialize_function_definition(bytes: &mut Vec<u8>, func: &FunctionDefinition) {
    bytes.extend_from_slice(&func.name_index.to_le_bytes());
    bytes.extend_from_slice(&func.return_type_index.to_le_bytes());
    // 参数类型
    bytes.extend_from_slice(&(func.param_type_indices.len() as u16).to_le_bytes());
    for idx in &func.param_type_indices {
        bytes.extend_from_slice(&idx.to_le_bytes());
    }
    // 参数名称
    bytes.extend_from_slice(&(func.param_name_indices.len() as u16).to_le_bytes());
    for idx in &func.param_name_indices {
        bytes.extend_from_slice(&idx.to_le_bytes());
    }
    // 修饰符
    serialize_method_modifiers(bytes, &func.modifiers);
    // 函数体（函数必须有体）
    serialize_code_body(bytes, &func.body);
    // 局部变量表大小和操作数栈深度
    bytes.extend_from_slice(&func.max_locals.to_le_bytes());
    bytes.extend_from_slice(&func.max_stack.to_le_bytes());
}

/// 序列化全局变量列表
fn serialize_global_variables(bytes: &mut Vec<u8>, vars: &[GlobalVariable]) {
    bytes.extend_from_slice(&(vars.len() as u32).to_le_bytes());
    for var in vars {
        bytes.extend_from_slice(&var.name_index.to_le_bytes());
        bytes.extend_from_slice(&var.type_index.to_le_bytes());
        serialize_field_modifiers(bytes, &var.modifiers);
        match var.initial_value {
            Some(idx) => {
                bytes.push(1);
                bytes.extend_from_slice(&idx.to_le_bytes());
            }
            None => bytes.push(0),
        }
    }
}

/// 序列化字符串表
fn serialize_string_table(bytes: &mut Vec<u8>, table: &[String]) {
    bytes.extend_from_slice(&(table.len() as u32).to_le_bytes());
    for s in table {
        serialize_string(bytes, s);
    }
}

/// 序列化元数据
fn serialize_metadata(bytes: &mut Vec<u8>, metadata: &std::collections::HashMap<String, Vec<u8>>) {
    bytes.extend_from_slice(&(metadata.len() as u32).to_le_bytes());
    for (key, value) in metadata {
        serialize_string(bytes, key);
        bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        bytes.extend_from_slice(value);
    }
}

// ==================== 反序列化 ====================

/// 从文件反序列化字节码模块
pub fn deserialize_from_file(path: &str) -> Result<BytecodeModule, SerializationError> {
    let bytes = std::fs::read(path)?;
    deserialize(&bytes)
}

/// 从字节数组反序列化字节码模块
pub fn deserialize(bytes: &[u8]) -> Result<BytecodeModule, SerializationError> {
    let mut offset = 0;

    // 1. 检查魔数
    if bytes.len() < 4 || &bytes[0..4] != CAYBC_MAGIC {
        return Err(SerializationError::InvalidMagic);
    }
    offset += 4;

    // 2. 检查版本号
    if bytes.len() < offset + 4 {
        return Err(SerializationError::InvalidVersion);
    }
    let major = u16::from_le_bytes([bytes[offset], bytes[offset + 1]]);
    let minor = u16::from_le_bytes([bytes[offset + 2], bytes[offset + 3]]);
    offset += 4;

    if major != CAYBC_VERSION_MAJOR || minor != CAYBC_VERSION_MINOR {
        // 版本不匹配必须硬错误：不同版本的格式可能不兼容，
        // 静默继续解析会产出语义错误的模块
        return Err(SerializationError::InvalidFormat(format!(
            "字节码版本不匹配: 文件为 {}.{}, 本工具支持 {}.{}",
            major, minor, CAYBC_VERSION_MAJOR, CAYBC_VERSION_MINOR
        )));
    }

    // 3. 反序列化头部
    let header = deserialize_header(bytes, &mut offset)?;

    // 4. 反序列化常量池
    let constant_pool = ConstantPool::deserialize(bytes, &mut offset).ok_or_else(|| {
        SerializationError::InvalidFormat("Failed to deserialize constant pool".to_string())
    })?;

    // 5. 反序列化类型定义
    let type_definitions = deserialize_type_definitions(bytes, &mut offset)?;

    // 6. 反序列化函数定义
    let functions = deserialize_function_definitions(bytes, &mut offset)?;

    // 7. 反序列化全局变量
    let global_variables = deserialize_global_variables(bytes, &mut offset)?;

    // 8. 反序列化字符串表
    let string_table = deserialize_string_table(bytes, &mut offset)?;

    // 9. 反序列化元数据
    let metadata = deserialize_metadata(bytes, &mut offset)?;

    Ok(BytecodeModule {
        header,
        constant_pool,
        type_definitions,
        functions,
        global_variables,
        string_table,
        metadata,
    })
}

/// 反序列化头部信息
fn deserialize_header(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<ModuleHeader, SerializationError> {
    let name = deserialize_string(bytes, offset)?;
    let target_platform = deserialize_string(bytes, offset)?;
    let timestamp = read_u64(bytes, offset)?;
    let obfuscated = read_u8(bytes, offset)? != 0;
    let runtime_major = read_u16(bytes, offset)?;
    let runtime_minor = read_u16(bytes, offset)?;
    let external_libs = deserialize_string_vec(bytes, offset)?;

    Ok(ModuleHeader {
        name,
        target_platform,
        timestamp,
        obfuscated,
        runtime_version: (runtime_major, runtime_minor),
        external_libs,
    })
}

/// 读取u8
fn read_u8(bytes: &[u8], offset: &mut usize) -> Result<u8, SerializationError> {
    if *offset >= bytes.len() {
        return Err(SerializationError::InvalidFormat(
            "Unexpected end of data".to_string(),
        ));
    }
    let value = bytes[*offset];
    *offset += 1;
    Ok(value)
}

/// 读取u16
fn read_u16(bytes: &[u8], offset: &mut usize) -> Result<u16, SerializationError> {
    if *offset + 2 > bytes.len() {
        return Err(SerializationError::InvalidFormat(
            "Unexpected end of data".to_string(),
        ));
    }
    let value = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
    *offset += 2;
    Ok(value)
}

/// 读取u32
fn read_u32(bytes: &[u8], offset: &mut usize) -> Result<u32, SerializationError> {
    if *offset + 4 > bytes.len() {
        return Err(SerializationError::InvalidFormat(
            "Unexpected end of data".to_string(),
        ));
    }
    let value = u32::from_le_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
    ]);
    *offset += 4;
    Ok(value)
}

/// 读取u64
fn read_u64(bytes: &[u8], offset: &mut usize) -> Result<u64, SerializationError> {
    if *offset + 8 > bytes.len() {
        return Err(SerializationError::InvalidFormat(
            "Unexpected end of data".to_string(),
        ));
    }
    let value = u64::from_le_bytes([
        bytes[*offset],
        bytes[*offset + 1],
        bytes[*offset + 2],
        bytes[*offset + 3],
        bytes[*offset + 4],
        bytes[*offset + 5],
        bytes[*offset + 6],
        bytes[*offset + 7],
    ]);
    *offset += 8;
    Ok(value)
}

/// 校验从数据中读出的元素数量是否合理。
/// 长度字段来自文件（攻击者可控），直接 Vec::with_capacity(len) 可能导致
/// 巨额内存预分配。序列化后每个元素至少占 1 字节，因此 len 超过剩余输入
/// 字节数时必为非法数据，直接报错。
fn check_count(
    len: usize,
    bytes: &[u8],
    offset: usize,
    what: &str,
) -> Result<(), SerializationError> {
    let remaining = bytes.len().saturating_sub(offset);
    if len > remaining {
        return Err(SerializationError::InvalidFormat(format!(
            "{} 数量 {} 超出剩余输入数据 {} 字节",
            what, len, remaining
        )));
    }
    Ok(())
}

/// 反序列化字符串
fn deserialize_string(bytes: &[u8], offset: &mut usize) -> Result<String, SerializationError> {
    let len = read_u32(bytes, offset)? as usize;
    if *offset + len > bytes.len() {
        return Err(SerializationError::InvalidFormat(
            "String length exceeds data".to_string(),
        ));
    }
    let s = String::from_utf8(bytes[*offset..*offset + len].to_vec())
        .map_err(|_| SerializationError::InvalidFormat("Invalid UTF-8 string".to_string()))?;
    *offset += len;
    Ok(s)
}

/// 反序列化字符串数组
fn deserialize_string_vec(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Vec<String>, SerializationError> {
    let len = read_u32(bytes, offset)? as usize;
    check_count(len, bytes, *offset, "字符串数组")?;
    let mut vec = Vec::with_capacity(len);
    for _ in 0..len {
        vec.push(deserialize_string(bytes, offset)?);
    }
    Ok(vec)
}

/// 反序列化类型定义列表
fn deserialize_type_definitions(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Vec<TypeDefinition>, SerializationError> {
    let len = read_u32(bytes, offset)? as usize;
    check_count(len, bytes, *offset, "类型定义")?;
    let mut types = Vec::with_capacity(len);
    for _ in 0..len {
        types.push(deserialize_type_definition(bytes, offset)?);
    }
    Ok(types)
}

/// 反序列化单个类型定义
fn deserialize_type_definition(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<TypeDefinition, SerializationError> {
    let name_index = read_u16(bytes, offset)?;
    let parent_index = if read_u8(bytes, offset)? != 0 {
        Some(read_u16(bytes, offset)?)
    } else {
        None
    };

    let interface_count = read_u16(bytes, offset)? as usize;
    check_count(interface_count, bytes, *offset, "接口索引")?;
    let mut interface_indices = Vec::with_capacity(interface_count);
    for _ in 0..interface_count {
        interface_indices.push(read_u16(bytes, offset)?);
    }

    let modifiers = deserialize_type_modifiers(bytes, offset)?;

    let field_count = read_u16(bytes, offset)? as usize;
    check_count(field_count, bytes, *offset, "字段")?;
    let mut fields = Vec::with_capacity(field_count);
    for _ in 0..field_count {
        fields.push(deserialize_field_definition(bytes, offset)?);
    }

    let method_count = read_u16(bytes, offset)? as usize;
    check_count(method_count, bytes, *offset, "方法")?;
    let mut methods = Vec::with_capacity(method_count);
    for _ in 0..method_count {
        methods.push(deserialize_method_definition(bytes, offset)?);
    }

    Ok(TypeDefinition {
        name_index,
        parent_index,
        interface_indices,
        modifiers,
        fields,
        methods,
    })
}

/// 反序列化类型修饰符
fn deserialize_type_modifiers(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<TypeModifiers, SerializationError> {
    let flags = read_u8(bytes, offset)?;
    Ok(TypeModifiers {
        is_public: flags & 0x01 != 0,
        is_final: flags & 0x02 != 0,
        is_abstract: flags & 0x04 != 0,
        is_interface: flags & 0x08 != 0,
    })
}

/// 反序列化字段定义
fn deserialize_field_definition(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<FieldDefinition, SerializationError> {
    let name_index = read_u16(bytes, offset)?;
    let type_index = read_u16(bytes, offset)?;
    let modifiers = deserialize_field_modifiers(bytes, offset)?;
    let initial_value = if read_u8(bytes, offset)? != 0 {
        Some(read_u16(bytes, offset)?)
    } else {
        None
    };

    Ok(FieldDefinition {
        name_index,
        type_index,
        modifiers,
        initial_value,
    })
}

/// 反序列化字段修饰符
fn deserialize_field_modifiers(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<FieldModifiers, SerializationError> {
    let flags = read_u8(bytes, offset)?;
    Ok(FieldModifiers {
        is_public: flags & 0x01 != 0,
        is_private: flags & 0x02 != 0,
        is_protected: flags & 0x04 != 0,
        is_static: flags & 0x08 != 0,
        is_final: flags & 0x10 != 0,
    })
}

/// 反序列化方法定义
fn deserialize_method_definition(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<MethodDefinition, SerializationError> {
    let name_index = read_u16(bytes, offset)?;
    let return_type_index = read_u16(bytes, offset)?;

    let param_type_count = read_u16(bytes, offset)? as usize;
    check_count(param_type_count, bytes, *offset, "参数类型索引")?;
    let mut param_type_indices = Vec::with_capacity(param_type_count);
    for _ in 0..param_type_count {
        param_type_indices.push(read_u16(bytes, offset)?);
    }

    let param_name_count = read_u16(bytes, offset)? as usize;
    check_count(param_name_count, bytes, *offset, "参数名称索引")?;
    let mut param_name_indices = Vec::with_capacity(param_name_count);
    for _ in 0..param_name_count {
        param_name_indices.push(read_u16(bytes, offset)?);
    }

    let modifiers = deserialize_method_modifiers(bytes, offset)?;

    let body = if read_u8(bytes, offset)? != 0 {
        Some(deserialize_code_body(bytes, offset)?)
    } else {
        None
    };

    let max_locals = read_u16(bytes, offset)?;
    let max_stack = read_u16(bytes, offset)?;

    Ok(MethodDefinition {
        name_index,
        return_type_index,
        param_type_indices,
        param_name_indices,
        modifiers,
        body,
        max_locals,
        max_stack,
    })
}

/// 反序列化方法修饰符
fn deserialize_method_modifiers(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<MethodModifiers, SerializationError> {
    let flags = read_u16(bytes, offset)?;
    Ok(MethodModifiers {
        is_public: flags & 0x0001 != 0,
        is_private: flags & 0x0002 != 0,
        is_protected: flags & 0x0004 != 0,
        is_static: flags & 0x0008 != 0,
        is_final: flags & 0x0010 != 0,
        is_abstract: flags & 0x0020 != 0,
        is_native: flags & 0x0040 != 0,
        is_override: flags & 0x0080 != 0,
    })
}

/// 反序列化代码体
fn deserialize_code_body(bytes: &[u8], offset: &mut usize) -> Result<CodeBody, SerializationError> {
    let instr_count = read_u32(bytes, offset)? as usize;
    check_count(instr_count, bytes, *offset, "指令")?;
    let mut instructions = Vec::with_capacity(instr_count);

    for _ in 0..instr_count {
        let opcode_byte = read_u8(bytes, offset)?;
        let opcode = Opcode::from_byte(opcode_byte);

        let operand_size = opcode.operand_size().ok_or_else(|| {
            SerializationError::InvalidFormat(format!(
                "Variable-length opcode not supported: {:?}",
                opcode
            ))
        })?;

        if *offset + operand_size > bytes.len() {
            return Err(SerializationError::InvalidFormat(
                "Instruction operands exceed data".to_string(),
            ));
        }

        let operands = bytes[*offset..*offset + operand_size].to_vec();
        *offset += operand_size;

        instructions.push(Instruction { opcode, operands });
    }

    let exception_count = read_u16(bytes, offset)? as usize;
    check_count(exception_count, bytes, *offset, "异常处理表")?;
    let mut exception_table = Vec::with_capacity(exception_count);
    for _ in 0..exception_count {
        exception_table.push(ExceptionHandler {
            start_pc: read_u32(bytes, offset)?,
            end_pc: read_u32(bytes, offset)?,
            handler_pc: read_u32(bytes, offset)?,
            catch_type: read_u16(bytes, offset)?,
        });
    }

    let line_count = read_u16(bytes, offset)? as usize;
    check_count(line_count, bytes, *offset, "行号表")?;
    let mut line_number_table = Vec::with_capacity(line_count);
    for _ in 0..line_count {
        line_number_table.push(LineNumberEntry {
            pc: read_u32(bytes, offset)?,
            line: read_u32(bytes, offset)?,
        });
    }

    Ok(CodeBody {
        instructions,
        exception_table,
        line_number_table,
    })
}

/// 反序列化函数定义列表
fn deserialize_function_definitions(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Vec<FunctionDefinition>, SerializationError> {
    let len = read_u32(bytes, offset)? as usize;
    check_count(len, bytes, *offset, "函数定义")?;
    let mut functions = Vec::with_capacity(len);
    for _ in 0..len {
        functions.push(deserialize_function_definition(bytes, offset)?);
    }
    Ok(functions)
}

/// 反序列化单个函数定义
fn deserialize_function_definition(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<FunctionDefinition, SerializationError> {
    let name_index = read_u16(bytes, offset)?;
    let return_type_index = read_u16(bytes, offset)?;

    let param_type_count = read_u16(bytes, offset)? as usize;
    check_count(param_type_count, bytes, *offset, "参数类型索引")?;
    let mut param_type_indices = Vec::with_capacity(param_type_count);
    for _ in 0..param_type_count {
        param_type_indices.push(read_u16(bytes, offset)?);
    }

    let param_name_count = read_u16(bytes, offset)? as usize;
    check_count(param_name_count, bytes, *offset, "参数名称索引")?;
    let mut param_name_indices = Vec::with_capacity(param_name_count);
    for _ in 0..param_name_count {
        param_name_indices.push(read_u16(bytes, offset)?);
    }

    let modifiers = deserialize_method_modifiers(bytes, offset)?;
    let body = deserialize_code_body(bytes, offset)?;
    let max_locals = read_u16(bytes, offset)?;
    let max_stack = read_u16(bytes, offset)?;

    Ok(FunctionDefinition {
        name_index,
        return_type_index,
        param_type_indices,
        param_name_indices,
        modifiers,
        body,
        max_locals,
        max_stack,
    })
}

/// 反序列化全局变量列表
fn deserialize_global_variables(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Vec<GlobalVariable>, SerializationError> {
    let len = read_u32(bytes, offset)? as usize;
    check_count(len, bytes, *offset, "全局变量")?;
    let mut vars = Vec::with_capacity(len);
    for _ in 0..len {
        let name_index = read_u16(bytes, offset)?;
        let type_index = read_u16(bytes, offset)?;
        let modifiers = deserialize_field_modifiers(bytes, offset)?;
        let initial_value = if read_u8(bytes, offset)? != 0 {
            Some(read_u16(bytes, offset)?)
        } else {
            None
        };
        vars.push(GlobalVariable {
            name_index,
            type_index,
            modifiers,
            initial_value,
        });
    }
    Ok(vars)
}

/// 反序列化字符串表
fn deserialize_string_table(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Vec<String>, SerializationError> {
    let len = read_u32(bytes, offset)? as usize;
    check_count(len, bytes, *offset, "字符串表")?;
    let mut table = Vec::with_capacity(len);
    for _ in 0..len {
        table.push(deserialize_string(bytes, offset)?);
    }
    Ok(table)
}

/// 反序列化元数据
fn deserialize_metadata(
    bytes: &[u8],
    offset: &mut usize,
) -> Result<std::collections::HashMap<String, Vec<u8>>, SerializationError> {
    let len = read_u32(bytes, offset)? as usize;
    check_count(len, bytes, *offset, "元数据条目")?;
    let mut metadata = std::collections::HashMap::new();
    for _ in 0..len {
        let key = deserialize_string(bytes, offset)?;
        let value_len = read_u32(bytes, offset)? as usize;
        if *offset + value_len > bytes.len() {
            return Err(SerializationError::InvalidFormat(
                "Metadata value exceeds data".to_string(),
            ));
        }
        let value = bytes[*offset..*offset + value_len].to_vec();
        *offset += value_len;
        metadata.insert(key, value);
    }
    Ok(metadata)
}
