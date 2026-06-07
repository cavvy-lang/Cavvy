/// 常量池实现
/// 存储字节码中使用的常量，支持高效的序列化和反序列化

/// 常量池索引类型
pub type ConstantIndex = u16;

/// 常量池条目类型
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    /// UTF-8字符串（用于内部表示）
    Utf8(String),
    /// 整数常量（32位）
    Integer(i32),
    /// 长整数常量（64位）
    Long(i64),
    /// 浮点数常量（32位）
    Float(f32),
    /// 双精度浮点数常量（64位）
    Double(f64),
    /// 字符串常量（引用UTF8条目）
    String(ConstantIndex),
    /// 类引用
    Class { name_index: ConstantIndex },
    /// 字段引用
    FieldRef {
        class_index: ConstantIndex,
        name_and_type_index: ConstantIndex,
    },
    /// 方法引用
    MethodRef {
        class_index: ConstantIndex,
        name_and_type_index: ConstantIndex,
    },
    /// 接口方法引用
    InterfaceMethodRef {
        class_index: ConstantIndex,
        name_and_type_index: ConstantIndex,
    },
    /// 名称和类型描述符
    NameAndType {
        name_index: ConstantIndex,
        descriptor_index: ConstantIndex,
    },
    /// 方法句柄
    MethodHandle {
        reference_kind: u8,
        reference_index: ConstantIndex,
    },
    /// 方法类型
    MethodType { descriptor_index: ConstantIndex },
    /// 动态调用点
    InvokeDynamic {
        bootstrap_method_attr_index: ConstantIndex,
        name_and_type_index: ConstantIndex,
    },
    /// 模块名称
    Module { name_index: ConstantIndex },
    /// 包名称
    Package { name_index: ConstantIndex },
}

/// 常量标签（用于序列化）
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstantTag {
    Utf8 = 1,
    Integer = 3,
    Float = 4,
    Long = 5,
    Double = 6,
    Class = 7,
    String = 8,
    FieldRef = 9,
    MethodRef = 10,
    InterfaceMethodRef = 11,
    NameAndType = 12,
    MethodHandle = 15,
    MethodType = 16,
    Dynamic = 17,
    InvokeDynamic = 18,
    Module = 19,
    Package = 20,
}

impl ConstantTag {
    pub fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(ConstantTag::Utf8),
            3 => Some(ConstantTag::Integer),
            4 => Some(ConstantTag::Float),
            5 => Some(ConstantTag::Long),
            6 => Some(ConstantTag::Double),
            7 => Some(ConstantTag::Class),
            8 => Some(ConstantTag::String),
            9 => Some(ConstantTag::FieldRef),
            10 => Some(ConstantTag::MethodRef),
            11 => Some(ConstantTag::InterfaceMethodRef),
            12 => Some(ConstantTag::NameAndType),
            15 => Some(ConstantTag::MethodHandle),
            16 => Some(ConstantTag::MethodType),
            17 => Some(ConstantTag::Dynamic),
            18 => Some(ConstantTag::InvokeDynamic),
            19 => Some(ConstantTag::Module),
            20 => Some(ConstantTag::Package),
            _ => None,
        }
    }

    pub fn to_byte(self) -> u8 {
        self as u8
    }
}

/// 常量池
#[derive(Debug, Clone)]
pub struct ConstantPool {
    /// 常量条目列表（索引从1开始，0保留）
    entries: Vec<Constant>,
    /// 字符串到索引的缓存映射
    string_cache: std::collections::HashMap<String, ConstantIndex>,
}

impl ConstantPool {
    /// 创建新的常量池
    pub fn new() -> Self {
        Self {
            entries: vec![Constant::Utf8(String::new())], // 索引0保留
            string_cache: std::collections::HashMap::new(),
        }
    }

    /// 获取常量池大小
    pub fn size(&self) -> usize {
        self.entries.len()
    }

    /// 添加常量并返回索引
    pub fn add(&mut self, constant: Constant) -> ConstantIndex {
        let index = self.entries.len() as ConstantIndex;
        self.entries.push(constant);
        index
    }

    /// 添加UTF-8字符串
    pub fn add_utf8(&mut self, s: &str) -> ConstantIndex {
        // 检查缓存
        if let Some(&index) = self.string_cache.get(s) {
            return index;
        }

        let index = self.add(Constant::Utf8(s.to_string()));
        self.string_cache.insert(s.to_string(), index);
        index
    }

    /// 添加整数常量
    pub fn add_integer(&mut self, value: i32) -> ConstantIndex {
        self.add(Constant::Integer(value))
    }

    /// 添加长整数常量
    pub fn add_long(&mut self, value: i64) -> ConstantIndex {
        self.add(Constant::Long(value))
    }

    /// 添加浮点数常量
    pub fn add_float(&mut self, value: f32) -> ConstantIndex {
        self.add(Constant::Float(value))
    }

    /// 添加双精度浮点数常量
    pub fn add_double(&mut self, value: f64) -> ConstantIndex {
        self.add(Constant::Double(value))
    }

    /// 添加字符串常量
    pub fn add_string(&mut self, s: &str) -> ConstantIndex {
        let utf8_index = self.add_utf8(s);
        self.add(Constant::String(utf8_index))
    }

    /// 添加类引用
    pub fn add_class(&mut self, name: &str) -> ConstantIndex {
        let name_index = self.add_utf8(name);
        self.add(Constant::Class { name_index })
    }

    /// 添加字段引用
    pub fn add_field_ref(
        &mut self,
        class_name: &str,
        field_name: &str,
        descriptor: &str,
    ) -> ConstantIndex {
        let class_index = self.add_class(class_name);
        let name_and_type_index = self.add_name_and_type(field_name, descriptor);
        self.add(Constant::FieldRef {
            class_index,
            name_and_type_index,
        })
    }

    /// 添加方法引用
    pub fn add_method_ref(
        &mut self,
        class_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> ConstantIndex {
        let class_index = self.add_class(class_name);
        let name_and_type_index = self.add_name_and_type(method_name, descriptor);
        self.add(Constant::MethodRef {
            class_index,
            name_and_type_index,
        })
    }

    /// 添加接口方法引用
    pub fn add_interface_method_ref(
        &mut self,
        interface_name: &str,
        method_name: &str,
        descriptor: &str,
    ) -> ConstantIndex {
        let class_index = self.add_class(interface_name);
        let name_and_type_index = self.add_name_and_type(method_name, descriptor);
        self.add(Constant::InterfaceMethodRef {
            class_index,
            name_and_type_index,
        })
    }

    /// 添加名称和类型描述符
    pub fn add_name_and_type(&mut self, name: &str, descriptor: &str) -> ConstantIndex {
        let name_index = self.add_utf8(name);
        let descriptor_index = self.add_utf8(descriptor);
        self.add(Constant::NameAndType {
            name_index,
            descriptor_index,
        })
    }

    /// 获取常量
    pub fn get(&self, index: ConstantIndex) -> Option<&Constant> {
        self.entries.get(index as usize)
    }

    /// 获取UTF-8字符串
    pub fn get_utf8(&self, index: ConstantIndex) -> Option<&str> {
        match self.get(index) {
            Some(Constant::Utf8(s)) => Some(s),
            _ => None,
        }
    }

    /// 获取字符串（解析String常量的UTF8内容）
    pub fn get_string(&self, index: ConstantIndex) -> Option<String> {
        match self.get(index) {
            Some(Constant::String(utf8_index)) => self.get_utf8(*utf8_index).map(|s| s.to_string()),
            Some(Constant::Utf8(s)) => Some(s.clone()),
            _ => None,
        }
    }

    /// 获取整数
    pub fn get_integer(&self, index: ConstantIndex) -> Option<i32> {
        match self.get(index) {
            Some(Constant::Integer(v)) => Some(*v),
            _ => None,
        }
    }

    /// 获取长整数
    pub fn get_long(&self, index: ConstantIndex) -> Option<i64> {
        match self.get(index) {
            Some(Constant::Long(v)) => Some(*v),
            _ => None,
        }
    }

    /// 获取浮点数
    pub fn get_float(&self, index: ConstantIndex) -> Option<f32> {
        match self.get(index) {
            Some(Constant::Float(v)) => Some(*v),
            _ => None,
        }
    }

    /// 获取双精度浮点数
    pub fn get_double(&self, index: ConstantIndex) -> Option<f64> {
        match self.get(index) {
            Some(Constant::Double(v)) => Some(*v),
            _ => None,
        }
    }

    /// 获取类名称
    pub fn get_class_name(&self, index: ConstantIndex) -> Option<String> {
        match self.get(index) {
            Some(Constant::Class { name_index }) => {
                self.get_utf8(*name_index).map(|s| s.to_string())
            }
            _ => None,
        }
    }

    /// 序列化常量池为字节
    pub fn serialize(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        // 写入常量池大小（不包括保留的0索引）
        let count = (self.entries.len() as u16).to_le_bytes();
        bytes.extend_from_slice(&count);

        // 跳过索引0（保留）
        for entry in &self.entries[1..] {
            self.serialize_entry(&mut bytes, entry);
        }

        bytes
    }

    /// 序列化单个常量条目
    fn serialize_entry(&self, bytes: &mut Vec<u8>, constant: &Constant) {
        match constant {
            Constant::Utf8(s) => {
                bytes.push(ConstantTag::Utf8.to_byte());
                let s_bytes = s.as_bytes();
                bytes.extend_from_slice(&(s_bytes.len() as u16).to_le_bytes());
                bytes.extend_from_slice(s_bytes);
            }
            Constant::Integer(v) => {
                bytes.push(ConstantTag::Integer.to_byte());
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            Constant::Long(v) => {
                bytes.push(ConstantTag::Long.to_byte());
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            Constant::Float(v) => {
                bytes.push(ConstantTag::Float.to_byte());
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            Constant::Double(v) => {
                bytes.push(ConstantTag::Double.to_byte());
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            Constant::String(utf8_index) => {
                bytes.push(ConstantTag::String.to_byte());
                bytes.extend_from_slice(&utf8_index.to_le_bytes());
            }
            Constant::Class { name_index } => {
                bytes.push(ConstantTag::Class.to_byte());
                bytes.extend_from_slice(&name_index.to_le_bytes());
            }
            Constant::FieldRef {
                class_index,
                name_and_type_index,
            } => {
                bytes.push(ConstantTag::FieldRef.to_byte());
                bytes.extend_from_slice(&class_index.to_le_bytes());
                bytes.extend_from_slice(&name_and_type_index.to_le_bytes());
            }
            Constant::MethodRef {
                class_index,
                name_and_type_index,
            } => {
                bytes.push(ConstantTag::MethodRef.to_byte());
                bytes.extend_from_slice(&class_index.to_le_bytes());
                bytes.extend_from_slice(&name_and_type_index.to_le_bytes());
            }
            Constant::InterfaceMethodRef {
                class_index,
                name_and_type_index,
            } => {
                bytes.push(ConstantTag::InterfaceMethodRef.to_byte());
                bytes.extend_from_slice(&class_index.to_le_bytes());
                bytes.extend_from_slice(&name_and_type_index.to_le_bytes());
            }
            Constant::NameAndType {
                name_index,
                descriptor_index,
            } => {
                bytes.push(ConstantTag::NameAndType.to_byte());
                bytes.extend_from_slice(&name_index.to_le_bytes());
                bytes.extend_from_slice(&descriptor_index.to_le_bytes());
            }
            Constant::MethodHandle {
                reference_kind,
                reference_index,
            } => {
                bytes.push(ConstantTag::MethodHandle.to_byte());
                bytes.push(*reference_kind);
                bytes.extend_from_slice(&reference_index.to_le_bytes());
            }
            Constant::MethodType { descriptor_index } => {
                bytes.push(ConstantTag::MethodType.to_byte());
                bytes.extend_from_slice(&descriptor_index.to_le_bytes());
            }
            Constant::InvokeDynamic {
                bootstrap_method_attr_index,
                name_and_type_index,
            } => {
                bytes.push(ConstantTag::InvokeDynamic.to_byte());
                bytes.extend_from_slice(&bootstrap_method_attr_index.to_le_bytes());
                bytes.extend_from_slice(&name_and_type_index.to_le_bytes());
            }
            Constant::Module { name_index } => {
                bytes.push(ConstantTag::Module.to_byte());
                bytes.extend_from_slice(&name_index.to_le_bytes());
            }
            Constant::Package { name_index } => {
                bytes.push(ConstantTag::Package.to_byte());
                bytes.extend_from_slice(&name_index.to_le_bytes());
            }
        }
    }

    /// 从字节反序列化常量池
    pub fn deserialize(bytes: &[u8], offset: &mut usize) -> Option<Self> {
        if *offset + 2 > bytes.len() {
            return None;
        }

        let count = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]) as usize;
        *offset += 2;

        let mut pool = Self::new();

        for _ in 1..count {
            if let Some(constant) = Self::deserialize_entry(bytes, offset) {
                pool.entries.push(constant);
            } else {
                return None;
            }
        }

        Some(pool)
    }

    /// 反序列化单个常量条目
    fn deserialize_entry(bytes: &[u8], offset: &mut usize) -> Option<Constant> {
        if *offset >= bytes.len() {
            return None;
        }

        let tag = ConstantTag::from_byte(bytes[*offset])?;
        *offset += 1;

        match tag {
            ConstantTag::Utf8 => {
                if *offset + 2 > bytes.len() {
                    return None;
                }
                let len = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]) as usize;
                *offset += 2;

                if *offset + len > bytes.len() {
                    return None;
                }

                let s = String::from_utf8(bytes[*offset..*offset + len].to_vec()).ok()?;
                *offset += len;
                Some(Constant::Utf8(s))
            }
            ConstantTag::Integer => {
                if *offset + 4 > bytes.len() {
                    return None;
                }
                let v = i32::from_le_bytes([
                    bytes[*offset],
                    bytes[*offset + 1],
                    bytes[*offset + 2],
                    bytes[*offset + 3],
                ]);
                *offset += 4;
                Some(Constant::Integer(v))
            }
            ConstantTag::Long => {
                if *offset + 8 > bytes.len() {
                    return None;
                }
                let v = i64::from_le_bytes([
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
                Some(Constant::Long(v))
            }
            ConstantTag::Float => {
                if *offset + 4 > bytes.len() {
                    return None;
                }
                let v = f32::from_le_bytes([
                    bytes[*offset],
                    bytes[*offset + 1],
                    bytes[*offset + 2],
                    bytes[*offset + 3],
                ]);
                *offset += 4;
                Some(Constant::Float(v))
            }
            ConstantTag::Double => {
                if *offset + 8 > bytes.len() {
                    return None;
                }
                let v = f64::from_le_bytes([
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
                Some(Constant::Double(v))
            }
            ConstantTag::String => {
                if *offset + 2 > bytes.len() {
                    return None;
                }
                let index = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
                *offset += 2;
                Some(Constant::String(index))
            }
            ConstantTag::Class => {
                if *offset + 2 > bytes.len() {
                    return None;
                }
                let name_index = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
                *offset += 2;
                Some(Constant::Class { name_index })
            }
            ConstantTag::FieldRef => {
                if *offset + 4 > bytes.len() {
                    return None;
                }
                let class_index = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
                let name_and_type_index =
                    u16::from_le_bytes([bytes[*offset + 2], bytes[*offset + 3]]);
                *offset += 4;
                Some(Constant::FieldRef {
                    class_index,
                    name_and_type_index,
                })
            }
            ConstantTag::MethodRef => {
                if *offset + 4 > bytes.len() {
                    return None;
                }
                let class_index = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
                let name_and_type_index =
                    u16::from_le_bytes([bytes[*offset + 2], bytes[*offset + 3]]);
                *offset += 4;
                Some(Constant::MethodRef {
                    class_index,
                    name_and_type_index,
                })
            }
            ConstantTag::InterfaceMethodRef => {
                if *offset + 4 > bytes.len() {
                    return None;
                }
                let class_index = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
                let name_and_type_index =
                    u16::from_le_bytes([bytes[*offset + 2], bytes[*offset + 3]]);
                *offset += 4;
                Some(Constant::InterfaceMethodRef {
                    class_index,
                    name_and_type_index,
                })
            }
            ConstantTag::NameAndType => {
                if *offset + 4 > bytes.len() {
                    return None;
                }
                let name_index = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
                let descriptor_index = u16::from_le_bytes([bytes[*offset + 2], bytes[*offset + 3]]);
                *offset += 4;
                Some(Constant::NameAndType {
                    name_index,
                    descriptor_index,
                })
            }
            ConstantTag::MethodHandle => {
                if *offset + 3 > bytes.len() {
                    return None;
                }
                let reference_kind = bytes[*offset];
                let reference_index = u16::from_le_bytes([bytes[*offset + 1], bytes[*offset + 2]]);
                *offset += 3;
                Some(Constant::MethodHandle {
                    reference_kind,
                    reference_index,
                })
            }
            ConstantTag::MethodType => {
                if *offset + 2 > bytes.len() {
                    return None;
                }
                let descriptor_index = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
                *offset += 2;
                Some(Constant::MethodType { descriptor_index })
            }
            ConstantTag::InvokeDynamic => {
                if *offset + 4 > bytes.len() {
                    return None;
                }
                let bootstrap_method_attr_index =
                    u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
                let name_and_type_index =
                    u16::from_le_bytes([bytes[*offset + 2], bytes[*offset + 3]]);
                *offset += 4;
                Some(Constant::InvokeDynamic {
                    bootstrap_method_attr_index,
                    name_and_type_index,
                })
            }
            ConstantTag::Module => {
                if *offset + 2 > bytes.len() {
                    return None;
                }
                let name_index = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
                *offset += 2;
                Some(Constant::Module { name_index })
            }
            ConstantTag::Package => {
                if *offset + 2 > bytes.len() {
                    return None;
                }
                let name_index = u16::from_le_bytes([bytes[*offset], bytes[*offset + 1]]);
                *offset += 2;
                Some(Constant::Package { name_index })
            }
            _ => None,
        }
    }
}

impl Default for ConstantPool {
    fn default() -> Self {
        Self::new()
    }
}
